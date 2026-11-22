//! Small native rendering proof: live DWM backdrop versus a per-pixel
//! premultiplied surface. No production overlay setting selects this yet.
use std::{mem, ptr};
use windows_sys::core::HRESULT;
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    System::LibraryLoader::GetModuleHandleW,
    UI::{Controls::MARGINS, WindowsAndMessaging::*},
};

const CLASS: &str = "TuanpcDeskGlassTrial";
const ROLE_ACRYLIC: usize = 1;
const ROLE_ALPHA: usize = 2;
const ROLE_REFERENCE: usize = 3;
const SAMPLE_TIMER: usize = 2;

unsafe fn dwm_value<T>(hwnd: HWND, attribute: DWMWINDOWATTRIBUTE, value: &T) -> HRESULT {
    DwmSetWindowAttribute(hwnd, attribute as u32, value as *const T as _, mem::size_of::<T>() as u32)
}

/// HRESULT describes API availability only. It does not certify that DWM
/// painted acrylic (OS transparency/accessibility policies can substitute it).
unsafe fn request_acrylic(hwnd: HWND) -> (HRESULT, HRESULT) {
    let margins = MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
    let _ = dwm_value(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &DWMWCP_ROUND);
    let _ = dwm_value(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &1i32);
    (DwmExtendFrameIntoClientArea(hwnd, &margins),
        dwm_value(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &DWMSBT_TRANSIENTWINDOW))
}

fn distance(x: f32, y: f32, left: f32, top: f32, width: f32, height: f32, radius: f32) -> f32 {
    let qx = (x-left-width*0.5).abs()-(width*0.5-radius);
    let qy = (y-top-height*0.5).abs()-(height*0.5-radius);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
}

fn glass_pixel(x: f32, y: f32, width: f32, height: f32, density: f32) -> [u8;4] {
    let inset=8.0*density;
    let h=height-2.0*inset;
    let d=distance(x,y,inset,inset,width-2.0*inset,h,h*0.5);
    let cover=(0.5-d).clamp(0.0,1.0);
    let shadow=0.17*(1.0-(d/(7.0*density)).max(0.0)).clamp(0.0,1.0).powi(2)*(1.0-cover);
    let edge=(0.85+d/density).clamp(0.0,1.0);
    let shine=(1.0-(y-inset).clamp(0.0,h)/h).powi(4);
    let alpha=cover*(0.57+0.13*edge)+shadow;
    let tone=cover*(26.0+28.0*shine+32.0*edge);
    // Premultiplied BGRA. Shadow contributes alpha with zero color.
    [(tone*1.18*alpha).round() as u8,(tone*1.07*alpha).round() as u8,(tone*alpha).round() as u8,(alpha*255.0).round() as u8]
}

pub(super) struct Layer {
    dc: HDC, bitmap: HBITMAP, previous: HGDIOBJ, bits: *mut u8, pub(super) size: (i32,i32),
}
impl Layer {
    pub(super) unsafe fn new(width:i32,height:i32) -> Option<Self> {
        if width<1||height<1||width as i64*height as i64>2_000_000 {return None;}
        let dc=CreateCompatibleDC(ptr::null_mut());
        if dc.is_null() {return None;}
        let mut info:BITMAPINFO=mem::zeroed();
        info.bmiHeader=BITMAPINFOHEADER {biSize:mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth:width,biHeight:-height,biPlanes:1,biBitCount:32,biCompression:BI_RGB,..mem::zeroed()};
        let mut bits=ptr::null_mut();
        let bitmap=CreateDIBSection(dc,&info,DIB_RGB_COLORS,&mut bits,ptr::null_mut(),0);
        if bitmap.is_null() || bits.is_null() {
            if !bitmap.is_null() {DeleteObject(bitmap);}
            DeleteDC(dc);return None;
        }
        let previous=SelectObject(dc,bitmap);
        if previous.is_null() || previous as isize == -1 {
            DeleteObject(bitmap);DeleteDC(dc);return None;
        }
        Some(Self{dc,bitmap,previous,bits:bits.cast(),size:(width,height)})
    }
    pub(super) fn pixels(&mut self) -> &mut [u8] {
        unsafe {std::slice::from_raw_parts_mut(self.bits,self.size.0 as usize*self.size.1 as usize*4)}
    }
    pub(super) fn dc(&self)->HDC{self.dc}
    pub(super) unsafe fn present(&self, hwnd:HWND) -> bool {
        let size=SIZE{cx:self.size.0,cy:self.size.1};
        let origin=POINT{x:0,y:0};
        let blend=BLENDFUNCTION{BlendOp:AC_SRC_OVER as u8,BlendFlags:0,SourceConstantAlpha:255,AlphaFormat:AC_SRC_ALPHA as u8};
        UpdateLayeredWindow(hwnd,ptr::null_mut(),ptr::null(),&size,self.dc,&origin,0,&blend,ULW_ALPHA)!=0
    }
    pub(super) unsafe fn blit(&self, target:HDC)->bool {
        BitBlt(target,0,0,self.size.0,self.size.1,self.dc,0,0,SRCCOPY)!=0
    }
}
impl Drop for Layer {
    fn drop(&mut self) {unsafe {SelectObject(self.dc,self.previous);DeleteObject(self.bitmap);DeleteDC(self.dc);}}
}

