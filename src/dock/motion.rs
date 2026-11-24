//! Shared elapsed-time native motion. No idle timer is needed at rest.
use std::{time::Instant,sync::atomic::{AtomicU32,AtomicBool,Ordering}};
use serde_json::{Value,json};

/// The main settings owner persists this once; both native UI threads read
/// the same snapshot. Durations are captured by each new transition.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) struct Settings { pub enabled:bool,pub speed:u32,pub respect_system:bool }
impl Default for Settings{
    fn default()->Self{Self{enabled:true,speed:70,respect_system:true}}
}
fn normalize_speed(value:&Value)->u32{
    let Some(value)=value.as_f64().filter(|v|v.is_finite()&&*v>0.0)else{return 70;};
    ((value.clamp(0.25,2.0)*20.0).round() as u32)*5
}
impl Settings {
    pub fn from_json(value:&Value)->Self{
        Self{enabled:value["animationsEnabled"].as_bool().unwrap_or(true),
            speed:normalize_speed(&value["animationSpeed"]),
            respect_system:value["respectSystemReducedMotion"].as_bool().unwrap_or(true)}
    }
    pub fn from_profile(root:&Value,legacy:&Value)->Self{
        if root["motion"].is_object(){return Self::from_json(&root["motion"]);}
        let speed=legacy["animationSpeed"].as_u64().filter(|s|*s!=100&&*s>0)
            .map_or(70,|s|normalize_speed(&json!(s as f64/100.0)));
        Self{speed,enabled:root["spotlight"]["animations"].as_bool().unwrap_or(true)
            &&!legacy["reducedMotion"].as_bool().unwrap_or(false),..Self::default()}
    }
    pub fn json(self)->Value{
        json!({"animationsEnabled":self.enabled,"animationSpeed":self.speed as f64/100.0,
            "respectSystemReducedMotion":self.respect_system})
    }
    fn packed(self)->u32{self.speed|((self.enabled as u32)<<8)|((self.respect_system as u32)<<9)}
    fn unpack(v:u32)->Self{Self{speed:v&255,enabled:v&256!=0,respect_system:v&512!=0}}
    fn allowed(self,system:bool)->bool{self.enabled&&(!self.respect_system||system)}
    fn duration(self,base:u32)->u32{((base as f64*100.0/self.speed as f64).round() as u32).max(1)}
}
static ACTIVE:AtomicU32=AtomicU32::new(70|256|512);
static SYSTEM:AtomicBool=AtomicBool::new(true);
pub(crate) fn settings()->Settings{Settings::unpack(ACTIVE.load(Ordering::Relaxed))}
pub(crate) fn configure(value:Settings){
    refresh_system();
    ACTIVE.store(value.packed(),Ordering::Relaxed);
}
pub(crate) fn refresh_system(){SYSTEM.store(system_allowed(),Ordering::Relaxed);}
pub(crate) fn allowed()->bool{settings().allowed(SYSTEM.load(Ordering::Relaxed))}
pub(crate) fn native_allowed()->bool{
    #[cfg(test)]
    if !NATIVE_FIXTURE.get(){return false;}
    allowed()
}
#[cfg(test)]
thread_local!{static NATIVE_FIXTURE:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
/// Called only from individually dispatched, ignored native CI fixtures.
#[cfg(test)]
pub(crate) fn native_fixture<T>(speed:u32,run:impl FnOnce()->T)->T{
    struct Reset(Settings,bool);
    impl Drop for Reset{fn drop(&mut self){configure(self.0);NATIVE_FIXTURE.set(self.1);}}
    let _guard=Reset(settings(),NATIVE_FIXTURE.replace(true));
    configure(Settings{speed,..Default::default()});
    run()
}
pub(crate) fn duration(base:u32)->u32{settings().duration(base)}
// Pointer feedback should remain responsive even at very slow visual speeds.
pub(crate) fn hover_ms()->u32{duration(HOVER_MS as u32).clamp(50,140)}
#[derive(Clone,Copy)]
pub(crate) struct Span{start:Instant,duration:u32}
impl Span{
    pub fn new(base:u32)->Self{Self::fixed(duration(base))}
    pub fn hover()->Self{Self::fixed(hover_ms())}
    pub fn fixed(duration:u32)->Self{Self{start:Instant::now(),duration:duration.max(1)}}
    pub fn progress(self)->f32{(self.start.elapsed().as_secs_f32()*1000.0/self.duration as f32).clamp(0.0,1.0)}
    #[cfg(test)]
    pub(crate) fn ms(self)->u32{self.duration}
}

#[derive(Clone,Copy)]
pub(crate) struct Curve {pub open:u32,pub close:u32,pub scale:f32}
pub(crate) const FULL:Curve=Curve{open:300,close:200,scale:0.90};
pub(crate) const COMPACT:Curve=Curve{open:220,close:160,scale:0.94};
pub(crate) const START:Curve=Curve{open:240,close:180,scale:0.965};
pub(crate) const MENU:Curve=Curve{open:180,close:120,scale:0.955};
pub(crate) const CONTROL:Curve=Curve{open:220,close:160,scale:1.0};
pub(crate) const CALENDAR:Curve=Curve{open:200,close:150,scale:1.0};
pub(crate) const FOLDER_MS:u32=240;
pub(crate) const HOVER_MS:u128=100;
pub(crate) const PAGE_MS:u128=220;
#[derive(Clone,Copy)]
pub(crate) struct Tween {
    pub start:Instant,
    pub duration:u32,
    pub from:f32,
    pub to:f32,
    frozen:bool,
}
impl Tween {
    pub fn new(open:bool,curve:Curve,previous:Option<Self>)->Self{
        Self::at_speed(open,curve,previous,settings())
    }
    fn at_speed(open:bool,curve:Curve,previous:Option<Self>,policy:Settings)->Self{
        Self{start:Instant::now(),duration:policy.duration(if open{curve.open}else{curve.close}),
            from:previous.map_or(if open{0.0}else{1.0},Self::openness),to:if open{1.0}else{0.0},frozen:false}
    }
    pub fn prepared(mut self)->Self{self.frozen=true;self}
    pub fn restart_clock(mut self)->Self{self.start=Instant::now();self.frozen=false;self}
    pub fn progress(self)->f32{
        if self.frozen{return 0.0;}
        (1000.0*self.start.elapsed().as_secs_f32()/self.duration.max(1) as f32).clamp(0.0,1.0)
    }
    pub fn openness(self)->f32{self.from+(self.to-self.from)*ease(self.progress())}
    #[cfg(test)]
    pub(crate) fn finish(mut self)->Self{self.frozen=false;self.start-=std::time::Duration::from_millis(u64::from(self.duration)+1);self}
}
pub(crate) fn ease(value:f32)->f32{1.0-(1.0-value.clamp(0.0,1.0)).powi(3)}
pub(crate) fn zoom(base:f32,open:f32)->f32{base+(1.0-base)*open}
pub(crate) fn inverse(value:f32,anchor:f32,scale:f32)->f32{anchor+(value-anchor)/scale.max(0.01)}

pub(crate) fn system_allowed()->bool{
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW,SPI_GETCLIENTAREAANIMATION};
    let mut active=1i32;
    if !cfg!(test){unsafe{SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION,0,(&mut active as *mut i32).cast(),0);}}
    active!=0
}
#[derive(Clone, Copy, Default)]
pub(super) struct Spring {
    pub value: f32,
    velocity: f32,
}

