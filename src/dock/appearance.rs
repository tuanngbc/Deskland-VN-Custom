//! Persistent Dock appearance. Display targeting and app scope stay separate.
use serde_json::{json,Value};

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum DockSide { #[default] Bottom, Left, Right }
impl DockSide {
    fn parse(value:&Value)->Self {match value.as_str(){Some("left")=>Self::Left,Some("right")=>Self::Right,_=>Self::Bottom}}
    fn key(self)->&'static str {match self{Self::Bottom=>"bottom",Self::Left=>"left",Self::Right=>"right"}}
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum Theme { #[default] Auto, Light, Dark }
impl Theme {
    pub(super) fn parse(value:&Value)->Self {match value.as_str(){Some("light")=>Self::Light,Some("dark")=>Self::Dark,_=>Self::Auto}}
    pub(super) fn key(self)->&'static str {match self{Self::Auto=>"auto",Self::Light=>"light",Self::Dark=>"dark"}}
    pub(super) fn dark(self,system:bool)->bool{match self{Self::Auto=>system,Self::Light=>false,Self::Dark=>true}}
}

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum StartIcon { #[default] Windows, Grid, Orb, Minimal }
impl StartIcon {
    pub const ALL:[(Self,&'static str);4]=[(Self::Windows,"Logo Windows"),(Self::Grid,"Lưới ứng dụng"),(Self::Orb,"Huy hiệu Dock"),(Self::Minimal,"Bốn ô đơn sắc")];
    fn parse(value:&Value)->Self {match value.as_str(){Some("grid")=>Self::Grid,Some("orb")=>Self::Orb,Some("minimal")=>Self::Minimal,_=>Self::Windows}}
    fn key(self)->&'static str {match self{Self::Windows=>"windows",Self::Grid=>"grid",Self::Orb=>"orb",Self::Minimal=>"minimal"}}
}

/// Percentage controls are stored as integers so settings acknowledgement and
/// persistence can compare them exactly. On unsupported backdrops the same
/// opacity/contrast controls still affect a true composited alpha surface.
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct Appearance {
    pub size:u32,
    pub icon_size:u32,
    pub start_icon:StartIcon,
    pub spacing:u32,
    pub transparency:u32,
    pub position:DockSide,
    pub theme:Theme,
    pub magnification:u32,
    pub animation_speed:u32,
    pub reduced_motion:bool,
    pub auto_hide:bool,
}
impl Default for Appearance {
    fn default()->Self{Self{size:100,icon_size:44,start_icon:StartIcon::default(),spacing:10,transparency:36,position:DockSide::Bottom,theme:Theme::Auto,
        magnification:38,animation_speed:100,reduced_motion:false,auto_hide:false}}
}
impl Appearance {
    pub(super) fn pixel_scale(&self,dpi:u32)->f32{(dpi as f32/96.0).clamp(0.75,3.0)*self.size as f32/100.0}
    pub fn from_json(v:&Value)->Self {
        fn bounded(v:&Value,default:u32,min:u32,max:u32)->u32{
            v.as_u64().map(|x|x.clamp(min as u64,max as u64) as u32).unwrap_or(default)
        }
        let old=Self::default();
        Self {
            size:bounded(&v["size"],old.size,75,140),
            icon_size:bounded(&v["iconSize"],old.icon_size,28,60),
            start_icon:StartIcon::parse(&v["startIcon"]),
            spacing:bounded(&v["spacing"],old.spacing,4,22),
            transparency:bounded(&v["transparency"],old.transparency,4,74),
            position:DockSide::parse(&v["position"]),
            theme:Theme::parse(&v["theme"]),
            magnification:bounded(&v["magnification"],old.magnification,0,55),
            animation_speed:bounded(&v["animationSpeed"],old.animation_speed,50,170),
            reduced_motion:v["reducedMotion"].as_bool().unwrap_or(false),
            auto_hide:v["autoHide"].as_bool().unwrap_or(false),
        }
    }
    pub fn json(&self)->Value {
        json!({"size":self.size,"iconSize":self.icon_size,"startIcon":self.start_icon.key(),"spacing":self.spacing,"transparency":self.transparency,
            "position":self.position.key(),"theme":self.theme.key(),"magnification":self.magnification,
            "animationSpeed":self.animation_speed,"reducedMotion":self.reduced_motion,"autoHide":self.auto_hide})
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn start_icon_defaults_for_old_profiles_and_roundtrips(){
        assert_eq!(Appearance::from_json(&json!({"size":self.size,"iconSize":48})).start_icon,StartIcon::Windows);
        assert_eq!(Appearance::from_json(&json!({"startIcon":"unknown"})).start_icon,StartIcon::Windows);
        for (icon,_) in StartIcon::ALL{
            let settings=Appearance{start_icon:icon,..Default::default()};
            assert_eq!(Appearance::from_json(&settings.json()),settings);
        }
    }
}
