use crate::desktop::wide;
use super::panel_chrome::{self,Anchor};
use std::{mem,ptr,sync::atomic::{AtomicBool,Ordering},thread,time::Duration};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::DwmGetWindowAttribute,Gdi::{MonitorFromPoint,MonitorFromWindow,MONITOR_DEFAULTTONEAREST}},
    System::Threading::{OpenProcess,QueryFullProcessImageNameW,PROCESS_QUERY_LIMITED_INFORMATION},
    UI::{Input::KeyboardAndMouse::*,WindowsAndMessaging::*},
};
use windows::core::Interface;
use windows::Win32::{
    Foundation::HWND as AutomationHwnd,
    System::Com::{CoCreateInstance,CoInitializeEx,CoUninitialize,CLSCTX_INPROC_SERVER,COINIT_MULTITHREADED},
    UI::Accessibility::{CUIAutomation,IUIAutomation,IUIAutomation2,IUIAutomationInvokePattern,IUIAutomationTogglePattern,
        TreeScope_Descendants,UIA_InvokePatternId,UIA_TogglePatternId},
};

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) enum Target { Start, Tray, Clock }

impl Target {
    pub const ALL:[Self;3]=[Self::Start,Self::Tray,Self::Clock];
    pub fn title(self,dock_mode:bool)->&'static str {
        if dock_mode {return match self{Self::Start=>"Start · Tìm kiếm Deskland",Self::Tray=>"Trung tâm điều khiển Deskland",Self::Clock=>"Ngày giờ · Lịch Deskland"};}
        match self {Self::Start=>"Start · Windows",Self::Tray=>"Biểu tượng ẩn · Windows",Self::Clock=>"Ngày giờ · Lịch Windows"}
    }
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) enum Route { Native, Launcher, Hub, Calendar }
pub(super) fn route(dock_mode:bool,target:Target,context:bool)->Route{
    match (dock_mode,target,context){
        (true,Target::Start,false)=>Route::Launcher,
        (true,Target::Start,true)|(true,Target::Tray,_)=>Route::Hub,
        (true,Target::Clock,_)=>Route::Calendar,
        _=>Route::Native,
    }
}
pub(super) fn native_busy()->bool{[&OPENING_START,&OPENING_TRAY].into_iter().any(|v|v.load(Ordering::Acquire))}

