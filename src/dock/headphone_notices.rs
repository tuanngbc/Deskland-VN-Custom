//! Original sensori.h Bluetooth endpoint notices, without its recurring poll.
//! The MMDevice callback posts only; one bounded pool job reads device metadata.
use std::{cell::RefCell,collections::BTreeSet,ptr,sync::{Arc,Mutex,atomic::{AtomicBool,AtomicUsize,Ordering}}};
use windows_sys::Win32::{Foundation::HWND,System::Threading::{TrySubmitThreadpoolCallback,PTP_CALLBACK_INSTANCE},UI::WindowsAndMessaging::*};
pub(super) const CHANGED:u32=WM_APP+167;
pub(super) const READY:u32=WM_APP+168;
#[derive(Clone,Debug,PartialEq,Eq)]struct Endpoint{id:String,name:String,container:[u8;16],bluetooth:bool,battery:Option<u8>}
#[derive(Default)]struct Tracker{previous:Option<BTreeSet<String>>}
impl Tracker{
    fn observe(&mut self,reading:Option<Vec<Endpoint>>)->Vec<Endpoint>{
        let Some(rows)=reading else{self.previous=None;return Vec::new();};
        let ids=rows.iter().map(|r|r.id.clone()).collect();
        let Some(previous)=self.previous.replace(ids)else{return Vec::new();};
        let mut containers=BTreeSet::new();
        rows.into_iter().filter(|r|r.bluetooth&&r.container!=[0;16]&&!previous.contains(&r.id)&&containers.insert(r.container)).collect()
    }
}
#[derive(Default)]struct Shared{closed:AtomicBool,result:Mutex<Option<Option<Vec<Endpoint>>>>}
struct Watch{ticket:usize,pending:bool,dirty:bool,tracker:Tracker,shared:Arc<Shared>}
impl Drop for Watch{fn drop(&mut self){self.shared.closed.store(true,Ordering::Release);}}
struct Job{owner:usize,ticket:usize,shared:Arc<Shared>,#[cfg(test)]gate:Option<Arc<Gate>>}
thread_local!{static WATCH:RefCell<Option<Watch>>=const{RefCell::new(None)};}
#[cfg(test)]struct Gate{reached:AtomicBool,resume:Mutex<bool>,wake:std::sync::Condvar}
#[cfg(test)]impl Gate{fn release(&self){if let Ok(mut resume)=self.resume.lock(){*resume=true;self.wake.notify_all();}}}
#[cfg(test)]thread_local!{static GATE:RefCell<Option<Arc<Gate>>>=const{RefCell::new(None)};}
static ACTIVE:AtomicBool=AtomicBool::new(false);
static NEXT:AtomicUsize=AtomicUsize::new(1);
#[repr(C)]struct NativeRow{id:[u16;1025],name:[u16;257],container:[u8;16],bluetooth:i32,battery:i32}
#[repr(C)]struct Native{count:u32,rows:[NativeRow;64]}
#[cfg(target_env="msvc")]extern "C"{
    fn tlauncher_headphones_watch_start(owner:HWND,message:u32,ticket:usize)->bool;
    fn tlauncher_headphones_watch_stop()->bool;
    fn tlauncher_headphones_watch_ack(ticket:usize);
    fn tlauncher_headphones_read(out:*mut Native)->bool;
    #[cfg(test)]fn tlauncher_headphones_fixture(owner:HWND,message:u32)->bool;
}
fn enabled()->bool{super::STATE.with(|s|s.try_borrow().is_ok_and(|s|s.prefs.enabled&&!s.suspended))}
fn label(row:&Endpoint)->String{
    let name=if row.name.trim().is_empty(){crate::launcher::localized("Tai nghe Bluetooth","Bluetooth headphones","蓝牙耳机").into()}else{row.name.chars().take(80).collect::<String>()};
    match row.battery{Some(level)=>format!("{name} · {level}%"),None=>format!("{name} · {}",crate::launcher::localized("đã kết nối","connected","已连接"))}
}
unsafe fn read()->Option<Vec<Endpoint>>{
    #[cfg(target_env="msvc")]{
        use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_MULTITHREADED};
        if CoInitializeEx(None,COINIT_MULTITHREADED).is_err(){return None;}
        struct Apartment;impl Drop for Apartment{fn drop(&mut self){unsafe{CoUninitialize();}}}let _apartment=Apartment;
        let mut native:Box<Native>=Box::new(std::mem::zeroed());
        if !tlauncher_headphones_read(native.as_mut())||native.count>64{return None;}
        let decode=|s:&[u16]|String::from_utf16_lossy(&s[..s.iter().position(|c|*c==0).unwrap_or(s.len())]);
        return Some(native.rows.iter().take(native.count as usize).map(|r|Endpoint{id:decode(&r.id),name:decode(&r.name),container:r.container,bluetooth:r.bluetooth==1,battery:(0..=100).contains(&r.battery).then_some(r.battery as u8)}).collect());
    }
    #[cfg(not(target_env="msvc"))]None
}
unsafe extern "system" fn worker(_:PTP_CALLBACK_INSTANCE,raw:*mut std::ffi::c_void){
    let job=*Box::from_raw(raw.cast::<Job>());
    #[cfg(test)]if let Some(gate)=&job.gate{gate.reached.store(true,Ordering::Release);if let Ok(resume)=gate.resume.lock(){drop(gate.wake.wait_timeout_while(resume,std::time::Duration::from_secs(15),|resume|!*resume));}}
    if !job.shared.closed.load(Ordering::Acquire){let reading=read();if !job.shared.closed.load(Ordering::Acquire){if let Ok(mut slot)=job.shared.result.lock(){*slot=Some(reading);}}}
    ACTIVE.store(false,Ordering::Release);
    // Even an old cancelled job wakes admission for a replacement subscription.
    // Its source ticket and closed state can never deliver an old notice.
    PostMessageW(job.owner as HWND,READY,job.ticket,0);
}
unsafe fn submit(){
    WATCH.with(|s|{let mut s=s.borrow_mut();let Some(w)=s.as_mut()else{return;};
        if w.pending||!w.dirty||ACTIVE.swap(true,Ordering::AcqRel){return;}
        w.pending=true;w.dirty=false;let raw=Box::into_raw(Box::new(Job{owner:super::CONTROL.get() as usize,ticket:w.ticket,shared:w.shared.clone(),#[cfg(test)]gate:GATE.with(|g|g.borrow().clone())}));
        if TrySubmitThreadpoolCallback(Some(worker),raw.cast(),ptr::null())==0{drop(Box::from_raw(raw));ACTIVE.store(false,Ordering::Release);w.pending=false;w.dirty=true;}
    });
}
pub(super) unsafe fn sync(){
    if !enabled(){reset();return;}
    if WATCH.with(|s|s.borrow().is_some()){return;}
    let owner=super::CONTROL.get();if owner.is_null(){return;}
    let ticket=NEXT.fetch_add(1,Ordering::Relaxed).max(1);
    #[cfg(target_env="msvc")]if !tlauncher_headphones_watch_start(owner,CHANGED,ticket){return;}
    #[cfg(not(target_env="msvc"))]return;
    WATCH.with(|s|s.replace(Some(Watch{ticket,pending:false,dirty:true,tracker:Tracker::default(),shared:Arc::default()})));submit();
}
pub(super) unsafe fn changed(ticket:usize){
    #[cfg(target_env="msvc")]tlauncher_headphones_watch_ack(ticket);
    WATCH.with(|s|{if let Some(w)=s.borrow_mut().as_mut().filter(|w|w.ticket==ticket){w.dirty=true;}});if enabled(){submit();}
}
pub(super) unsafe fn receive(ticket:usize){
    let notices=WATCH.with(|s|{let mut s=s.borrow_mut();let Some(w)=s.as_mut().filter(|w|w.ticket==ticket)else{return Vec::new();};
        let reading=w.shared.result.lock().ok().and_then(|mut r|r.take());
        let Some(reading)=reading else{return Vec::new();};w.pending=false;
        // An event during the initial enumeration requires another silent
        // baseline; an endpoint present before monitoring must not be announced.
        if w.dirty&&w.tracker.previous.is_none(){return Vec::new();}
        w.tracker.observe(reading)
    });
    if enabled(){for row in notices{super::volume_osd::headphones(row.battery,&label(&row));}submit();}
}
pub(super) unsafe fn reset(){
    WATCH.with(|s|s.borrow_mut().take());
    #[cfg(target_env="msvc")]tlauncher_headphones_watch_stop();
}
#[cfg(test)]mod tests{
    use super::*;
    fn row(id:&str,container:u8,bluetooth:bool,battery:Option<u8>)->Endpoint{Endpoint{id:id.into(),name:"Synthetic headset".into(),container:[container;16],bluetooth,battery}}
    #[test]fn original_new_endpoint_rules_suppress_baselines_duplicates_and_unknown(){
        let a=row("a",1,true,None);let b=row("b",2,true,Some(73));let c=row("c",2,true,None);let usb=row("usb",3,false,None);
        let mut t=Tracker::default();assert!(t.observe(Some(vec![a.clone()])).is_empty());
        assert_eq!(t.observe(Some(vec![a.clone(),b.clone(),c.clone(),usb])),vec![b.clone()]);
        assert!(t.observe(Some(vec![a.clone(),b.clone(),c])).is_empty());assert!(t.observe(Some(vec![a.clone()])).is_empty());
        assert_eq!(t.observe(Some(vec![a.clone(),b.clone()])),vec![b]);
        assert!(t.observe(None).is_empty());assert!(t.observe(Some(vec![a])).is_empty());
        assert!(label(&row("battery",1,true,Some(73))).ends_with("73%"));assert!(!label(&row("unknown",1,true,None)).contains('%'));
    }
    #[cfg(target_env="msvc")]
    #[test]#[ignore="hosted read-only MMDevice registration, event coalescing and cancelled delivery; no hardware changes"]
    fn native_endpoint_events_release_and_refuse_stale_notice_delivery(){
        if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
        unsafe{
            use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
            CoInitializeEx(None,COINIT_APARTMENTTHREADED).unwrap();
            let owner=CreateWindowExW(0,crate::desktop::wide("STATIC").as_ptr(),crate::desktop::wide("Endpoint event fixture").as_ptr(),0,0,0,1,1,HWND_MESSAGE,ptr::null_mut(),ptr::null_mut(),ptr::null());assert!(!owner.is_null());
            let old_control=super::super::CONTROL.replace(owner);let old_state=super::super::STATE.with(|s|{let mut s=s.borrow_mut();let old=(s.prefs.enabled,s.suspended);s.prefs.enabled=true;s.suspended=false;old});
            struct Cleanup(HWND,HWND,(bool,bool));impl Drop for Cleanup{fn drop(&mut self){unsafe{reset();super::super::CONTROL.set(self.1);super::super::STATE.with(|s|{let mut s=s.borrow_mut();s.prefs.enabled=self.2.0;s.suspended=self.2.1;});DestroyWindow(self.0);CoUninitialize();}}}let _cleanup=Cleanup(owner,old_control,old_state);
            for _ in 0..6{assert!(tlauncher_headphones_fixture(owner,CHANGED));}
            let reading=std::thread::spawn(||read()).join().unwrap();assert!(reading.as_ref().is_none_or(|rows|rows.len()<=64));
            println!("FLUIDWIN_HEADPHONES native_read_rows={:?} real_device_identifiers=NOT_EXPORTED",reading.as_ref().map(Vec::len));
            let mut t=Tracker::default();assert!(t.observe(reading).is_empty());
            for _ in 0..3{
                sync();let (ticket,shared)=WATCH.with(|s|{let s=s.borrow();let w=s.as_ref().unwrap();(w.ticket,w.shared.clone())});
                let deadline=std::time::Instant::now()+std::time::Duration::from_secs(20);
                while ACTIVE.load(Ordering::Acquire){let mut msg:MSG=std::mem::zeroed();while PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}assert!(std::time::Instant::now()<deadline);std::thread::sleep(std::time::Duration::from_millis(10));}
                receive(ticket);assert!(!WATCH.with(|s|s.borrow().as_ref().unwrap().pending));
                reset();assert!(shared.closed.load(Ordering::Acquire));*shared.result.lock().unwrap()=Some(Some(vec![row("late",7,true,Some(99))]));receive(ticket);assert!(WATCH.with(|s|s.borrow().is_none()));
            }
            let pump=||{let mut msg:MSG=std::mem::zeroed();while PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)!=0{match msg.message{CHANGED=>changed(msg.wParam),READY=>receive(msg.wParam),_=>{TranslateMessage(&msg);DispatchMessageW(&msg);}}}std::thread::sleep(std::time::Duration::from_millis(5));};
            let gate=Arc::new(Gate{reached:AtomicBool::new(false),resume:Mutex::new(false),wake:std::sync::Condvar::new()});
            struct Unpause(Arc<Gate>);impl Drop for Unpause{fn drop(&mut self){self.0.release();GATE.with(|g|g.borrow_mut().take());}}let _unpause=Unpause(gate.clone());
            GATE.with(|g|g.replace(Some(gate.clone())));sync();
            let old_ticket=WATCH.with(|s|s.borrow().as_ref().unwrap().ticket);let deadline=std::time::Instant::now()+std::time::Duration::from_secs(10);
            while !gate.reached.load(Ordering::Acquire){pump();assert!(std::time::Instant::now()<deadline);}
            assert!(ACTIVE.load(Ordering::Acquire));reset();GATE.with(|g|g.borrow_mut().take());sync();
            let new_ticket=WATCH.with(|s|{let s=s.borrow();let w=s.as_ref().unwrap();assert!(!w.pending&&w.dirty,"the held old callback must retain the only admission permit");w.ticket});assert_ne!(old_ticket,new_ticket);
            for _ in 0..40{changed(new_ticket);receive(old_ticket);assert!(ACTIVE.load(Ordering::Acquire));assert!(!WATCH.with(|s|s.borrow().as_ref().unwrap().pending));}
            gate.release();let deadline=std::time::Instant::now()+std::time::Duration::from_secs(20);
            while ACTIVE.load(Ordering::Acquire)||WATCH.with(|s|s.borrow().as_ref().is_some_and(|w|w.dirty||w.pending)){pump();assert!(std::time::Instant::now()<deadline,"late cancellation must admit a new silent baseline after completion");}
            WATCH.with(|s|assert_eq!(s.borrow().as_ref().unwrap().ticket,new_ticket));reset();
            super::super::STATE.with(|s|s.borrow_mut().suspended=true);sync();assert!(WATCH.with(|s|s.borrow().is_none()));
            println!("FLUIDWIN_HEADPHONES native_registration=RELEASED event_burst=COALESCED initial_list=SILENT unknown_pin=OMITTED late_delivery=REFUSED worker=BOUNDED physical_bluetooth_change=NOT_INVOKED audio_write=NONE poll=NONE");
        }
    }
}
