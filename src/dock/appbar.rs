//! Cooperative work-area reservations. The shell owns the work area; the
//! hidden registration HWND covers its full strip while the UI stays compact.
use super::{DockSide, Display, same_rect};
use std::{mem,ptr};
use windows_sys::Win32::{Foundation::*,UI::{Shell::*,WindowsAndMessaging::*}};

pub const CALLBACK:u32=WM_APP+54;

#[derive(Clone)]
pub struct Request {
    pub display:String,
    pub stage:bool,
    pub monitor:RECT,
    pub side:DockSide,
    pub depth:i32,
}
impl Request {
    fn edge(&self)->u32 { match self.side {DockSide::Bottom=>ABE_BOTTOM,DockSide::Left=>ABE_LEFT,DockSide::Right=>ABE_RIGHT} }
    fn matches(&self,other:&Self)->bool {
        self.display==other.display&&self.stage==other.stage&&self.side==other.side&&self.depth==other.depth&&same_rect(self.monitor,other.monitor)
    }
}

fn trim(mut rect:RECT,side:DockSide,depth:i32)->Option<RECT>{
    if rect.right<=rect.left || rect.bottom<=rect.top{return None;}
    match side{
        DockSide::Bottom=>{if rect.bottom-rect.top<=depth{return None;}rect.top=rect.bottom-depth;}
        DockSide::Left=>{if rect.right-rect.left<=depth{return None;}rect.right=rect.left+depth;}
        DockSide::Right=>{if rect.right-rect.left<=depth{return None;}rect.left=rect.right-depth;}
    }
    Some(rect)
}

struct Bar{window:HWND,request:Request,position:Option<RECT>}
impl Bar {
    unsafe fn create(request:Request)->Option<Self>{
        let m=request.monitor;
        let window=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,
            super::wide(super::CLASS).as_ptr(),ptr::null(),WS_POPUP,
            m.left,m.top,1,1,ptr::null_mut(),ptr::null_mut(),super::GetModuleHandleW(ptr::null()),ptr::null());
        if window.is_null(){return None;}
        let bar=Self{window,request,position:None};
        let mut data=bar.data();
        data.uCallbackMessage=CALLBACK;
        if SHAppBarMessage(ABM_NEW,&mut data)==0{return None;}
        Some(bar)
    }
    fn data(&self)->APPBARDATA {
        APPBARDATA{cbSize:mem::size_of::<APPBARDATA>() as u32,hWnd:self.window,
            uCallbackMessage:0,uEdge:self.request.edge(),rc:self.request.monitor,lParam:0}
    }
    unsafe fn arrange(&mut self)->bool {
        let mut data=self.data();
        if SHAppBarMessage(ABM_QUERYPOS,&mut data)==0{return false;}
        let Some(rect)=trim(data.rc,self.request.side,self.request.depth) else{return false;};
        if self.position.is_some_and(|old|same_rect(old,rect)){return true;}
        data.rc=rect;
        if SHAppBarMessage(ABM_SETPOS,&mut data)==0{return false;}
        let r=data.rc;
        if r.right<=r.left||r.bottom<=r.top{return false;}
        if SetWindowPos(self.window,ptr::null_mut(),r.left,r.top,r.right-r.left,r.bottom-r.top,SWP_NOACTIVATE|SWP_NOZORDER)==0{return false;}
        self.position=Some(r);
        true
    }
}
impl Drop for Bar {
    fn drop(&mut self){unsafe{let mut data=self.data();SHAppBarMessage(ABM_REMOVE,&mut data);DestroyWindow(self.window);}}
}
#[derive(Default)]
pub struct Bars{bars:Vec<Bar>,requested:Vec<Request>}
impl Bars {
    pub fn clear(&mut self){self.bars.clear();self.requested.clear();}
    /// Preserve order for adjacent strips. Recreate only on configuration /
    /// topology change or an Explorer restart; ordinary AppBar notifications
    /// re-negotiate only where the negotiated rectangle actually changed.
    pub unsafe fn sync(&mut self,requests:Vec<Request>,notify:bool)->bool {
        let changed=requests.len()!=self.requested.len()||!requests.iter().zip(&self.requested).all(|(a,b)|a.matches(b));
        if changed {
            self.bars.clear();
            self.requested=requests;
        }
        if !changed&&!notify{return true;}
        let mut ok=true;
        for request in &self.requested {
            if self.bars.iter().any(|b|b.request.matches(request)){continue;}
            if let Some(mut bar)=Bar::create(request.clone()){
                if bar.arrange(){self.bars.push(bar);}else{ok=false;}
            }else{ok=false;}
        }
        self.bars.retain_mut(|bar|{
            let arranged=bar.arrange();ok&=arranged;arranged
        });
        ok
    }
    pub fn placement(&self,display:&Display,stage:bool)->(Display,Option<i32>){
        let mut result=display.clone();
        let Some(bar)=self.bars.iter().find(|b|b.request.display==display.id&&b.request.stage==stage) else{return (result,None);};
        let Some(r)=bar.position else{return (result,None);};
        // Use the shell strip cross axis and the final work area along its
        // length. This avoids centering against our own reserved work area
        // and prevents Stage and Dock intersecting if they share an edge.
        match bar.request.side {
            DockSide::Bottom=>{result.work.bottom=r.bottom;}
            DockSide::Left=>{result.work.left=r.left;}
            DockSide::Right=>{result.work.right=r.right;}
        }
        (result,Some(bar.request.depth))
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn shell_strip_keeps_negative_coordinates_and_the_taskbar_inset(){
        let available=RECT{left:-1080,top:-731,right:0,bottom:1141};
        assert!(same_rect(trim(available,DockSide::Bottom,80).unwrap(),RECT{top:1061,..available}));
        assert!(same_rect(trim(available,DockSide::Left,132).unwrap(),RECT{right:-948,..available}));
        assert!(same_rect(trim(available,DockSide::Right,132).unwrap(),RECT{left:-132,..available}));
        assert!(trim(available,DockSide::Left,1080).is_none());
    }
}
