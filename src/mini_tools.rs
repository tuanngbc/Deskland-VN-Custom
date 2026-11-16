//! On-demand native utilities. The wallpaper process only owns shortcuts; each
//! utility runs in another instance of this same executable and exits on close.
mod screen;
mod editor;
mod recorder;
mod preview;

use crate::desktop::wide;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const CAPTURE_ID: i32 = 31;
pub const RECORD_ID: i32 = 32;
pub const PREVIEW_ID: i32 = 33;
pub const PREVIEW_SPACE_ID: i32 = 34;
static MAIN_WINDOW:std::sync::atomic::AtomicIsize=std::sync::atomic::AtomicIsize::new(0);
static SPACE_ENABLED:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
static SPACE_HOOKS:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub enabled: bool,
    pub capture_mods: u32, pub capture_key: u32,
    pub record_mods: u32, pub record_key: u32,
    pub preview_mods: u32, pub preview_key: u32,
    pub fps: u32, pub audio: bool, pub space_preview:bool,
}
impl Default for Options {
    fn default() -> Self { Self { enabled:true,
        capture_mods:MOD_CONTROL|MOD_SHIFT|MOD_NOREPEAT,capture_key:b'S' as u32,
        record_mods:MOD_CONTROL|MOD_SHIFT|MOD_NOREPEAT,record_key:b'R' as u32,
        preview_mods:MOD_CONTROL|MOD_ALT|MOD_NOREPEAT,preview_key:VK_SPACE as u32,
        fps:30,audio:true,space_preview:true } }
}
impl Options {
    pub fn from_json(v:&serde_json::Value)->Self {
        let mut o=Self::default();
        o.enabled=v["enabled"].as_bool().unwrap_or(o.enabled);
        o.audio=v["audio"].as_bool().unwrap_or(o.audio);
        o.space_preview=v["spacePreview"].as_bool().unwrap_or(o.space_preview);
        o.fps=if v["fps"].as_u64()==Some(60){60}else{30};
        for (name,mods,key) in [("capture",&mut o.capture_mods,&mut o.capture_key),("record",&mut o.record_mods,&mut o.record_key),("preview",&mut o.preview_mods,&mut o.preview_key)] {
            let m=v[name]["mods"].as_u64().unwrap_or(*mods as u64) as u32;
            let k=v[name]["vk"].as_u64().unwrap_or(*key as u64) as u32;
            if m&(MOD_CONTROL|MOD_ALT|MOD_SHIFT)!=0 && (k==VK_SPACE as u32 || (0x41..=0x5a).contains(&k) || (0x70..=0x7b).contains(&k)) {
                *mods=(m&(MOD_CONTROL|MOD_ALT|MOD_SHIFT))|MOD_NOREPEAT; *key=k;
            }
        }
        o
    }
    pub fn json(&self)->serde_json::Value { serde_json::json!({"enabled":self.enabled,"fps":self.fps,"audio":self.audio,"spacePreview":self.space_preview,
        "capture":{"mods":self.capture_mods,"vk":self.capture_key},"record":{"mods":self.record_mods,"vk":self.record_key},"preview":{"mods":self.preview_mods,"vk":self.preview_key}}) }
}

