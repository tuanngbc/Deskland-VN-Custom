//! Logical layout and user metadata stored by the existing settings owner.
use super::{catalog, Entry};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct Preferences {
    pub enabled: bool,
    pub dock_fullscreen: bool,
    pub dock_follow_hotkey: bool,
    pub hotkey_fullscreen: bool,
    pub theme: String,
    pub icon_size: i32,
    pub spacing: i32,
    pub glass_strength: i32,
    pub glass_blur: i32,
    pub animations: bool,
    pub edge_wave: bool,
    pub show_names: bool,
    pub favorites_first: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            enabled: true,
            dock_fullscreen: true,
            dock_follow_hotkey: false,
            hotkey_fullscreen: false,
            theme: "system".into(),
            icon_size: 86,
            spacing: 18,
            glass_strength: 44,
            glass_blur: 48,
            animations: true,
            edge_wave: false,
            show_names: true,
            favorites_first: true,
        }
    }
}
impl Preferences {
    pub(super) fn fullscreen_for(&self, dock: bool) -> bool {
        if dock && !self.dock_follow_hotkey { self.dock_fullscreen } else { self.hotkey_fullscreen }
    }

    pub fn from_json(value: &serde_json::Value) -> Self {
        let mut out = serde_json::from_value::<Self>(value.clone()).unwrap_or_default();
        out.icon_size = out.icon_size.clamp(36, 96);
        out.spacing = out.spacing.clamp(8, 42);
        out.glass_strength = out.glass_strength.clamp(20, 48);
        out.glass_blur = out.glass_blur.clamp(24,64);
        if !matches!(out.theme.as_str(), "system" | "light" | "dark") {
            out.theme = "system".into();
        }
        out
    }
}

