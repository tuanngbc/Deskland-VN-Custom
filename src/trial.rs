//! Explicit owner smoke-test isolation. Never selects or copies the real profile.
//! Keep the normal singleton: production and a trial must not run concurrently.
use std::{path::{Path, PathBuf}, sync::OnceLock};

static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
static BENCHMARK: OnceLock<bool> = OnceLock::new();
const MARKER: &str = ".tuanpc-desk-trial";
const MARKER_CONTENT: &str = "tuanpc-desk isolated native smoke profile v1\n";

pub fn active() -> bool { DIRECTORY.get().is_some() }
pub fn data_dir() -> Option<&'static Path> { DIRECTORY.get().map(PathBuf::as_path) }

pub fn benchmark_active() -> bool {
    let Some(dir) = data_dir() else { return false; };
    *BENCHMARK.get_or_init(|| std::fs::read_to_string(dir.join("dock-benchmark.enable"))
        .is_ok_and(|flag| flag.trim() == "tuanpc-desk dock benchmark v1"))
}

pub fn benchmark_coverage(screens: &[windows_sys::Win32::Foundation::RECT], covered: &[bool]) {
    if !benchmark_active() { return; }
    thread_local! { static LAST: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) }; }
    let mut outputs = screens.iter().zip(covered).map(|(r, c)| (r.left, r.top, r.right, r.bottom, *c)).collect::<Vec<_>>();
    outputs.sort_unstable();
    let line = outputs.into_iter().map(|(l,t,r,b,c)| format!("{l},{t},{r},{b}:{}", u8::from(c)))
        .collect::<Vec<_>>().join(";");
    LAST.with(|last| {
        if last.borrow().as_ref() != Some(&line) {
            crate::log!("DOCK_BENCH_COVERAGE {line}");
            last.replace(Some(line));
        }
    });
}

fn validate(path: &Path, production: &Path) -> Result<(), String> {
    if !path.is_absolute() || path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("Trial directory must be an absolute path without '..'".into());
    }
    if !path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("tuanpc-desk-trial-")) {
        return Err("Trial directory name must start with tuanpc-desk-trial-".into());
    }
    if path.components().any(|c| c.as_os_str().to_string_lossy().eq_ignore_ascii_case(".deskland-rs"))
        || path.starts_with(production) || production.starts_with(path) {
        return Err("Trial directory must be separate from the production profile".into());
    }
    Ok(())
}

pub fn initialize(args: &[String]) -> Result<(), String> {
    let values: Vec<_> = args.iter().enumerate().filter(|(_, s)| s.as_str() == "--trial-dir").collect();
    if values.is_empty() { return Ok(()); }
    if values.len() != 1 { return Err("Only one --trial-dir is allowed".into()); }
    let value = args.get(values[0].0 + 1).filter(|s| !s.starts_with("--"))
        .ok_or("--trial-dir requires a directory")?;
    let path = PathBuf::from(value);
    let production = PathBuf::from(std::env::var_os("USERPROFILE").ok_or("USERPROFILE is missing")?).join(".deskland-rs");
    validate(&path, &production)?;
    // Refuse reuse of unrelated data, even if the directory has a plausible name.
    if path.exists() && std::fs::read_dir(&path).map_err(|e| e.to_string())?.next().is_some()
        && std::fs::read_to_string(path.join(MARKER)).ok().as_deref() != Some(MARKER_CONTENT) {
        return Err("Nonempty trial directory has no valid ownership marker".into());
    }
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    let resolved = path.canonicalize().map_err(|e| e.to_string())?;
    let production = production.canonicalize().unwrap_or(production);
    validate(&resolved, &production)?; // Resolve junctions before selecting storage.
    std::fs::write(resolved.join(MARKER), MARKER_CONTENT).map_err(|e| e.to_string())?;
    DIRECTORY.set(resolved).map_err(|_| "Trial already initialized".to_string())
}

pub fn protect_windows() -> Result<(), String> {
    if active() { Err("Trial mode: Windows Startup, saver, lock and wallpaper changes are blocked".into()) }
    else { Ok(()) }
}

/// Invoked by the built release EXE in CI, not a mock of the persistence guards.
pub fn verify_guards() -> Result<(), String> {
    if !active() { return Err("--trial-isolation-check requires --trial-dir".into()); }
    use windows_sys::Win32::System::Registry::HKEY_CURRENT_USER;
    if protect_windows().is_ok() { return Err("Trial guard is inactive".into()); }
    crate::settings::migrate_autostart();
    crate::settings::set_autostart(true);
    crate::settings::set_autostart(false);
    if crate::lock::reg_set_string(HKEY_CURRENT_USER, r"Software\tuanpc-desk-TrialProbe", "blocked", "1")
        || crate::lock::reg_delete_value(HKEY_CURRENT_USER, r"Software\tuanpc-desk-TrialProbe", "blocked")
        || crate::lock::restore_winl()
        || crate::lock::install_saver(1).is_ok()
        || crate::lock::uninstall_saver().is_ok()
        || crate::lock::preview_saver().is_ok() {
        return Err("A Windows persistence boundary accepted a trial write".into());
    }
    crate::lock::reconcile_disabled_saver()?;
    crate::lock::lock_now();
    crate::lock::start_secure_saver(std::ptr::null_mut());
    crate::lock::sync_lock_image(&data_dir().unwrap().join("absent.bmp"));
    crate::lock::set_static_wallpaper(data_dir().unwrap().join("absent.bmp"));
    crate::config_store::save(&data_dir().unwrap().join("settings.json"), serde_json::json!({"trialProbe":true})).map_err(|e| e.to_string())?;
    std::fs::write(data_dir().unwrap().join("ISOLATION-PASS.txt"), "Windows persistence guards exercised by release EXE\n").map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_production_relative_parent_and_unscoped_paths() {
        let prod = Path::new(r"C:\Users\owner\.deskland-rs");
        for p in [r"relative\tuanpc-desk-trial-test", r"C:\Users\owner\.deskland-rs\tuanpc-desk-trial-test",
            r"C:\tuanpc-desk-trial-test\..\other", r"C:\Users\owner", r"C:\random"] {
            assert!(validate(Path::new(p), prod).is_err(), "{p}");
        }
        assert!(validate(Path::new(r"E:\D9-Test\tuanpc-desk-trial-profile"), prod).is_ok());
    }
}
