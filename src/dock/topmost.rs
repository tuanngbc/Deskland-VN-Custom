//! FluidWin primoPiano, with the existing detached Guardian owning rollback.
use super::{WindowIdentity,STATE,CONTROL,window_lease::Lease,control_center::{self,Card,Command,View}};
use std::{cell::{Cell,RefCell},time::{Duration,Instant},sync::atomic::{AtomicBool,AtomicU32,AtomicUsize,Ordering}};
use windows_sys::Win32::{Foundation::*,UI::{WindowsAndMessaging::*,Input::KeyboardAndMouse::{RegisterHotKey,UnregisterHotKey,MOD_CONTROL,MOD_WIN,MOD_NOREPEAT}}};
pub(super) const TIMER:usize=35;
pub(super) const HOTKEY:i32=45;
struct Pin{id:WindowIdentity,lease:Lease,started:Instant,releasing:bool,confirmed:bool}
struct Request{owner:usize,id:WindowIdentity,started:Instant}
thread_local!{
    static PINS:RefCell<Vec<Pin>>=const{RefCell::new(Vec::new())};
    static REQUEST:RefCell<Option<Request>>=const{RefCell::new(None)};
    static BOUND:Cell<bool>=const{Cell::new(false)};
    static BIND_TRIED:Cell<bool>=const{Cell::new(false)};
}
static HOTKEY_ERROR:AtomicU32=AtomicU32::new(0);
static HOTKEY_BOUND:AtomicBool=AtomicBool::new(false);
static CONFIRMED:AtomicUsize=AtomicUsize::new(0);
fn text(vi:&str,en:&str,zh:&str)->String{crate::launcher::localized(vi,en,zh).into()}
pub(super) fn wanted()->bool{REQUEST.with(|r|r.borrow().is_some())||PINS.with(|p|!p.borrow().is_empty())}
pub(super) fn owned(id:WindowIdentity)->bool{PINS.with(|p|p.borrow().iter().any(|p|p.id==id&&p.confirmed&&!p.releasing))}
pub(super) fn diagnostics()->serde_json::Value{serde_json::json!({"hotkeyRegistered":HOTKEY_BOUND.load(Ordering::Acquire),"hotkeyError":HOTKEY_ERROR.load(Ordering::Acquire),"confirmedWindows":CONFIRMED.load(Ordering::Acquire),"permissionEscalation":false})}
unsafe extern "system" fn owned_window(h:HWND,raw:LPARAM)->BOOL{
    let (source,found)=&mut *(raw as *mut (HWND,bool));if GetWindow(h,GW_OWNER)==*source{*found=true;return 0;}1
}
unsafe fn eligible(id:WindowIdentity)->bool{
    let h=id.hwnd as HWND;let mut class=[0u16;128];let n=GetClassNameW(h,class.as_mut_ptr(),128).max(0) as usize;
    let mut owners=(h,false);EnumWindows(Some(owned_window),(&mut owners as *mut (HWND,bool)) as LPARAM);
    id.matches(h)&&IsWindowVisible(h)!=0&&IsIconic(h)==0&&GetAncestor(h,GA_ROOT)==h&&GetWindow(h,GW_OWNER).is_null()
        &&!owners.1&&!super::is_shell_class(&String::from_utf16_lossy(&class[..n]))
        &&GetWindowLongW(h,GWL_EXSTYLE) as u32&(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE)==0
        &&super::window_effect::elevation(id.pid)==Some(false)&&super::previews::permitted_pid(h).is_ok()
}
fn enabled()->bool{STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended})}
pub(super) unsafe fn sync_hotkey(){
    let wanted=enabled()&&STATE.with(|s|s.borrow().prefs.always_on_top)&&!crate::trial::demo_active()&&!crate::trial::benchmark_active();
    if !wanted{if BOUND.replace(false){UnregisterHotKey(CONTROL.get(),HOTKEY);}HOTKEY_BOUND.store(false,Ordering::Release);BIND_TRIED.set(false);HOTKEY_ERROR.store(0,Ordering::Release);return;}
    if BOUND.get()||CONTROL.get().is_null()||BIND_TRIED.replace(true){return;}
    let ok=RegisterHotKey(CONTROL.get(),HOTKEY,MOD_CONTROL|MOD_WIN|MOD_NOREPEAT,0x54)!=0;
    BOUND.set(ok);HOTKEY_BOUND.store(ok,Ordering::Release);let error=if ok{0}else{GetLastError()};HOTKEY_ERROR.store(error,Ordering::Release);
    if !ok{crate::log!("TOPMOST hotkey unavailable error={error}");}
}
pub(super) unsafe fn hotkey(){
    if !BOUND.get()||!enabled()||!STATE.with(|s|s.borrow().prefs.always_on_top){return;}
    let Some(id)=WindowIdentity::read(GetForegroundWindow())else{return;};
    let owner=STATE.with(|s|s.borrow().bars.iter().find(|b|!b.hidden).map(|b|b.hwnd));
    if let Some(owner)=owner{toggle(owner,id);}
}
fn report(owner:HWND,message:String){unsafe{
    control_center::show(owner,super::dock_app_anchor(owner,None),View{title:text("Luôn ở trên cùng","Always on top","窗口置顶"),hint:String::new(),metrics:None,
        cards:vec![Card{icon:None,title:message,hint:String::new(),action:None}]});
}}
pub(super) unsafe fn choose(owner:HWND){
    let cards=STATE.with(|s|s.borrow().windows.iter().filter_map(|w|{
        let id=WindowIdentity::read(w.hwnd)?;if !owned(id)&&(!eligible(id)||GetWindowLongW(w.hwnd,GWL_EXSTYLE) as u32&WS_EX_TOPMOST!=0){return None;}
        Some(Card{icon:None,title:w.title.clone(),hint:if owned(id){text("Đang ghim · chọn để bỏ ghim","Pinned · choose to unpin","已置顶 · 选择以取消")}else{String::new()},action:Some(Command::Topmost(Some(id)))})
    }).take(48).collect::<Vec<_>>());
    let hint=if cards.is_empty(){text("Không có cửa sổ phù hợp","No eligible windows","没有可用窗口")}else{text("Chọn cửa sổ để ghim hoặc bỏ ghim","Choose a window to pin or unpin","选择窗口以置顶或取消置顶")};
    super::utilities::close_panels();control_center::show_list(owner,super::dock_app_anchor(owner,None),View{title:text("Luôn ở trên cùng","Always on top","窗口置顶"),hint,cards,metrics:None});
}
pub(super) unsafe fn toggle(owner:HWND,id:WindowIdentity){
    if !enabled()||!id.matches(id.hwnd as HWND){return;}
    let released=PINS.with(|pins|{let mut pins=pins.borrow_mut();let Some(pin)=pins.iter_mut().find(|p|p.id==id)else{return false;};pin.lease.restore();pin.releasing=true;pin.started=Instant::now();true});
    if released{SetTimer(CONTROL.get(),TIMER,80,None);return;}
    if REQUEST.with(|r|r.borrow().is_some())||PINS.with(|p|p.borrow().len()>=32){return;}
    if !eligible(id)||GetWindowLongW(id.hwnd as HWND,GWL_EXSTYLE) as u32&WS_EX_TOPMOST!=0{
        report(owner,text("Giữ nguyên trạng thái của cửa sổ này","This window's existing state is preserved","保留此窗口现有状态"));return;
    }
    super::utilities::close_panels();
    super::window_geometry::cancel();super::window_motion::cancel();super::window_effect::cancel();
    REQUEST.with(|r|r.replace(Some(Request{owner:owner as usize,id,started:Instant::now()})));
    if SetTimer(CONTROL.get(),TIMER,80,None)==0{REQUEST.with(|r|r.borrow_mut().take());return;}
    super::motion_guard::sync();poll();
}
pub(super) unsafe fn poll(){
    if !enabled(){reset();return;}
    let request=REQUEST.with(|r|r.borrow_mut().take());
    if let Some(request)=request{
        let h=request.id.hwnd as HWND;
        if !eligible(request.id)||GetWindowLongW(h,GWL_EXSTYLE) as u32&WS_EX_TOPMOST!=0||request.started.elapsed()>Duration::from_secs(8){
            report(request.owner as HWND,text("Chưa ghim được cửa sổ","Could not pin this window","无法置顶此窗口"));
        }else if super::motion_guard::session().is_none(){REQUEST.with(|r|r.replace(Some(request)));}
        else if let Some(lease)=Lease::begin_topmost(h){PINS.with(|p|p.borrow_mut().push(Pin{id:request.id,lease,started:Instant::now(),releasing:false,confirmed:false}));}
        else{report(request.owner as HWND,text("Chưa ghim được cửa sổ","Could not pin this window","无法置顶此窗口"));}
    }
    let mut pending=false;
    PINS.with(|p|p.borrow_mut().retain_mut(|pin|{
        let status=pin.lease.status();let h=pin.id.hwnd as HWND;
        if !pin.id.matches(h)||matches!(status,-1|2){return false;}
        if !pin.releasing&&status==1&&GetWindowLongW(h,GWL_EXSTYLE) as u32&WS_EX_TOPMOST==0{return false;}
        pin.confirmed=status==1;
        if status==0||pin.releasing{
            if pin.started.elapsed()>Duration::from_secs(3){pin.lease.restore();return false;}
            pending=true;
        }
        true
    }));
    CONFIRMED.store(PINS.with(|p|p.borrow().iter().filter(|p|p.confirmed&&!p.releasing).count()),Ordering::Release);
    if !pending&&REQUEST.with(|r|r.borrow().is_none()){KillTimer(CONTROL.get(),TIMER);}
}
pub(super) unsafe fn reset(){
    REQUEST.with(|r|r.borrow_mut().take());
    let pins=PINS.with(|p|std::mem::take(&mut *p.borrow_mut()));drop(pins);KillTimer(CONTROL.get(),TIMER);
    CONFIRMED.store(0,Ordering::Release);
}
pub(super) unsafe fn shutdown(){reset();if BOUND.replace(false){UnregisterHotKey(CONTROL.get(),HOTKEY);}HOTKEY_BOUND.store(false,Ordering::Release);BIND_TRIED.set(false);}

