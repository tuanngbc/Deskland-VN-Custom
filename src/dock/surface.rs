//! Dock surface: antialiased cached pixels; DWM composes the window.
//! No desktop capture, timer, or per-frame allocation in the static path.
use super::{appearance::Theme, geometry::{Layout,Rect}, glass_probe::Layer};
use std::{collections::{HashMap,VecDeque}, sync::{Arc,Weak}};
use windows_sys::Win32::Foundation::HWND;
use crate::launcher::IconPixels;
use super::UtilityWidget;

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
pub struct Surface { layer:Layer, glass:Rect, material:Material, static_pixels:Vec<u8>, icons:IconRasters }
impl Surface {
    pub unsafe fn new(w:i32,h:i32,glass:Rect,material:Material)->Option<Self>{
        let layer=Layer::new(w,h)?;
        Some(Self{layer,glass,material,static_pixels:Vec::new(),icons:IconRasters::default()})
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
                self.icons.paint(data,w,h,*slot,icon);
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
        let utility=layout.utility;
        if hover==Some(items.len()+1){rounded(data,w,h,utility,utility.w*0.22,tint,50);}
        let divider=|a:Rect,b:Rect|{
            if layout.side==super::DockSide::Bottom{
                Rect{x:(a.x+a.w+b.x)*0.5-0.5*scale,y:layout.glass.y+layout.glass.h*0.22,w:scale,h:layout.glass.h*0.56}
            }else{
                Rect{x:layout.glass.x+layout.glass.w*0.22,y:(a.y+a.h+b.y)*0.5-0.5*scale,w:layout.glass.w*0.56,h:scale}
            }
        };
        if utility.w>0.0 {
            let group=if layout.utility_leading{divider(utility,layout.slots.first().copied().unwrap_or(scan))}
                else{divider(scan,utility)};
            rounded(data,w,h,group,scale*0.5,tint,82);
        }
        if let Some(split)=items.windows(2).position(|pair|pair[0].pinned&&!pair[1].pinned){
            let group=divider(layout.slots[split],layout.slots[split+1]);
            rounded(data,w,h,group,scale*0.5,tint,58);
        }
        for (i,(widget,slot)) in layout.inline.iter().copied().enumerate(){
            if hover==Some(items.len()+2+i){rounded(data,w,h,slot,slot.w*0.22,tint,50);}
            status_on(data,w,h,slot,scale,tint,Some(widget));
        }
        status_on(data,w,h,utility,scale,tint,None);
        self.layer.present(hwnd)
    }
}

