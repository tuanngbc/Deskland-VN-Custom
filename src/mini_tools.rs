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
pub fn hotkey_text(mods: u32, vk: u32) -> String { crate::shortcuts::label(mods,vk) }

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
            if crate::shortcuts::normalize(m,k).is_some() {
                *mods=(m&(MOD_CONTROL|MOD_ALT|MOD_SHIFT))|MOD_NOREPEAT; *key=k;
            }
        }
        o
    }
    pub fn json(&self)->serde_json::Value { serde_json::json!({"enabled":self.enabled,"fps":self.fps,"audio":self.audio,
        "capture":{"mods":self.capture_mods,"vk":self.capture_key},"record":{"mods":self.record_mods,"vk":self.record_key}}) }
}
static MAIN_WINDOW:std::sync::atomic::AtomicIsize=std::sync::atomic::AtomicIsize::new(0);
pub fn register(hwnd:HWND, o:&Options)->[bool;2] {
    unsafe {
        MAIN_WINDOW.store(hwnd as isize,std::sync::atomic::Ordering::Relaxed);
        for id in [CAPTURE_ID,RECORD_ID] { UnregisterHotKey(hwnd,id); }
        if !o.enabled { return [false;2]; }
        [(CAPTURE_ID,o.capture_mods,o.capture_key,"Chụp"),(RECORD_ID,o.record_mods,o.record_key,"Quay")].map(|(id,m,k,name)| {
            if crate::shortcuts::normalize(m,k).is_none() {
                crate::log!("Phím tắt {name}: tổ hợp không hợp lệ (mods={m}, vk={k})");
                return false;
            }
            let ok = RegisterHotKey(hwnd,id,m,k)!=0;
            if ok {
                crate::log!("Phím tắt {name}: đã đăng ký {}", hotkey_text(m,k));
            } else {
                crate::log!("Phím tắt {name}: đăng ký thất bại {} (GetLastError={})", hotkey_text(m,k), GetLastError());
            }
            ok
        })
    }
}
pub fn spawn(mode:&str, o:&Options) {
    spawn_inner(mode, o, None);
}
pub fn spawn_capture_staged(o:&Options) {
    spawn_inner("capture", o, screen::take_hotkey_capture());
}
fn spawn_inner(mode:&str, o:&Options, staged:Option<PathBuf>) {
    if crate::trial::benchmark_active() || crate::trial::demo_active() {
        discard_staged(staged);return;
    }
    if mode=="record"{unsafe{let existing=FindWindowW(wide("tuanpc-deskRecordingPanel").as_ptr(),std::ptr::null());if !existing.is_null(){PostMessageW(existing,WM_CLOSE,0,0);return;}}}
    let result=std::env::current_exe().and_then(|exe| {
        let mut command=std::process::Command::new(exe);
        command
        .arg("--mini-tool").arg(mode).arg("--tool-fps").arg(o.fps.to_string())
        .arg(if o.audio{"--tool-audio"}else{"--tool-silent"});
        if let Some(path)=crate::trial::data_dir(){
            command.arg("--trial-dir").arg(path).arg("--trial-mini-owner").arg(std::process::id().to_string());
        }
        if let Some(path)=&staged { command.arg("--capture-image").arg(path); }
        command.creation_flags(0x08000000).spawn()
    });
    if let Err(e)=result { discard_staged(staged);message(&format!("Không mở được tiện ích: {e}")); }
}
fn discard_staged(staged:Option<PathBuf>){
    if let Some(path)=staged{let _=std::fs::remove_file(path);}
}
fn supervise_trial_owner(owner:u32)->bool{
    use windows_sys::Win32::System::Threading::{OpenProcess,QueryFullProcessImageNameW,WaitForSingleObject,PROCESS_QUERY_LIMITED_INFORMATION};
    if owner==0||owner==std::process::id(){return false;}
    unsafe{
        let handle=OpenProcess(0x00100000|PROCESS_QUERY_LIMITED_INFORMATION,0,owner);
        if handle.is_null(){return false;}
        let mut buf=[0u16;1024];let mut n=buf.len() as u32;
        let valid=QueryFullProcessImageNameW(handle,0,buf.as_mut_ptr(),&mut n)!=0
            && std::env::current_exe().is_ok_and(|exe|exe.to_string_lossy().eq_ignore_ascii_case(&String::from_utf16_lossy(&buf[..n as usize])))
            && WaitForSingleObject(handle,0)==WAIT_TIMEOUT;
        if !valid{CloseHandle(handle);return false;}
        let handle_value=handle as usize;
        if std::thread::Builder::new().name("deskland-mini-owner".into()).spawn(move||{
            let handle=handle_value as HANDLE;
            WaitForSingleObject(handle,u32::MAX);CloseHandle(handle);
            std::process::exit(0);
        }).is_err(){CloseHandle(handle);return false;}
        true
    }
}
/// Must run before wallpaper singleton, logging, WebView, timers or OpenGL.
pub fn dispatch(args:&[String])->bool {
    let Some(i)=args.iter().position(|a|a=="--mini-tool") else {return false;};
    let trial_owner=args.windows(2).find(|w|w[0]=="--trial-mini-owner").and_then(|w|w[1].parse::<u32>().ok());
    if crate::trial::active() && (crate::trial::benchmark_active() || crate::trial::demo_active()
        || !trial_owner.is_some_and(supervise_trial_owner)){return true;}
    let Some(mode)=args.get(i+1).map(String::as_str).filter(|mode|matches!(*mode,"capture"|"record"))else{return true;};
    unsafe {
        windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _=windows::Win32::System::Com::CoInitializeEx(None,windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
    }
    let mutex=unsafe { windows_sys::Win32::System::Threading::CreateMutexW(std::ptr::null(),0,wide(&format!("tuanpc-deskMiniTool_{mode}")).as_ptr()) };
    if unsafe {GetLastError()}==ERROR_ALREADY_EXISTS { unsafe { if !mutex.is_null(){CloseHandle(mutex);} } return true; }
    let staged= args.windows(2).find(|v|v[0]=="--capture-image").map(|v|PathBuf::from(&v[1]))
        .filter(|p|trial_owner.is_none()||crate::trial::data_dir().is_some_and(|dir|trial_owner
            .is_some_and(|pid|p==&dir.join(format!("capture-hotkey-{pid}.png")))));
    let initial=staged.as_ref().and_then(|path| image::open(path).ok().map(|image|image.to_rgba8()));
    discard_staged(staged);
    let result=match mode {
        "capture"=>screen::select_with_image(false, initial).and_then(|selection| match selection {Some((image,rect))=>editor::run(editor::Document::new(screen::crop(&image,rect)),false),None=>Ok(())}),
        "record"=>screen::select(true).and_then(|selection|match selection {Some((image,rect))=>{drop(image);recorder::run(rect,if args.windows(2).any(|v|v[0]=="--tool-fps"&&v[1]=="60"){60}else{30},args.iter().any(|a|a=="--tool-audio"))},None=>Ok(())}),
        _=>Err("Tiện ích không hợp lệ".into()),
    };
    if let Err(error)=result {message(&error);}
    unsafe {if !mutex.is_null(){CloseHandle(mutex);} windows::Win32::System::Com::CoUninitialize();}
    true
}
pub fn message(text:&str) {unsafe {MessageBoxW(std::ptr::null_mut(),wide(text).as_ptr(),wide("TLauncher — Tiện ích").as_ptr(),MB_OK|MB_ICONINFORMATION);}}
pub fn output_path(kind:&str,ext:&str)->Result<PathBuf,String> {
    let dir=if let Some(trial)=crate::trial::data_dir(){trial.join("captures")}else{
        std::env::var("USERPROFILE").map(PathBuf::from).map_err(|e|e.to_string())?
            .join(if kind=="Screenshot"{"Pictures"}else{"Videos"}).join("TLauncher")
    };
    std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?;
    Ok(dir.join(format!("{kind}-{}-{}.{ext}",now.as_millis(),std::process::id())))
}
pub unsafe fn loop_messages() { let mut msg:MSG=std::mem::zeroed(); while GetMessageW(&mut msg,std::ptr::null_mut(),0,0)>0 {TranslateMessage(&msg);DispatchMessageW(&msg);} }
#[cfg(test)] mod tests { use super::*; #[test] fn tool_options_roundtrip_and_reject_invalid_shortcuts(){let o=Options::default();assert_eq!(Options::from_json(&o.json()),o);let v=serde_json::json!({"fps":999,"capture":{"mods":0,"vk":0}});assert_eq!(Options::from_json(&v),o);} }
