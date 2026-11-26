//! Exact FluidWin Desktop/Explorer menu implementation behind the shared hook.
use std::{sync::Mutex, time::Instant};
use windows_sys::Win32::{Foundation::*, UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*}};

pub(super) const NOTICE:u32=WM_APP+106;
#[cfg(target_env="msvc")]
extern "C" {
    fn tlauncher_fluidwin_menu_policy(language:i32,speed:f64,reduced:bool);
    #[cfg(test)]fn tlauncher_fluidwin_menu_policy_fixture(language:i32,speed:f64,reduced:bool)->bool;
    fn tlauncher_fluidwin_shell_hit(x:i32,y:i32)->i32;
    fn tlauncher_fluidwin_shell_menu(owner:HWND,x:i32,y:i32)->i32;
    fn tlauncher_fluidwin_shell_menu_close();
    fn tlauncher_fluidwin_shell_menu_shutdown();
    #[cfg(test)] fn tlauncher_fluidwin_shell_menu_fixture(owner:HWND,language:i32)->u32;
    #[cfg(test)] fn tlauncher_fluidwin_owner_draw_fixture(owner:HWND)->u32;
    #[cfg(test)]fn tlauncher_fluidwin_menu_accessible(hwnd:HWND)->*mut std::ffi::c_void;
    #[cfg(test)]fn tlauncher_fluidwin_menu_access_probe(raw:*mut std::ffi::c_void,disconnected:bool)->i32;
    #[cfg(test)]fn tlauncher_fluidwin_menu_access_free(raw:*mut std::ffi::c_void);
}
pub(super) fn configure_menu(){
    #[cfg(target_env="msvc")]unsafe{
        let language=match crate::launcher::localized("vi","en","zh"){"vi"=>0,"zh"=>2,_=>1};
        tlauncher_fluidwin_menu_policy(language,super::motion::settings().speed.clamp(25,200) as f64/100.0,!super::popup_motion_allowed());
    }
}

#[derive(Clone,Copy)]
pub(super) struct Click { hwnd:usize,pid:u32,tid:u32,point:POINT,at:Instant }
// At most one queued request; no heap pointers in cross-thread window messages.
static PENDING:Mutex<Option<(usize,Click)>>=Mutex::new(None);
thread_local! { static BUSY:std::cell::Cell<bool>=const{std::cell::Cell::new(false)}; }

impl Click {
    pub(super) unsafe fn begin(point:POINT)->Option<Self>{
        #[cfg(not(target_env="msvc"))] { let _=point; return None; }
        #[cfg(target_env="msvc")]
        {
            if GetAsyncKeyState(VK_SHIFT as i32)<0 || tlauncher_fluidwin_shell_hit(point.x,point.y)==0 {return None;}
            // WM_CONTEXTMENU uses signed 16-bit screen coordinates for fallback.
            if i16::try_from(point.x).is_err()||i16::try_from(point.y).is_err(){return None;}
            let hwnd=WindowFromPoint(point); let mut pid=0;
            let tid=GetWindowThreadProcessId(hwnd,&mut pid);
            (pid!=0&&tid!=0).then_some(Self{hwnd:hwnd as usize,pid,tid,point,at:Instant::now()})
        }
    }
    unsafe fn same_source(self)->bool{
        let mut pid=0;
        let hwnd=self.hwnd as HWND;
        let tid=GetWindowThreadProcessId(hwnd,&mut pid);
        tid==self.tid&&pid==self.pid&&WindowFromPoint(self.point)==hwnd
    }
    pub(super) unsafe fn queue(self,point:POINT,target:HWND,ticket:usize)->bool{
        if self.at.elapsed().as_secs_f32()>2.0 || GetAsyncKeyState(VK_SHIFT as i32)<0
            || (point.x-self.point.x).abs()>GetSystemMetrics(SM_CXDRAG).max(2)
            || (point.y-self.point.y).abs()>GetSystemMetrics(SM_CYDRAG).max(2)
            || !self.same_source(){return false;}
        let mut pending=PENDING.lock().unwrap_or_else(|e|e.into_inner());
        if pending.is_some(){return false;}
        *pending=Some((ticket,self));
        if PostMessageW(target,NOTICE,ticket,0)==0{*pending=None;return false;}
        true
    }
    unsafe fn native(self){
        if !self.same_source(){return;}
        let position=(u32::from(self.point.x as u16)|(u32::from(self.point.y as u16)<<16)) as LPARAM;
        PostMessageW(self.hwnd as HWND,WM_CONTEXTMENU,self.hwnd,position);
    }
}

