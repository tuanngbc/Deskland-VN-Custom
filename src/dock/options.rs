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
    pub card_height: u32,
    pub right: Option<bool>,
    pub scale: u32,
    pub spacing: u32,
    pub edge_gap: u32,
    pub app_gap: u32,
    pub group_limit: u32,
    pub opacity: u32,
    pub stack_groups: bool,
    pub theme:super::Theme,
}
impl Default for StageStyle {
    fn default() -> Self { Self { card_height:0,right: None, scale: 100, spacing: 6, edge_gap:4, app_gap:0, group_limit: 5, opacity: 76, stack_groups: true,theme:super::Theme::Auto } }
}
impl StageStyle {
    pub fn from_json(v: &Value) -> Self {
        let size = |key: &str, base, min, max| v[key].as_u64().map_or(base, |x| x.clamp(min, max) as u32);
        Self {
            card_height:card_height(&v["cardHeight"]),
            right: match v["side"].as_str() { Some("left") => Some(false), Some("right") => Some(true), _ => None },
            scale: size("scale", 100, 75, 140),
            spacing: size("spacing", 6, 2, 20),
            edge_gap:size("edgeGap",4,0,64),
            app_gap:size("appGap",0,0,64),
            group_limit: size("groupLimit", 5, 1, 8),
            opacity: size("opacity", 76, 35, 100),
            stack_groups: v["groupPresentation"].as_str()!=Some("flat"),
            theme:super::Theme::parse(&v["theme"]),
        }
    }
    fn json(self) -> Value {
        json!({"cardHeight":self.card_height,"side":match self.right{Some(false)=>"left",Some(true)=>"right",None=>"default"},
            "scale":self.scale,"spacing":self.spacing,"edgeGap":self.edge_gap,"appGap":self.app_gap,"groupLimit":self.group_limit,"opacity":self.opacity,
            "groupPresentation":if self.stack_groups{"stacked"}else{"flat"},"theme":self.theme.key()})
    }
}

fn card_height(v:&Value)->u32{
    v.as_u64().filter(|&n|n!=0).map_or(0,|n|n.clamp(72,180) as u32)
}
fn persistent_group(key:&str)->bool{
    key.len()<=2048 && !key.chars().any(char::is_control)
        && ["exe:","aumid:"].iter().any(|prefix|key.strip_prefix(*prefix).is_some_and(|s|!s.is_empty()))
}

