//! Window previews use compositor relationships, not repeated screen
//! capture. Every relationship is released on hide, detach, or teardown.
use super::{Item,same_rect};
use std::{collections::HashMap,mem};
use windows_sys::Win32::{Foundation::*,Graphics::Dwm::*,UI::WindowsAndMessaging::*};

pub struct Link { raw:isize, pid:u32, area:RECT }
impl Drop for Link {fn drop(&mut self){unsafe{DwmUnregisterThumbnail(self.raw);}}}

#[derive(Default)]
pub struct Previews { links:HashMap<usize,Link> }
impl Previews {
    pub fn clear(&mut self)->bool{
        let changed=!self.links.is_empty();self.links.clear();changed
    }
    pub fn includes(&self,hwnd:HWND)->bool{self.links.contains_key(&(hwnd as usize))}
    pub unsafe fn update(&mut self,dst:HWND,items:&[Item],width:i32,height:i32,gap:i32)->bool{
        let mut next=HashMap::new();
        let mut changed=false;
        for (i,item) in items.iter().enumerate(){
            let source=item.hwnd;
            let Some(pid)=preview_pid(source) else{continue;};
            let key=source as usize;
            let region=tile(width,height,gap,i);
            let slot=RECT{left:region.left+5,top:region.top+5,right:region.right-5,bottom:region.bottom-22};
            if slot.right<=slot.left||slot.bottom<=slot.top{continue;}
            let existing=self.links.remove(&key);
            let link=match existing{
                Some(link) if link.pid==pid=>link,
                old=>{
                    drop(old);
                    let mut raw=0isize;
                    if DwmRegisterThumbnail(dst,source,&mut raw)<0||raw==0{continue;}
                    changed=true;
                    Link{raw,pid,area:mem::zeroed()}
                }
            };
            let mut size:SIZE=mem::zeroed();
            if DwmQueryThumbnailSourceSize(link.raw,&mut size)<0{changed=true;continue;}
            let Some(area)=fit(slot,size) else{changed=true;continue;};
            let mut link=link;
            if !same_rect(link.area,area){
                let properties=DWM_THUMBNAIL_PROPERTIES {
                    dwFlags:DWM_TNP_RECTDESTINATION|DWM_TNP_VISIBLE|DWM_TNP_OPACITY,
                    rcDestination:area,rcSource:mem::zeroed(),opacity:255,fVisible:1,fSourceClientAreaOnly:0,
                };
                if DwmUpdateThumbnailProperties(link.raw,&properties)<0{changed=true;continue;}
                link.area=area;
                changed=true;
            }
            next.insert(key,link);
        }
        changed|=!self.links.is_empty();
        self.links=next;
        changed
    }
}

unsafe fn preview_pid(window:HWND)->Option<u32>{
    if window.is_null()||IsWindow(window)==0||IsWindowVisible(window)==0||IsIconic(window)!=0{return None;}
    let mut pid=0;
    GetWindowThreadProcessId(window,&mut pid);
    if pid==0||pid==std::process::id(){return None;}
    let mut cloaked=0u32;
    if DwmGetWindowAttribute(window,DWMWA_CLOAKED as u32,(&mut cloaked as *mut u32).cast(),mem::size_of::<u32>() as u32)<0||cloaked!=0{
        return None;
    }
    let mut affinity=u32::MAX;
    // If policy cannot be verified, use the app title fallback. Never retry
    // with PrintWindow, screen capture, or a saved image for restricted windows.
    if GetWindowDisplayAffinity(window,&mut affinity)==0||affinity!=WDA_NONE{return None;}
    Some(pid)
}

pub fn tile(width:i32,height:i32,gap:i32,i:usize)->RECT{
    let step=((height-28)/5).clamp(28,126);
    let top=31+i as i32*step;
    RECT{left:7,top,right:width-7,bottom:(top+step-gap.min((step/4).max(2))).min(height-4)}
}
pub fn slot_at(width:i32,height:i32,x:i32,y:i32,items:usize,gap:i32)->Option<usize>{
    (0..items).find(|&i|{let r=tile(width,height,gap,i);x>=r.left&&x<r.right&&y>=r.top&&y<r.bottom})
}
fn fit(slot:RECT,source:SIZE)->Option<RECT>{
    if source.cx<=0||source.cy<=0{return None;}
    let w=slot.right-slot.left;let h=slot.bottom-slot.top;
    if w<1||h<1{return None;}
    let factor=(w as f32/source.cx as f32).min(h as f32/source.cy as f32);
    let tw=((source.cx as f32*factor).round() as i32).clamp(1,w);
    let th=((source.cy as f32*factor).round() as i32).clamp(1,h);
    let x=slot.left+(w-tw)/2;let y=slot.top+(h-th)/2;
    Some(RECT{left:x,top:y,right:x+tw,bottom:y+th})
}

pub unsafe fn style_destination(window:HWND){
    let rounded=DWMWCP_ROUNDSMALL;
    DwmSetWindowAttribute(window,DWMWA_WINDOW_CORNER_PREFERENCE as u32,(&rounded as *const i32).cast(),mem::size_of::<i32>() as u32);
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn thumbnail_fits_its_tile_without_covering_labels_or_hit_areas(){
        let r=tile(164,590,6,2);
        let area=RECT{left:r.left+5,top:r.top+5,right:r.right-5,bottom:r.bottom-22};
        for source in [SIZE{cx:1920,cy:1080},SIZE{cx:1080,cy:1920},SIZE{cx:20000,cy:20}]{
            let dest=fit(area,source).unwrap();
            assert!(dest.left>=area.left&&dest.right<=area.right&&dest.top>=area.top&&dest.bottom<=area.bottom);
            assert_eq!(slot_at(164,590,dest.left,dest.top,5,6),Some(2));
        }
        assert!(fit(area,SIZE{cx:0,cy:120}).is_none());
    }
}
