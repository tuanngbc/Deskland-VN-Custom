//! DPI-aware dock geometry, independent of Win32 painting and input.
use super::{Appearance, Display, DockSide, UtilityWidget};
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

/// Preserve a normal window's logical size and relative position within its
/// source work area. Target work area already excludes Windows and AppBars.
pub(super) fn transfer_window(window:RECT,source:&Display,target:&Display)->Option<RECT>{
    let relocate=|start:i32,end:i32,from_start:i32,from_end:i32,to_start:i32,to_end:i32|{
        let size=end as f64-start as f64;
        let from=from_end as f64-from_start as f64;
        let to=to_end as f64-to_start as f64;
        if size<=0.0||from<=0.0||to<=0.0{return None;}
        let relative=((start as f64-from_start as f64)/(from-size).max(1.0)).clamp(0.0,1.0);
        let size=(size*target.dpi.clamp(72,288) as f64/source.dpi.clamp(72,288) as f64).round().clamp(1.0,to);
        let position=to_start as f64+((to-size)*relative).round();
        Some((position as i32,(position+size) as i32))
    };
    let (left,right)=relocate(window.left,window.right,source.work.left,source.work.right,target.work.left,target.work.right)?;
    let (top,bottom)=relocate(window.top,window.bottom,source.work.top,source.work.bottom,target.work.top,target.work.bottom)?;
    Some(RECT{left,top,right,bottom})
}

#[derive(Clone)]
pub struct Layout {
    pub window: RECT,
    pub glass: Rect,
    pub slots: Vec<Rect>,
    pub scan: Rect,
    pub utility: Rect,
    pub utility_leading:bool,
    pub inline:Vec<(UtilityWidget,Rect)>,
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
            else{self.inline.iter().position(|(_,r)|r.contains(x,y)).map(|i|self.slots.len()+2+i)}
        })
    }
    pub fn inline_at(&self,x:f32,y:f32)->Option<UtilityWidget>{
        self.inline.iter().find(|(_,r)|r.contains(x,y)).map(|(w,_)|*w)
    }
    pub fn is_utility_button(&self,x:f32,y:f32)->bool{self.utility.contains(x,y)}
    pub fn is_launch_button(&self,x:f32,y:f32)->bool { self.scan.contains(x,y) }
    pub fn axis(&self,x:f32,y:f32)->f32 {if self.side==DockSide::Bottom{x}else{y}}
    /// Insertion boundaries follow the drawn slots, including gaps and ends.
    pub fn pin_boundary(&self,x:f32,y:f32,pins:usize)->Option<usize>{
        let pins=self.slots.get(..pins)?;
        let (first,last)=(pins.first()?,pins.last()?);
        let horizontal=self.side==DockSide::Bottom;
        let start=|r:&Rect| if horizontal {r.x}else{r.y};
        let span=|r:&Rect| if horizontal {r.w}else{r.h};
        let p=self.axis(x,y);
        let cross=if horizontal{y}else{x};
        let margin=10.0*self.scale;
        let low=pins.iter().map(|r|if horizontal{r.y}else{r.x}).fold(f32::INFINITY,f32::min);
        let high=pins.iter().map(|r|if horizontal{r.y+r.h}else{r.x+r.w}).fold(0.0,f32::max);
        if cross<low-margin||cross>high+margin||p<start(first)-margin||p>start(last)+span(last)+margin{return None;}
        Some(pins.iter().position(|r|p<start(r)+span(r)*0.5).unwrap_or(pins.len()))
    }
    pub fn pin_marker(&self,boundary:usize,pins:usize)->Option<Rect>{
        let slots=self.slots.get(..pins)?;
        let horizontal=self.side==DockSide::Bottom;
        let start=|r:&Rect|if horizontal{r.x}else{r.y};
        let end=|r:&Rect|if horizontal{r.x+r.w}else{r.y+r.h};
        let gap=2.0*self.scale;
        let axis=match (boundary.checked_sub(1).and_then(|i|slots.get(i)),slots.get(boundary)){
            (Some(left),Some(right))=>(end(left)+start(right))*0.5,
            (None,Some(right))=>start(right)-gap,
            (Some(left),None) if boundary==pins=>end(left)+gap,
            _=>return None,
        };
        let b=self.glass;
        Some(if horizontal { Rect{x:axis-self.scale,y:b.y+b.h*0.27,w:2.0*self.scale,h:b.h*0.46} }
            else { Rect{x:b.x+b.w*0.27,y:axis-self.scale,w:b.w*0.46,h:2.0*self.scale} })
    }
    /// Use visual order without changing the click/paint indices.
    pub fn key_order(&self)->Vec<usize>{
        let n=self.slots.len();
        let mut indexes=Vec::with_capacity(n+self.inline.len()+2);
        let append_status=|out:&mut Vec<usize>| {
            if self.utility_leading{out.extend(n+2..n+2+self.inline.len());}
            if self.utility.w>0.0{out.push(n+1);}
            if !self.utility_leading{out.extend(n+2..n+2+self.inline.len());}
        };
        if self.utility_leading{append_status(&mut indexes);}
        indexes.extend(0..=n);
        if !self.utility_leading{append_status(&mut indexes);}
        indexes
    }
}