pub(super) unsafe fn activate(target:Target, context:bool,anchor:Option<Anchor>,owner:HWND,control:HWND){
    super::utilities::close_panels();
    match (target,context){
        (Target::Start,true)=>{win_chord(b'X' as u16);}
        (Target::Start,false)|(Target::Tray,_)=>open_native(target,anchor,owner,control),
        (Target::Clock,_)=>{win_chord(b'N' as u16);}
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

static OPENING_START:AtomicBool=AtomicBool::new(false);
static OPENING_TRAY:AtomicBool=AtomicBool::new(false);

fn is_overflow(class:&str,id:&str,name:&str)->bool{
    matches!(class,"SystemTray.ChevronIconView"|"SystemTray.OverflowButton")
        || matches!(id,"ChevronIconView"|"NotificationChevron"|"OverflowButton"|"SystemTrayIcon")
            && matches!(class,"SystemTray.NormalButton"|"SystemTray.IconView"|""|"Button")
            && matches!(name,"Show hidden icons"|"Show Hidden Icons"|"Hiển thị biểu tượng ẩn"|"Hiện biểu tượng ẩn")
}

unsafe fn class(hwnd:HWND)->String{
    let mut buf=[0u16;128];
    let n=GetClassNameW(hwnd,buf.as_mut_ptr(),buf.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&buf[..n])
}
unsafe fn pid(hwnd:HWND)->u32{
    let mut value=0;GetWindowThreadProcessId(hwnd,&mut value);value
}
unsafe extern "system" fn collect(hwnd:HWND,raw:LPARAM)->BOOL{
    (&mut *(raw as *mut Vec<HWND>)).push(hwnd);
    1
}
unsafe fn top_windows()->Vec<HWND>{
    let mut windows=Vec::new();
    EnumWindows(Some(collect),(&mut windows as *mut Vec<HWND>) as LPARAM);
    windows
}

unsafe fn taskbar_roots(bar:HWND,shell:u32)->Vec<HWND>{
    let mut children=Vec::new();
    EnumChildWindows(bar,Some(collect),(&mut children as *mut Vec<HWND>) as LPARAM);
    // Windows 11 exposes the real Start and overflow controls under the
    // XAML island's HWND, but they may not be in the outer taskbar UIA tree.
    children.retain(|&h|pid(h)==shell&&class(h)=="Windows.UI.Composition.DesktopWindowContentBridge");
    children.push(bar);
    children
}
unsafe fn invoke_taskbar(automation:&IUIAutomation,target:Target,shell:u32,anchor:Option<Anchor>)->bool{
    if shell==0{return false;}
    let monitor=anchor.map(|a|MonitorFromPoint(a.point,MONITOR_DEFAULTTONEAREST));
    let mut bars=top_windows().into_iter().filter(|&h|pid(h)==shell&&matches!(class(h).as_str(),"Shell_TrayWnd"|"Shell_SecondaryTrayWnd")).collect::<Vec<_>>();
    bars.sort_by_key(|&h|u8::from(monitor!=Some(MonitorFromWindow(h,MONITOR_DEFAULTTONEAREST))));
    let Ok(condition)=automation.ControlViewCondition()else{return false;};
    let started=std::time::Instant::now();
    for hwnd in bars.into_iter().flat_map(|bar|taskbar_roots(bar,shell)) {
        if started.elapsed()>=Duration::from_millis(650){return false;}
        let Ok(root)=automation.ElementFromHandle(AutomationHwnd(hwnd))else{continue;};
        let Ok(children)=root.FindAll(TreeScope_Descendants,&condition)else{continue;};
        for index in 0..children.Length().unwrap_or(0).min(300){
            if started.elapsed()>=Duration::from_millis(650){return false;}
            let Ok(item)=children.GetElement(index)else{continue;};
            if item.CurrentProcessId().ok()!=Some(shell as i32){continue;}
            let id=item.CurrentAutomationId().map(|x|x.to_string()).unwrap_or_default();
            if target==Target::Start{
                if id!="StartButton"{continue;}
                if let Ok(toggle)=item.GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId){
                    if toggle.Toggle().is_ok(){return true;}
                }
            }else{
                let class=item.CurrentClassName().map(|x|x.to_string()).unwrap_or_default();
                let name=item.CurrentName().map(|x|x.to_string()).unwrap_or_default();
                if !is_overflow(&class,&id,&name){continue;}
            }
            if let Ok(invoke)=item.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId){
                if invoke.Invoke().is_ok(){return true;}
            }
        }
    }
    false
}

unsafe fn start_host(process:u32)->bool{
    let Some(windows)=std::env::var_os("SystemRoot")else{return false;};
    let expected=std::path::PathBuf::from(windows).join(r"SystemApps\Microsoft.Windows.StartMenuExperienceHost_cw5n1h2txyewy\StartMenuExperienceHost.exe");
    let handle=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,0,process);
    if handle.is_null(){return false;}
    let mut buf=[0u16;1024];let mut len=buf.len() as u32;
    let ok=QueryFullProcessImageNameW(handle,0,buf.as_mut_ptr(),&mut len)!=0;
    CloseHandle(handle);
    ok&&expected.to_string_lossy().eq_ignore_ascii_case(&String::from_utf16_lossy(&buf[..len as usize]))
}
unsafe fn native_popup(target:Target,shell:u32)->Option<(HWND,RECT)>{
    let mut matches=top_windows().into_iter().filter_map(|h|{
        if IsWindowVisible(h)==0{return None;}
        let kind=class(h);
        let process=pid(h);
        let expected=match target{
            Target::Tray=>shell!=0&&process==shell&&matches!(kind.as_str(),"TopLevelWindowForOverflowXamlIsland"|"NotifyIconOverflowWindow"),
            Target::Start=>kind=="Windows.UI.Core.CoreWindow"&&start_host(process),
            _=>false,
        };
        if !expected{return None;}
        let mut cloaked=0u32;
        if DwmGetWindowAttribute(h,14,(&mut cloaked as *mut u32).cast(),4)<0||cloaked!=0{return None;}
        let mut r:RECT=mem::zeroed();
        if GetWindowRect(h,&mut r)==0||r.right-r.left<32||r.bottom-r.top<24{return None;}
        Some((h,r))
    });
    let first=matches.next()?;
    matches.next().is_none().then_some(first)
}
unsafe fn place_popup(target:Target,shell:u32,anchor:Anchor)->bool{
    let mut last=None;
    for _ in 0..24{
        thread::sleep(Duration::from_millis(25));
        let Some((hwnd,r))=native_popup(target,shell)else{last=None;continue;};
        let stamp=(hwnd,r.left,r.top,r.right,r.bottom);
        if last!=Some(stamp){last=Some(stamp);continue;}
        let Some(work)=anchor.work()else{return true;};
        let (w,h)=(r.right-r.left,r.bottom-r.top);
        if w>work.right-work.left||h>work.bottom-work.top{return true;}
        let (x,y)=panel_chrome::placement(work,anchor,w,h,12);
        if (x,y)!=(r.left,r.top){
            // Move only the recognized flyout that just opened. Keep the
            // original menu, focus, z-order, icons and context handlers.
            let moved=SetWindowPos(hwnd,ptr::null_mut(),x,y,0,0,SWP_NOSIZE|SWP_NOZORDER|SWP_NOACTIVATE|SWP_ASYNCWINDOWPOS)!=0;
            crate::log!("DOCK_SHELL {target:?} anchor requested={moved}");
        }
        return true;
    }
    crate::log!("DOCK_SHELL {target:?} native flyout not recognized for positioning");
    false
}