impl Spring {
    pub fn rest(value: f32) -> Self { Self { value, velocity: 0.0 } }

    // Closed-form critically damped spring. Reversals preserve velocity, and a
    // delayed Windows message does not turn the integration unstable.
    pub fn advance(&mut self, target: f32, dt: f32, frequency: f32, epsilon: f32) -> bool {
        let offset = self.value - target;
        let slope = self.velocity + frequency * offset;
        let decay = (-frequency * dt).exp();
        self.value = target + (offset + slope * dt) * decay;
        self.velocity = (self.velocity - frequency * slope * dt) * decay;
        if (self.value - target).abs() <= epsilon && self.velocity.abs() <= epsilon * frequency {
            *self = Self::rest(target);
            false
        } else {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_round_trip_migration_clamping_and_all_surface_curves(){
        for speed in [25,50,70,75,100,150,200]{
            let policy=Settings{speed,..Default::default()};
            let parsed=Settings::from_json(&policy.json());
            assert_eq!(parsed,policy);
            for curve in [FULL,COMPACT,START,MENU,CONTROL,CALENDAR]{
                let a=Tween::at_speed(true,curve,None,parsed);
                let b=Tween::at_speed(false,curve,None,parsed);
                assert_eq!(a.duration,(curve.open as f64*100.0/speed as f64).round() as u32);
                assert_eq!(b.duration,(curve.close as f64*100.0/speed as f64).round() as u32);
                let mut first=a.prepared();
                first.start-=std::time::Duration::from_secs(8);
                assert_eq!(first.openness(),0.0,"preparation must not consume opening duration");
                let ready=first.restart_clock();
                assert!(!ready.frozen&&ready.duration==a.duration);
                let mut endpoint=b;endpoint.start-=std::time::Duration::from_secs(8);
                assert_eq!(endpoint.openness(),0.0);
            }
            assert_eq!(parsed.duration(300),(30000.0/speed as f64).round() as u32);
            assert_eq!(parsed.duration(PAGE_MS as u32),(22000.0/speed as f64).round() as u32);
            assert_eq!(parsed.duration(FOLDER_MS),(24000.0/speed as f64).round() as u32);
        }
        for invalid in [json!(null),json!(0),json!(-1),json!("NaN"),json!("Infinity")]{assert_eq!(normalize_speed(&invalid),70);}
        assert_eq!(normalize_speed(&json!(0.01)),25);
        assert_eq!(normalize_speed(&json!(9)),200);
        assert_eq!(normalize_speed(&json!(0.77)),75);
        assert_eq!(Settings::from_profile(&json!({}),&json!({"animationSpeed":100})),Settings::default());
        assert_eq!(Settings::from_profile(&json!({}),&json!({"animationSpeed":125})).speed,125);
        assert_eq!(Settings::default().speed,70);
        assert_eq!(Settings::from_profile(&json!({"motion":{"animationSpeed":0.75}}),&json!({})).speed,75);
        assert!(!Settings::from_profile(&json!({"spotlight":{"animations":false}}),&json!({})).enabled);
        assert!(!Settings::from_profile(&json!({}),&json!({"reducedMotion":true})).enabled);
        assert!(!Settings::default().allowed(false));
        assert!(Settings{respect_system:false,..Default::default()}.allowed(false));
        assert!(!Settings{enabled:false,respect_system:false,..Default::default()}.allowed(true));
    }

    #[test]
    #[ignore="isolated GitHub Actions motion policy propagation"]
    fn live_speed_reaches_both_ui_threads_without_restarting_active_tweens(){
        let original=settings();
        configure(Settings{speed:25,..Default::default()});
        let slow=Tween::new(true,FULL,None);
        assert_eq!(slow.duration,1200);
        for speed in [25,50,70,75,100,150,200]{
            configure(Settings{speed,..Default::default()});
            std::thread::spawn(move||{
                assert_eq!(settings().speed,speed);
                for curve in [FULL,COMPACT,START,MENU,CONTROL,CALENDAR]{
                    assert_eq!(Tween::new(true,curve,None).duration,settings().duration(curve.open));
                    assert_eq!(Tween::new(false,curve,None).duration,settings().duration(curve.close));
                }
                assert_eq!(Span::new(FOLDER_MS).duration,settings().duration(FOLDER_MS));
                assert_eq!(Span::new(PAGE_MS as u32).duration,settings().duration(PAGE_MS as u32));
            }).join().unwrap();
            assert_eq!(slow.duration,1200,"in-flight duration is stable");
        }
        configure(Settings{enabled:false,..Default::default()});
        assert!(!allowed());
        configure(original);
        println!("MOTION_SPEED live_policy=PASS all_surfaces=PASS six_speeds=PASS first_frame=PASS");
    }

    #[test]
    fn reversal_starts_at_the_present_value_and_transform_keeps_anchor(){
        let mut opening=Tween::new(true,MENU,None);
        opening.start-=std::time::Duration::from_millis(60);
        let before=opening.openness();
        let closing=Tween::new(false,MENU,Some(opening));
        assert!((before-closing.from).abs()<0.10);
        assert_eq!(closing.to,0.0);
        assert_eq!(inverse(20.0,20.0,MENU.scale),20.0);
        assert!((inverse(20.0+42.0*MENU.scale,20.0,MENU.scale)-62.0).abs()<0.01);
    }

    #[test]
    fn elapsed_time_not_timer_count_controls_the_spring() {
        let mut fine = Spring::default();
        let mut coarse = fine;
        for _ in 0..8 { fine.advance(1.0, 0.01, 22.0, 0.0001); }
        coarse.advance(1.0, 0.08, 22.0, 0.0001);
        assert!((fine.value - coarse.value).abs() < 0.00001);
        assert!((fine.velocity - coarse.velocity).abs() < 0.00001);
    }

    #[test]
    fn rapid_reversal_keeps_position_and_eventually_reaches_exact_rest() {
        let mut spring = Spring::default();
        assert!(spring.advance(1.0, 0.055, 24.0, 0.001));
        let previous = spring.value;
        assert!(spring.advance(0.0, 0.001, 24.0, 0.001));
        assert!((spring.value - previous).abs() < 0.02);
        for _ in 0..100 { spring.advance(0.0, 0.016, 24.0, 0.001); }
        assert_eq!(spring.value, 0.0);
        assert!(!spring.advance(0.0, 0.016, 24.0, 0.001));
    }
}
