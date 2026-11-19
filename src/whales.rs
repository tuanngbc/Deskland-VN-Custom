//! A small, deterministic pod. Only three bounded quads are shaded; no full-screen
//! whale loop, textures, particles, timers or extra render targets are needed.
use crate::engine::{sstep, Rng, TASKBAR};
use crate::palm_layout::Viewport;
use std::f32::consts::PI;

pub const VERT: &str = include_str!("../assets/shaders/whale.vert");
pub const FRAG: &str = include_str!("../assets/shaders/whale.frag");

#[derive(Clone, Debug, PartialEq)]
pub struct Whale {
    pub x: f32,
    pub y: f32,
    pub heading: f32,
    pub scale: f32,
    pub age: f32,
    speed: f32,
    output: Viewport,
}

impl Whale {
    pub fn surface(&self) -> f32 {
        let phase = self.age.rem_euclid(27.0);
        sstep(1.0, 3.5, phase) * (1.0 - sstep(11.5, 15.0, phase))
    }

    /// One short breath per surface cycle; zero outside this interval.
    pub fn spray_age(&self) -> f32 { (self.age.rem_euclid(27.0) - 5.0) / 3.4 }

    fn fits(&self, x: f32, y: f32) -> bool {
        let [ox, oy, w, h] = self.output;
        // Include the upright spray, tail and wake, and leave the taskbar clear.
        x > ox + 110.0 * self.scale && x < ox + w - 110.0 * self.scale
            && y > oy + 150.0 * self.scale && y < oy + h - TASKBAR - 100.0 * self.scale
    }
}

pub struct Pod {
    pub whales: Vec<Whale>,
    want: usize,
    seed: u64,
}

impl Pod {
    pub fn new(want: usize, seed: u64) -> Self {
        Self { whales: Vec::with_capacity(want.min(3)), want: want.min(3), seed }
    }

    /// Called on shoreline commits, not on every frame. Existing positions stay
    /// fixed when desktop icons are clicked or their distance field is refreshed.
    pub fn populate(&mut self, outputs: &[Viewport], sample: impl Fn(f32, f32) -> f32) {
        if self.whales.len() >= self.want || outputs.is_empty() { return; }
        let mut rng = Rng::new(self.seed);
        for n in self.whales.len()..self.want {
            let mut found = None;
            for attempt in 0..384 {
                let output = outputs[(n + attempt / 128) % outputs.len()];
                let scale = [1.0, 0.86, 1.12][n];
                let mut whale = Whale {
                    x: output[0] + rng.next() * output[2],
                    y: output[1] + rng.next() * output[3],
                    heading: rng.next() * 2.0 * PI,
                    scale, age: 2.0 + n as f32 * 8.3,
                    speed: 10.0 + n as f32 * 1.6, output,
                };
                if !whale.fits(whale.x, whale.y) || sample(whale.x, whale.y) < 150.0 * scale { continue; }
                if self.whales.iter().any(|w| (w.x-whale.x).hypot(w.y-whale.y) < 240.0) { continue; }
                // Start along the coast rather than swimming straight onto it.
                let gx = sample(whale.x+12.0, whale.y)-sample(whale.x-12.0, whale.y);
                let gy = sample(whale.x, whale.y+12.0)-sample(whale.x, whale.y-12.0);
                if gx.hypot(gy) > 0.01 { whale.heading = gy.atan2(gx) + PI * 0.5; }
                found = Some(whale);
                break;
            }
            if let Some(w) = found { self.whales.push(w); } else { break; }
        }
    }

