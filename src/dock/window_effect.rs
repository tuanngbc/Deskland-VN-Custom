//! Dock-scoped Scale transitions. A fresh bounded WGC image comes from the
//! existing Stage worker. The only animated HWND is our click-through proxy.
//! Windows retains ownership of the source's placement, visibility and focus.
use super::{motion,stage_snapshot,WindowIdentity,PopupPixels,STATE,CONTROL,queue_refresh};
use crate::desktop::wide;
use std::{cell::RefCell,mem,ptr,time::{Duration,Instant}};
use windows_sys::Win32::{Foundation::*,Graphics::{Dwm::*,Gdi::*},
    System::LibraryLoader::GetModuleHandleW,UI::WindowsAndMessaging::*};

pub(super) const TIMER:usize=17;
const PREPARE:Duration=Duration::from_millis(175);
const SETTLE:Duration=Duration::from_millis(175);
const MAX_PIXELS:i64=2_000_000;
#[derive(Clone,Copy,PartialEq,Eq)]
enum Action{Minimize,Restore}
#[derive(Clone)]
struct Request{
    owner:usize,path:String,source:WindowIdentity,area:RECT,normal:RECT,
    dpi:u32,monitor:usize,title:String,action:Action,
}
struct Recent{request:Request,revision:u64}
thread_local!{
    static ACTIVE:RefCell<Option<Transition>>=const{RefCell::new(None)};
    static RECENT:RefCell<Option<Recent>>=const{RefCell::new(None)};
}
fn dims(r:RECT)->(i32,i32){(r.right-r.left,r.bottom-r.top)}
fn valid_area(r:RECT)->bool{
    let (w,h)=dims(r);w>0&&h>0&&i64::from(w)*i64::from(h)<=MAX_PIXELS
}
fn icon_fit(area:RECT,icon:RECT)->RECT{
    let (w,h)=dims(area);
    let (iw,ih)=dims(icon);
    let s=(iw as f32/w as f32).min(ih as f32/h as f32);
    let (w,h)=((w as f32*s).round().max(1.0) as i32,(h as f32*s).round().max(1.0) as i32);
    let (x,y)=((icon.left+icon.right-w)/2,(icon.top+icon.bottom-h)/2);
    RECT{left:x,top:y,right:x+w,bottom:y+h}
}
fn interpolate(a:RECT,b:RECT,k:f32)->RECT{
    let mix=|x:i32,y:i32|(x as f32+(y as f32-x as f32)*k).round() as i32;
    RECT{left:mix(a.left,b.left),top:mix(a.top,b.top),right:mix(a.right,b.right),bottom:mix(a.bottom,b.bottom)}
}
fn valid_pixels(frame:&stage_snapshot::Frame,area:RECT)->bool{
    let (data,w,h)=frame;
    let (aw,ah)=dims(area);
    if *w<=0||*h<=0||data.len()!=*w as usize* *h as usize*4||aw<=0||ah<=0{return false;}
    if ((*w as f64/ *h as f64)/(aw as f64/ah as f64)-1.0).abs()>0.08{return false;}
    // Reject missing/black/uniform frames rather than animate a blank card.
    let mut lo=255u8;let mut hi=0u8;
    for p in data.chunks_exact(4).step_by(7){
        let value=((u16::from(p[0])+u16::from(p[1])+u16::from(p[2]))/3) as u8;
        lo=lo.min(value);hi=hi.max(value);
    }
    hi>=24&&hi.saturating_sub(lo)>=12
}
unsafe fn placement(hwnd:HWND)->Option<WINDOWPLACEMENT>{
    let mut p:WINDOWPLACEMENT=mem::zeroed();p.length=mem::size_of::<WINDOWPLACEMENT>() as u32;
    (GetWindowPlacement(hwnd,&mut p)!=0).then_some(p)
}
unsafe fn title(hwnd:HWND)->String{
    let mut buf=[0u16;256];let n=GetWindowTextW(hwnd,buf.as_mut_ptr(),buf.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&buf[..n])
}
unsafe fn visible_area(hwnd:HWND)->Option<RECT>{
    let mut r:RECT=mem::zeroed();
    if DwmGetWindowAttribute(hwnd,DWMWA_EXTENDED_FRAME_BOUNDS as u32,(&mut r as *mut RECT).cast(),mem::size_of::<RECT>() as u32)<0||!valid_area(r){return None;}
    Some(r)
}
unsafe fn elevation(pid:u32)->Option<bool>{
    use windows_sys::Win32::{Security::{GetTokenInformation,TokenElevation,TOKEN_ELEVATION,TOKEN_QUERY},
        System::Threading::{OpenProcess,OpenProcessToken,PROCESS_QUERY_LIMITED_INFORMATION}};
    let process=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,0,pid);
    if process.is_null(){return None;}
    let mut token=ptr::null_mut();
    let open=OpenProcessToken(process,TOKEN_QUERY,&mut token);
    CloseHandle(process);
    if open==0{return None;}
    let mut data:TOKEN_ELEVATION=mem::zeroed();let mut len=0;
    let ok=GetTokenInformation(token,TokenElevation,(&mut data as *mut TOKEN_ELEVATION).cast(),mem::size_of::<TOKEN_ELEVATION>() as u32,&mut len);
    CloseHandle(token);
    (ok!=0).then_some(data.TokenIsElevated!=0)
}
// DWMWA_TRANSITIONS_FORCEDISABLED is documented for Set, not Get.
// Do not probe the unsupported getter or change a foreign HWND's policy.
// Only an already-disabled Windows minimize animation can be certified.
unsafe fn without_native_overlap(_hwnd:HWND)->bool{
    let mut info:ANIMATIONINFO=mem::zeroed();
    info.cbSize=mem::size_of::<ANIMATIONINFO>() as u32;
    SystemParametersInfoW(SPI_GETANIMATION,info.cbSize,(&mut info as *mut ANIMATIONINFO).cast(),0)!=0&&info.iMinAnimate==0
}
unsafe fn permitted(id:WindowIdentity)->bool{
    let h=id.hwnd as HWND;
    id.matches(h)&&IsWindowVisible(h)!=0&&GetAncestor(h,GA_ROOT)==h
        &&GetWindowLongPtrW(h,GWL_STYLE)&WS_MINIMIZEBOX as isize!=0
        &&super::previews::permitted_pid(h)==Ok(id.pid)
}
unsafe fn allowed(id:WindowIdentity)->bool{
    if !motion::allowed()||!motion::settings().scale_windows{return false;}
    let reason=if !permitted(id){"source-unavailable-or-protected"}
        else if elevation(id.pid)!=Some(false){"elevated-or-unverified"}
        else if !without_native_overlap(id.hwnd as HWND){"native-animation-overlap"}
        else{return true;};
    if crate::trial::active(){crate::log!("DOCK_WINDOW_SCALE fallback={reason}");}
    false
}
unsafe fn target(owner:HWND,path:&str)->Option<RECT>{
    let slot=STATE.with(|s|{
        let s=s.borrow();
        let b=s.bars.iter().find(|b|b.hwnd==owner&&!b.hidden&&!b.collapsed&&b.reveal.value>=0.997)?;
        let i=b.items.iter().position(|item|item.path.eq_ignore_ascii_case(path))?;
        Some(*b.layout.as_ref()?.slots.get(i)?)
    })?;
    let size=(slot.w.min(slot.h)*0.84).round() as i32;
    if size<8{return None;}
    let mut p=POINT{x:(slot.x+(slot.w-size as f32)*0.5).round() as i32,y:(slot.y+(slot.h-size as f32)*0.5).round() as i32};
    if IsWindowVisible(owner)==0||ClientToScreen(owner,&mut p)==0{return None;}
    Some(RECT{left:p.x,top:p.y,right:p.x+size,bottom:p.y+size})
}
unsafe fn request(owner:HWND,path:&str,id:WindowIdentity,action:Action)->Option<Request>{
    if !allowed(id){return None;}
    let h=id.hwnd as HWND;
    let p=placement(h)?;
    let monitor=MonitorFromWindow(h,MONITOR_DEFAULTTONULL);
    if monitor.is_null(){return None;}
    let mut mi:MONITORINFO=mem::zeroed();mi.cbSize=mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(monitor,&mut mi)==0{return None;}
    let dpi=windows_sys::Win32::UI::HiDpi::GetDpiForWindow(h);
    let name=title(h);
    let area=match action{
        Action::Minimize=>visible_area(h)?,
        Action::Restore=>RECENT.with(|r|{
            let r=r.borrow();let saved=&r.as_ref()?.request;
            (saved.source==id&&saved.monitor==monitor as usize&&saved.dpi==dpi
                &&saved.title==name&&super::same_rect(saved.normal,p.rcNormalPosition)).then_some(saved.area)
        })?,
    };
    if area.left<mi.rcMonitor.left||area.top<mi.rcMonitor.top||area.right>mi.rcMonitor.right||area.bottom>mi.rcMonitor.bottom{return None;}
    target(owner,path)?;
    Some(Request{owner:owner as usize,path:path.into(),source:id,area,normal:p.rcNormalPosition,dpi,monitor:monitor as usize,title:name,action})
}
struct Proxy{hwnd:HWND,source:PopupPixels,canvas:PopupPixels}
impl Drop for Proxy{fn drop(&mut self){unsafe{DestroyWindow(self.hwnd);}}}
impl Proxy{
    unsafe fn new(frame:&stage_snapshot::Frame,area:RECT)->Option<Self>{
        let (data,w,h)=frame;
        if !valid_pixels(frame,area){return None;}
        let (aw,ah)=dims(area);
        let mut source=PopupPixels::new(*w,*h)?;
        source.pixels().copy_from_slice(data);
        let canvas=PopupPixels::new(aw,ah)?;
        let hwnd=CreateWindowExW(WS_EX_LAYERED|WS_EX_TRANSPARENT|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_TOPMOST,
            wide("STATIC").as_ptr(),wide("TLauncher window transition").as_ptr(),WS_POPUP,
            area.left,area.top,1,1,ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null_mut());
        if hwnd.is_null(){return None;}
        let disable=1i32;
        let _=DwmSetWindowAttribute(hwnd,DWMWA_TRANSITIONS_FORCEDISABLED as u32,(&disable as *const i32).cast(),4);
        Some(Self{hwnd,source,canvas})
    }
    unsafe fn present(&self,r:RECT,opacity:u8)->bool{
        let (w,h)=dims(r);
        if w<1||h<1||w>self.canvas.size.0||h>self.canvas.size.1{return false;}
        let dc=self.canvas.dc();
        SetStretchBltMode(dc,HALFTONE);
        SetBrushOrgEx(dc,0,0,ptr::null_mut());
        if StretchBlt(dc,0,0,w,h,self.source.dc(),0,0,self.source.size.0,self.source.size.1,SRCCOPY)==0{return false;}
        let p=POINT{x:r.left,y:r.top};let origin=POINT{x:0,y:0};let size=SIZE{cx:w,cy:h};
        let blend=BLENDFUNCTION{BlendOp:AC_SRC_OVER as u8,BlendFlags:0,SourceConstantAlpha:opacity,AlphaFormat:0};
        if UpdateLayeredWindow(self.hwnd,ptr::null_mut(),&p,&size,dc,&origin,0,&blend,ULW_ALPHA)==0{return false;}
        if IsWindowVisible(self.hwnd)==0{ShowWindow(self.hwnd,SW_SHOWNOACTIVATE);}
        true
    }
}
enum Phase{
    Capture{job:stage_snapshot::TransitionCapture,started:Instant},
    WaitingMinimize{proxy:Proxy,frame:stage_snapshot::Frame,started:Instant},
    Animate{proxy:Proxy,span:motion::Span},
    WaitingRestore{_proxy:Proxy,started:Instant},
}
struct Transition{req:Request,phase:Phase}
impl Transition{
    unsafe fn fallback(self){
        let id=self.req.source;let h=id.hwnd as HWND;
        let action=self.req.action;
        drop(self);
        if !id.matches(h){return;}
        match action{
            Action::Minimize if IsIconic(h)==0=>{
                ShowWindowAsync(h,if super::taskbar_hidden(){SW_SHOWMINNOACTIVE}else{SW_MINIMIZE});
            }
            Action::Restore=>{
                if IsIconic(h)!=0{ShowWindowAsync(h,SW_RESTORE);}
                SetForegroundWindow(h);
            }
            _=>{}
        }
        queue_refresh();
    }
    unsafe fn tick(mut self)->Option<Self>{
        let h=self.req.source.hwnd as HWND;
        if !self.req.source.matches(h){return None;}
        let icon=target(self.req.owner as HWND,&self.req.path);
        let stable=motion::allowed()&&motion::settings().scale_windows&&permitted(self.req.source)
            &&windows_sys::Win32::UI::HiDpi::GetDpiForWindow(h)==self.req.dpi
            &&MonitorFromWindow(h,MONITOR_DEFAULTTONULL) as usize==self.req.monitor
            &&placement(h).is_some_and(|p|super::same_rect(p.rcNormalPosition,self.req.normal));
        if !stable||icon.is_none(){self.fallback();return None;}
        let icon=icon_fit(self.req.area,icon.unwrap());
        let action=self.req.action;
        match self.phase{
            Phase::Capture{ref job,started}=>{
                if started.elapsed()>PREPARE||IsIconic(h)!=0||visible_area(h).is_none_or(|r|!super::same_rect(r,self.req.area)){
                    self.fallback();return None;
                }
                let result=job.receiver.try_recv();
                match result{
                    Ok(Ok(frame))=>{
                        if !without_native_overlap(h){self.fallback();return None;}
                        let Some(proxy)=Proxy::new(&frame,self.req.area)else{self.fallback();return None;};
                        if !proxy.present(self.req.area,255){self.fallback();return None;}
                        if ShowWindowAsync(h,if super::taskbar_hidden(){SW_SHOWMINNOACTIVE}else{SW_MINIMIZE})==0{self.fallback();return None;}
                        self.phase=Phase::WaitingMinimize{proxy,frame,started:Instant::now()};
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty)=>{}
                    _=>{self.fallback();return None;}
                }
            }
            Phase::WaitingMinimize{proxy,frame,started}=>{
                if IsIconic(h)!=0{
                    let revision=stage_snapshot::remember_transition(self.req.source,frame);
                    RECENT.with(|r|{r.replace(revision.map(|revision|Recent{request:self.req.clone(),revision}));});
                    self.phase=Phase::Animate{proxy,span:motion::Span::new(300)};
                    queue_refresh();
                }else if started.elapsed()>SETTLE{
                    self.phase=Phase::WaitingMinimize{proxy,frame,started};
                    self.fallback();return None;
                }else{self.phase=Phase::WaitingMinimize{proxy,frame,started};}
            }
            Phase::Animate{proxy,span}=>{
                if IsIconic(h)==0{
                    // External restore or repeated input wins. Never re-hide it.
                    return None;
                }
                let t=span.progress();
                let k=motion::ease(t);
                let (a,b,opacity)=match action{
                    Action::Minimize=>(self.req.area,icon,(255.0*(1.0-((t-0.72)/0.28).clamp(0.0,1.0))) as u8),
                    Action::Restore=>(icon,self.req.area,(255.0*(0.25+0.75*(t*2.0).min(1.0))) as u8),
                };
                if !proxy.present(interpolate(a,b,k),opacity){
                    self.phase=Phase::Animate{proxy,span};self.fallback();return None;
                }
                if t>=1.0{
                    if action==Action::Minimize{return None;}
                    if !without_native_overlap(h)||ShowWindowAsync(h,SW_RESTORE)==0{
                        self.phase=Phase::Animate{proxy,span};self.fallback();return None;
                    }
                    self.phase=Phase::WaitingRestore{_proxy:proxy,started:Instant::now()};
                }else{self.phase=Phase::Animate{proxy,span};}
            }
            Phase::WaitingRestore{ref _proxy,started}=>{
                if IsIconic(h)==0{
                    SetForegroundWindow(h);queue_refresh();return None;
                }
                if started.elapsed()>SETTLE{self.fallback();return None;}
            }
        }
        Some(self)
    }
}
unsafe fn begin(owner:HWND,path:&str,id:WindowIdentity,action:Action)->bool{
    cancel();
    let Some(req)=request(owner,path,id,action)else{return false;};
    let controller=CONTROL.with(|c|c.get());
    if controller.is_null(){return false;}
    let phase=match action{
        Action::Minimize=>{
            RECENT.with(|r|{r.borrow_mut().take();});
            let Some(job)=stage_snapshot::transition_capture(id,controller)else{return false;};
            Phase::Capture{job,started:Instant::now()}
        }
        Action::Restore=>{
            let frame=RECENT.with(|r|r.borrow_mut().take()).and_then(|r|stage_snapshot::recent_transition(id,r.revision));
            let Some(frame)=frame else{return false;};
            let Some(proxy)=Proxy::new(&frame,req.area)else{return false;};
            let Some(icon)=target(owner,path)else{return false;};
            if !proxy.present(icon_fit(req.area,icon),64){return false;}
            Phase::Animate{proxy,span:motion::Span::new(340)}
        }
    };
    if SetTimer(controller,TIMER,16,None)==0{return false;}
    ACTIVE.with(|a|{a.replace(Some(Transition{req,phase}));});
    true
}
pub(super) unsafe fn minimize(owner:HWND,path:&str,id:WindowIdentity)->bool{begin(owner,path,id,Action::Minimize)}
pub(super) unsafe fn restore(owner:HWND,path:&str,id:WindowIdentity)->bool{begin(owner,path,id,Action::Restore)}
pub(super) unsafe fn timer(){
    let active=ACTIVE.with(|a|a.borrow_mut().take());
    let next=active.and_then(|t|t.tick());
    if next.is_none(){KillTimer(CONTROL.with(|c|c.get()),TIMER);}
    ACTIVE.with(|a|{a.replace(next);});
}
pub(super) fn restoring(id:WindowIdentity)->bool{
    ACTIVE.with(|a|a.borrow().as_ref().is_some_and(|t|t.req.source==id&&t.req.action==Action::Restore))
}
pub(super) unsafe fn cancel(){
    ACTIVE.with(|a|{a.borrow_mut().take();});
    KillTimer(CONTROL.with(|c|c.get()),TIMER);
}
pub(super) fn invalidate_recent(hwnd:HWND){
    RECENT.with(|r|{let mut r=r.borrow_mut();if r.as_ref().is_some_and(|t|t.request.source.hwnd==hwnd as usize){r.take();}});
}
pub(super) unsafe fn reset(){
    cancel();RECENT.with(|r|{r.borrow_mut().take();});
}

