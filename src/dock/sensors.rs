//! Read-only sensori.h microphone/camera activity, only on explicit open/refresh.
//! Windows reports are informative, never an assertion that recording is absent.
use super::{STATE,control_center::{self,Card,Command,View}};
use std::{cell::RefCell,ptr,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}}};
use windows_sys::Win32::{Foundation::HWND,System::Threading::{TrySubmitThreadpoolCallback,PTP_CALLBACK_INSTANCE},UI::WindowsAndMessaging::*};
pub(super) const READY:u32=WM_APP+152;
#[derive(Clone,Copy)]pub(super) enum Action{Open,Refresh(usize),Privacy(bool)}
#[derive(Default)]struct Snapshot{mic_status:i32,cam_status:i32,mic:Vec<String>,cam:Vec<String>}
#[derive(Default)]struct Shared{closed:AtomicBool,result:Mutex<Option<Snapshot>>}
struct Panel{owner:usize,popup:usize,ticket:usize,pending:bool,view:Snapshot,shared:Arc<Shared>}
impl Drop for Panel{fn drop(&mut self){self.shared.closed.store(true,Ordering::Release);}}
struct Job{popup:usize,ticket:usize,shared:Arc<Shared>}
thread_local!{static PANEL:RefCell<Option<Panel>>=const{RefCell::new(None)};}
static ACTIVE:AtomicBool=AtomicBool::new(false);
#[repr(C)]struct Native{mic_status:i32,cam_status:i32,mic_count:u32,cam_count:u32,mic:[[u16;257];16],cam:[[u16;257];16]}
#[cfg(target_env="msvc")]extern "C"{fn tlauncher_sensors_read(out:*mut Native)->bool;#[cfg(test)]fn tlauncher_sensors_fixture()->bool;}
unsafe fn read()->Snapshot{
    #[cfg(target_env="msvc")]{
        use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_MULTITHREADED};
        if CoInitializeEx(None,COINIT_MULTITHREADED).is_ok(){
            struct Apartment;impl Drop for Apartment{fn drop(&mut self){unsafe{CoUninitialize();}}}let _apartment=Apartment;
            let mut out:Native=std::mem::zeroed();if tlauncher_sensors_read(&mut out){
                let decode=|rows:&[[u16;257];16],count:u32|rows.iter().take(count.min(16) as usize).map(|s|String::from_utf16_lossy(&s[..s.iter().position(|c|*c==0).unwrap_or(256)])).collect::<Vec<_>>();
                return Snapshot{mic_status:out.mic_status,cam_status:out.cam_status,mic:decode(&out.mic,out.mic_count),cam:decode(&out.cam,out.cam_count)};
            }
        }
    }
    Snapshot::default()
}
unsafe extern "system" fn worker(_:PTP_CALLBACK_INSTANCE,raw:*mut std::ffi::c_void){
    let job=*Box::from_raw(raw.cast::<Job>());
    if !job.shared.closed.load(Ordering::Acquire){let view=read();if !job.shared.closed.load(Ordering::Acquire){if let Ok(mut result)=job.shared.result.lock(){*result=Some(view);}}}
    ACTIVE.store(false,Ordering::Release);
    if !job.shared.closed.load(Ordering::Acquire){PostMessageW(job.popup as HWND,READY,job.ticket,0);}
}
fn text(vi:&str,en:&str,zh:&str)->String{crate::launcher::localized(vi,en,zh).into()}
fn title()->String{text("Microphone và webcam","Microphone and webcam","麦克风与摄像头")}
fn enabled()->bool{STATE.with(|s|s.try_borrow().is_ok_and(|s|s.prefs.enabled&&!s.suspended))}
fn activity(status:i32,apps:&[String])->String{
    let state=match status{
        1=>text("Báo cáo Windows","Windows report","Windows 报告"),2=>text("Báo cáo chưa đầy đủ","Partial report","报告不完整"),
        _=>return text("Chưa đọc được trạng thái","Status unavailable","状态不可用"),
    };
    let detail=if apps.is_empty(){text("Chưa có ứng dụng được báo cáo","No apps reported","未报告使用中的应用")}else{format!("{} {}",apps.len(),text("ứng dụng","apps","个应用"))};
    format!("{state} · {detail}")
}
impl Panel{
    fn view(&self)->View{
        let mut cards=vec![
            Card{icon:None,title:text("Microphone · phiên âm thanh đang hoạt động","Microphone · active audio sessions","麦克风 · 活动音频会话"),hint:activity(self.view.mic_status,&self.view.mic),action:None},
            Card{icon:None,title:text("Webcam · dữ liệu quyền riêng tư Windows","Webcam · Windows privacy activity","摄像头 · Windows 隐私活动"),hint:activity(self.view.cam_status,&self.view.cam),action:None},
            Card{icon:None,title:text("Làm mới","Refresh","刷新"),hint:String::new(),action:(!self.pending).then_some(Command::Sensors(Action::Refresh(self.ticket)))},
        ];
        for (apps,label) in [(&self.view.mic,text("Microphone","Microphone","麦克风")),(&self.view.cam,text("Webcam","Webcam","摄像头"))]{
            for app in apps{cards.push(Card{icon:None,title:app.clone(),hint:label.clone(),action:None});}
        }
        for (camera,label) in [(false,text("Quyền riêng tư microphone","Microphone privacy","麦克风隐私")),(true,text("Quyền riêng tư webcam","Webcam privacy","摄像头隐私"))]{cards.push(Card{icon:None,title:label,hint:String::new(),action:Some(Command::Sensors(Action::Privacy(camera)))});}
        View{title:title(),hint:if self.pending{text("Đang đọc…","Reading…","正在读取…")}else{text("Chỉ đọc · Windows có thể báo thiếu hoặc chậm","Read only · Windows reports may be incomplete or delayed","只读 · Windows 报告可能不完整或有延迟")},cards,metrics:None}
    }
    unsafe fn submit(&mut self){
        if self.pending{return;}if ACTIVE.swap(true,Ordering::AcqRel){return;}
        self.pending=true;let raw=Box::into_raw(Box::new(Job{popup:self.popup,ticket:self.ticket,shared:self.shared.clone()}));
        if TrySubmitThreadpoolCallback(Some(worker),raw.cast(),ptr::null())==0{drop(Box::from_raw(raw));ACTIVE.store(false,Ordering::Release);self.pending=false;}
    }
}
pub(super) fn detach(popup:HWND){PANEL.with(|s|{let mut s=s.borrow_mut();if s.as_ref().is_some_and(|p|p.popup==popup as usize){s.take();}});}
pub(super) unsafe fn receive(popup:HWND,ticket:usize){
    let update=PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().filter(|p|p.popup==popup as usize&&p.ticket==ticket)?;
        let view=p.shared.result.lock().ok().and_then(|mut s|s.take())?;p.pending=false;p.view=view;Some((p.owner as HWND,p.ticket,p.view()))});
    if let Some((owner,ticket,view))=update{if enabled(){control_center::update(owner,ticket,view);}}
}
pub(super) unsafe fn perform(owner:HWND,popup:HWND,action:Action){
    if !enabled(){return;}
    match action{
        Action::Open=>{
            let mut p=Panel{owner:owner as usize,popup:0,ticket:0,pending:false,view:Snapshot::default(),shared:Arc::new(Shared::default())};
            control_center::show_list_capacity(owner,super::dock_app_anchor(owner,None),p.view(),37);
            let (Some(popup),Some(ticket))=(control_center::window(owner),control_center::ticket(owner))else{return;};
            p.popup=popup as usize;p.ticket=ticket;p.submit();let view=p.view();PANEL.with(|s|s.replace(Some(p)));control_center::update(owner,ticket,view);
        },
        Action::Refresh(ticket)=>{
            let update=PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().filter(|p|p.owner==owner as usize&&p.popup==popup as usize&&p.ticket==ticket)?;p.submit();Some((p.ticket,p.view()))});
            if let Some((ticket,view))=update{control_center::update(owner,ticket,view);}
        },
        Action::Privacy(camera)=>{
            if !cfg!(test){let uri=crate::desktop::wide(if camera{"ms-settings:privacy-webcam"}else{"ms-settings:privacy-microphone"});windows_sys::Win32::UI::Shell::ShellExecuteW(owner,crate::desktop::wide("open").as_ptr(),uri.as_ptr(),ptr::null(),ptr::null(),SW_SHOWNORMAL);}
        },
    }
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn unknown_and_partial_activity_never_claim_absent_recording(){
        assert_eq!(activity(0,&[]),text("Chưa đọc được trạng thái","Status unavailable","状态不可用"));
        assert!(activity(2,&["Meeting".into()]).contains(&text("Báo cáo chưa đầy đủ","Partial report","报告不完整")));
        assert!(activity(1,&[]).contains(&text("Chưa có ứng dụng được báo cáo","No apps reported","未报告使用中的应用")));
    }
    #[cfg(target_env="msvc")]
    #[test]#[ignore="hosted read-only device activity and synthetic vi/en/zh panels; never records or changes privacy"]
    fn native_read_only_activity_releases_and_refuses_closed_delivery(){
        if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
        unsafe{
            use windows_sys::Win32::Foundation::*;
            use windows_sys::Win32::Graphics::{Dwm::DwmFlush,Gdi::*};
            use crate::desktop::wide;
            assert!(tlauncher_sensors_fixture());
            let old=STATE.with(|s|{let mut s=s.borrow_mut();let old=s.prefs.enabled;s.prefs.enabled=true;old});
            struct Reset(bool,String);impl Drop for Reset{fn drop(&mut self){STATE.with(|s|s.borrow_mut().prefs.enabled=self.0);crate::launcher::spotlight_locale(&self.1);}}
            let _reset=Reset(old,crate::launcher::localized("vi","en","zh-CN").into());
            let owner=CreateWindowExW(WS_EX_TOOLWINDOW,wide("STATIC").as_ptr(),wide("Device activity fixture").as_ptr(),WS_POPUP,10,10,320,200,ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),ptr::null());assert!(!owner.is_null());
            let pump=||{let mut msg:MSG=std::mem::zeroed();while PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}std::thread::sleep(std::time::Duration::from_millis(10));};
            for language in ["vi","en","zh-CN"]{
                crate::launcher::spotlight_locale(language);perform(owner,ptr::null_mut(),Action::Open);
                let (popup,ticket,shared)=PANEL.with(|s|{let s=s.borrow();let p=s.as_ref().unwrap();(p.popup as HWND,p.ticket,p.shared.clone())});
                let started=std::time::Instant::now();while PANEL.with(|s|s.borrow().as_ref().unwrap().pending){pump();assert!(started.elapsed()<std::time::Duration::from_secs(15));}
                PANEL.with(|s|{let s=s.borrow();let p=s.as_ref().unwrap();assert!(matches!(p.view.mic_status,0..=2));assert!(matches!(p.view.cam_status,0..=2));assert!(p.view.mic.len()<=16&&p.view.cam.len()<=16);});
                perform(owner,popup,Action::Refresh(ticket.wrapping_add(1)));assert!(!PANEL.with(|s|s.borrow().as_ref().unwrap().pending));
                // Replace all native activity with explicit synthetic labels
                // before any image export; no real process name is exported.
                let view=PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().unwrap();p.view=Snapshot{mic_status:2,cam_status:1,mic:vec!["Synthetic meeting".into()],cam:vec!["Synthetic camera".into()]};p.view()});
                control_center::update(owner,ticket,view);
                let start=std::time::Instant::now();while start.elapsed()<std::time::Duration::from_millis(400){pump();}
                if let Some(dir)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){
                    DwmFlush();let mut r:RECT=std::mem::zeroed();GetWindowRect(popup,&mut r);let (w,h)=(r.right-r.left,r.bottom-r.top);let mut layer=super::super::glass_probe::Layer::new(w,h).unwrap();let dc=GetDC(ptr::null_mut());
                    assert_ne!(BitBlt(layer.dc(),0,0,w,h,dc,r.left,r.top,SRCCOPY|CAPTUREBLT),0);ReleaseDC(ptr::null_mut(),dc);GdiFlush();
                    let rgba=layer.pixels().chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],255]).collect::<Vec<_>>();image::save_buffer(std::path::PathBuf::from(dir).join(format!("device-activity-{language}.png")),&rgba,w as u32,h as u32,image::ColorType::Rgba8).unwrap();
                }
                control_center::close_for(owner);assert!(shared.closed.load(Ordering::Acquire));assert!(PANEL.with(|s|s.borrow().is_none()));
                *shared.result.lock().unwrap()=Some(Snapshot{mic_status:1,..Default::default()});receive(popup,ticket);assert!(PANEL.with(|s|s.borrow().is_none()));
                perform(owner,ptr::null_mut(),Action::Open);let shared=PANEL.with(|s|s.borrow().as_ref().unwrap().shared.clone());control_center::close_for(owner);assert!(shared.closed.load(Ordering::Acquire));
                let started=std::time::Instant::now();while ACTIVE.load(Ordering::Acquire){pump();assert!(started.elapsed()<std::time::Duration::from_secs(15));}
            }
            DestroyWindow(owner);println!("FLUIDWIN_DEVICE_ACTIVITY read_only_native=PASS bounded_names=PASS unknown_partial=PASS locales=vi,en,zh stale_refresh=REFUSED closed_delivery=REFUSED scoped_worker=RELEASED actual_app_names=NOT_EXPORTED recording=NOT_INVOKED privacy_write=NOT_INVOKED hardware_truth=NOT_VERIFIED");
        }
    }
}
