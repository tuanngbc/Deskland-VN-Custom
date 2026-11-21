//! Canonical Dock/Stage settings. Only Deskland main persists these values.
use serde_json::{json, Value};
use super::{Appearance,utilities::Profile};
use std::collections::{BTreeMap, HashSet};

fn display_id(v: &str) -> Option<String> {
    (!v.is_empty() && v.len() <= 1024 && !v.chars().any(char::is_control))
        .then(|| v.to_lowercase())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowMode { #[default] Default, Reserve, Floating, AutoHide }
impl WindowMode {
    pub fn from_json(v:&Value)->Self {
        match v.as_str(){Some("reserve")=>Self::Reserve,Some("floating")=>Self::Floating,Some("autoHide")=>Self::AutoHide,_=>Self::Default}
    }
    pub fn key(self)->&'static str {
        match self{Self::Default=>"default",Self::Reserve=>"reserve",Self::Floating=>"floating",Self::AutoHide=>"autoHide"}
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowPolicy {pub dock:WindowMode,pub stage:WindowMode}
impl WindowPolicy {
    pub fn from_json(v:&Value)->Self {
        Self{dock:WindowMode::from_json(&v["dock"]),stage:WindowMode::from_json(&v["stage"])}
    }
    fn json(self)->Value {json!({"dock":self.dock.key(),"stage":self.stage.key()})}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageStyle {
    pub right: Option<bool>,
    pub scale: u32,
    pub spacing: u32,
    pub group_limit: u32,
    pub opacity: u32,
}
impl Default for StageStyle {
    fn default() -> Self { Self { right: None, scale: 100, spacing: 6, group_limit: 5, opacity: 76 } }
}
impl StageStyle {
    pub fn from_json(v: &Value) -> Self {
        let size = |key: &str, base, min, max| v[key].as_u64().map_or(base, |x| x.clamp(min, max) as u32);
        Self {
            right: match v["side"].as_str() { Some("left") => Some(false), Some("right") => Some(true), _ => None },
            scale: size("scale", 100, 75, 140),
            spacing: size("spacing", 6, 2, 20),
            group_limit: size("groupLimit", 5, 1, 8),
            opacity: size("opacity", 76, 35, 100),
        }
    }
    fn json(self) -> Value {
        json!({"side":match self.right{Some(false)=>"left",Some(true)=>"right",None=>"default"},
            "scale":self.scale,"spacing":self.spacing,"groupLimit":self.group_limit,"opacity":self.opacity})
    }
}

/// Output placement is independent of the application model's window scope.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayMode { #[default] All, Primary, Selected, Custom }
impl DisplayMode {
    pub fn key(self) -> &'static str {
        match self { Self::All => "all", Self::Primary => "primary", Self::Selected => "selected", Self::Custom => "custom" }
    }
    pub fn parse(value: &str) -> Self {
        match value { "primary" => Self::Primary, "selected" => Self::Selected, "custom" => Self::Custom, _ => Self::All }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplayTarget {
    pub mode: DisplayMode,
    pub selected: Option<String>,
    pub custom: Vec<String>,
}
impl DisplayTarget {
    fn from_json(v: &Value) -> Self {
        fn id(v: &Value) -> Option<String> {
            v.as_str().and_then(display_id)
        }
        let mut seen = HashSet::new();
        Self {
            mode: DisplayMode::parse(v["mode"].as_str().unwrap_or("all")),
            selected: id(&v["selected"]),
            custom: v["custom"].as_array().into_iter().flatten().filter_map(id)
                .filter(|s| seen.insert(s.clone())).take(32).collect(),
        }
    }
    fn json(&self) -> Value {
        json!({"mode":self.mode.key(),"selected":self.selected,"custom":self.custom})
    }
    /// Preserve disconnected IDs. Only use the current primary if *none* of
    /// the requested outputs are present. Reconnecting restores the selection.
    pub fn outputs<'a>(&self, outputs: impl IntoIterator<Item = (&'a str, bool)>) -> Vec<usize> {
        let outputs: Vec<_> = outputs.into_iter().collect();
        let mut selected: Vec<_> = outputs.iter().enumerate().filter(|(_, (id, primary))| match self.mode {
            DisplayMode::All => true,
            DisplayMode::Primary => *primary,
            DisplayMode::Selected => self.selected.as_deref() == Some(*id),
            DisplayMode::Custom => self.custom.iter().any(|s| s == *id),
        }).map(|(i, _)| i).collect();
        if selected.is_empty() && !outputs.is_empty() {
            selected.push(outputs.iter().position(|(_, primary)| *primary).unwrap_or(0));
        }
        selected
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub enabled: bool,
    pub stage_enabled: bool,
    pub pinned: Vec<String>,
    pub all_windows: bool,
    pub stage_side: String,
    pub dock_display: DisplayTarget,
    pub stage_display: DisplayTarget,
    pub stage_styles: BTreeMap<String, StageStyle>,
    pub appearance: Appearance,
    pub window_policies:BTreeMap<String,WindowPolicy>,
    pub utility_profiles:BTreeMap<String,Profile>,
}
#[derive(Debug)]
pub enum Edit {
    Pin(String), Unpin(String), MovePin(String,String), AllWindows(bool), StageSide(String), DockEnabled(bool), StageEnabled(bool),
    StageStyle(String, StageStyle),
    UtilityProfile(String, Profile),
    WindowPolicy(String, WindowPolicy),
}
impl Edit {
    pub fn apply(self, options: &mut Options) {
        match self {
            Self::Pin(path)=>{if options.pinned.len()<32 && !path.is_empty() && path.len()<=32760 && !path.contains('\0') && !options.pinned.iter().any(|p|p.eq_ignore_ascii_case(&path)){options.pinned.push(path);}}
            Self::Unpin(path)=>options.pinned.retain(|p|!p.eq_ignore_ascii_case(&path)),
            Self::MovePin(path,target)=>{
                if let (Some(from),Some(to))=(options.pinned.iter().position(|p|p.eq_ignore_ascii_case(&path)),
                    options.pinned.iter().position(|p|p.eq_ignore_ascii_case(&target))){
                    if from!=to{let pin=options.pinned.remove(from);options.pinned.insert(to,pin);}
                }
            }
            Self::AllWindows(on)=>options.all_windows=on,
            Self::StageSide(side)=>{if side=="left"||side=="right" {options.stage_side=side;options.stage_enabled=true;}}
            Self::StageStyle(id, style)=> {
                if let Some(id) = display_id(&id) {
                    if style == StageStyle::default() { options.stage_styles.remove(&id); }
                    else if options.stage_styles.len() < 32 || options.stage_styles.contains_key(&id) {
                        options.stage_styles.insert(id, style);
                    }
                }
            }
            Self::UtilityProfile(id, profile)=>{
                if let Some(id)=display_id(&id){
                    if profile==Profile::default(){options.utility_profiles.remove(&id);}
                    else if options.utility_profiles.len()<32||options.utility_profiles.contains_key(&id){
                        options.utility_profiles.insert(id,profile);
                    }
                }
            }
            Self::WindowPolicy(id,policy)=>{
                if let Some(id)=display_id(&id){
                    if policy==WindowPolicy::default(){options.window_policies.remove(&id);}
                    else if options.window_policies.len()<32||options.window_policies.contains_key(&id){
                        options.window_policies.insert(id,policy);
                    }
                }
            }
            Self::DockEnabled(on)=>options.enabled=on,
            Self::StageEnabled(on)=>options.stage_enabled=on,
        }
    }
}
impl Default for Options {
    fn default() -> Self {
        Self { enabled: false, stage_enabled: false, pinned: Vec::new(), all_windows: false, stage_side: "left".into(), dock_display: DisplayTarget::default(), stage_display: DisplayTarget::default(), stage_styles: BTreeMap::new(), appearance: Appearance::default(),utility_profiles:BTreeMap::new(),window_policies:BTreeMap::new() }
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
            dock_display: DisplayTarget::from_json(&v["dockDisplay"]),
            stage_display: DisplayTarget::from_json(&v["stageDisplay"]),
            stage_styles: v["stageStyles"].as_object().into_iter().flatten().filter_map(|(id,value)|{
                let style=StageStyle::from_json(value);
                (style!=StageStyle::default()).then(|| (display_id(id),style))
                    .and_then(|(id,style)| Some((id?,style)))
            }).take(32).collect(),
            utility_profiles:v["utilityProfiles"].as_object().into_iter().flatten().filter_map(|(id,value)|{
                let profile=Profile::from_json(value);
                (profile!=Profile::default()).then(||(display_id(id),profile)).and_then(|(id,p)|Some((id?,p)))
            }).take(32).collect(),
            window_policies:v["windowPolicies"].as_object().into_iter().flatten().filter_map(|(id,value)|{
                let mode=WindowPolicy::from_json(value);
                (mode!=WindowPolicy::default()).then(||(display_id(id),mode)).and_then(|(id,p)|Some((id?,p)))
            }).take(32).collect(),
            appearance: Appearance::from_json(&v["appearance"]),
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
    pub fn window_policy(&self,id:&str)->WindowPolicy {self.window_policies.get(id).copied().unwrap_or_default()}
    pub fn dock_auto_hide(&self,id:&str)->bool{
        match self.window_policy(id).dock{
            WindowMode::Default=>self.appearance.auto_hide,
            WindowMode::AutoHide=>true,
            _=>false,
        }
    }
    pub fn utility_cell(&self,id:&str)->u32{ self.utility_profiles.get(id).map_or(28,|p|p.cell) }
    pub fn utility_position(&self,id:&str)->Option<bool>{
        match self.utility_profiles.get(id){Some(p) if !p.enabled||p.hidden.len()==super::utilities::Widget::ALL.len()=>None,Some(p)=>Some(p.leading),None=>Some(false)}
    }
    pub fn stage_style(&self, id: &str) -> StageStyle {
        let mut settings = self.stage_styles.get(id).copied().unwrap_or_default();
        settings.right = Some(settings.right.unwrap_or(self.stage_side == "right"));
        settings
    }
    pub fn json(&self) -> Value {
        json!({"version":2,"enabled":self.enabled,"stageEnabled":self.stage_enabled,
            "pinned":self.pinned,"allWindows":self.all_windows,"stageSide":self.stage_side,
            "dockDisplay":self.dock_display.json(),"stageDisplay":self.stage_display.json(),"appearance":self.appearance.json(),
            "windowPolicies":self.window_policies.iter().map(|(id,p)|(id.clone(),p.json())).collect::<serde_json::Map<_,_>>(),
            "utilityProfiles":self.utility_profiles.iter().map(|(id,p)|(id.clone(),p.json())).collect::<serde_json::Map<_,_>>(),
            "stageStyles":self.stage_styles.iter().map(|(id,style)|(id.clone(),style.json())).collect::<serde_json::Map<_,_>>()})
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
            let options=Options{enabled,stage_enabled,pinned:vec!["B.exe".into(),"A.exe".into()],all_windows:true,stage_side:"right".into(),..Default::default()};
            assert_eq!(Options::from_json(&options.json()),options);
            assert_eq!(options.any_enabled(),enabled||stage_enabled);
        }}
        let bad=json!({"enabled":"true","stageEnabled":1,"stageSide":"off","pinned":["A.exe","a.EXE","",5,"nul\u{0000}path"]});
        let options=Options::from_json(&bad);assert!(!options.any_enabled());assert_eq!(options.pinned,vec!["A.exe"]);assert_eq!(options.stage_side,"left");
        let many=json!({"pinned":(0..100).map(|i|format!("{i}.exe")).collect::<Vec<_>>()});
        assert_eq!(Options::from_json(&many).pinned.len(),32);
    }
    #[test]
    fn output_targets_survive_primary_swap_disconnect_and_reconnect() {
        let options = Options::from_json(&json!({"allWindows":true,
            "dockDisplay":{"mode":"primary"},
            "stageDisplay":{"mode":"selected","selected":"PORTRAIT"}}));
        let normal = [("landscape",true),("portrait",false)];
        assert_eq!(options.dock_display.outputs(normal),vec![0]);
        assert_eq!(options.stage_display.outputs(normal),vec![1]);
        assert_eq!(options.dock_display.outputs([("portrait",true),("landscape",false)]),vec![0]);
        assert_eq!(options.stage_display.outputs([("landscape",true)]),vec![0]);
        assert_eq!(Options::from_json(&options.json()).stage_display.outputs(normal),vec![1]);
        let custom = DisplayTarget::from_json(&json!({"mode":"custom","custom":["other","Portrait","portrait",null]}));
        assert_eq!(custom.custom,vec!["other","portrait"]);
        assert_eq!(custom.outputs(normal),vec![1]);
        assert_eq!(custom.outputs([]),Vec::<usize>::new());
        assert_eq!(Options::migrate(&json!({"dock":{"allWindows":false}}),None).dock_display.outputs(normal),vec![0,1]);
    }
    #[test]
    fn stage_styles_keep_global_defaults_and_disconnected_display_overrides() {
        let mut options = Options::from_json(&json!({"stageSide":"right","stageStyles":{
            "Portrait":{"side":"left","scale":200,"spacing":0},
            "missing":{"side":"right","scale":125,"spacing":10}
        }}));
        assert_eq!(options.stage_style("primary"),StageStyle{right:Some(true),..Default::default()});
        assert_eq!(options.stage_style("portrait"),StageStyle{right:Some(false),scale:140,spacing:2,..Default::default()});
        Edit::StageStyle("portrait".into(),StageStyle::default()).apply(&mut options);
        let options = Options::from_json(&options.json());
        assert_eq!(options.stage_style("portrait").right,Some(true));
        assert_eq!(options.stage_style("missing").scale,125);
        assert!(!options.stage_enabled);
    }
    #[test]
    fn drag_reorders_only_existing_pins_and_roundtrips() {
        let mut options=Options{pinned:vec!["first.exe".into(),"second.exe".into(),"third.exe".into()],..Default::default()};
        Edit::MovePin("first.exe".into(),"third.exe".into()).apply(&mut options);
        Edit::MovePin("unpinned.exe".into(),"third.exe".into()).apply(&mut options);
        assert_eq!(options.pinned,["second.exe","third.exe","first.exe"]);
        assert_eq!(Options::from_json(&options.json()),options);
    }
    #[test]
    fn window_policy_keeps_legacy_auto_hide_and_per_screen_overrides() {
        let mut options=Options::from_json(&json!({"appearance":{"autoHide":true},"windowPolicies":{
            "Portrait":{"dock":"reserve","stage":"autoHide"},
            "disconnected":{"dock":"floating","stage":"reserve"}
        }}));
        assert!(options.dock_auto_hide("main"));
        assert!(!options.dock_auto_hide("portrait"));
        assert_eq!(options.window_policy("portrait").stage,WindowMode::AutoHide);
        Edit::WindowPolicy("portrait".into(),WindowPolicy::default()).apply(&mut options);
        assert!(options.dock_auto_hide("portrait"));
        assert_eq!(Options::from_json(&options.json()),options);
        assert_eq!(options.window_policy("disconnected").dock,WindowMode::Floating);
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