/// Output placement is independent of the application model's window scope.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayMode { #[default] All, Primary, Selected, Custom, Follow }
impl DisplayMode {
    pub fn key(self) -> &'static str {
        match self { Self::All => "all", Self::Primary => "primary", Self::Selected => "selected", Self::Custom => "custom", Self::Follow=>"follow" }
    }
    pub fn parse(value: &str) -> Self {
        match value { "primary" => Self::Primary, "selected" => Self::Selected, "custom" => Self::Custom, "follow"=>Self::Follow, _ => Self::All }
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
            DisplayMode::Selected|DisplayMode::Follow => self.selected.as_deref() == Some(*id),
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
    pub taskbar_default:bool,
    pub stage_enabled: bool,
    pub stage_peek:bool,
    pub stage_peek_delay:u32,
    pub clipboard_history:bool,
    pub clipboard_history_limit:u32,
    pub pinned: Vec<String>,
    pub hidden_apps: Vec<String>,
    pub app_icons:crate::app_icons::Overrides,
    pub all_windows: bool,
    pub native_shell_buttons: bool,
    pub start_shortcut:bool,
    pub win_key_start:bool,
    pub shell_context_menu:bool,
    pub alt_tab_overview:bool,
    pub volume_keys:bool,
    pub snap_layouts:bool,
    pub always_on_top:bool,
    pub jelly_drag:bool,
    pub news_source:u8,
    pub weather_unit:u8,
    pub stage_side: String,
    pub dock_display: DisplayTarget,
    pub stage_display: DisplayTarget,
    pub stage_perspective:bool,
    pub stage_card_height:u32,
    pub stage_card_overrides:BTreeMap<String,u32>,
    pub stage_styles: BTreeMap<String, StageStyle>,
    pub appearance: Appearance,
    pub window_policies:BTreeMap<String,WindowPolicy>,
    pub utility_profiles:BTreeMap<String,Profile>,
}
#[derive(Debug)]
pub enum Edit {
    FollowDisplay(String),
    NewsSource(u8),
    WeatherUnit(u8),
    HideApp(String), ShowHiddenApp(Option<String>),
    Pin(String), Unpin(String), MovePin { path:String, anchor:String, after:bool }, AllWindows(bool), StageSide(String), DockEnabled(bool), StageEnabled(bool),
    StageStyle(String, StageStyle),
    StagePresentation(bool), StageCardHeight(u32), StageGroupHeight(String,u32),
    UtilityProfile(String, Profile),
    WindowPolicy(String, WindowPolicy),
    TaskbarDefault(bool),
    AppIcon{key:String,file:Option<String>},
}
impl Edit {
    pub fn apply(self, options: &mut Options) {
        match self {
            Self::FollowDisplay(id)=>{if options.dock_display.mode==DisplayMode::Follow{if let Some(id)=display_id(&id){options.dock_display.selected=Some(id);}}},
            Self::NewsSource(source)=>options.news_source=source.min(5),
            Self::WeatherUnit(unit)=>options.weather_unit=unit.min(2),
            Self::HideApp(path)=>{
                if !path.is_empty()&&path.len()<=32760&&!path.chars().any(char::is_control)&&options.hidden_apps.len()<128
                    &&!options.pinned.iter().any(|p|p.eq_ignore_ascii_case(&path))&&!options.hidden_apps.iter().any(|p|p.eq_ignore_ascii_case(&path)){
                    options.hidden_apps.push(path);
                }
            },
            Self::ShowHiddenApp(Some(path))=>options.hidden_apps.retain(|p|!p.eq_ignore_ascii_case(&path)),
            Self::ShowHiddenApp(None)=>options.hidden_apps.clear(),
            Self::AppIcon{key,file}=>{
                let Some(key)=crate::app_icons::normalized(&key)else{return;};
                match file{
                    Some(file) if crate::app_icons::valid_file(&file)
                        &&(options.app_icons.len()<crate::app_icons::MAX_ICONS||options.app_icons.contains_key(&key))=>{
                        options.app_icons.insert(key,file);
                    }
                    None=>{options.app_icons.remove(&key);}
                    _=>{}
                }
            }
            Self::Pin(path)=>{if options.pinned.len()<32 && !path.is_empty() && path.len()<=32760 && !path.contains('\0') && !options.pinned.iter().any(|p|p.eq_ignore_ascii_case(&path)){options.hidden_apps.retain(|p|!p.eq_ignore_ascii_case(&path));options.pinned.push(path);}}
            Self::Unpin(path)=>options.pinned.retain(|p|!p.eq_ignore_ascii_case(&path)),
            Self::MovePin{path,anchor,after}=>{
                if let (Some(from),Some(to))=(options.pinned.iter().position(|p|p.eq_ignore_ascii_case(&path)),
                    options.pinned.iter().position(|p|p.eq_ignore_ascii_case(&anchor))){
                    if from!=to{
                        let boundary=to+usize::from(after);
                        let pin=options.pinned.remove(from);
                        options.pinned.insert(boundary-usize::from(from<boundary),pin);
                    }
                }
            }
            Self::AllWindows(on)=>options.all_windows=on,
            Self::StageSide(side)=>{if side=="left"||side=="right" {options.stage_side=side;options.stage_enabled=true;}}
            Self::StagePresentation(on)=>options.stage_perspective=on,
            Self::StageCardHeight(h)=>options.stage_card_height=card_height(&json!(h)),
            Self::StageGroupHeight(key,h)=>{
                if persistent_group(&key){
                    if h==0{options.stage_card_overrides.remove(&key);}
                    else if options.stage_card_overrides.len()<128||options.stage_card_overrides.contains_key(&key){
                        options.stage_card_overrides.insert(key,card_height(&json!(h)));
                    }
                }
            }
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
            Self::TaskbarDefault(on)=>options.taskbar_default=on,
            Self::DockEnabled(on)=>options.enabled=on,
            Self::StageEnabled(on)=>options.stage_enabled=on,
        }
    }
}
impl Default for Options {
    fn default() -> Self {
        Self { enabled: false, taskbar_default:true, stage_enabled: false, stage_peek:false, stage_peek_delay:350,clipboard_history:false,clipboard_history_limit:20,pinned: Vec::new(), hidden_apps:Vec::new(), app_icons:Default::default(), all_windows: false, native_shell_buttons:false, start_shortcut:false, win_key_start:false, shell_context_menu:false, alt_tab_overview:false, volume_keys:false, snap_layouts:false, always_on_top:false, jelly_drag:false, news_source:0, weather_unit:0, stage_side: "left".into(), dock_display: DisplayTarget::default(), stage_display: DisplayTarget::default(), stage_styles: BTreeMap::new(), stage_perspective:true,stage_card_height:0,stage_card_overrides:BTreeMap::new(),appearance: Appearance::default(),utility_profiles:BTreeMap::new(),window_policies:BTreeMap::new() }
    }
}
impl Options {
    pub fn from_json(v: &Value) -> Self {
        let mut seen = HashSet::new();
        Self {
            enabled: v["enabled"].as_bool().unwrap_or(false),
            taskbar_default:v["taskbarDefault"].as_bool().unwrap_or(true),
            stage_enabled: v["stageEnabled"].as_bool().unwrap_or(false),
            stage_peek:v["stagePeek"].as_bool().unwrap_or(false),
            stage_peek_delay:v["stagePeekDelay"].as_u64().unwrap_or(350).clamp(150,1200) as u32,
            clipboard_history:v["clipboardHistory"].as_bool().unwrap_or(false),
            clipboard_history_limit:v["clipboardHistoryLimit"].as_u64().unwrap_or(20).clamp(5,40) as u32,
            pinned: v["pinned"].as_array().map(|a| a.iter().filter_map(Value::as_str)
                .filter(|s| !s.is_empty() && s.len() <= 32760 && !s.contains('\0'))
                .filter(|s| seen.insert(s.to_lowercase())).take(32).map(str::to_owned).collect()).unwrap_or_default(),
            hidden_apps:{let mut seen=HashSet::new();v["hiddenApps"].as_array().into_iter().flatten().filter_map(Value::as_str)
                .filter(|s|!s.is_empty()&&s.len()<=32760&&!s.chars().any(char::is_control))
                .filter(|s|seen.insert(s.to_lowercase())).take(128).map(str::to_owned).collect()},
            app_icons:v["appIcons"].as_array().into_iter().flatten().filter_map(|entry|{
                let key=crate::app_icons::normalized(entry["app"].as_str()?)?;
                let file=entry["image"].as_str()?;
                crate::app_icons::valid_file(file).then(||(key,file.to_owned()))
            }).take(crate::app_icons::MAX_ICONS).collect(),
            all_windows: v["allWindows"].as_bool().unwrap_or(false),
            native_shell_buttons:v["nativeShellButtons"].as_bool().unwrap_or(false),
            start_shortcut:v["startShortcut"].as_bool().unwrap_or(false),
            win_key_start:v["winKeyStart"].as_bool().unwrap_or(false),
            shell_context_menu:v["shellContextMenu"].as_bool().unwrap_or(false),
            alt_tab_overview:v["altTabOverview"].as_bool().unwrap_or(false),
            volume_keys:v["volumeKeys"].as_bool().unwrap_or(false),
            snap_layouts:v["snapLayouts"].as_bool().unwrap_or(false),
            always_on_top:v["alwaysOnTop"].as_bool().unwrap_or(false),
            jelly_drag:v["jellyDrag"].as_bool().unwrap_or(false),
            news_source:v["newsSource"].as_u64().filter(|&n|n<=5).unwrap_or(0) as u8,
            weather_unit:v["weatherUnit"].as_u64().filter(|&n|n<=2).unwrap_or(0) as u8,
            dock_display: DisplayTarget::from_json(&v["dockDisplay"]),
            stage_display: DisplayTarget::from_json(&v["stageDisplay"]),
            stage_perspective:v["stagePresentation"].as_str()!=Some("dwm"),
            stage_card_height:card_height(&v["stageCardHeight"]),
            stage_card_overrides:v["stageCardOverrides"].as_object().into_iter().flatten().filter_map(|(key,v)|{
                let h=card_height(v);
                (h!=0&&persistent_group(key)).then(||(key.clone(),h))
            }).take(128).collect(),
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
    pub(super) fn utility_cluster(&self,id:&str)->super::geometry::Cluster{
        let p=self.utility_profiles.get(id);
        let signals=p.map_or(2,|p|p.dock_signals());
        let inline_metrics=p.is_some_and(|p|p.dock_metrics());
        super::geometry::Cluster{side:(signals>0||inline_metrics).then(||p.is_some_and(|p|p.leading)),
            cell:p.map_or(28,|p|p.cell),gap:p.map_or(4,|p|p.gap),inline_metrics,signals}
    }
    pub fn stable_stage_key(key:&str)->bool{persistent_group(key)}
    pub fn stage_style(&self, id: &str) -> StageStyle {
        let mut settings = self.stage_styles.get(id).copied().unwrap_or_default();
        settings.right = Some(settings.right.unwrap_or(self.stage_side == "right"));
        settings
    }
    pub fn json(&self) -> Value {
        json!({"version":2,"enabled":self.enabled,"taskbarDefault":self.taskbar_default,"stageEnabled":self.stage_enabled,
            "stagePeek":self.stage_peek,"stagePeekDelay":self.stage_peek_delay,
            "clipboardHistory":self.clipboard_history,"clipboardHistoryLimit":self.clipboard_history_limit,
            "newsSource":self.news_source,
            "weatherUnit":self.weather_unit,
            "hiddenApps":self.hidden_apps,
            "pinned":self.pinned,"allWindows":self.all_windows,"nativeShellButtons":self.native_shell_buttons,"startShortcut":self.start_shortcut,"winKeyStart":self.win_key_start,"shellContextMenu":self.shell_context_menu,"altTabOverview":self.alt_tab_overview,"volumeKeys":self.volume_keys,"snapLayouts":self.snap_layouts,"alwaysOnTop":self.always_on_top,"jellyDrag":self.jelly_drag,"stageSide":self.stage_side,
            "appIcons":self.app_icons.iter().map(|(app,image)|json!({"app":app,"image":image})).collect::<Vec<_>>(),
            "stagePresentation":if self.stage_perspective{"perspective"}else{"dwm"},"stageCardHeight":self.stage_card_height,"stageCardOverrides":self.stage_card_overrides,
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
    fn taskbar_preference_defaults_to_deskland_but_preserves_explicit_windows(){
        let mut options=Options::migrate(&json!({"dock":{"enabled":true}}),None);
        assert!(options.taskbar_default&&options.enabled);
        Edit::TaskbarDefault(false).apply(&mut options);
        assert!(!Options::from_json(&options.json()).taskbar_default);
        assert!(!Options::default().enabled,"this preference must not enable Dock by itself");
    }
    #[test]
    fn dock_and_per_display_stage_themes_round_trip_independently(){
        let options=Options::from_json(&json!({"appearance":{"theme":"dark"},"stageStyles":{"main":{"theme":"light"},"aux":{"theme":"dark"}}}));
        let restored=Options::from_json(&options.json());
        assert_eq!(restored.appearance.theme,super::super::Theme::Dark);
        assert_eq!(restored.stage_style("main").theme,super::super::Theme::Light);
        assert_eq!(restored.stage_style("aux").theme,super::super::Theme::Dark);
        assert_eq!(restored.stage_style("new").theme,super::super::Theme::Auto);
    }
    #[test]
    fn stage_preview_and_height_migration_preserve_existing_choices(){
        let legacy=json!({"stageEnabled":true,"stageStyles":{"vertical":{"side":"right"}}});
        let mut p=Options::from_json(&legacy);
        assert!(p.stage_perspective);
        assert_eq!(p.stage_card_height,0);
        assert_eq!(p.stage_style("vertical").right,Some(true));
        Edit::StagePresentation(false).apply(&mut p);
        Edit::StageCardHeight(120).apply(&mut p);
        let style=p.stage_styles["vertical"];
        Edit::StageStyle("vertical".into(),StageStyle{card_height:72,..style}).apply(&mut p);
        Edit::StageGroupHeight("hwnd:123".into(),100).apply(&mut p);
        Edit::StageGroupHeight("manual:5".into(),100).apply(&mut p);
        Edit::StageGroupHeight("exe:c:/paint.exe".into(),500).apply(&mut p);
        assert_eq!(p.stage_card_overrides.len(),1);
        assert_eq!(p.stage_card_overrides["exe:c:/paint.exe"],180);
        assert_eq!(Options::from_json(&p.json()),p);
        Edit::StageGroupHeight("exe:c:/paint.exe".into(),0).apply(&mut p);
        assert!(p.stage_card_overrides.is_empty());
    }

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
        let mut follow=Options::from_json(&json!({"dockDisplay":{"mode":"follow","selected":"PORTRAIT"}}));
        assert_eq!(follow.dock_display.outputs(normal),vec![1]);
        assert_eq!(follow.dock_display.outputs([("landscape",true)]),vec![0]);
        Edit::FollowDisplay("landscape".into()).apply(&mut follow);
        assert_eq!(Options::from_json(&follow.json()).dock_display.outputs(normal),vec![0]);
        follow.dock_display.mode=DisplayMode::All;
        Edit::FollowDisplay("portrait".into()).apply(&mut follow);
        assert_eq!(follow.dock_display.selected.as_deref(),Some("landscape"));
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
        let move_pin=|path:&str,anchor:&str,after|Edit::MovePin{path:path.into(),anchor:anchor.into(),after};
        move_pin("first.exe","third.exe",true).apply(&mut options);
        move_pin("unpinned.exe","third.exe",false).apply(&mut options);
        assert_eq!(options.pinned,["second.exe","third.exe","first.exe"]);
        move_pin("first.exe","third.exe",false).apply(&mut options);
        assert_eq!(options.pinned,["second.exe","first.exe","third.exe"]);
        move_pin("third.exe","closed.exe",false).apply(&mut options);
        move_pin("second.exe","first.exe",false).apply(&mut options);
        assert_eq!(options.pinned,["second.exe","first.exe","third.exe"]);
        assert_eq!(Options::from_json(&options.json()),options);
    }
    #[test]
    fn override_icon_survives_roundtrip_and_reset_replaces_the_saved_array(){
        let mut options=Options{pinned:vec![r"C:\Apps\editor.exe".into()],..Default::default()};
        let key=r"C:/Apps/Editor.exe";
        let file="0123456789abcdef0123456789abcdef.png";
        Edit::AppIcon{key:key.into(),file:Some(file.into())}.apply(&mut options);
        let saved=options.json();
        assert_eq!(saved["appIcons"][0]["app"],r"c:\apps\editor.exe");
        assert_eq!(saved["appIcons"][0]["image"],file);
        assert_eq!(Options::from_json(&saved),options);
        Edit::AppIcon{key:key.into(),file:Some("../old.png".into())}.apply(&mut options);
        assert_eq!(options.json()["appIcons"],saved["appIcons"]);
        Edit::AppIcon{key:key.into(),file:None}.apply(&mut options);
        assert_eq!(options.json()["appIcons"],json!([]));
        assert_eq!(options.json()["pinned"],saved["pinned"]);
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
