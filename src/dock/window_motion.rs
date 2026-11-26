//! Global native move events, one active GPU proxy and the existing Guardian.
use super::{CONTROL,STATE,WindowIdentity,window_effect::Proxy,window_lease::Lease,stage_snapshot,jelly::Jelly};
use std::{cell::RefCell,mem,ptr,time::{Instant,Duration},sync::mpsc::TryRecvError};
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::*,UI::{Accessibility::*,HiDpi::GetDpiForWindow,Input::KeyboardAndMouse::{GetAsyncKeyState,VK_LBUTTON,VK_RBUTTON},WindowsAndMessaging::*}};
pub(super) const TIMER:usize=30;
thread_local!{static ACTIVE:RefCell<Option<Drag>>=const{RefCell::new(None)};static DETACH:RefCell<Option<Detach>>=const{RefCell::new(None)};}
// Original fluidwin.cpp EVENT_OBJECT_LOCATIONCHANGE / gUltimoPosto route for
// apps that move their own HWND without native MOVESIZESTART. One foreground
// observation replaces upstream's all-window map; no idle timer or worker.
#[derive(Clone,Copy)]struct Observation{id:WindowIdentity,rect:RECT,dpi:u32,monitor:usize,at:Instant}
thread_local!{static OBSERVED:RefCell<Option<Observation>>=const{RefCell::new(None)};}
fn custom_move(old:Observation,new:Observation,held:bool)->bool{
    old.id==new.id&&held&&new.at.saturating_duration_since(old.at)<Duration::from_secs(2)&&old.dpi==new.dpi&&old.monitor==new.monitor
        &&(old.rect.left!=new.rect.left||old.rect.top!=new.rect.top)
        &&old.rect.right-old.rect.left==new.rect.right-new.rect.left&&old.rect.bottom-old.rect.top==new.rect.bottom-new.rect.top
}
#[cfg(test)]#[test]
fn custom_drag_admission_requires_same_identity_size_monitor_dpi_and_held_input(){
    let now=Instant::now();let original=Observation{id:WindowIdentity{hwnd:7,pid:8,tid:9},rect:RECT{left:20,top:30,right:420,bottom:330},dpi:144,monitor:2,at:now};
    let moved=Observation{rect:RECT{left:28,top:36,right:428,bottom:336},at:now+Duration::from_millis(20),..original};
    assert!(custom_move(original,moved,true));assert!(!custom_move(original,moved,false));assert!(!custom_move(original,original,true));
    assert!(!custom_move(original,Observation{dpi:96,..moved},true));assert!(!custom_move(original,Observation{monitor:3,..moved},true));
    assert!(!custom_move(original,Observation{id:WindowIdentity{pid:10,..moved.id},..moved},true));
    assert!(!custom_move(original,Observation{at:now+Duration::from_secs(2),..moved},true));
    assert!(!custom_move(original,Observation{rect:RECT{right:430,..moved.rect},..moved},true));
}
unsafe fn observe_custom(h:HWND,moving:bool){
    if !STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&s.prefs.jelly_drag&&!s.suspended}){OBSERVED.with(|s|s.take());return;}
    if h.is_null()||h!=GetForegroundWindow()||GetAncestor(h,GA_ROOT)!=h{return;}
    let mut rect:RECT=mem::zeroed();
    if IsIconic(h)!=0||IsZoomed(h)!=0||IsWindowVisible(h)==0||GetWindowRect(h,&mut rect)==0||rect.left<=-30000||rect.top<=-30000{
        OBSERVED.with(|s|s.take());return;
    }
    let Some(id)=WindowIdentity::read(h)else{return;};let monitor=MonitorFromWindow(h,MONITOR_DEFAULTTONULL);if monitor.is_null(){return;}
    let now=Observation{id,rect,dpi:GetDpiForWindow(h),monitor:monitor as usize,at:Instant::now()};
    let old=OBSERVED.with(|s|s.replace(Some(now)));
    if moving&&!active()&&!DETACH.with(|s|s.borrow().is_some())&&!super::window_geometry::active()&&!super::window_effect::active()
        &&old.is_some_and(|old|custom_move(old,now,held())){start(h);}
}
// Windows retains ownership of restoring a maximized titlebar drag. Wait for
// its actual restored bounds before attaching the existing live Jelly stream.
// No mouse edge, native restore command or global Snap policy is intercepted.
struct Detach{id:WindowIdentity,monitor:usize,dpi:u32,started:Instant,last:Option<RECT>,stable:u32}
#[cfg(test)]thread_local!{static HELD:std::cell::Cell<Option<bool>>=const{std::cell::Cell::new(None)};}
#[cfg(test)]thread_local!{static PREPARE_TIMEOUT:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
unsafe fn held()->bool{
    #[cfg(test)]if let Some(value)=HELD.get(){return value;}
    // GetAsyncKeyState reads physical buttons. Follow Windows' logical
    // primary button, as upstream's detach path does, without changing it.
    GetAsyncKeyState(i32::from(if GetSystemMetrics(SM_SWAPBUTTON)!=0{VK_RBUTTON}else{VK_LBUTTON}))<0
}
struct Drag{
    id:WindowIdentity,outer:RECT,borders:RECT,area:RECT,monitor:usize,dpi:u32,
    capture:stage_snapshot::TransitionCapture,proxy:Option<Proxy>,jelly:Jelly,
    lease:Option<Lease>,started:Instant,last:Instant,released:Option<Instant>,revealed:Option<Instant>,frames:u32,
}
impl Drop for Drag{fn drop(&mut self){
    // Revoke concealment before destroying the only rendered replacement.
    if let Some(lease)=&self.lease{unsafe{lease.restore();}}
}}
pub(super) fn active()->bool{ACTIVE.with(|s|s.borrow().is_some())}
pub(super) unsafe fn cancel(){OBSERVED.with(|s|s.take());DETACH.with(|s|s.take());let old=ACTIVE.with(|s|s.borrow_mut().take());if old.is_some(){drop(old);super::window_effect::schedule_release();}KillTimer(CONTROL.get(),TIMER);}
pub(super) unsafe fn frame_ready()->HANDLE{ACTIVE.with(|s|s.borrow().as_ref().filter(|d|d.revealed.is_none()||d.lease.as_ref().is_some_and(|l|l.status()==2)).and_then(|d|d.proxy.as_ref()).map_or(ptr::null_mut(),|p|p.frame_clock()))}
pub(super) unsafe fn event(event:u32,h:HWND){
    if event==EVENT_SYSTEM_FOREGROUND{
        if DETACH.with(|s|s.borrow().as_ref().is_some_and(|d|d.id.hwnd!=h as usize)){cancel();}
        if ACTIVE.with(|s|s.borrow().as_ref().is_some_and(|d|d.id.hwnd!=h as usize)){cancel();}
        observe_custom(h,false);
    }else if event==EVENT_SYSTEM_MOVESIZESTART{start(h);}
    else if event==EVENT_OBJECT_LOCATIONCHANGE{observe_custom(h,true);}
    else if event==EVENT_SYSTEM_MOVESIZEEND{
        if DETACH.with(|s|s.borrow().as_ref().is_some_and(|d|d.id.hwnd==h as usize)){cancel();}
        ACTIVE.with(|s|if let Some(d)=s.borrow_mut().as_mut().filter(|d|d.id.hwnd==h as usize){d.released.get_or_insert_with(Instant::now);});
    }else if event==EVENT_OBJECT_DESTROY||event==EVENT_OBJECT_HIDE{
        if OBSERVED.with(|s|s.borrow().as_ref().is_some_and(|o|o.id.hwnd==h as usize)){OBSERVED.with(|s|s.take());}
        if DETACH.with(|s|s.borrow().as_ref().is_some_and(|d|d.id.hwnd==h as usize)){cancel();}
        if ACTIVE.with(|s|s.borrow().as_ref().is_some_and(|d|d.id.hwnd==h as usize)){cancel();}
    }
}
unsafe fn start(h:HWND){
    #[cfg(test)]PREPARE_TIMEOUT.set(false);
    let enabled=STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&s.prefs.jelly_drag&&!s.suspended});
    if !enabled||!super::motion::allowed()||super::motion_guard::session().is_none()||!held(){return;}
    // The source can still be layered by our own settling drag. Re-grab that
    // exact live session before the general already-layered refusal below.
    if resume(h){return;}
    super::window_geometry::cancel();
    if IsZoomed(h)!=0{
        if DETACH.with(|s|s.borrow().as_ref().is_some_and(|d|d.id.hwnd==h as usize&&d.id.matches(h))){return;}
        if super::window_effect::active()||IsIconic(h)!=0||GetWindowLongW(h,GWL_EXSTYLE) as u32&WS_EX_LAYERED!=0{return;}
        let Some(id)=WindowIdentity::read(h).filter(|id|super::previews::permitted_pid(h).is_ok()&&super::window_effect::elevation(id.pid)==Some(false))else{return;};
        let monitor=MonitorFromWindow(h,MONITOR_DEFAULTTONULL);if monitor.is_null(){return;}
        cancel();DETACH.with(|s|s.replace(Some(Detach{id,monitor:monitor as usize,dpi:GetDpiForWindow(h),started:Instant::now(),last:None,stable:0})));
        if SetTimer(CONTROL.get(),TIMER,20,None)==0{cancel();}return;
    }
    if super::window_effect::active()||IsIconic(h)!=0||IsZoomed(h)!=0||GetWindowLongW(h,GWL_EXSTYLE) as u32&WS_EX_LAYERED!=0{return;}
    let Some(id)=WindowIdentity::read(h).filter(|id|super::previews::permitted_pid(h).is_ok()&&super::window_effect::elevation(id.pid)==Some(false))else{return;};
    let Some(area)=super::window_effect::visible_area(h)else{return;};
    let mut point:POINT=mem::zeroed();let mut outer:RECT=mem::zeroed();
    if GetCursorPos(&mut point)==0||GetWindowRect(h,&mut outer)==0||PtInRect(&area,point)==0{return;}
    let monitor=MonitorFromWindow(h,MONITOR_DEFAULTTONULL);if monitor.is_null(){return;}
    cancel();super::utilities::close_panels();
    let Ok(capture)=stage_snapshot::live_capture(id,CONTROL.get())else{return;};
    let borders=RECT{left:area.left-outer.left,top:area.top-outer.top,right:outer.right-area.right,bottom:outer.bottom-area.bottom};
    let now=Instant::now();
    ACTIVE.with(|s|s.replace(Some(Drag{id,outer,borders,area,monitor:monitor as usize,dpi:GetDpiForWindow(h),capture,
        proxy:None,jelly:Jelly::new(area,point),lease:None,started:now,last:now,released:None,revealed:None,frames:0})));
    if SetTimer(CONTROL.get(),TIMER,20,None)==0{cancel();}
}
unsafe fn resume(h:HWND)->bool{
    ACTIVE.with(|s|{
        let mut s=s.borrow_mut();let Some(d)=s.as_mut().filter(|d|d.id.hwnd==h as usize&&d.id.matches(h))else{return false;};
        if IsIconic(h)!=0||IsZoomed(h)!=0||GetDpiForWindow(h)!=d.dpi||MonitorFromWindow(h,MONITOR_DEFAULTTONULL) as usize!=d.monitor{return false;}
        if d.revealed.is_some()&&d.lease.as_ref().is_some_and(|l|l.status()!=2){return false;}
        let mut outer:RECT=mem::zeroed();let mut point:POINT=mem::zeroed();
        let Some(area)=super::window_effect::visible_area(h)else{return false;};
        if GetWindowRect(h,&mut outer)==0||GetCursorPos(&mut point)==0||PtInRect(&area,point)==0
            ||area.right-area.left!=d.area.right-d.area.left||area.bottom-area.top!=d.area.bottom-d.area.top{return false;}
        if d.revealed.take().is_some(){d.lease=None;d.frames=0;}
        d.area=area;d.outer=outer;d.released=None;d.started=Instant::now();d.last=d.started;d.jelly.regrab(area,point);
        true
    })
}
impl Drag{
    unsafe fn step(&mut self)->bool{
        let h=self.id.hwnd as HWND;
        let enabled=STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&s.prefs.jelly_drag&&!s.suspended});
        if !enabled||!super::motion::allowed()||!self.id.matches(h)||super::motion_guard::session().is_none()
            ||super::previews::permitted_pid(h).is_err()||IsWindowVisible(h)==0||IsIconic(h)!=0||IsZoomed(h)!=0
            ||GetDpiForWindow(h)!=self.dpi||MonitorFromWindow(h,MONITOR_DEFAULTTONULL) as usize!=self.monitor{return false;}
        let mut outer:RECT=mem::zeroed();
        if GetWindowRect(h,&mut outer)==0||outer.right-outer.left!=self.outer.right-self.outer.left||outer.bottom-outer.top!=self.outer.bottom-self.outer.top{return false;}
        let rect=RECT{left:outer.left+self.borders.left,top:outer.top+self.borders.top,right:outer.right-self.borders.right,bottom:outer.bottom-self.borders.bottom};
        let now=Instant::now();if !held(){self.released.get_or_insert(now);}
        match self.capture.receiver.try_recv(){
            Ok(Ok(frame)) if self.proxy.is_none()=>{
                if frame.size()!=(self.area.right-self.area.left,self.area.bottom-self.area.top){return false;}
                let Some(mut canvas)=super::monitor_bounds(self.monitor as HMONITOR)else{return false;};
                let margin=(160.0*f64::from(self.dpi)/96.0).ceil() as i32;
                canvas.left-=margin;canvas.top-=margin;canvas.right+=margin;canvas.bottom+=margin;
                self.proxy=Proxy::global(&frame,self.area,canvas,self.dpi);if self.proxy.is_none(){return false;}
            },
            Ok(Err(reason))=>{if cfg!(test)||crate::trial::active(){eprintln!("FLUIDWIN_JELLY capture fallback: {reason}");}return false;},
            Err(TryRecvError::Disconnected)=>return false,
            _=>{},
        }
        let Some(proxy)=&self.proxy else{
            let waiting=self.released.is_none()&&self.started.elapsed()<Duration::from_millis(550);
            if !waiting&&self.released.is_none(){
                #[cfg(test)]PREPARE_TIMEOUT.set(true);
                if cfg!(test)||crate::trial::active(){eprintln!("FLUIDWIN_JELLY native_fallback=capture-timeout timing={:?}",self.capture.timing.lock().ok().as_deref());}
            }
            return waiting;
        };
        if self.lease.is_none()&&self.released.is_some(){return false;}
        if outer.left==self.outer.left&&outer.top==self.outer.top&&self.lease.is_none(){return true;}
        let scale=f64::from(self.dpi)/96.0;let speed=f64::from(super::motion::settings().speed)/100.0;
        let dt=now.duration_since(self.last).as_secs_f64()*speed;self.last=now;
        let settled=self.jelly.step(rect,dt,scale,self.released.is_none());
        if self.revealed.is_none()&&self.released.is_some_and(|t|settled||t.elapsed().as_secs_f64()*speed>=2.5){
            if let Some(lease)=&self.lease{lease.restore();}self.revealed=Some(now);
        }
        let (opacity,deform)=if let Some(t)=self.revealed{
            let Some(lease)=&self.lease else{return false;};
            if lease.status()!=2{return t.elapsed()<Duration::from_millis(500);}
            let progress=(t.elapsed().as_secs_f64()*speed/0.15).clamp(0.0,1.0);
            if progress>=1.0{return false;}(((1.0-progress)*255.0).round() as u8,false)
        }else{(255,true)};
        let vertices=self.jelly.vertices();
        let status=proxy.custom(rect,if deform{&vertices[..]}else{&[]},(self.jelly.lift*self.jelly.lift*(3.0-2.0*self.jelly.lift)) as f32,opacity);
        if status!=1&&status!=2{return false;}
        if status==1{self.frames+=1;}
        if self.frames>=2&&self.lease.is_none(){self.lease=Lease::begin_interactive(h);if self.lease.is_none(){return false;}}
        if let Some(lease)=&self.lease{
            let status=lease.status();if status<0{return false;}
            if status==0&&self.started.elapsed()>Duration::from_millis(850){return false;}
        }
        true
    }
}
pub(super) unsafe fn tick(){
    let detach=DETACH.with(|s|s.borrow_mut().take());
    if let Some(mut d)=detach{
        let h=d.id.hwnd as HWND;let enabled=STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&s.prefs.jelly_drag&&!s.suspended});
        let valid=enabled&&held()&&super::motion::allowed()&&super::motion_guard::session().is_some()&&d.started.elapsed()<Duration::from_secs(4)
            &&d.id.matches(h)&&IsWindowVisible(h)!=0&&IsIconic(h)==0&&GetDpiForWindow(h)==d.dpi&&MonitorFromWindow(h,MONITOR_DEFAULTTONULL) as usize==d.monitor
            &&super::previews::permitted_pid(h).is_ok()&&GetWindowLongW(h,GWL_EXSTYLE) as u32&WS_EX_LAYERED==0;
        if valid{
            if IsZoomed(h)==0{
                if let Some(area)=super::window_effect::visible_area(h){
                    if d.last.is_some_and(|old|old.right-old.left==area.right-area.left&&old.bottom-old.top==area.bottom-area.top){d.stable+=1;}else{d.stable=0;}
                    d.last=Some(area);
                    if d.stable>=2{start(h);if !active(){KillTimer(CONTROL.get(),TIMER);}return;}
                }
            }else{d.last=None;d.stable=0;}
            DETACH.with(|s|s.replace(Some(d)));return;
        }
        KillTimer(CONTROL.get(),TIMER);
    }
    let active=ACTIVE.with(|s|s.borrow_mut().take());
    if let Some(mut drag)=active{
        if drag.step(){ACTIVE.with(|s|s.replace(Some(drag)));}else{drop(drag);super::window_effect::schedule_release();KillTimer(CONTROL.get(),TIMER);}
    }
}

