//! Original FluidWin DWM grid, scoped to one monitor and one modal session.
use super::{WindowIdentity,STATE,Item};
use std::{cell::{Cell,RefCell},ptr,sync::atomic::{AtomicBool,Ordering}};
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::*,UI::WindowsAndMessaging::*};
pub(super) const NOTICE:u32=WM_APP+143;
pub(super) const TIMER:usize=29;
static READY:AtomicBool=AtomicBool::new(false);
thread_local!{
    static ACTIVE:Cell<bool>=const{Cell::new(false)};
    static OWNER:Cell<usize>=const{Cell::new(0)};
    static SOURCES:RefCell<Vec<WindowIdentity>>=const{RefCell::new(Vec::new())};
    static PENDING:Cell<Option<(usize,i32)>>=const{Cell::new(None)};
}
#[cfg(target_env="msvc")]
extern "C"{
    fn tlauncher_overview_show(valid:unsafe extern "C" fn(HWND)->bool,area:*const RECT,speed:f64,reduced:bool,alt:bool,steps:i32)->HWND;
    fn tlauncher_overview_key(key:i32);
    #[cfg(test)] fn tlauncher_overview_window()->HWND;
    #[cfg(test)] fn tlauncher_overview_thumbnails()->u32;
}
pub(super) fn active()->bool{ACTIVE.get()}
pub(super) fn open_for(owner:HWND)->bool{active()&&OWNER.get()==owner as usize}
pub(super) fn ready()->bool{
    if !READY.load(Ordering::Acquire){return false;}
    unsafe{
        let h=GetForegroundWindow();let mut rect:RECT=std::mem::zeroed();
        let fullscreen=super::monitor_bounds(MonitorFromWindow(h,MONITOR_DEFAULTTONULL)).is_some_and(|area|
            GetWindowRect(h,&mut rect)!=0&&rect.left<=area.left&&rect.top<=area.top&&rect.right>=area.right&&rect.bottom>=area.bottom&&GetWindowLongW(h,GWL_STYLE) as u32&WS_POPUP!=0);
        !fullscreen
    }
}
pub(super) fn refresh(){
    let ready=STATE.with(|s|{let Ok(s)=s.try_borrow()else{return false;};
        s.prefs.enabled&&s.prefs.alt_tab_overview&&!s.suspended&&!active()&&!super::window_effect::active()&&
        s.windows.iter().any(|w|unsafe{IsWindowVisible(w.hwnd)!=0&&WindowIdentity::read(w.hwnd).is_some()&&super::previews::permitted_pid(w.hwnd).is_ok()})
    });
    READY.store(ready&&!crate::trial::demo_active()&&!crate::trial::benchmark_active(),Ordering::Release);
}
pub(super) unsafe fn close_for(owner:HWND){
    READY.store(false,Ordering::Release);
    if PENDING.get().is_some_and(|(h,_)|owner.is_null()||h==owner as usize){PENDING.set(None);KillTimer(super::CONTROL.get(),TIMER);}
    if active()&&(owner.is_null()||open_for(owner)){key(99);}
}
pub(super) unsafe fn key(key:i32){#[cfg(target_env="msvc")]tlauncher_overview_key(key);}
pub(super) unsafe fn receive(owner:HWND,step:i32){
    if !matches!(step,-2..=2){return;}
    let first=step.abs()==1;let step=step.signum();
    if active(){key(step);return;}
    if let Some((owner,steps))=PENDING.get(){
        if step==0{
            PENDING.set(None);KillTimer(super::CONTROL.get(),TIMER);direct(owner as HWND,steps);
        }else{PENDING.set(Some((owner,steps.saturating_add(step).clamp(-4096,4096))));}
    }else if step!=0&&first{
        PENDING.set(Some((owner as usize,step)));
        if SetTimer(super::CONTROL.get(),TIMER,180,None)==0{PENDING.set(None);direct(owner,step);}
    }
}
pub(super) unsafe fn poll(){
    KillTimer(super::CONTROL.get(),TIMER);
    if let Some((owner,steps))=PENDING.take(){
        let monitor=super::monitor_bounds(MonitorFromWindow(GetForegroundWindow(),MONITOR_DEFAULTTONEAREST));
        let grid=STATE.with(|s|s.borrow().windows.iter().any(|w|IsIconic(w.hwnd)==0&&monitor.is_some_and(|r|super::rect_intersects(w.rect,r))));
        if grid{show(owner as HWND,true,steps);}else{PENDING.set(Some((owner,steps)));}
    }
}
fn direct_index(steps:i32,count:usize)->Option<usize>{
    if steps==0||count==0{return None;}
    let n=count as i32;Some(if steps>0{(steps-1)%n}else{steps.rem_euclid(n)} as usize)
}
unsafe fn direct(owner:HWND,steps:i32){
    let foreground=GetAncestor(GetForegroundWindow(),GA_ROOT);
    let candidates=STATE.with(|s|{let s=s.borrow();
        if !s.prefs.enabled||!s.prefs.alt_tab_overview||s.suspended{return Vec::new();}
        s.windows.iter().filter(|w|w.hwnd!=foreground).filter_map(|w|WindowIdentity::read(w.hwnd))
            .filter(|id|IsWindowVisible(id.hwnd as HWND)!=0&&super::previews::permitted_pid(id.hwnd as HWND).is_ok()).take(128).collect::<Vec<_>>()
    });
    // Read current native Z order, including minimized windows, as upstream.
    let mut ordered=Vec::new();let mut h=GetTopWindow(ptr::null_mut());let mut remaining=4096;
    while !h.is_null()&&remaining>0{
        if let Some(id)=candidates.iter().find(|id|id.hwnd==h as usize){ordered.push(*id);}
        h=GetWindow(h,GW_HWNDNEXT);remaining-=1;
    }
    if let Some(id)=direct_index(steps,ordered.len()).map(|i|ordered[i]).filter(|id|id.matches(id.hwnd as HWND)){
        let item=Item{path:String::new(),title:String::new(),hwnd:id.hwnd as HWND,count:1,pinned:false};super::activate(owner,&item);
    }
}
unsafe extern "C" fn valid(hwnd:HWND)->bool{
    let identity=SOURCES.with(|s|s.borrow().iter().find(|id|id.hwnd==hwnd as usize).copied());
    identity.is_some_and(|id|id.matches(hwnd)&&IsWindowVisible(hwnd)!=0&&IsIconic(hwnd)==0&&super::previews::permitted_pid(hwnd).is_ok())
}
pub(super) unsafe fn show(owner:HWND,alt:bool,steps:i32){
    if active(){key(99);return;}
    let enabled=STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended&&(!alt||s.prefs.alt_tab_overview)});
    if !enabled||super::window_effect::active(){return;}
    let monitor=if alt{MonitorFromWindow(GetForegroundWindow(),MONITOR_DEFAULTTONEAREST)}
        else{MonitorFromWindow(owner,MONITOR_DEFAULTTONEAREST)};
    let Some(area)=super::monitor_bounds(monitor)else{return;};
    let sources=STATE.with(|s|s.borrow().windows.iter().filter(|w|super::rect_intersects(w.rect,area))
        .filter_map(|w|WindowIdentity::read(w.hwnd)).filter(|id|IsIconic(id.hwnd as HWND)==0&&super::previews::permitted_pid(id.hwnd as HWND).is_ok()).take(128).collect::<Vec<_>>());
    if sources.is_empty(){return;}
    super::utilities::close_panels();
    SOURCES.with(|s|s.replace(sources));OWNER.set(owner as usize);ACTIVE.set(true);READY.store(false,Ordering::Release);
    struct Session;impl Drop for Session{fn drop(&mut self){ACTIVE.set(false);OWNER.set(0);SOURCES.with(|s|s.borrow_mut().clear());refresh();}}
    let session=Session;
    #[cfg(target_env="msvc")]
    let selected=tlauncher_overview_show(valid,&area,f64::from(super::motion::settings().speed)/100.0,!super::motion::allowed(),alt,steps);
    #[cfg(not(target_env="msvc"))]
    let selected=ptr::null_mut();
    let target=valid(selected).then_some(selected);
    drop(session);
    if let Some(target)=target{
        let item=Item{path:String::new(),title:String::new(),hwnd:target,count:1,pinned:false};
        super::activate(owner,&item);
    }
    super::panel_closed(owner);
}

