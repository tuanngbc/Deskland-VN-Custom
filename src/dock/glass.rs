//! Snapshot frost shared with Spotlight. Geometry/focus changes invalidate
//! persistent surfaces; hovering and idle animation never sample the screen.
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::*,System::Threading::GetCurrentProcessId,UI::WindowsAndMessaging::*};
use std::ptr;
use crate::launcher::{capture_glass,frost,Appearance};

pub(crate) struct Glass{
    pixels:Vec<u8>, width:i32, height:i32, key:([i32;4],u32,bool,u32,usize),
}
impl Glass{
    pub(crate) unsafe fn capture(rect:RECT,dpi:u32,dark:bool,transparency:u32)->Self{
        Self::make(rect,dpi,dark,transparency,0)
    }
    unsafe fn make(rect:RECT,dpi:u32,dark:bool,transparency:u32,focus:usize)->Self{
        let (w,h)=(rect.right-rect.left,rect.bottom-rect.top);
        let style=Appearance{theme:if dark{"dark"}else{"light"}.into(),
            transparency:(0.24+transparency.min(85) as f32*0.004).min(if dark{0.41}else{0.53}),blur:20.0};
        let pixels=if w>0&&h>0&&i64::from(w)*i64::from(h)<=8_000_000{
            let raw=capture_glass(rect.left,rect.top,w,h);
            frost(&raw,w,h,dpi as f32/96.0,&style)
        }else{Vec::new()};
        Self{pixels,width:(w/4).max(1),height:(h/4).max(1),key:([rect.left,rect.top,rect.right,rect.bottom],dpi,dark,transparency,focus)}
    }
    pub(crate) unsafe fn refresh(slot:&mut Option<Self>,rect:RECT,dpi:u32,dark:bool,transparency:u32)->bool{
        let fg=GetForegroundWindow();
        let mut pid=0;GetWindowThreadProcessId(fg,&mut pid);
        let focus=if pid==GetCurrentProcessId(){slot.as_ref().map_or(0,|g|g.key.4)}else{fg as usize};
        let key=([rect.left,rect.top,rect.right,rect.bottom],dpi,dark,transparency,focus);
        if slot.as_ref().is_some_and(|g|g.key==key){return false;}
        *slot=Some(Self::make(rect,dpi,dark,transparency,focus));
        true
    }
    pub(crate) fn is_ready(&self)->bool{!self.pixels.is_empty()}
    pub(crate) unsafe fn draw(&self,dc:HDC,w:i32,h:i32)->bool{
        if !self.is_ready(){return false;}
        let mut info:BITMAPINFO=std::mem::zeroed();
        info.bmiHeader.biSize=std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth=self.width;info.bmiHeader.biHeight=-self.height;
        info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;
        SetStretchBltMode(dc,HALFTONE);
        SetBrushOrgEx(dc,0,0,ptr::null_mut());
        StretchDIBits(dc,0,0,w,h,0,0,self.width,self.height,self.pixels.as_ptr().cast(),&info,DIB_RGB_COLORS,SRCCOPY)!=0
    }
    pub(crate) fn tint(&self,x:f32,y:f32,base:[u8;3],strength:u8)->[u8;3]{
        if !self.is_ready(){return base;}
        let x=((x as i32)/4).clamp(0,self.width-1);
        let y=((y as i32)/4).clamp(0,self.height-1);
        let at=((y*self.width+x)*4) as usize;
        let k=u16::from(strength);
        std::array::from_fn(|c|((u16::from(base[c])*(255-k)+u16::from(self.pixels[at+c])*k+127)/255) as u8)
    }
}
