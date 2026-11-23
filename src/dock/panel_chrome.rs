//! Shared window anchoring and alpha for native utility popovers.
use super::{DockSide,geometry::Rect,glass_probe::Layer,surface::{draw,distance,Palette}};
use windows_sys::Win32::{
    Foundation::*,Graphics::{Dwm::DwmSetWindowAttribute,Gdi::*},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::*,
};
use std::{mem,ptr};

#[derive(Clone, Copy)]
pub(super) struct Anchor {pub point:POINT,pub side:DockSide,pub dark:bool,pub transparency:u32}
impl Anchor {
    pub unsafe fn for_owner(owner:HWND,point:POINT)->Self{
        let (side,dark,transparency)=super::STATE.with(|s|{
            let state=s.borrow();
            let side=state.bars.iter().find(|b|b.hwnd==owner).and_then(|b|b.layout.as_ref())
                .map_or(DockSide::Bottom,|l|l.side);
            let material=state.material;
            (side,material.is_some_and(|m|m.dark),material.map_or(36,|m|m.transparency))
        });
        Self{point,side,dark,transparency}
    }
    pub unsafe fn work(self)->Option<RECT>{
        let mut monitor:MONITORINFO=mem::zeroed();
        monitor.cbSize=mem::size_of::<MONITORINFO>() as u32;
        (GetMonitorInfoW(MonitorFromPoint(self.point,MONITOR_DEFAULTTONEAREST),&mut monitor)!=0).then_some(monitor.rcWork)
    }
    pub unsafe fn panel_work(self,dpi:u32)->Option<RECT>{
        panel_work(self.point,self.side,scale(dpi,PANEL_GAP))
    }
    pub fn alpha(self)->u8{(254-self.transparency.clamp(4,74)/4) as u8}
}
pub(crate) const PANEL_GAP:i32=10;
pub(crate) unsafe fn panel_work(point:POINT,side:DockSide,gap:i32)->Option<RECT>{
    let mut info:MONITORINFO=mem::zeroed();
    info.cbSize=mem::size_of::<MONITORINFO>() as u32;
    (GetMonitorInfoW(MonitorFromPoint(point,MONITOR_DEFAULTTONEAREST),&mut info)!=0)
        .then(||panel_area(info.rcMonitor,info.rcWork,point,side,gap)).flatten()
}
fn panel_area(monitor:RECT,work:RECT,point:POINT,side:DockSide,gap:i32)->Option<RECT>{
    let mut area=RECT{left:monitor.left,right:monitor.right,top:work.top,bottom:work.bottom};
    match side{
        DockSide::Bottom=>area.bottom=monitor.bottom.min(point.y-gap),
        DockSide::Left=>area.left=area.left.max(point.x+gap),
        DockSide::Right=>area.right=area.right.min(point.x-gap),
    }
    (area.right>area.left&&area.bottom>area.top).then_some(area)
}
pub(super) fn placement(work:RECT,anchor:Anchor,width:i32,height:i32,gap:i32)->(i32,i32){
    let p=anchor.point;
    let (x,y)=match anchor.side{
        DockSide::Bottom=>(p.x-width/2,p.y-height-gap),
        DockSide::Left=>(p.x+gap,p.y-height/2),
        DockSide::Right=>(p.x-width-gap,p.y-height/2),
    };
    (x.clamp(work.left,(work.right-width).max(work.left)),y.clamp(work.top,(work.bottom-height).max(work.top)))
}
pub(super) unsafe fn follow_rect(anchor:Anchor,dpi:u32,width:i32,height:i32)->Option<RECT>{
    let work=anchor.panel_work(dpi)?;
    if width>work.right-work.left||height>work.bottom-work.top{return None;}
    let (x,y)=placement(work,anchor,width,height,scale(dpi,PANEL_GAP));
    Some(RECT{left:x,top:y,right:x+width,bottom:y+height})
}
pub(crate) fn scale(dpi:u32,n:i32)->i32{
    (n*dpi.clamp(72,288) as i32+48)/96
}
pub(super) unsafe fn register(name:&[u16],proc:WNDPROC)->bool{
    register_with_style(name,proc,CS_DROPSHADOW)
}
pub(super) unsafe fn register_with_style(name:&[u16],proc:WNDPROC,style:u32)->bool{
    let wc=WNDCLASSW{style,lpfnWndProc:proc,
        hInstance:GetModuleHandleW(ptr::null()),hCursor:LoadCursorW(ptr::null_mut(),IDC_ARROW),
        lpszClassName:name.as_ptr(),..mem::zeroed()};
    RegisterClassW(&wc)!=0||GetLastError()==ERROR_CLASS_ALREADY_EXISTS
}
pub(super) unsafe fn frame(hwnd:HWND,width:i32,height:i32,dpi:u32,anchor:Anchor){
    disable_system_border(hwnd);
    clip_corners(hwnd,width,height,dpi);
    SetLayeredWindowAttributes(hwnd,0,anchor.alpha(),LWA_ALPHA);
}
pub(super) unsafe fn clip_corners(hwnd:HWND,width:i32,height:i32,dpi:u32){
    let r=scale(dpi,32).max(2);
    let region=CreateRoundRectRgn(0,0,width+1,height+1,r,r);
    if !region.is_null()&&SetWindowRgn(hwnd,region,0)==0{DeleteObject(region);}
}
pub(super) unsafe fn disable_system_border(hwnd:HWND){
    let no_round=1u32; // DWMWCP_DONOTROUND: this window supplies its own corners.
    let no_border=0xfffffffeu32; // DWMWA_COLOR_NONE
    let _=DwmSetWindowAttribute(hwnd,33,(&no_round as *const u32).cast(),4);
    let _=DwmSetWindowAttribute(hwnd,34,(&no_border as *const u32).cast(),4);
}
/// Common shadow for the three Dock panels. Its transparent surface never
/// covers the capsule; the Dock-facing falloff reaches zero within the gap.
pub(crate) struct PanelShadow{pub(super) hwnd:HWND,style:(i32,i32,u32,DockSide,bool),inset:(i32,i32),at:(i32,i32)}
impl Drop for PanelShadow{
    fn drop(&mut self){unsafe{DestroyWindow(self.hwnd);}}
}
impl PanelShadow{
    pub(crate) unsafe fn sync(slot:&mut Option<Self>,panel:HWND,owner:HWND,rect:RECT,dpi:u32,side:DockSide,dark:bool){
        let (width,height)=(rect.right-rect.left,rect.bottom-rect.top);
        let style=(width,height,dpi,side,dark);
        if let Some(shadow)=slot.as_mut().filter(|s|s.style==style){
            let at=(rect.left-shadow.inset.0,rect.top-shadow.inset.1);
            if shadow.at!=at{
                SetWindowPos(shadow.hwnd,ptr::null_mut(),at.0,at.1,0,0,SWP_NOACTIVATE|SWP_NOSIZE|SWP_NOZORDER);
                shadow.at=at;
            }
            return;
        }
        *slot=None;
        let pad=scale(dpi,21);
        let near=(scale(dpi,PANEL_GAP)-1).max(1);
        let (left,right,bottom)=match side{
            DockSide::Bottom=>(pad,pad,near),
            DockSide::Left=>(near,pad,pad),
            DockSide::Right=>(pad,near,pad),
        };
        let at=(rect.left-left,rect.top-pad);
        let (w,h)=(width+left+right,height+pad+bottom);
        let Some(mut layer)=Layer::new(w,h)else{return;};
        let card=Rect{x:left as f32,y:pad as f32,w:width as f32,h:height as f32};
        let canvas=Rect{x:0.0,y:0.0,w:w as f32,h:h as f32};
        let radius=scale(dpi,16) as f32;
        let data=layer.pixels();data.fill(0);
        draw(data,w,h,canvas,|x,y|{
            let d=distance(card,radius,x,y);
            let toward=match side{
                DockSide::Bottom=>(y-card.y-card.h).max(0.0),
                DockSide::Left=>(card.x-x).max(0.0),
                DockSide::Right=>(x-card.x-card.w).max(0.0),
            };
            let falloff=(1.0-d.max(0.0)/pad as f32).clamp(0.0,1.0);
            let facing=(1.0-toward/near as f32).clamp(0.0,1.0);
            let inner=(1.0+d/(pad as f32*0.5)).clamp(0.0,1.0);
            Some(([20,18,16],if dark{104.0}else{84.0}*falloff*falloff*facing*facing*inner))
        });
        let name=crate::desktop::wide("STATIC");
        let hwnd=CreateWindowExW(WS_EX_LAYERED|WS_EX_TRANSPARENT|WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST,
            name.as_ptr(),ptr::null(),WS_POPUP,at.0,at.1,w,h,owner,ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
        if hwnd.is_null(){return;}
        let shadow=Self{hwnd,style,inset:(left,pad),at};
        disable_system_border(hwnd);
        if layer.present_at(hwnd,Some(POINT{x:at.0,y:at.1}),255)&&
            SetWindowPos(hwnd,panel,0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE|SWP_SHOWWINDOW)!=0{
            *slot=Some(shadow);
        }
    }
}
/// Start keeps its native Edit control. The GDI window paints the solid
/// interior; this sister window supplies only the four fractional-alpha
/// corners, sampled from that exact same frame. It never takes focus or input.
pub(crate) struct StartCorners{hwnd:HWND,layer:Layer}
impl Drop for StartCorners{
    fn drop(&mut self){unsafe{DestroyWindow(self.hwnd);}}
}
impl StartCorners{
    pub(crate) unsafe fn paint(slot:&mut Option<Self>,owner:HWND,dc:HDC,w:i32,h:i32,dpi:u32){
        if slot.as_ref().is_none_or(|s|s.layer.size!=(w,h)){
            *slot=None;
            let class=crate::desktop::wide("STATIC");
            let Some(layer)=Layer::new(w,h)else{return;};
            let hwnd=CreateWindowExW(WS_EX_LAYERED|WS_EX_TRANSPARENT|WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST,
                class.as_ptr(),ptr::null(),WS_POPUP,0,0,w,h,ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
            if hwnd.is_null(){return;}
            disable_system_border(hwnd);
            *slot=Some(Self{hwnd,layer});
        }
        let Some(edge)=slot.as_mut()else{return;};
        let r=scale(dpi,16).min(w/2).min(h/2);
        edge.layer.pixels().fill(0);
        let dest=edge.layer.dc();
        for (x,y) in [(0,0),(w-r,0),(0,h-r),(w-r,h-r)]{
            BitBlt(dest,x,y,r,r,dc,x,y,SRCCOPY);
        }
        GdiFlush();
        let pixels=edge.layer.pixels();
        for (x,y) in [(0,0),(w-r,0),(0,h-r),(w-r,h-r)]{
            for n in y..y+r{for m in x..x+r{
                let dx=r as f32-(m.min(w-1-m) as f32+0.5);
                let dy=r as f32-(n.min(h-1-n) as f32+0.5);
                let a=((r as f32+0.5-dx.hypot(dy)).clamp(0.0,1.0)*255.0).round() as u8;
                let p=&mut pixels[((n*w+m)*4) as usize..][..4];
                for c in &mut p[..3]{*c=((u16::from(*c)*u16::from(a)+127)/255) as u8;}
                p[3]=a;
            }}
        }
        let mut rect:RECT=mem::zeroed();
        if GetWindowRect(owner,&mut rect)==0{return;}
        if !edge.layer.present_at(edge.hwnd,Some(POINT{x:rect.left,y:rect.top}),255)||
            SetWindowPos(edge.hwnd,owner,0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE|SWP_SHOWWINDOW)==0{
            *slot=None;
            clip_corners(owner,w,h,dpi);
            return;
        }
    }
    pub(crate) unsafe fn move_to(&self,x:i32,y:i32){
        SetWindowPos(self.hwnd,ptr::null_mut(),x,y,0,0,SWP_NOACTIVATE|SWP_NOSIZE|SWP_NOZORDER);
    }
}
pub(crate) unsafe fn start_frame(hwnd:HWND,width:i32,height:i32,dpi:u32){
    let disabled=1u32; // DWMNCRP_DISABLED: the shared shadow supplies the frame.
    let _=DwmSetWindowAttribute(hwnd,2,(&disabled as *const u32).cast(),4);
    disable_system_border(hwnd);
    let r=scale(dpi,16).min(width/2).min(height/2);
    let region=CreateRectRgn(0,r,width,height-r);
    let line=CreateRectRgn(0,0,0,0);
    if region.is_null()||line.is_null(){
        if !region.is_null(){DeleteObject(region);}
        if !line.is_null(){DeleteObject(line);}
        clip_corners(hwnd,width,height,dpi);
        return;
    }
    for y in 0..r{
        let dy=r as f32-y as f32-0.5;
        let inner=((r as f32-1.3).powi(2)-dy*dy).max(0.0).sqrt();
        let x=(r as f32-inner).ceil() as i32;
        for row in [y,height-1-y]{
            SetRectRgn(line,x,row,width-x,row+1);
            CombineRgn(region,region,line,RGN_OR);
        }
    }
    DeleteObject(line);
    if SetWindowRgn(hwnd,region,0)==0{DeleteObject(region);clip_corners(hwnd,width,height,dpi);}
}
pub(super) fn mask_popup(data:&mut [u8],w:i32,h:i32,radius:f32,opacity:u8){
    let (w,h)=(w as usize,h as usize);
    let r=radius.min(w.min(h) as f32*0.5);
    for (y,row) in data.chunks_exact_mut(w*4).enumerate(){
        let dy=(r-(y.min(h-1-y) as f32+0.5)).max(0.0);
        for (x,pixel) in row.chunks_exact_mut(4).enumerate(){
            let dx=(r-(x.min(w-1-x) as f32+0.5)).max(0.0);
            let alpha=if dx==0.0||dy==0.0{opacity}else{((r+0.5-dx.hypot(dy)).clamp(0.0,1.0)*opacity as f32).round() as u8};
            for channel in &mut pixel[..3]{*channel=((*channel as u16*alpha as u16+127)/255) as u8;}
            pixel[3]=alpha;
        }
    }
}
pub(super) fn color(rgb:[u8;3])->u32{rgb[2] as u32|((rgb[1] as u32)<<8)|((rgb[0] as u32)<<16)}
pub(super) unsafe fn text(dc:HDC,mut bounds:RECT,string:&str,ink:[u8;3],flags:u32){
    if flags&DT_SINGLELINE!=0&&flags&DT_RIGHT==0{
        let current=GetCurrentObject(dc,OBJ_FONT as u32);
        if crate::native_text::draw(dc,bounds,string,current,ink,flags&DT_CENTER!=0,flags&DT_VCENTER!=0){return;}
    }
    SetTextColor(dc,color(ink));SetBkMode(dc,TRANSPARENT as i32);
    DrawTextW(dc,crate::desktop::wide(string).as_ptr(),-1,&mut bounds,DT_NOPREFIX|flags);
}
pub(super) unsafe fn box_fill(dc:HDC,rect:RECT,radius:i32,fill:[u8;3],edge:Option<[u8;3]>){
    let b=CreateSolidBrush(color(fill));
    let pen=edge.map(|rgb|CreatePen(PS_SOLID,1,color(rgb))).unwrap_or_else(||GetStockObject(NULL_PEN) as _);
    let b0=SelectObject(dc,b);
    let p0=SelectObject(dc,pen);
    RoundRect(dc,rect.left,rect.top,rect.right,rect.bottom,radius,radius);
    SelectObject(dc,b0);SelectObject(dc,p0);
    DeleteObject(b);
    if edge.is_some(){DeleteObject(pen);}
}
pub(super) unsafe fn choose_font(dpi:u32,n:i32,weight:i32)->HFONT{
    crate::native_text::font_pixels(scale(dpi,n),weight)
}
pub(super) unsafe fn child_color(dc:HDC,dark:bool)->LRESULT{
    let p=Palette::new(dark);
    SetTextColor(dc,color(p.ink));
    SetBkColor(dc,color(p.panel));
    SetDCBrushColor(dc,color(p.panel));
    GetStockObject(DC_BRUSH) as LRESULT
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn panel_has_the_same_gap_across_dpis_edges_and_dock_sizes(){
        let monitor=RECT{left:-1440,top:-300,right:0,bottom:1200};
        for dpi in [96,144,192]{
            let gap=scale(dpi,PANEL_GAP);
            for displacement in [52,90,175]{
                for side in [DockSide::Bottom,DockSide::Left,DockSide::Right]{
                    let point=match side{
                        DockSide::Bottom=>POINT{x:-90,y:monitor.bottom-displacement},
                        DockSide::Left=>POINT{x:monitor.left+displacement,y:-240},
                        DockSide::Right=>POINT{x:monitor.right-displacement,y:-240},
                    };
                    // Work area is inset by Stage. Along the edge of the
                    // Dock, only the physical monitor constrains the popup.
                    let work=RECT{left:-1140,right:-260,..monitor};
                    let area=panel_area(monitor,work,point,side,gap).unwrap();
                    let anchor=Anchor{point,side,dark:false,transparency:32};
                    let (w,h)=(scale(dpi,320),scale(dpi,348));
                    let (x,y)=placement(area,anchor,w,h,gap);
                    assert!(x>=area.left&&x+w<=area.right&&y>=area.top&&y+h<=area.bottom);
                    match side{
                        DockSide::Bottom=>assert_eq!((x+w,point.y-(y+h)),(monitor.right,gap)),
                        DockSide::Left=>assert_eq!(x-point.x,gap),
                        DockSide::Right=>assert_eq!(point.x-(x+w),gap),
                    }
                }
            }
        }
    }
    #[test]
    fn popup_keeps_a_smooth_premultiplied_edge(){
        let mut data=vec![255;24*24*4];
        mask_popup(&mut data,24,24,10.0,230);
        assert_eq!(&data[..4],&[0;4]);
        assert_eq!(&data[12*4..13*4],&[230;4]);
        assert!(data.chunks_exact(4).any(|p|p[3]>0&&p[3]<230));
        assert!(data.chunks_exact(4).all(|p|p[..3].iter().all(|&v|v<=p[3])));
    }
    #[test]
    fn all_anchors_remain_on_the_selected_negative_monitor(){
        let work=RECT{left:-1080,top:-731,right:0,bottom:1141};
        for side in [DockSide::Bottom,DockSide::Left,DockSide::Right] {
            for point in [POINT{x:-1078,y:-728},POINT{x:-2,y:1139},POINT{x:-500,y:100}] {
                let (x,y)=placement(work,Anchor{point,side,dark:true,transparency:30},460,450,10);
                assert!(x>=work.left&&x+460<=work.right&&y>=work.top&&y+450<=work.bottom);
            }
        }
    }
}

