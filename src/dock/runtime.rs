//! One managed UI thread in the Deskland process, no companion EXE/config owner.
use super::Options;
use std::{sync::mpsc::{self, Receiver, Sender}, thread::JoinHandle};

pub(super) enum Command {
    Configure(Options),
    Suspend(bool),
    Stop,
    Focus,
    Taskbar(bool),
    #[cfg(test)] InspectSettings { reply:Sender<Vec<(String,usize,bool,bool)>> },
    #[cfg(test)] InspectStageSettings { reply:Sender<Vec<(String,usize,bool)>> },
    #[cfg(test)] Inspect { reply:Sender<Vec<usize>>, paint:bool },
}
#[derive(Debug)]
pub enum Event { Edit(super::Edit), Error(String), Utility(super::HostAction), WinKey(String), Taskbar(super::TaskbarStatus) }

pub struct Runtime {
    control: usize,
    commands: Sender<Command>,
    events: Receiver<Event>,
    thread: Option<JoinHandle<()>>,
}
impl Runtime {
    pub fn new(options: Options, owner:usize) -> Result<Self,String> {
        let (commands,receiver)=mpsc::channel();let (events_tx,events)=mpsc::channel();
        let (ready_tx,ready)=mpsc::sync_channel(1);
        let thread=std::thread::Builder::new().name("Deskland Dock & Stage".into()).spawn(move || {
            super::window_lease::recover_orphans_once();
            super::run(options,owner,receiver,events_tx,ready_tx);
        }).map_err(|e|e.to_string())?;
        match ready.recv() {
            Ok(Ok(control))=>Ok(Self{control,commands,events,thread:Some(thread)}),
            result=>{let _=thread.join();Err(match result {Ok(Err(error))=>error,_=>"Dock initialization ended without readiness".into()})}
        }
    }
    fn send(&self,command:Command) {
        if self.commands.send(command).is_ok() {
            unsafe {windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(self.control as _,super::WM_CONFIG,0,0);}
        }
    }
    pub fn configure(&self,options:Options) {self.send(Command::Configure(options));}
    pub fn set_taskbar_hidden(&self,hidden:bool){self.send(Command::Taskbar(hidden));}
    pub fn focus(&self){self.send(Command::Focus);}
    pub fn suspend(&self,on:bool) {self.send(Command::Suspend(on));}
    pub fn take_events(&self)->Vec<Event> {self.events.try_iter().collect()}
    pub fn finished(&self)->bool {self.thread.as_ref().is_none_or(JoinHandle::is_finished)}
    #[cfg(test)]
    fn inspect(&self)->Vec<usize> {
        self.inspect_with_paint(false)
    }
    #[cfg(test)]
    fn inspect_with_paint(&self,paint:bool)->Vec<usize> {
        let (tx,rx)=mpsc::channel();self.send(Command::Inspect{reply:tx,paint});
        rx.recv_timeout(std::time::Duration::from_secs(5)).expect("Dock UI must answer")
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.send(Command::Stop);
        if let Some(thread)=self.thread.take() {let _=thread.join();}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId,IsWindow,IsWindowVisible};
    #[test]
    #[ignore = "real Settings-owner HWND regression; explicitly run in CI"]
    fn native_startup_never_synchronously_reads_own_settings_title() {
        use windows_sys::Win32::{Foundation::*, UI::WindowsAndMessaging::*, System::LibraryLoader::GetModuleHandleW};
        use std::sync::atomic::{AtomicUsize,Ordering};
        static REQUESTS:AtomicUsize=AtomicUsize::new(0);
        unsafe extern "system" fn proc(h:HWND,m:u32,w:WPARAM,l:LPARAM)->LRESULT {
            if m==WM_GETTEXT {REQUESTS.fetch_add(1,Ordering::Relaxed);}
            DefWindowProcW(h,m,w,l)
        }
        unsafe {
            let name=crate::desktop::wide("DesklandOwnSettingsRegression");
            let mut wc:WNDCLASSW=std::mem::zeroed();wc.lpfnWndProc=Some(proc);
            wc.hInstance=GetModuleHandleW(std::ptr::null());wc.lpszClassName=name.as_ptr();
            RegisterClassW(&wc);
            let hwnd=CreateWindowExW(0,name.as_ptr(),crate::desktop::wide("Own Settings title must be excluded").as_ptr(),
                WS_OVERLAPPEDWINDOW|WS_VISIBLE,10,10,320,160,std::ptr::null_mut(),std::ptr::null_mut(),wc.hInstance,std::ptr::null());
            assert!(!hwnd.is_null());REQUESTS.store(0,Ordering::Relaxed);
            let (tx,rx)=mpsc::channel();
            let worker=std::thread::spawn(move||{let _=tx.send(Runtime::new(Options{enabled:true,..Default::default()},0));});
            let started=std::time::Instant::now();
            let runtime=loop {
                if let Ok(result)=rx.try_recv(){break result.unwrap();}
                assert!(started.elapsed()<std::time::Duration::from_secs(10),"Dock startup must finish while Settings exists");
                let mut msg:MSG=std::mem::zeroed();
                while PeekMessageW(&mut msg,std::ptr::null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}
                std::thread::sleep(std::time::Duration::from_millis(5));
            };
            DestroyWindow(hwnd);worker.join().unwrap();drop(runtime);
            assert_eq!(REQUESTS.load(Ordering::Relaxed),0,"Dock must exclude its Settings before cross-thread WM_GETTEXT");
            UnregisterClassW(name.as_ptr(),wc.hInstance);
        }
    }
    #[test]
    #[ignore = "native Settings registry, minimize and recreate with real HWNDs"]
    fn native_registered_settings_identity_state_and_pins(){
        use crate::dock::internal_app::{self,KEY};
        use windows_sys::Win32::{Foundation::*,System::LibraryLoader::GetModuleHandleW,UI::WindowsAndMessaging::*};
        use std::ptr::null_mut;
        unsafe {
            let screens=super::super::displays();
            let display=screens.first().unwrap();
            let create=|x,y|{
                let hwnd=CreateWindowExW(0,crate::wide("STATIC").as_ptr(),crate::wide("Different native title").as_ptr(),
                    WS_OVERLAPPEDWINDOW,x,y,160,120,null_mut(),null_mut(),GetModuleHandleW(null_mut()),null_mut());
                assert!(!hwnd.is_null());SetWindowPos(hwnd,null_mut(),0,0,0,0,
                    SWP_NOMOVE|SWP_NOSIZE|SWP_NOZORDER|SWP_NOACTIVATE|SWP_SHOWWINDOW);
                hwnd
            };
            let first=create(display.bounds.left+10,display.bounds.top+10);
            let other=create(display.bounds.left+24,display.bounds.top+24);
            let mut options=Options{enabled:true,stage_enabled:true,all_windows:false,..Default::default()};
            let rt=Runtime::new(options.clone(),0).unwrap();
            let inspect=||{
                let (tx,rx)=mpsc::channel();rt.send(Command::InspectSettings{reply:tx});
                rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap()
            };
            let inspect_stage=||{
                let (tx,rx)=mpsc::channel();rt.send(Command::InspectStageSettings{reply:tx});
                rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap()
            };
            let stage_row=|screen:&str,hwnd:HWND,minimized:bool|vec![(screen.to_owned(),hwnd as usize,minimized)];
            assert!(inspect().is_empty(),"all unregistered internal windows must stay excluded");
            assert!(inspect_stage().is_empty(),"Stage must not include arbitrary in-process windows");
            internal_app::register(first);
            assert_eq!(inspect(),vec![(display.id.clone(),first as usize,false,false)]);
            assert_eq!(inspect_stage(),stage_row(&display.id,first,false));
            assert!(super::super::stage_window_owner(first).is_some());
            assert!(super::super::stage_window_owner(other).is_none());
            assert!(super::super::previews::permitted_pid(first).is_err(),"the generic capture path must still exclude this process");
            assert!(super::super::previews::permitted_settings_pid(other).is_err(),"the dedicated route must only accept registered Settings");
            ShowWindow(first,SW_MINIMIZE);
            assert_eq!(inspect(),vec![(display.id.clone(),first as usize,true,false)]);
            assert_eq!(inspect_stage(),stage_row(&display.id,first,true));
            assert!(crate::desktop::show_settings_window(first));
            assert_eq!(inspect(),vec![(display.id.clone(),first as usize,false,false)]);
            assert_eq!(inspect_stage(),stage_row(&display.id,first,false));
            internal_app::register(other);
            internal_app::unregister(first);DestroyWindow(first);
            assert_eq!(inspect(),vec![(display.id.clone(),other as usize,false,false)]);
            assert_eq!(inspect_stage(),stage_row(&display.id,other,false));
            if let Some(second)=screens.get(1){
                SetWindowPos(other,null_mut(),second.bounds.left+10,second.bounds.top+10,0,0,SWP_NOSIZE|SWP_NOZORDER|SWP_NOACTIVATE);
                assert_eq!(inspect(),vec![(second.id.clone(),other as usize,false,false)]);
                assert_eq!(inspect_stage(),stage_row(&second.id,other,false));
            }
            internal_app::unregister(other);DestroyWindow(other);
            assert!(inspect().is_empty());
            assert!(inspect_stage().is_empty());
            options.pinned=vec![KEY.into()];rt.configure(options);
            let pinned=inspect();
            assert_eq!(pinned.len(),screens.len());
            assert!(pinned.iter().all(|(_,handle,min,pin)|*handle==0&&!*min&&*pin));
        }
    }
    #[test]
    #[ignore = "actual overlay windows on available Windows displays; explicitly run in CI"]
    fn native_display_targets_filter_hwnds_and_keep_other_module() {
        use super::super::{displays,DisplayMode};
        use windows_sys::Win32::{Foundation::RECT,UI::WindowsAndMessaging::GetWindowRect};
        let displays=displays();
        let primary=displays.iter().find(|d|d.primary).expect("primary monitor");
        let selected=displays.iter().find(|d|!d.primary).unwrap_or(primary);
        let mut options=Options{enabled:true,stage_enabled:true,all_windows:true,..Default::default()};
        options.dock_display.mode=DisplayMode::Primary;
        options.stage_display.mode=DisplayMode::Selected;
        options.stage_display.selected=Some(selected.id.clone());
        let runtime=Runtime::new(options.clone(),0).unwrap();
        let fits=|handle:usize,area:RECT|unsafe{
            let mut r:RECT=std::mem::zeroed();
            GetWindowRect(handle as _,&mut r)!=0&&r.left>=area.left&&r.top>=area.top&&r.right<=area.right&&r.bottom<=area.bottom
        };
        let h=runtime.inspect();
        assert_eq!(h.len(),2,"application scope must not create overlays on other outputs");
        assert!(fits(h[0],primary.bounds));assert!(fits(h[1],selected.bounds));
        options.enabled=false;runtime.configure(options.clone());assert_eq!(runtime.inspect(),vec![h[1]]);
        options.stage_display.mode=DisplayMode::Custom;
        options.stage_display.custom=vec!["monitor:missing-fixture".into()];
        runtime.configure(options.clone());
        let fallback=runtime.inspect();
        assert_eq!(fallback.len(),1);assert!(fits(fallback[0],primary.bounds));
        options.stage_display.custom.push(selected.id.clone());runtime.configure(options);
        let restored=runtime.inspect();
        assert_eq!(restored.len(),1);assert!(fits(restored[0],selected.bounds));
    }
    #[test]
    #[ignore = "real four-edge Dock and top popup placement; hosted Windows only"]
    fn native_four_edge_dock_keeps_top_popups_inside_the_display() {
        use super::super::{displays,DockSide,DisplayMode,control_center,panel_chrome,glass_probe::Layer};
        use windows_sys::Win32::{Foundation::*,Graphics::{Dwm::DwmFlush,Gdi::*},UI::WindowsAndMessaging::*};
        use std::{ptr,time::{Duration,Instant}};
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(),Ok("true"));
        let display=displays().into_iter().find(|d|d.primary).unwrap();
        let mut options=Options{enabled:true,..Default::default()};
        options.dock_display.mode=DisplayMode::Primary;
        options.appearance.reduced_motion=true;
        options.appearance.position=DockSide::Top;
        let runtime=Runtime::new(options.clone(),0).unwrap();
        let mut old=Vec::new();
        for side in [DockSide::Top,DockSide::Left,DockSide::Right,DockSide::Bottom,DockSide::Top] {
            options.appearance.position=side;runtime.configure(options.clone());
            let handles=runtime.inspect_with_paint(true);assert_eq!(handles.len(),1);
            let owner=handles[0] as HWND;old.push(owner as usize);
            unsafe {
                let mut bounds:RECT=std::mem::zeroed();assert_ne!(GetWindowRect(owner,&mut bounds),0);
                assert!(bounds.left>=display.work.left&&bounds.right<=display.work.right&&bounds.top>=display.work.top&&bounds.bottom<=display.work.bottom);
                if side!=DockSide::Top{continue;}
                assert!(bounds.top<display.work.top+40,"top Dock must remain at the selected top edge");
                let anchor=panel_chrome::Anchor{point:POINT{x:(bounds.left+bounds.right)/2,y:bounds.bottom},side,dark:true,transparency:30};
                control_center::show_list(owner,anchor,control_center::View{title:"Top Dock".into(),hint:"Synthetic placement fixture".into(),
                    cards:vec![control_center::Card{title:"Below the Dock".into(),hint:"Keyboard and pointer use the same popup".into(),icon:None,action:None}],metrics:None});
                let popup=control_center::window(owner).unwrap();
                let until=Instant::now()+Duration::from_millis(500);
                while Instant::now()<until {
                    let mut msg:MSG=std::mem::zeroed();for _ in 0..64{if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&msg);DispatchMessageW(&msg);}
                    std::thread::sleep(Duration::from_millis(10));
                }
                let mut panel:RECT=std::mem::zeroed();assert_ne!(GetWindowRect(popup,&mut panel),0);
                assert!(panel.top>=bounds.bottom&&panel.bottom<=display.work.bottom&&panel.left>=display.bounds.left&&panel.right<=display.bounds.right);
                DwmFlush();
                let (left,top,right,bottom)=(bounds.left.min(panel.left),bounds.top,bounds.right.max(panel.right),panel.bottom);
                let (w,h)=(right-left,bottom-top);let mut pixels=Layer::new(w,h).unwrap();
                let dc=GetDC(ptr::null_mut());assert_ne!(BitBlt(pixels.dc(),0,0,w,h,dc,left,top,SRCCOPY|CAPTUREBLT),0);ReleaseDC(ptr::null_mut(),dc);GdiFlush();
                let rgba=pixels.pixels().chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],255]).collect::<Vec<_>>();
                if let Some(root)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE") {
                    let root=std::path::PathBuf::from(root);std::fs::create_dir_all(&root).unwrap();
                    image::save_buffer(root.join("dock-top-popup.png"),&rgba,w as u32,h as u32,image::ColorType::Rgba8).unwrap();
                }
                control_center::close_for(owner);assert_eq!(IsWindow(popup),0);
            }
        }
        drop(runtime);for hwnd in old{assert_eq!(unsafe{IsWindow(hwnd as HWND)},0);}
        println!("DOCK_EDGES four_positions=PASS top_popup_below=PASS reconfigure=PASS teardown=PASS physical_mixed_dpi=NOT_VERIFIED");
    }
    #[test]
    #[ignore = "real isolated Windows HWND/thread lifecycle; explicitly run in CI"]
    fn native_integrated_modules_toggle_suspend_and_cleanup() {
        // Never load/write a real profile or use the installed application.
        let screens=crate::monitors().len();assert!(screens>0);
        let mut options=Options{enabled:true,stage_enabled:true,..Default::default()};
        for _ in 0..3 {
            let runtime=Runtime::new(options.clone(),0).unwrap();
            let control=runtime.control;
            let handles=runtime.inspect();assert_eq!(handles.len(),screens*2);
            for &handle in &handles {unsafe {let mut pid=0;GetWindowThreadProcessId(handle as _,&mut pid);assert_eq!(pid,std::process::id());}}
            runtime.suspend(true);
            for handle in runtime.inspect(){assert_eq!(unsafe{IsWindowVisible(handle as _)},0);}
            runtime.suspend(false);runtime.inspect();
            options.enabled=false;runtime.configure(options.clone());
            let stages=runtime.inspect();assert_eq!(stages.len(),screens);
            assert!(stages.iter().all(|handle|handles.contains(handle)),"disabling Dock must retain the existing Stage HWNDs");
            options.stage_enabled=false;runtime.configure(options.clone());assert!(runtime.inspect().is_empty());
            options.enabled=true;runtime.configure(options.clone());assert_eq!(runtime.inspect().len(),screens);
            drop(runtime);
            assert_eq!(unsafe{IsWindow(control as _)},0,"hidden controller must be destroyed too");
            for handle in handles.into_iter().chain(stages){assert_eq!(unsafe{IsWindow(handle as _)},0);}
            options.stage_enabled=true;
        }
    }

    #[test]
    #[ignore = "isolated Windows Actions desktop only; terminates its own test subprocess"]
    fn native_appbar_fatal_exit_restores_work_area() {
        use super::super::{appbar,displays,same_rect,DockSide,CLASS};
        use std::{process::{Command as ProcessCommand,Stdio}, thread, time::{Duration,Instant}};
        use windows_sys::Win32::{System::LibraryLoader::GetModuleHandleW,UI::WindowsAndMessaging::*};
        const CHILD:&str="TUANPC_DESK_APPBAR_CI_CHILD";
        const TEST:&str="dock::runtime::tests::native_appbar_fatal_exit_restores_work_area";
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(),Ok("true"),"only run on the disposable Actions desktop");
        let shell=unsafe{ FindWindowW(crate::desktop::wide("Shell_TrayWnd").as_ptr(),std::ptr::null()) };
        if shell.is_null(){
            eprintln!("SKIP live AppBar crash test: Explorer taskbar missing on runner");
            return;
        }
        let primary=||displays().into_iter().find(|d|d.primary).expect("primary display");
        if std::env::var_os(CHILD).is_some(){
            let screen=primary();
            let request=|stage,side|appbar::Request{
                display:screen.id.clone(),stage,monitor:screen.bounds,side,depth:70
            };
            let class=crate::desktop::wide(CLASS);
            let mut bars=appbar::Bars::default();
            unsafe{
                let mut wc:WNDCLASSW=std::mem::zeroed();
                wc.lpfnWndProc=Some(DefWindowProcW);
                wc.hInstance=GetModuleHandleW(std::ptr::null());
                wc.lpszClassName=class.as_ptr();
                assert_ne!(RegisterClassW(&wc),0);
                assert!(bars.sync(vec![request(false,DockSide::Bottom),request(true,DockSide::Left)],false,false).healthy);
                let until=Instant::now()+Duration::from_secs(25);
                while Instant::now()<until{
                    let mut msg:MSG=std::mem::zeroed();
                    while PeekMessageW(&mut msg,std::ptr::null_mut(),0,0,PM_REMOVE)!=0{DispatchMessageW(&msg);}
                    thread::sleep(Duration::from_millis(15));
                }
            }
            return; // bounded fallback if the controller process exits unexpectedly
        }
        let before=primary();
        let mut child=ProcessCommand::new(std::env::current_exe().unwrap())
            .args(["--exact",TEST,"--ignored","--nocapture","--test-threads=1"])
            .env(CHILD,"1").stdin(Stdio::null()).spawn().unwrap();
        let deadline=Instant::now()+Duration::from_secs(8);
        let reserved=loop {
            let screen=primary();
            if screen.id==before.id&&screen.work.bottom<before.work.bottom&&screen.work.left>before.work.left{break true;}
            if Instant::now()>=deadline||child.try_wait().unwrap().is_some(){break false;}
            thread::sleep(Duration::from_millis(100));
        };
        // A direct child handle identifies the test process. Don't touch
        // Explorer or repair the work area artificially.
        if child.try_wait().unwrap().is_none(){child.kill().unwrap();}
        let _=child.wait().unwrap();
        let deadline=Instant::now()+Duration::from_secs(8);
        let restored=loop {
            let screen=primary();
            if screen.id==before.id&&same_rect(screen.work,before.work){break true;}
            if Instant::now()>=deadline{break false;}
            thread::sleep(Duration::from_millis(100));
        };
        assert!(reserved,"live Explorer did not reserve both AppBar strips");
        assert!(restored,"release blocker: Explorer retained a reserved strip after process termination");
        eprintln!("Live Explorer AppBar reservations and forced-exit cleanup observed on the Actions desktop");
    }

    #[test]
    #[ignore = "real Windows resources; explicitly run in CI"]
    fn native_100_toggles_release_overlays_and_process_handles() {
        use std::time::{Duration,Instant};
        use windows_sys::Win32::System::Threading::{GetCurrentProcess,GetProcessHandleCount};
        fn handles() -> u32 {
            let mut n=0;
            assert_ne!(unsafe{GetProcessHandleCount(GetCurrentProcess(),&mut n)},0);
            n
        }
        let off=Options::default();
        let on=Options{enabled:true,stage_enabled:true,..Default::default()};
        let runtime=Runtime::new(on.clone(),0).unwrap();
        // Paint the real overlays and let Windows finish asynchronous graphics
        // initialization before the OFF-only control measurement. On the cold
        // runner that one-time allocation can land in either the control loop
        // or the Stage warm-up, depending on scheduling.
        let cold=handles();
        assert!(!runtime.inspect_with_paint(true).is_empty());
        std::thread::sleep(Duration::from_secs(1));
        assert!(!runtime.inspect_with_paint(true).is_empty());
        runtime.configure(off.clone());
        assert!(runtime.inspect().is_empty());
        std::thread::sleep(Duration::from_millis(750));
        eprintln!("Cold native paint/capture calibration: {cold} -> {} handles",handles());
        let measure=|interval:u32| {
            let deadline=Instant::now()+Duration::from_secs(2);
            while handles()>interval+8 && Instant::now()<deadline {
                std::thread::sleep(Duration::from_millis(25));
            }
            handles()
        };
        // Calibrate the probe itself. Cold Windows/COM/runtime initialization
        // can continue after the first modules are off, before the fixture
        // makes any active toggles. Keep that measurement in the CI log.
        let cold=handles();
        for _ in 0..100 {
            runtime.configure(off.clone());
            assert!(runtime.inspect().is_empty());
            runtime.configure(off.clone());
            assert!(runtime.inspect().is_empty());
        }
        let calibrated=measure(cold);
        eprintln!("Cold controller calibration (modules OFF): {cold} -> {calibrated} handles");
        let mut results=Vec::new();
        for (label,dock,stage) in [("control",false,false),("stage-only",false,true),
            ("dock-only",true,false),("both",true,true)] {
            let on=Options{enabled:dock,stage_enabled:stage,..Default::default()};
            let toggle=||{
                runtime.configure(on.clone());
                let active=runtime.inspect_with_paint(dock||stage);
                assert_eq!(active.is_empty(),!(dock||stage));
                runtime.configure(off.clone());
                assert!(runtime.inspect().is_empty());
                assert!(active.iter().all(|h| unsafe{IsWindow(*h as _)}==0));
            };
            // Rendering a stage with real windows initializes system-wide
            // font/compositor caches lazily; warm the same path, not just
            // the idle controller. Keep the cold delta in the log and the
            // original <=8 growth bound for the subsequent 100 toggles.
            let cold=handles();
            for _ in 0..12{toggle();}
            let start=measure(cold);
            eprintln!("{label} active-path warm-up: {cold} -> {start} handles");
            let t=Instant::now();
            for _ in 0..100{toggle();}
            let after=measure(start);
            eprintln!("100 {label} toggles in {:?}; process handles {start} -> {after}",t.elapsed());
            results.push((label,start,after));
        }
        let controller=runtime.control;
        drop(runtime);
        assert_eq!(unsafe{IsWindow(controller as _)},0);
        // The real app drops the controller when both modules are disabled.
        // Verify that path separately from live reconfiguration.
        let cold_restart=handles();
        for _ in 0..5 {drop(Runtime::new(on.clone(),0).unwrap());}
        let resumed=measure(cold_restart);
        eprintln!("Controller restart warm-up: {cold_restart} -> {resumed} handles");
        let t=Instant::now();
        for _ in 0..100 {
            let runtime=Runtime::new(on.clone(),0).unwrap();
            let controller=runtime.control;
            let active=runtime.inspect_with_paint(true);
            assert!(!active.is_empty());
            drop(runtime);
            assert_eq!(unsafe{IsWindow(controller as _)},0);
            assert!(active.iter().all(|h| unsafe{IsWindow(*h as _)}==0));
        }
        let after=measure(resumed);
        eprintln!("100 full controller restarts in {:?}; process handles {resumed} -> {after}",t.elapsed());
        results.push(("full-restart",resumed,after));
        assert!(results.iter().all(|(_,start,after)|*after<=start+8),
            "process handles must not accumulate after module OFF: {results:?}");
    }
}
