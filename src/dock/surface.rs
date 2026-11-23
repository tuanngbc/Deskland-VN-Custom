//! Dock surface: antialiased cached pixels; DWM composes the window.
//! No desktop capture, timer, or per-frame allocation in the static path.
use super::{appearance::Theme, geometry::{Layout,Rect}, glass_probe::Layer};
use std::{collections::{HashMap,VecDeque}, sync::{Arc,Weak},time::{Duration,Instant}};
use windows_sys::Win32::{Foundation::HWND,Graphics::Gdi::*};
use super::{control_center::{self,MetricOptions},stage_surface::{Box2,draw_mask,draw_mask_scaled}};
use crate::launcher::IconPixels;
use super::{FeedbackKind,UtilityWidget};

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

// Preserve the chosen glass opacity in open areas. Only the small surfaces
// behind monochrome controls need enough backing to survive any wallpaper.
pub(super) const CONTROL_OPACITY: u8 = 218;

/// Shared color tokens for native Dock, Stage and utility controls (BGR order).
#[derive(Clone,Copy)]
pub(crate) struct Palette {
    pub panel:[u8;3],pub ink:[u8;3],pub muted:[u8;3],pub accent:[u8;3],pub edge:[u8;3],
}
impl Palette {
    pub fn new(dark:bool)->Self{
        if dark{Self{panel:[51,39,31],ink:[252,248,241],muted:[216,198,182],accent:[255,198,124],edge:[149,125,107]}}
        else{Self{panel:[255,251,247],ink:[52,38,26],muted:[100,79,65],accent:[184,99,37],edge:[220,199,181]}}
    }
}
pub(super) fn control_backing(base: f32) -> f32 {
    // Source-over opacity needed to bring the existing material to the floor.
    let floor = CONTROL_OPACITY as f32;
    if base >= floor { 0.0 } else { (floor - base) * 255.0 / (255.0 - base) }
}

