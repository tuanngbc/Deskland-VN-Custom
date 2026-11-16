//! Native, non-destructive screenshot editor. History stores drawing commands,
//! not a full-screen bitmap for every mouse movement or undo step.
use super::screen::{self,Area};
use crate::desktop::wide;
use image::RgbaImage;
use std::cell::RefCell;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::UI::Controls::Dialogs::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone,Copy,Debug,PartialEq)]
enum Tool {Crop,Arrow,Box,Pen,Text,Step,Pixel,Cover,Pan}
#[derive(Clone,Debug)]
struct Mark {tool:Tool,points:Vec<(f32,f32)>,text:String,color:[u8;3]}
#[derive(Clone)] enum Edit {Mark(Mark),Crop(Area)}
pub struct Document {base:RgbaImage,edits:Vec<Edit>,undone:Vec<Edit>}
impl Document {
    pub fn new(base:RgbaImage)->Self{Self{base,edits:Vec::new(),undone:Vec::new()}}
    fn area(&self)->Area{self.edits.iter().rev().find_map(|e|if let Edit::Crop(a)=e{Some(*a)}else{None}).unwrap_or(Area{x:0,y:0,w:self.base.width() as i32,h:self.base.height() as i32})}
    fn push(&mut self,e:Edit){self.edits.push(e);self.undone.clear();}
    fn undo(&mut self){if let Some(e)=self.edits.pop(){self.undone.push(e);}}
    fn redo(&mut self){if let Some(e)=self.undone.pop(){self.edits.push(e);}}
    fn render(&self,pending:Option<&Mark>)->RgbaImage{
        let a=self.area();let image=screen::crop(&self.base,a);
        let Some(mut pix)=tiny_skia::Pixmap::from_vec(image.into_raw(),tiny_skia::IntSize::from_wh(a.w as u32,a.h as u32).unwrap())else{return screen::crop(&self.base,a);};
        let marks:Vec<_>=self.edits.iter().filter_map(|e|if let Edit::Mark(m)=e{Some(m)}else{None}).chain(pending).collect();
        for m in &marks {
            if m.tool==Tool::Text||m.tool==Tool::Step {
                if let Some(&(x,y))=m.points.first(){
                    let mut image=RgbaImage::from_raw(a.w as u32,a.h as u32,pix.take()).unwrap();
                    stamp_text(&mut image,&m.text,x as i32-a.x,y as i32-a.y,if m.tool==Tool::Step{30}else{26},m.color);
                    pix=tiny_skia::Pixmap::from_vec(image.into_raw(),tiny_skia::IntSize::from_wh(a.w as u32,a.h as u32).unwrap()).unwrap();
                }
            } else {draw_mark(&mut pix,m,a);}
        }
        RgbaImage::from_raw(a.w as u32,a.h as u32,pix.take()).unwrap()
    }
}
fn draw_mark(p:&mut tiny_skia::Pixmap,m:&Mark,a:Area){
    let Some(&(x,y))=m.points.first()else{return;};let(x,y)=(x-a.x as f32,y-a.y as f32);let &(ex,ey)=m.points.last().unwrap();let(ex,ey)=(ex-a.x as f32,ey-a.y as f32);
    if m.tool==Tool::Pixel {
        let r=Area::between((x as i32,y as i32),(ex as i32,ey as i32)).intersect(Area{x:0,y:0,w:p.width() as i32,h:p.height() as i32});let width=p.width() as usize;let data=p.data_mut();
        for yy in (r.y..r.y+r.h).step_by(14){for xx in(r.x..r.x+r.w).step_by(14){let mut sum=[0u64;3];let mut n=0u64;for by in yy..(yy+14).min(r.y+r.h){for bx in xx..(xx+14).min(r.x+r.w){let i=(by as usize*width+bx as usize)*4;for c in 0..3{sum[c]+=data[i+c] as u64;}n+=1;}}if n>0{for by in yy..(yy+14).min(r.y+r.h){for bx in xx..(xx+14).min(r.x+r.w){let i=(by as usize*width+bx as usize)*4;for c in 0..3{data[i+c]=(sum[c]/n)as u8;}}}}}}
        return;
    }
    let mut color=tiny_skia::Paint::default();color.anti_alias=true;color.set_color_rgba8(m.color[0],m.color[1],m.color[2],255);
    let mut path=tiny_skia::PathBuilder::new();
    match m.tool{
        Tool::Box|Tool::Cover=>{if let Some(r)=tiny_skia::Rect::from_xywh(x.min(ex),y.min(ey),(ex-x).abs().max(1.0),(ey-y).abs().max(1.0)){path.push_rect(r);}},
        Tool::Arrow=>{path.move_to(x,y);path.line_to(ex,ey);let angle=(ey-y).atan2(ex-x);let len=20.0f32.min(((ex-x).powi(2)+(ey-y).powi(2)).sqrt()*0.45);for delta in [-0.5f32,0.5]{path.move_to(ex,ey);path.line_to(ex-len*(angle+delta).cos(),ey-len*(angle+delta).sin());}},
        Tool::Pen=>{path.move_to(x,y);for &(px,py) in m.points.iter().skip(1){path.line_to(px-a.x as f32,py-a.y as f32);}},
        _=>return,
    }
    if let Some(path)=path.finish(){if m.tool==Tool::Cover{p.fill_path(&path,&color,tiny_skia::FillRule::Winding,tiny_skia::Transform::identity(),None);}else{p.stroke_path(&path,&color,&tiny_skia::Stroke{width:3.5,line_cap:tiny_skia::LineCap::Round,line_join:tiny_skia::LineJoin::Round,..Default::default()},tiny_skia::Transform::identity(),None);}}
}
fn stamp_text(image:&mut RgbaImage,text:&str,x:i32,y:i32,size:i32,color:[u8;3]){unsafe{
    let dc=CreateCompatibleDC(std::ptr::null_mut());let mut bits=std::ptr::null_mut();let bitmap=CreateDIBSection(dc,&screen::info(image.width() as i32,image.height() as i32),DIB_RGB_COLORS,&mut bits,std::ptr::null_mut(),0);
    if dc.is_null()||bitmap.is_null()||bits.is_null(){if !dc.is_null(){DeleteDC(dc);}if !bitmap.is_null(){DeleteObject(bitmap);}return;}
    let old=SelectObject(dc,bitmap);let target=std::slice::from_raw_parts_mut(bits as *mut u8,image.as_raw().len());target.copy_from_slice(image.as_raw());for p in target.chunks_exact_mut(4){p.swap(0,2);}
    let font=make_font(size,600);let oldfont=SelectObject(dc,font);SetBkMode(dc,TRANSPARENT as i32);SetTextColor(dc,color[0]as u32|((color[1]as u32)<<8)|((color[2]as u32)<<16));let text=wide(text);TextOutW(dc,x,y,text.as_ptr(),text.len()as i32-1);GdiFlush();
    for p in target.chunks_exact_mut(4){p.swap(0,2);p[3]=255;}image.as_mut().copy_from_slice(target);SelectObject(dc,oldfont);DeleteObject(font);SelectObject(dc,old);DeleteObject(bitmap);DeleteDC(dc);
}}
pub unsafe fn make_font(size:i32,weight:i32)->HFONT {CreateFontW(-size,0,0,0,weight,0,0,0,DEFAULT_CHARSET as u32,OUT_DEFAULT_PRECIS as u32,CLIP_DEFAULT_PRECIS as u32,ANTIALIASED_QUALITY as u32,DEFAULT_PITCH as u32,wide("Segoe UI").as_ptr())}