unsafe extern "system" fn proc(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE=>MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND=>1,
        WM_PAINT=>{
            let mut ps: PAINTSTRUCT=mem::zeroed();
            let dc=BeginPaint(hwnd,&mut ps);
            let role=GetWindowLongPtrW(hwnd,GWLP_USERDATA) as usize;
            if role==ROLE_ACRYLIC {
                let mut rect:RECT=mem::zeroed();
                GetClientRect(hwnd,&mut rect);
                // Black in the extended glass client area leaves the backdrop visible.
                FillRect(dc,&rect,GetStockObject(BLACK_BRUSH) as HBRUSH);
            } else if role==ROLE_REFERENCE {
                let mut client:RECT=mem::zeroed();
                GetClientRect(hwnd,&mut client);
                // Sharp stripes give an unambiguous blur reference. No
                // screenshots of the user's real desktop are sampled here.
                let colors=[0x005f2e20,0x00492aaf,0x002197d8,0x009cc278,0x00e4f8f8];
                let stripe=14;
                for x in (0..client.right).step_by(stripe) {
                    let brush=CreateSolidBrush(colors[(x as usize/stripe)%colors.len()]);
                    FillRect(dc,&RECT{left:x,top:0,right:x+stripe as i32,bottom:client.bottom},brush);
                    DeleteObject(brush);
                }
            }
            EndPaint(hwnd,&ps);
            0
        }
        WM_TIMER if w==1=>{PostQuitMessage(0);0},
        WM_CLOSE=>{PostQuitMessage(0);0},
        _=>DefWindowProcW(hwnd,msg,w,l),
    }
}

struct Window(HWND);
impl Drop for Window {fn drop(&mut self){unsafe{DestroyWindow(self.0);}}}

/// A sharp edge changes much more over one pixel than a broad blur does.
/// Measure both edge concentration and spatial variation to separate blur
/// from a flat, opaque fallback. Only aggregate values leave the probe.
fn edge_profile(samples:&[u8])->(u8,u8){
    let spread=samples.iter().max().zip(samples.iter().min()).map_or(0,|(&a,&b)|a-b);
    let edge=samples.windows(2).map(|pair|pair[0].abs_diff(pair[1])).max().unwrap_or(0);
    (spread,edge)
}

#[test]
fn stripe_measurement_separates_blur_from_flat_and_translucent(){
    assert_eq!(edge_profile(&[10,10,200,200]),(190,190));
    assert_eq!(edge_profile(&[80,80,120,120]),(40,40));
    assert_eq!(edge_profile(&[80,90,100,110,120]),(40,10));
    assert_eq!(edge_profile(&[82,82,82,82]),(0,0));
}

unsafe fn fixture_row(window:HWND,x:i32,y:i32,width:i32)->Option<(u8,u8)>{
    // Reject an obstructed reference or overlay. Never sample elsewhere,
    // retain the scanline, or emit a desktop image from this diagnostic.
    if (x..x+width).any(|col|WindowFromPoint(POINT{x:col,y})!=window){return None;}
    let mut row=Layer::new(width,1)?;
    let dc=GetDC(ptr::null_mut());
    if dc.is_null(){return None;}
    let ok=BitBlt(row.dc(),0,0,width,1,dc,x,y,SRCCOPY|CAPTUREBLT)!=0;
    ReleaseDC(ptr::null_mut(),dc);
    if !ok{return None;}
    GdiFlush();
    let gray=row.pixels().chunks_exact(4).map(|p|((29*p[0] as u32+150*p[1] as u32+77*p[2] as u32)/256) as u8).collect::<Vec<_>>();
    Some(edge_profile(&gray))
}

