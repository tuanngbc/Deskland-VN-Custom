//! Shared window anchoring and alpha for native utility popovers.
use super::{DockSide,surface::Palette};
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
    pub fn alpha(self)->u8{(254-self.transparency.clamp(4,74)/4) as u8}
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
pub(super) fn scale(dpi:u32,n:i32)->i32{
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
    CreateFontW(-scale(dpi,n),0,0,0,weight,0,0,0,DEFAULT_CHARSET as u32,0,0,
        ANTIALIASED_QUALITY as u32,DEFAULT_PITCH as u32,crate::desktop::wide("Segoe UI Variable Text").as_ptr())
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

