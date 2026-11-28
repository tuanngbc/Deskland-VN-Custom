#![windows_subsystem = "windows"]

// Only the on-demand Win32 tools are linked here. This executable never
// initializes the wallpaper, Dock, tray, startup tasks or legacy singleton.
#[path = "mini_tools/screen.rs"]
mod screen;
#[path = "mini_tools/editor.rs"]
mod editor;
#[path = "mini_tools/recorder.rs"]
mod recorder;

mod desktop {
    pub fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
}
mod lock {
    pub fn data_dir() -> std::path::PathBuf {
        std::env::var_os("TLAUNCHER_NATIVE_TRIAL_DIR").map(std::path::PathBuf::from).unwrap_or_default()
    }
}
use desktop::wide;
use std::path::PathBuf;
use std::sync::OnceLock;
use windows_sys::Win32::{Foundation::*, UI::WindowsAndMessaging::*};
static OUTPUT: OnceLock<PathBuf> = OnceLock::new();

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    let Some(mode) = args.get(1).and_then(|s| s.to_str()).filter(|s| matches!(*s, "capture" | "record")) else { return };
    let root = lock::data_dir();
    if !root.is_absolute()
        || !root.file_name().and_then(|s| s.to_str()).is_some_and(|s| s.starts_with("tlauncher-native-trial-"))
        || !root.is_dir() { return }
    let output = args.get(4).map(PathBuf::from).filter(|p| p.is_absolute() && p.is_dir()).unwrap_or_else(|| root.join("captures"));
    let _ = OUTPUT.set(output);
    let name = wide(&format!("Local\\TLauncher.Native.Capture.{}.{}", root.file_name().unwrap().to_string_lossy(), mode));
    unsafe {
        let mutex = windows_sys::Win32::System::Threading::CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if mutex.is_null() { return }
        if GetLastError() == ERROR_ALREADY_EXISTS { CloseHandle(mutex); return }
        windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let initialized = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED).is_ok();
        let result = if mode == "capture" {
            screen::select(false).and_then(|selection| match selection {
                Some((image, rect)) => editor::run(editor::Document::new(screen::crop(&image, rect)), false),
                None => Ok(()),
            })
        } else {
            let fps = if args.get(2).is_some_and(|s| s == "60") { 60 } else { 30 };
            let audio = args.get(3).is_some_and(|s| s == "audio");
            screen::select(true).and_then(|selection| match selection {
                Some((image, rect)) => { drop(image); recorder::run(rect, fps, audio) },
                None => Ok(()),
            })
        };
        if let Err(error) = result { message(&error); }
        if initialized { windows::Win32::System::Com::CoUninitialize(); }
        CloseHandle(mutex);
    }
}
fn output_path(kind: &str, ext: &str) -> Result<PathBuf, String> {
    let dir = OUTPUT.get().ok_or_else(|| "Capture folder unavailable".to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?;
    Ok(dir.join(format!("{kind}-{}-{}.{ext}", now.as_millis(), std::process::id())))
}
fn message(text: &str) {
    unsafe { MessageBoxW(std::ptr::null_mut(), wide(text).as_ptr(), wide("TLauncher").as_ptr(), MB_OK | MB_ICONINFORMATION); }
}
unsafe fn loop_messages() {
    let mut message: MSG = std::mem::zeroed();
    while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
        TranslateMessage(&message); DispatchMessageW(&message);
    }
}
