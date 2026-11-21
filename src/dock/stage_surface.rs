//! Opaque Stage panel artwork. Its HWND stays a regular DWM thumbnail
//! destination; only small static card backplates are rendered here.
use super::{geometry::Rect,glass_probe::Layer,previews,surface::rounded};
use crate::desktop::wide;
use windows_sys::Win32::Graphics::Gdi::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Look {
    pub dark:bool,
    pub gap:i32,
    pub count:usize,
    pub active:u8,
    pub stacked:u8,
}
pub(super) struct Panel {
    layer:Layer,
    look:Option<Look>,
    font:HFONT,
    dpi:u32,
}
impl Panel {
    pub unsafe fn new(w:i32,h:i32,dpi:u32)->Option<Self>{
        let layer=Layer::new(w,h)?;
        let font_height=((13*dpi.clamp(72,288)+48)/96).min(22) as i32;
        let font=CreateFontW(-font_height,0,0,0,500,0,0,0,DEFAULT_CHARSET as u32,0,0,
            CLEARTYPE_QUALITY as u32,DEFAULT_PITCH as u32,wide("Segoe UI Variable").as_ptr());
        Some(Self{layer,look:None,font,dpi})
    }
    pub fn geometry(&self)->(i32,i32,u32){(self.layer.size.0,self.layer.size.1,self.dpi)}
    pub fn font(&self)->HFONT{self.font}
    pub unsafe fn paint(&mut self,target:HDC,look:Look)->bool {
        if self.look!=Some(look){
            let (w,h)=self.layer.size;
            let data=self.layer.pixels();
            for y in 0..h as usize {
                let tone=(8*((h as usize-y).min(192))/192) as u8;
                let pixel=if look.dark{[26+tone,21+tone,17+tone,255]}else{[243+tone,240+tone,235+tone,255]};
                for p in data[y*w as usize*4..(y+1)*w as usize*4].chunks_exact_mut(4){p.copy_from_slice(&pixel);}
            }
            for i in 0..look.count {
                let area=previews::tile(w,h,look.gap,i);
                let height=area.bottom-area.top;
                if height<22{continue;}
                let active=look.active&(1<<i)!=0;
                let stack=if look.stacked&(1<<i)!=0{3.0}else{0.0};
                let tile=Rect{x:area.left as f32,y:area.top as f32,w:(area.right-area.left) as f32,h:height as f32-stack};
                let shadow=Rect{x:tile.x+1.0,y:tile.y+2.5,w:tile.w-2.0,h:tile.h+stack};
                rounded(data,w,h,shadow,10.0,[10,8,6],55);
                let (edge,fill)=match (look.dark,active){
                    (true,true)=>([231,162,107],[81,57,42]),
                    (true,false)=>([74,64,61],[47,39,37]),
                    (false,true)=>([201,126,66],[255,240,220]),
                    (false,false)=>([220,214,211],[253,252,250]),
                };
                rounded(data,w,h,tile,10.0,edge,255);
                let inset=Rect{x:tile.x+1.0,y:tile.y+1.0,w:tile.w-2.0,h:tile.h-2.0};
                rounded(data,w,h,inset,9.0,fill,255);
            }
            self.look=Some(look);
        }
        self.layer.blit(target)
    }
}
impl Drop for Panel {
    fn drop(&mut self){if !self.font.is_null(){unsafe{DeleteObject(self.font);}}}
}