pub type Atlas = HashMap<String, Option<Arc<crate::launcher::IconPixels>>>;
pub struct Surface {
    layer:Layer,glass:Rect,material:Material,static_pixels:Vec<u8>,icons:IconRasters,metric:Option<MetricInk>,clock:Option<ClockInk>,
}
struct MetricInk{size:(i32,i32),fonts:[HFONT;3],mask:Layer,pixels:Vec<u8>,key:Option<(usize,MetricOptions,bool,bool)>,painted_at:Option<Instant>}
impl Drop for MetricInk{fn drop(&mut self){unsafe{for &font in &self.fonts{if !font.is_null(){DeleteObject(font);}}}}}
impl MetricInk{
    unsafe fn new(w:i32,h:i32,scale:f32)->Option<Self>{
        let mask=Layer::new(w,h)?;
        let font=|size:f32,weight|crate::native_text::font_pixels((size*scale).round().max(8.0) as i32,weight);
        Some(Self{size:(w,h),fonts:[font(10.0,540),font(12.0,630),font(11.0,550)],mask,pixels:vec![0;w as usize*h as usize*4],key:None,painted_at:None})
    }
    unsafe fn paint(&mut self,data:&mut[u8],w:i32,h:i32,x:i32,y:i32,scale:f32,cfg:MetricOptions,palette:Palette,dark:bool,vertical:bool)->bool{
        let (mw,mh)=self.size;
        if x<0||y<0||x+mw>w||y+mh>h{return false;}
        let key=(control_center::inline_revision(),cfg,dark,vertical);
        let restyled=self.key.is_none_or(|(_,old,theme,edge)|old!=cfg||theme!=dark||edge!=vertical);
        let due=self.painted_at.is_none_or(|at|at.elapsed()>=Duration::from_millis(cfg.interval*1000-100));
        if self.key!=Some(key)&&(restyled||due){
            let readings=control_center::inline_readings(cfg);
            self.pixels.fill(0);
            let px=|n:f32|(n*scale).round() as i32;
            let mut write=|index:usize,rect:Box2,text:&str,center:bool,ink:[u8;3]|{
                if rect.w>0&&rect.h>0{
                    draw_mask(&mut self.pixels,mw,mh,&mut self.mask,self.fonts[index],rect,text,center,ink);
                }
            };
            if vertical{
                let row=px(16.0).min(mh/readings.len().max(1) as i32);
                let top=(mh-row*readings.len() as i32)/2;
                for (i,r) in readings.iter().enumerate(){
                    let y=top+i as i32*row;
                    write(0,Box2{x:px(3.0),y,w:px(25.0),h:row},r.label,false,palette.muted);
                    write(1,Box2{x:px(29.0),y,w:mw-px(31.0),h:row},&r.value,false,palette.ink);
                }
            }else{
                let split=readings.iter().position(|r|r.network).unwrap_or(readings.len());
                let (system,network)=readings.split_at(split);
                let top=(mh-if system.is_empty(){px(18.0)}else if network.is_empty(){px(28.0)}else{px(44.0)})/2;
                for (i,r) in system.iter().enumerate(){
                    let x=i as i32*mw/system.len() as i32;
                    let width=(i as i32+1)*mw/system.len() as i32-x;
                    write(0,Box2{x,y:top,w:width,h:px(12.0)},r.label,true,palette.muted);
                    write(1,Box2{x,y:top+px(12.0),w:width,h:px(15.0)},&r.value,true,palette.ink);
                }
                for (i,r) in network.iter().enumerate(){
                    let x=i as i32*mw/network.len() as i32;
                    let width=(i as i32+1)*mw/network.len() as i32-x;
                    let y=top+if system.is_empty(){0}else{px(28.0)};
                    write(0,Box2{x:x+px(3.0),y,w:px(11.0),h:px(16.0)},r.label,true,palette.muted);
                    write(2,Box2{x:x+px(15.0),y,w:width-px(15.0),h:px(16.0)},&r.value,false,palette.ink);
                }
            }
            self.key=Some(key);
            self.painted_at=(!readings.is_empty()).then(Instant::now);
        }
        let mut drawn=false;
        for row in 0..mh as usize{
            let src=&self.pixels[row*mw as usize*4..][..mw as usize*4];
            let dest=&mut data[((y as usize+row)*w as usize+x as usize)*4..][..src.len()];
            for (src,dest) in src.chunks_exact(4).zip(dest.chunks_exact_mut(4)){
                if src[3]==0{continue;}
                drawn=true;
                let inv=255-src[3] as u16;
                for i in 0..4{dest[i]=(src[i] as u16+(dest[i] as u16*inv+127)/255).min(255) as u8;}
            }
        }
        drawn
    }
}
struct ClockInk{
    mask:Layer, pixels:Vec<u8>, fonts:[HFONT;2], size:(i32,i32), stamp:Option<(u16,u16,u16,u16,u16,bool,bool)>,
}
impl Drop for ClockInk{
    fn drop(&mut self){unsafe{for font in self.fonts{if !font.is_null(){DeleteObject(font);}}}}
}
impl ClockInk{
    unsafe fn new(w:i32,h:i32,scale:f32)->Option<Self>{
        let dpi=(scale*96.0).round() as u32;
        Some(Self{mask:Layer::new(w*2,h*2)?,pixels:vec![0;w as usize*h as usize*4],size:(w,h),
            fonts:[super::panel_chrome::choose_font(dpi,26,510),super::panel_chrome::choose_font(dpi,23,510)],stamp:None})
    }
    unsafe fn paint(&mut self,data:&mut[u8],w:i32,h:i32,x:i32,y:i32,scale:f32,p:Palette,dark:bool,vertical:bool){
        use windows_sys::Win32::{Foundation::SYSTEMTIME,System::SystemInformation::GetLocalTime};
        let (cw,ch)=self.size;
        if x<0||y<0||x+cw>w||y+ch>h{return;}
        let mut t:SYSTEMTIME=std::mem::zeroed();GetLocalTime(&mut t);
        let stamp=Some((t.wYear,t.wMonth,t.wDay,t.wHour,t.wMinute,dark,vertical));
        if self.stamp!=stamp{
            self.pixels.fill(0);
            let line=(16.0*scale).round() as i32;
            let top=(ch-2*line)/2;
            let time=format!("{:02}:{:02}",t.wHour,t.wMinute);
            let date=if vertical{format!("{:02}/{:02}",t.wDay,t.wMonth)}else{format!("{:02}/{:02}/{}",t.wDay,t.wMonth,t.wYear)};
            for (i,(value,ink)) in [(&time,p.ink),(&date,p.muted)].into_iter().enumerate(){
                let b=Box2{x:0,y:top+i as i32*line,w:cw,h:line};
                draw_mask_scaled(&mut self.pixels,cw,ch,&mut self.mask,self.fonts[i],b,value,true,ink,2);
            }
            self.stamp=stamp;
        }
        for row in 0..ch as usize{
            let src=&self.pixels[row*cw as usize*4..][..cw as usize*4];
            let dest=&mut data[((y as usize+row)*w as usize+x as usize)*4..][..src.len()];
            for (src,dest) in src.chunks_exact(4).zip(dest.chunks_exact_mut(4)){
                if src[3]==0{continue;}
                let inv=255-src[3] as u16;
                for i in 0..4{dest[i]=(src[i] as u16+(dest[i] as u16*inv+127)/255).min(255) as u8;}
            }
        }
    }
}
impl Surface {
    pub unsafe fn new(w:i32,h:i32,glass:Rect,material:Material)->Option<Self>{
        let layer=Layer::new(w,h)?;
        Some(Self{layer,glass,material,static_pixels:Vec::new(),icons:IconRasters::default(),metric:None,clock:None})
    }
    pub fn size(&self)->(i32,i32){self.layer.size}
    pub fn set_material(&mut self,value:Material){
        if self.material!=value{self.material=value;self.static_pixels.clear();}
    }
    pub unsafe fn rail(&mut self,hwnd:HWND,layout:&Layout)->bool{
        let (w,h)=self.layer.size;
        let data=self.layer.pixels();data.fill(0);
        paint_rail(data,w,h,layout,self.material.dark,1.0);
        self.layer.present(hwnd)
    }
    pub unsafe fn paint(&mut self,hwnd:HWND,layout:&Layout,items:&[super::Item],atlas:&Atlas,start_icon:super::StartIcon,active:Option<usize>,minimized:u16,attention:u16,
        hover:Option<usize>,drop_target:Option<usize>,openness:f32,feedback:Option<(usize,FeedbackKind,f32)>,metrics:Option<MetricOptions>)->bool{
        let (w,h)=self.layer.size;
        let scale=layout.scale;
        if self.glass!=layout.glass||self.static_pixels.is_empty() {
            self.glass=layout.glass;
            self.static_pixels.resize(w as usize*h as usize*4,0);
            render_glass(&mut self.static_pixels,w,h,layout.glass,self.material,scale);
        }
        let data=self.layer.pixels();
        data.copy_from_slice(&self.static_pixels);
        let palette=Palette::new(self.material.dark);
        let control_tint=palette.panel;
        let control_alpha=control_backing((100-self.material.transparency) as f32*2.55).ceil() as u8;
        for (i,(slot,item)) in layout.slots.iter().zip(items).enumerate(){
            let front=active==Some(i);
            let asleep=(minimized & (1<<i))!=0;
            let tint=if self.material.dark{[245,248,252]}else{[58,69,78]};
            if !matches!(atlas.get(&item.path),Some(Some(_)))&&control_alpha!=0 {
                let r=Rect{x:slot.x+slot.w*0.10,y:slot.y+slot.h*0.10,w:slot.w*0.80,h:slot.h*0.80};
                rounded(data,w,h,r,r.w*0.26,control_tint,control_alpha);
            }
            if front||hover==Some(i){
                rounded(data,w,h,*slot,slot.w*0.25,if front{palette.accent}else{palette.ink},if front{38}else{31});
            }
            if let Some(Some(icon))=atlas.get(&item.path){
                self.icons.paint(data,w,h,*slot,icon);
            }else{
                fallback_icon(data,w,h,*slot,tint);
            }
            let (x,y)=layout.indicators[i];
            if item.count>0 {
                let alert=(attention & (1<<i))!=0;
                let (a,b)=if front {(5.0,1.9)}else if alert{(3.5,2.0)}else{(1.9,1.9)};
                let (kx,ky)=if layout.side==super::DockSide::Bottom{(a*scale,b*scale)}else{(b*scale,a*scale)};
                let r=Rect{x:x-kx,y:y-ky,w:kx*2.0,h:ky*2.0};
                if alert{
                    rounded(data,w,h,r,kx.min(ky),if self.material.dark{[113,200,255]}else{[35,104,174]},255);
                }else if asleep {
                    draw(data,w,h,r,|x,y|{
                        let t=(distance(r,kx.min(ky),x,y)+0.5*scale).abs();
                        Some((tint,(0.8*scale-t).clamp(0.0,1.0)*200.0))
                    });
                }else{rounded(data,w,h,r,kx.min(ky),if front{palette.accent}else{tint},if front{255}else{185});}
            }
            if let Some((_,kind,t))=feedback.filter(|(index,_,_)|*index==i) {
                let strength=((1.0-t).max(0.0).powf(0.75)*if matches!(kind,FeedbackKind::Failed){220.0}else{160.0}) as f32;
                let ring=match kind {
                    FeedbackKind::Failed=>[135,150,255],
                    FeedbackKind::Minimize=>tint,
                    _=>if self.material.dark{[244,218,173]}else{[170,103,42]},
                };
                draw(data,w,h,*slot,|x,y|{
                    let d=distance(*slot,slot.w*0.26,x,y);
                    Some((ring,(0.5-d).clamp(0.0,1.0)*(d+2.0*scale).clamp(0.0,1.0)*strength))
                });
            }
        }
        let scan=layout.scan;
        let tint=palette.ink;
        if let Some(cluster)=layout.cluster {
            let radius=cluster.w.min(cluster.h).min(28.0*scale)*0.42;
            let backing=(control_backing((100-self.material.transparency) as f32*2.55)*0.45).min(86.0);
            rounded(data,w,h,cluster,radius,control_tint,backing.ceil() as u8);
        }else if control_alpha!=0{rounded(data,w,h,scan,scan.w*0.32,control_tint,control_alpha);}
        if hover==Some(items.len()){rounded(data,w,h,scan,scan.w*0.22,tint,50);}
        let center=(scan.x+0.45*scan.w,scan.y+0.44*scan.h);
        let radius=scan.w*0.19;
        draw(data,w,h,scan,|x,y|{
            let ring=((x-center.0).hypot(y-center.1)-radius).abs()-0.85*scale;
            let tail=segment_distance(x,y,center.0+radius*0.65,center.1+radius*0.65,scan.x+scan.w*0.73,scan.y+scan.h*0.73)-0.88*scale;
            Some((tint,(0.5-ring.min(tail)).clamp(0.0,1.0)*238.0))
        });
        let utility=layout.utility;
        if utility.w>0.0{rounded(data,w,h,utility,utility.w*0.31,palette.ink,if self.material.dark{24}else{15});}
        if hover==Some(items.len()+1){rounded(data,w,h,utility,utility.w*0.22,tint,50);}
        let divider=|a:Rect,b:Rect|{
            if layout.side==super::DockSide::Bottom{
                Rect{x:(a.x+a.w+b.x)*0.5-0.5*scale,y:layout.glass.y+layout.glass.h*0.22,w:scale,h:layout.glass.h*0.56}
            }else{
                Rect{x:layout.glass.x+layout.glass.w*0.22,y:(a.y+a.h+b.y)*0.5-0.5*scale,w:layout.glass.w*0.56,h:scale}
            }
        };
        if let Some(group)=layout.divider{rounded(data,w,h,group,scale*0.5,palette.edge,154);}
        if let Some(split)=items.windows(2).position(|pair|pair[0].pinned&&!pair[1].pinned){
            let group=divider(layout.slots[split],layout.slots[split+1]);
            rounded(data,w,h,group,scale*0.5,tint,58);
        }
        if layout.metrics.is_none(){self.metric=None;}
        for (i,(widget,slot)) in layout.inline.iter().copied().enumerate(){
            if hover==Some(items.len()+2+i){rounded(data,w,h,slot,7.0*scale,tint,50);}
            if let Some(cfg)=metrics.filter(|_|layout.metrics==Some(slot)&&widget==UtilityWidget::Metrics){
                let vertical=layout.side!=super::DockSide::Bottom;
                let line=if vertical{
                    Rect{x:slot.x+slot.w*0.12,y:slot.y-3.0*scale,w:slot.w*0.76,h:scale}
                }else{
                    Rect{x:slot.x-3.0*scale,y:slot.y+slot.h*0.15,w:scale,h:slot.h*0.70}
                };
                rounded(data,w,h,line,scale*0.5,palette.muted,82);
                let x=slot.x.ceil() as i32;let y=slot.y.ceil() as i32;
                let mw=(slot.x+slot.w).floor() as i32-x;
                let mh=(slot.y+slot.h).floor() as i32-y;
                if self.metric.as_ref().is_none_or(|s|s.size!=(mw,mh)){
                    self.metric=MetricInk::new(mw,mh,scale);
                }
                if self.metric.as_mut().is_some_and(|r|r.paint(data,w,h,x,y,scale,cfg,palette,self.material.dark,vertical)){continue;}
                let edge=21.0*scale;
                let badge=Rect{x:slot.x+(slot.w-edge)/2.0,y:slot.y+(slot.h-edge)/2.0,w:edge,h:edge};
                status_on(data,w,h,badge,scale,palette.muted,Some(widget));
                continue;
            }
            status_on(data,w,h,slot,scale,tint,Some(widget));
        }
        status_on(data,w,h,utility,scale,tint,None);
        let first=layout.shell_index();
        for (i,slot) in [layout.start,layout.tray,layout.clock].into_iter().enumerate(){
            if hover==Some(first+i){rounded(data,w,h,slot,8.0*scale,palette.ink,37);}
        }
        paint_start(data,w,h,layout.start,start_icon,palette,self.material.dark);
        let t=layout.tray;
        let mid_x=t.x+t.w*0.5;let mid_y=t.y+t.h*0.5;
        let k=(t.w.min(t.h)*0.18).min(5.0*scale);
        draw(data,w,h,t,|x,y|{
            let a=segment_distance(x,y,mid_x-k,mid_y+k*0.38,mid_x,mid_y-k*0.50);
            let b=segment_distance(x,y,mid_x,mid_y-k*0.50,mid_x+k,mid_y+k*0.38);
            Some((tint,(0.5+0.78*scale-a.min(b)).clamp(0.0,1.0)*238.0))
        });
        let c=layout.clock;
        let x=c.x.ceil() as i32;let y=c.y.ceil() as i32;
        let cw=(c.x+c.w).floor() as i32-x;let ch=(c.y+c.h).floor() as i32-y;
        if cw>=24&&ch>=22{
            let font_scale=scale.min(ch as f32/36.0).min(cw as f32/46.0);
            if self.clock.as_ref().is_none_or(|old|old.size!=(cw,ch)){self.clock=ClockInk::new(cw,ch,font_scale);}
            if let Some(clock)=self.clock.as_mut(){clock.paint(data,w,h,x,y,font_scale,palette,self.material.dark,layout.side!=super::DockSide::Bottom);}
        }
        if let Some(marker)=drop_target.and_then(|i|layout.pin_marker(i,items.iter().take_while(|item|item.pinned).count())){
            let ink=if self.material.dark{[244,219,180]}else{[171,106,44]};
            rounded(data,w,h,marker,scale,ink,242);
        }
        if openness<1.0 {
            let alpha=(openness.clamp(0.0,1.0)*255.0).round() as u16;
            for value in data.iter_mut(){*value=((*value as u16*alpha+127)/255) as u8;}
            paint_rail(data,w,h,layout,self.material.dark,1.0-openness);
        }
        self.layer.present(hwnd)
    }
}

