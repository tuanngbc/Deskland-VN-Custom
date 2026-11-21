//! DPI-aware dock geometry, independent of Win32 painting and input.
use super::{Appearance, Display, DockSide};
use windows_sys::Win32::Foundation::RECT;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32, pub y: f32, pub w: f32, pub h: f32,
}
impl Rect {
    pub fn contains(self,x:f32,y:f32)->bool {
        x>=self.x && x<self.x+self.w && y>=self.y && y<self.y+self.h
    }
}

#[derive(Clone)]
pub struct Layout {
    pub window: RECT,
    pub glass: Rect,
    pub slots: Vec<Rect>,
    pub scan: Rect,
    pub utility: Rect,
    pub utility_leading:bool,
    pub indicators: Vec<(f32,f32)>,
    pub scale:f32,
    pub side:DockSide,
}
impl Layout {
    pub fn hit(&self,x:f32,y:f32)->Option<usize>{
        self.slots.iter().position(|r|r.contains(x,y))
    }
    pub fn hot(&self,x:f32,y:f32)->Option<usize>{
        self.hit(x,y).or_else(||{
            if self.is_launch_button(x,y){Some(self.slots.len())}
            else if self.is_utility_button(x,y){Some(self.slots.len()+1)}
            else{None}
        })
    }
    pub fn is_utility_button(&self,x:f32,y:f32)->bool{self.utility.contains(x,y)}
    pub fn is_launch_button(&self,x:f32,y:f32)->bool { self.scan.contains(x,y) }
    pub fn axis(&self,x:f32,y:f32)->f32 {if self.side==DockSide::Bottom{x}else{y}}
}

#[cfg(test)]
pub fn measure(display:&Display,style:&Appearance,count:usize,focus:Option<f32>,amount:f32)->Layout{
    measure_utilities(display,style,count,focus,amount,Some(false))
}
pub fn measure_utilities(display:&Display, style:&Appearance, count:usize, focus:Option<f32>, amount:f32, utility_position:Option<bool>)->Layout {
    let oriented=style.position!=DockSide::Bottom;
    let scale=(display.dpi as f32/96.0).clamp(0.75,3.0);
    let spacing=style.spacing as f32*scale;
    let edge=8.0*scale;
    let shadow=6.0*scale;
    let pad=12.0*scale;
    let margins=2.0*(edge+shadow);
    let max_span=if oriented {display.work.bottom-display.work.top} else {display.work.right-display.work.left};
    // A permanent search affordance opens the existing Deskland launcher.
    let cells=count+1+usize::from(utility_position.is_some());
    let utility_leading=utility_position==Some(true);
    let max_span=max_span as f32-margins;
    let available=(max_span-2.0*pad-(cells-1) as f32*spacing).max(cells as f32*8.0);
    // Reserve enough room for the largest neighborhood before drawing the
    // first hover frame. Hit testing uses the actual rectangles below.
    let zoom_range=if style.reduced_motion {0.0}else{style.magnification as f32/100.0};
    let magnify_reserve=(1.0+zoom_range*2.6/cells as f32).max(1.0);
    let icon=(style.icon_size as f32*scale).min(available/cells as f32/magnify_reserve).max(8.0);
    let cross=icon*(1.0+zoom_range)+20.0*scale;
    let base_span=icon*cells as f32+(cells-1) as f32*spacing+2.0*pad;
    let reserved=(base_span+(icon*zoom_range*2.6).min(max_span-base_span).max(0.0)).ceil();
    let extent=reserved+2.0*shadow;
    let total_cross=(cross+2.0*shadow).ceil();
    let available_x=display.work.right-display.work.left;
    let available_y=display.work.bottom-display.work.top;
    let (x,y,w,h)=match style.position{
        DockSide::Bottom=>(display.work.left+(available_x-extent as i32)/2,
            display.work.bottom-(edge+total_cross) as i32,extent,total_cross),
        DockSide::Left=>(display.work.left+edge as i32,display.work.top+(available_y-extent as i32)/2,
            total_cross,extent),
        DockSide::Right=>(display.work.right-(edge+total_cross) as i32,display.work.top+(available_y-extent as i32)/2,
            total_cross,extent),
    };
    let unscaled_start=(extent-base_span)*0.5+pad;
    let widths:Vec<f32>=(0..cells).map(|i|{
        let center=unscaled_start+(icon+spacing)*i as f32+icon*0.5;
        let delta=focus.map(|p|(center-p)/(icon+spacing)).unwrap_or(10.0);
        let weight=(-delta*delta/1.60).exp();
        icon*(1.0+amount.clamp(0.0,1.0)*zoom_range*weight)
    }).collect();
    let span=widths.iter().sum::<f32>()+(cells-1) as f32*spacing+2.0*pad;
    let start=(extent-span)*0.5;
    let mut cursor=start+pad;
    let mut all=Vec::with_capacity(cells);
    let mut indicators=Vec::with_capacity(count);
    for (i,size) in widths.into_iter().enumerate(){
        let cross_axis=match style.position{
            DockSide::Bottom|DockSide::Right=>total_cross-shadow-size-13.0*scale,
            DockSide::Left=>shadow+13.0*scale,
        };
        let r=if oriented{Rect{x:cross_axis,y:cursor,w:size,h:size}}else{Rect{x:cursor,y:cross_axis,w:size,h:size}};
        let item_index=i.checked_sub(usize::from(utility_leading));
        if item_index.is_some_and(|index|index<count){
            let center=cursor+size*0.5;
            indicators.push(match style.position{
                DockSide::Bottom=>(center,total_cross-shadow-6.0*scale),
                DockSide::Right=>(total_cross-shadow-6.0*scale,center),
                DockSide::Left=>(shadow+6.0*scale,center),
            });
        }
        all.push(r);
        cursor+=size+spacing;
    }
    let utility=match utility_position{Some(true)=>all.remove(0),Some(false)=>all.pop().unwrap_or_default(),None=>Rect::default()};
    let scan=all.pop().unwrap_or_default();
    // The background follows the app row; empty hover slack remains alpha 0.
    let height=icon+20.0*scale;
    let glass=match style.position{
        DockSide::Bottom=>Rect{x:start,y:total_cross-shadow-height,w:span,h:height},
        DockSide::Right=>Rect{x:total_cross-shadow-height,y:start,w:height,h:span},
        DockSide::Left=>Rect{x:shadow,y:start,w:height,h:span},
    };
    Layout{window:RECT{left:x,top:y,right:x+w as i32,bottom:y+h as i32},glass,slots:all,scan,utility,utility_leading,indicators,scale,side:style.position}
}

