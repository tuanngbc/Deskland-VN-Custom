//! Floating Stage geometry and paint. A disconnected Win32 region gives the
//! gaps real desktop transparency without changing the DWM destination type.
use super::{geometry::Rect,surface::{rounded, icon_on, fallback_icon, Atlas},glass_probe::Layer,Item};
use crate::desktop::wide;
use windows_sys::Win32::{Foundation::{HWND,RECT},Graphics::Gdi::*};

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) struct Box2 { pub x:i32,pub y:i32,pub w:i32,pub h:i32 }
impl Box2 {
    fn rect(self)->Rect{Rect{x:self.x as f32,y:self.y as f32,w:self.w as f32,h:self.h as f32}}
    pub fn win(self)->RECT{RECT{left:self.x,top:self.y,right:self.x+self.w,bottom:self.y+self.h}}
    fn hit(self,x:i32,y:i32)->bool{x>=self.x&&x<self.x+self.w&&y>=self.y&&y<self.y+self.h}
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) struct Layout {
    pub width:i32,pub height:i32,pub card_h:i32,pub inset:i32,pub gap:i32,
    pub radius:i32,pub caption:i32,pub rows:usize,pub count:usize,pub paged:bool,
    pub stack:u8,pub nav_h:i32,pub right:bool,
}
impl Layout {
    pub fn new(width:i32,height:i32,dpi:u32,gap:i32,limit:usize,total:usize,right:bool)->Self{
        let px=|n:i32|(n*dpi.clamp(72,288) as i32+48)/96;
        let inset=px(6);
        let nav_h=px(25);
        let gap=gap.max(px(5));
        let min_h=px(48);
        let rows_for=|nav|((height-inset*2-nav+gap)/(min_h+gap)).max(1) as usize;
        let limit=limit.clamp(1,8);
        let paged=total>rows_for(0).min(limit);
        let reserve=if paged{nav_h+gap}else{0};
        let rows=rows_for(reserve).min(limit);
        let shown=total.min(rows).max(1) as i32;
        let available=(height-2*inset-reserve-(shown-1)*gap)/shown;
        let ideal=((width-2*inset)*2/3+px(18)).clamp(min_h,px(160));
        let card_h=available.min(ideal).max(1);
        Self{width,height,card_h,inset,gap,radius:px(10),caption:px(24),rows,count:0,paged,stack:0,nav_h,right}
    }
    pub fn card(self,i:usize)->Box2 {
        // Reserve the same margin for the stack on every card so thumbnails
        // do not jump sideways when a window joins a group.
        let room=(self.inset*2).min((self.width-12).max(0)/3);
        let x=self.inset+if self.right{room}else{0};
        Box2{x,y:self.inset+i as i32*(self.card_h+self.gap),w:(self.width-2*self.inset-room).max(1),h:self.card_h}
    }
    fn rear(self,i:usize,depth:i32)->Box2{
        let mut c=self.card(i);
        let d=(self.inset*depth/2).max(1);
        c.x+=if self.right{-d}else{d};
        c.y+=d/2;c.h=(c.h-d).max(1);c
    }
    pub fn preview(self,i:usize)->RECT{
        let c=self.card(i);let p=(self.inset/2).max(2);
        RECT{left:c.x+p,top:c.y+p,right:c.x+c.w-p,bottom:c.y+c.h-self.caption}
    }
    pub fn nav(self)->Option<Box2>{
        if !self.paged||self.count==0{return None;}
        let c=self.card(self.count-1);
        let w=(c.w*2/3).max(1);
        let y=c.y+c.h+self.gap;
        (y+self.nav_h<=self.height).then_some(Box2{x:c.x+(c.w-w)/2,y,w,h:self.nav_h})
    }
    pub fn hit(self,x:i32,y:i32)->Option<usize>{
        (0..self.count).find(|&i|self.card(i).hit(x,y)||(self.stack&(1<<i)!=0&&self.rear(i,3).hit(x,y)))
    }
    pub unsafe fn apply(self,hwnd:HWND)->bool {
        let region=CreateRectRgn(0,0,0,0);
        if region.is_null(){return false;}
        let add=|b:Box2,r:i32|{
            let diameter=(2*r).min(b.w).min(b.h).max(1);
            let tile=CreateRoundRectRgn(b.x,b.y,b.x+b.w+1,b.y+b.h+1,diameter,diameter);
            if tile.is_null(){return false;}
            let ok=CombineRgn(region,region,tile,RGN_OR)!=0;
            DeleteObject(tile);
            ok
        };
        let mut ok=true;
        for i in 0..self.count{
            if self.stack&(1<<i)!=0{ok&=add(self.rear(i,3),self.radius);}
            ok&=add(self.card(i),self.radius);
        }
        if let Some(nav)=self.nav(){ok&=add(nav,nav.h/2);}
        if ok&&SetWindowRgn(hwnd,region,1)!=0{return true;} // Win32 owns it after success.
        DeleteObject(region);
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Look {pub dark:bool,pub layout:Layout,pub active:u8,pub hover:Option<usize>}

pub(super) struct Panel {layer:Layer,look:Option<Look>,static_pixels:Vec<u8>,font:HFONT,dpi:u32}
impl Panel {
    pub unsafe fn new(w:i32,h:i32,dpi:u32)->Option<Self>{
        let layer=Layer::new(w,h)?;
        let font_height=((12*dpi.clamp(72,288)+48)/96) as i32;
        let font=CreateFontW(-font_height,0,0,0,500,0,0,0,DEFAULT_CHARSET as u32,0,0,
            ANTIALIASED_QUALITY as u32,DEFAULT_PITCH as u32,wide("Segoe UI Variable").as_ptr());
        Some(Self{layer,look:None,static_pixels:Vec::new(),font,dpi})
    }
    pub fn geometry(&self)->(i32,i32,u32){(self.layer.size.0,self.layer.size.1,self.dpi)}
    pub fn font(&self)->HFONT{self.font}
    pub unsafe fn paint(&mut self,target:HDC,look:Look,items:&[Item],atlas:&Atlas,ready:&[bool])->bool {
        let (w,h)=self.layer.size;
        let layout=look.layout;
        if self.look!=Some(look){
            self.static_pixels.resize(w as usize*h as usize*4,0);
            self.static_pixels.fill(0);
            let data=&mut self.static_pixels;
            for i in 0..layout.count {
                let b=layout.card(i).rect();
                let radius=(layout.radius as f32).min(b.h/2.0);
                if layout.stack&(1<<i)!=0 {
                    rounded(data,w,h,layout.rear(i,3).rect(),radius,if look.dark{[66,56,51]}else{[166,153,145]},255);
                    rounded(data,w,h,layout.rear(i,2).rect(),radius,if look.dark{[89,80,73]}else{[208,199,192]},255);
                }
                let active=look.active&(1<<i)!=0;
                let hover=look.hover==Some(i);
                let edge=match (look.dark,active,hover){
                    (true,true,_)=>[241,183,115],(false,true,_)=>[202,126,61],
                    (true,false,true)=>[182,161,145],(false,false,true)=>[173,141,112],
                    (true,false,false)=>[94,83,75],(false,false,false)=>[196,185,178],
                };
                rounded(data,w,h,b,radius,edge,255);
                let inner=Rect{x:b.x+1.0,y:b.y+1.0,w:b.w-2.0,h:b.h-2.0};
                if inner.w>0.0&&inner.h>0.0 {
                    rounded(data,w,h,inner,(radius-1.0).max(0.0),if look.dark{[47,41,39]}else{[244,239,235]},255);
                }
                // Recessed neutral well stays behind aspect-fit previews.
                let slot=layout.preview(i);
                let p=Rect{x:slot.left as f32,y:slot.top as f32,w:(slot.right-slot.left) as f32,h:(slot.bottom-slot.top) as f32};
                if p.w>0.0&&p.h>0.0 {
                    rounded(data,w,h,p,4.0,if look.dark{[32,29,27]}else{[224,218,213]},255);
                }
            }
            if let Some(nav)=layout.nav(){
                rounded(data,w,h,nav.rect(),nav.h as f32/2.0,if look.dark{[48,40,37]}else{[241,234,228]},255);
            }
            self.look=Some(look);
        }
        let data=self.layer.pixels();
        data.copy_from_slice(&self.static_pixels);
        let tint=if look.dark{[222,219,216]}else{[64,59,53]};
        for (i,item) in items.iter().enumerate(){
            let c=layout.card(i);
            let size=(layout.caption-5).max(1) as f32;
            let small=Rect{x:(c.x+layout.inset) as f32,y:(c.y+c.h) as f32-(layout.caption as f32+size)/2.0,w:size,h:size};
            let mut badge=|r|{
                if let Some(Some(icon))=atlas.get(&item.path){icon_on(data,w,h,r,icon);}
                else{fallback_icon(data,w,h,r,tint);}
            };
            badge(small);
            let slot=layout.preview(i);
            if !ready[i]&&slot.bottom-slot.top>6 {
                let s=((slot.bottom-slot.top-4) as f32).min(size*2.1);
                badge(Rect{x:(slot.left+slot.right) as f32/2.0-s/2.0,y:(slot.top+slot.bottom) as f32/2.0-s/2.0,w:s,h:s});
            }
        }
        self.layer.blit(target)
    }
}
impl Drop for Panel {
    fn drop(&mut self){if !self.font.is_null(){unsafe{DeleteObject(self.font);}}}
}
pub(super) unsafe fn caption(dc:HDC,layout:Layout,i:usize,text:&str,ink:u32){
    let c=layout.card(i);
    let mut r=RECT{left:c.x+layout.inset+layout.caption,top:c.y+c.h-layout.caption,
        right:c.x+c.w-layout.inset,bottom:c.y+c.h};
    if r.left>=r.right{return;}
    SetTextColor(dc,ink);SetBkMode(dc,TRANSPARENT as i32);
    DrawTextW(dc,wide(text).as_ptr(),-1,&mut r,DT_SINGLELINE|DT_END_ELLIPSIS|DT_VCENTER|DT_NOPREFIX);
}
pub(super) unsafe fn nav_label(dc:HDC,b:Box2,label:&str,ink:u32){
    let mut r=b.win();
    SetTextColor(dc,ink);SetBkMode(dc,TRANSPARENT as i32);
    DrawTextW(dc,wide(label).as_ptr(),-1,&mut r,DT_CENTER|DT_SINGLELINE|DT_VCENTER|DT_NOPREFIX);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn floating_layout_reserves_click_through_gaps_and_paging() {
        for dpi in [96,120,144,192] {
            let scale=dpi as i32/24;
            let mut l=Layout::new(40*scale,136*scale,dpi,2*scale,5,8,false);
            assert!(l.paged);
            l.count=l.rows;
            l.stack=1;
            let c=l.card(0);
            let b=l.card(1);
            let mut opposite=l;opposite.right=true;
            assert!(opposite.card(0).x>c.x);
            assert!(c.w>0&&c.h>0&&b.y>c.y+c.h);
            assert_eq!(l.hit(c.x,c.y+c.h/2),Some(0));
            assert_eq!(l.hit(c.x,c.y+c.h+1),None);
            assert_eq!(l.hit(c.x,c.y-1),None);
            let nav=l.nav().unwrap();
            assert!(nav.y+nav.h<=l.height);
            assert_eq!(l.hit(nav.x+nav.w/2,nav.y+nav.h/2),None);
        }
    }
    #[test]
    #[ignore = "native Win32 window region; explicitly run in CI"]
    fn native_region_clips_desktop_gaps_and_preserves_thumbnail_slots() {
        use std::ptr::null_mut;
        use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW,DestroyWindow,WS_POPUP};
        unsafe {
            let hwnd=CreateWindowExW(0,wide("STATIC").as_ptr(),null_mut(),WS_POPUP,0,0,164,400,
                null_mut(),null_mut(),null_mut(),null_mut());
            assert!(!hwnd.is_null());
            let mut l=Layout::new(164,400,96,10,3,8,false);
            l.count=3;l.stack=0b010;
            assert!(l.apply(hwnd));
            let rgn=CreateRectRgn(0,0,0,0);
            assert_ne!(GetWindowRgn(hwnd,rgn),0);
            for i in 0..l.count{
                let r=l.preview(i);
                assert_ne!(PtInRegion(rgn,r.left+1,r.top+1),0);
                assert_ne!(PtInRegion(rgn,r.right-2,r.bottom-2),0);
                let b=l.card(i);
                assert_eq!(PtInRegion(rgn,b.x+b.w/2,b.y+b.h+2),0);
            }
            assert_eq!(PtInRegion(rgn,0,0),0);
            let nav=l.nav().unwrap();
            assert_ne!(PtInRegion(rgn,nav.x+nav.w/2,nav.y+nav.h/2),0);
            DeleteObject(rgn);
            DestroyWindow(hwnd);
        }
    }
}