unsafe fn open_native(target:Target,anchor:Option<Anchor>,owner:HWND,control:HWND){
    let opening=if target==Target::Start{&OPENING_START}else{&OPENING_TRAY};
    if opening.swap(true,Ordering::AcqRel){return;}
    let (owner,control)=(owner as usize,control as usize);
    let before=GetForegroundWindow() as usize;
    let fallback=move||{
        PostMessageW(control as HWND,super::WM_SHELL_FALLBACK,owner,if target==Target::Start{0}else{1});
    };
    if thread::Builder::new().name("deskland-shell-flyout".into()).spawn(move||{
        struct Finish(&'static AtomicBool);
        impl Drop for Finish{fn drop(&mut self){self.0.store(false,Ordering::Release);}}
        let _finish=Finish(opening);
        unsafe{
            let shell=pid(GetShellWindow());
            let was_open=native_popup(target,shell).is_some();
            let mut invoked=false;
            if CoInitializeEx(None,COINIT_MULTITHREADED).is_ok(){
                {
                    let uia:windows::core::Result<IUIAutomation>=CoCreateInstance(&CUIAutomation,None,CLSCTX_INPROC_SERVER);
                    if let Ok(uia)=uia{
                        if let Ok(tuning)=uia.cast::<IUIAutomation2>(){
                            let _=tuning.SetConnectionTimeout(250);
                            let _=tuning.SetTransactionTimeout(350);
                        }
                        invoked=invoke_taskbar(&uia,target,shell,anchor);
                    }
                }
                CoUninitialize();
            }
            if !invoked&&target==Target::Start{invoked=send_win(None);}
            let claimed=invoked&&(was_open||anchor.is_none_or(|anchor|place_popup(target,shell,anchor)));
            // Windows updates have changed the Start flyout class in the past.
            // Don't open a second panel when the real host took the foreground.
            if !claimed&&GetForegroundWindow() as usize==before{
                crate::log!("DOCK_SHELL {target:?} native unavailable; using Deskland");
                fallback();
            }
        }
    }).is_err(){opening.store(false,Ordering::Release);fallback();}
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn dock_mode_never_routes_mouse_or_keyboard_shell_buttons_to_explorer(){
        for target in Target::ALL{
            for context in [true,false]{
                assert_eq!(route(false,target,context),Route::Native);
                assert_ne!(route(true,target,context),Route::Native);
            }
        }
        assert_eq!(route(true,Target::Start,false),Route::Launcher);
        assert_eq!(route(true,Target::Tray,false),Route::Hub);
        assert_eq!(route(true,Target::Clock,false),Route::Calendar);
    }
    #[test]
    fn tray_selection_never_activates_an_application_icon(){
        assert!(is_overflow("SystemTray.ChevronIconView","",""));
        assert!(is_overflow("SystemTray.NormalButton","SystemTrayIcon","Show Hidden Icons"));
        assert!(!is_overflow("SystemTray.IconView","SystemTrayIcon","Network"));
        assert!(!is_overflow("SystemTray.NotifyIconView","NotifyItemIcon","Show hidden icons"));
        assert!(!is_overflow("","OverflowButton","Other menu"));
    }
}
