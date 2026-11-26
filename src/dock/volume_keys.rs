//! Original physical volume-key route (fluidwin.cpp / centro.h). The shared
//! input hook only admits work; the existing utilities pool permit serializes
//! Core Audio actions. No COM, input replay or waiting on the hook thread.
use std::{collections::VecDeque,ffi::c_void,ptr,sync::{Arc,Mutex,atomic::{AtomicBool,AtomicU64,Ordering}}};
use windows_sys::Win32::{Foundation::*,System::{SystemInformation::GetTickCount64,Threading::{TrySubmitThreadpoolCallback,PTP_CALLBACK_INSTANCE}},UI::{Input::KeyboardAndMouse::*,WindowsAndMessaging::*}};
pub(super) const READY:u32=WM_APP+167;
pub(super) const TIMER:usize=36;
const CAPACITY:usize=64;
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) enum Key{Up,Down,Mute}
impl Key{
    pub(super) fn adjust(self,level:f32,muted:bool)->(f32,bool){match self{
        Self::Up=>((((level+0.02)*50.0).round()/50.0).clamp(0.0,1.0),false),
        Self::Down=>((((level-0.02)*50.0).round()/50.0).clamp(0.0,1.0),muted),
        Self::Mute=>(level,!muted),
    }}
    fn index(self)->usize{match self{Self::Up=>0,Self::Down=>1,Self::Mute=>2}}
    fn vk(self)->u16{match self{Self::Up=>VK_VOLUME_UP,Self::Down=>VK_VOLUME_DOWN,Self::Mute=>VK_VOLUME_MUTE}}
    pub(super) fn from_vk(vk:u16)->Option<Self>{match vk{VK_VOLUME_UP=>Some(Self::Up),VK_VOLUME_DOWN=>Some(Self::Down),VK_VOLUME_MUTE=>Some(Self::Mute),_=>None}}
}
#[derive(Default)]pub(super) struct Edges{held:[u8;3]}
impl Edges{
    // Once a down passes, keep the rest of that physical press native. If a
    // repeat cannot be admitted, its down and subsequent up both pass too.
    pub(super) fn edge(&mut self,key:Key,down:bool,compatible:bool,enqueue:impl FnOnce()->bool)->bool{
        let held=&mut self.held[key.index()];
        if !down{return std::mem::take(held)==2;}
        if *held==1{return false;}
        let accepted=compatible&&enqueue();*held=if accepted{2}else{1};accepted
    }
}
#[derive(Clone,Copy)]struct Request{key:Key,at:u64}
#[derive(Default)]struct Queue{keys:VecDeque<Request>,running:bool,answer:Option<super::utilities::Audio>}
pub(super) struct Input{
    controller:usize,ticket:usize,pulse:AtomicU64,closed:AtomicBool,failed:AtomicBool,queue:Mutex<Queue>,
    #[cfg(test)]fixture:Option<Arc<Fixture>>,
}
impl Input{
    pub(super) fn new(controller:HWND,ticket:usize)->Arc<Self>{Arc::new(Self{controller:controller as usize,ticket,pulse:AtomicU64::new(0),closed:AtomicBool::new(false),failed:AtomicBool::new(false),queue:Mutex::new(Queue::default()),#[cfg(test)]fixture:None})}
    pub(super) fn pulse(&self){self.pulse.store(unsafe{GetTickCount64()}.max(1),Ordering::Release);}
    fn live(&self)->bool{let at=self.pulse.load(Ordering::Acquire);at!=0&&unsafe{GetTickCount64()}.saturating_sub(at)<1000&&!self.closed.load(Ordering::Acquire)&&!self.failed.load(Ordering::Acquire)}
    fn fresh(&self,request:Request)->bool{!self.closed.load(Ordering::Acquire)&&unsafe{GetTickCount64()}.saturating_sub(request.at)<1000}
    pub(super) fn device_changed(&self){if !self.closed.load(Ordering::Acquire){self.failed.store(false,Ordering::Release);}}
    pub(super) fn close(&self){self.closed.store(true,Ordering::Release);if let Ok(mut q)=self.queue.lock(){q.keys.clear();q.answer=None;}}
    pub(super) fn take(&self)->Option<super::utilities::Audio>{self.queue.lock().ok()?.answer.take()}
    pub(super) fn enqueue(self:&Arc<Self>,key:Key)->bool{
        if !self.live()||crate::trial::active(){return false;}
        #[cfg(test)]if self.fixture.is_none(){return false;}
        // Hook delivery must never wait for an audio callback or the UI.
        let Ok(mut q)=self.queue.try_lock()else{return false;};
        if !self.live()||q.keys.len()>=CAPACITY{return false;}
        let request=Request{key,at:unsafe{GetTickCount64()}};
        if q.running{q.keys.push_back(request);return true;}
        let Some(permit)=super::utilities::try_permit()else{return false;};
        q.keys.push_back(request);q.running=true;
        let raw=Box::into_raw(Box::new(Job{input:self.clone(),permit:Some(permit)}));
        if unsafe{TrySubmitThreadpoolCallback(Some(worker),raw.cast(),ptr::null())}==0{
            unsafe{drop(Box::from_raw(raw));}q.running=false;q.keys.clear();return false;
        }true
    }
}
struct Job{input:Arc<Input>,permit:Option<super::utilities::Permit>}
unsafe extern "system" fn worker(_:PTP_CALLBACK_INSTANCE,raw:*mut c_void){
    let mut job=Box::from_raw(raw.cast::<Job>());let input=job.input.clone();
    loop{
        let request={
            let Ok(mut q)=input.queue.lock()else{return;};
            if !input.live(){q.keys.clear();}
            match q.keys.pop_front(){Some(key)=>key,None=>{
                // Release global admission while holding this queue's lock,
                // so a new key cannot see running=false with the old permit.
                job.permit.take();q.running=false;return;
            }}
        };
        if !input.fresh(request){continue;}
        let key=request.key;
        #[cfg(test)]let (audio,fallback)=if let Some(fixture)=input.fixture.as_ref(){fixture.apply(key,&input)}else{(None,false)};
        #[cfg(not(test))]let (audio,fallback)=super::utilities::key_audio(key,||input.live()&&input.fresh(request));
        if let Some(audio)=audio{
            if !input.closed.load(Ordering::Acquire){
                if let Ok(mut q)=input.queue.lock(){q.answer=Some(audio);}
                PostMessageW(input.controller as HWND,READY,input.ticket,0);
            }
        }else{
            // A preflight failure never entered a setter. Replay only that
            // command; after any attempted write its outcome is uncertain.
            if fallback&&input.fresh(request){
                #[cfg(not(test))]replay(key);
                #[cfg(test)]if let Some(f)=input.fixture.as_ref(){f.fallbacks.fetch_add(1,Ordering::Relaxed);}
            }
            input.failed.store(true,Ordering::Release);
            // Already-admitted commands must not silently disappear when the
            // first preflight fails. None of these has entered a setter.
            let pending=input.queue.lock().ok().map(|mut q|{q.answer=None;q.keys.drain(..).collect::<Vec<_>>()}).unwrap_or_default();
            if !input.closed.load(Ordering::Acquire){for pending in pending{if !input.fresh(pending){continue;}
                #[cfg(not(test))]replay(pending.key);
                #[cfg(test)]{if let Some(f)=input.fixture.as_ref(){f.fallbacks.fetch_add(1,Ordering::Relaxed);}}
            }}
            crate::log!("VOLUME_KEYS endpoint unavailable; native keys retained, uncertain commands never replayed");
        }
    }
}
#[cfg(not(test))]unsafe fn replay(key:Key){
    let down=INPUT{r#type:INPUT_KEYBOARD,Anonymous:INPUT_0{ki:KEYBDINPUT{wVk:key.vk(),wScan:0,dwFlags:KEYEVENTF_EXTENDEDKEY,time:0,dwExtraInfo:0x544c564b}}};
    let mut up=down;up.Anonymous.ki.dwFlags|=KEYEVENTF_KEYUP;
    let keys=[down,up];if SendInput(2,keys.as_ptr(),std::mem::size_of::<INPUT>() as i32)==1{SendInput(1,&up,std::mem::size_of::<INPUT>() as i32);}
}
pub(super) unsafe fn receive(ticket:usize){
    let audio=super::WIN_KEY.with(|s|s.borrow().as_ref().filter(|h|h.ticket==ticket&&h.volume_enabled).and_then(|h|h.volume.take()));
    let owner=super::STATE.with(|s|{let s=s.borrow();(s.prefs.enabled&&s.prefs.volume_keys&&!s.suspended).then(||s.bars.iter().find(|b|!b.hidden).map(|b|b.hwnd)).flatten()});
    if let (Some(audio),Some(owner))=(audio,owner){super::volume_osd::show(owner,audio);}
}
pub(super) fn device_changed(){super::WIN_KEY.with(|s|{if let Some(hook)=s.borrow().as_ref(){hook.volume.device_changed();}});}

#[cfg(test)]struct Fixture{
    audio:Mutex<(u32,bool)>,commands:Mutex<Vec<Key>>,entered:AtomicBool,hold:AtomicBool,fail:AtomicBool,attempted:AtomicBool,fallbacks:std::sync::atomic::AtomicUsize,
}
#[cfg(test)]impl Fixture{
    fn apply(&self,key:Key,input:&Input)->(Option<super::utilities::Audio>,bool){
        self.entered.store(true,Ordering::Release);
        while self.hold.load(Ordering::Acquire)&&!input.closed.load(Ordering::Acquire){std::thread::sleep(std::time::Duration::from_millis(2));}
        if input.closed.load(Ordering::Acquire){return (None,false);}
        if self.fail.load(Ordering::Acquire){return (None,!self.attempted.load(Ordering::Acquire));}
        self.commands.lock().unwrap().push(key);let mut value=self.audio.lock().unwrap();
        let changed=key.adjust(value.0 as f32/100.0,value.1);*value=((changed.0*100.0).round() as u32,changed.1);
        (Some(super::utilities::Audio::fixture(value.0,value.1)),false)
    }
}
#[cfg(test)]mod tests{
    use super::*;
    use std::time::{Duration,Instant};
    fn input()->(Arc<Input>,Arc<Fixture>){
        let f=Arc::new(Fixture{audio:Mutex::new((46,false)),commands:Mutex::new(Vec::new()),entered:AtomicBool::new(false),hold:AtomicBool::new(false),fail:AtomicBool::new(false),attempted:AtomicBool::new(false),fallbacks:Default::default()});
        let mut i=Input::new(ptr::null_mut(),71);Arc::get_mut(&mut i).unwrap().fixture=Some(f.clone());i.pulse();(i,f)
    }
    fn until(done:impl Fn()->bool){let start=Instant::now();while !done(){assert!(start.elapsed()<Duration::from_secs(4));std::thread::sleep(Duration::from_millis(2));}}
    fn idle(i:&Input)->bool{i.queue.lock().unwrap().running==false}
    #[test]fn physical_edges_balance_native_fallback_repeats_modifiers_and_injected_path(){
        let mut e=Edges::default();assert!(e.edge(Key::Up,true,true,||true));assert!(e.edge(Key::Up,true,true,||true));
        assert!(!e.edge(Key::Up,true,false,||panic!("stale heartbeat cannot enqueue")));assert!(!e.edge(Key::Up,true,true,||panic!("native press stays native")));assert!(!e.edge(Key::Up,false,true,||false));
        assert!(!e.edge(Key::Mute,true,false,||panic!("modified chord stays native")));assert!(!e.edge(Key::Mute,false,true,||false));
        assert!(e.edge(Key::Down,true,true,||true));assert!(e.edge(Key::Down,false,false,||panic!("release never enqueues")));assert!(!e.edge(Key::Down,false,true,||false));
        assert_eq!(Key::Up.adjust(0.513,true),(0.54,false));assert_eq!(Key::Down.adjust(0.513,true),(0.5,true));
        assert_eq!(Key::Up.adjust(1.0,true),(1.0,false));assert_eq!(Key::Down.adjust(0.0,true),(0.0,true));assert_eq!(Key::Mute.adjust(0.513,true),(0.513,false));
    }
    #[test]
    #[ignore="hosted Windows pool and real shared utilities admission; synthetic audio only, no device/input writes"]
    fn native_volume_key_queue_serializes_repeats_cancellation_failure_and_teardown(){
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(),Ok("true"));
        // Drive the actual low-level callback with local event structures.
        // No hardware event, SendInput or real endpoint is invoked.
        let (routed,r)=input();r.hold.store(true,Ordering::Release);
        assert!(unsafe{super::super::win_key::fixture_volume_edge(routed.clone(),Key::Up.vk(),true,false)});
        until(||r.entered.load(Ordering::Acquire));
        assert!(unsafe{super::super::win_key::fixture_volume_edge(routed.clone(),Key::Up.vk(),false,false)});
        assert!(!unsafe{super::super::win_key::fixture_volume_edge(routed.clone(),Key::Mute.vk(),true,true)},"injected events must pass the real callback");
        r.hold.store(false,Ordering::Release);until(||idle(&routed));assert_eq!(*r.commands.lock().unwrap(),vec![Key::Up]);routed.close();
        assert_eq!(unsafe{super::super::utilities::key_audio(Key::Up,||true)},(None,false),"TEST refuses real audio mutation and replay");
        let (i,f)=input();f.hold.store(true,Ordering::Release);assert!(i.enqueue(Key::Up));until(||f.entered.load(Ordering::Acquire));
        assert!(super::super::utilities::try_permit().is_none(),"same global callback permit as Control Center");
        for key in [Key::Up,Key::Mute,Key::Down,Key::Mute]{assert!(i.enqueue(key));}
        f.hold.store(false,Ordering::Release);until(||idle(&i));assert_eq!(*f.commands.lock().unwrap(),vec![Key::Up,Key::Up,Key::Mute,Key::Down,Key::Mute]);assert_eq!(*f.audio.lock().unwrap(),(48,false));assert_eq!(i.take().unwrap().osd_value(),(48,false));
        // Admission failure must leave the physical command for Windows.
        let permit=super::super::utilities::try_permit().unwrap();assert!(!i.enqueue(Key::Up));drop(permit);
        i.pulse.store(1,Ordering::Release);assert!(!i.enqueue(Key::Up));i.pulse();
        // Hold an actual admitted callback through closure/replacement.
        f.entered.store(false,Ordering::Release);f.hold.store(true,Ordering::Release);assert!(i.enqueue(Key::Up));until(||f.entered.load(Ordering::Acquire));assert!(i.enqueue(Key::Mute));i.close();until(||idle(&i));assert_eq!(f.commands.lock().unwrap().len(),5);assert!(i.take().is_none());assert!(!i.enqueue(Key::Up));
        let (bounded,b)=input();b.hold.store(true,Ordering::Release);assert!(bounded.enqueue(Key::Up));until(||b.entered.load(Ordering::Acquire));for _ in 0..CAPACITY{assert!(bounded.enqueue(Key::Down));}assert!(!bounded.enqueue(Key::Mute));bounded.close();until(||idle(&bounded));
        for attempted in [false,true]{let (failed,f)=input();f.hold.store(true,Ordering::Release);f.fail.store(true,Ordering::Release);f.attempted.store(attempted,Ordering::Release);assert!(failed.enqueue(Key::Up));until(||f.entered.load(Ordering::Acquire));assert!(failed.enqueue(Key::Mute));f.hold.store(false,Ordering::Release);until(||idle(&failed));assert_eq!(f.fallbacks.load(Ordering::Acquire),if attempted{1}else{2});assert!(failed.take().is_none());assert!(!failed.enqueue(Key::Down));failed.close();}
        assert!(super::super::utilities::try_permit().is_some());
        let mut prefs=super::super::Options::default();assert!(!prefs.volume_keys);prefs.volume_keys=true;assert!(super::super::Options::from_json(&prefs.json()).volume_keys);
        println!("FLUIDWIN_VOLUME_KEYS actual_hook_callback=PASS injected_events=PASS ordered_repeat_mute=PASS shared_pool_permit=PASS queue_bound=64 native_admission_fallback=PASS stale_heartbeat=REFUSED closed_callback=CANCELLED uncertain_write=NEVER_REPLAYED preflight_failure=NATIVE_FALLBACK actual_audio=NOT_INVOKED SendInput=NOT_INVOKED");
    }
}