#[cfg(test)]
pub(super) unsafe fn probe_owned_window(source:HWND,frame:&stage_snapshot::Frame){
    use windows_sys::Win32::System::Threading::*;
    let area=visible_area(source).unwrap();
    if !valid_pixels(frame,area){println!("WINDOW_SCALE_NATIVE=BLOCKED capture aspect or pixel validation");return;}
    let icon=RECT{left:area.right-48,top:area.bottom-48,right:area.right, bottom:area.bottom};
    let end=icon_fit(area,icon);
    let counts=||{let h=GetCurrentProcess();let mut n=0;GetProcessHandleCount(h,&mut n);
        (GetGuiResources(h,0),GetGuiResources(h,1),n)};
    // Native policy is read-only even in the hosted fixture. The owned
    // transient proxy has its own DWM policy; the source is not modified.
    println!("WINDOW_SCALE_NATIVE_GATE source_policy_already_disabled={}",without_native_overlap(source));
    
    let cold=crate::private_mb();
    let cold_handles=counts();
    let mut before=cold;
    let mut handles=cold_handles;
    let original=placement(source).unwrap();
    let mut visible_pixels=false;
    let mut peak=before;
    let mut longest=0.0f64;
    let settle=||{
        GdiFlush();let _=DwmFlush();
        let mut msg:MSG=mem::zeroed();
        while PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}
    };
    // Warm the system-owned layered HWND and native show-state machinery,
    // then measure 100 additional cycles from a settled baseline.
    for i in 0..105{
        let proxy=Proxy::new(frame,area).expect("valid owned WGC proxy");
        let started=Instant::now();
        assert!(proxy.present(area,255));
        if i==0{
            DwmFlush();
            let dc=GetDC(ptr::null_mut());
            if !dc.is_null(){
                let (w,h)=dims(area);
                let color=GetPixel(dc,area.left+w/10,area.top+h/2);
                let x=frame.1/10;let y=frame.2/2;
                let p=&frame.0[((y*frame.1+x)*4) as usize..][..3];
                if color!=u32::MAX{
                    visible_pixels=[color as u8,(color>>8) as u8,(color>>16) as u8].into_iter()
                        .zip([p[2],p[1],p[0]]).all(|(a,b)|a.abs_diff(b)<28);
                }
                ReleaseDC(ptr::null_mut(),dc);
            }
        }
        // The actual render buffer, not a synthetic icon or just an HWND
        // success code, must contain the captured source's colored marker.
        GdiFlush();
        let color=GetPixel(proxy.canvas.dc(),(area.right-area.left)/10,(area.bottom-area.top)/2);
        assert_ne!(color,u32::MAX);
        assert!((color>>8&255)>(color&255)+25,"expected the owned source's green marker");
        ShowWindow(source,SW_SHOWMINNOACTIVE);
        assert_ne!(IsIconic(source),0);
        for k in [0.25,0.5,0.75,1.0]{assert!(proxy.present(interpolate(area,end,k),255));}
        for k in [0.25,0.5,0.75,1.0]{assert!(proxy.present(interpolate(end,area,k),255));}
        ShowWindow(source,SW_RESTORE);
        assert_eq!(IsIconic(source),0);
        assert!(super::same_rect(placement(source).unwrap().rcNormalPosition,original.rcNormalPosition));
        let now=crate::private_mb();peak=(peak.0.max(now.0),peak.1.max(now.1));
        longest=longest.max(started.elapsed().as_secs_f64()*1000.0);
        let h=proxy.hwnd;drop(proxy);assert_eq!(IsWindow(h),0);
        settle();
        if i==4{
            for _ in 0..6{settle();std::thread::sleep(Duration::from_millis(40));}
            before=crate::private_mb();handles=counts();peak=before;longest=0.0;
            println!("WINDOW_SCALE_NATIVE_WARM handles={cold_handles:?}->{handles:?} memory={cold:?}->{before:?}");
        }
        if i>=24&&(i-4)%20==0{println!("WINDOW_SCALE_NATIVE_PROGRESS cycles={} handles={:?} memory={:?}",i-4,counts(),crate::private_mb());}
    }
    for _ in 0..12{settle();std::thread::sleep(Duration::from_millis(40));}
    let after=crate::private_mb();
    let final_handles=counts();
    println!("WINDOW_SCALE_NATIVE cycles=100 rendered_source_pixels=PASS actual_screen_pixels={} placement=PASS proxy_cleanup=PASS handles={handles:?}->{final_handles:?} memory={before:?}->{after:?} peak={peak:?} max_submit_cycle_ms={longest:.2} native_gate={} overlap=NOT_VERIFIED",
        if visible_pixels{"MATCH"}else{"NOT_VERIFIED"},without_native_overlap(source));
    assert!(final_handles.0<=handles.0+2&&final_handles.1<=handles.1+2&&final_handles.2<=handles.2+10,"steady-state proxy resources grew: {handles:?} -> {final_handles:?}");
    assert!(after.0-before.0<12.0,"private bytes did not recover after 100 bounded window proxies");
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn scale_keeps_center_aspect_and_negative_monitor_coordinates(){
        let source=RECT{left:-1160,top:-400,right:-200,bottom:140};
        let icon=RECT{left:-664,top:650,right:-616,bottom:698};
        let end=icon_fit(source,icon);
        assert_eq!(dims(end),(48,27));
        assert_eq!((end.left+end.right)/2,-640);
        assert!((end.top+end.bottom-1348).abs()<=1);
        assert!(super::super::same_rect(interpolate(source,end,0.0),source));
        assert!(super::super::same_rect(interpolate(source,end,1.0),end));
        let middle=interpolate(source,end,0.5);
        let (w,h)=dims(middle);
        assert!((w as f64/h as f64-960.0/540.0).abs()<0.015);
        assert!(!valid_pixels(&(vec![0;4*48*27],48,27),source));
    }
}
