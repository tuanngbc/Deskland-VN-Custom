//! Dock surface: antialiased cached pixels; DWM composes the window.
//! No desktop capture, timer, or per-frame allocation in the static path.
use super::{appearance::Theme, geometry::{Layout,Rect}, glass_probe::Layer};
use std::{collections::HashMap, sync::Arc};
use windows_sys::Win32::Foundation::HWND;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material { pub dark:bool, pub transparency:u32, pub reduced_motion:bool }
impl Material {
    pub fn system(theme:Theme,transparency:u32,reduced_motion:bool)->Self{
        let dark=match theme {
            Theme::Light=>false,Theme::Dark=>true,
            Theme::Auto=>crate::lock::reg_get_dword(windows_sys::Win32::System::Registry::HKEY_CURRENT_USER,
                r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize","AppsUseLightTheme").unwrap_or(0)==0,
        };
        Self{dark,transparency,reduced_motion}
    }
}

pub type Atlas = HashMap<String, Option<Arc<crate::launcher::IconPixels>>>;
pub struct Surface { layer:Layer, glass:Rect, material:Material, static_pixels:Vec<u8> }
impl Surface {
    pub unsafe fn new(w:i32,h:i32,glass:Rect,material:Material)->Option<Self>{
        let layer=Layer::new(w,h)?;
        Some(Self{layer,glass,material,static_pixels:Vec::new()})
    }
    pub fn size(&self)->(i32,i32){self.layer.size}
    pub fn set_material(&mut self,value:Material){
        if self.material!=value{self.material=value;self.static_pixels.clear();}
    }
    pub unsafe fn rail(&mut self,hwnd:HWND,layout:&Layout)->bool{
        let (w,h)=self.layer.size;
        let data=self.layer.pixels();data.fill(0);
        let scale=layout.scale;
        let area=layout.glass;
        let rail=match layout.side{
            super::DockSide::Bottom=>Rect{x:area.x+area.w*0.19,y:area.y+area.h-4.0*scale,w:area.w*0.62,h:4.0*scale},
            super::DockSide::Left=>Rect{x:area.x,y:area.y+area.h*0.19,w:4.0*scale,h:area.h*0.62},
            super::DockSide::Right=>Rect{x:area.x+area.w-4.0*scale,y:area.y+area.h*0.19,w:4.0*scale,h:area.h*0.62},
        };
        rounded(data,w,h,rail,rail.w.min(rail.h)*0.5,if self.material.dark{[232,236,244]}else{[31,43,51]},90);
        self.layer.present(hwnd)
    }
    pub unsafe fn paint(&mut self,hwnd:HWND,layout:&Layout,items:&[super::Item],atlas:&Atlas,active:isize,hover:Option<usize>)->bool{
        let (w,h)=self.layer.size;
        let scale=layout.scale;
        if self.glass!=layout.glass||self.static_pixels.is_empty() {
            self.glass=layout.glass;
            self.static_pixels.resize(w as usize*h as usize*4,0);
            let rect=self.glass;
            let tint=self.material;
            let radius=rect.w.min(rect.h)*0.5;
            for (i,p) in self.static_pixels.chunks_exact_mut(4).enumerate() {
                let x=(i%w as usize) as f32+0.5;
                let y=(i/w as usize) as f32+0.5;
                let d=distance(rect,radius,x,y);
                let coverage=(0.5-d).clamp(0.0,1.0);
                let shadow=if tint.reduced_motion{0.0}else{(1.0-(d/(6.5*scale)).max(0.0)).clamp(0.0,1.0).powi(2)*0.22*(1.0-coverage)};
                let edge=(1.2+d/scale).clamp(0.0,1.0)*0.28;
                let reflection=(1.0-((y-rect.y)/rect.h).clamp(0.0,1.0)).powi(5)*0.25;
                let opacity=((100-tint.transparency) as f32*0.01+edge+reflection).clamp(0.0,0.96);
                let tone=if tint.dark{(29.0+124.0*(edge+reflection)).min(255.0)}else{(224.0+46.0*(edge+reflection)).min(255.0)};
                let premul=tone*coverage*opacity;
                p[0]=(premul*1.04).round().clamp(0.0,255.0) as u8;
                p[1]=premul.round() as u8;p[2]=(premul*0.97).round() as u8;
                p[3]=((coverage*opacity+shadow)*255.0).round() as u8;
                for c in 0..3{p[c]=p[c].min(p[3]);}
            }
        }
        let data=self.layer.pixels();
        data.copy_from_slice(&self.static_pixels);
        for (i,(slot,item)) in layout.slots.iter().zip(items).enumerate(){
            let front=item.hwnd as isize==active && active!=0;
            let tint=if self.material.dark{[245,248,252]}else{[58,69,78]};
            if front||hover==Some(i){rounded(data,w,h,*slot,slot.w*0.22,tint,if front{54}else{32});}
            if let Some(Some(icon))=atlas.get(&item.path){
                icon_on(data,w,h,*slot,icon);
            }else{
                fallback_icon(data,w,h,*slot,tint);
            }
            let (x,y)=layout.indicators[i];
            if front || item.count>0 || item.pinned {
                let k=if front{3.5}else if item.count>0{2.4}else{1.6}*scale;
                rounded(data,w,h,Rect{x:x-k,y:y-k,w:k*2.0,h:k*2.0},k,tint,if item.count>0{248}else{104});
            }
        }
        let scan=layout.scan;
        let tint=if self.material.dark{[238,244,252]}else{[43,53,64]};
        if hover==Some(items.len()){rounded(data,w,h,scan,scan.w*0.22,tint,50);}
        let center=(scan.x+0.45*scan.w,scan.y+0.44*scan.h);
        let radius=scan.w*0.19;
        draw(data,w,h,scan,|x,y|{
            let ring=((x-center.0).hypot(y-center.1)-radius).abs()-0.85*scale;
            let tail=segment_distance(x,y,center.0+radius*0.65,center.1+radius*0.65,scan.x+scan.w*0.73,scan.y+scan.h*0.73)-0.88*scale;
            Some((tint,(0.5-ring.min(tail)).clamp(0.0,1.0)*238.0))
        });
        self.layer.present(hwnd)
    }
}

fn segment_distance(x:f32,y:f32,ax:f32,ay:f32,bx:f32,by:f32)->f32{
    let (dx,dy)=(bx-ax,by-ay);
    let t=(((x-ax)*dx+(y-ay)*dy)/(dx*dx+dy*dy).max(0.001)).clamp(0.0,1.0);
    (x-ax-dx*t).hypot(y-ay-dy*t)
}
fn distance(rect:Rect,r:f32,x:f32,y:f32)->f32{
    let qx=(x-rect.x-rect.w*0.5).abs()-(rect.w*0.5-r);
    let qy=(y-rect.y-rect.h*0.5).abs()-(rect.h*0.5-r);
    qx.max(0.0).hypot(qy.max(0.0))+qx.max(qy).min(0.0)-r
}
fn draw(data:&mut [u8],w:i32,h:i32,r:Rect,f:impl Fn(f32,f32)->Option<([u8;3],f32)>){
    for y in (r.y.floor() as i32).clamp(0,h)..((r.y+r.h).ceil() as i32).clamp(0,h){
        for x in (r.x.floor() as i32).clamp(0,w)..((r.x+r.w).ceil() as i32).clamp(0,w){
            let Some((rgb,a))=f(x as f32+0.5,y as f32+0.5) else{continue};
            let a=a.round().clamp(0.0,255.0) as u32;
            if a==0{continue;}
            let i=((y*w+x)*4) as usize;
            for c in 0..3{data[i+c]=((a*rgb[c] as u32+(255-a)*data[i+c] as u32+127)/255) as u8;}
            data[i+3]=(a+(255-a)*data[i+3] as u32/255) as u8;
        }
    }
}
pub(super) fn rounded(data:&mut [u8],w:i32,h:i32,r:Rect,radius:f32,tint:[u8;3],opacity:u8){
    draw(data,w,h,r,|x,y|Some((tint,(0.5-distance(r,radius,x,y)).clamp(0.0,1.0)*opacity as f32)));
}
fn fallback_icon(data:&mut[u8],w:i32,h:i32,slot:Rect,tint:[u8;3]){
    let inner=Rect{x:slot.x+slot.w*0.24,y:slot.y+slot.h*0.24,w:slot.w*0.52,h:slot.h*0.52};
    rounded(data,w,h,inner,slot.w*0.13,tint,84);
    for (u,v) in [(0.39,0.39),(0.61,0.39),(0.39,0.61),(0.61,0.61)]{
        let k=slot.w*0.06;
        rounded(data,w,h,Rect{x:slot.x+u*slot.w-k,y:slot.y+v*slot.h-k,w:k*2.0,h:k*2.0},k,tint,188);
    }
}
fn icon_on(data:&mut[u8],w:i32,h:i32,slot:Rect,icon:&crate::launcher::IconPixels){
    let source_w=icon.width as usize;let source_h=icon.height as usize;
    if source_w==0||source_h==0{return;}
    let inset=slot.w*0.08;
    let target=Rect{x:slot.x+inset,y:slot.y+inset,w:slot.w-2.0*inset,h:slot.h-2.0*inset};
    draw(data,w,h,target,|x,y|{
        let a=((x-target.x)/target.w*source_w as f32-0.5).clamp(0.0,(source_w-1) as f32);
        let b=((y-target.y)/target.h*source_h as f32-0.5).clamp(0.0,(source_h-1) as f32);
        let (i,j)=(a as usize,b as usize);
        let (u,v)=(a-a.floor(),b-b.floor());
        let mut out=[0.0;4];
        for (xx,yy,weight) in [(i,j,(1.0-u)*(1.0-v)),((i+1).min(source_w-1),j,u*(1.0-v)),
            (i,(j+1).min(source_h-1),(1.0-u)*v),((i+1).min(source_w-1),(j+1).min(source_h-1),u*v)]{
            let k=(yy*source_w+xx)*4;
            for c in 0..4{out[c]+=icon.pixels[k+c] as f32*weight;}
        }
        Some(([out[0].round() as u8,out[1].round() as u8,out[2].round() as u8],out[3]))
    });
}
