//! Original FluidWin layout picker; only the shared controller moves a source.
use super::{WindowIdentity,STATE,CONTROL,control_center::{self,Card,Command,View}};
use std::{cell::RefCell,ffi::c_void,mem,ptr::{self,NonNull}};
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::*,UI::{HiDpi::GetDpiForWindow,WindowsAndMessaging::*}};
pub(super) const TIMER:usize=33;
#[cfg(target_env="msvc")]
extern "C"{
    fn tlauncher_snap_open(source:HWND,anchor:POINT,title:*const u16,labels:*const *const u16,count:u32,speed:f64,reduced:bool)->*mut c_void;
    fn tlauncher_snap_poll(raw:*mut c_void,selected:*mut RECT)->i32;
    fn tlauncher_snap_window(raw:*mut c_void)->HWND;
    fn tlauncher_snap_free(raw:*mut c_void);
    #[cfg(test)]fn tlauncher_snap_key(raw:*mut c_void,key:u32);
}
struct Native(NonNull<c_void>);
impl Drop for Native{fn drop(&mut self){#[cfg(target_env="msvc")]unsafe{tlauncher_snap_free(self.0.as_ptr());}}}
struct Panel{native:Native,owner:usize,id:WindowIdentity,monitor:usize,dpi:u32,work:RECT,motion:super::motion::Settings}
thread_local!{static OPEN:RefCell<Option<Panel>>=const{RefCell::new(None)};}
fn text(vi:&str,en:&str,zh:&str)->String{crate::launcher::localized(vi,en,zh).into()}
fn title()->String{text("Bố cục cửa sổ","Snap layouts","窗口布局")}
unsafe fn eligible(id:WindowIdentity)->bool{
    let h=id.hwnd as HWND;
    id.matches(h)&&IsWindowVisible(h)!=0&&IsIconic(h)==0&&GetWindowLongW(h,GWL_STYLE) as u32&WS_THICKFRAME!=0
        &&super::previews::permitted_pid(h).is_ok()&&super::window_effect::elevation(id.pid)==Some(false)
}
unsafe fn work(m:HMONITOR)->Option<RECT>{let mut info:MONITORINFO=mem::zeroed();info.cbSize=mem::size_of::<MONITORINFO>() as u32;(GetMonitorInfoW(m,&mut info)!=0).then_some(info.rcWork)}
pub(super) fn open_for(owner:HWND)->bool{OPEN.with(|p|p.borrow().as_ref().is_some_and(|p|p.owner==owner as usize))}
pub(super) unsafe fn close_for(owner:HWND){
    let old=OPEN.with(|s|{let mut s=s.borrow_mut();if s.as_ref().is_some_and(|p|owner.is_null()||p.owner==owner as usize){s.take()}else{None}});
    if let Some(old)=old{let owner=old.owner as HWND;drop(old);KillTimer(CONTROL.get(),TIMER);super::panel_closed(owner);}
}
pub(super) unsafe fn choose_window(owner:HWND){
    let cards=STATE.with(|s|s.borrow().windows.iter().filter_map(|w|{
        let id=WindowIdentity::read(w.hwnd)?;
        eligible(id).then(||Card{icon:None,title:w.title.clone(),hint:String::new(),action:Some(Command::SnapWindow(id))})
    }).collect::<Vec<_>>());
    super::utilities::close_panels();
    control_center::show_list(owner,super::dock_app_anchor(owner,None),View{title:title(),
        hint:if cards.is_empty(){text("Không có cửa sổ có thể đổi kích thước","No resizable windows","没有可调整大小的窗口")}else{text("Chọn cửa sổ","Choose a window","选择窗口")},cards,metrics:None});
}
fn labels()->Vec<Vec<u16>>{
    [("Nửa trái","Left half","左半屏"),("Nửa phải","Right half","右半屏"),
        ("Hai phần ba bên trái","Left two thirds","左侧三分之二"),("Một phần ba bên phải","Right third","右侧三分之一"),
        ("Cột trái","Left column","左列"),("Cột giữa","Center column","中列"),("Cột phải","Right column","右列"),
        ("Góc trên trái","Top left quarter","左上角"),("Góc trên phải","Top right quarter","右上角"),
        ("Góc dưới trái","Bottom left quarter","左下角"),("Góc dưới phải","Bottom right quarter","右下角")]
        .into_iter().map(|(v,e,z)|crate::desktop::wide(&text(v,e,z))).collect()
}
pub(super) unsafe fn open(owner:HWND,id:WindowIdentity)->bool{
    if IsWindow(owner)==0||!eligible(id)||!STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended}){return false;}
    let h=id.hwnd as HWND;let monitor=MonitorFromWindow(h,MONITOR_DEFAULTTONULL);let Some(work)=work(monitor)else{return false;};
    let Some(area)=super::window_effect::visible_area(h)else{return false;};
    super::utilities::close_panels();super::show_desktop::pause();super::window_geometry::cancel();super::window_motion::cancel();super::window_effect::cancel();
    let motion=super::motion::settings();
    #[cfg(target_env="msvc")]
    let raw={let labels=labels();let pointers:Vec<_>=labels.iter().map(|s|s.as_ptr()).collect();
        tlauncher_snap_open(h,POINT{x:area.left+(area.right-area.left)/2,y:area.top+32},crate::desktop::wide(&title()).as_ptr(),pointers.as_ptr(),pointers.len() as u32,f64::from(motion.speed)/100.0,!super::motion::allowed())};
    #[cfg(not(target_env="msvc"))]let raw=ptr::null_mut();
    let Some(native)=NonNull::new(raw).map(Native)else{return false;};
    if SetTimer(CONTROL.get(),TIMER,80,None)==0{drop(native);return false;}
    OPEN.with(|s|s.replace(Some(Panel{native,owner:owner as usize,id,monitor:monitor as usize,dpi:GetDpiForWindow(h),work,motion})));true
}
pub(super) unsafe fn poll(){
    let Some(panel)=OPEN.with(|s|s.borrow_mut().take())else{KillTimer(CONTROL.get(),TIMER);return;};
    let h=panel.id.hwnd as HWND;
    let valid=IsWindow(panel.owner as HWND)!=0&&eligible(panel.id)&&GetDpiForWindow(h)==panel.dpi
        &&MonitorFromWindow(h,MONITOR_DEFAULTTONULL) as usize==panel.monitor&&work(panel.monitor as HMONITOR).is_some_and(|r|super::same_rect(r,panel.work))
        &&panel.motion==super::motion::settings()&&STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended});
    let mut rect:RECT=mem::zeroed();
    #[cfg(target_env="msvc")]
    let (status,focused)={let focused=GetForegroundWindow()==tlauncher_snap_window(panel.native.0.as_ptr());
        (if valid{tlauncher_snap_poll(panel.native.0.as_ptr(),&mut rect)}else{-1},focused)};
    #[cfg(not(target_env="msvc"))]let (status,focused)=(-1,false);
    if status==0{OPEN.with(|s|s.replace(Some(panel)));return;}
    let owner=panel.owner as HWND;let id=panel.id;let inside=rect.left>=panel.work.left&&rect.top>=panel.work.top&&rect.right<=panel.work.right&&rect.bottom<=panel.work.bottom&&rect.right>rect.left&&rect.bottom>rect.top;
    drop(panel);KillTimer(CONTROL.get(),TIMER);super::panel_closed(owner);
    if valid&&status==1&&inside{SetForegroundWindow(h);super::window_geometry::command(id,super::window_geometry::Action::Snap(rect));}
    else if valid&&focused{SetForegroundWindow(h);}
}