#[cfg(test)]
pub fn measure(display:&Display,style:&Appearance,count:usize,focus:Option<f32>,amount:f32)->Layout{
    measure_utilities(display,style,count,focus,amount,Some(false))
}
#[cfg(test)]
pub fn measure_utilities(display:&Display,style:&Appearance,count:usize,focus:Option<f32>,amount:f32,utility_position:Option<bool>)->Layout{
    measure_widgets(display,style,count,focus,amount,utility_position,&[],28)
}
pub fn measure_widgets(display:&Display, style:&Appearance, count:usize, focus:Option<f32>, amount:f32, utility_position:Option<bool>, pinned:&[UtilityWidget],utility_cell:u32)->Layout {
    let oriented=style.position!=DockSide::Bottom;
    let scale=(display.dpi as f32/96.0).clamp(0.75,3.0);
    let edge=8.0*scale;
    let shadow=6.0*scale;
    let pad=12.0*scale;
    let max_span=if oriented {display.work.bottom-display.work.top} else {display.work.right-display.work.left};
    let max_span=max_span as f32-2.0*(edge+shadow);
    let utility_leading=utility_position==Some(true);
    let host=usize::from(utility_position.is_some());
    let app_cells=count+1; // The launcher uses an app-sized cell.
    let status_gap=2.0*scale;
    let group_gap=6.0*scale;
    let ideal_icon=style.icon_size as f32*scale;
    let min_icon=ideal_icon.min(28.0*scale);
    let ideal_status=utility_cell.clamp(24,32) as f32*scale;
    let zoom_range=if style.reduced_motion {0.0}else{style.magnification as f32/100.0};
    let extra=2.6*zoom_range;
    let app_room=(max_span-2.0*pad).max(0.0);
    let spacing=(style.spacing as f32*scale)
        .min(((app_room-app_cells as f32*min_icon)/app_cells as f32).max(scale));
    let app_gaps=count as f32*spacing;
    let status_total=|n:usize,cell:f32| if n==0{0.0}else{group_gap+n as f32*cell+(n-1) as f32*status_gap};
    let inline_count=if host==1 {
        (0..=pinned.len().min(5)).rev().find(|&n| {
            status_total(n+1,ideal_status)+app_gaps+min_icon*(app_cells as f32+extra)<=app_room
        }).unwrap_or(0)
    }else{0};
    let status_count=host+inline_count;
    let status=ideal_status.min((app_room-app_gaps-app_cells as f32*8.0).max(8.0));
    let cells=app_cells+status_count;
    let lead_count=if utility_leading{status_count}else{0};
    let icon=ideal_icon.min(((app_room-status_total(status_count,status)-app_gaps)/(app_cells as f32+extra)).max(1.0));
    let gap=|i:usize| {
        if i+1==cells{return 0.0;}
        let app=|k:usize|k>=lead_count&&k<lead_count+app_cells;
        match (app(i),app(i+1)){(true,true)=>spacing,(false,false)=>status_gap,_=>group_gap}
    };
    let is_app=|i:usize| i>=lead_count&&i<lead_count+app_cells;
    let base_span=icon*app_cells as f32+app_gaps+status_total(status_count,status)+2.0*pad;
    let cross=icon*(1.0+zoom_range)+20.0*scale;
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
    let mut unscaled=(extent-base_span)*0.5+pad;
    let widths:Vec<f32>=(0..cells).map(|i|{
        let size=if is_app(i){icon}else{status};
        let center=unscaled+size*0.5;
        unscaled+=size+gap(i);
        if !is_app(i){return size;}
        let delta=focus.map(|p|(center-p)/(icon+spacing)).unwrap_or(10.0);
        let weight=(-delta*delta/1.60).exp();
        icon*(1.0+amount.clamp(0.0,1.0)*zoom_range*weight)
    }).collect();
    let span=widths.iter().sum::<f32>()+(0..cells).map(gap).sum::<f32>()+2.0*pad;
    let start=(extent-span)*0.5;
    let mut cursor=start+pad;
    let mut all=Vec::with_capacity(cells);
    let mut indicators=Vec::with_capacity(count);
    for (i,size) in widths.into_iter().enumerate(){
        let cross_axis=if is_app(i){match style.position{
            DockSide::Bottom|DockSide::Right=>total_cross-shadow-size-13.0*scale,
            DockSide::Left=>shadow+13.0*scale,
        }}else{match style.position {
            DockSide::Bottom|DockSide::Right=>total_cross-shadow-(icon+20.0*scale+size)*0.5,
            DockSide::Left=>shadow+(icon+20.0*scale-size)*0.5,
        }};
        let r=if oriented{Rect{x:cross_axis,y:cursor,w:size,h:size}}else{Rect{x:cursor,y:cross_axis,w:size,h:size}};
        let item_index=i.checked_sub(lead_count);
        if item_index.is_some_and(|index|index<count){
            let center=cursor+size*0.5;
            indicators.push(match style.position{
                DockSide::Bottom=>(center,total_cross-shadow-6.0*scale),
                DockSide::Right=>(total_cross-shadow-6.0*scale,center),
                DockSide::Left=>(shadow+6.0*scale,center),
            });
        }
        all.push(r);
        cursor+=size+gap(i);
    }
    let inline_rects=if utility_leading{all.drain(..inline_count).collect::<Vec<_>>()}
        else{all.split_off(cells-inline_count)};
    let inline=pinned.iter().copied().zip(inline_rects).collect();
    let utility=match utility_position{Some(true)=>all.remove(0),Some(false)=>all.pop().unwrap_or_default(),None=>Rect::default()};
    let scan=all.pop().unwrap_or_default();
    // The background follows the app row; empty hover slack remains alpha 0.
    let height=icon+20.0*scale;
    let glass=match style.position{
        DockSide::Bottom=>Rect{x:start,y:total_cross-shadow-height,w:span,h:height},
        DockSide::Right=>Rect{x:total_cross-shadow-height,y:start,w:height,h:span},
        DockSide::Left=>Rect{x:shadow,y:start,w:height,h:span},
    };
    Layout{window:RECT{left:x,top:y,right:x+w as i32,bottom:y+h as i32},glass,slots:all,scan,utility,utility_leading,inline,indicators,scale,side:style.position}
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn insertion_targets_gaps_and_row_ends_at_rest_and_during_zoom(){
        for side in [DockSide::Bottom,DockSide::Left,DockSide::Right]{
            for amount in [0.0,0.4,1.0]{
                let screen=Display{id:"test".into(),name:String::new(),bounds:RECT{left:-900,top:0,right:0,bottom:1200},
                    work:RECT{left:-900,top:0,right:0,bottom:1160},primary:false,dpi:144};
                let style=Appearance{position:side,..Default::default()};
                let layout=measure_widgets(&screen,&style,5,Some(160.0),amount,Some(false),&[],28);
                for edge in 0..=3{
                    let marker=layout.pin_marker(edge,3).unwrap();
                    assert_eq!(layout.pin_boundary(marker.x+marker.w/2.0,marker.y+marker.h/2.0,3),Some(edge));
                }
                assert_eq!(layout.pin_boundary(-100.0,-100.0,3),None);
            }
        }
    }
    fn overlaps(a:Rect,b:Rect)->bool{a.x<b.x+b.w&&a.x+a.w>b.x&&a.y<b.y+b.h&&a.y+a.h>b.y}
    #[test]
    fn window_transfer_respects_mixed_dpi_and_negative_work_area(){
        let display=|work,dpi|Display{id:String::new(),name:String::new(),bounds:work,work,primary:false,dpi};
        let from=display(RECT{left:0,top:0,right:1920,bottom:1000},96);
        let to=display(RECT{left:-1100,top:-640,right:-30,bottom:1220},144);
        let normal=transfer_window(RECT{left:600,top:300,right:1320,bottom:700},&from,&to).unwrap();
        assert_eq!((normal.left,normal.top,normal.right,normal.bottom),(-1100,-10,-30,590));
        let late=transfer_window(RECT{left:1680,top:870,right:2180,bottom:1100},&from,&to).unwrap();
        assert_eq!((late.right,late.bottom),(-30,1220));
        assert_eq!((late.right-late.left,late.bottom-late.top),(750,345));
    }
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
    #[test]
    fn overflow_preserves_apps_and_launcher_when_inline_widgets_do_not_fit(){
        for (width,dpi,expected) in [(1080,96,5),(440,192,0)]{
            let work=RECT{left:-width,top:0,right:0,bottom:1040};
            let d=Display{id:"display".into(),name:"Display".into(),bounds:work,work,primary:true,dpi};
            let pins=&UtilityWidget::ALL[..5];
            let l=measure_widgets(&d,&Appearance::default(),10,None,0.0,Some(false),pins,28);
            assert_eq!(l.slots.len(),10);
            assert_eq!(l.inline.len(),expected);
            assert!(l.window.left>=work.left&&l.window.right<=work.right);
            assert!(l.utility.w>0.0&&l.scan.w>0.0);
            if expected>0{
                assert_eq!(l.utility.w,28.0);
                assert!(l.inline.iter().all(|(_,r)|r.w==28.0));
                assert_eq!(l.slots[0].w,44.0);
                let lead=measure_widgets(&d,&Appearance::default(),10,None,0.0,Some(true),pins,28);
                assert_eq!(&lead.key_order()[..6],&[12,13,14,15,16,11]);
                assert_eq!(l.key_order(),(0..17).collect::<Vec<_>>());
                for (i,(_,r)) in lead.inline.iter().enumerate(){
                    assert_eq!(lead.hot(r.x+r.w*0.5,r.y+r.h*0.5),Some(i+12));
                }
                let zoom=measure_widgets(&d,&Appearance::default(),10,Some(l.slots[3].x),1.0,Some(false),pins,28);
                assert_eq!(zoom.inline[0].1.w,28.0);
                assert!(zoom.slots[3].w>44.0);
            }
        }
    }
}
