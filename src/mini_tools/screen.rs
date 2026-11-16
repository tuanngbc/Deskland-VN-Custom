//! Physical-pixel selection across monitors, including negative origins.
use crate::desktop::wide;
use image::RgbaImage;
use std::cell::RefCell;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone,Copy,Default,Debug,PartialEq,Eq)]
pub struct Area {pub x:i32,pub y:i32,pub w:i32,pub h:i32}
impl Area {
    pub fn between(a:(i32,i32),b:(i32,i32))->Self {Self{x:a.0.min(b.0),y:a.1.min(b.1),w:(a.0-b.0).abs(),h:(a.1-b.1).abs()}}
    pub fn intersect(self,b:Self)->Self {let x=self.x.max(b.x);let y=self.y.max(b.y);Self{x,y,w:(self.x+self.w).min(b.x+b.w).saturating_sub(x).max(0),h:(self.y+self.h).min(b.y+b.h).saturating_sub(y).max(0)}}
    pub fn rect(self)->RECT {RECT{left:self.x,top:self.y,right:self.x+self.w,bottom:self.y+self.h}}
}
pub fn virtual_area()->Area {unsafe{Area{x:GetSystemMetrics(SM_XVIRTUALSCREEN),y:GetSystemMetrics(SM_YVIRTUALSCREEN),w:GetSystemMetrics(SM_CXVIRTUALSCREEN),h:GetSystemMetrics(SM_CYVIRTUALSCREEN)}}}
pub fn monitor_at(x:i32,y:i32)->Area {unsafe{let h=MonitorFromPoint(POINT{x,y},MONITOR_DEFAULTTONEAREST);let mut m:MONITORINFO=std::mem::zeroed();m.cbSize=std::mem::size_of::<MONITORINFO>() as u32;GetMonitorInfoW(h,&mut m);Area{x:m.rcMonitor.left,y:m.rcMonitor.top,w:m.rcMonitor.right-m.rcMonitor.left,h:m.rcMonitor.bottom-m.rcMonitor.top}}}

pub struct Grab {dc:HDC,bitmap:HBITMAP,old:HGDIOBJ,bits:*mut u8,pub area:Area}
impl Grab {
    pub fn new(area:Area)->Result<Self,String>{unsafe{
        if area.w<=0||area.h<=0||area.w as i64*area.h as i64>100_000_000{return Err("Vùng chụp không hợp lệ hoặc quá lớn".into());}
        let screen=GetDC(std::ptr::null_mut());let dc=CreateCompatibleDC(screen);let mut bits=std::ptr::null_mut();
        let bitmap=CreateDIBSection(screen,&info(area.w,area.h),DIB_RGB_COLORS,&mut bits,std::ptr::null_mut(),0);ReleaseDC(std::ptr::null_mut(),screen);
        if dc.is_null()||bitmap.is_null()||bits.is_null(){if !dc.is_null(){DeleteDC(dc);}if !bitmap.is_null(){DeleteObject(bitmap);}return Err("Không tạo được bộ đệm chụp màn hình".into());}
        let old=SelectObject(dc,bitmap);Ok(Self{dc,bitmap,old,bits:bits as *mut u8,area})
    }}
    pub fn update(&mut self,cursor:bool)->Result<&[u8],String>{unsafe{
        let dc=GetDC(std::ptr::null_mut());let ok=BitBlt(self.dc,0,0,self.area.w,self.area.h,dc,self.area.x,self.area.y,SRCCOPY|CAPTUREBLT);ReleaseDC(std::ptr::null_mut(),dc);
        if ok==0{return Err("Không chụp được màn hình".into());}
        if cursor {let mut c:CURSORINFO=std::mem::zeroed();c.cbSize=std::mem::size_of::<CURSORINFO>() as u32;if GetCursorInfo(&mut c)!=0&&c.flags==CURSOR_SHOWING{let mut i:ICONINFO=std::mem::zeroed();if GetIconInfo(c.hCursor,&mut i)!=0{DrawIconEx(self.dc,c.ptScreenPos.x-self.area.x-i.xHotspot as i32,c.ptScreenPos.y-self.area.y-i.yHotspot as i32,c.hCursor,0,0,0,std::ptr::null_mut(),DI_NORMAL);if !i.hbmMask.is_null(){DeleteObject(i.hbmMask);}if !i.hbmColor.is_null(){DeleteObject(i.hbmColor);}}}}
        GdiFlush();Ok(std::slice::from_raw_parts(self.bits,(self.area.w*self.area.h*4) as usize))
    }}
    pub fn image(&mut self)->Result<RgbaImage,String>{let(w,h)=(self.area.w as u32,self.area.h as u32);let mut bytes=self.update(false)?.to_vec();for p in bytes.chunks_exact_mut(4){p.swap(0,2);p[3]=255;}RgbaImage::from_raw(w,h,bytes).ok_or_else(||"Lỗi dữ liệu ảnh chụp".into())}
}
impl Drop for Grab {fn drop(&mut self){unsafe{SelectObject(self.dc,self.old);DeleteObject(self.bitmap);DeleteDC(self.dc);}}}
pub fn info(w:i32,h:i32)->BITMAPINFO {unsafe{let mut i:BITMAPINFO=std::mem::zeroed();i.bmiHeader.biSize=std::mem::size_of::<BITMAPINFOHEADER>() as u32;i.bmiHeader.biWidth=w;i.bmiHeader.biHeight=-h;i.bmiHeader.biPlanes=1;i.bmiHeader.biBitCount=32;i}}
pub fn crop(image:&RgbaImage,area:Area)->RgbaImage {image::imageops::crop_imm(image,area.x.max(0) as u32,area.y.max(0) as u32,area.w as u32,area.h as u32).to_image()}
pub unsafe fn draw_image(dc:HDC,image:&RgbaImage,rect:Area){let mut bytes=image.as_raw().clone();for p in bytes.chunks_exact_mut(4){p.swap(0,2);}SetStretchBltMode(dc,HALFTONE);StretchDIBits(dc,rect.x,rect.y,rect.w,rect.h,0,0,image.width() as i32,image.height() as i32,bytes.as_ptr() as *const _,&info(image.width() as i32,image.height() as i32),DIB_RGB_COLORS,SRCCOPY);}