#[cfg(all(test,target_env="msvc"))]
pub(super) unsafe fn probe(owner:HWND,id:WindowIdentity){
    use std::time::{Duration,Instant};
    use windows_sys::Win32::{Graphics::Dwm::DwmFlush,UI::Input::KeyboardAndMouse::{VK_TAB,VK_RETURN,VK_ESCAPE}};
    let h=id.hwnd as HWND;let mut original:RECT=mem::zeroed();assert_ne!(GetWindowRect(h,&mut original),0);
    let style=GetWindowLongPtrW(h,GWL_EXSTYLE);let policy=super::motion::settings();
    let enabled=STATE.with(|s|{let mut s=s.borrow_mut();let old=s.prefs.enabled;s.prefs.enabled=true;old});
    struct Reset{h:HWND,rect:RECT,policy:super::motion::Settings,enabled:bool}
    impl Drop for Reset{fn drop(&mut self){unsafe{close_for(ptr::null_mut());super::window_geometry::cancel();super::motion::configure(self.policy);STATE.with(|s|s.borrow_mut().prefs.enabled=self.enabled);
        ShowWindowAsync(self.h,SW_RESTORE);SetWindowPos(self.h,ptr::null_mut(),self.rect.left,self.rect.top,self.rect.right-self.rect.left,self.rect.bottom-self.rect.top,SWP_NOZORDER|SWP_NOACTIVATE|SWP_ASYNCWINDOWPOS);}}}
    let reset=Reset{h,rect:original,policy,enabled};let marker=crate::desktop::wide(&super::motion_guard::session().unwrap().1);
    let pump=||{super::motion_guard::pulse_fixture();super::TASKBAR_WATCH.with(|w|if let Some(w)=w.borrow_mut().as_mut(){w.poll().unwrap();});
        let mut msg:MSG=mem::zeroed();for _ in 0..64{if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&msg);DispatchMessageW(&msg);}
        poll();super::window_geometry::tick();std::thread::sleep(Duration::from_millis(12));};
    let wait=|condition:&dyn Fn()->bool|{let start=Instant::now();while !condition(){pump();assert!(start.elapsed()<Duration::from_secs(8),"Snap layout state timeout");}};
    let key=|key:u16|OPEN.with(|s|tlauncher_snap_key(s.borrow().as_ref().unwrap().native.0.as_ptr(),u32::from(key)));
    let panel=||OPEN.with(|s|s.borrow().as_ref().map_or(ptr::null_mut(),|p|tlauncher_snap_window(p.native.0.as_ptr())));
    let prepare=||{ShowWindowAsync(h,SW_RESTORE);wait(&||IsZoomed(h)==0&&IsIconic(h)==0);
        SetWindowPos(h,ptr::null_mut(),original.left,original.top,original.right-original.left,original.bottom-original.top,SWP_NOACTIVATE|SWP_NOZORDER|SWP_ASYNCWINDOWPOS);
        wait(&||{let mut r:RECT=mem::zeroed();GetWindowRect(h,&mut r)!=0&&super::same_rect(r,original)});super::window_effect::fixture_settle_source(h);};
    let area=work(MonitorFromWindow(h,MONITOR_DEFAULTTONULL)).unwrap();
    let fractions=[(0,0,500,1000),(500,0,1000,1000),(0,0,667,1000),(667,0,1000,1000),(0,0,333,1000),(333,0,667,1000),(667,0,1000,1000),(0,0,500,500),(500,0,1000,500),(0,500,500,1000),(500,500,1000,1000)];
    let mut animated=0;
    for (index,(l,t,r,b)) in fractions.into_iter().enumerate(){
        prepare();assert!(open(owner,id));let window=panel();assert_ne!(IsWindowVisible(window),0);
        if index==0{
            let start=Instant::now();while start.elapsed()<Duration::from_millis(550){pump();}assert!(open_for(owner));
            DwmFlush();let mut bounds:RECT=mem::zeroed();assert_ne!(GetWindowRect(window,&mut bounds),0);
            let (w,height)=(bounds.right-bounds.left,bounds.bottom-bounds.top);let mut image=super::glass_probe::Layer::new(w,height).unwrap();let dc=GetDC(ptr::null_mut());
            assert_ne!(BitBlt(image.dc(),0,0,w,height,dc,bounds.left,bounds.top,SRCCOPY|CAPTUREBLT),0);ReleaseDC(ptr::null_mut(),dc);GdiFlush();
            let rgba=image.pixels().chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],255]).collect::<Vec<_>>();
            assert!(rgba.chunks_exact(4).any(|p|p[..3]!=rgba[..3]),"actual Snap screen is flat");
            if let Some(dir)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){image::save_buffer(std::path::PathBuf::from(dir).join("snap-layout-panel.png"),&rgba,w as u32,height as u32,image::ColorType::Rgba8).unwrap();}
        }
        let mut unchanged:RECT=mem::zeroed();assert_ne!(GetWindowRect(h,&mut unchanged),0);assert!(super::same_rect(unchanged,original),"picker must not move source before selection");
        key(b'1'.into());for _ in 0..index{key(VK_TAB);}key(VK_RETURN);
        wait(&||!open_for(owner)&&!super::window_geometry::active());assert_eq!(IsWindow(window),0);
        let expected=RECT{left:area.left+(area.right-area.left)*l/1000,top:area.top+(area.bottom-area.top)*t/1000,right:area.left+(area.right-area.left)*r/1000,bottom:area.top+(area.bottom-area.top)*b/1000};
        wait(&||super::window_effect::visible_area(h).is_some_and(|r|[r.left-expected.left,r.top-expected.top,r.right-expected.right,r.bottom-expected.bottom].iter().all(|v|v.abs()<=6)));
        wait(&||GetWindowLongPtrW(h,GWL_EXSTYLE)==style&&GetPropW(h,marker.as_ptr()).is_null());
        if super::window_geometry::diagnostics()["actualEffect"]=="fluidwin-geometry"{animated+=1;}
    }
    assert!(animated>0,"picker must reach shared prepared geometry");
    for reason in ["escape","owner-close","monitor-invalidated","focus-changed"]{
        prepare();assert!(open(owner,id));let window=panel();
        match reason{"escape"=>key(VK_ESCAPE),"owner-close"=>close_for(owner),"monitor-invalidated"=>OPEN.with(|s|s.borrow_mut().as_mut().unwrap().monitor=0),_=>{ShowWindow(owner,SW_SHOW);SetForegroundWindow(owner);}}
        wait(&||!open_for(owner));assert_eq!(IsWindow(window),0);assert!(!super::window_geometry::active());
        let mut r:RECT=mem::zeroed();assert_ne!(GetWindowRect(h,&mut r),0);assert!(super::same_rect(r,original));
    }
    // Native/reduced motion still commits a maximized source to the selected
    // visible region, after native restore has actually settled.
    prepare();super::motion::configure(super::motion::Settings{enabled:false,..policy});
    ShowWindowAsync(h,SW_MAXIMIZE);wait(&||IsZoomed(h)!=0);super::window_effect::fixture_settle_source(h);
    assert!(open(owner,id));key(b'1'.into());key(VK_RETURN);wait(&||!open_for(owner)&&!super::window_geometry::active());
    assert_eq!(IsZoomed(h),0);wait(&||super::window_effect::visible_area(h).is_some_and(|actual|
        [actual.left-area.left,actual.top-area.top,actual.right-(area.left+(area.right-area.left)/2),actual.bottom-area.bottom].iter().all(|v|v.abs()<=6)));
    assert_eq!(GetWindowLongPtrW(h,GWL_EXSTYLE),style);assert!(GetPropW(h,marker.as_ptr()).is_null());
    drop(reset);wait(&||{let mut r:RECT=mem::zeroed();GetWindowRect(h,&mut r)!=0&&super::same_rect(r,original)});super::window_effect::fixture_settle_source(h);
    println!("FLUIDWIN_SNAP_LAYOUT_FIXTURE regions=11 keyboard=PASS no_premature_move=PASS screen=CAPTURED prepared={animated} cancel=PASS native_maximized=PASS source_style=RESTORED journal=CLEARED");
}
