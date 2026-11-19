//! Deterministic offshore routes and continuous, vertically articulated motion.
use crate::engine::{sstep, Rng, TASKBAR};
use crate::palm_layout::Viewport;
use std::f32::consts::PI;

#[derive(Clone, Copy, Debug)]
pub struct Motion {
    pub surface: f32,
    pub depth: f32,
    pub pitch: f32,
    pub roll: f32,
    pub arch: f32,
    pub stroke: f32,
    pub fin_left: f32,
    pub fin_right: f32,
    pub spray_age: f32,
    pub entry_age: f32,
    pub visibility: f32,
    pub juvenile: f32,
}

impl Motion {
    pub fn at(age: f32) -> Self {
        Self::for_whale(age,0.0)
    }

    pub fn for_whale(age:f32,variant:f32)->Self {
        let t=(age/(1.0+variant*0.035)).rem_euclid(36.0);
        // Continuous cubic curves for centre depth and pitch. The animal really
        // rotates through the water plane: head first, raised flukes last.
        const KEYS:[[f32;3];12]=[
            [0.0,-48.0,0.12],[2.0,-24.0,0.20],[4.0,-8.0,0.08],
            [7.0,-7.0,0.01],[10.0,-8.0,-0.03],[12.0,-7.0,-0.13],
            [14.0,-8.0,-0.42],[16.0,-15.0,-0.79],[18.0,-36.0,-0.82],
            [21.0,-72.0,-0.28],[28.0,-88.0,0.04],[33.0,-72.0,0.16],
        ];
        let index=KEYS.iter().rposition(|k|k[0]<=t).unwrap_or(0);
        let get=|offset:isize|{
            let n=index as isize+offset;
            let mut k=KEYS[n.rem_euclid(KEYS.len() as isize) as usize];
            k[0]+=n.div_euclid(KEYS.len() as isize) as f32*36.0;
            k
        };
        let (a,b,c,d)=(get(-1),get(0),get(1),get(2));
        let span=c[0]-b[0];let u=(t-b[0])/span;
        let curve=|axis:usize|{
            let m0=(c[axis]-a[axis])/(c[0]-a[0])*span;
            let m1=(d[axis]-b[axis])/(d[0]-b[0])*span;
            (2.0*u*u*u-3.0*u*u+1.0)*b[axis]+(u*u*u-2.0*u*u+u)*m0
                +(-2.0*u*u*u+3.0*u*u)*c[axis]+(u*u*u-u*u)*m1
        };
        let depth=curve(1).min(-5.0);
        Self {
            surface:1.0-sstep(9.0,30.0,-depth),depth,pitch:curve(2),
            roll:(age*0.39+variant*1.7).sin()*0.035,
            arch:sstep(10.0,13.0,t)*(1.0-sstep(15.0,18.0,t)),
            stroke:age*(1.42-variant*0.08)+variant*1.8,
            fin_left:(age*0.54+0.7+variant).sin()*0.13,
            fin_right:(age*0.47+2.3+variant).sin()*0.12,
            spray_age:(t-5.0)/2.8,entry_age:(t-17.2)/5.0,
            visibility:1.0,juvenile:0.0,
        }
    }