#[cfg(all(test,target_env="msvc"))]
pub(super) unsafe fn probe_drag(id:WindowIdentity){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use windows_sys::Win32::Graphics::Dwm::DwmFlush;
    let h=id.hwnd as HWND;let mut original:RECT=mem::zeroed();assert_ne!(GetWindowRect(h,&mut original),0);
    let mut cursor:POINT=mem::zeroed();GetCursorPos(&mut cursor);let style=GetWindowLongPtrW(h,GWL_EXSTYLE);
    let old=STATE.with(|s|{let mut s=s.borrow_mut();let old=s.prefs.clone();s.prefs.enabled=true;s.prefs.jelly_drag=true;old});
    struct Reset{prefs:super::Options,h:HWND,rect:RECT,cursor:POINT}
    impl Drop for Reset{fn drop(&mut self){unsafe{
        cancel();HELD.set(None);STATE.with(|s|s.borrow_mut().prefs=self.prefs.clone());
        SetWindowPos(self.h,ptr::null_mut(),self.rect.left,self.rect.top,self.rect.right-self.rect.left,self.rect.bottom-self.rect.top,SWP_NOACTIVATE|SWP_NOZORDER|SWP_ASYNCWINDOWPOS);
        SetCursorPos(self.cursor.x,self.cursor.y);
    }}}
    let reset=Reset{prefs:old,h,rect:original,cursor};
    let (_,marker)=super::motion_guard::session().unwrap();let marker=crate::desktop::wide(&marker);
    let pump=||{
        super::TASKBAR_WATCH.with(|w|if let Some(w)=w.borrow_mut().as_mut(){w.poll().unwrap();});super::motion_guard::pulse_fixture();
        let mut msg:MSG=mem::zeroed();for _ in 0..64{if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&msg);DispatchMessageW(&msg);}
        tick();std::thread::sleep(Duration::from_millis(12));
    };
    // Pending native detach does not conceal or command the source, and every
    // cancellation path must end its existing temporary motion timer.
    for refusal in ["release","end","monitor","dpi","deadline","foreground"]{
        ShowWindowAsync(h,SW_MAXIMIZE);let until=Instant::now();while IsZoomed(h)==0{pump();assert!(until.elapsed()<Duration::from_secs(4));}
        HELD.set(Some(true));event(EVENT_SYSTEM_MOVESIZESTART,h);
        assert!(DETACH.with(|s|s.borrow().is_some()));assert!(!active());
        for _ in 0..3{pump();}
        assert_eq!(GetWindowLongPtrW(h,GWL_EXSTYLE),style);assert!(GetPropW(h,marker.as_ptr()).is_null());assert_ne!(IsZoomed(h),0);
        match refusal{
            "release"=>HELD.set(Some(false)),"end"=>event(EVENT_SYSTEM_MOVESIZEEND,h),"foreground"=>event(EVENT_SYSTEM_FOREGROUND,ptr::null_mut()),
            "monitor"=>DETACH.with(|s|s.borrow_mut().as_mut().unwrap().monitor=0),
            "dpi"=>DETACH.with(|s|s.borrow_mut().as_mut().unwrap().dpi=0),
            _=>DETACH.with(|s|s.borrow_mut().as_mut().unwrap().started=Instant::now()-Duration::from_secs(5)),
        }
        pump();assert!(DETACH.with(|s|s.borrow().is_none()));assert!(!active());
        assert_eq!(GetWindowLongPtrW(h,GWL_EXSTYLE),style);assert!(GetPropW(h,marker.as_ptr()).is_null());assert_ne!(IsZoomed(h),0);
        println!("FLUIDWIN_JELLY_NATIVE_DETACH cancellation={refusal} no_native_restore_command=PASS no_visibility_lease=PASS pending_cleared=PASS");
    }
    for mode in ["release","native-maximized-detach","custom-app","regrab","cancel","resize","monitor"]{
        'setup:for attempt in 0..3{
        ShowWindowAsync(h,SW_RESTORE);SetWindowPos(h,ptr::null_mut(),original.left,original.top,original.right-original.left,original.bottom-original.top,SWP_NOACTIVATE|SWP_NOZORDER|SWP_ASYNCWINDOWPOS);
        PostMessageW(h,WM_APP+171,0x00ff0000,0);std::thread::sleep(Duration::from_millis(100));
        super::window_effect::fixture_settle_source(h);
        if mode=="native-maximized-detach"{
            ShowWindowAsync(h,SW_MAXIMIZE);let until=Instant::now();while IsZoomed(h)==0{pump();assert!(until.elapsed()<Duration::from_secs(4));}
            HELD.set(Some(true));event(EVENT_SYSTEM_MOVESIZESTART,h);assert!(!active());assert!(DETACH.with(|s|s.borrow().is_some()));
            assert_eq!(GetWindowLongPtrW(h,GWL_EXSTYLE),style);assert!(GetPropW(h,marker.as_ptr()).is_null());
            // The fixture supplies the native restore normally owned by the
            // physical Windows titlebar drag; the controller must not issue it.
            ShowWindowAsync(h,SW_RESTORE);let until=Instant::now();
            while !active(){
                if IsZoomed(h)==0{let area=super::window_effect::visible_area(h).unwrap();SetCursorPos(area.left+100,area.top+12);}
                pump();assert!(until.elapsed()<Duration::from_secs(4),"native restored geometry did not attach existing Jelly");
            }
            assert!(DETACH.with(|s|s.borrow().is_none()));
        }else if mode=="custom-app"{
            let area=super::window_effect::visible_area(h).unwrap();SetCursorPos(area.left+100,area.top+12);HELD.set(Some(true));SetForegroundWindow(h);
            let until=Instant::now();while GetForegroundWindow()!=h{pump();SetForegroundWindow(h);assert!(until.elapsed()<Duration::from_secs(3),"custom-app fixture needs actual foreground ownership");}
            event(EVENT_SYSTEM_FOREGROUND,h);event(EVENT_OBJECT_LOCATIONCHANGE,h);assert!(!active(),"unchanged foreground baseline cannot begin a drag");
            SetWindowPos(h,ptr::null_mut(),original.left+12,original.top+8,0,0,SWP_NOSIZE|SWP_NOACTIVATE|SWP_NOZORDER|SWP_ASYNCWINDOWPOS);
            let until=Instant::now();loop{let mut r:RECT=mem::zeroed();GetWindowRect(h,&mut r);if r.left==original.left+12&&r.top==original.top+8{break;}pump();assert!(until.elapsed()<Duration::from_secs(3));}
            event(EVENT_OBJECT_LOCATIONCHANGE,h);
        }else{
            let area=super::window_effect::visible_area(h).unwrap();SetCursorPos(area.left+100,area.top+12);HELD.set(Some(true));event(EVENT_SYSTEM_MOVESIZESTART,h);
        }
        assert!(active(),"global move start must create a real stream request");
        let start=Instant::now();let mut evidence=false;
        loop{
            SetWindowPos(h,ptr::null_mut(),original.left+45,original.top+28,0,0,SWP_NOSIZE|SWP_NOACTIVATE|SWP_NOZORDER|SWP_ASYNCWINDOWPOS);
            pump();
            if !evidence{evidence=ACTIVE.with(|s|s.borrow().as_ref().and_then(|d|d.proxy.as_ref()).is_some_and(|p|p.enable_evidence()));}
            let ready=ACTIVE.with(|s|s.borrow().as_ref().is_some_and(|d|d.lease.as_ref().is_some_and(|l|l.status()==1)));
            if ready{break 'setup;}
            if !active(){
                assert!(PREPARE_TIMEOUT.get(),"Jelly setup failed before a prepared proxy: {mode}");
                assert_eq!(GetWindowLongPtrW(h,GWL_EXSTYLE),style);assert!(GetPropW(h,marker.as_ptr()).is_null());
                println!("FLUIDWIN_JELLY setup={mode} capture_timeout_native_fallback=PASS attempt={attempt}");
                assert!(attempt<2,"Jelly never acquired a real journaled visibility lease: {mode}");break;
            }
            assert!(start.elapsed()<Duration::from_secs(4),"Jelly did not acquire a journaled visibility lease: {mode}");
        }
        }
        assert!(!GetPropW(h,marker.as_ptr()).is_null());let mut color=0;let mut alpha=0;let mut flags=0;
        assert_ne!(GetLayeredWindowAttributes(h,&mut color,&mut alpha,&mut flags),0);assert_eq!((alpha,flags),(1,LWA_ALPHA),"dragged source remains hit-testable");
        if mode=="release"||mode=="native-maximized-detach"||mode=="custom-app"{
            SetWindowPos(h,ptr::null_mut(),original.left+140,original.top+60,0,0,SWP_NOSIZE|SWP_NOACTIVATE|SWP_NOZORDER|SWP_ASYNCWINDOWPOS);
            for _ in 0..4{pump();}
            ACTIVE.with(|s|{
                let s=s.borrow();let d=s.as_ref().unwrap();let vertices=d.jelly.vertices();
                let edge=vertices.iter().step_by(14).map(|v|v.x).collect::<Vec<_>>();
                let spread=edge.iter().copied().fold(f32::NEG_INFINITY,f32::max)-edge.iter().copied().fold(f32::INFINITY,f32::min);
                assert!(spread>0.75,"actual moving proxy must deform instead of translating a rectangle: {spread}");
                let (pixels,w,height,_)=d.proxy.as_ref().unwrap().evidence().unwrap();
                for p in pixels.chunks_exact(4){assert!(p[..3].iter().all(|&c|c<=p[3]));}
                if let Some(dir)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){
                    let rgba=pixels.chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],p[3]]).collect::<Vec<_>>();
                    image::save_buffer(std::path::PathBuf::from(dir).join(if mode=="release"{"jelly-moving.png"}else if mode=="custom-app"{"jelly-custom-app-moving.png"}else{"jelly-native-maximized-detach.png"}),&rgba,w,height,image::ColorType::Rgba8).unwrap();
                }
            });
        }
        for _ in 0..24{pump();}
        let sample=|name:&str,expected:usize|{
            ACTIVE.with(|s|{
                let s=s.borrow();let d=s.as_ref().expect("live drag");let p=d.proxy.as_ref().unwrap();
                let (pixels,w,height,canvas)=p.evidence().unwrap();let vertex=d.jelly.vertices()[77];
                let x=(vertex.x.round() as i32-canvas.left).clamp(0,w as i32-1);let y=(vertex.y.round() as i32-canvas.top).clamp(0,height as i32-1);
                let pixel=&pixels[((y as u32*w+x as u32)*4) as usize..][..4];
                assert!(pixel[expected]>180&&pixel[(expected+1)%3]<45&&pixel[(expected+2)%3]<45,"live GPU source {name}: {pixel:?}");
                for pixel in pixels.chunks_exact(4){assert!(pixel[..3].iter().all(|&c|c<=pixel[3]),"premultiplied full Jelly canvas");}
                if let Some(dir)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){
                    let rgba=pixels.chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],p[3]]).collect::<Vec<_>>();
                    image::save_buffer(std::path::PathBuf::from(dir).join(if mode=="release"{format!("jelly-live-{name}.png")}else if mode=="custom-app"{format!("jelly-custom-app-live-{name}.png")}else{format!("jelly-native-detach-live-{name}.png")}),&rgba,w,height,image::ColorType::Rgba8).unwrap();
                }
            });
        };
        if mode=="release"||mode=="native-maximized-detach"||mode=="custom-app"{
            sample("blue",0);
            // WGC reports content changes, not a guaranteed stream cadence for
            // a static HWND. Require new pixels and a new GPU copy for each
            // deliberate repaint, including while the alpha-1 lease is held.
            for (name,color,channel) in [("red",0x000000ff,2),("blue",0x00ff0000,0),("red",0x000000ff,2)]{
                let before=ACTIVE.with(|s|s.borrow().as_ref().unwrap().capture.timing.lock().unwrap().live_frames);
                assert_ne!(PostMessageW(h,WM_APP+171,color,0),0);
                let changed=Instant::now();
                loop{
                    pump();
                    let updated=ACTIVE.with(|s|s.borrow().as_ref().is_some_and(|d|{
                        if d.capture.timing.lock().unwrap().live_frames<=before{return false;}
                        let Some(p)=&d.proxy else{return false;};
                        let Some((pixels,w,height,canvas))=p.evidence()else{return false;};
                        let vertex=d.jelly.vertices()[77];
                        let x=(vertex.x.round() as i32-canvas.left).clamp(0,w as i32-1);
                        let y=(vertex.y.round() as i32-canvas.top).clamp(0,height as i32-1);
                        let pixel=&pixels[((y as u32*w+x as u32)*4) as usize..][..4];
                        pixel[channel]>180&&pixel[(channel+1)%3]<45&&pixel[(channel+2)%3]<45
                    }));
                    if updated{break;}
                    assert!(active()&&changed.elapsed()<Duration::from_secs(3),"same WGC session did not deliver repainted {name} content");
                }
                sample(name,channel);
            }
        }
        if mode=="regrab"{
            let timing=ACTIVE.with(|s|s.borrow().as_ref().unwrap().capture.timing.clone());
            let clock=frame_ready();HELD.set(Some(false));event(EVENT_SYSTEM_MOVESIZEEND,h);pump();
            let area=super::window_effect::visible_area(h).unwrap();SetCursorPos(area.left+100,area.top+12);
            HELD.set(Some(true));event(EVENT_SYSTEM_MOVESIZESTART,h);
            ACTIVE.with(|s|{let s=s.borrow();let d=s.as_ref().unwrap();assert!(d.released.is_none());
                assert!(std::sync::Arc::ptr_eq(&timing,&d.capture.timing),"re-grab keeps the same WGC session and texture");
                assert_eq!(d.proxy.as_ref().unwrap().frame_clock(),clock);assert_eq!(d.lease.as_ref().unwrap().status(),1);
            });
            SetWindowPos(h,ptr::null_mut(),original.left+180,original.top+80,0,0,SWP_NOSIZE|SWP_NOACTIVATE|SWP_NOZORDER|SWP_ASYNCWINDOWPOS);
            for _ in 0..4{pump();}
        }
        match mode{
            "cancel"=>cancel(),
            "resize"=>{SetWindowPos(h,ptr::null_mut(),0,0,original.right-original.left+10,original.bottom-original.top,SWP_NOMOVE|SWP_NOACTIVATE|SWP_NOZORDER|SWP_ASYNCWINDOWPOS);},
            "monitor"=>{ACTIVE.with(|s|s.borrow_mut().as_mut().unwrap().monitor=0);},
            _=>{HELD.set(Some(false));event(EVENT_SYSTEM_MOVESIZEEND,h);},
        }
        let until=Instant::now();
        while active()||GetWindowLongPtrW(h,GWL_EXSTYLE)!=style||!GetPropW(h,marker.as_ptr()).is_null(){
            pump();assert!(until.elapsed()<Duration::from_secs(6),"Jelly failed to restore source: {mode}");
        }
        assert_eq!(IsIconic(h),0);assert_ne!(IsWindowVisible(h),0);DwmFlush();
        println!("FLUIDWIN_JELLY mode={mode} original_style=RESTORED journal=CLEARED live_gpu=EXERCISED input_injection=NONE");
    }
    drop(reset);let until=Instant::now();
    loop{let mut rect:RECT=mem::zeroed();GetWindowRect(h,&mut rect);if super::same_rect(rect,original){break;}pump();assert!(until.elapsed()<Duration::from_secs(3));}
}