#[cfg(test)]
#[test]
fn activation_modes_are_independent_and_migrate_existing_dock_preferences() {
    let defaults=Preferences::from_json(&serde_json::json!({}));
    assert!(!defaults.fullscreen_for(false));
    assert!(defaults.fullscreen_for(true));
    let mut prefs=Preferences::from_json(&serde_json::json!({"dockFullscreen":false,"iconSize":74}));
    assert!(!prefs.fullscreen_for(true));
    prefs.hotkey_fullscreen=true;
    assert!(prefs.fullscreen_for(false));
    assert!(!prefs.fullscreen_for(true));
    prefs.dock_follow_hotkey=true;
    assert!(prefs.fullscreen_for(true));
    let restored=Preferences::from_json(&serde_json::to_value(&prefs).unwrap());
    assert_eq!(restored,prefs);
    assert_eq!(restored.icon_size,74);
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Library {
    manual: Vec<Manual>,
    meta: Vec<Metadata>,
    pub folders: Vec<Folder>,
    order: Vec<String>,
    serial: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Manual {
    name: String,
    launch: String,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Metadata {
    id: String,
    name: String,
    hidden: bool,
    favorite: bool,
    folder: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Folder {
    pub id: u64,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TileKey {
    App(String),
    Folder(u64),
}
impl TileKey {
    fn order_key(&self) -> String {
        match self {
            Self::App(id) => format!("app:{id}"),
            Self::Folder(id) => format!("folder:{id}"),
        }
    }
}
fn clean_label(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(80)
        .collect()
}
fn manual_target(value: &str) -> bool {
    if crate::dock::command_pin::decode(value).is_some() {
        return true;
    }
    let path = Path::new(value);
    path.is_absolute()
        && value.len() <= 32760
        && !value.chars().any(char::is_control)
        && path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("exe") || x.eq_ignore_ascii_case("lnk"))
}
impl Library {
    pub fn from_json(value: &serde_json::Value) -> Self {
        let mut out = serde_json::from_value::<Self>(value.clone()).unwrap_or_default();
        let mut seen = HashSet::new();
        out.manual.retain(|m| {
            manual_target(&m.launch) && seen.insert(catalog::identity(Path::new(&m.launch)))
        });
        out.manual.truncate(256);
        seen.clear();
        out.meta
            .retain(|m| !m.id.is_empty() && m.id.len() <= 32760 && seen.insert(m.id.clone()));
        out.meta.truncate(4096);
        let mut folders = HashSet::new();
        out.folders
            .retain(|f| f.id > 0 && !clean_label(&f.name).is_empty() && folders.insert(f.id));
        out.folders.truncate(128);
        out.serial = out
            .serial
            .max(out.folders.iter().map(|f| f.id).max().unwrap_or(0));
        out.order.truncate(8192);
        out
    }
    pub(super) fn add(
        &mut self,
        path: &Path,
        name: &str,
        args: &str,
        dir: &str,
    ) -> Result<(), &'static str> {
        if self.manual.len() >= 256 {
            return Err("Thư viện đã đạt giới hạn 256 mục tự thêm.");
        }
        let raw = path.to_str().ok_or("Đường dẫn không hợp lệ.")?;
        if !manual_target(raw) || !path.is_file() {
            return Err("Chọn tệp EXE hoặc LNK đang tồn tại.");
        }
        let default = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Ứng dụng");
        let name = clean_label(if name.trim().is_empty() {
            default
        } else {
            name
        });
        let launch = if args.trim().is_empty() && dir.trim().is_empty() {
            raw.to_owned()
        } else {
            crate::dock::command_pin::encode_application(path, &name, args, dir)?
        };
        let id = catalog::identity(Path::new(&launch));
        if self.is_manual(&id) {
            return Err("Mục này đã có trong thư viện.");
        }
        self.manual.push(Manual { name, launch });
        Ok(())
    }
    pub(super) fn remove_manual(&mut self, id: &str) {
        self.manual
            .retain(|m| catalog::identity(Path::new(&m.launch)) != id);
    }
    pub(super) fn is_manual(&self, id: &str) -> bool {
        self.manual
            .iter()
            .any(|m| catalog::identity(Path::new(&m.launch)) == id)
    }
    pub(super) fn merge_manual(&self, rows: &mut Vec<Entry>) {
        if self.manual.is_empty() {
            return;
        }
        let mut index = rows
            .iter()
            .enumerate()
            .map(|(i, e)| (catalog::identity(&e.path), i))
            .collect::<std::collections::HashMap<_, _>>();
        for m in &self.manual {
            let key = catalog::identity(Path::new(&m.launch));
            if let Some(i) = index.get(&key) {
                rows[*i].name.clone_from(&m.name);
                continue;
            }
            index.insert(key, rows.len());
            rows.push(Entry {
                name: m.name.clone(),
                path: m.launch.clone().into(),
                kind: "Application",
                detail: "Ứng dụng tự thêm".into(),
            });
        }
    }
    fn meta(&self, id: &str) -> Option<&Metadata> {
        self.meta.iter().find(|m| m.id == id)
    }
    fn edit(&mut self, id: &str) -> Option<&mut Metadata> {
        let i = if let Some(i) = self.meta.iter().position(|m| m.id == id) {
            i
        } else {
            if self.meta.len() >= 4096 {
                return None;
            }
            self.meta.push(Metadata {
                id: id.to_owned(),
                ..Metadata::default()
            });
            self.meta.len() - 1
        };
        self.meta.get_mut(i)
    }
    pub(super) fn name<'a>(&'a self, id: &str, default: &'a str) -> &'a str {
        self.meta(id)
            .map(|m| m.name.as_str())
            .filter(|n| !n.is_empty())
            .unwrap_or(default)
    }
    pub(super) fn rename(&mut self, id: &str, value: &str) {
        if let Some(m) = self.edit(id) {
            m.name = clean_label(value);
        }
    }
    pub(super) fn hidden(&self, id: &str) -> bool {
        self.meta(id).is_some_and(|m| m.hidden)
    }
    pub(super) fn hide(&mut self, id: &str, value: bool) {
        if let Some(m) = self.edit(id) {
            m.hidden = value;
        }
    }
    pub(super) fn favorite(&self, id: &str) -> bool {
        self.meta(id).is_some_and(|m| m.favorite)
    }
    pub(super) fn toggle_favorite(&mut self, id: &str) {
        if let Some(m) = self.edit(id) {
            m.favorite = !m.favorite;
        }
    }
    pub(super) fn folder(&self, id: &str) -> u64 {
        self.meta(id)
            .map(|m| m.folder)
            .filter(|id| self.folders.iter().any(|f| f.id == *id))
            .unwrap_or(0)
    }
    pub(super) fn move_to(&mut self, id: &str, folder: u64) {
        if folder == 0 || self.folders.iter().any(|f| f.id == folder) {
            if let Some(m) = self.edit(id) {
                m.folder = folder;
            }
        }
    }
    pub(super) fn create_folder(&mut self, label: &str) -> Option<u64> {
        let label = clean_label(label);
        if label.is_empty() || self.folders.len() >= 128 {
            return None;
        }
        self.serial = self.serial.checked_add(1)?;
        self.folders.push(Folder {
            id: self.serial,
            name: label,
        });
        Some(self.serial)
    }
    pub(super) fn rename_folder(&mut self, id: u64, label: &str) {
        let label = clean_label(label);
        if !label.is_empty() {
            if let Some(folder) = self.folders.iter_mut().find(|f| f.id == id) {
                folder.name = label;
            }
        }
    }
    pub(super) fn delete_folder(&mut self, id: u64) {
        self.folders.retain(|f| f.id != id);
        for m in &mut self.meta {
            if m.folder == id {
                m.folder = 0;
            }
        }
        self.order.retain(|key| key != &format!("folder:{id}"));
    }
    pub(super) fn ordered(&self, rows: &mut [(TileKey, Entry)], favorites: bool) {
        use std::collections::HashMap;
        let ranks: HashMap<_, _> = self
            .order
            .iter()
            .enumerate()
            .map(|(i, k)| (k.as_str(), i))
            .collect();
        rows.sort_by_cached_key(|(key, entry)| {
            let rank = ranks.get(key.order_key().as_str()).copied();
            let favorite = favorites && matches!(key, TileKey::App(id) if self.favorite(id));
            (
                rank.is_none(),
                rank.unwrap_or(usize::MAX),
                !favorite,
                entry.name.to_lowercase(),
            )
        });
    }
    pub(super) fn reorder(&mut self, rows: &[TileKey], from: &TileKey, to: &TileKey) {
        let mut current = rows.iter().map(TileKey::order_key).collect::<Vec<_>>();
        let src = from.order_key();
        let dst = to.order_key();
        let Some(a) = current.iter().position(|k| k == &src) else {
            return;
        };
        let Some(b) = current.iter().position(|k| k == &dst) else {
            return;
        };
        if a == b {
            return;
        }
        current.remove(a);
        current.insert(b, src);
        let visible = current.iter().cloned().collect::<HashSet<_>>();
        self.order.retain(|k| !visible.contains(k));
        self.order.extend(current);
        self.order.truncate(8192);
    }
    pub fn reset_layout(&mut self) {
        self.order.clear();
        self.folders.clear();
        for m in &mut self.meta {
            m.folder = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_roundtrip_does_not_delete_apps_or_hidden_state() {
        let mut lib = Library::default();
        let id = r"c:\example.exe";
        let folder = lib.create_folder("  Làm việc  ").unwrap();
        lib.move_to(id, folder);
        lib.hide(id, true);
        lib.rename(id, "Tên mới");
        lib.toggle_favorite(id);
        let restored = Library::from_json(&serde_json::to_value(&lib).unwrap());
        assert_eq!(restored, lib);
        assert_eq!(restored.folders[0].name, "Làm việc");
        lib.delete_folder(folder);
        assert_eq!(lib.folder(id), 0);
        assert!(lib.hidden(id));
        lib.hide(id, false);
        lib.reset_layout();
        assert!(lib.favorite(id));
        assert_eq!(lib.name(id, "Default"), "Tên mới");
    }

    #[test]
    fn manual_target_rejects_discovery_uris_and_non_launch_files() {
        assert!(manual_target(r"C:\Portable software\Tool.EXE"));
        assert!(manual_target(r"C:\Shortcuts\Tool.lnk"));
        for bad in [
            "cmd.exe",
            "ms-settings:",
            r"C:\file.cmd",
            r"C:\file.exe\child",
            "https://example.net",
        ] {
            assert!(!manual_target(bad));
        }
    }

    #[test]
    fn order_is_logical_across_viewport_sizes() {
        let mut lib = Library::default();
        let item = |name: &str| {
            (
                TileKey::App(name.into()),
                Entry {
                    name: name.into(),
                    path: name.into(),
                    kind: "Application",
                    detail: String::new(),
                },
            )
        };
        let mut rows = vec![item("a"), item("b"), item("c")];
        let keys = rows.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
        lib.reorder(&keys, &keys[2], &keys[0]);
        lib.ordered(&mut rows, false);
        assert_eq!(
            rows.iter()
                .map(|(_, e)| e.name.as_str())
                .collect::<Vec<_>>(),
            ["c", "a", "b"]
        );
    }
}
