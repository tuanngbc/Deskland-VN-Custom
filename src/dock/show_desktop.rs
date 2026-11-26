//! FluidWin dock.h desktop cascade, sharing the existing motion controller.
use super::{WindowIdentity,STATE,CONTROL};
use std::{cell::RefCell,collections::VecDeque,mem,ptr,time::{Duration,Instant}};
use windows_sys::Win32::{Foundation::*,System::Threading::{OpenProcess,GetProcessTimes,PROCESS_QUERY_LIMITED_INFORMATION},UI::WindowsAndMessaging::*};
pub(super) const TIMER:usize=31;
#[derive(Clone)]
struct Source{id:WindowIdentity,path:String,normal:RECT,birth:u64}
struct Pending{source:Source,minimize:bool,started:Instant}
#[derive(Default)]
struct Desktop{saved:Vec<Source>,queue:VecDeque<Source>,pending:Option<Pending>,next:Option<(usize,bool)>,owner:usize,minimize:bool}
thread_local!{static DESKTOP:RefCell<Desktop>=RefCell::new(Desktop::default());}
unsafe fn birth(pid:u32)->Option<u64>{
    let process=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,0,pid);if process.is_null(){return None;}
    let mut created:FILETIME=mem::zeroed();let mut exit=created;let mut kernel=created;let mut user=created;
    let ok=GetProcessTimes(process,&mut created,&mut exit,&mut kernel,&mut user)!=0;CloseHandle(process);
    ok.then_some((u64::from(created.dwHighDateTime)<<32)|u64::from(created.dwLowDateTime))
}
unsafe fn normal(h:HWND)->Option<RECT>{
    let mut p:WINDOWPLACEMENT=mem::zeroed();p.length=mem::size_of::<WINDOWPLACEMENT>() as u32;
    (GetWindowPlacement(h,&mut p)!=0).then_some(p.rcNormalPosition)
}
impl Source{
    unsafe fn read(id:WindowIdentity,path:String)->Option<Self>{
        let h=id.hwnd as HWND;
        if !id.matches(h)||IsWindowVisible(h)==0||IsIconic(h)!=0||GetWindowLongW(h,GWL_STYLE) as u32&WS_MINIMIZEBOX==0
            ||super::window_effect::elevation(id.pid)!=Some(false){return None;}
        Some(Self{id,path,normal:normal(h)?,birth:birth(id.pid)?})
    }
    unsafe fn valid(&self)->bool{
        let h=self.id.hwnd as HWND;
        self.id.matches(h)&&birth(self.id.pid)==Some(self.birth)&&IsWindowVisible(h)!=0
            &&normal(h).is_some_and(|r|super::same_rect(r,self.normal))
    }
}
unsafe fn sources()->Vec<Source>{
    // State includes windows from apps hidden from the Dock. Read fresh native
    // Z order so restoration can replay from the back to the original front.
    let candidates=STATE.with(|s|s.borrow().windows.iter().filter_map(|w|Source::read(WindowIdentity::read(w.hwnd)?,w.path.clone())).collect::<Vec<_>>());
    let mut ordered=Vec::new();let mut h=GetTopWindow(ptr::null_mut());let mut remaining=4096;
    while !h.is_null()&&remaining>0&&ordered.len()<200{
        if let Some(source)=candidates.iter().find(|s|s.id.hwnd==h as usize){ordered.push(source.clone());}
        h=GetWindow(h,GW_HWNDNEXT);remaining-=1;
    }
    ordered
}
pub(super) unsafe fn can_restore()->bool{DESKTOP.with(|s|s.borrow().saved.iter().any(|s|s.valid()&&IsIconic(s.id.hwnd as HWND)!=0))}
pub(super) unsafe fn perform(owner:HWND,restore:bool){
    let enabled=STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended});if !enabled{return;}
    super::window_geometry::cancel();
    super::window_motion::cancel();super::overview::close_for(ptr::null_mut());super::utilities::close_panels();
    DESKTOP.with(|s|s.borrow_mut().next=Some((owner as usize,!restore)));
    if SetTimer(CONTROL.get(),TIMER,45,None)==0{pause();return;}
    tick();
}
impl Desktop{
    unsafe fn prepare(&mut self,owner:usize,minimize:bool,opened:Vec<Source>){
        self.owner=owner;self.minimize=minimize;
        self.saved.retain(|s|s.valid()&&IsIconic(s.id.hwnd as HWND)!=0);
        self.queue=if minimize{opened.into()}else{self.saved.iter().rev().cloned().collect()};
    }
    unsafe fn step(&mut self){
        // One shared proxy is intentional. Starting the next effect before the
        // previous one completes would cancel it in the existing controller.
        if super::window_effect::active(){return;}
        if let Some(p)=self.pending.take(){
            let h=p.source.id.hwnd as HWND;
            if p.source.valid()&&(IsIconic(h)!=0)==p.minimize{
                self.saved.retain(|s|s.id!=p.source.id);
                if p.minimize{self.saved.push(p.source);}
            }else if p.source.valid()&&p.started.elapsed()<Duration::from_secs(4){self.pending=Some(p);return;}
        }
        if let Some((owner,minimize))=self.next.take(){self.prepare(owner,minimize,if minimize{sources()}else{Vec::new()});}
        while let Some(source)=self.queue.pop_front(){
            let h=source.id.hwnd as HWND;
            if !source.valid()||(IsIconic(h)!=0)==self.minimize{continue;}
            let animated=if self.minimize{super::window_effect::minimize(self.owner as HWND,&source.path,source.id)}
                else{super::window_effect::restore(self.owner as HWND,&source.path,source.id)};
            if animated||PostMessageW(h,WM_SYSCOMMAND,if self.minimize{SC_MINIMIZE}else{SC_RESTORE} as usize,0)!=0{
                self.pending=Some(Pending{source,minimize:self.minimize,started:Instant::now()});break;
            }
        }
    }
}
pub(super) unsafe fn tick(){
    let mut desktop=DESKTOP.with(|s|mem::take(&mut *s.borrow_mut()));
    let enabled=STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended});
    if enabled{desktop.step();}else{desktop.queue.clear();desktop.pending=None;desktop.next=None;}
    let busy=desktop.pending.is_some()||!desktop.queue.is_empty()||desktop.next.is_some();
    DESKTOP.with(|s|s.replace(desktop));if !busy{KillTimer(CONTROL.get(),TIMER);}
}
pub(super) unsafe fn pause(){
    KillTimer(CONTROL.get(),TIMER);
    DESKTOP.with(|s|{let mut s=s.borrow_mut();
        if let Some(p)=s.pending.take(){if p.minimize&&p.source.valid()&&IsIconic(p.source.id.hwnd as HWND)!=0{s.saved.push(p.source);}}
        s.queue.clear();s.next=None;
    });
}
pub(super) unsafe fn reset(){pause();DESKTOP.with(|s|s.replace(Desktop::default()));}
pub(super) fn forget(hwnd:HWND){DESKTOP.with(|s|{let mut s=s.borrow_mut();s.saved.retain(|x|x.id.hwnd!=hwnd as usize);s.queue.retain(|x|x.id.hwnd!=hwnd as usize);if s.pending.as_ref().is_some_and(|p|p.source.id.hwnd==hwnd as usize){s.pending=None;}});}