fn paint_start(data:&mut[u8],w:i32,h:i32,slot:Rect,icon:super::StartIcon,palette:Palette,dark:bool){
    use super::StartIcon::*;
    let size=slot.w.min(slot.h)*0.91;
    let base=Rect{x:slot.x+(slot.w-size)*0.5,y:slot.y+(slot.h-size)*0.5,w:size,h:size};
    let (background,ink,rows)=match icon{
        Windows=>(Some([205,121,26]),[255,252,248],2),
        Grid=>(Some(if dark{[119,80,55]}else{[177,107,57]}),[255,248,238],3),
        Orb=>(Some(if dark{[78,108,159]}else{[87,114,164]}),[255,251,246],2),
        Minimal=>(None,palette.ink,2),
    };
    if let Some(color)=background{
        rounded(data,w,h,base,size*if icon==Orb{0.5}else{0.24},color,255);
    }
    let glyph=size*if background.is_some(){0.57}else{0.72};
    let pitch=glyph/rows as f32;
    let tile=pitch*if rows==3{0.68}else{0.88};
    let start_x=slot.x+(slot.w-(glyph-pitch+tile))*0.5;
    let start_y=slot.y+(slot.h-(glyph-pitch+tile))*0.5;
    for col in 0..rows{for row in 0..rows{
        let r=Rect{x:start_x+col as f32*pitch,y:start_y+row as f32*pitch,w:tile,h:tile};
        rounded(data,w,h,r,tile*if rows==3||icon==Orb{0.38}else{0.07},ink,255);
    }}
}

