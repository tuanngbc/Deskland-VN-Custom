//! Prepared maximize/restore/Snap commands, reusing the shared GPU and Guardian.
use super::{WindowIdentity,STATE,CONTROL,window_effect::Proxy,window_lease::Lease,stage_snapshot,motion};
use std::{cell::RefCell,mem,ptr,time::{Duration,Instant},sync::{Mutex,mpsc::TryRecvError}};
use serde_json::{Value,json};
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::*,UI::{HiDpi::GetDpiForWindow,WindowsAndMessaging::*}};
pub(super) const TIMER:usize=32;
#[derive(Clone,Copy)]
pub(super) enum Action{Maximize,Restore,Snap(RECT)}
impl Action{
    fn name(self)->&'static str{match self{Self::Maximize=>"maximize",Self::Restore=>"restore",Self::Snap(_)=>"snap"}}
    unsafe fn issue(self,h:HWND)->bool{match self{
        Self::Maximize=>ShowWindowAsync(h,SW_MAXIMIZE)!=0,
        Self::Restore=>ShowWindowAsync(h,SW_RESTORE)!=0,
        Self::Snap(r)=>{
            let mut outer:RECT=mem::zeroed();let Some(visible)=super::window_effect::visible_area(h)else{return false;};
            if GetWindowRect(h,&mut outer)==0{return false;}
            let left=r.left-(visible.left-outer.left);let top=r.top-(visible.top-outer.top);
            let right=r.right+(outer.right-visible.right);let bottom=r.bottom+(outer.bottom-visible.bottom);
            SetWindowPos(h,ptr::null_mut(),left,top,right-left,bottom-top,SWP_NOACTIVATE|SWP_NOZORDER|SWP_ASYNCWINDOWPOS)!=0
        },
    }}
}
enum Phase{Capture,Prime,Conceal,RestoreBeforeSnap,Observe,Travel(motion::Span),Release,NativeRestoreSnap}
struct Transition{
    id:WindowIdentity,action:Action,area:RECT,target:RECT,monitor:usize,dpi:u32,phase:Phase,
    capture:Option<stage_snapshot::TransitionCapture>,proxy:Option<Proxy>,lease:Option<Lease>,
    started:Instant,phase_at:Instant,last_present:Instant,frames:u32,stable:u32,commanded:bool,done:bool,native_reason:Option<String>,
}
thread_local!{static ACTIVE:RefCell<Option<Transition>>=const{RefCell::new(None)};}
#[cfg(test)]thread_local!{static FAIL_PRESENT:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
static LAST:Mutex<Option<Value>>=Mutex::new(None);
pub(super) fn diagnostics()->Value{LAST.lock().ok().and_then(|v|v.clone()).unwrap_or_else(||json!({"availability":"not-requested"}))}
impl Drop for Transition{fn drop(&mut self){
    if let Some(lease)=&self.lease{unsafe{lease.restore();}}
    // This precedes proxy destruction; an opaque native source always wins.
    if !self.done{self.finish(Some("interrupted"));}
}}
impl Transition{
    fn finish(&mut self,reason:Option<&str>){
        if self.done{return;}self.done=true;
        let value=json!({"action":self.action.name(),"requestedEffect":"fluidwin-geometry","actualEffect":if reason.is_none(){"fluidwin-geometry"}else{"windows"},
            "fallbackReason":reason,"preparedFrames":self.frames,"commandIssued":self.commanded,"elapsedMs":self.started.elapsed().as_millis() as u64});
        if cfg!(test)||crate::trial::active(){eprintln!("FLUIDWIN_GEOMETRY {value}");}
        if let Ok(mut last)=LAST.lock(){*last=Some(value);}
    }
    fn phase(&mut self,phase:Phase){self.phase=phase;self.phase_at=Instant::now();self.last_present=self.phase_at;}
    unsafe fn present(&mut self,rect:RECT,opacity:u8)->bool{
        #[cfg(test)]if FAIL_PRESENT.replace(false){return false;}
        let Some(proxy)=&self.proxy else{return false;};
        match proxy.custom(rect,&[],0.0,opacity){
            1=>{self.frames+=1;self.last_present=Instant::now();true},
            2=>self.last_present.elapsed()<Duration::from_millis(150),
            _=>false,
        }
    }
    unsafe fn valid(&self)->bool{
        let h=self.id.hwnd as HWND;
        self.id.matches(h)&&IsWindowVisible(h)!=0&&IsIconic(h)==0&&GetDpiForWindow(h)==self.dpi
            &&MonitorFromRect(&self.area,MONITOR_DEFAULTTONULL) as usize==self.monitor
            &&super::previews::permitted_pid(h).is_ok()&&super::motion_guard::session().is_some()
            &&STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended})&&motion::allowed()
    }
    unsafe fn fallback(&mut self,reason:&str)->bool{
        if let Some(lease)=&self.lease{lease.restore();}
        if !self.commanded&&self.id.matches(self.id.hwnd as HWND){
            if matches!(self.action,Action::Snap(_))&&IsZoomed(self.id.hwnd as HWND)!=0{
                // Keep only the native command pending while restore runs.
                // This preserves the requested Snap even if capture failed.
                if ShowWindowAsync(self.id.hwnd as HWND,SW_RESTORE)!=0{
                    self.proxy=None;self.capture=None;self.stable=0;self.native_reason=Some(reason.into());self.phase(Phase::NativeRestoreSnap);return true;
                }
            }else{self.commanded=self.action.issue(self.id.hwnd as HWND);}
        }
        self.finish(Some(reason));false
    }
    unsafe fn step(&mut self)->bool{
        if matches!(self.phase,Phase::NativeRestoreSnap){
            let h=self.id.hwnd as HWND;
            if self.id.matches(h)&&IsWindowVisible(h)!=0&&IsIconic(h)==0&&super::previews::permitted_pid(h).is_ok()
                &&GetDpiForWindow(h)==self.dpi&&MonitorFromWindow(h,MONITOR_DEFAULTTONULL) as usize==self.monitor
                &&STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended}){
                if IsZoomed(h)==0{
                    if let Some(rect)=super::window_effect::visible_area(h){
                        if super::same_rect(rect,self.target){self.stable+=1;}else{self.target=rect;self.stable=0;}
                        if self.stable>=2{self.commanded=self.action.issue(h);}
                    }
                }
                if !self.commanded&&self.phase_at.elapsed()<Duration::from_millis(550){return true;}
            }
            let reason=self.native_reason.take();self.finish(reason.as_deref());return false;
        }
        if !self.valid(){return self.fallback("window-or-policy-changed");}
        let h=self.id.hwnd as HWND;
        match &self.phase{
            Phase::Capture=>{
                if super::window_effect::visible_area(h).is_none_or(|r|!super::same_rect(r,self.area)){return self.fallback("source-moved");}
                match self.capture.as_ref().unwrap().receiver.try_recv(){
                    Ok(Ok(frame))=>{
                        if frame.size()!=(self.area.right-self.area.left,self.area.bottom-self.area.top){return self.fallback("capture-size");}
                        let Some(mut canvas)=super::monitor_bounds(self.monitor as HMONITOR)else{return self.fallback("monitor-changed");};
                        let margin=(80.0*f64::from(self.dpi)/96.0).ceil() as i32;canvas.left-=margin;canvas.top-=margin;canvas.right+=margin;canvas.bottom+=margin;
                        self.proxy=Proxy::global(&frame,self.area,canvas,self.dpi);self.capture=None;
                        if self.proxy.is_none(){return self.fallback("proxy-unavailable");}self.phase(Phase::Prime);
                    },
                    Err(TryRecvError::Empty) if self.phase_at.elapsed()<Duration::from_millis(550)=>{},
                    _=>return self.fallback("capture-unavailable"),
                }
            },
            Phase::Prime=>{
                if !self.present(self.area,255){return self.fallback("present-failed");}
                if self.frames>=2{
                    self.lease=Lease::begin(h);if self.lease.is_none(){return self.fallback("guardian-lease-unavailable");}self.phase(Phase::Conceal);
                }
            },
            Phase::Conceal=>{
                if !self.present(self.area,255){return self.fallback("present-failed");}
                match self.lease.as_ref().unwrap().status(){
                    1=>{
                        if matches!(self.action,Action::Snap(_))&&IsZoomed(h)!=0{
                            if ShowWindowAsync(h,SW_RESTORE)==0{return self.fallback("native-restore-failed");}
                            self.phase(Phase::RestoreBeforeSnap);
                        }else{
                            if !self.action.issue(h){return self.fallback("native-command-failed");}
                            self.commanded=true;self.phase(Phase::Observe);
                        }
                    },
                    0 if self.phase_at.elapsed()<Duration::from_millis(550)=>{},
                    _=>return self.fallback("guardian-lease-failed"),
                }
            },
            Phase::RestoreBeforeSnap=>{
                if !self.present(self.area,255){return self.fallback("present-failed");}
                if IsZoomed(h)==0{
                    if !self.action.issue(h){return self.fallback("native-command-failed");}self.commanded=true;self.phase(Phase::Observe);
                }else if self.phase_at.elapsed()>Duration::from_millis(550){return self.fallback("native-restore-timeout");}
            },
            Phase::Observe=>{
                if !self.present(self.area,255){return self.fallback("present-failed");}
                let state_ok=(IsZoomed(h)!=0)==matches!(self.action,Action::Maximize);
                if let Some(rect)=super::window_effect::visible_area(h).filter(|rect|state_ok&&match self.action{
                    Action::Snap(r)=>[rect.left-r.left,rect.top-r.top,rect.right-r.right,rect.bottom-r.bottom].iter().all(|v|v.abs()<=6),_=>true,
                }){
                    if super::same_rect(rect,self.target){self.stable+=1;}else{self.target=rect;self.stable=0;}
                    if self.stable>=2{
                        if self.target.right<=self.target.left||self.target.bottom<=self.target.top{return self.fallback("native-bounds-invalid");}
                        self.phase(Phase::Travel(motion::Span::new(300)));
                    }
                }
                if matches!(self.phase,Phase::Observe)&&self.phase_at.elapsed()>Duration::from_millis(550){return self.fallback("native-command-timeout");}
            },
            Phase::Travel(span)=>{
                if super::window_effect::visible_area(h).is_none_or(|r|!super::same_rect(r,self.target)){return self.fallback("native-placement-changed");}
                let progress=span.progress();let k=(1.0-(std::f64::consts::PI*f64::from(progress)).cos())*0.5;
                let mix=|a:i32,b:i32|(f64::from(a)+f64::from(b-a)*k).round() as i32;
                let rect=RECT{left:mix(self.area.left,self.target.left),top:mix(self.area.top,self.target.top),right:mix(self.area.right,self.target.right),bottom:mix(self.area.bottom,self.target.bottom)};
                let before=self.frames;if !self.present(rect,255){return self.fallback("present-failed");}
                if progress>=1.0&&self.frames>before{self.lease.as_ref().unwrap().restore();self.phase(Phase::Release);}
            },
            Phase::Release=>{
                if self.lease.as_ref().unwrap().status()==2{SetForegroundWindow(h);self.finish(None);super::queue_refresh();return false;}
                if self.phase_at.elapsed()>Duration::from_millis(550){return self.fallback("native-reveal-pending");}
            },
            Phase::NativeRestoreSnap=>unreachable!(),
        }
        true
    }
}
pub(super) fn active()->bool{ACTIVE.with(|s|s.borrow().is_some())}
pub(super) unsafe fn frame_ready()->HANDLE{ACTIVE.with(|s|s.borrow().as_ref().filter(|t|!matches!(t.phase,Phase::Release|Phase::NativeRestoreSnap)).and_then(|t|t.proxy.as_ref()).map_or(ptr::null_mut(),|p|p.frame_clock()))}
pub(super) unsafe fn cancel(){let old=ACTIVE.with(|s|s.borrow_mut().take());drop(old);KillTimer(CONTROL.get(),TIMER);super::window_effect::schedule_release();}
pub(super) unsafe fn event(event:u32,h:HWND){
    use windows_sys::Win32::UI::Accessibility::*;
    let stop=ACTIVE.with(|s|s.borrow().as_ref().is_some_and(|t|
        if event==EVENT_SYSTEM_FOREGROUND{h as usize!=t.id.hwnd}
        else{h as usize==t.id.hwnd&&matches!(event,EVENT_OBJECT_DESTROY|EVENT_OBJECT_HIDE|EVENT_SYSTEM_MINIMIZESTART|EVENT_SYSTEM_MOVESIZESTART)}));
    if stop{cancel();}
}
pub(super) unsafe fn tick(){
    let old=ACTIVE.with(|s|s.borrow_mut().take());
    if let Some(mut transition)=old{if transition.step(){ACTIVE.with(|s|s.replace(Some(transition)));}else{drop(transition);KillTimer(CONTROL.get(),TIMER);super::window_effect::schedule_release();}}
}
pub(super) unsafe fn apply(id:WindowIdentity,action:Action)->bool{
    let h=id.hwnd as HWND;
    if !motion::allowed()||motion::settings().window_effect==motion::WindowEffect::Windows||super::motion_guard::session().is_none()||!id.matches(h)||IsIconic(h)!=0||IsWindowVisible(h)==0
        ||GetWindowLongW(h,GWL_EXSTYLE) as u32&WS_EX_LAYERED!=0||super::window_effect::elevation(id.pid)!=Some(false)
        ||super::previews::permitted_pid(h).is_err(){return false;}
    if !STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended}){return false;}
    if matches!(action,Action::Maximize)&&IsZoomed(h)!=0||matches!(action,Action::Restore)&&IsZoomed(h)==0{return false;}
    if GetWindowLongW(h,GWL_STYLE) as u32&WS_THICKFRAME==0||matches!(action,Action::Maximize)&&GetWindowLongW(h,GWL_STYLE) as u32&WS_MAXIMIZEBOX==0{return false;}
    if matches!(action,Action::Snap(r) if r.right<=r.left||r.bottom<=r.top){return false;}
    let Some(area)=super::window_effect::visible_area(h)else{return false;};
    let monitor=MonitorFromWindow(h,MONITOR_DEFAULTTONULL);if monitor.is_null(){return false;}
    super::show_desktop::pause();super::window_motion::cancel();super::window_effect::cancel();cancel();
    let Ok(capture)=stage_snapshot::transition_capture(id,CONTROL.get())else{return false;};
    let now=Instant::now();
    ACTIVE.with(|s|s.replace(Some(Transition{id,action,area,target:area,monitor:monitor as usize,dpi:GetDpiForWindow(h),phase:Phase::Capture,
        capture:Some(capture),proxy:None,lease:None,started:now,phase_at:now,last_present:now,frames:0,stable:0,commanded:false,done:false,native_reason:None})));
    if SetTimer(CONTROL.get(),TIMER,20,None)==0{cancel();return false;}true
}

