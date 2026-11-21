//! One managed UI thread in the Deskland process, no companion EXE/config owner.
use super::Options;
use std::{sync::mpsc::{self, Receiver, Sender}, thread::JoinHandle};

pub(super) enum Command {
    Configure(Options),
    Suspend(bool),
    Stop,
    #[cfg(test)] Inspect(Sender<Vec<usize>>),
}
#[derive(Debug)]
pub enum Event { Edit(super::Edit), Error(String) }

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
    pub fn suspend(&self,on:bool) {self.send(Command::Suspend(on));}
    pub fn take_events(&self)->Vec<Event> {self.events.try_iter().collect()}
    pub fn finished(&self)->bool {self.thread.as_ref().is_none_or(JoinHandle::is_finished)}
    #[cfg(test)]
    fn inspect(&self)->Vec<usize> {
        let (tx,rx)=mpsc::channel();self.send(Command::Inspect(tx));
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
}