#[cfg(test)]
#[test]
fn quick_switch_matches_original_forward_backward_and_zero_steps(){
    assert_eq!(direct_index(1,3),Some(0));assert_eq!(direct_index(2,3),Some(1));
    assert_eq!(direct_index(-1,3),Some(2));assert_eq!(direct_index(-2,3),Some(1));
    assert_eq!(direct_index(4,3),Some(0));assert_eq!(direct_index(0,3),None);assert_eq!(direct_index(1,0),None);
    let old=super::Options::from_json(&serde_json::json!({"enabled":true}));assert!(!old.alt_tab_overview);
    let mut on=old;on.alt_tab_overview=true;assert!(super::Options::from_json(&on.json()).alt_tab_overview);
}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="original FluidWin Overview DWM links and modal release on hosted Windows; no injected keys"]
fn native_original_overview_selects_cancels_and_releases(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use crate::desktop::wide;
    use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
    thread_local!{static SOURCE:Cell<usize>=const{Cell::new(0)};static MODE:Cell<u32>=const{Cell::new(0)};static LINKS:Cell<u32>=const{Cell::new(0)};}
    unsafe extern "C" fn fixture_valid(h:HWND)->bool{SOURCE.get()==h as usize&&IsWindow(h)!=0}
    unsafe extern "system" fn tick(_:HWND,_:u32,id:usize,_:u32){
        let window=tlauncher_overview_window();if window.is_null(){return;}
        LINKS.set(tlauncher_overview_thumbnails());KillTimer(ptr::null_mut(),id);
        if MODE.get()==0{tlauncher_overview_key(0);}else if MODE.get()==1{tlauncher_overview_key(99);}else{DestroyWindow(SOURCE.get() as HWND);}
    }
    unsafe{
        CoInitializeEx(None,COINIT_APARTMENTTHREADED).unwrap();
        struct Apartment;impl Drop for Apartment{fn drop(&mut self){unsafe{CoUninitialize();}}}let _apartment=Apartment;
        let source=CreateWindowExW(0,wide("STATIC").as_ptr(),wide("Overview source").as_ptr(),WS_OVERLAPPEDWINDOW|WS_VISIBLE,
            80,80,400,300,ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),ptr::null());assert!(!source.is_null());SOURCE.set(source as usize);
        let area=super::monitor_bounds(MonitorFromWindow(source,MONITOR_DEFAULTTONEAREST)).unwrap();
        for mode in 0..3{
            MODE.set(mode);LINKS.set(0);let timer=SetTimer(ptr::null_mut(),0,100,Some(tick));assert_ne!(timer,0);
            let target=tlauncher_overview_show(fixture_valid,&area,0.70,true,true,1);
            KillTimer(ptr::null_mut(),timer);assert_eq!(LINKS.get(),1);
            assert_eq!(target,if mode==0{source}else{ptr::null_mut()});
            assert!(tlauncher_overview_window().is_null());assert_eq!(tlauncher_overview_thumbnails(),0);
        }
        assert_eq!(IsWindow(source),0);
    }
    println!("FLUIDWIN_OVERVIEW selection=PASS cancellation=PASS destroyed_source=PASS links_released=PASS input_injection=NONE");
}
