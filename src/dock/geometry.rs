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
    pub cluster:Option<Rect>,
    pub divider:Option<Rect>,
    pub metrics:Option<Rect>,
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
            if self.cluster.is_some(){out.push(n);}
            out.extend(n+2..n+2+self.inline.len());
            if self.utility.w>0.0{out.push(n+1);}
        };
        if self.utility_leading{append_status(&mut indexes);}
        indexes.extend(0..n+usize::from(self.cluster.is_none()));
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
#[derive(Clone,Copy)]
pub(super) struct Cluster {
    pub side:Option<bool>,
    pub cell:u32,
    pub gap:u32,
    pub two_rows:bool,
    pub inline_metrics:bool,
}
impl Cluster {
    pub fn cross(self)->f32{
        if self.side.is_none(){0.0}
        else{
            let grid=if self.two_rows{2.0*self.cell as f32+self.gap as f32+8.0}else{self.cell as f32+12.0};
            grid.max(if self.inline_metrics{68.0}else{0.0})
        }
    }
}
#[cfg(test)]
fn measure_widgets(display:&Display,style:&Appearance,count:usize,focus:Option<f32>,amount:f32,side:Option<bool>,pinned:&[UtilityWidget],cell:u32)->Layout{
    measure_cluster(display,style,count,focus,amount,pinned,Cluster{side,cell,gap:4,two_rows:false,inline_metrics:false})
}
pub(super) fn measure_cluster(display:&Display,style:&Appearance,count:usize,focus:Option<f32>,amount:f32,pinned:&[UtilityWidget],cluster:Cluster)->Layout{
    let vertical=style.position!=DockSide::Bottom;
    let leading=cluster.side==Some(true);
    let host=cluster.side.is_some();
    let scale=(display.dpi as f32/96.0).clamp(0.75,3.0);
    let screen_w=(display.work.right-display.work.left).max(1) as f32;
    let screen_h=(display.work.bottom-display.work.top).max(1) as f32;
    let (axis,cross_axis)=if vertical{(screen_h,screen_w)}else{(screen_w,screen_h)};
    let edge=(8.0*scale).min(axis.min(cross_axis)*0.04);
    let shadow=(6.0*scale).min(axis.min(cross_axis)*0.035);
    let pad=(if vertical || !display.primary && screen_h>screen_w {8.0}else{10.0}*scale).min(axis*0.04);
    let room=(axis-2.0*(edge+shadow+pad)).max(1.0);
    let style_gap=style.spacing as f32*scale;
    let spacing=style_gap.min(if vertical||screen_h>screen_w{8.0*scale}else{14.0*scale})
        .min(room/(count.max(1) as f32*5.0));
    let gap=(cluster.gap as f32*scale).min(room*0.06);
    let group_gap=(10.0*scale).min(room*0.06);
    let rows=if cluster.two_rows{2}else{1};
    let ideal_status=(cluster.cell as f32*scale).min(room*0.32).min((cross_axis*0.75-gap)/rows as f32).max(1.0);
    let apps=count+usize::from(!host);
    let gaps=apps.saturating_sub(1) as f32*spacing;
    let zoom=if style.reduced_motion{0.0}else{style.magnification as f32/100.0};
    // The sum of these Gaussian weights is below 2.6 over all slot phases.
    let spill=2.6*zoom;
    let span=|n:usize| if !host{0.0}else{
        let cols=(n+2).div_ceil(rows);
        cols as f32*ideal_status+cols.saturating_sub(1) as f32*gap+10.0*scale
    };
    let mut widgets=pinned.iter().copied().filter(|&w|w!=UtilityWidget::Spotlight).collect::<Vec<_>>();
    let min_icon=(style.icon_size.min(29) as f32)*scale;
    let app_gap=if host&&apps>0{group_gap}else{0.0};
    let minimum=if apps>0{gaps+app_gap+min_icon*(apps as f32+spill)}else{0.0};
    let metric_span=(if vertical{96.0}else{144.0})*scale;
    let inline_metrics=host&&cluster.inline_metrics&&widgets.contains(&UtilityWidget::Metrics)
        && minimum+span(0)+metric_span+gap<=room;
    let extras=if inline_metrics{metric_span+gap}else{0.0};
    if inline_metrics{widgets.retain(|&w|w!=UtilityWidget::Metrics);}
    widgets.truncate(10);
    let inline_count=if host{
        (0..=widgets.len()).rev().find(|&n|span(n)+minimum+extras<=room).unwrap_or(0)
    }else{0};
    let status_span=span(inline_count)+extras;
    let icon=if apps==0{0.0}else{
        (style.icon_size as f32*scale).min(
            ((room-status_span-app_gap-gaps)/(apps as f32+spill)).max(1.0)
        )
    };
    let lanes=if host{rows.min(inline_count+2)}else{0};
    let grid_cross=lanes as f32*ideal_status+lanes.saturating_sub(1) as f32*gap;
    let metric_cross=if inline_metrics{(if vertical{62.0}else{56.0})*scale}else{0.0};
    let panel_cross=(icon+18.0*scale).max(grid_cross.max(metric_cross)+8.0*scale);
    let total_cross=(panel_cross.max(icon*(1.0+zoom)+18.0*scale)+2.0*shadow).ceil()
        .min((cross_axis-edge).max(1.0));
    let rest_apps=icon*apps as f32+gaps;
    let slack=if apps>0{icon*spill}else{0.0};
    let rest_span=rest_apps+status_span+app_gap+2.0*pad;
    let extent=(rest_span+slack+2.0*shadow).ceil().min((axis-2.0*edge).max(1.0));
    // Keep the resting capsule centered on the monitor. Reserve motion
    // clearance on the app side; utility anchors never slide during zoom.
    let rest_start=shadow+if host && !leading{slack}else if !host{slack*0.5}else{0.0};
    let center=(axis-rest_span)*0.5-rest_start;
    let offset=center.clamp(edge,(axis-edge-extent).max(edge)).round();
    let (x,y,w,h)=match style.position{
        DockSide::Bottom=>(display.work.left+offset as i32,display.work.bottom-(edge+total_cross).ceil() as i32,extent,total_cross),
        DockSide::Left=>(display.work.left+edge.ceil() as i32,display.work.top+offset as i32,total_cross,extent),
        DockSide::Right=>(display.work.right-(edge+total_cross).ceil() as i32,display.work.top+offset as i32,total_cross,extent),
    };
    let wall=if style.position==DockSide::Left{shadow}else{total_cross-shadow};
    let panel_base=if style.position==DockSide::Left{wall}else{wall-panel_cross};
    let bar=|major:f32,minor:f32,along:f32,across:f32|if vertical{
        Rect{x:minor,y:major,w:across,h:along}
    }else{Rect{x:major,y:minor,w:along,h:across}};
    let base=rest_start+pad;
    let status_start=if leading{base}else{base+rest_apps+app_gap};
    let app_rest=if leading{base+status_span+app_gap}else{base};
    let widths=(0..apps).map(|i|{
        let center=app_rest+i as f32*(icon+spacing)+icon*0.5;
        let delta=focus.map_or(10.0,|pos|(center-pos)/(icon+spacing).max(1.0));
        icon*(1.0+amount.clamp(0.0,1.0)*zoom*(-delta*delta/1.60).exp())
    }).collect::<Vec<_>>();
    let app_len=widths.iter().sum::<f32>()+gaps;
    let expanded=app_len-rest_apps;
    let range_start=if leading{app_rest}else{shadow+pad};
    let range_end=if host&&!leading{status_start-app_gap}else{extent-shadow-pad};
    let app_start=if apps==0{app_rest}else{
        let ideal=focus.filter(|_|icon>0.0).map_or(app_rest-expanded*0.5,|pos|{
            let idx=((pos-app_rest)/(icon+spacing).max(1.0)).floor().clamp(0.0,(apps-1) as f32) as usize;
            let before=widths[..idx].iter().sum::<f32>()-icon*idx as f32;
            app_rest-before-(widths[idx]-icon)*0.5
        });
        ideal.clamp(range_start,(range_end-app_len).max(range_start))
    };
    let mut cursor=app_start;
    let mut slots=Vec::with_capacity(count);
    let mut indicators=Vec::with_capacity(count);
    let mut scan=Rect::default();
    for (i,&size) in widths.iter().enumerate(){
        let minor=if style.position==DockSide::Left{wall+12.0*scale}else{wall-12.0*scale-size};
        let slot=bar(cursor,minor,size,size);
        if i<count{
            slots.push(slot);
            let marker=if style.position==DockSide::Left{wall+5.5*scale}else{wall-5.5*scale};
            let mid=cursor+size/2.0;
            indicators.push(if vertical{(marker,mid)}else{(mid,marker)});
        }else{scan=slot;}
        cursor+=size+spacing;
    }
    let mut inline=Vec::with_capacity(inline_count);
    let mut utility=Rect::default();
    let mut group=None;
    let mut divider=None;
    let mut metrics=None;
    if host{
        let n=inline_count+2;
        let cols=n.div_ceil(rows);
        let first=status_start+5.0*scale;
        let minor=panel_base+(panel_cross-grid_cross)*0.5;
        // Row-major ordering matches keyboard traversal even when rotated.
        let slot=|idx:usize| {
            let row=idx/cols;
            let col=idx%cols;
            let len=(n-row*cols).min(cols);
            let bias=(cols-len) as f32*(ideal_status+gap)*0.5;
            bar(first+bias+col as f32*(ideal_status+gap),minor+row as f32*(ideal_status+gap),ideal_status,ideal_status)
        };
        scan=slot(0);
        utility=slot(n-1);
        inline.extend(widgets.into_iter().take(inline_count).enumerate().map(|(i,w)|(w,slot(i+1))));
        if inline_metrics{
            let r=bar(status_start+span(inline_count)+gap-4.0*scale,panel_base+(panel_cross-metric_cross)*0.5,metric_span,metric_cross);
            metrics=Some(r);
            inline.push((UtilityWidget::Metrics,r));
        }
        let inner_cross=grid_cross.max(metric_cross);
        group=Some(bar(status_start,panel_base+(panel_cross-inner_cross)*0.5-3.0*scale,status_span,inner_cross+6.0*scale));
        if apps>0{
            let major=if leading{status_start+status_span+app_gap/2.0}else{status_start-app_gap/2.0};
            divider=Some(bar(major-scale*0.5,panel_base+panel_cross*0.26,scale,panel_cross*0.48));
        }
    }
    let app_end=app_start+app_len;
    let glass_left=(if host&&leading{status_start}else if apps>0{app_start}else{status_start})-pad;
    let glass_right=(if host&&!leading||apps==0{status_start+status_span}else{app_end})+pad;
    let glass=bar(glass_left,panel_base,(glass_right-glass_left).max(1.0),panel_cross);
    Layout{window:RECT{left:x,top:y,right:x+w as i32,bottom:y+h as i32},glass,slots,scan,utility,utility_leading:leading,
        cluster:group,divider,metrics,inline,indicators,scale,side:style.position}
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
    #[test]
    fn two_lane_status_has_stable_hit_boxes_and_fits_both_orientations(){
        let pins=[UtilityWidget::Network,UtilityWidget::Sound,UtilityWidget::Bluetooth,UtilityWidget::Calendar,UtilityWidget::QuickSettings];
        for (width,height) in [(1080,1920),(1920,1080),(600,960)]{
            for dpi in [96,144,192]{
                let work=RECT{left:-width,top:-200,right:0,bottom:height-200};
                let screen=Display{id:"test".into(),name:String::new(),bounds:work,work,primary:false,dpi};
                for position in [DockSide::Bottom,DockSide::Left,DockSide::Right]{
                    for side in [Some(true),Some(false)]{
                        let style=Appearance{position,magnification:55,..Default::default()};
                        let cfg=Cluster{side,two_rows:true,cell:28,gap:4,inline_metrics:false};
                        let a=measure_cluster(&screen,&style,7,None,0.0,&pins,cfg);
                        let b=measure_cluster(&screen,&style,7,Some(a.axis(a.slots[3].x,a.slots[3].y)),1.0,&pins,cfg);
                        assert_eq!(a.scan,b.scan);assert_eq!(a.utility,b.utility);assert_eq!(a.inline,b.inline);
                        assert_eq!(a.hot(a.scan.x+a.scan.w/2.0,a.scan.y+a.scan.h/2.0),Some(7));
                        let cells=[a.scan,a.utility].into_iter().chain(a.inline.iter().map(|(_,r)|*r)).collect::<Vec<_>>();
                        let cross=|r:Rect|if position==DockSide::Bottom{r.y}else{r.x};
                        assert_ne!(cross(a.scan),cross(a.utility));
                        assert!(cells.iter().enumerate().all(|(i,r)|cells[i+1..].iter().all(|&other|!overlaps(*r,other))
                            && b.slots.iter().all(|&slot|!overlaps(*r,slot))));
                        assert!(a.window.left>=work.left&&a.window.right<=work.right&&a.window.top>=work.top&&a.window.bottom<=work.bottom);
                    }
                }
            }
        }
    }
    #[test]
    fn metric_tile_stays_clickable_during_zoom_and_yields_to_overflow_on_narrow_displays(){
        let pins=[UtilityWidget::Network,UtilityWidget::Sound,UtilityWidget::Bluetooth,UtilityWidget::Metrics];
        for position in [DockSide::Bottom,DockSide::Left,DockSide::Right]{
            let r=RECT{left:-1080,top:-240,right:0,bottom:1680};
            let screen=Display{id:"display".into(),name:String::new(),bounds:r,work:r,primary:false,dpi:144};
            let style=Appearance{position,magnification:55,..Default::default()};
            let cfg=Cluster{side:Some(false),two_rows:true,cell:28,gap:4,inline_metrics:true};
            let a=measure_cluster(&screen,&style,6,None,0.0,&pins,cfg);
            let b=measure_cluster(&screen,&style,6,Some(a.axis(a.slots[2].x,a.slots[2].y)),1.0,&pins,cfg);
            let rect=a.metrics.unwrap();
            assert_eq!(b.metrics,Some(rect));
            assert_eq!(b.inline_at(rect.x+rect.w/2.0,rect.y+rect.h/2.0),Some(UtilityWidget::Metrics));
            assert!(!overlaps(rect,b.utility)&&b.slots.iter().all(|r|!overlaps(rect,*r)));
        }
        let r=RECT{left:0,top:0,right:440,bottom:800};
        let screen=Display{id:"narrow".into(),name:String::new(),bounds:r,work:r,primary:false,dpi:192};
        let cfg=Cluster{side:Some(false),two_rows:true,cell:28,gap:4,inline_metrics:true};
        let result=measure_cluster(&screen,&Appearance::default(),10,None,0.0,&pins,cfg);
        assert!(result.metrics.is_none()&&result.utility.w>0.0);
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
                Some(false)=>assert!(l.utility.x>=l.slots.last().unwrap().x+l.slots.last().unwrap().w),
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
                assert_eq!(&lead.key_order()[..7],&[10,12,13,14,15,16,11]);
                assert_eq!(l.key_order(),(0..=10).chain(12..17).chain([11]).collect::<Vec<_>>());
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
