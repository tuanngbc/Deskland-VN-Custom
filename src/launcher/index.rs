//! The Windows Search provider runs only on the existing Spotlight query worker.
use super::{Entry, PathBuf};

pub(super) const FILE: &str = "Indexed file";
pub(super) const FOLDER: &str = "Indexed folder";
pub(super) fn is_indexed(entry: &Entry) -> bool { matches!(entry.kind, FILE | FOLDER) }

pub(super) fn eligible(query: &str) -> bool {
    !cfg!(test) && cfg!(target_env="msvc") && (2..=80).contains(&query.trim().chars().take(81).count()) &&
        query.chars().any(char::is_alphabetic) &&
        !query.chars().any(char::is_control)
}

pub(super) fn search(query: &str) -> Vec<Entry> {
    let mut rows = Vec::new();
    #[cfg(target_env="msvc")]
    {
        if !eligible(query) { return rows; }
        use std::ffi::c_void;
        use windows::{core::PCWSTR, Win32::System::Com::*};
        type Report = unsafe extern "C" fn(*mut c_void, *const u16, *const u16, *const u16, i32);
        extern "C" {
            fn tlauncher_fluidwin_index(query: *const u16, context: *mut c_void, report: Report);
        }
        unsafe extern "C" fn collect(ctx: *mut c_void, name: *const u16, detail: *const u16, path: *const u16, folder: i32) {
            let (Ok(name), Ok(path)) = (PCWSTR(name).to_string(), PCWSTR(path).to_string()) else { return; };
            let path = PathBuf::from(path);
            if name.is_empty() || !path.is_absolute() { return; }
            let rows = &mut *ctx.cast::<Vec<Entry>>();
            if rows.len() >= 48 { return; }
            rows.push(Entry { name, path, kind: if folder != 0 { FOLDER } else { FILE },
                detail: PCWSTR(detail).to_string().unwrap_or_default() });
        }
        unsafe {
            if CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() {
                tlauncher_fluidwin_index(crate::desktop::wide(query).as_ptr(), (&mut rows as *mut Vec<Entry>).cast(), collect);
                CoUninitialize();
            }
        }
    }
    rows
}
