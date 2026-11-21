//! Reuse the launcher's high-resolution Shell extraction away from the window
//! thread. Bounded requests, bounded atlas, no timer to check for completions.
use std::{
    collections::{HashMap,HashSet},
    path::Path,
    sync::{Arc,atomic::{AtomicBool,Ordering},mpsc::{self,Receiver,SyncSender,TrySendError}},
    thread::JoinHandle,
};
use crate::launcher::{extract_icon,IconPixels};
use super::{Item,WM_ICON_CHANGED,surface::Atlas};

type ResultIcon=Option<Arc<IconPixels>>;
const CAP:usize=48;
const PENDING:usize=16;
// Quitting a module must never wait on a third-party Shell provider. Keep at
// most one extraction thread, including an older one still returning from COM.
static WORKER_ACTIVE:AtomicBool=AtomicBool::new(false);
struct Permit;
impl Drop for Permit {
    fn drop(&mut self){WORKER_ACTIVE.store(false,Ordering::Release);}
}

pub struct Worker {
    send:Option<SyncSender<String>>,
    done:Receiver<(String,ResultIcon)>,
    cache:HashMap<String,(ResultIcon,u64)>,
    pending:HashSet<String>,
    clock:u64,
    stop:Arc<AtomicBool>,
    worker:Option<JoinHandle<()>>,
}
impl Worker {
    pub fn new(control:*mut std::ffi::c_void)->Option<Self>{
        if WORKER_ACTIVE.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).is_err(){
            return None;
        }
        let permit=Permit;
        let (send,work)=mpsc::sync_channel::<String>(PENDING);
        let (reply,done)=mpsc::channel();
        let stop=Arc::new(AtomicBool::new(false));
        let cancel=stop.clone();
        let control=control as usize;
        let worker=std::thread::Builder::new().name("Deskland Dock icons".into()).spawn(move||unsafe{
            let _permit=permit;
            use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
            use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;
            let init=CoInitializeEx(None,COINIT_APARTMENTTHREADED);
            while let Ok(path)=work.recv(){
                if cancel.load(Ordering::Acquire){break;}
                // Never ask external file providers to resolve an arbitrary
                // network pin while starting or closing the desktop overlay.
                let icon=if path.starts_with(r"\\")&&!path.starts_with(r"\\?\C:\"){
                    None
                }else if init.is_ok(){
                    extract_icon(Path::new(&path),128).map(Arc::new)
                }else{None};
                if cancel.load(Ordering::Acquire)||reply.send((path,icon)).is_err(){break;}
                PostMessageW(control as _,WM_ICON_CHANGED,0,0);
            }
            if init.is_ok(){CoUninitialize();}
        }).ok()?;
        Some(Self{send:Some(send),done,cache:HashMap::new(),pending:HashSet::new(),clock:0,stop,worker:Some(worker)})
    }
    pub fn receive(&mut self)->bool{
        let mut changed=false;
        while let Ok((path,icon))=self.done.try_recv(){
            self.pending.remove(&path);
            if self.cache.len()>=CAP {
                if let Some(oldest)=self.cache.iter().min_by_key(|(_,(_,at))|*at).map(|(id,_)|id.clone()){self.cache.remove(&oldest);}
            }
            self.cache.insert(path,(icon,self.clock));
            changed=true;
        }
        changed
    }
    pub fn request(&mut self,items:&[Item])->Atlas{
        self.clock=self.clock.wrapping_add(1);
        let mut visible=Atlas::new();
        for item in items{
            if let Some((icon,at))=self.cache.get_mut(&item.path){
                *at=self.clock;visible.insert(item.path.clone(),icon.clone());
                continue;
            }
            if self.pending.len()>=PENDING||self.pending.contains(&item.path){continue;}
            if let Some(send)=&self.send{
                match send.try_send(item.path.clone()){
                    Ok(())=>{self.pending.insert(item.path.clone());}
                    Err(TrySendError::Full(_))=>break,
                    Err(TrySendError::Disconnected(_))=>break,
                }
            }
        }
        visible
    }
}
impl Drop for Worker{
    fn drop(&mut self){
        self.stop.store(true,Ordering::Release);
        self.send.take();
        // If a third-party Shell provider is in a call, do not block the app
        // message loop on it. Pending work is canceled and it can no longer
        // deliver icons; process exit bounds lifetime in the worst case.
        if let Some(worker)=self.worker.take(){
            if worker.is_finished(){let _=worker.join();}
        }
    }
}
