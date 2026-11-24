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

fn system_identity(key: &str) -> String {
    // Exact Windows identities only. Never merge applications by a translated
    // display name, or merge an unrelated executable named calc.exe.
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let system=|name:&str| Path::new(&windir).join("System32").join(name).to_string_lossy().to_lowercase();
    match key {
        "shell:appsfolder\\microsoft.windowscalculator_8wekyb3d8bbwe!app" | "calc.exe" => system("calc.exe"),
        "shell:appsfolder\\windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel" | "ms-settings:" => "ms-settings:".into(),
        "explorer.exe" => Path::new(&windir).join("explorer.exe").to_string_lossy().to_lowercase(),
        "taskmgr.exe" => system("taskmgr.exe"),
        "control.exe" => system("control.exe"),
        _ => key.to_owned(),
    }
}

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
        .unwrap_or_else(||system_identity(&key))
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

#[derive(Debug,PartialEq,Eq)]
enum TargetKind { Application, System, Document, Website, Helper, Unknown }

fn target_kind(target:&str, arguments:&str)->TargetKind {
    let target=target.to_lowercase();
    if target.starts_with("https:") || target.starts_with("http:") { return TargetKind::Website; }
    if target=="ms-settings:" { return TargetKind::Application; }
    let path=Path::new(&target);
    let ext=path.extension().and_then(|s|s.to_str()).unwrap_or("");
    if matches!(ext,"msc"|"cpl") { return TargetKind::System; }
    if ext=="appref-ms" { return TargetKind::Application; }
    if ext!="exe" { return if ext.is_empty(){TargetKind::Unknown}else{TargetKind::Document}; }
    let stem=path.file_stem().and_then(|s|s.to_str()).unwrap_or("");
    let base=stem.trim_end_matches(|c:char|c.is_ascii_digit());
    let args=arguments.to_lowercase();
    if matches!(base,"uninstall"|"uninstaller"|"unins"|"uninst"|"update"|"updater"|"javacpl"|"setup"|"installer"|"msiexec")
        || base.ends_with("uninstaller") || base.ends_with("_uninstall") || base.ends_with("_updater")
        || (stem=="javaws" && args.split_whitespace().any(|s|s=="-viewer" || s=="-about"))
    { return TargetKind::Helper; }
    if stem=="hh" {return TargetKind::Document;}
    if stem=="rundll32" {return TargetKind::System;}
    if stem=="explorer" && !args.trim().is_empty() && args.trim()!="ms-settings:" {
        return TargetKind::System;
    }
    TargetKind::Application
}

struct LinkMetadata { target:String, args:String, directory:String, aumid:Option<String> }
unsafe fn link_metadata(path:&Path)->Option<LinkMetadata> {
    use windows::{core::{GUID,Interface,PCWSTR},Win32::{Foundation::PROPERTYKEY,System::Com::{CoCreateInstance,CLSCTX_INPROC_SERVER,IPersistFile,STGM_READ,CoTaskMemFree,StructuredStorage::{PropVariantClear,PropVariantToStringAlloc}},UI::Shell::{IShellLinkW,ShellLink,PropertiesSystem::IPropertyStore}}};
    let link:IShellLinkW=CoCreateInstance(&ShellLink,None,CLSCTX_INPROC_SERVER).ok()?;
    link.cast::<IPersistFile>().ok()?.Load(PCWSTR(wide(&path.to_string_lossy()).as_ptr()),STGM_READ).ok()?;
    let read=|buf:&[u16]| String::from_utf16_lossy(&buf[..buf.iter().position(|&c|c==0).unwrap_or(buf.len())]);
    let mut target=vec![0u16;32768];
    let mut args=vec![0u16;32768];
    let mut directory=vec![0u16;32768];
    let _=link.GetPath(&mut target,std::ptr::null_mut(),0);
    link.GetArguments(&mut args).ok()?;
    let _=link.GetWorkingDirectory(&mut directory);
    const AUMID:PROPERTYKEY=PROPERTYKEY{fmtid:GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),pid:5};
    let aumid=link.cast::<IPropertyStore>().ok().and_then(|store| {
        let mut value=store.GetValue(&AUMID).ok()?;
        let id=PropVariantToStringAlloc(&value).ok().and_then(|raw|{
            let result=raw.to_string().ok(); CoTaskMemFree(Some(raw.0.cast())); result
        });
        let _=PropVariantClear(&mut value); id
    }).filter(|id|id.len()<=350 && id.split_once('!').is_some_and(|(a,b)|[a,b].iter().all(|s|!s.is_empty()&&s.bytes().all(|v|v.is_ascii_alphanumeric()||b"_.-".contains(&v)))));
    Some(LinkMetadata{target:read(&target),args:read(&args),directory:read(&directory),aumid})
}

