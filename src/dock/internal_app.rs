//! The Settings owners publish exactly one top-level HWND. The Dock never
//! reads a title or sends a synchronous message to an internal window.
use std::sync::Mutex;
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::*,UI::WindowsAndMessaging::*};

pub(super) const KEY:&str="deskland:settings";
pub(super) const LABEL:&str="Cài đặt tuanpc-desk";
#[derive(Default)]
struct Registry { hwnd:usize, listener:usize }
static SETTINGS:Mutex<Registry>=Mutex::new(Registry{hwnd:0,listener:0});

fn notify(listener:usize){
    if listener!=0{unsafe{PostMessageW(listener as HWND,super::WM_INTERNAL_APP,0,0);}}
}
pub(crate) fn register(hwnd:HWND){
    if hwnd.is_null(){return;}
    let listener={
        let mut state=SETTINGS.lock().unwrap();
        state.hwnd=hwnd as usize;
        state.listener
    };
    notify(listener);
}
pub(crate) fn unregister(hwnd:HWND){
    let listener={
        let mut state=SETTINGS.lock().unwrap();
        if state.hwnd!=hwnd as usize{return;}
        state.hwnd=0;
        state.listener
    };
    notify(listener);
}
pub(crate) fn changed(hwnd:HWND){
    let state=SETTINGS.lock().unwrap();
    let listener=if state.hwnd==hwnd as usize{state.listener}else{0};
    drop(state);
    notify(listener);
}
pub(super) fn listen(hwnd:HWND){SETTINGS.lock().unwrap().listener=hwnd as usize;}
pub(super) fn unlisten(hwnd:HWND){
    let mut state=SETTINGS.lock().unwrap();
    if state.listener==hwnd as usize{state.listener=0;}
}
pub(super) fn current()->HWND{SETTINGS.lock().unwrap().hwnd as HWND}
pub(super) fn window()->Option<super::AppWindow>{
    if crate::trial::demo_active()||crate::trial::benchmark_active(){return None;}
    let hwnd=current();
    unsafe{
        let mut pid=0;
        if hwnd.is_null()||GetWindowThreadProcessId(hwnd,&mut pid)==0||pid!=std::process::id()||IsWindowVisible(hwnd)==0{return None;}
        let monitor=MonitorFromWindow(hwnd,MONITOR_DEFAULTTONEAREST);
        let mut info=MONITORINFO{cbSize:std::mem::size_of::<MONITORINFO>() as u32,..std::mem::zeroed()};
        if GetMonitorInfoW(monitor,&mut info)==0||current()!=hwnd{return None;}
        Some(super::AppWindow{hwnd,title:LABEL.into(),path:KEY.into(),rect:info.rcMonitor})
    }
}