fn paint_rail(data:&mut[u8],w:i32,h:i32,layout:&Layout,dark:bool,strength:f32) {
    let scale=layout.scale;
    let area=layout.glass;
    let rail=match layout.side{
        super::DockSide::Bottom=>Rect{x:area.x+area.w*0.25,y:area.y+area.h-4.0*scale,w:area.w*0.50,h:4.0*scale},
        super::DockSide::Left=>Rect{x:area.x,y:area.y+area.h*0.25,w:4.0*scale,h:area.h*0.50},
        super::DockSide::Right=>Rect{x:area.x+area.w-4.0*scale,y:area.y+area.h*0.25,w:4.0*scale,h:area.h*0.50},
    };
    rounded(data,w,h,rail,rail.w.min(rail.h)*0.5,if dark{[241,246,255]}else{[26,34,48]},(126.0*strength.clamp(0.0,1.0)).round() as u8);
}

fn glass_pixel(tint:Material,rect:Rect,radius:f32,x:f32,y:f32,scale:f32,d:f32)->[u8;4]{
    if d>7.8*scale{return [0;4];}
    let coverage=(0.5-d).clamp(0.0,1.0);
    let shadow=if coverage<1.0 {
        let shadow_distance=distance(rect,radius,x,y-1.8*scale);
        (1.0-(shadow_distance/(6.0*scale)).max(0.0)).clamp(0.0,1.0).powi(2)*0.25*(1.0-coverage)
    }else{0.0};
    let v=((y-rect.y)/rect.h).clamp(0.0,1.0);
    let lip=(1.2+d/scale).clamp(0.0,1.0);
    let shine=(1.0-v).powi(5);
    let opacity=((100-tint.transparency) as f32*0.01 + 0.10*lip+0.09*shine).clamp(0.0,0.99);
    let alpha=coverage*opacity;
    let (base,reflection)=if tint.dark{([38.0,33.0,30.0],54.0)}else{([247.0,242.0,237.0],20.0)};
    let mut pixel=[0;4];
    for c in 0..3{
        let rim=if tint.dark{(1.0-0.5*v)*lip*48.0}else{(0.8-v)*lip*26.0};
        pixel[c]=((base[c]+shine*reflection+rim).clamp(0.0,255.0)*alpha).round() as u8;
    }
    pixel[3]=((alpha+shadow)*255.0).round() as u8;
    pixel
}

