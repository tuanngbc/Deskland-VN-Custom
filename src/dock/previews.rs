//! Window previews use compositor relationships, not repeated screen
//! capture. Every relationship is released on hide, detach, or teardown.
use super::{Item,same_rect,stage_surface::Layout};
use std::{collections::HashMap,mem};
use windows_sys::Win32::{Foundation::*,Graphics::{Dwm::*,Gdi::*},UI::WindowsAndMessaging::*};

pub struct Link { raw:isize, pid:u32, area:RECT, crop:Option<RECT> }
impl Drop for Link {fn drop(&mut self){unsafe{DwmUnregisterThumbnail(self.raw);}}}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewStep {
    NotRun, Duplicate, Gone, Hidden, Minimized, ForeignIdentity, CloakQuery(i32), Cloaked,
    AffinityQuery(u32), Protected, EmptySlot, Register(i32), NullLink, SourceSize(i32),
    EmptySource, Update(i32), Linked,
}

#[derive(Default)]
pub struct Previews { links:HashMap<usize,Link>, next:HashMap<usize,Link>, destination:usize }
impl Previews {
    pub fn clear(&mut self)->bool{
        let changed=!self.links.is_empty()||!self.next.is_empty();
        self.links.clear();self.next.clear();self.destination=0;
        changed
    }
    pub fn areas(&self)->Vec<RECT>{
        let mut areas=self.links.values().map(|link|link.area).collect::<Vec<_>>();
        areas.sort_unstable_by_key(|r|(r.top,r.left,r.bottom,r.right));
        areas
    }
    pub fn includes(&self,hwnd:HWND)->bool{self.links.contains_key(&(hwnd as usize))}
    pub fn benchmark_counts(&self, pid: u32, anchor: usize) -> (usize, usize) {
        let fixture = self.links.iter().filter(|&(&source, link)| source != anchor && link.pid == pid).count();
        (fixture, self.links.len() - fixture)
    }
    #[cfg(test)]
    pub unsafe fn update(&mut self,dst:HWND,items:&[Item],layout:Layout)->bool{
        self.update_traced(dst,items,layout,None)
    }
    pub unsafe fn update_traced(&mut self,dst:HWND,items:&[Item],layout:Layout,mut steps:Option<&mut [PreviewStep]>)->bool{
        self.update_targets(dst,items.iter().enumerate().map(|(i,item)|(item.hwnd,layout.preview(i),None)),steps.as_deref_mut())
    }
    pub unsafe fn update_stacks(&mut self,dst:HWND,groups:&[Vec<usize>],layout:Layout)->bool{
        let targets=groups.iter().enumerate().flat_map(|(group,members)|members.iter().take(layout.depth(group))
            .enumerate().map(move |(depth,&handle)|{
                let (area,clip)=layout.preview_layer(group,depth);
                (handle as HWND,area,clip)
            }));
        self.update_targets(dst,targets,None)
    }
    unsafe fn update_targets(&mut self,dst:HWND,targets:impl Iterator<Item=(HWND,RECT,Option<RECT>)>,mut steps:Option<&mut [PreviewStep]>)->bool{
        // A thumbnail is bound to its original DWM destination. Changing
        // hosts must not reuse registrations even if the source list matches.
        let mut changed=self.destination!=dst as usize&&self.clear();
        self.destination=dst as usize;
        for (i,(source,slot,clip)) in targets.enumerate(){
            let key=source as usize;
            let mut record=|step|{if let Some(entry)=steps.as_deref_mut().and_then(|entries|entries.get_mut(i)){*entry=step;}};
            if self.next.contains_key(&key){record(PreviewStep::Duplicate);continue;}
            let pid=match preview_pid(source){Ok(pid)=>pid,Err(step)=>{record(step);continue;}};
            if slot.right<=slot.left||slot.bottom<=slot.top{record(PreviewStep::EmptySlot);continue;}
            let existing=self.links.remove(&key);
            let link=match existing{
                Some(link) if link.pid==pid=>link,
                old=>{
                    drop(old);
                    let mut raw=0isize;
                    let result=DwmRegisterThumbnail(dst,source,&mut raw);
                    if result<0{record(PreviewStep::Register(result));continue;}
                    if raw==0{record(PreviewStep::NullLink);continue;}
                    changed=true;
                    Link{raw,pid,area:mem::zeroed(),crop:None}
                }
            };
            let mut size:SIZE=mem::zeroed();
            let result=DwmQueryThumbnailSourceSize(link.raw,&mut size);
            if result<0{record(PreviewStep::SourceSize(result));changed=true;continue;}
            let Some((area,crop))=fit_layer(slot,size,clip) else{record(PreviewStep::EmptySource);changed=true;continue;};
            let mut link=link;
            if !same_rect(link.area,area)||!same_crop(link.crop,crop){
                let properties=DWM_THUMBNAIL_PROPERTIES {
                    dwFlags:DWM_TNP_RECTDESTINATION|DWM_TNP_VISIBLE|DWM_TNP_OPACITY|if crop.is_some()||link.crop.is_some(){DWM_TNP_RECTSOURCE}else{0},
                    rcDestination:area,rcSource:crop.unwrap_or_else(||RECT{left:0,top:0,right:size.cx,bottom:size.cy}),opacity:255,fVisible:1,fSourceClientAreaOnly:0,
                };
                let result=DwmUpdateThumbnailProperties(link.raw,&properties);
                if result<0{record(PreviewStep::Update(result));changed=true;continue;}
                link.area=area;link.crop=crop;
                changed=true;
            }
            self.next.insert(key,link);
            record(PreviewStep::Linked);
        }
        changed|=!self.links.is_empty();
        // Retain the capacity of the two small live-card maps across window
        // events. Clearing the old entries still unregisters stale links.
        self.links.clear();
        std::mem::swap(&mut self.links,&mut self.next);
        changed
    }
}

