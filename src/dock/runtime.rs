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
    pub fn new(options: Options) -> Result<Self,String> {
        let (commands,receiver)=mpsc::channel();let (events_tx,events)=mpsc::channel();
        let (ready_tx,ready)=mpsc::sync_channel(1);
        let thread=std::thread::Builder::new().name("Deskland Dock & Stage".into()).spawn(move || {
            super::run(options,receiver,events_tx,ready_tx);
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
    #[ignore = "real isolated Windows HWND/thread lifecycle; explicitly run in CI"]
    fn native_integrated_modules_toggle_suspend_and_cleanup() {
        // Never load/write a real profile or use the installed application.
        let screens=crate::monitors().len();assert!(screens>0);
        let mut options=Options{enabled:true,stage_enabled:true,..Default::default()};
        for _ in 0..3 {
            let runtime=Runtime::new(options.clone()).unwrap();
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
            for handle in handles.into_iter().chain(stages){assert_eq!(unsafe{IsWindow(handle as _)},0);}
            options.stage_enabled=true;
        }
    }
}