#[cfg(test)]
mod tests{
    use super::*;
    fn overlaps(a:Rect,b:Rect)->bool{a.x<b.x+b.w&&a.x+a.w>b.x&&a.y<b.y+b.h&&a.y+a.h>b.y}
    #[test]
    fn all_positions_remain_inside_negative_coordinate_work_area() {
        for dpi in [96,120,144,192] {
            let work=RECT{left:-1080,top:-731,right:0,bottom:1140};
            let display=Display{id:"portrait".into(),name:"Portrait".into(),bounds:work,work,primary:false,dpi};
            for position in [DockSide::Bottom,DockSide::Left,DockSide::Right] {
                let style=Appearance{position,icon_size:60,magnification:55,..Default::default()};
                let layout=measure(&display,&style,10,Some(320.0),1.0);
                assert!(layout.window.left>=work.left&&layout.window.right<=work.right&&layout.window.top>=work.top&&layout.window.bottom<=work.bottom);
                let (width,height)=((layout.window.right-layout.window.left) as f32,(layout.window.bottom-layout.window.top) as f32);
                for (i,a) in layout.slots.iter().enumerate(){
                    assert!(a.x>=0.0&&a.y>=0.0&&a.x+a.w<=width&&a.y+a.h<=height);
                    assert_eq!(layout.hit(a.x+a.w*0.5,a.y+a.h*0.5),Some(i));
                    assert!(!overlaps(*a,layout.scan));
                    for b in &layout.slots[i+1..]{assert!(!overlaps(*a,*b));}
                }
            }
        }
    }
    #[test]
    fn neighbor_magnification_decays_without_changing_window_or_choosing_a_stale_slot(){
        let r=RECT{left:0,top:0,right:2560,bottom:1032};
        let display=Display{id:"main".into(),name:"Main".into(),bounds:r,work:r,primary:true,dpi:96};
        let style=Appearance::default();
        let idle=measure(&display,&style,7,None,0.0);
        let hovered=idle.slots[3];
        let focus=hovered.x+hovered.w/2.0;
        let grown=measure(&display,&style,7,Some(focus),1.0);
        assert_eq!(idle.window.left,grown.window.left);assert_eq!(idle.window.right,grown.window.right);
        assert!(grown.slots[3].w>grown.slots[2].w&&grown.slots[2].w>grown.slots[1].w);
        assert_eq!(grown.hit(focus,grown.slots[3].y+10.0),Some(3));
        let relaxed=measure(&display,&Appearance{reduced_motion:true,..style},7,Some(focus),1.0);
        assert!(relaxed.slots.iter().all(|s|(s.w-relaxed.slots[0].w).abs()<0.001));
    }
    #[test]
    fn utility_button_moves_or_disappears_without_changing_app_indices(){
        let work=RECT{left:-1080,top:-731,right:0,bottom:1189};
        let d=Display{id:"portrait".into(),name:"Portrait".into(),bounds:work,work,primary:false,dpi:192};
        let style=Appearance::default();
        for side in [Some(true),Some(false),None]{
            let l=measure_utilities(&d,&style,10,None,0.0,side);
            assert!(l.window.left>=work.left&&l.window.right<=work.right);
            assert_eq!(l.hot(l.slots[0].x+1.0,l.slots[0].y+1.0),Some(0));
            assert_eq!(l.hot(l.scan.x+1.0,l.scan.y+1.0),Some(10));
            match side {
                Some(true)=>assert!(l.utility.x+l.utility.w<=l.slots[0].x),
                Some(false)=>assert!(l.utility.x>=l.scan.x+l.scan.w),
                None=>assert_eq!(l.utility.w,0.0),
            }
            if side.is_some(){assert_eq!(l.hot(l.utility.x+1.0,l.utility.y+1.0),Some(11));}
        }
    }
}