/// Requested geometry remains usable with native motion/reduced motion or an
/// unavailable capture device. A maximized Snap waits for native restore.
pub(super) unsafe fn command(id:WindowIdentity,action:Action)->bool{
    let h=id.hwnd as HWND;
    if !id.matches(h)||IsWindowVisible(h)==0||IsIconic(h)!=0||GetWindowLongW(h,GWL_STYLE) as u32&WS_THICKFRAME==0
        ||super::previews::permitted_pid(h).is_err()||super::window_effect::elevation(id.pid)!=Some(false)
        ||!STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended})
        ||matches!(action,Action::Snap(r) if r.right<=r.left||r.bottom<=r.top){return false;}
    if apply(id,action){return true;}
    super::show_desktop::pause();super::window_motion::cancel();super::window_effect::cancel();cancel();
    if !matches!(action,Action::Snap(_))||IsZoomed(h)==0{return action.issue(h);}
    let Some(area)=super::window_effect::visible_area(h)else{return false;};let now=Instant::now();
    if ShowWindowAsync(h,SW_RESTORE)==0{return false;}
    ACTIVE.with(|s|s.replace(Some(Transition{id,action,area,target:area,monitor:MonitorFromWindow(h,MONITOR_DEFAULTTONULL) as usize,dpi:GetDpiForWindow(h),phase:Phase::NativeRestoreSnap,
        capture:None,proxy:None,lease:None,started:now,phase_at:now,last_present:now,frames:0,stable:0,commanded:false,done:false,native_reason:Some("native-motion-policy-or-capture".into())})));
    if SetTimer(CONTROL.get(),TIMER,20,None)==0{cancel();return false;}true
}

