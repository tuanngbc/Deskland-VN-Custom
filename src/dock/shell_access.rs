use crate::desktop::wide;
use std::{mem,ptr, sync::atomic::{AtomicBool,Ordering}, thread, time::Duration};
use windows_sys::Win32::UI::{Input::KeyboardAndMouse::*,WindowsAndMessaging::{FindWindowW,GetWindowThreadProcessId,IsWindowVisible}};
use windows::Win32::{
    System::Com::{CoCreateInstance,CoInitializeEx,CoUninitialize,CLSCTX_INPROC_SERVER,COINIT_MULTITHREADED},
    UI::Accessibility::{IUIAutomation,IUIAutomationInvokePattern,CUIAutomation,UIA_InvokePatternId},
};

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) enum Target { Start, Tray, Clock }

impl Target {
    pub const ALL:[Self;3]=[Self::Start,Self::Tray,Self::Clock];
    pub fn title(self)->&'static str {
        match self {Self::Start=>"Start · Windows",Self::Tray=>"Biểu tượng ẩn · Windows",Self::Clock=>"Ngày giờ · Lịch Windows"}
    }
}

pub(super) unsafe fn activate(target:Target, context:bool){
    super::utilities::close_panels();
    match (target,context){
        (Target::Start,true)=>{win_chord(b'X' as u16);}
        (Target::Start,false)=>{send_win(None);}
        (Target::Clock,_)=>{win_chord(b'N' as u16);}
        (Target::Tray,_)=>open_tray(),
    }
}

pub(super) unsafe fn win_chord(key:u16){send_win(Some(key));}
unsafe fn send_win(key:Option<u16>)->bool{
    // Leave physically held keys alone and don't steal input with a queued
    // Enter. Keyboard events are sent only after an explicit Dock action.
    if [VK_LWIN,VK_RWIN,VK_SHIFT,VK_CONTROL,VK_MENU].into_iter().chain(key).any(|vk|GetAsyncKeyState(vk as i32)<0){return false;}
    let event=|vk,flags|INPUT{r#type:INPUT_KEYBOARD,Anonymous:INPUT_0{ki:KEYBDINPUT{
        wVk:vk,wScan:0,dwFlags:flags,time:0,dwExtraInfo:0}}};
    let win_up=event(VK_LWIN,KEYEVENTF_KEYUP);
    let win_down=event(VK_LWIN,0);
    let input=key.map_or([win_down,win_up,win_up,win_up],|key|
        [win_down,event(key,0),event(key,KEYEVENTF_KEYUP),win_up]);
    let len=if key.is_some(){4}else{2};
    let count=SendInput(len,input.as_ptr(),mem::size_of::<INPUT>() as i32);
    if count>0&&count<len{
        let tail=if count==2{&input[2..]}else{std::slice::from_ref(&win_up)};
        SendInput(tail.len() as u32,tail.as_ptr(),mem::size_of::<INPUT>() as i32);
    }
    count==len
}

static PICKING_TRAY:AtomicBool=AtomicBool::new(false);
unsafe fn open_tray(){
    if PICKING_TRAY.swap(true,Ordering::AcqRel){return;}
    if !send_win(Some(b'B' as u16)){
        PICKING_TRAY.store(false,Ordering::Release);
        return;
    }
    if thread::Builder::new().name("deskland-tray".into()).spawn(||{
        struct Finish;
        impl Drop for Finish{fn drop(&mut self){PICKING_TRAY.store(false,Ordering::Release);}}
        let _finish=Finish;
        unsafe{
            if CoInitializeEx(None,COINIT_MULTITHREADED).is_err(){return;}
            {
                let automation:windows::core::Result<IUIAutomation>=CoCreateInstance(&CUIAutomation,None,CLSCTX_INPROC_SERVER);
                if let Ok(automation)=automation {
                    // Win+B focuses the system area. Invoke only the real
                    // overflow control, because on some PCs it is disabled or
                    // absent and Enter would trigger a different tray app.
                    let taskbar=FindWindowW(wide("Shell_TrayWnd").as_ptr(),ptr::null());
                    let mut pid=0;
                    if !taskbar.is_null()&&IsWindowVisible(taskbar)!=0{GetWindowThreadProcessId(taskbar,&mut pid);}
                    for _ in 0..5 {
                        if pid==0{break;}
                        thread::sleep(Duration::from_millis(40));
                        let Ok(element)=automation.GetFocusedElement()else{continue;};
                        if element.CurrentProcessId().ok()!=Some(pid as i32){continue;}
                        let class=element.CurrentClassName().map(|s|s.to_string()).unwrap_or_default();
                        let id=element.CurrentAutomationId().map(|s|s.to_string()).unwrap_or_default();
                        let name=element.CurrentName().map(|s|s.to_string()).unwrap_or_default();
                        if is_overflow(&class,&id,&name){
                            if let Ok(invoke)=element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId){
                                let _=invoke.Invoke();
                            }
                            break;
                        }
                    }
                }
            }
            CoUninitialize();
        }
    }).is_err(){PICKING_TRAY.store(false,Ordering::Release);}
}

fn is_overflow(class:&str,id:&str,name:&str)->bool{
    matches!(class,"SystemTray.ChevronIconView"|"SystemTray.OverflowButton")
        || matches!(id,"ChevronIconView"|"NotificationChevron"|"OverflowButton"|"SystemTrayIcon")
            && matches!(name,"Show hidden icons"|"Show Hidden Icons"|"Hiển thị biểu tượng ẩn"|"Hiện biểu tượng ẩn")
}

#[cfg(test)]
mod tests{
    use super::is_overflow;
    #[test]
    fn tray_focus_guard_never_activates_an_application_icon(){
        assert!(is_overflow("SystemTray.ChevronIconView","",""));
        assert!(is_overflow("SystemTray.IconView","SystemTrayIcon","Show hidden icons"));
        assert!(!is_overflow("SystemTray.IconView","SystemTrayIcon","Network"));
        assert!(!is_overflow("SystemTray.NotifyIconView","NotifyItemIcon","Show hidden icons"));
        assert!(!is_overflow("","OverflowButton","Other menu"));
    }
}