#[cfg(test)]
mod material_tests {
    use super::*;

    #[test]
    fn glass_controls_keep_contrast_on_extreme_backdrops() {
        fn luminance(rgb: [f32;3]) -> f32 {
            let [b,g,r] = rgb.map(|c| ((c / 255.0 + 0.055) / 1.055).powf(2.4));
            0.0722*b + 0.7152*g + 0.2126*r
        }
        for opacity in [26.0,64.0,90.0] {
            let base = opacity*2.55;
            let alpha = (base + control_backing(base)*(1.0-base/255.0))/255.0;
            for (material,ink,wall) in [([34.0,27.0,23.0],[238.0,244.0,252.0],255.0),
                ([242.0,236.0,231.0],[43.0,53.0,64.0],0.0)] {
                let background = luminance(material.map(|value| value*alpha + wall*(1.0-alpha)));
                let foreground = luminance(ink);
                assert!((foreground.max(background)+0.05)/(foreground.min(background)+0.05)>4.5);
            }
        }
        assert_eq!(control_backing(255.0),0.0);
    }

    #[test]
    fn scanline_glass_matches_the_original_distance_field(){
        let (w,h)=(262,162);
        let mat=Material{dark:true,transparency:35,reduced_motion:false};
        for rect in [Rect{x:8.35,y:19.8,w:180.1,h:48.3},Rect{x:18.1,y:10.3,w:44.4,h:143.9}]{
            let mut data=vec![0;w as usize*h as usize*4];
            render_glass(&mut data,w,h,rect,mat,1.0);
            let radius=rect.w.min(rect.h)*0.4;
            for (i,actual) in data.chunks_exact(4).enumerate(){
                let (x,y)=((i%w as usize) as f32+0.5,(i/w as usize) as f32+0.5);
                let expected=glass_pixel(mat,rect,radius,x,y,1.0,distance(rect,radius,x,y));
                assert!(actual.iter().zip(expected).all(|(&a,b)|a.abs_diff(b)<=1), "at ({x}, {y}) {actual:?} != {expected:?}");
            }
        }
    }
    #[test]
    fn capsule_has_clear_corners_a_soft_shadow_and_premultiplied_edges_at_both_dpis() {
        for scale in [1.0,2.0] {
            let r=Rect{x:8.0*scale,y:8.0*scale,w:150.0*scale,h:58.0*scale};
            let radius=r.h/2.0;
            for dark in [false,true] {
                let paint=|x,y,t|glass_pixel(Material{dark,transparency:t,reduced_motion:false},r,radius,x,y,scale,distance(r,radius,x,y));
                assert_eq!(paint(r.x,r.y,35),[0;4]);
                let (x,y)=(r.x+r.w/2.0,r.y+r.h/2.0);
                assert!(paint(x,y,18)[3]>paint(x,y,64)[3]);
                let shadow=paint(x,r.y+r.h+3.0*scale,35);
                assert!(shadow[3]>0 && shadow[3]<64);
                for i in 0..32 {
                    let p=paint(x,r.y+(i as f32/4.0-4.0)*scale,35);
                    assert!(p[..3].iter().all(|&component|component<=p[3]));
                }
            }
        }
    }
}