    pub fn uniforms(&self)->([f32;4],[f32;4],[f32;4]) {
        ([self.depth,self.pitch,self.roll,self.arch],
         [self.stroke,self.fin_left,self.fin_right,self.surface],
         [self.spray_age,self.entry_age,self.surface,self.visibility])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Whale {
    pub x: f32,
    pub y: f32,
    pub heading: f32,
    pub scale: f32,
    pub age: f32,
    pub variant: f32,
    turn_rate: f32,
    speed: f32,
    output: Viewport,
    juvenile: bool,
    visibility: f32,
}

impl Whale {
    pub fn surface(&self) -> f32 {
        self.motion().surface
    }

    pub fn motion(&self) -> Motion {
        // Nursing calves surface more often and make shallower dives. They do
        // not share a rigid, frame-identical pose with the accompanying mother.
        let mut motion=Motion::for_whale(self.age*if self.juvenile {1.32}else{1.0},self.variant);
        if self.juvenile {motion.depth*=0.62;motion.pitch*=0.72;motion.juvenile=1.0;}
        motion.visibility=self.visibility;
        motion
    }

    fn fits(&self, x: f32, y: f32) -> bool {
        let [ox, oy, w, h] = self.output;
        // Include the upright spray, tail and wake, and leave the taskbar clear.
        // Adults reserve room for a calf on either side before turning ashore.
        let side=if self.juvenile {110.0}else{130.0};
        let bottom=if self.juvenile {100.0}else{120.0};
        x > ox + side * self.scale && x < ox + w - side * self.scale
            && y > oy + 150.0 * self.scale && y < oy + h - TASKBAR - bottom * self.scale
    }
}

pub struct Pod {
    pub whales: Vec<Whale>,
    pub calf: Option<Whale>,
    want: usize,
    seed: u64,
    family_wait: f32,
    family_age: f32,
    family_duration: f32,
    family_parent: usize,
    family_side: f32,
    family_event: u64,
    family_departing: bool,
}

impl Pod {
    pub fn new(want: usize, seed: u64) -> Self {
        let mut rng=Rng::new(seed^0x43414c46);
        Self { whales: Vec::with_capacity(want.min(3)), calf:None, want: want.min(3), seed,
            family_wait:5.0+rng.next()*5.0,family_age:0.0,family_duration:0.0,
            family_parent:0,family_side:1.0,family_event:0,family_departing:false }
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
                    scale, age: 2.0 + n as f32 * 11.0,
                    variant:n as f32,turn_rate:0.0,
                    speed: 10.0 + n as f32 * 1.6, output,juvenile:false,visibility:1.0,
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
        if dt==0.0 {return;}
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
            let target_rate=(d.sin().atan2(d.cos())*0.65).clamp(-0.17,0.17);
            whale.turn_rate+=(target_rate-whale.turn_rate)*(1.0-(-dt*1.3).exp());
            whale.heading+=whale.turn_rate*dt;
            let speed = whale.speed * (0.73 + 0.12*whale.motion().stroke.sin() + 0.15*whale.surface());
            let (nx, ny) = (whale.x+whale.heading.cos()*speed*dt, whale.y+whale.heading.sin()*speed*dt);
            if whale.fits(nx, ny) && sample(nx, ny) > 105.0*whale.scale {
                whale.x = nx; whale.y = ny;
            }
        }
        self.step_family(dt,&sample);
    }

    fn family_position(mother:&Whale,side:f32,age:f32)->(f32,f32) {
        let behind=(-28.0+(age*0.27).sin()*4.0)*mother.scale;
        let beside=(62.0+(age*0.19).sin()*3.0)*mother.scale*side;
        (mother.x+mother.heading.cos()*behind-mother.heading.sin()*beside,
         mother.y+mother.heading.sin()*behind+mother.heading.cos()*beside)
    }

    fn step_family(&mut self,dt:f32,sample:&impl Fn(f32,f32)->f32) {
        if self.whales.is_empty() {return;}
        if self.calf.is_none() {
            self.family_wait-=dt;
            if self.family_wait>0.0 {return;}
            self.family_event+=1;
            let mut rng=Rng::new(self.seed^self.family_event.wrapping_mul(0x9e3779b97f4a7c15));
            self.family_wait=12.0; // Retry offshore if the selected coast is narrow.
            let first=(rng.next()*self.whales.len() as f32) as usize%self.whales.len();
            for offset in 0..self.whales.len() {
                let parent=(first+offset)%self.whales.len();
                let mother=&self.whales[parent];
                let side=if rng.next()<0.5 {-1.0}else{1.0};
                for side in [side,-side] {
                    let (x,y)=Self::family_position(mother,side,0.0);
                    let mut calf=mother.clone();
                    calf.x=x;calf.y=y;calf.scale=mother.scale*(0.40+rng.next()*0.06);
                    calf.age=0.0;calf.variant=mother.variant+0.4;
                    calf.juvenile=true;calf.visibility=0.0;
                    if calf.fits(x,y)&&sample(x,y)>110.0*calf.scale {
                        self.calf=Some(calf);self.family_parent=parent;self.family_side=side;
                        self.family_age=0.0;self.family_duration=85.0+rng.next()*55.0;
                        self.family_departing=false;
                        crate::log!("Bãi biển: cặp cá voi mẹ–con xuất hiện, cá con {:.0}% kích cỡ mẹ",self.calf.as_ref().unwrap().scale/mother.scale*100.0);
                        return;
                    }
                }
            }
            return;
        }
        self.family_age+=dt;
        if !self.family_departing && self.family_age>=self.family_duration
            && self.calf.as_ref().unwrap().motion().depth < -35.0 {
            self.family_departing=true;self.family_duration=self.family_age+6.0;
        }
        if self.family_departing && self.family_age>=self.family_duration {
            self.calf=None;
            let mut rng=Rng::new(self.seed^self.family_event.wrapping_mul(0x517cc1b727220a95));
            self.family_wait=75.0+rng.next()*105.0;
            return;
        }
        let mother=&self.whales[self.family_parent];
        let (tx,ty)=Self::family_position(mother,self.family_side,self.family_age);
        let calf=self.calf.as_mut().unwrap();
        let blend=1.0-(-dt*1.15).exp();
        let (nx,ny)=(calf.x+(tx-calf.x)*blend,calf.y+(ty-calf.y)*blend);
        if calf.fits(nx,ny)&&sample(nx,ny)>105.0*calf.scale {calf.x=nx;calf.y=ny;}
        let angle=mother.heading-calf.heading;
        calf.heading+=angle.sin().atan2(angle.cos())*(1.0-(-dt*1.4).exp());
        calf.age+=dt;
        let separation=(calf.x-tx).hypot(calf.y-ty);
        calf.visibility=sstep(0.0,4.0,self.family_age)
            *if self.family_departing {1.0-sstep(self.family_duration-6.0,self.family_duration,self.family_age)}else{1.0}
            *(1.0-sstep(45.0,100.0,separation));
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
            if let Some(calf)=&pod.calf {
                assert!(calf.fits(calf.x,calf.y));
                assert!(sea(calf.x,calf.y)>105.0*calf.scale);
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
        for _ in 0..1200 {
            pod.step(1.0/30.0, |_,_|999.0);
            let mut active = 0;
            for (i,w) in pod.whales.iter().enumerate() {
                if (0.0..1.0).contains(&w.motion().spray_age) { breaths[i]+=1; active+=1; }
                assert!((0.0..=1.0).contains(&w.surface()));
            }
            assert!(active<=1);
        }
        assert!(breaths.iter().all(|&n| n>=80 && n<=175));
        let frozen = pod.whales.clone();
        pod.step(0.0, |_,_|999.0);
        assert_eq!(pod.whales,frozen);
    }

    #[test]
    fn dive_submerges_head_and_back_before_the_raised_flukes() {
        let afloat = Motion::at(6.0);
        assert!(afloat.depth>-9.0 && afloat.depth<0.0);
        assert!((0.0..1.0).contains(&afloat.spray_age));
        let diving=Motion::at(16.0);
        let height=|x:f32,z:f32|diving.depth+diving.pitch.sin()*x+diving.pitch.cos()*z;
        assert!(height(45.0,8.0) < -30.0,"head must actually pass under water");
        assert!(height(-72.0,0.0)>25.0,"flukes must physically lift above water");
        assert!(Motion::at(25.0).depth < -80.0);
        let mut previous=Motion::at(0.0);
        for n in 1..=3600 {
            let current=Motion::at(n as f32/100.0);
            assert!((previous.depth-current.depth).abs()<0.30,"depth jumps");
            assert!((previous.pitch-current.pitch).abs()<0.01,"pitch jumps");
            assert!((previous.fin_left-current.fin_left).abs()<0.01,"fin jumps");
            previous=current;
        }
    }

    #[test]
    fn course_changes_have_inertia_instead_of_pivoting_on_the_spot() {
        let mut pod=Pod::new(1,17);
        pod.populate(&[[0.0,0.0,1280.0,800.0]],|_,_|999.0);
        let mut previous=pod.whales[0].clone();
        for _ in 0..3600 {
            pod.step(1.0/30.0,|_,_|999.0);
            let current=&pod.whales[0];
            assert!((current.turn_rate-previous.turn_rate).abs()<0.015);
            assert!((current.heading-previous.heading).abs()<0.006);
            previous=current.clone();
        }
    }

    #[test]
    fn calf_follows_a_mother_with_smooth_appearances_and_freezes_when_paused() {
        let mut pod=Pod::new(3,12345);
        pod.populate(&[[0.0,0.0,2560.0,1440.0]],|_,_|999.0);
        let mut previous:Option<(u64,Whale)>=None;
        let mut visible=0;
        for _ in 0..12_000 {
            pod.step(1.0/30.0,|_,_|999.0);
            if let Some(calf)=&pod.calf {
                let mother=&pod.whales[pod.family_parent];
                assert!((0.39..0.47).contains(&(calf.scale/mother.scale)));
                assert!((calf.x-mother.x).hypot(calf.y-mother.y)<120.0*mother.scale);
                assert!(calf.fits(calf.x,calf.y));
                assert!(calf.motion().juvenile==1.0);
                assert!(calf.motion().depth>=-60.0);
                if calf.visibility>0.9 {visible+=1;}
                if let Some((event,old))=&previous {
                    if *event==pod.family_event {
                        assert!((calf.x-old.x).hypot(calf.y-old.y)<3.0,"calf teleported");
                        assert!((calf.visibility-old.visibility).abs()<0.03,"calf popped in/out");
                    }
                }
                previous=Some((pod.family_event,calf.clone()));
            } else {previous=None;}
        }
        assert!(visible>500);assert!(pod.family_event>=2);
        let frozen=(pod.whales.clone(),pod.calf.clone(),pod.family_age,pod.family_wait);
        pod.step(0.0,|_,_|999.0);
        assert_eq!(frozen,(pod.whales.clone(),pod.calf.clone(),pod.family_age,pod.family_wait));
        let mut empty=Pod::new(0,17);
        for _ in 0..1000 {empty.step(0.1,|_,_|999.0);}
        assert!(empty.calf.is_none());
    }

    #[test]
    fn mother_selection_and_appearance_timing_vary_with_scene_seed() {
        let mut parents=std::collections::BTreeSet::new();
        let mut times=std::collections::BTreeSet::new();
        for seed in 1..20 {
            let mut pod=Pod::new(3,seed);
            pod.populate(&[[0.0,0.0,2560.0,1440.0]],|_,_|999.0);
            times.insert((pod.family_wait*10.0) as u32);
            for _ in 0..150 {pod.step(0.1,|_,_|999.0);}
            assert!(pod.calf.is_some());parents.insert(pod.family_parent);
        }
        assert!(parents.len()>=2);assert!(times.len()>=10);
    }
}
