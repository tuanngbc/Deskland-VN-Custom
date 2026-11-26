// Copyright (c) 2026 Clooudy, MIT (third_party/fluidwin/LICENSE).
// Exact spring integration from FluidWin v1.3.1 fluidwin.cpp passoGelatine,
// commit 2f773a8733b84c7dbee88fff6628d14e9ac802b9. Runtime ownership is separate.
use super::fluidwin_mesh::Vertex;
use windows_sys::Win32::Foundation::{RECT,POINT};
const C:usize=14;const R:usize=11;const N:usize=C*R;
pub(super) struct Jelly{x:[f64;N],y:[f64;N],vx:[f64;N],vy:[f64;N],u:f64,v:f64,pub lift:f64}
impl Jelly{
    pub fn new(rect:RECT,point:POINT)->Self{
        let (w,h)=(f64::from(rect.right-rect.left),f64::from(rect.bottom-rect.top));
        Self{x:std::array::from_fn(|k|f64::from(rect.left)+w*(k%C) as f64/(C-1) as f64),
            y:std::array::from_fn(|k|f64::from(rect.top)+h*(k/C) as f64/(R-1) as f64),vx:[0.0;N],vy:[0.0;N],
            u:(f64::from(point.x-rect.left)/w).clamp(0.0,1.0),v:(f64::from(point.y-rect.top)/h).clamp(0.0,1.0),lift:0.0}
    }
    pub fn step(&mut self,rect:RECT,dt:f64,scale:f64,held:bool)->bool{
        let dt=dt.clamp(1.0/240.0,1.0/30.0);
        self.lift=(self.lift+if held{dt/0.20}else{-dt/0.20}).clamp(0.0,1.0);
        let (w,h)=(f64::from(rect.right-rect.left),f64::from(rect.bottom-rect.top));
        let diag=w.hypot(h);let max_dist=80.0*scale;let h2=dt/4.0;let mut settled=true;
        for r in 0..R{for c in 0..C{
            let k=r*C+c;let (u,v)=(c as f64/(C-1) as f64,r as f64/(R-1) as f64);
            let (ox,oy)=(f64::from(rect.left)+u*w,f64::from(rect.top)+v*h);
            let d=((u-self.u)*w).hypot((v-self.v)*h)/diag.max(1.0);
            let rigidity=300.0+1500.0*(-(d*d)/(0.35*0.35)).exp();let damping=2.0*rigidity.sqrt()*0.50;
            for _ in 0..4{
                self.vx[k]+=(rigidity*(ox-self.x[k])-damping*self.vx[k])*h2;self.x[k]+=self.vx[k]*h2;
                self.vy[k]+=(rigidity*(oy-self.y[k])-damping*self.vy[k])*h2;self.y[k]+=self.vy[k]*h2;
            }
            let glue=(-(d*d)/(0.06*0.06)).exp();self.x[k]+=(ox-self.x[k])*glue;self.y[k]+=(oy-self.y[k])*glue;
            let (dx,dy)=(self.x[k]-ox,self.y[k]-oy);let distance=dx.hypot(dy);
            if distance>max_dist{self.x[k]=ox+dx*max_dist/distance;self.y[k]=oy+dy*max_dist/distance;}
            if distance>0.6*scale||self.vx[k].hypot(self.vy[k])>10.0*scale{settled=false;}
        }}
        settled&&self.lift<=0.0
    }
    pub fn regrab(&mut self,rect:RECT,point:POINT){
        self.u=(f64::from(point.x-rect.left)/f64::from(rect.right-rect.left)).clamp(0.0,1.0);
        self.v=(f64::from(point.y-rect.top)/f64::from(rect.bottom-rect.top)).clamp(0.0,1.0);
    }
    pub fn vertices(&self)->[Vertex;N]{std::array::from_fn(|k|Vertex{x:self.x[k] as f32,y:self.y[k] as f32,u:(k%C) as f32/(C-1) as f32,v:(k/C) as f32/(R-1) as f32})}
}
#[cfg(test)]
#[test]
fn dragging_keeps_anchor_bounded_and_settles_after_release(){
    let rect=RECT{left:40,top:60,right:840,bottom:660};let mut jelly=Jelly::new(rect,POINT{x:440,y:60});
    let moved=RECT{left:1040,right:1840,..rect};jelly.step(moved,1.0/60.0,1.0,true);
    let vertices=jelly.vertices();let mut deformed=false;
    for (k,p) in vertices.iter().enumerate(){
        let ox=moved.left as f32+(k%C) as f32*800.0/(C-1) as f32;let oy=moved.top as f32+(k/C) as f32*600.0/(R-1) as f32;
        let distance=(p.x-ox).hypot(p.y-oy);assert!(distance<=80.001);deformed|=distance>1.0;
    }assert!(deformed);
    let mut settled=false;for _ in 0..240{settled=jelly.step(moved,1.0/60.0,1.0,false);if settled{break;}}
    assert!(settled,"upstream springs must settle within the bounded release interval");
}