fn segment_distance(x:f32,y:f32,ax:f32,ay:f32,bx:f32,by:f32)->f32{
    let (dx,dy)=(bx-ax,by-ay);
    let t=(((x-ax)*dx+(y-ay)*dy)/(dx*dx+dy*dy).max(0.001)).clamp(0.0,1.0);
    (x-ax-dx*t).hypot(y-ay-dy*t)
}
// Only the shadow bounds need drawing. Along a horizontal capsule's
// straight middle, glass color, rim and shadow depend on y alone.
fn render_glass(data:&mut[u8],w:i32,h:i32,rect:Rect,tint:Material,scale:f32){
    data.fill(0);
    let radius=rect.w.min(rect.h)*0.40;
    let margin=9.0*scale;
    let start=((rect.x-margin).floor() as i32).clamp(0,w);
    let end=((rect.x+rect.w+margin).ceil() as i32).clamp(start,w);
    let left=((rect.x+rect.h*0.5).ceil() as i32).clamp(start,end);
    let right=((rect.x+rect.w-rect.h*0.5).floor() as i32).clamp(left,end);
    let top=((rect.y-margin).floor() as i32).clamp(0,h);
    let bottom=((rect.y+rect.h+margin).ceil() as i32).clamp(top,h);
    for y in top..bottom{
        let yy=y as f32+0.5;
        let at=|x|glass_pixel(tint,rect,radius,x,yy,scale,distance(rect,radius,x,yy));
        let row=&mut data[(y*w*4) as usize..((y+1)*w*4) as usize];
        let center=if left<right{at(rect.x+rect.w*0.5)}else{[0;4]};
        for x in start..left{row[(x*4) as usize..(x*4+4) as usize].copy_from_slice(&at(x as f32+0.5));}
        for pixel in row[(left*4) as usize..(right*4) as usize].chunks_exact_mut(4){pixel.copy_from_slice(&center);}
        for x in right..end{row[(x*4) as usize..(x*4+4) as usize].copy_from_slice(&at(x as f32+0.5));}
    }
}
pub(super) fn distance(rect:Rect,r:f32,x:f32,y:f32)->f32{
    let qx=(x-rect.x-rect.w*0.5).abs()-(rect.w*0.5-r);
    let qy=(y-rect.y-rect.h*0.5).abs()-(rect.h*0.5-r);
    if qx>0.0&&qy>0.0 {qx.hypot(qy)-r}else{qx.max(qy)-r}
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
pub(super) fn status_on(data:&mut[u8],w:i32,h:i32,cell:Rect,scale:f32,tint:[u8;3],widget:Option<UtilityWidget>){
    use UtilityWidget::*;
    if cell.w<=0.0||cell.h<=0.0{return;}
    let edge=(20.0*scale).min(cell.w.min(cell.h));
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
    use super::utilities::{NetworkGlyph,VolumeGlyph};
    let (network,volume)=super::utilities::status_glyphs();
    let stroke=(0.78*scale).max(0.66);
    let cover=|d:f32|(0.5+stroke-edge*d).clamp(0.0,1.0);
    draw(data,w,h,ink,|x,y|{
        let (x,y)=((x-px)/edge,(y-py)/edge);
        let segments=|parts:&[(f32,f32,f32,f32)]|parts.iter().map(|&(ax,ay,bx,by)|line(x,y,ax,ay,bx,by))
            .fold(f32::INFINITY,f32::min);
        let d=match widget {
            None=>[0.22_f32,0.50,0.78].into_iter().map(|cx|(x-cx).hypot(y-0.50)-0.045).fold(f32::INFINITY,f32::min),
            Some(Network)=>{
                if network==NetworkGlyph::Wired{
                    let d=segments(&[(0.25,0.24,0.75,0.24),(0.25,0.24,0.25,0.68),(0.75,0.24,0.75,0.68),
                        (0.25,0.68,0.40,0.68),(0.60,0.68,0.75,0.68),(0.40,0.68,0.40,0.80),
                        (0.60,0.68,0.60,0.80),(0.40,0.80,0.60,0.80),(0.42,0.24,0.42,0.40),(0.58,0.24,0.58,0.40)]);
                    return Some((tint,cover(d)*247.0));
                }
                let bands=[(x-0.50).hypot(y-0.80)-0.033,
                    arc(x,y,0.50,0.78,0.20,0.0,-1.0),arc(x,y,0.50,0.78,0.38,0.0,-1.0),
                    arc(x,y,0.50,0.78,0.56,0.0,-1.0)];
                let (count,mark)=match network{
                    NetworkGlyph::Wifi(n)=>(n,f32::INFINITY),
                    NetworkGlyph::Disabled=>(0,line(x,y,0.18,0.18,0.82,0.84)),
                    NetworkGlyph::Disconnected=>(0,segments(&[(0.65,0.64,0.86,0.85),(0.86,0.64,0.65,0.85)])),
                    _=>(0,f32::INFINITY),
                };
                let a=bands.into_iter().enumerate().fold(cover(mark),|a,(i,d)|
                    a.max(cover(d)*if i<usize::from(count){1.0}else{0.23}));
                return Some((tint,a*247.0));
            }
            Some(Sound)=>{
                let horn=segments(&[(0.09,0.39,0.21,0.39),(0.09,0.39,0.09,0.61),(0.09,0.61,0.21,0.61),
                    (0.21,0.39,0.40,0.23),(0.21,0.61,0.40,0.77),(0.40,0.23,0.40,0.77)]);
                match volume{
                    VolumeGlyph::Muted=>horn.min(segments(&[(0.56,0.37,0.83,0.63),(0.83,0.37,0.56,0.63)])),
                    VolumeGlyph::Unknown=>horn.min(line(x,y,0.57,0.50,0.77,0.50)),
                    VolumeGlyph::Waves(n)=>[0.16,0.32,0.49].into_iter().take(usize::from(n))
                        .fold(horn,|d,r|d.min(arc(x,y,0.42,0.50,r,1.0,0.0))),
                }
            }
            Some(widget)=>widget.glyph().iter().map(|&(ax,ay,bx,by)|{
                // Expand each control equally about its optical center.
                let pos=|p:f32|0.5+(p-0.5)*1.17;
                line(x,y,pos(ax),pos(ay),pos(bx),pos(by))
            }).fold(f32::INFINITY,f32::min),
        };
        Some((tint,cover(d)*247.0))
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
