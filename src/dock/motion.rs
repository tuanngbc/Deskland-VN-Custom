//! Shared elapsed-time native motion. No idle timer is needed at rest.
use std::time::Instant;

#[derive(Clone,Copy)]
pub(crate) struct Curve {pub open:u32,pub close:u32,pub scale:f32}
pub(crate) const FULL:Curve=Curve{open:255,close:180,scale:0.93};
pub(crate) const COMPACT:Curve=Curve{open:200,close:145,scale:0.95};
pub(crate) const START:Curve=Curve{open:205,close:145,scale:0.965};
pub(crate) const MENU:Curve=Curve{open:140,close:90,scale:0.97};
pub(crate) const PANEL:Curve=Curve{open:155,close:105,scale:0.98};
pub(crate) const HOVER_MS:u128=100;
pub(crate) const PAGE_MS:u128=118;
#[derive(Clone,Copy)]
pub(crate) struct Tween {
    pub start:Instant,
    pub duration:u32,
    pub from:f32,
    pub to:f32,
}
impl Tween {
    pub fn new(open:bool,curve:Curve,previous:Option<Self>)->Self{
        Self{start:Instant::now(),duration:if open{curve.open}else{curve.close},
            from:previous.map_or(if open{0.0}else{1.0},Self::openness),to:if open{1.0}else{0.0}}
    }
    pub fn progress(self)->f32{
        (1000.0*self.start.elapsed().as_secs_f32()/self.duration.max(1) as f32).clamp(0.0,1.0)
    }
    pub fn openness(self)->f32{self.from+(self.to-self.from)*ease(self.progress())}
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