struct Selection {image:RgbaImage,origin:Area,start:Option<(i32,i32)>,area:Area,result:Option<Area>,record:bool}
thread_local!{static PICK:RefCell<Option<Selection>>=const{RefCell::new(None)};}
pub fn select(record:bool)->Result<Option<(RgbaImage,Area)>,String>{
    let origin=virtual_area();let image=Grab::new(origin)?.image()?;
    PICK.with(|c|*c.borrow_mut()=Some(Selection{image,origin,start:None,area:Area::default(),result:None,record}));
    unsafe{
        let cls=wide("DesklandCaptureSelection");let mut wc:WNDCLASSW=std::mem::zeroed();wc.lpfnWndProc=Some(proc);wc.lpszClassName=cls.as_ptr();wc.hCursor=LoadCursorW(std::ptr::null_mut(),IDC_CROSS);RegisterClassW(&wc);
        let hwnd=CreateWindowExW(WS_EX_TOPMOST|WS_EX_TOOLWINDOW,cls.as_ptr(),wide(if record{"Deskland — Chọn vùng quay"}else{"Deskland — Chọn vùng chụp"}).as_ptr(),WS_POPUP,origin.x,origin.y,origin.w,origin.h,std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null());
        if hwnd.is_null(){PICK.with(|c|c.borrow_mut().take());return Err("Không mở được khung chọn vùng".into());}
        ShowWindow(hwnd,SW_SHOW);SetForegroundWindow(hwnd);super::loop_messages();
    }
    Ok(PICK.with(|c|c.borrow_mut().take()).and_then(|s|s.result.map(|mut r|{if record{r.x+=s.origin.x;r.y+=s.origin.y;}(s.image,r)})))
}
unsafe fn finish(hwnd:HWND,area:Area){
    let valid=PICK.with(|c|{let mut g=c.borrow_mut();let s=g.as_mut().unwrap();let a=area.intersect(Area{x:0,y:0,w:s.origin.w,h:s.origin.h});if a.w<4||a.h<4{return false;}s.result=Some(a);true});
    if valid{DestroyWindow(hwnd);}
}
unsafe extern "system" fn proc(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{match msg{
    WM_ERASEBKGND=>1,
    WM_PAINT=>{let mut ps=std::mem::zeroed();let dc=BeginPaint(hwnd,&mut ps);PICK.with(|c|{let g=c.borrow();let Some(s)=g.as_ref()else{return;};draw_image(dc,&s.image,Area{x:0,y:0,w:s.origin.w,h:s.origin.h});
        let pen=CreatePen(PS_SOLID,2,0x00F4CF36);let old=SelectObject(dc,pen);let brush=SelectObject(dc,GetStockObject(NULL_BRUSH));let r=s.area;Rectangle(dc,r.x,r.y,r.x+r.w,r.y+r.h);SelectObject(dc,brush);SelectObject(dc,old);DeleteObject(pen);
        let mut p=POINT{x:0,y:0};GetCursorPos(&mut p);let m=monitor_at(p.x,p.y);let box_area=Area{x:m.x-s.origin.x+20,y:m.y-s.origin.y+20,w:650.min(m.w-40),h:68};let bg=CreateSolidBrush(0x0025201B);FillRect(dc,&box_area.rect(),bg);DeleteObject(bg);let font=super::editor::make_font(17,500);let old_font=SelectObject(dc,font);SetBkMode(dc,TRANSPARENT as i32);SetTextColor(dc,0x00FFFFFF);
        let text=if s.record{"QUAY MÀN HÌNH  ·  Kéo chọn vùng; Space: cả màn hình; Esc: hủy"}else{"CHỤP MÀN HÌNH  ·  Kéo chọn vùng; Space: cả màn hình; Esc: hủy"};let mut r=box_area.rect();r.left+=15;r.top+=10;DrawTextW(dc,wide(text).as_ptr(),-1,&mut r,DT_LEFT|DT_WORDBREAK);
        if s.area.w>0{let t=wide(&format!("{} × {}",s.area.w,s.area.h));TextOutW(dc,s.area.x+5,(s.area.y-24).max(box_area.y+box_area.h+5),t.as_ptr(),t.len() as i32-1);}
        SelectObject(dc,old_font);DeleteObject(font);
    });EndPaint(hwnd,&ps);0},
    WM_LBUTTONDOWN=>{let p=((l as i16) as i32,((l>>16) as i16) as i32);PICK.with(|c|if let Some(s)=c.borrow_mut().as_mut(){s.start=Some(p);s.area=Area::default();});SetCapture(hwnd);0},
    WM_MOUSEMOVE=>{let p=((l as i16) as i32,((l>>16) as i16) as i32);let changed=PICK.with(|c|{let mut g=c.borrow_mut();let s=g.as_mut().unwrap();if let Some(a)=s.start{s.area=Area::between(a,p);true}else{false}});if changed{InvalidateRect(hwnd,std::ptr::null(),0);}0},
    WM_LBUTTONUP=>{ReleaseCapture();let area=PICK.with(|c|{let mut g=c.borrow_mut();let s=g.as_mut().unwrap();s.start=None;s.area});finish(hwnd,area);0},
    WM_KEYDOWN=>{if w==VK_ESCAPE as usize{DestroyWindow(hwnd);}else if w==VK_SPACE as usize{let mut p=POINT{x:0,y:0};GetCursorPos(&mut p);let mut a=monitor_at(p.x,p.y);PICK.with(|c|{let g=c.borrow();let s=g.as_ref().unwrap();a.x-=s.origin.x;a.y-=s.origin.y;});finish(hwnd,a);}0},
    WM_CLOSE=>{DestroyWindow(hwnd);0},WM_DESTROY=>{PostQuitMessage(0);0},_=>DefWindowProcW(hwnd,msg,w,l)
}}

#[cfg(test)]mod tests{use super::*;#[test]fn negative_origin_selection_and_crop_are_exact(){let r=Area::between((-1070,-700),(-900,-500));assert_eq!(r,Area{x:-1070,y:-700,w:170,h:200});assert_eq!(r.intersect(Area{x:-1000,y:-600,w:500,h:500}),Area{x:-1000,y:-600,w:100,h:100});let i=RgbaImage::from_fn(10,8,|x,y|image::Rgba([x as u8,y as u8,0,255]));let c=crop(&i,Area{x:2,y:3,w:4,h:2});assert_eq!(c.dimensions(),(4,2));assert_eq!(c.get_pixel(0,0).0,[2,3,0,255]);}}
