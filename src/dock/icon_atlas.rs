//! Dock icon cache fed by a single cancellable Windows thread pool request.
//! There is no private worker thread to start and stop during module toggles.
use std::{
    collections::{HashMap,HashSet},
    ffi::c_void,
    path::Path,
    ptr,
    sync::{Arc,atomic::{AtomicBool,Ordering},mpsc::{self,Receiver,Sender}},
};
use crate::launcher::{shell_icon,IconPixels};
use super::{Item,WM_ICON_CHANGED,surface::Atlas};
use windows_sys::Win32::{
    System::Threading::{TrySubmitThreadpoolCallback,PTP_CALLBACK_INSTANCE},
    UI::WindowsAndMessaging::PostMessageW,
};

type ResultIcon=Option<Arc<IconPixels>>;
const CAP:usize=48;
const SOURCE_BUDGET:usize=5*1024*1024;
/// Request a source comfortably above the largest physical icon, with stable
/// buckets so fractional motion doesn't churn extraction or Shell caches.
pub fn source_request(pixel_edge:f32)->i32{
    let target=(pixel_edge*1.5).ceil() as i32;
    [96,128,192,256].into_iter().find(|size|*size>=target).unwrap_or(256)
}
fn bytes(icon:&ResultIcon)->usize{icon.as_ref().map_or(0,|i|i.pixels.len())}
// Bound across old and new Dock instances. An unresponsive Shell provider can
// delay new icons, but cannot leave a thread or queue for each toggle.
static EXTRACTION_ACTIVE:AtomicBool=AtomicBool::new(false);
struct Permit;
impl Drop for Permit {
    fn drop(&mut self){EXTRACTION_ACTIVE.store(false,Ordering::Release);}
}
struct Job {
    path:String,
    source_size:i32,
    revision:u64,
    result:Sender<(String,i32,u64,ResultIcon)>,
    cancelled:Arc<AtomicBool>,
    control:usize,
    permit:Permit,
}
unsafe extern "system" fn extract(_:PTP_CALLBACK_INSTANCE,raw:*mut c_void) {
    use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_MULTITHREADED};
    let Job{path,source_size,revision,result,cancelled,control,permit}=*Box::from_raw(raw.cast());
    if cancelled.load(Ordering::Acquire){return;}
    let icon=if path.starts_with(r"\\")&&!path.starts_with(r"\\?\C:\") {
        None
    } else {
        let com=CoInitializeEx(None,COINIT_MULTITHREADED);
        if com.is_ok() {
            let command=super::command_pin::decode(&path);
            let own=(path==super::internal_app::KEY).then(||std::env::current_exe().ok()).flatten();
            let file=own.as_deref().unwrap_or_else(||Path::new(command.as_ref().map_or(path.as_str(),|c|c.exe.as_str())));
            let icon=crate::app_icons::load(Path::new(&path)).or_else(||shell_icon(file,source_size)).map(Arc::new);
            CoUninitialize();
            icon
        }else{None}
    };
    // Release before notifying the UI so its next paint can submit another icon.
    drop(permit);
    if !cancelled.load(Ordering::Acquire)&&result.send((path,source_size,revision,icon)).is_ok(){
        PostMessageW(control as _,WM_ICON_CHANGED,0,0);
    }
}

