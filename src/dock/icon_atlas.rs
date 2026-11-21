//! Dock icon cache fed by a single cancellable Windows thread pool request.
//! There is no private worker thread to start and stop during module toggles.
use std::{
    collections::HashMap,
    ffi::c_void,
    path::Path,
    ptr,
    sync::{Arc,atomic::{AtomicBool,Ordering},mpsc::{self,Receiver,Sender}},
};
use crate::launcher::{extract_icon,IconPixels};
use super::{Item,WM_ICON_CHANGED,surface::Atlas};
use windows_sys::Win32::{
    System::Threading::{TrySubmitThreadpoolCallback,PTP_CALLBACK_INSTANCE},
    UI::WindowsAndMessaging::PostMessageW,
};

type ResultIcon=Option<Arc<IconPixels>>;
const CAP:usize=48;
// Bound across old and new Dock instances. An unresponsive Shell provider can
// delay new icons, but cannot leave a thread or queue for each toggle.
static EXTRACTION_ACTIVE:AtomicBool=AtomicBool::new(false);
struct Permit;
impl Drop for Permit {
    fn drop(&mut self){EXTRACTION_ACTIVE.store(false,Ordering::Release);}
}
struct Job {
    path:String,
    result:Sender<(String,ResultIcon)>,
    cancelled:Arc<AtomicBool>,
    control:usize,
    permit:Permit,
}
unsafe extern "system" fn extract(_:PTP_CALLBACK_INSTANCE,raw:*mut c_void) {
    use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_MULTITHREADED};
    let Job{path,result,cancelled,control,permit}=*Box::from_raw(raw.cast());
    if cancelled.load(Ordering::Acquire){return;}
    let icon=if path.starts_with(r"\\")&&!path.starts_with(r"\\?\C:\") {
        None
    } else {
        let com=CoInitializeEx(None,COINIT_MULTITHREADED);
        if com.is_ok() {
            let icon=extract_icon(Path::new(&path),128).map(Arc::new);
            CoUninitialize();
            icon
        }else{None}
    };
    // Release before notifying the UI so its next paint can submit another icon.
    drop(permit);
    if !cancelled.load(Ordering::Acquire)&&result.send((path,icon)).is_ok(){
        PostMessageW(control as _,WM_ICON_CHANGED,0,0);
    }
}

pub struct Worker {
    result:Sender<(String,ResultIcon)>,
    done:Receiver<(String,ResultIcon)>,
    cache:HashMap<String,(ResultIcon,u64)>,
    pending:bool,
    clock:u64,
    cancelled:Arc<AtomicBool>,
    control:usize,
}
impl Worker {
    pub fn new(control:*mut c_void)->Self{
        let (result,done)=mpsc::channel();
        Self{result,done,cache:HashMap::new(),pending:false,clock:0,
            cancelled:Arc::new(AtomicBool::new(false)),control:control as usize}
    }
    pub fn receive(&mut self)->bool{
        let Ok((path,icon))=self.done.try_recv()else{return false;};
        self.pending=false;
        if self.cache.len()>=CAP {
            if let Some(oldest)=self.cache.iter().min_by_key(|(_,(_,at))|*at).map(|(id,_)|id.clone()){
                self.cache.remove(&oldest);
            }
        }
        self.cache.insert(path,(icon,self.clock));
        true
    }
    pub fn request(&mut self,items:&[Item])->Atlas{
        self.clock=self.clock.wrapping_add(1);
        let mut visible=Atlas::new();
        for item in items{
            if let Some((icon,at))=self.cache.get_mut(&item.path){
                *at=self.clock;visible.insert(item.path.clone(),icon.clone());
            }else if !self.pending&&EXTRACTION_ACTIVE.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_ok(){
                let raw=Box::into_raw(Box::new(Job{path:item.path.clone(),result:self.result.clone(),
                    cancelled:self.cancelled.clone(),control:self.control,permit:Permit}));
                unsafe{
                    if TrySubmitThreadpoolCallback(Some(extract),raw.cast(),ptr::null())!=0{
                        self.pending=true;
                    }else{
                        // The callback cannot run when submission fails.
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