#[cfg(all(test,target_env="msvc"))]
pub(super) unsafe fn probe(owner:HWND,ids:&[WindowIdentity]){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    let previous=super::motion::settings();super::motion::configure(super::motion::Settings{enabled:false,..previous});
    let prefs=STATE.with(|s|{let mut s=s.borrow_mut();let old=s.prefs.enabled;s.prefs.enabled=true;old});
    struct Reset{motion:super::motion::Settings,prefs:bool,ids:Vec<WindowIdentity>}
    impl Drop for Reset{fn drop(&mut self){unsafe{reset();super::motion::configure(self.motion);STATE.with(|s|s.borrow_mut().prefs.enabled=self.prefs);for id in &self.ids{if id.matches(id.hwnd as HWND){ShowWindowAsync(id.hwnd as HWND,SW_RESTORE);}}}}}
    let _reset=Reset{motion:previous,prefs,ids:ids.to_vec()};
    let pump=||{super::motion_guard::pulse_fixture();super::TASKBAR_WATCH.with(|w|if let Some(w)=w.borrow_mut().as_mut(){w.poll().unwrap();});let mut msg:MSG=mem::zeroed();for _ in 0..64{if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&msg);DispatchMessageW(&msg);}super::window_effect::timer();tick();std::thread::sleep(Duration::from_millis(12));};
    let wait=|condition:&dyn Fn()->bool|{let start=Instant::now();while !condition(){pump();assert!(start.elapsed()<Duration::from_secs(6),"desktop cascade state timeout");}};
    for id in ids{ShowWindowAsync(id.hwnd as HWND,SW_RESTORE);}wait(&||ids.iter().all(|id|IsIconic(id.hwnd as HWND)==0));
    let opened=ids.iter().map(|id|Source::read(*id,"fixture.exe".into()).unwrap()).collect::<Vec<_>>();
    // A window minimized before the command must never enter its restore set.
    ShowWindowAsync(ids[1].hwnd as HWND,SW_MINIMIZE);wait(&||IsIconic(ids[1].hwnd as HWND)!=0);
    DESKTOP.with(|s|s.borrow_mut().prepare(owner as usize,true,vec![opened[0].clone()]));tick();
    wait(&||DESKTOP.with(|s|s.borrow().pending.is_none())&&IsIconic(ids[0].hwnd as HWND)!=0);
    assert!(can_restore());
    DESKTOP.with(|s|s.borrow_mut().prepare(owner as usize,false,Vec::new()));tick();
    wait(&||DESKTOP.with(|s|s.borrow().pending.is_none())&&IsIconic(ids[0].hwnd as HWND)==0);
    assert_ne!(IsIconic(ids[1].hwnd as HWND),0);assert!(!can_restore());
    ShowWindowAsync(ids[1].hwnd as HWND,SW_RESTORE);wait(&||IsIconic(ids[1].hwnd as HWND)==0);
    super::motion::configure(previous);
    DESKTOP.with(|s|s.borrow_mut().prepare(owner as usize,true,opened.clone()));tick();
    assert!(super::window_effect::active(),"desktop cascade uses the shared prepared-capture engine");
    wait(&||DESKTOP.with(|s|{let s=s.borrow();s.pending.is_none()&&s.queue.is_empty()}));
    assert!(ids.iter().all(|id|IsIconic(id.hwnd as HWND)!=0));
    DESKTOP.with(|s|{let mut s=s.borrow_mut();s.prepare(owner as usize,true,Vec::new());assert_eq!(s.saved.len(),ids.len(),"repeated show preserves previously minimized windows");});
    DESKTOP.with(|s|{let mut s=s.borrow_mut();s.prepare(owner as usize,false,Vec::new());assert_eq!(s.queue.iter().map(|s|s.id).collect::<Vec<_>>(),ids.iter().rev().copied().collect::<Vec<_>>());});tick();
    wait(&||DESKTOP.with(|s|{let s=s.borrow();s.pending.is_none()&&s.queue.is_empty()}));
    assert!(ids.iter().all(|id|IsIconic(id.hwnd as HWND)==0));assert!(!can_restore());
    println!("FLUIDWIN_SHOW_DESKTOP native_fallback=PASS shared_animation=EXERCISED reverse_restore=PASS repeated_show=PASS preexisting_minimized=PRESERVED session_state=CLEARED");
}