pub struct Worker {
    result:Sender<(String,i32,u64,ResultIcon)>,
    done:Receiver<(String,i32,u64,ResultIcon)>,
    revision:u64,
    cache:HashMap<String,(ResultIcon,i32,u64)>,
    source_size:i32,
    pending:bool,
    budget_suppressed:HashSet<String>,
    visible:HashSet<String>,
    clock:u64,
    cancelled:Arc<AtomicBool>,
    control:usize,
}
impl Worker {
    pub fn new(control:*mut c_void)->Self{
        let (result,done)=mpsc::channel();
        Self{result,done,revision:crate::app_icons::revision(),cache:HashMap::new(),source_size:96,pending:false,budget_suppressed:HashSet::new(),visible:HashSet::new(),clock:0,
            cancelled:Arc::new(AtomicBool::new(false)),control:control as usize}
    }
    pub fn restrict_visible<'a>(&mut self,items:impl Iterator<Item=&'a Item>,source_size:i32){
        // The caller supplies Dock items before Stage items. Count distinct
        // paths so mirrored outputs don't consume the whole allowance.
        let mut next=HashSet::new();
        for path in items.map(|item|item.path.as_str()).filter(|path|!path.is_empty()){
            next.insert(path);
            if next.len()==CAP{break;}
        }
        let changed=next.len()!=self.visible.len()||next.iter().any(|path|!self.visible.contains(*path));
        if changed||source_size!=self.source_size{
            for path in self.budget_suppressed.drain(){self.cache.remove(&path);}
        }
        self.source_size=source_size;
        if changed{self.visible=next.into_iter().map(str::to_owned).collect();}
    }
    pub fn receive(&mut self)->bool{
        let Ok((path,size,revision,icon))=self.done.try_recv()else{return false;};
        self.pending=false;
        if revision!=crate::app_icons::revision(){return true;}
        if !self.visible.contains(&path){return true;}
        let entry=self.cache.entry(path).or_insert((None,0,0));
        // A provider may fail a higher-resolution request. Keep the previous
        // source and remember the attempt to avoid a repaint/extraction loop.
        if icon.is_some(){entry.0=icon;}
        entry.1=size;
        entry.2=self.clock;
        let mut memory=self.cache.values().map(|(icon,_,_)|bytes(icon)).sum::<usize>();
        // Retain negative responses (bounded by count) so a failed Shell
        // extraction does not keep the Dock awake. Never evict visible icons
        // solely to re-request them on the very next paint.
        while self.cache.len()>CAP || memory>SOURCE_BUDGET {
            let oldest=self.cache.iter().filter(|(id,_)|!self.visible.contains(*id))
                .min_by_key(|(_,(_,_,at))|*at).map(|(id,_)|id.clone());
            let Some(oldest)=oldest else{
                // Keep existing rasters as a quality fallback during load.
                // A source beyond the shared budget is suppressed until the
                // visible set or size tier changes.
                if let Some((id,entry))=self.cache.iter_mut()
                    .filter(|(_,e)|bytes(&e.0)>0)
                    .min_by_key(|(_,(_,_,at))|*at) {
                    memory=memory.saturating_sub(bytes(&entry.0));entry.0=None;self.budget_suppressed.insert(id.clone());
                    continue;
                }
                break;
            };
            if let Some((old,_,_))=self.cache.remove(&oldest){memory=memory.saturating_sub(bytes(&old));}
        }
        true
    }
    pub fn request(&mut self,items:&[Item])->Atlas{
        let revision=crate::app_icons::revision();
        if self.revision!=revision{self.cache.clear();self.budget_suppressed.clear();self.revision=revision;}
        self.clock=self.clock.wrapping_add(1);
        let mut visible=Atlas::new();
        for item in items{
            let current=self.cache.get_mut(&item.path);
            let ready=if let Some((icon,size,at))=current{
                *at=self.clock;visible.insert(item.path.clone(),icon.clone());
                *size>=self.source_size||self.budget_suppressed.contains(&item.path)
            }else{false};
            if !ready&&self.visible.contains(&item.path)&&!self.pending&&EXTRACTION_ACTIVE.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_ok(){
                let raw=Box::into_raw(Box::new(Job{path:item.path.clone(),source_size:self.source_size,revision,result:self.result.clone(),
                    cancelled:self.cancelled.clone(),control:self.control,permit:Permit}));
                unsafe{
                    if TrySubmitThreadpoolCallback(Some(extract),raw.cast(),ptr::null())!=0{
                        self.pending=true;
                    }else{
                        drop(Box::from_raw(raw));
                    }
                }
            }
        }
        visible
    }
}
impl Drop for Worker {
    fn drop(&mut self){self.cancelled.store(true,Ordering::Release);}
}


#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn physical_icon_size_selects_a_stable_source_tier(){
        assert_eq!([51.0,64.0,76.0,102.0].map(source_request),[96,96,128,192]);
        assert_eq!(source_request(234.0),256);
    }
}