fn segment_distance(x:f32,y:f32,ax:f32,ay:f32,bx:f32,by:f32)->f32{
    let (dx,dy)=(bx-ax,by-ay);
    let t=(((x-ax)*dx+(y-ay)*dy)/(dx*dx+dy*dy).max(0.001)).clamp(0.0,1.0);
    (x-ax-dx*t).hypot(y-ay-dy*t)
}
pub(super) fn distance(rect:Rect,r:f32,x:f32,y:f32)->f32{
    let qx=(x-rect.x-rect.w*0.5).abs()-(rect.w*0.5-r);
    let qy=(y-rect.y-rect.h*0.5).abs()-(rect.h*0.5-r);
    qx.max(0.0).hypot(qy.max(0.0))+qx.max(qy).min(0.0)-r
}
pub(super) fn draw(data:&mut [u8],w:i32,h:i32,r:Rect,f:impl Fn(f32,f32)->Option<([u8;3],f32)>){
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
pub(super) fn fallback_icon(data:&mut[u8],w:i32,h:i32,slot:Rect,tint:[u8;3]){
    let inner=Rect{x:slot.x+slot.w*0.24,y:slot.y+slot.h*0.24,w:slot.w*0.52,h:slot.h*0.52};
    rounded(data,w,h,inner,slot.w*0.13,tint,84);
    for (u,v) in [(0.39,0.39),(0.61,0.39),(0.39,0.61),(0.61,0.61)]{
        let k=slot.w*0.06;
        rounded(data,w,h,Rect{x:slot.x+u*slot.w-k,y:slot.y+v*slot.h-k,w:k*2.0,h:k*2.0},k,tint,188);
    }
}
fn status_on(data:&mut[u8],w:i32,h:i32,cell:Rect,scale:f32,tint:[u8;3],widget:Option<UtilityWidget>){
    use UtilityWidget::*;
    if cell.w<=0.0||cell.h<=0.0{return;}
    let edge=(18.0*scale).min(cell.w.min(cell.h));
    let px=(cell.x+(cell.w-edge)*0.5).round();
    let py=(cell.y+(cell.h-edge)*0.5).round();
    let ink=Rect{x:px,y:py,w:edge,h:edge};
    let line=|x,y,ax,ay,bx,by|segment_distance(x,y,ax,ay,bx,by);
    // Ring segment used by the original Wi-Fi and sound glyphs; round caps
    // are kept at the arc endpoints when the test point lies outside the sweep.
    let arc=|x:f32,y:f32,cx:f32,cy:f32,r:f32,ux:f32,uy:f32|{
        let (dx,dy)=(x-cx,y-cy);
        let len=dx.hypot(dy);
        let center=dx*ux+dy*uy;
        if center>=len*0.68 {(len-r).abs()}else{
            let lateral=(dy*ux-dx*uy).signum()*0.7332;
            (dx-r*(ux*0.68-uy*lateral)).hypot(dy-r*(uy*0.68+ux*lateral))
        }
    };
    draw(data,w,h,ink,|x,y|{
        let (x,y)=((x-px)/edge,(y-py)/edge);
        let d=match widget {
            None=>[0.22_f32,0.50,0.78].into_iter().map(|cx|(x-cx).hypot(y-0.50)-0.045).fold(f32::INFINITY,f32::min),
            Some(Network)=>{
                let bands=[0.54,0.365,0.195].into_iter().map(|r|arc(x,y,0.50,0.73,r,0.0,-1.0)).fold(f32::INFINITY,f32::min);
                bands.min((x-0.50).hypot(y-0.76)-0.025)
            }
            Some(Calendar)=>((x-0.50).hypot(y-0.50)-0.39).abs()
                .min(line(x,y,0.50,0.28,0.50,0.51)).min(line(x,y,0.50,0.51,0.65,0.59)),
            Some(Sound)=>{
                let horn=[(0.10,0.39,0.25,0.39),(0.10,0.39,0.10,0.61),(0.10,0.61,0.25,0.61),
                    (0.25,0.39,0.48,0.22),(0.25,0.61,0.48,0.78),(0.48,0.22,0.48,0.78)]
                    .into_iter().map(|(ax,ay,bx,by)|line(x,y,ax,ay,bx,by)).fold(f32::INFINITY,f32::min);
                horn.min(arc(x,y,0.47,0.50,0.24,1.0,0.0)).min(arc(x,y,0.47,0.50,0.45,1.0,0.0))
            }
            Some(widget)=>widget.glyph().iter().map(|&(ax,ay,bx,by)|{
                // Expand each control equally about its optical center.
                let pos=|p:f32|0.5+(p-0.5)*1.17;
                line(x,y,pos(ax),pos(ay),pos(bx),pos(by))
            }).fold(f32::INFINITY,f32::min),
        };
        let stroke=(0.78*scale).max(0.66);
        Some((tint,(0.5+stroke-edge*d).clamp(0.0,1.0)*247.0))
    });
}

struct Raster { width:i32, height:i32, pixels:Vec<u8> }
const RASTER_BUDGET:usize=384*1024; // per visible Dock/Stage surface
const RASTER_ITEMS:usize=32;
#[derive(Default)]
pub(super) struct IconRasters {recent:VecDeque<(Weak<IconPixels>,Raster)>,bytes:usize}
impl IconRasters {
    pub fn paint(&mut self,data:&mut[u8],w:i32,h:i32,slot:Rect,source:&Arc<IconPixels>){
        if source.width<=0||source.height<=0{return;}
        let size=(slot.w.min(slot.h)*0.84).round().max(1.0);
        let longest=source.width.max(source.height) as f32;
        let (tw,th)=((size*source.width as f32/longest).round().max(1.0) as i32,
            (size*source.height as f32/longest).round().max(1.0) as i32);
        let index=self.recent.iter().position(|(old,r)|old.as_ptr()==Arc::as_ptr(source)&&r.width==tw&&r.height==th);
        let entry=if let Some(index)=index {
            self.recent.remove(index).unwrap()
        }else{
            let raster=resample(source,tw,th);
            let used=raster.pixels.len();
            while self.bytes+used>RASTER_BUDGET||self.recent.len()>=RASTER_ITEMS {
                let Some((_,old))=self.recent.pop_front() else{break;};
                self.bytes-=old.pixels.len();
            }
            self.bytes+=used;
            (Arc::downgrade(source),raster)
        };
        let raster=&entry.1;
        let left=(slot.x+(slot.w-tw as f32)*0.5).round() as i32;
        let top=(slot.y+(slot.h-th as f32)*0.5).round() as i32;
        for y in 0.max(-top)..th.min(h-top){
            for x in 0.max(-left)..tw.min(w-left) {
                let p=((y*tw+x)*4) as usize;
                let d=(((top+y)*w+left+x)*4) as usize;
                let inverse=255-raster.pixels[p+3] as u32;
                if inverse==255{continue;}
                for c in 0..4{
                    data[d+c]=(raster.pixels[p+c] as u32+(data[d+c] as u32*inverse+127)/255).min(255) as u8;
                }
            }
        }
        if self.bytes<=RASTER_BUDGET{self.recent.push_back(entry);}else{self.bytes-=entry.1.pixels.len();}
    }
}

fn resample(icon:&IconPixels,width:i32,height:i32)->Raster{
    // Integrate on downscale and use linear sampling only when the shell image
    // is too small. Keep the native premultiplied BGRA in every intermediate.
    fn axis(pixel:i32,extent:i32,src:i32)->(usize,usize,f32,f32){
        let step=src as f32/extent as f32;
        let (lo,hi)=if step>=1.0{
            (pixel as f32*step,((pixel+1) as f32*step).min(src as f32))
        }else{
            let at=((pixel as f32+0.5)*step-0.5).clamp(0.0,(src-1) as f32);
            (at,at+1.0)
        };
        (lo.floor() as usize,(hi.ceil() as usize).min(src as usize),lo,hi)
    }
    let xaxis=(0..width).map(|x|axis(x,width,icon.width)).collect::<Vec<_>>();
    let mut pixels=Vec::with_capacity((width*height*4) as usize);
    for y in 0..height{
        let (t,b,ay,by)=axis(y,height,icon.height);
        for &(l,r,ax,bx) in &xaxis{
            let mut out=[0.0;4];let mut sum=0.0;
            for yy in t..b{let wy=((yy+1) as f32).min(by)-(yy as f32).max(ay);
                for xx in l..r{
                    let wx=((xx+1) as f32).min(bx)-(xx as f32).max(ax);
                    let weight=wx*wy;
                    let at=(yy*icon.width as usize+xx)*4;
                    for c in 0..4{out[c]+=icon.pixels[at+c] as f32*weight;}
                    sum+=weight;
                }
            }
            if sum>0.0{pixels.extend(out.map(|c|(c/sum).round() as u8));}
            else{pixels.extend([0;4]);}
        }
    }
    Raster{width,height,pixels}
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn premultiplied_sampling_preserves_channel_order_and_alpha(){
        let icon=IconPixels{width:2,height:1,pixels:vec![20,40,140,200,0,0,0,0]};
        assert_eq!(resample(&icon,1,1).pixels,[10,20,70,100]);
        let expanded=resample(&icon,4,1);
        assert_eq!(&expanded.pixels[..4],&[20,40,140,200]);
        assert_eq!(&expanded.pixels[12..],&[0,0,0,0]);
    }
    #[test]
    fn raster_cache_is_bounded_when_hover_cycles_through_physical_sizes(){
        let icon=Arc::new(IconPixels{width:2,height:2,pixels:vec![90;16]});
        let mut cache=IconRasters::default();
        let mut canvas=vec![0;128*128*4];
        for edge in 16..150{
            cache.paint(&mut canvas,128,128,Rect{x:0.0,y:0.0,w:edge as f32,h:edge as f32},&icon);
            assert!(cache.bytes<=RASTER_BUDGET&&cache.recent.len()<=RASTER_ITEMS);
        }
        assert!(!cache.recent.is_empty());
    }
}
