//! Canonical Dock/Stage settings. Only Deskland main persists these values.
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub enabled: bool,
    pub stage_enabled: bool,
    pub pinned: Vec<String>,
    pub all_windows: bool,
    pub stage_side: String,
}
#[derive(Debug)]
pub enum Edit {
    Pin(String), Unpin(String), AllWindows(bool), StageSide(String), DockEnabled(bool), StageEnabled(bool),
}
impl Edit {
    pub fn apply(self, options: &mut Options) {
        match self {
            Self::Pin(path)=>{if options.pinned.len()<32 && !path.contains('\0') && !options.pinned.iter().any(|p|p.eq_ignore_ascii_case(&path)){options.pinned.push(path);}}
            Self::Unpin(path)=>options.pinned.retain(|p|!p.eq_ignore_ascii_case(&path)),
            Self::AllWindows(on)=>options.all_windows=on,
            Self::StageSide(side)=>{if side=="left"||side=="right" {options.stage_side=side;options.stage_enabled=true;}}
            Self::DockEnabled(on)=>options.enabled=on,
            Self::StageEnabled(on)=>options.stage_enabled=on,
        }
    }
}
impl Default for Options {
    fn default() -> Self {
        Self { enabled: false, stage_enabled: false, pinned: Vec::new(), all_windows: false, stage_side: "left".into() }
    }
}
impl Options {
    pub fn from_json(v: &Value) -> Self {
        let mut seen = HashSet::new();
        Self {
            enabled: v["enabled"].as_bool().unwrap_or(false),
            stage_enabled: v["stageEnabled"].as_bool().unwrap_or(false),
            pinned: v["pinned"].as_array().map(|a| a.iter().filter_map(Value::as_str)
                .filter(|s| !s.is_empty() && s.len() <= 32760 && !s.contains('\0'))
                .filter(|s| seen.insert(s.to_lowercase())).take(32).map(str::to_owned).collect()).unwrap_or_default(),
            all_windows: v["allWindows"].as_bool().unwrap_or(false),
            stage_side: if v["stageSide"] == "right" { "right" } else { "left" }.into(),
        }
    }
    /// Presence, including malformed/null canonical data, prevents a stale legacy
    /// file from resurrecting state. Migration preserves pins/scope/side, never
    /// silently enables overlays that used to be manually launched.
    pub fn migrate(root: &Value, legacy: Option<&Value>) -> Self {
        if let Some(v) = root.get("dock") { return Self::from_json(v); }
        let mut options = legacy.map(Self::from_json).unwrap_or_default();
        options.enabled = false;
        options.stage_enabled = false;
        options
    }
    pub fn any_enabled(&self) -> bool { self.enabled || self.stage_enabled }
    pub fn json(&self) -> Value {
        json!({"version":1,"enabled":self.enabled,"stageEnabled":self.stage_enabled,
            "pinned":self.pinned,"allWindows":self.all_windows,"stageSide":self.stage_side})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_migration_is_once_only_and_does_not_enable_modules() {
        let legacy = json!({"pinned":["C:\\Apps\\one.exe"],"allWindows":true,"stageSide":"right","enabled":true,"stageEnabled":true});
        let migrated = Options::migrate(&json!({"theme":"ocean"}), Some(&legacy));
        assert!(!migrated.any_enabled());assert!(migrated.all_windows);assert_eq!(migrated.stage_side,"right");
        assert_eq!(migrated.pinned,vec!["C:\\Apps\\one.exe"]);
        let mut saved = migrated.clone();saved.enabled=true;saved.pinned.clear();
        assert_eq!(Options::migrate(&json!({"dock":saved.json()}),Some(&legacy)),saved);
        assert_eq!(Options::migrate(&json!({"dock":null}),Some(&legacy)),Options::default());
    }
    #[test]
    fn validated_roundtrip_preserves_independent_flags_and_order() {
        for enabled in [false,true] {for stage_enabled in [false,true] {
            let options=Options{enabled,stage_enabled,pinned:vec!["B.exe".into(),"A.exe".into()],all_windows:true,stage_side:"right".into()};
            assert_eq!(Options::from_json(&options.json()),options);
            assert_eq!(options.any_enabled(),enabled||stage_enabled);
        }}
        let bad=json!({"enabled":"true","stageEnabled":1,"stageSide":"off","pinned":["A.exe","a.EXE","",5,"nul\u{0000}path"]});
        let options=Options::from_json(&bad);assert!(!options.any_enabled());assert_eq!(options.pinned,vec!["A.exe"]);assert_eq!(options.stage_side,"left");
        let many=json!({"pinned":(0..100).map(|i|format!("{i}.exe")).collect::<Vec<_>>()});
        assert_eq!(Options::from_json(&many).pinned.len(),32);
    }
    #[test]
    fn queued_edits_do_not_overwrite_other_modules_or_newer_preferences() {
        let mut options=Options{enabled:true,stage_enabled:true,..Default::default()};
        Edit::DockEnabled(false).apply(&mut options);
        Edit::Pin("new.exe".into()).apply(&mut options);
        Edit::AllWindows(true).apply(&mut options);
        assert!(!options.enabled);assert!(options.stage_enabled);assert!(options.all_windows);
        Edit::StageEnabled(false).apply(&mut options);assert!(!options.any_enabled());
    }
}