#[cfg(all(test,target_env="msvc"))]
pub(super) unsafe fn probe(owner:HWND,id:WindowIdentity){
    use std::{mem,ptr};
    let h=id.hwnd as HWND;assert!(eligible(id));
    let original=GetWindowLongPtrW(h,GWL_EXSTYLE);let mut rect:RECT=mem::zeroed();assert_ne!(GetWindowRect(h,&mut rect),0);
    let settings=super::motion::settings();let (enabled,shortcut)=STATE.with(|s|{let mut s=s.borrow_mut();let old=(s.prefs.enabled,s.prefs.always_on_top);s.prefs.enabled=true;s.prefs.always_on_top=true;old});
    struct Reset{enabled:bool,shortcut:bool,settings:super::motion::Settings}
    impl Drop for Reset{fn drop(&mut self){unsafe{shutdown();STATE.with(|s|{let mut s=s.borrow_mut();s.prefs.enabled=self.enabled;s.prefs.always_on_top=self.shortcut;});super::motion::configure(self.settings);}}}
    let _reset=Reset{enabled,shortcut,settings};
    super::motion::configure(super::motion::Settings{enabled:false,..settings});
    let (_,marker)=super::motion_guard::session().expect("existing fixture Guardian ready");let key=crate::desktop::wide(&(marker+"_Topmost"));
    let pump_until=|condition:&dyn Fn()->bool|{let start=Instant::now();loop{
        super::motion_guard::pulse_fixture();super::TASKBAR_WATCH.with(|w|if let Some(w)=w.borrow_mut().as_mut(){w.poll().unwrap();});
        let mut msg:MSG=mem::zeroed();for _ in 0..64{if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&msg);DispatchMessageW(&msg);}poll();
        if condition(){break;}assert!(start.elapsed()<Duration::from_secs(6));std::thread::sleep(Duration::from_millis(10));
    }};
    // Actual native hotkey registration, then the real receive handler. No
    // physical-key/display-quality claim is made from direct dispatch.
    sync_hotkey();assert!(BOUND.get(),"runner Ctrl+Win+T registration: {}",HOTKEY_ERROR.load(Ordering::Acquire));
    assert_ne!(SetForegroundWindow(h),0);pump_until(&||GetForegroundWindow()==h);
    hotkey();pump_until(&||owned(id));assert_ne!(GetWindowLongPtrW(h,GWL_EXSTYLE)&WS_EX_TOPMOST as isize,0);
    super::perform_control(owner,Command::Topmost(Some(id)));pump_until(&||!wanted()&&GetPropW(h,key.as_ptr()).is_null());
    assert_eq!(GetWindowLongPtrW(h,GWL_EXSTYLE),original);
    super::perform_control(owner,Command::Topmost(Some(id)));pump_until(&||owned(id));
    reset();pump_until(&||GetPropW(h,key.as_ptr()).is_null());assert_eq!(GetWindowLongPtrW(h,GWL_EXSTYLE),original);
    let mut after:RECT=mem::zeroed();GetWindowRect(h,&mut after);assert!(super::same_rect(rect,after));
    STATE.with(|s|s.borrow_mut().prefs.always_on_top=false);sync_hotkey();assert!(!BOUND.get());
    println!("FLUIDWIN_TOPMOST native_hotkey_registration=PASS receive_handler=PASS control_command=PASS motion_disabled=SUPPORTED reset=RESTORED geometry=PRESERVED journal=CLEARED physical_keys=NOT_VERIFIED");
}