/// Called only by explicit trial dispatch. Profiles and the Explorer UI are
/// unchanged. Uses two tiny surfaces; timeout guarantees clean normal exit.
pub fn probe(dir:&std::path::Path,duration:u32) -> Result<(),String> {
    use crate::desktop::wide;
    let screen=super::displays().into_iter().find(|d|d.primary).ok_or("No drawable primary display")?;
    let scale=screen.dpi as f32/96.0;
    let width=(340.0*scale).round() as i32;
    let height=(68.0*scale).round() as i32;
    let x=screen.work.left+22;
    let y=screen.work.bottom-2*height-32;
    unsafe {
        let instance=GetModuleHandleW(ptr::null());
        let class=wide(CLASS);
        let mut wc:WNDCLASSEXW=mem::zeroed();
        wc.cbSize=mem::size_of::<WNDCLASSEXW>() as u32;
        wc.hInstance=instance;wc.lpfnWndProc=Some(proc);wc.lpszClassName=class.as_ptr();
        if RegisterClassExW(&wc)==0 {return Err(format!("Glass trial class: {}",GetLastError()));}
        let build=|role:usize,label:&str,top:i32| {
            let hwnd=CreateWindowExW(WS_EX_TOPMOST|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|if role==ROLE_ALPHA{WS_EX_LAYERED}else{0},
                class.as_ptr(),wide(label).as_ptr(),WS_POPUP,x,top,width,height,
                ptr::null_mut(),ptr::null_mut(),instance,ptr::null());
            if hwnd.is_null(){return Err(format!("Glass trial window: {}",GetLastError()));}
            SetWindowLongPtrW(hwnd,GWLP_USERDATA,role as isize);
            Ok(Window(hwnd))
        };
        let reference=CreateWindowExW(WS_EX_TOPMOST|WS_EX_NOACTIVATE,class.as_ptr(),
            wide("tuanpc-desk - Glass rendering trial").as_ptr(),WS_CAPTION|WS_SYSMENU,
            x-14,y-58,width+28,2*height+110,ptr::null_mut(),ptr::null_mut(),instance,ptr::null());
        if reference.is_null(){return Err(format!("Glass trial reference: {}",GetLastError()));}
        let reference=Window(reference);
        SetWindowLongPtrW(reference.0,GWLP_USERDATA,ROLE_REFERENCE as isize);
        let acrylic=build(ROLE_ACRYLIC,"Deskland glass probe: Windows acrylic",y)?;
        let alpha=build(ROLE_ALPHA,"Deskland glass probe: per-pixel alpha",y+height+10)?;
        let mut layer=Layer::new(width,height).ok_or("Glass trial buffer allocation")?;
        for (i,pixel) in layer.pixels().chunks_exact_mut(4).enumerate(){
            pixel.copy_from_slice(&glass_pixel((i%width as usize) as f32+0.5,(i/width as usize) as f32+0.5,width as f32,height as f32,scale));
        }
        let (extend,backdrop)=request_acrylic(acrylic.0);
        let layered=layer.present(alpha.0);
        let mut stat=serde_json::json!({"dpi":screen.dpi,"extent":[width,height],"extendHresult":extend,"backdropHresult":backdrop,
            "alphaPresent":layered,"acrylic":[x,y],"layered":[x,y+height+10],"verifiedBackdropVisually":false});
        ShowWindow(reference.0,SW_SHOWNOACTIVATE);
        ShowWindow(acrylic.0,SW_SHOWNOACTIVATE);
        ShowWindow(alpha.0,SW_SHOWNOACTIVATE);
        if SetTimer(alpha.0,1,duration.clamp(5,150)*1000,None)==0 {return Err("Glass trial timeout not installed".into());}
        SetTimer(alpha.0,SAMPLE_TIMER,900,None);
        let mut msg:MSG=mem::zeroed();
        while GetMessageW(&mut msg,ptr::null_mut(),0,0)>0 {
            if msg.hwnd==alpha.0&&msg.message==WM_TIMER&&msg.wParam==SAMPLE_TIMER {
                KillTimer(alpha.0,SAMPLE_TIMER);
                let left=x+width/5;
                let count=width*3/5;
                let reference=fixture_row(reference.0,left,y+height+5,count);
                let layered=fixture_row(alpha.0,left,y+height+10+height/2,count);
                let acrylic=fixture_row(acrylic.0,left,y+height/2,count);
                stat["pixelProbe"]=serde_json::json!({
                    "reference":reference,"alpha":layered,"acrylic":acrylic,
                    "format":"[luma spread, strongest adjacent edge]", "samples":count,
                    "allRowsVisible":reference.is_some()&&layered.is_some()&&acrylic.is_some()
                });
                std::fs::write(dir.join("dock-glass-probe.json"),stat.to_string()).map_err(|e|e.to_string())?;
            }else{
                TranslateMessage(&msg);DispatchMessageW(&msg);
            }
        }
        KillTimer(alpha.0,1);
        KillTimer(alpha.0,SAMPLE_TIMER);
        std::fs::write(dir.join("dock-glass-probe.json"),stat.to_string()).map_err(|e|e.to_string())?;
        if !layered {return Err("Layered presentation failed".into());}
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capsule_edges_are_antialiased_and_pixels_are_premultiplied() {
        for dpi in [1.0,1.25,1.5,2.0] {
            let (w,h)=(340.0*dpi,68.0*dpi);
            assert_eq!(glass_pixel(0.5,0.5,w,h,dpi),[0;4]);
            assert!(glass_pixel(w/2.0,h/2.0,w,h,dpi)[3]>120);
            assert!((0..h as i32).any(|y|{let p=glass_pixel(w/2.0,y as f32+0.5,w,h,dpi); p[3]>0&&p[3]<36}));
            assert!((0..h as i32).all(|y|{let p=glass_pixel(w/3.0,y as f32+0.5,w,h,dpi);p[..3].iter().all(|&c|c<=p[3])}));
        }
    }
}
