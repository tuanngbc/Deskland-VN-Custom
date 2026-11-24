//! One lazy shell discovery worker for Start and both Spotlight views.
use super::*;
use std::collections::{HashMap, HashSet};
use std::sync::{mpsc, Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(180);
type Rows = Arc<[Entry]>;

#[derive(Default)]
struct Cache {
    apps: Option<Rows>,
    files: Option<Rows>,
    pending: Vec<(bool, mpsc::Sender<Vec<Entry>>)>,
    discovery: bool,
    refreshed: Option<Instant>,
    revision: u64,
}
static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(|| Mutex::new(Cache::default()));
static ALIASES: LazyLock<std::sync::RwLock<HashMap<String, String>>> =
    LazyLock::new(Default::default);
static LABELS: LazyLock<std::sync::RwLock<HashMap<String, String>>> =
    LazyLock::new(Default::default);

pub(super) fn identity(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(c) = crate::dock::command_pin::decode(&value) {
        // Labels are editable. Keep parameters case-sensitive; only Windows
        // paths can be folded. Serialize tuple to avoid delimiter collisions.
        return serde_json::json!([
            "command",
            c.exe.to_lowercase(),
            c.parameters,
            c.directory.to_lowercase()
        ])
        .to_string();
    }
    let key = crate::app_icons::normalized(&value).unwrap_or_default();
    ALIASES
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .cloned()
        .unwrap_or(key)
}
pub(super) fn label(path: &Path) -> Option<String> {
    LABELS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(&identity(path))
        .cloned()
}
pub(super) fn invalidate() {
    let mut state = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    state.refreshed = None;
    state.revision = state.revision.wrapping_add(1);
}

/// Subscribe only while the library is open. Shell notifications are hints;
/// the TTL and explicit refresh also cover providers that publish no event.
pub(super) struct ShellWatch(Vec<u32>);

impl ShellWatch {
    pub(super) unsafe fn new(hwnd: HWND, message: u32) -> Self {
        use windows::{
            core::BOOL,
            Win32::{Foundation::HWND as ShellHwnd, System::Com::CoTaskMemFree, UI::Shell::*},
        };
        let mut ids = Vec::new();
        if cfg!(test) {
            return Self(ids);
        }
        let mask = (SHCNE_RENAMEITEM
            | SHCNE_CREATE
            | SHCNE_DELETE
            | SHCNE_MKDIR
            | SHCNE_RMDIR
            | SHCNE_RENAMEFOLDER
            | SHCNE_UPDATEITEM
            | SHCNE_UPDATEDIR)
            .0 as i32;
        for (folder, source, recursive) in [
            (
                FOLDERID_Programs,
                SHCNRF_ShellLevel | SHCNRF_InterruptLevel | SHCNRF_RecursiveInterrupt,
                1,
            ),
            (
                FOLDERID_CommonPrograms,
                SHCNRF_ShellLevel | SHCNRF_InterruptLevel | SHCNRF_RecursiveInterrupt,
                1,
            ),
            (FOLDERID_AppsFolder, SHCNRF_ShellLevel, 0),
        ] {
            if let Ok(item) = SHGetKnownFolderIDList(&folder, 0, None) {
                let entry = SHChangeNotifyEntry {
                    pidl: item,
                    fRecursive: BOOL(recursive),
                };
                let registration = SHChangeNotifyRegister(
                    ShellHwnd(hwnd),
                    source | SHCNRF_NewDelivery,
                    mask,
                    message,
                    1,
                    &entry,
                );
                if registration != 0 {
                    ids.push(registration);
                }
                CoTaskMemFree(Some(item.cast()));
            }
        }
        Self(ids)
    }
}

impl Drop for ShellWatch {
    fn drop(&mut self) {
        for id in self.0.drain(..) {
            unsafe {
                SHChangeNotifyDeregister(id);
            }
        }
    }
}

fn normalize(rows: Vec<Entry>) -> Vec<Entry> {
    let mut aliases = HashMap::new();
    let mut labels = HashMap::new();
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for entry in rows {
        let path = entry.path.to_string_lossy();
        let Some(raw) = crate::app_icons::normalized(&path) else {
            continue;
        };
        let id = crate::app_icons::key(&path).unwrap_or_else(|| raw.clone());
        if raw != id {
            aliases.insert(raw, id.clone());
        }
        if seen.insert(id.clone()) {
            if app_entry(&entry) {
                labels.insert(id, entry.name.clone());
            }
            result.push(entry);
        }
    }
    *ALIASES.write().unwrap_or_else(|e| e.into_inner()) = aliases;
    *LABELS.write().unwrap_or_else(|e| e.into_inner()) = labels;
    result.sort_by_cached_key(|e| e.name.to_lowercase());
    result
}

fn local_files() -> Vec<Entry> {
    if cfg!(test) || crate::trial::active() {
        return Vec::new();
    }
    let mut files = Vec::new();
    if let Ok(base) = std::env::var("USERPROFILE") {
        for folder in [
            "Desktop",
            "Documents",
            "Downloads",
            "OneDrive\\Desktop",
            "OneDrive\\Documents",
        ] {
            enum_dir(&PathBuf::from(&base).join(folder), 2, true, &mut files);
        }
    }
    for file in &mut files {
        if file.kind == "Application" {
            file.kind = "File";
        }
    }
    let mut seen = HashSet::new();
    files.retain(|e| seen.insert(identity(&e.path)));
    files.sort_by_cached_key(|e| e.name.to_lowercase());
    files
}

fn deliver(apps: &Rows, files: Option<&Rows>, include_files: bool) -> Vec<Entry> {
    let mut rows = apps
        .iter()
        .filter(|e| include_files || app_entry(e))
        .cloned()
        .collect::<Vec<_>>();
    if include_files {
        if let Some(files) = files {
            let mut seen = rows
                .iter()
                .map(|e| identity(&e.path))
                .collect::<HashSet<_>>();
            rows.extend(
                files
                    .iter()
                    .filter(|e| seen.insert(identity(&e.path)))
                    .cloned(),
            );
        }
    }
    rows
}

pub(super) fn snapshot(include_files: bool) -> Vec<Entry> {
    let state = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    state.apps.as_ref().map_or_else(builtin_apps, |a| {
        deliver(a, state.files.as_ref(), include_files)
    })
}

pub(super) fn request(include_files: bool, force: bool) -> Option<mpsc::Receiver<Vec<Entry>>> {
    let (tx, rx) = mpsc::channel();
    let mut state = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if !force && state.refreshed.is_some_and(|time| time.elapsed() < TTL) {
        if let Some(apps) = &state.apps {
            let _ = tx.send(deliver(apps, state.files.as_ref(), include_files));
            return Some(rx);
        }
    }
    if state.pending.len() >= 16 {
        return None;
    }
    state.pending.push((include_files, tx));
    if !state.discovery {
        state.discovery = true;
        let revision = state.revision;
        if std::thread::Builder::new()
            .name("tlauncher-application-catalog".into())
            .spawn(move || refresh(revision))
            .is_err()
        {
            state.discovery = false;
            state.pending.clear();
            return None;
        }
    }
    Some(rx)
}

fn refresh(revision: u64) {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    let mut apps = app_entries();
    // Keep the user's Start Menu link when it has a distinct launch target.
    let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if com.is_ok() && !cfg!(test) {
        apps.extend(unsafe { installed_apps() });
    }
    let apps: Rows = normalize(apps).into();
    if com.is_ok() {
        unsafe { CoUninitialize() };
    }
    {
        let mut state = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        state.apps = Some(apps.clone());
        state.pending.retain(|(files, tx)| {
            if !files {
                let _ = tx.send(deliver(&apps, None, false));
            }
            *files
        });
    }
    let files: Rows = local_files().into();
    let mut state = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    state.files = Some(files.clone());
    state.refreshed = (state.revision == revision).then(Instant::now);
    state.discovery = false;
    for (with_files, tx) in state.pending.drain(..) {
        let _ = tx.send(deliver(&apps, Some(&files), with_files));
    }
}

pub(super) fn search_rank(name: &str, query: &str) -> Option<u8> {
    let name = name.to_lowercase();
    if name == query {
        Some(0)
    } else if name.starts_with(query) {
        Some(1)
    } else if query.split_whitespace().all(|word| name.contains(word)) {
        Some(2)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_does_not_confuse_equal_display_names() {
        let make = |p: &str| Entry {
            name: "Ứng dụng".into(),
            path: p.into(),
            kind: "Application",
            detail: String::new(),
        };
        let found = normalize(vec![
            make(r"C:\Apps\One.exe"),
            make(r"c:\apps\one.EXE"),
            make(r"C:\Apps\Two.exe"),
            make(r"shell:AppsFolder\Company.App_abc!App"),
        ]);
        assert_eq!(found.len(), 3);
        assert_eq!(search_rank("Ứng dụng Mới", "ứng mới"), Some(2));
        assert_eq!(search_rank("Ứng dụng Mới", "ứng dụng"), Some(1));
        assert_eq!(search_rank("Ứng dụng Mới", "khác"), None);
    }
}