#[cfg(all(test,target_env="msvc"))]
pub(super) unsafe fn probe(id:WindowIdentity){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    let h=id.hwnd as HWND;let mut original:RECT=mem::zeroed();assert_ne!(GetWindowRect(h,&mut original),0);
    let style=GetWindowLongPtrW(h,GWL_EXSTYLE);let enabled=STATE.with(|s|{let mut s=s.borrow_mut();let old=s.prefs.enabled;s.prefs.enabled=true;old});
    struct Reset{h:HWND,rect:RECT,enabled:bool}
    impl Drop for Reset{fn drop(&mut self){unsafe{cancel();FAIL_PRESENT.set(false);STATE.with(|s|s.borrow_mut().prefs.enabled=self.enabled);ShowWindowAsync(self.h,SW_RESTORE);SetWindowPos(self.h,ptr::null_mut(),self.rect.left,self.rect.top,self.rect.right-self.rect.left,self.rect.bottom-self.rect.top,SWP_NOACTIVATE|SWP_NOZORDER|SWP_ASYNCWINDOWPOS);}}}
    let reset=Reset{h,rect:original,enabled};
    let (_,marker)=super::motion_guard::session().unwrap();let marker=crate::desktop::wide(&marker);
    let pump=||{super::motion_guard::pulse_fixture();super::TASKBAR_WATCH.with(|w|if let Some(w)=w.borrow_mut().as_mut(){w.poll().unwrap();});
        let mut msg:MSG=mem::zeroed();for _ in 0..64{if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&msg);DispatchMessageW(&msg);}tick();std::thread::sleep(Duration::from_millis(12));};
    let wait=|condition:&dyn Fn()->bool|{let started=Instant::now();while !condition(){pump();assert!(started.elapsed()<Duration::from_secs(6),"geometry state timeout");}};
    let prepare=|restore:bool|{
        ShowWindowAsync(h,if restore{SW_MAXIMIZE}else{SW_RESTORE});wait(&||(IsZoomed(h)!=0)==restore&&IsIconic(h)==0);
        if !restore{SetWindowPos(h,ptr::null_mut(),original.left,original.top,original.right-original.left,original.bottom-original.top,SWP_NOACTIVATE|SWP_NOZORDER|SWP_ASYNCWINDOWPOS);
            wait(&||{let mut r:RECT=mem::zeroed();GetWindowRect(h,&mut r)!=0&&super::same_rect(r,original)});}
        super::window_effect::fixture_settle_source(h);
    };
    let monitor=MonitorFromWindow(h,MONITOR_DEFAULTTONEAREST);let mut mi:MONITORINFO=mem::zeroed();mi.cbSize=mem::size_of::<MONITORINFO>() as u32;assert_ne!(GetMonitorInfoW(monitor,&mut mi),0);
    let mut left=mi.rcWork;left.right=(left.left+left.right)/2;let mut right=mi.rcWork;right.left=left.right;
    for action in [Action::Maximize,Action::Restore,Action::Snap(left),Action::Snap(right)]{
        for attempt in 0..3{
            prepare(matches!(action,Action::Restore));assert!(apply(id,action));let mut sampled=false;let started=Instant::now();
            while active(){
                ACTIVE.with(|s|if let Some(t)=s.borrow().as_ref(){if let Some(p)=&t.proxy{p.enable_evidence();}});pump();
                ACTIVE.with(|s|if let Some(t)=s.borrow().as_ref(){if matches!(t.phase,Phase::Travel(span) if span.progress()>0.15&&span.progress()<0.85)&&!sampled{
                    let (pixels,w,height,_)=t.proxy.as_ref().unwrap().evidence().unwrap();
                    assert!(pixels.chunks_exact(4).any(|p|p[3]>128));for p in pixels.chunks_exact(4){assert!(p[..3].iter().all(|c|*c<=p[3]));}
                    if let Some(dir)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){
                        let rgba=pixels.chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],p[3]]).collect::<Vec<_>>();
                        image::save_buffer(std::path::PathBuf::from(dir).join(format!("geometry-{}-{}.png",action.name(),if matches!(action,Action::Snap(r) if r.left==left.left){"left"}else{"right"})),&rgba,w,height,image::ColorType::Rgba8).unwrap();
                    }sampled=true;
                }});
                assert!(started.elapsed()<Duration::from_secs(6));
            }
            wait(&||GetWindowLongPtrW(h,GWL_EXSTYLE)==style&&GetPropW(h,marker.as_ptr()).is_null());
            let result=diagnostics();if result["actualEffect"]=="fluidwin-geometry"{
                assert!(sampled,"real intermediate geometry pixels");assert!(result["preparedFrames"].as_u64().unwrap()>5);break;
            }
            assert_eq!(result["fallbackReason"],"capture-unavailable","unexpected geometry failure: {result}");assert!(attempt<2,"geometry never reached a prepared capture: {result}");
        }
        assert_eq!(IsZoomed(h)!=0,matches!(action,Action::Maximize));
        if let Action::Snap(r)=action{let actual=super::window_effect::visible_area(h).unwrap();assert!([actual.left-r.left,actual.top-r.top,actual.right-r.right,actual.bottom-r.bottom].iter().all(|v|v.abs()<=6));}
        println!("FLUIDWIN_GEOMETRY_FIXTURE action={} rendered_midpoint=PASS original_style=RESTORED journal=CLEARED",action.name());
    }
    for failure in ["cancel","injected-present-failure","injected-monitor-change"]{
        prepare(false);assert!(apply(id,Action::Maximize));wait(&||!active()||!GetPropW(h,marker.as_ptr()).is_null());assert!(active(),"geometry recovery requires a journaled proxy");
        match failure{"cancel"=>cancel(),"injected-present-failure"=>{FAIL_PRESENT.set(true);tick();},_=>{ACTIVE.with(|s|s.borrow_mut().as_mut().unwrap().monitor=0);tick();}}
        assert!(!active());wait(&||GetWindowLongPtrW(h,GWL_EXSTYLE)==style&&GetPropW(h,marker.as_ptr()).is_null());assert_ne!(IsWindowVisible(h),0);assert_eq!(IsIconic(h),0);
        println!("FLUIDWIN_GEOMETRY_FIXTURE recovery={failure} original_style=RESTORED journal=CLEARED");
    }
    drop(reset);wait(&||{let mut r:RECT=mem::zeroed();GetWindowRect(h,&mut r)!=0&&super::same_rect(r,original)});
    super::window_effect::fixture_settle_source(h);
}