struct State {doc:Document,tool:Tool,pending:Option<Mark>,drawing:bool,crop_start:Option<(i32,i32)>,crop_end:(i32,i32),cached:Option<RgbaImage>,zoom:f32,pan:(f32,f32),pan_start:Option<(i32,i32)>,color:usize,notice:String,readonly:bool,pinned:bool}
thread_local!{static EDITOR:RefCell<Option<State>>=const{RefCell::new(None)};}
const COLORS:[[u8;3];5]=[[241,69,75],[27,151,245],[255,199,45],[255,255,255],[0,0,0]];
const LABELS:[&str;16]=["✂","➤","□","✎","T","#","▦","▰","✋","●","↶","↷","⤢","⧉","▣","⚑"];
const TOOL_NAMES:[&str;9]=["Cắt ảnh","Mũi tên","Khung","Bút vẽ","Chữ","Số bước","Pixel","Tô kín","Di chuyển"];
fn toolbar(width:i32)->(i32,i32){let cols=((width-24)/60).clamp(1,16);(cols,((16+cols-1)/cols)*38+56)}
fn view(s:&State,width:i32,height:i32)->(f32,f32,f32){let(_,top)=toolbar(width);let a=s.doc.area();let scale=((width-32).max(1)as f32/a.w as f32).min((height-top-62).max(1)as f32/a.h as f32).min(1.0)*s.zoom;let x=(width as f32-a.w as f32*scale)/2.0+s.pan.0;let y=top as f32+((height-top-54)as f32-a.h as f32*scale)/2.0+s.pan.1;(x,y,scale)}
fn image_point(s:&State,x:i32,y:i32,w:i32,h:i32)->Option<(f32,f32)>{let(vx,vy,k)=view(s,w,h);let a=s.doc.area();let(px,py)=((x as f32-vx)/k+a.x as f32,(y as f32-vy)/k+a.y as f32);if px<a.x as f32||py<a.y as f32||px>=(a.x+a.w)as f32||py>=(a.y+a.h)as f32{None}else{Some((px,py))}}
unsafe fn size(hwnd:HWND)->(i32,i32){let mut r=RECT{left:0,top:0,right:0,bottom:0};GetClientRect(hwnd,&mut r);(r.right,r.bottom)}
pub fn run(doc:Document,readonly:bool)->Result<(),String>{
    EDITOR.with(|c|*c.borrow_mut()=Some(State{doc,tool:Tool::Arrow,pending:None,drawing:false,crop_start:None,crop_end:(0,0),cached:None,zoom:1.0,pan:(0.0,0.0),pan_start:None,color:0,notice:if readonly{"Xem trước · Esc đóng · con lăn phóng to".into()}else{"Enter: copy và đóng · Ctrl+C: copy · Ctrl+S: lưu · Ctrl+Z/Y: hoàn tác/làm lại".into()},readonly,pinned:false}));
    unsafe{let cls=wide("DesklandScreenshotEditor");let mut wc:WNDCLASSW=std::mem::zeroed();wc.lpfnWndProc=Some(proc);wc.lpszClassName=cls.as_ptr();wc.hCursor=LoadCursorW(std::ptr::null_mut(),IDC_ARROW);RegisterClassW(&wc);let mut mouse=POINT{x:0,y:0};GetCursorPos(&mut mouse);let m=screen::monitor_at(mouse.x,mouse.y);let w=1100.min(m.w-40).max(450);let h=800.min(m.h-100).max(360);let hwnd=CreateWindowExW(0,cls.as_ptr(),wide("Deskland — Chỉnh sửa ảnh chụp").as_ptr(),WS_OVERLAPPEDWINDOW,m.x+(m.w-w)/2,m.y+(m.h-h)/2,w,h,std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null());if hwnd.is_null(){EDITOR.with(|c|c.borrow_mut().take());return Err("Không mở được trình sửa ảnh".into());}ShowWindow(hwnd,SW_SHOW);SetForegroundWindow(hwnd);super::loop_messages();}
    EDITOR.with(|c|c.borrow_mut().take());Ok(())
}
fn exported()->Option<RgbaImage>{EDITOR.with(|c|c.borrow().as_ref().map(|s|s.doc.render(s.pending.as_ref())))}
unsafe fn clipboard(hwnd:HWND,image:&RgbaImage)->Result<(),String>{
    let header=screen::info(image.width()as i32,image.height()as i32).bmiHeader;let n=std::mem::size_of::<BITMAPINFOHEADER>()+image.as_raw().len();let mem=GlobalAlloc(GMEM_MOVEABLE,n);if mem.is_null(){return Err("Không đủ bộ nhớ để copy ảnh".into());}let ptr=GlobalLock(mem)as *mut u8;if ptr.is_null(){GlobalFree(mem);return Err("Không mở được bộ đệm ảnh".into());}std::ptr::copy_nonoverlapping(&header as *const _ as *const u8,ptr,std::mem::size_of::<BITMAPINFOHEADER>());let data=std::slice::from_raw_parts_mut(ptr.add(std::mem::size_of::<BITMAPINFOHEADER>()),image.as_raw().len());data.copy_from_slice(image.as_raw());for p in data.chunks_exact_mut(4){p.swap(0,2);p[3]=255;}GlobalUnlock(mem);
    if OpenClipboard(hwnd)==0{GlobalFree(mem);return Err("Clipboard đang bận, anh thử copy lại".into());}EmptyClipboard();let ok=SetClipboardData(8,mem);CloseClipboard();if ok.is_null(){GlobalFree(mem);Err("Không copy được ảnh".into())}else{Ok(())}
}
unsafe fn save(hwnd:HWND,image:&RgbaImage)->Result<Option<String>,String>{
    let mut path=[0u16;32768];let initial=super::output_path("Screenshot","png")?;let name=wide(&initial.to_string_lossy());let n=name.len().min(32768);path[..n].copy_from_slice(&name[..n]);let filter=wide("PNG (*.png)\0*.png\0JPEG (*.jpg)\0*.jpg\0\0");let mut o:OPENFILENAMEW=std::mem::zeroed();o.lStructSize=std::mem::size_of::<OPENFILENAMEW>()as u32;o.hwndOwner=hwnd;o.lpstrFile=path.as_mut_ptr();o.nMaxFile=path.len()as u32;o.lpstrFilter=filter.as_ptr();o.nFilterIndex=1;let ext=wide("png");o.lpstrDefExt=ext.as_ptr();o.Flags=OFN_OVERWRITEPROMPT|OFN_PATHMUSTEXIST|OFN_NOCHANGEDIR;
    o.Flags|=OFN_EXPLORER;if GetSaveFileNameW(&mut o)==0{return Ok(None);}let name=String::from_utf16_lossy(&path[..path.iter().position(|&c|c==0).unwrap_or(path.len())]);let p=std::path::PathBuf::from(name);let is_jpeg=p.extension().and_then(|v|v.to_str()).is_some_and(|v|v.eq_ignore_ascii_case("jpg")||v.eq_ignore_ascii_case("jpeg"));
    if is_jpeg{image::DynamicImage::ImageRgba8(image.clone()).to_rgb8().save_with_format(&p,image::ImageFormat::Jpeg).map_err(|e|e.to_string())?;}else{image.save_with_format(&p,image::ImageFormat::Png).map_err(|e|e.to_string())?;}Ok(Some(p.display().to_string()))
}
unsafe fn action(hwnd:HWND,index:usize){
    if index==13||index==14{if let Some(image)=exported(){let notice=if index==13{clipboard(hwnd,&image).map(|_|"Đã copy ảnh".into())}else{save(hwnd,&image).map(|v|v.map(|p|format!("Đã lưu: {p}")).unwrap_or_else(||"Đã hủy lưu ảnh".into()))};EDITOR.with(|c|if let Some(s)=c.borrow_mut().as_mut(){s.notice=notice.unwrap_or_else(|e|e);});}InvalidateRect(hwnd,std::ptr::null(),0);return;}
    let pin=EDITOR.with(|c|{let mut g=c.borrow_mut();let s=g.as_mut()?;
        if index!=4{if let Some(m)=s.pending.take(){if m.tool==Tool::Text&&!m.text.is_empty(){s.doc.push(Edit::Mark(m));}}}
        match index{0..=8=>{s.tool=[Tool::Crop,Tool::Arrow,Tool::Box,Tool::Pen,Tool::Text,Tool::Step,Tool::Pixel,Tool::Cover,Tool::Pan][index];},9=>s.color=(s.color+1)%COLORS.len(),10=>s.doc.undo(),11=>s.doc.redo(),12=>{s.zoom=1.0;s.pan=(0.0,0.0);},15=>{s.pinned=!s.pinned;return Some(s.pinned);},_=>{}}
        s.cached=None;None
    });if let Some(pin)=pin{SetWindowPos(hwnd,if pin{HWND_TOPMOST}else{HWND_NOTOPMOST},0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE);}InvalidateRect(hwnd,std::ptr::null(),0);
}
unsafe fn paint(hwnd:HWND){let mut ps=std::mem::zeroed();let target=BeginPaint(hwnd,&mut ps);let(w,h)=size(hwnd);if w<=0||h<=0{EndPaint(hwnd,&ps);return;}let dc=CreateCompatibleDC(target);let bitmap=CreateCompatibleBitmap(target,w,h);let old=SelectObject(dc,bitmap);let bg=CreateSolidBrush(0x00171717);FillRect(dc,&RECT{left:0,top:0,right:w,bottom:h},bg);DeleteObject(bg);let font=make_font(14,500);let oldfont=SelectObject(dc,font);SetBkMode(dc,TRANSPARENT as i32);SetTextColor(dc,0x00F5F5F1);
    EDITOR.with(|c|{let mut g=c.borrow_mut();let Some(s)=g.as_mut()else{return;};let(cols,top)=toolbar(w);let rule=CreateSolidBrush(0x00788B27);FillRect(dc,&RECT{left:0,top:top-5,right:w,bottom:top-3},rule);DeleteObject(rule);let(vx,vy,k)=view(s,w,h);let a=s.doc.area();if s.cached.is_none(){s.cached=Some(s.doc.render(s.pending.as_ref()));}if let Some(image)=&s.cached{let saved=SaveDC(dc);IntersectClipRect(dc,0,top,w,h-45);screen::draw_image(dc,image,Area{x:vx as i32,y:vy as i32,w:(a.w as f32*k)as i32,h:(a.h as f32*k)as i32});RestoreDC(dc,saved);}
        if let Some(start)=s.crop_start{let r=Area::between(start,s.crop_end);let p=CreatePen(PS_DASH,1,0x00FFFFFF);let op=SelectObject(dc,p);let ob=SelectObject(dc,GetStockObject(NULL_BRUSH));Rectangle(dc,(vx+(r.x-a.x)as f32*k)as i32,(vy+(r.y-a.y)as f32*k)as i32,(vx+(r.x+r.w-a.x)as f32*k)as i32,(vy+(r.y+r.h-a.y)as f32*k)as i32);SelectObject(dc,ob);SelectObject(dc,op);DeleteObject(p);}
        for(i,label)in LABELS.iter().enumerate(){let r=Area{x:12+(i as i32%cols)*60,y:12+(i as i32/cols)*38,w:54,h:32};let active=if i<9{[Tool::Crop,Tool::Arrow,Tool::Box,Tool::Pen,Tool::Text,Tool::Step,Tool::Pixel,Tool::Cover,Tool::Pan][i]==s.tool}else{i==15&&s.pinned};let b=CreateSolidBrush(if active{0x00788B27}else{0x002E2E30});let old_b=SelectObject(dc,b);RoundRect(dc,r.x,r.y,r.x+r.w,r.y+r.h,8,8);SelectObject(dc,old_b);DeleteObject(b);if i==9{let color=COLORS[s.color];SetTextColor(dc,color[0]as u32|((color[1]as u32)<<8)|((color[2]as u32)<<16));}else{SetTextColor(dc,0x00F8F7F5);}let mut r=r.rect();DrawTextW(dc,wide(label).as_ptr(),-1,&mut r,DT_CENTER|DT_VCENTER|DT_SINGLELINE);}
        let selected=match s.tool{Tool::Crop=>TOOL_NAMES[0],Tool::Arrow=>TOOL_NAMES[1],Tool::Box=>TOOL_NAMES[2],Tool::Pen=>TOOL_NAMES[3],Tool::Text=>TOOL_NAMES[4],Tool::Step=>TOOL_NAMES[5],Tool::Pixel=>TOOL_NAMES[6],Tool::Cover=>TOOL_NAMES[7],Tool::Pan=>TOOL_NAMES[8]};let mut opts=RECT{left:16,top:top-29,right:w-16,bottom:top-8};SetTextColor(dc,0x00919199);DrawTextW(dc,wide(&format!("{selected}   ·   Màu   ·   Cuộn để phóng to   ·   Enter để copy và đóng")).as_ptr(),-1,&mut opts,DT_LEFT|DT_VCENTER|DT_SINGLELINE|DT_END_ELLIPSIS);
        SetTextColor(dc,0x00919199);let text=format!("{} × {}  ·  {:.0}%   {}",a.w,a.h,k*100.0,s.notice);let mut r=RECT{left:16,top:h-38,right:w-12,bottom:h-5};DrawTextW(dc,wide(&text).as_ptr(),-1,&mut r,DT_LEFT|DT_VCENTER|DT_SINGLELINE|DT_END_ELLIPSIS);
    });BitBlt(target,0,0,w,h,dc,0,0,SRCCOPY);SelectObject(dc,oldfont);DeleteObject(font);SelectObject(dc,old);DeleteObject(bitmap);DeleteDC(dc);EndPaint(hwnd,&ps);
}
unsafe extern "system" fn proc(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{match msg{
    WM_ERASEBKGND=>1,WM_PAINT=>{paint(hwnd);0},WM_SIZE=>{InvalidateRect(hwnd,std::ptr::null(),0);0},
    WM_LBUTTONDOWN=>{let(x,y)=((l as i16)as i32,((l>>16)as i16)as i32);let(cw,ch)=size(hwnd);let(cols,_top)=toolbar(cw);let rows=(16+cols-1)/cols;let button_bottom=12+rows*38;if y<button_bottom{let i=((y-12)/38)*cols+(x-12)/60;if x>=12&&y>=12&&i>=0&&i<16{action(hwnd,i as usize);}return 0;}
        EDITOR.with(|c|{let mut g=c.borrow_mut();let s=g.as_mut().unwrap();if s.readonly&&s.tool!=Tool::Pan{return;}if s.tool==Tool::Pan{s.pan_start=Some((x,y));return;}let Some(p)=image_point(s,x,y,cw,ch)else{return;};if let Some(m)=s.pending.take(){if m.tool==Tool::Text&&!m.text.is_empty(){s.doc.push(Edit::Mark(m));}}
            match s.tool{Tool::Crop=>{s.crop_start=Some((p.0 as i32,p.1 as i32));s.crop_end=(p.0 as i32,p.1 as i32);},Tool::Step=>{let n=s.doc.edits.iter().filter(|e|matches!(e,Edit::Mark(m)if m.tool==Tool::Step)).count()+1;s.doc.push(Edit::Mark(Mark{tool:Tool::Step,points:vec![p],text:n.to_string(),color:COLORS[s.color]}));},_=>{s.pending=Some(Mark{tool:s.tool,points:vec![p,p],text:String::new(),color:COLORS[s.color]});s.drawing=s.tool!=Tool::Text;}}
            s.cached=None;
        });SetCapture(hwnd);InvalidateRect(hwnd,std::ptr::null(),0);0},
    WM_MOUSEMOVE=>{let(x,y)=((l as i16)as i32,((l>>16)as i16)as i32);let(cw,ch)=size(hwnd);let changed=EDITOR.with(|c|{let mut g=c.borrow_mut();let s=g.as_mut().unwrap();if let Some(last)=s.pan_start{s.pan.0+=(x-last.0)as f32;s.pan.1+=(y-last.1)as f32;s.pan_start=Some((x,y));return true;}let Some(p)=image_point(s,x,y,cw,ch)else{return false;};if s.crop_start.is_some(){s.crop_end=(p.0 as i32,p.1 as i32);return true;}if s.drawing{if let Some(m)=&mut s.pending{if m.tool==Tool::Pen{if m.points.len()<10000{m.points.push(p);}}else if let Some(end)=m.points.last_mut(){*end=p;}s.cached=None;return true;}}false});if changed{InvalidateRect(hwnd,std::ptr::null(),0);}0},
    WM_LBUTTONUP=>{ReleaseCapture();EDITOR.with(|c|{let mut g=c.borrow_mut();let s=g.as_mut().unwrap();s.pan_start=None;if let Some(start)=s.crop_start.take(){let a=Area::between(start,s.crop_end).intersect(s.doc.area());if a.w>=4&&a.h>=4{s.doc.push(Edit::Crop(a));s.zoom=1.0;s.pan=(0.0,0.0);}}if s.drawing{if let Some(m)=s.pending.take(){s.doc.push(Edit::Mark(m));}s.drawing=false;}s.cached=None;});InvalidateRect(hwnd,std::ptr::null(),0);0},
    WM_MOUSEWHEEL=>{EDITOR.with(|c|if let Some(s)=c.borrow_mut().as_mut(){s.zoom=(s.zoom*if(w>>16)as i16>0{1.2}else{1.0/1.2}).clamp(0.25,10.0);});InvalidateRect(hwnd,std::ptr::null(),0);0},
    WM_KEYDOWN=>{let ctrl=GetKeyState(VK_CONTROL as i32)<0;if ctrl{match w as u32{0x43=>action(hwnd,13),0x53=>action(hwnd,14),0x5a=>action(hwnd,10),0x59=>action(hwnd,11),_=>{}}}else if w==VK_ESCAPE as usize{DestroyWindow(hwnd);}else if w==VK_RETURN as usize{let editing=EDITOR.with(|c|{let mut g=c.borrow_mut();let s=g.as_mut().unwrap();if s.pending.as_ref().is_some_and(|m|m.tool==Tool::Text){if let Some(m)=s.pending.take(){if !m.text.is_empty(){s.doc.push(Edit::Mark(m));}}s.cached=None;true}else{false}});if !editing{if let Some(image)=exported(){match clipboard(hwnd,&image){Ok(())=>{DestroyWindow(hwnd);},Err(e)=>super::message(&e)}}}else{InvalidateRect(hwnd,std::ptr::null(),0);}}else if w==VK_BACK as usize{EDITOR.with(|c|if let Some(s)=c.borrow_mut().as_mut(){if let Some(m)=&mut s.pending{m.text.pop();s.cached=None;}});InvalidateRect(hwnd,std::ptr::null(),0);}0},
    WM_CHAR=>{if let Some(ch)=char::from_u32(w as u32).filter(|c|!c.is_control()){EDITOR.with(|c|if let Some(s)=c.borrow_mut().as_mut(){if let Some(m)=&mut s.pending{if m.tool==Tool::Text&&m.text.chars().count()<500{m.text.push(ch);s.cached=None;}}});InvalidateRect(hwnd,std::ptr::null(),0);}0},
    WM_CLOSE=>{DestroyWindow(hwnd);0},WM_DESTROY=>{PostQuitMessage(0);0},_=>DefWindowProcW(hwnd,msg,w,l)
}}

#[cfg(test)]mod tests{use super::*;#[test]fn crop_undo_redo_preserves_pixels_and_annotation_coordinates(){let base=RgbaImage::from_fn(100,80,|x,y|image::Rgba([x as u8,y as u8,0,255]));let mut d=Document::new(base.clone());d.push(Edit::Mark(Mark{tool:Tool::Cover,points:vec![(25.,25.),(45.,45.)],text:String::new(),color:[255,0,0]}));d.push(Edit::Crop(Area{x:20,y:20,w:40,h:30}));let r=d.render(None);assert_eq!(r.dimensions(),(40,30));assert_eq!(r.get_pixel(10,10).0,[255,0,0,255]);d.undo();assert_eq!(d.render(None).dimensions(),(100,80));d.undo();assert_eq!(d.render(None),base);d.redo();d.redo();assert_eq!(d.render(None),r);}}