fn normalize(rows: Vec<Entry>) -> Vec<Entry> {
    let mut aliases = HashMap::new();
    let mut labels = HashMap::new();
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    let mut launches=HashMap::<String,String>::new();
    for mut entry in rows {
        let path = entry.path.to_string_lossy();
        let Some(raw) = crate::app_icons::normalized(&path) else {
            continue;
        };
        let mut id = system_identity(&crate::app_icons::key(&path).unwrap_or_else(|| raw.clone()));
        // Only automatic discovery is filtered. Manual entries and pins are
        // merged later, retaining the owner's original launch target.
        if entry.kind=="Application" && raw.ends_with(".lnk") {
            let Some(link)=(unsafe{link_metadata(&entry.path)}) else { continue; };
            if link.target.is_empty() && link.args.is_empty() && link.aumid.is_some() {
                let package=format!("shell:AppsFolder\\{}",link.aumid.unwrap());
                id=system_identity(&crate::app_icons::normalized(&package).unwrap_or_else(||raw.clone()));
                entry.path=package.into();
            } else { match target_kind(&link.target,&link.args) {
                TargetKind::Application => {
                    let target=crate::app_icons::normalized(&link.target).unwrap_or_default();
                    if target==system_identity("explorer.exe") && link.args.trim().eq_ignore_ascii_case("ms-settings:") {
                        id="ms-settings:".into();
                    } else if link.args.is_empty() && Path::new(&link.target).is_absolute() {
                        id=system_identity(&crate::app_icons::normalized(&link.target).unwrap_or_else(||raw.clone()));
                    } else if !link.target.is_empty() {
                        let fingerprint=serde_json::json!([link.target.to_lowercase(),link.args,link.directory.to_lowercase()]).to_string();
                        if let Some(previous)=launches.get(&fingerprint){id=previous.clone();}else{launches.insert(fingerprint,id.clone());}
                    }
                }
                TargetKind::System => entry.kind="System",
                TargetKind::Document => entry.kind="File",
                TargetKind::Website => entry.kind="Web shortcut",
                TargetKind::Helper | TargetKind::Unknown => continue,
            } }
        } else if entry.kind=="Application" && raw.ends_with(".exe") && target_kind(&raw,"")==TargetKind::Helper {
            continue;
        }
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
        let mut packaged=unsafe { installed_apps() };
        packaged.append(&mut apps);
        apps=packaged;
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
    #[test]
    fn automatic_targets_classify_the_resolved_file_not_the_label() {
        assert_eq!(target_kind(r"C:\Apps\LDUninstaller9.exe",""),TargetKind::Helper);
        assert_eq!(target_kind(r"C:\Apps\Uninstall Manager.exe",""),TargetKind::Application);
        assert_eq!(target_kind(r"C:\Apps\manual.pdf",""),TargetKind::Document);
        assert_eq!(target_kind(r"C:\Windows\System32\eventvwr.msc",""),TargetKind::System);
        assert_eq!(target_kind(r"C:\Apps\App.exe","--profile work"),TargetKind::Application);
        assert_eq!(system_identity("shell:appsfolder\\microsoft.windowscalculator_8wekyb3d8bbwe!app"),system_identity("calc.exe"));
    }

    #[test]
    fn shell_links_are_resolved_and_argument_variants_are_preserved() {
        use windows::{core::{Interface,PCWSTR},Win32::System::Com::{CoCreateInstance,CoInitializeEx,CoUninitialize,CLSCTX_INPROC_SERVER,COINIT_APARTMENTTHREADED,IPersistFile},Win32::UI::Shell::{IShellLinkW,ShellLink}};
        let dir=std::env::temp_dir().join(format!("tlauncher-catalog-{}",std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let app=std::env::current_exe().unwrap();
        let doc=dir.join("help.txt");std::fs::write(&doc,"fixture").unwrap();
        let rows=unsafe {
            CoInitializeEx(None,COINIT_APARTMENTTHREADED).ok().unwrap();
            let mut rows=Vec::new();
            for (name,target,args) in [("App A",&app,""),("Alias A",&app,""),("Profile A",&app,"--profile A"),("Alias profile",&app,"--profile A"),("Profile B",&app,"--profile B"),("Documentation",&doc,"")] {
                let file=dir.join(format!("{name}.lnk"));
                let link:IShellLinkW=CoCreateInstance(&ShellLink,None,CLSCTX_INPROC_SERVER).unwrap();
                link.SetPath(PCWSTR(wide(&target.to_string_lossy()).as_ptr())).unwrap();
                link.SetArguments(PCWSTR(wide(args).as_ptr())).unwrap();
                link.SetWorkingDirectory(PCWSTR(wide(&dir.to_string_lossy()).as_ptr())).unwrap();
                link.cast::<IPersistFile>().unwrap().Save(PCWSTR(wide(&file.to_string_lossy()).as_ptr()),true).unwrap();
                rows.push(Entry{name:name.into(),path:file,kind:"Application",detail:String::new()});
            }
            let result=normalize(rows);
            CoUninitialize();result
        };
        assert_eq!(rows.iter().filter(|r|app_entry(r)).count(),3);
        assert_eq!(rows.iter().filter(|r|r.kind=="File").count(),1);
        for item in std::fs::read_dir(&dir).unwrap(){std::fs::remove_file(item.unwrap().path()).unwrap();}
        std::fs::remove_dir(&dir).unwrap();
    }
}
