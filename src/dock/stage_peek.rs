//! Nonactivating Stage Peek backed by DWM or the existing Stage snapshot.
use super::{previews::Previews, stage_snapshot, WindowIdentity, STATE};
use crate::desktop::wide;
use std::{cell::RefCell, mem, ptr};
use windows_sys::Win32::{Foundation::*, Graphics::Gdi::*, System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::*};

pub(super) const TIMER: usize = 20;
thread_local! { static OPEN: RefCell<Option<Peek>> = const {RefCell::new(None)}; }

struct Peek {
    owner: HWND,
    source: WindowIdentity,
    window: HWND,
    live: Previews,
    saved: Option<stage_snapshot::Frame>,
}
impl Drop for Peek {
    fn drop(&mut self){
        self.live.clear();
        unsafe{ DestroyWindow(self.window); }
    }
}

unsafe extern "system" fn proc(h:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
    match msg{
        WM_MOUSEACTIVATE=>MA_NOACTIVATE as isize,
        WM_NCHITTEST=>HTTRANSPARENT as isize,
        WM_ERASEBKGND=>1,
        WM_PAINT=>{
            let mut paint:PAINTSTRUCT=mem::zeroed();
            let dc=BeginPaint(h,&mut paint);
            if !dc.is_null(){
                let mut area:RECT=mem::zeroed(); GetClientRect(h,&mut area);
                FillRect(dc,&area,GetStockObject(BLACK_BRUSH) as HBRUSH);
                OPEN.with(|slot| {
                    let Ok(slot)=slot.try_borrow()else{return;};
                    let Some((pixels,w,h))=slot.as_ref().filter(|p|p.window==h).and_then(|p|p.saved.as_ref())else{return;};
                    let mut info:BITMAPINFO=mem::zeroed();
                    info.bmiHeader=BITMAPINFOHEADER{biSize:mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth:*w,biHeight:-*h,biPlanes:1,biBitCount:32,biCompression:BI_RGB,..mem::zeroed()};
                    SetStretchBltMode(dc,HALFTONE);
                    StretchDIBits(dc,0,0,area.right,area.bottom,0,0,*w,*h,pixels.as_ptr().cast(),&info,DIB_RGB_COLORS,SRCCOPY);
                });
            }
            EndPaint(h,&paint);0
        }
        _=>DefWindowProcW(h,msg,w,l)
    }
}

pub(super) fn close(owner: HWND) {
    let old=OPEN.with(|slot|{
        let mut slot=slot.borrow_mut();
        if owner.is_null()||slot.as_ref().is_some_and(|p|p.owner==owner){slot.take()}else{None}
    });
    drop(old);
}

pub(super) fn validate(owner:HWND,active:Option<WindowIdentity>){
    if OPEN.with(|slot|slot.borrow().as_ref().is_some_and(|p|p.owner==owner&&(Some(p.source)!=active||!eligible(p.source)))){close(owner);}
}

fn eligible(source:WindowIdentity)->bool{
    unsafe{
        let hwnd=source.hwnd as HWND;
        let allowed=if source.pid==std::process::id(){super::previews::permitted_settings_pid(hwnd)}
            else{super::previews::permitted_pid(hwnd)};
        source.matches_stage()&&allowed==Ok(source.pid)
    }
}

fn fit_preview(w:i32,h:i32,max_w:i32,max_h:i32)->Option<(i32,i32)>{
    if w<1||h<1||max_w<1||max_h<1{return None;}
    // A cached Stage card is often much smaller than the hover panel. Keep
    // its source pixels rather than stretching them across the entire panel.
    let scale=(max_w as f32/w as f32).min(max_h as f32/h as f32).min(1.0);
    let (w,h)=((w as f32*scale).round() as i32,(h as f32*scale).round() as i32);
    (w>=64&&h>=48).then_some((w,h))
}

#[cfg(test)]
#[test]
fn peek_shrinks_to_fit_without_enlarging_cached_pixels(){
    assert_eq!(fit_preview(360,168,1000,600),Some((360,168)));
    assert_eq!(fit_preview(2106,974,1000,600),Some((1000,462)));
    assert_eq!(fit_preview(568,1024,512,768),Some((426,768)));
    assert_eq!(fit_preview(0,168,1000,600),None);
}

pub(super) unsafe fn show(owner: HWND) {
    KillTimer(owner,TIMER);
    close(ptr::null_mut());
    let candidate=STATE.with(|state|{
        let state=state.borrow();
        if !state.prefs.stage_peek{return None;}
        let stage=state.stages.iter().find(|p|p.hwnd==owner&&!p.hidden&&!p.collapsed)?;
        let item=stage.items.get(stage.hover?)?;
        let source=WindowIdentity::read_stage(item.hwnd)?;
        Some((source,stage.bounds,stage.layout?.right))
    });
    let Some((source,display,right))=candidate else{return;};
    show_at(owner,source,display,right);
}