pub fn register(hwnd:HWND, o:&Options)->[bool;3] {
    unsafe {
        MAIN_WINDOW.store(hwnd as isize,std::sync::atomic::Ordering::Relaxed);
        SPACE_ENABLED.store(o.enabled&&o.space_preview,std::sync::atomic::Ordering::Relaxed);
        if !SPACE_HOOKS.swap(true,std::sync::atomic::Ordering::Relaxed){
            use windows_sys::Win32::UI::Accessibility::*;
            SetWinEventHook(EVENT_SYSTEM_FOREGROUND,EVENT_SYSTEM_FOREGROUND,std::ptr::null_mut(),Some(focus_changed),0,0,WINEVENT_OUTOFCONTEXT);
            SetWinEventHook(EVENT_OBJECT_FOCUS,EVENT_OBJECT_FOCUS,std::ptr::null_mut(),Some(focus_changed),0,0,WINEVENT_OUTOFCONTEXT);
        }
        update_space_shortcut();
        for id in [CAPTURE_ID,RECORD_ID,PREVIEW_ID] { UnregisterHotKey(hwnd,id); }
        if !o.enabled { return [false;3]; }
        [(CAPTURE_ID,o.capture_mods,o.capture_key),(RECORD_ID,o.record_mods,o.record_key),(PREVIEW_ID,o.preview_mods,o.preview_key)].map(|(id,m,k)| RegisterHotKey(hwnd,id,m,k)!=0)
    }
}
unsafe extern "system" fn focus_changed(_:windows_sys::Win32::UI::Accessibility::HWINEVENTHOOK,_:u32,_:HWND,_:i32,_:i32,_:u32,_:u32){update_space_shortcut();}
unsafe fn update_space_shortcut(){
    let hwnd=MAIN_WINDOW.load(std::sync::atomic::Ordering::Relaxed)as HWND;
    if hwnd.is_null(){return;}
    UnregisterHotKey(hwnd,PREVIEW_SPACE_ID);
    if !SPACE_ENABLED.load(std::sync::atomic::Ordering::Relaxed){return;}
    let foreground=GetForegroundWindow();let mut class=[0u16;128];let n=GetClassNameW(foreground,class.as_mut_ptr(),128);let top=String::from_utf16_lossy(&class[..n.max(0)as usize]);
    if !matches!(top.as_str(),"CabinetWClass"|"ExploreWClass"|"Progman"|"WorkerW"){return;}
    let mut info:GUITHREADINFO=std::mem::zeroed();info.cbSize=std::mem::size_of::<GUITHREADINFO>()as u32;
    if GetGUIThreadInfo(GetWindowThreadProcessId(foreground,std::ptr::null_mut()),&mut info)==0{return;}
    let n=GetClassNameW(info.hwndFocus,class.as_mut_ptr(),128);let focus=String::from_utf16_lossy(&class[..n.max(0)as usize]);
    // Space stays untouched in address/search/rename fields and all other apps.
    if matches!(focus.as_str(),"DirectUIHWND"|"SysListView32"|"SHELLDLL_DefView"){RegisterHotKey(hwnd,PREVIEW_SPACE_ID,MOD_NOREPEAT,VK_SPACE as u32);}
}
pub fn spawn(mode:&str, o:&Options) {
    if mode=="record"{unsafe{let existing=FindWindowW(wide("DesklandRecordingPanel").as_ptr(),std::ptr::null());if !existing.is_null(){PostMessageW(existing,WM_CLOSE,0,0);return;}}}
    let result=std::env::current_exe().and_then(|exe| std::process::Command::new(exe)
        .arg("--mini-tool").arg(mode).arg("--tool-fps").arg(o.fps.to_string())
        .arg(if o.audio{"--tool-audio"}else{"--tool-silent"})
        .creation_flags(0x08000000).spawn());
    if let Err(e)=result { message(&format!("Không mở được tiện ích: {e}")); }
}
pub fn spawn_preview(path:&Path) {
    if let Ok(exe)=std::env::current_exe() {
        let _=std::process::Command::new(exe).arg("--mini-tool").arg("preview").arg("--preview-file").arg(path).creation_flags(0x08000000).spawn();
    }
}

/// Must run before the wallpaper singleton, logging, WebView, timers or OpenGL.
pub fn dispatch(args:&[String])->bool {
    let Some(i)=args.iter().position(|a|a=="--mini-tool") else {return false;};
    let mode=args.get(i+1).map(String::as_str).unwrap_or("");
    unsafe {
        windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _=windows::Win32::System::Com::CoInitializeEx(None,windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
    }
    let mutex=unsafe { windows_sys::Win32::System::Threading::CreateMutexW(std::ptr::null(),0,wide(&format!("DesklandMiniTool_{mode}")).as_ptr()) };
    if unsafe {GetLastError()}==ERROR_ALREADY_EXISTS {
        unsafe { if !mutex.is_null(){CloseHandle(mutex);} }
        return true;
    }
    let result=match mode {
        "capture"=>screen::select(false).and_then(|selection| match selection {Some((image,rect))=>editor::run(editor::Document::new(screen::crop(&image,rect)),false),None=>Ok(())}),
        "record"=>screen::select(true).and_then(|selection|match selection {Some((image,rect))=>{drop(image);recorder::run(rect,if args.windows(2).any(|v|v[0]=="--tool-fps"&&v[1]=="60"){60}else{30},args.iter().any(|a|a=="--tool-audio"))},None=>Ok(())}),
        "preview"=>{
            let path=args.iter().position(|a|a=="--preview-file").and_then(|i|args.get(i+1)).map(PathBuf::from);
            preview::run(path)
        },
        _=>Err("Tiện ích không hợp lệ".into()),
    };
    if let Err(error)=result {message(&error);}
    unsafe {if !mutex.is_null(){CloseHandle(mutex);} windows::Win32::System::Com::CoUninitialize();}
    true
}
pub fn message(text:&str) {unsafe {MessageBoxW(std::ptr::null_mut(),wide(text).as_ptr(),wide("Deskland — Tiện ích").as_ptr(),MB_OK|MB_ICONINFORMATION);}}
pub fn output_path(kind:&str,ext:&str)->Result<PathBuf,String> {
    let base=std::env::var("USERPROFILE").map(PathBuf::from).map_err(|e|e.to_string())?;
    let dir=base.join(if kind=="Screenshot"{"Pictures"}else{"Videos"}).join("Deskland");
    std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?;
    Ok(dir.join(format!("{kind}-{}-{}.{ext}",now.as_millis(),std::process::id())))
}
pub unsafe fn loop_messages() {
    let mut msg:MSG=std::mem::zeroed();
    while GetMessageW(&mut msg,std::ptr::null_mut(),0,0)>0 {TranslateMessage(&msg);DispatchMessageW(&msg);}
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn tool_options_roundtrip_and_reject_invalid_shortcuts(){let o=Options::default();assert_eq!(Options::from_json(&o.json()),o);let v=serde_json::json!({"fps":999,"capture":{"mods":0,"vk":0}});assert_eq!(Options::from_json(&v),o);}
}