pub(super) unsafe fn receive(owner:HWND,ticket:usize,enabled:bool){
    let click={
        let mut pending=PENDING.lock().unwrap_or_else(|e|e.into_inner());
        if !pending.as_ref().is_some_and(|(generation,_)|*generation==ticket){return;}
        pending.take().unwrap().1
    };
    if !click.same_source(){return;}
    if !enabled || click.at.elapsed().as_secs_f32()>2.0 || BUSY.with(|b|b.replace(true)) {
        click.native();return;
    }
    struct Scope;
    impl Drop for Scope{fn drop(&mut self){BUSY.with(|b|b.set(false));}}
    let _scope=Scope;
    configure_menu();
    #[cfg(target_env="msvc")]
    if tlauncher_fluidwin_shell_menu(owner,click.point.x,click.point.y)!=0{return;}
    click.native();
}
pub(super) unsafe fn close(){
    #[cfg(target_env="msvc")] tlauncher_fluidwin_shell_menu_close();
}
pub(super) unsafe fn shutdown(){
    close();
    if let Some((_,click))=PENDING.lock().unwrap_or_else(|e|e.into_inner()).take(){click.native();}
    #[cfg(target_env="msvc")] tlauncher_fluidwin_shell_menu_shutdown();
}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="actual FluidWin menu keyboard/submenu selection on hosted Windows"]
fn native_imported_menu_selects_nested_shell_command(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    unsafe{
        use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
        CoInitializeEx(None,COINIT_APARTMENTTHREADED).ok().unwrap();
        let owner=CreateWindowExW(WS_EX_TOOLWINDOW,crate::desktop::wide("STATIC").as_ptr(),std::ptr::null(),WS_OVERLAPPEDWINDOW|WS_VISIBLE,20,20,300,200,std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null());
        assert!(!owner.is_null());
        let tid=windows_sys::Win32::System::Threading::GetCurrentThreadId();
        for language in 0..3{
        for speed in [0.25,0.5,0.7,0.75,1.0,1.5,2.0]{for reduced in [false,true]{assert!(tlauncher_fluidwin_menu_policy_fixture(language,speed,reduced));}}
        tlauncher_fluidwin_menu_policy(language,0.7,language==2);
        let helper=std::thread::spawn(move||{
            // Send only to this fixture thread's message queue, never global input.
            CoInitializeEx(None,COINIT_APARTMENTTHREADED).ok().unwrap();
            struct Com;impl Drop for Com{fn drop(&mut self){unsafe{CoUninitialize();}}}let _com=Com;
            std::thread::sleep(std::time::Duration::from_millis(300));
            let hwnd=FindWindowW(crate::desktop::wide("FluidWin.Menu").as_ptr(),std::ptr::null());
            assert_eq!(GetWindowThreadProcessId(hwnd,std::ptr::null_mut()),tid);
            let accessible=tlauncher_fluidwin_menu_accessible(hwnd);assert!(!accessible.is_null());
            assert_eq!(tlauncher_fluidwin_menu_access_probe(accessible,false),0);
            if let Some(root)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){
                use windows_sys::Win32::Graphics::{Dwm::DwmFlush,Gdi::*};
                let hwnd=FindWindowW(crate::desktop::wide("FluidWin.Menu").as_ptr(),std::ptr::null());
                assert_eq!(GetWindowThreadProcessId(hwnd,std::ptr::null_mut()),tid);
                DwmFlush();let mut rect:RECT=std::mem::zeroed();assert_ne!(GetWindowRect(hwnd,&mut rect),0);
                let (w,h)=(rect.right-rect.left,rect.bottom-rect.top);let mut layer=super::glass_probe::Layer::new(w,h).unwrap();
                let dc=GetDC(std::ptr::null_mut());assert_ne!(BitBlt(layer.dc(),0,0,w,h,dc,rect.left,rect.top,SRCCOPY|CAPTUREBLT),0);ReleaseDC(std::ptr::null_mut(),dc);GdiFlush();
                let rgba=layer.pixels().chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],255]).collect::<Vec<_>>();
                image::save_buffer(std::path::PathBuf::from(root).join(format!("fluidwin-shell-menu-{language}.png")),&rgba,w as u32,h as u32,image::ColorType::Rgba8).unwrap();
            }
            for key in [VK_RIGHT,VK_DOWN,VK_RETURN]{
                std::thread::sleep(std::time::Duration::from_millis(220));
                unsafe{PostThreadMessageW(tid,WM_KEYDOWN,key as usize,0);}
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
            unsafe{if !FindWindowW(crate::desktop::wide("FluidWin.Menu").as_ptr(),std::ptr::null()).is_null(){
                PostThreadMessageW(tid,WM_KEYDOWN,VK_ESCAPE as usize,0);PostThreadMessageW(tid,WM_KEYDOWN,VK_ESCAPE as usize,0);
            }}
            assert_eq!(tlauncher_fluidwin_menu_access_probe(accessible,true),0,"retained provider must disconnect after menu destruction");
            tlauncher_fluidwin_menu_access_free(accessible);
        });
        let selected=tlauncher_fluidwin_shell_menu_fixture(owner,language);
        helper.join().unwrap();
        assert_eq!(selected,73,"imported menu skips disabled row, opens submenu and dispatches its real command ID");
        }
        let native_messages=tlauncher_fluidwin_owner_draw_fixture(owner);
        DestroyWindow(owner);shutdown();CoUninitialize();
        assert_eq!(native_messages,3,"native extension fallback must route measurement and drawing messages");
        println!("FLUIDWIN_SHELL_MENU nested_keyboard=PASS locales=vi,en,zh owner_speed=0.70 reduced_motion=PASS msaa_names_roles_bounds=PASS disabled_action=REFUSED accessible_select=PASS disconnected_provider=PASS owner_shell_commands=NOT_VERIFIED");
    }
}