unsafe fn show_at(owner:HWND,source:WindowIdentity,display:RECT,right:bool){
    let hwnd=source.hwnd as HWND;
    if !eligible(source){return;}
    let is_min=IsIconic(hwnd)!=0;
    let mut saved=if is_min{stage_snapshot::peek_frame(source)}else{None};
    let mut original:RECT=mem::zeroed();
    if !is_min && GetWindowRect(hwnd,&mut original)==0{return;}
    let geometry=|frame:Option<&stage_snapshot::Frame>|{
        frame.map(|(_,w,h)|(*w,*h)).unwrap_or((original.right-original.left,original.bottom-original.top))
    };
    let (w,h)=geometry(saved.as_ref());
    if w<1||h<1{return;}
    let mut stage:RECT=mem::zeroed();
    if GetWindowRect(owner,&mut stage)==0{return;}
    let monitor=MonitorFromWindow(owner,MONITOR_DEFAULTTONEAREST);
    let mut info:MONITORINFO=mem::zeroed();info.cbSize=mem::size_of::<MONITORINFO>() as u32;
    let work=if GetMonitorInfoW(monitor,&mut info)!=0{info.rcWork}else{display};
    let (left,right)=if right{(work.left+8,(stage.left-16).min(work.right-8))}else{((stage.right+16).max(work.left+8),work.right-8)};
    let max_w=(right-left).min((display.right-display.left)*3/5);
    let max_h=(work.bottom-work.top)*3/5;
    if max_w<160||max_h<100{return;}
    let Some((w,h))=fit_preview(w,h,max_w,max_h)else{return;};
    let xy=|w,h|(left+(right-left-w)/2,work.top+(work.bottom-work.top-h)/2);
    let (x,y)=xy(w,h);
    let class=wide("TLauncher.StagePeek");
    let inst=GetModuleHandleW(ptr::null());
    let wc=WNDCLASSW{style:CS_DROPSHADOW,lpfnWndProc:Some(proc),hInstance:inst,lpszClassName:class.as_ptr(),..mem::zeroed()};
    if RegisterClassW(&wc)==0&&GetLastError()!=ERROR_CLASS_ALREADY_EXISTS{return;}
    let window=CreateWindowExW(WS_EX_NOACTIVATE|WS_EX_TOPMOST|WS_EX_TOOLWINDOW|WS_EX_TRANSPARENT,
        class.as_ptr(),ptr::null(),WS_POPUP,x,y,
        w,h,ptr::null_mut(),ptr::null_mut(),inst,ptr::null_mut());
    if window.is_null(){return;}
    let mut live=Previews::default();
    if !is_min&&!live.peek(window,hwnd,RECT{left:0,top:0,right:w,bottom:h}){
        saved=stage_snapshot::peek_frame(source);
        if let Some((_,sw,sh))=saved.as_ref(){
            if let Some((w,h))=fit_preview(*sw,*sh,max_w,max_h){
                let (x,y)=xy(w,h);
                if SetWindowPos(window,ptr::null_mut(),x,y,w,h,SWP_NOACTIVATE|SWP_NOZORDER)==0{saved=None;}
            }else{saved=None;}
        }
    }
    let peek=Peek{owner,source,window,live,saved};
    if !peek.live.includes(hwnd)&&peek.saved.is_none(){return;}
    OPEN.with(|slot|{slot.replace(Some(peek));});
    ShowWindow(window,SW_SHOWNOACTIVATE);
    InvalidateRect(window,ptr::null(),0);
}

#[cfg(test)]
pub(super) unsafe fn probe_external_window(source:WindowIdentity,display:RECT){
    let owner=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,wide("STATIC").as_ptr(),ptr::null(),
        WS_POPUP,display.left+4,display.top+100,70,200,ptr::null_mut(),ptr::null_mut(),
        GetModuleHandleW(ptr::null()),ptr::null_mut());
    assert!(!owner.is_null());
    ShowWindow(owner,SW_SHOWNOACTIVATE);
    let foreground=GetForegroundWindow();
    show_at(owner,source,display,false);
    let peek=OPEN.with(|slot|slot.borrow().as_ref().map(|p|p.window));
    if let Some(window)=peek{
        assert_eq!(GetForegroundWindow(),foreground);
        assert_eq!(IsIconic(source.hwnd as HWND),0);
        assert_ne!(GetWindowLongPtrW(window,GWL_EXSTYLE)&WS_EX_NOACTIVATE as isize,0);
        validate(owner,None);
        assert_eq!(IsWindow(window),0);
        println!("STAGE_PEEK_NATIVE=PASS live_thumbnail=true no_activation=true released=true");
    }else{println!("STAGE_PEEK_NATIVE=NOT_VERIFIED DWM thumbnail unavailable");}
    DestroyWindow(owner);
}