    pub fn step(&mut self, dt: f32, sample: impl Fn(f32, f32) -> f32) {
        let dt = dt.clamp(0.0, 0.1);
        for (i, whale) in self.whales.iter_mut().enumerate() {
            // Advance only with the theme's simulation. A paused wallpaper never
            // advances an independent whale/spray clock or starts an FPS boost.
            whale.age += dt;
            let [ox, oy, w, h] = whale.output;
            let (ax, ay) = (whale.x + whale.heading.cos()*95.0, whale.y + whale.heading.sin()*95.0);
            let target = if !whale.fits(ax, ay) || sample(ax, ay) < 125.0 * whale.scale {
                let mut gx = sample(whale.x+12.0, whale.y)-sample(whale.x-12.0, whale.y);
                let mut gy = sample(whale.x, whale.y+12.0)-sample(whale.x, whale.y-12.0);
                if !whale.fits(ax, ay) {
                    gx += (ox+w*0.5-whale.x)*0.12;
                    gy += (oy+(h-TASKBAR)*0.5-whale.y)*0.12;
                }
                gy.atan2(gx)
            } else {
                whale.heading + (whale.age*0.13+i as f32*2.1).sin()*0.12
            };
            let d = target-whale.heading;
            whale.heading += d.sin().atan2(d.cos()).clamp(-0.55*dt, 0.55*dt);
            let speed = whale.speed * (0.7 + 0.3*whale.surface());
            let (nx, ny) = (whale.x+whale.heading.cos()*speed*dt, whale.y+whale.heading.sin()*speed*dt);
            if whale.fits(nx, ny) && sample(nx, ny) > 105.0*whale.scale {
                whale.x = nx; whale.y = ny;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pod_stays_in_visible_water_on_staggered_outputs() {
        let outputs = [[0.0,0.0,1080.0,1920.0], [1080.0,731.0,2560.0,1080.0]];
        // An island straddles the two outputs near their shared edge.
        let sea = |x:f32,y:f32| (x-1160.0).hypot(y-1000.0)-260.0;
        let mut pod = Pod::new(3, 12345);
        pod.populate(&outputs, sea);
        assert_eq!(pod.whales.len(), 3);
        for _ in 0..18_000 {
            pod.step(1.0/30.0, sea);
            for whale in &pod.whales {
                assert!(whale.fits(whale.x,whale.y));
                assert!(sea(whale.x,whale.y)>105.0*whale.scale);
            }
        }
    }

    #[test]
    fn count_zero_and_no_water_are_valid_and_positions_do_not_reshuffle() {
        let outputs = [[0.0,0.0,2560.0,1080.0]];
        let mut empty = Pod::new(0, 17);
        empty.populate(&outputs, |_,_|999.0);
        assert!(empty.whales.is_empty());
        let mut dry = Pod::new(3, 17);
        dry.populate(&outputs, |_,_|-100.0);
        assert!(dry.whales.is_empty());
        let mut pod = Pod::new(100, 17);
        pod.populate(&outputs, |_,_|999.0);
        let before = pod.whales.clone();
        pod.populate(&outputs, |_,_|999.0);
        assert_eq!(pod.whales, before);
        assert_eq!(before.len(),3);
        let mut same = Pod::new(3,17);
        same.populate(&outputs, |_,_|999.0);
        assert_eq!(same.whales,before);
    }

    #[test]
    fn breath_cycles_are_staggered_and_freeze_without_simulation() {
        let mut pod = Pod::new(3,9);
        pod.populate(&[[0.0,0.0,2560.0,1080.0]], |_,_|999.0);
        let mut breaths = [0usize;3];
        for _ in 0..810 {
            pod.step(1.0/30.0, |_,_|999.0);
            let mut active = 0;
            for (i,w) in pod.whales.iter().enumerate() {
                if (0.0..1.0).contains(&w.spray_age()) { breaths[i]+=1; active+=1; }
                assert!((0.0..=1.0).contains(&w.surface()));
            }
            assert!(active<=1);
        }
        assert!(breaths.iter().all(|&n| n>90 && n<115));
        let frozen = pod.whales.clone();
        pod.step(0.0, |_,_|999.0);
        assert_eq!(pod.whales,frozen);
    }
}
