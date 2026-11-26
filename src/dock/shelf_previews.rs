//! Scoped, memory-only File Shelf images. Original notch.h::miniatura uses
//! IShellItemImageFactory; keep Shell extraction outside drawing/input/OLE.
use std::{cell::RefCell,path::PathBuf,ptr,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}}};
use windows_sys::Win32::{Foundation::HWND,System::Threading::{TrySubmitThreadpoolCallback,PTP_CALLBACK_INSTANCE},UI::WindowsAndMessaging::*};
use crate::launcher::IconPixels;
pub(super) const READY:u32=WM_APP+151;
pub(super) const RETRY:usize=19;
const LIMIT:usize=6;
type Rows=Vec<(u64,Option<Arc<IconPixels>>)>;
struct Delivery{closed:AtomicBool,result:Mutex<Option<Rows>>}
struct Preview{hwnd:usize,tag:usize,keys:Vec<(u64,PathBuf)>,size:i32,delivery:Arc<Delivery>,rows:Rows,submitted:bool,created:std::time::Instant}
impl Drop for Preview{fn drop(&mut self){self.delivery.closed.store(true,Ordering::Release);unsafe{KillTimer(self.hwnd as HWND,RETRY);}}}
struct Job{hwnd:usize,tag:usize,keys:Vec<(u64,PathBuf)>,size:i32,delivery:Arc<Delivery>}
static ACTIVE:AtomicBool=AtomicBool::new(false);
thread_local!{static STATE:RefCell<Option<Preview>>=const{RefCell::new(None)};}
unsafe extern "system" fn worker(_:PTP_CALLBACK_INSTANCE,raw:*mut std::ffi::c_void){
    let job=*Box::from_raw(raw.cast::<Job>());
    let apartment=windows::Win32::System::Com::CoInitializeEx(None,windows::Win32::System::Com::COINIT_APARTMENTTHREADED).is_ok();
    let mut rows=Vec::new();
    if apartment{
        for (id,path) in job.keys{
            if job.delivery.closed.load(Ordering::Acquire){break;}
            let image=crate::launcher::shell_thumbnail(&path,job.size).and_then(|mut icon|{
                if icon.width>job.size||icon.height>job.size{
                    let ratio=job.size as f64/icon.width.max(icon.height) as f64;
                    let width=(icon.width as f64*ratio).round().max(1.0) as u32;
                    let height=(icon.height as f64*ratio).round().max(1.0) as u32;
                    let source=image::RgbaImage::from_raw(icon.width as u32,icon.height as u32,icon.pixels)?;
                    icon.pixels=image::imageops::resize(&source,width,height,image::imageops::FilterType::Triangle).into_raw();
                    icon.width=width as i32;icon.height=height as i32;
                    for pixel in icon.pixels.chunks_exact_mut(4){for c in 0..3{pixel[c]=pixel[c].min(pixel[3]);}}
                }
                Some(Arc::new(icon))
            });
            rows.push((id,image));
        }
        windows::Win32::System::Com::CoUninitialize();
    }
    // Publish before returning the one global permit; an old callback cannot
    // replace a newly admitted page's data, even if Windows recycles its HWND.
    if !job.delivery.closed.load(Ordering::Acquire){if let Ok(mut out)=job.delivery.result.lock(){*out=Some(rows);}}
    ACTIVE.store(false,Ordering::Release);
    if !job.delivery.closed.load(Ordering::Acquire){PostMessageW(job.hwnd as HWND,READY,job.tag,0);}
}
pub(super) fn close(hwnd:HWND){STATE.with(|s|{let mut s=s.borrow_mut();if s.as_ref().is_some_and(|p|p.hwnd==hwnd as usize){s.take();}});}
pub(super) unsafe fn request(hwnd:HWND,tag:usize,keys:Vec<(u64,PathBuf)>,dpi:u32){
    let keys=keys.into_iter().take(LIMIT).collect::<Vec<_>>();
    let size=super::panel_chrome::scale(dpi,40).clamp(30,120);
    STATE.with(|s|{
        let mut s=s.borrow_mut();
        if s.as_ref().is_some_and(|p|p.hwnd==hwnd as usize&&p.tag==tag&&p.keys==keys&&p.size==size){return;}
        *s=Some(Preview{hwnd:hwnd as usize,tag,keys,size,delivery:Arc::new(Delivery{closed:AtomicBool::new(false),result:Mutex::new(None)}),rows:Vec::new(),submitted:false,created:std::time::Instant::now()});
    });
    submit(hwnd);
}
pub(super) unsafe fn submit(hwnd:HWND){STATE.with(|s|{
    let mut s=s.borrow_mut();let Some(p)=s.as_mut().filter(|p|p.hwnd==hwnd as usize&&!p.submitted)else{return;};
    if p.keys.is_empty()||p.created.elapsed()>=std::time::Duration::from_secs(4){p.submitted=true;KillTimer(hwnd,RETRY);return;}
    if ACTIVE.swap(true,Ordering::AcqRel){SetTimer(hwnd,RETRY,80,None);return;}
    let job=Box::into_raw(Box::new(Job{hwnd:p.hwnd,tag:p.tag,keys:p.keys.clone(),size:p.size,delivery:p.delivery.clone()}));
    if TrySubmitThreadpoolCallback(Some(worker),job.cast(),ptr::null())==0{
        drop(Box::from_raw(job));ACTIVE.store(false,Ordering::Release);SetTimer(hwnd,RETRY,80,None);
    }else{p.submitted=true;KillTimer(hwnd,RETRY);}
});}
pub(super) fn receive(hwnd:HWND,tag:usize)->bool{STATE.with(|s|{
    let mut s=s.borrow_mut();let Some(p)=s.as_mut().filter(|p|p.hwnd==hwnd as usize&&p.tag==tag)else{return false;};
    let Some(rows)=p.delivery.result.lock().ok().and_then(|mut r|r.take())else{return false;};
    p.rows=rows;true
})}
pub(super) fn image(id:u64)->Option<Arc<IconPixels>>{STATE.with(|s|s.borrow().as_ref()?.rows.iter().find(|r|r.0==id)?.1.clone())}
#[cfg(test)]pub(super) fn idle()->bool{!ACTIVE.load(Ordering::Acquire)}