pub(super) unsafe fn preview_pid(window:HWND)->Result<u32,PreviewStep>{
    if window.is_null()||IsWindow(window)==0{return Err(PreviewStep::Gone);}
    if IsWindowVisible(window)==0{return Err(PreviewStep::Hidden);}
    if IsIconic(window)!=0{return Err(PreviewStep::Minimized);}
    let mut pid=0;
    GetWindowThreadProcessId(window,&mut pid);
    if pid==0||pid==std::process::id(){return Err(PreviewStep::ForeignIdentity);}
    let mut cloaked=0u32;
    let result=DwmGetWindowAttribute(window,DWMWA_CLOAKED as u32,(&mut cloaked as *mut u32).cast(),mem::size_of::<u32>() as u32);
    if result<0{return Err(PreviewStep::CloakQuery(result));}
    if cloaked!=0{return Err(PreviewStep::Cloaked);}
    let mut affinity=u32::MAX;
    // If policy cannot be verified, use the app title fallback. Never retry
    // with PrintWindow, screen capture, or a saved image for restricted windows.
    if GetWindowDisplayAffinity(window,&mut affinity)==0{return Err(PreviewStep::AffinityQuery(GetLastError()));}
    if affinity!=WDA_NONE{return Err(PreviewStep::Protected);}
    Ok(pid)
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

fn same_crop(a:Option<RECT>,b:Option<RECT>)->bool{
    match (a,b){(None,None)=>true,(Some(a),Some(b))=>same_rect(a,b),_=>false}
}
fn fit_layer(slot:RECT,source:SIZE,clip:Option<RECT>)->Option<(RECT,Option<RECT>)>{
    let mut area=fit(slot,source)?;
    let Some(clip)=clip else{return Some((area,None));};
    let w=area.right-area.left;let h=area.bottom-area.top;
    // Aspect-fit first and align to the visible edge. Crop source pixels
    // rather than shrinking/stretched drawing into the exposed slice.
    let x=if clip.left<=slot.left{slot.left}else{slot.right-w};
    area.left=x;area.right=x+w;
    let visible=RECT{left:area.left.max(clip.left),top:area.top.max(clip.top),
        right:area.right.min(clip.right),bottom:area.bottom.min(clip.bottom)};
    if visible.left>=visible.right||visible.top>=visible.bottom{return None;}
    let sample=|p:i32,origin:i32,extent:i32,pixels:i32|((i64::from(p-origin)*i64::from(pixels))/i64::from(extent)) as i32;
    let left=sample(visible.left,area.left,w,source.cx);
    let top=sample(visible.top,area.top,h,source.cy);
    let right=sample(visible.right,area.left,w,source.cx).max(left+1).min(source.cx);
    let bottom=sample(visible.bottom,area.top,h,source.cy).max(top+1).min(source.cy);
    Some((visible,Some(RECT{left,top,right,bottom})))
}

// This non-layered DWM destination is visible only where a registered
// thumbnail is drawn. The layered Stage surface under it draws the cards.
pub unsafe fn clip_destination(window:HWND,areas:&[RECT])->bool{
    let combined=CreateRectRgn(0,0,0,0);
    if combined.is_null(){return false;}
    for a in areas {
        let tile=CreateRectRgn(a.left,a.top,a.right,a.bottom);
        if tile.is_null(){DeleteObject(combined);return false;}
        let ok=CombineRgn(combined,combined,tile,RGN_OR)!=0;
        DeleteObject(tile);
        if !ok{DeleteObject(combined);return false;}
    }
    if SetWindowRgn(window,combined,1)!=0{true}else{DeleteObject(combined);false}
}

pub unsafe extern "system" fn destination_proc(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT {
    match msg {
        WM_NCHITTEST=>HTTRANSPARENT as LRESULT,
        WM_MOUSEACTIVATE=>MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND=>1,
        _=>DefWindowProcW(hwnd,msg,w,l),
    }
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
        let slot=Layout::new(164,590,96,6,5,5,false).preview(2);
        let area=slot;
        for source in [SIZE{cx:1920,cy:1080},SIZE{cx:1080,cy:1920},SIZE{cx:20000,cy:20}]{
            let dest=fit(area,source).unwrap();
            assert!(dest.left>=area.left&&dest.right<=area.right&&dest.top>=area.top&&dest.bottom<=area.bottom);
        }
        assert!(fit(area,SIZE{cx:0,cy:120}).is_none());
    }

}

#[cfg(test)]
#[path = "preview_tests.rs"]
mod native_tests;
