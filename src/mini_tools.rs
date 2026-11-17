//! On-demand native screenshot and desktop recorder tools.
mod screen;
mod editor;
mod recorder;

use crate::desktop::wide;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const CAPTURE_ID: i32 = 31;
pub const RECORD_ID: i32 = 32;
pub fn stage_hotkey_capture() -> bool { screen::stage_hotkey_capture() }
pub fn hotkey_text(mods: u32, vk: u32) -> String {
    let mut parts: Vec<String> = Vec::new();
    if mods & MOD_CONTROL != 0 { parts.push("Ctrl".into()); }
    if mods & MOD_ALT != 0 { parts.push("Alt".into()); }
    if mods & MOD_SHIFT != 0 { parts.push("Shift".into()); }
    let key = match vk {
        x if x == VK_SPACE as u32 => "Space".to_string(),
        x if x == VK_RETURN as u32 => "Enter".to_string(),
        x if (b'A' as u32..=b'Z' as u32).contains(&x) => char::from_u32(x).unwrap().to_string(),
        x if (b'0' as u32..=b'9' as u32).contains(&x) => char::from_u32(x).unwrap().to_string(),
        x if (0x70..=0x7B).contains(&x) => format!("F{}", x - 0x6F),
        _ => format!("VK 0x{vk:X}"),
    };
    parts.push(key);
    parts.join("+")
}

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub enabled: bool,
    pub capture_mods: u32, pub capture_key: u32,
    pub record_mods: u32, pub record_key: u32,
    pub fps: u32, pub audio: bool,
}
impl Default for Options {
    fn default() -> Self { Self { enabled:true,
        capture_mods:MOD_CONTROL|MOD_SHIFT|MOD_NOREPEAT,capture_key:b'S' as u32,
        record_mods:MOD_CONTROL|MOD_SHIFT|MOD_NOREPEAT,record_key:b'R' as u32,
        fps:30,audio:true } }
}
impl Options {
    pub fn from_json(v:&serde_json::Value)->Self {
        let mut o=Self::default();
        o.enabled=v["enabled"].as_bool().unwrap_or(o.enabled);
        o.audio=v["audio"].as_bool().unwrap_or(o.audio);
        o.fps=if v["fps"].as_u64()==Some(60){60}else{30};
        for (name,mods,key) in [("capture",&mut o.capture_mods,&mut o.capture_key),("record",&mut o.record_mods,&mut o.record_key)] {
            let m=v[name]["mods"].as_u64().unwrap_or(*mods as u64) as u32;
            let k=v[name]["vk"].as_u64().unwrap_or(*key as u64) as u32;
            if m&(MOD_CONTROL|MOD_ALT|MOD_SHIFT)!=0 && valid_key(k) {
                *mods=(m&(MOD_CONTROL|MOD_ALT|MOD_SHIFT))|MOD_NOREPEAT; *key=k;
            }
        }
        o
    }
    pub fn json(&self)->serde_json::Value { serde_json::json!({"enabled":self.enabled,"fps":self.fps,"audio":self.audio,
        "capture":{"mods":self.capture_mods,"vk":self.capture_key},"record":{"mods":self.record_mods,"vk":self.record_key}}) }
}
fn valid_key(k: u32) -> bool {
    k == VK_SPACE as u32 || k == VK_RETURN as u32 || k == VK_TAB as u32 ||
        (0x30..=0x39).contains(&k) || (0x41..=0x5a).contains(&k) || (0x60..=0x69).contains(&k) || (0x70..=0x7b).contains(&k) ||
        [VK_LEFT as u32, VK_UP as u32, VK_RIGHT as u32, VK_DOWN as u32, VK_HOME as u32, VK_END as u32, VK_PRIOR as u32, VK_NEXT as u32, VK_INSERT as u32, VK_DELETE as u32].contains(&k)
}
static MAIN_WINDOW:std::sync::atomic::AtomicIsize=std::sync::atomic::AtomicIsize::new(0);
pub fn register(hwnd:HWND, o:&Options)->[bool;2] {
    unsafe {
        MAIN_WINDOW.store(hwnd as isize,std::sync::atomic::Ordering::Relaxed);
        for id in [CAPTURE_ID,RECORD_ID] { UnregisterHotKey(hwnd,id); }
        if !o.enabled { return [false;2]; }
        [(CAPTURE_ID,o.capture_mods,o.capture_key),(RECORD_ID,o.record_mods,o.record_key)].map(|(id,m,k)| RegisterHotKey(hwnd,id,m,k)!=0)
    }
}
pub fn spawn(mode:&str, o:&Options) {
    spawn_inner(mode, o, None);
}
pub fn spawn_capture_staged(o:&Options) {
    spawn_inner("capture", o, screen::take_hotkey_capture());
}
fn spawn_inner(mode:&str, o:&Options, staged:Option<PathBuf>) {
    if mode=="record"{unsafe{let existing=FindWindowW(wide("DesklandRecordingPanel").as_ptr(),std::ptr::null());if !existing.is_null(){PostMessageW(existing,WM_CLOSE,0,0);return;}}}
    let result=std::env::current_exe().and_then(|exe| {
        let mut command=std::process::Command::new(exe);
        command
        .arg("--mini-tool").arg(mode).arg("--tool-fps").arg(o.fps.to_string())
        .arg(if o.audio{"--tool-audio"}else{"--tool-silent"});
        if let Some(path)=staged { command.arg("--capture-image").arg(path); }
        command.creation_flags(0x08000000).spawn()
    });
    if let Err(e)=result { message(&format!("Không mở được tiện ích: {e}")); }
}
/// Must run before wallpaper singleton, logging, WebView, timers or OpenGL.
pub fn dispatch(args:&[String])->bool {
    let Some(i)=args.iter().position(|a|a=="--mini-tool") else {return false;};
    let mode=args.get(i+1).map(String::as_str).unwrap_or("");
    unsafe {
        windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _=windows::Win32::System::Com::CoInitializeEx(None,windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
    }
    let mutex=unsafe { windows_sys::Win32::System::Threading::CreateMutexW(std::ptr::null(),0,wide(&format!("DesklandMiniTool_{mode}")).as_ptr()) };
    if unsafe {GetLastError()}==ERROR_ALREADY_EXISTS { unsafe { if !mutex.is_null(){CloseHandle(mutex);} } return true; }
    let staged= args.windows(2).find(|v|v[0]=="--capture-image").map(|v|PathBuf::from(&v[1]));
    let initial=staged.as_ref().and_then(|path| image::open(path).ok().map(|image|image.to_rgba8()));
    if let Some(path)=staged { let _=std::fs::remove_file(path); }
    let result=match mode {
        "capture"=>screen::select_with_image(false, initial).and_then(|selection| match selection {Some((image,rect))=>editor::run(editor::Document::new(screen::crop(&image,rect)),false),None=>Ok(())}),
        "record"=>screen::select(true).and_then(|selection|match selection {Some((image,rect))=>{drop(image);recorder::run(rect,if args.windows(2).any(|v|v[0]=="--tool-fps"&&v[1]=="60"){60}else{30},args.iter().any(|a|a=="--tool-audio"))},None=>Ok(())}),
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
pub unsafe fn loop_messages() { let mut msg:MSG=std::mem::zeroed(); while GetMessageW(&mut msg,std::ptr::null_mut(),0,0)>0 {TranslateMessage(&msg);DispatchMessageW(&msg);} }
#[cfg(test)] mod tests { use super::*; #[test] fn tool_options_roundtrip_and_reject_invalid_shortcuts(){let o=Options::default();assert_eq!(Options::from_json(&o.json()),o);let v=serde_json::json!({"fps":999,"capture":{"mods":0,"vk":0}});assert_eq!(Options::from_json(&v),o);} }
