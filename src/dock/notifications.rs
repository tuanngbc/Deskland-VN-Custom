//! On-demand Windows notification history, using original notifiche.h.
//! No listener/poll loop or persisted content exists while the panel is closed.
use super::{STATE,control_center::{self,Card,Command,View}};
use crate::desktop::wide;
use std::{cell::RefCell,ffi::c_void,ptr,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}}};
use windows_sys::Win32::{Foundation::*,System::Threading::{TrySubmitThreadpoolCallback,PTP_CALLBACK_INSTANCE},UI::{Shell::ShellExecuteW,WindowsAndMessaging::*}};
pub(super) const READY:u32=WM_APP+149;
#[derive(Clone)]pub(super) enum Action{Open,Refresh(u64),Details(u64,u32),Dismiss(u64,u32),Back(u64),WindowsSettings}
#[derive(Clone,Debug,PartialEq,Eq)]struct Item{id:u32,time:i64,app:String,aumid:String,title:String,body:String}
#[derive(Default)]struct Snapshot{status:i32,items:Vec<Item>,command_failed:bool}
#[derive(Default)]struct Shared{closed:AtomicBool,answer:Mutex<Option<Snapshot>>}
struct Panel{owner:usize,popup:usize,ticket:usize,generation:u64,view:Snapshot,selected:Option<u32>,pending:bool,work:Arc<Shared>}
impl Drop for Panel{fn drop(&mut self){self.work.closed.store(true,Ordering::Release);}}
thread_local!{static PANEL:RefCell<Option<Panel>>=const{RefCell::new(None)};}
static WORKER:AtomicBool=AtomicBool::new(false);
#[cfg(target_env="msvc")]
unsafe extern "C"{
    fn tlauncher_notifications_create()->*mut c_void;fn tlauncher_notifications_free(raw:*mut c_void);
    fn tlauncher_notifications_read(raw:*mut c_void)->i32;fn tlauncher_notifications_count(raw:*mut c_void)->u32;
    fn tlauncher_notifications_item(raw:*mut c_void,index:u32,id:*mut u32,time:*mut i64,app:*mut u16,aumid:*mut u16,title:*mut u16,body:*mut u16,size:u32)->bool;
    fn tlauncher_notifications_remove(raw:*mut c_void,id:u32,time:i64,aumid:*const u16,read_only:bool)->bool;
    #[cfg(test)]fn tlauncher_notifications_identity_fixture()->bool;
}
struct Job{popup:usize,ticket:usize,work:Arc<Shared>,remove:Option<Item>}
unsafe extern "system" fn worker(_:PTP_CALLBACK_INSTANCE,raw:*mut c_void){
    let job=Box::from_raw(raw.cast::<Job>());
    struct Permit;impl Drop for Permit{fn drop(&mut self){WORKER.store(false,Ordering::Release);}}let _permit=Permit;
    if job.work.closed.load(Ordering::Acquire){return;}
    let mut view=Snapshot{status:3,..Default::default()};
    #[cfg(target_env="msvc")]{
        use windows::Win32::System::WinRT::{RoInitialize,RoUninitialize,RO_INIT_MULTITHREADED};
        if RoInitialize(RO_INIT_MULTITHREADED).is_ok(){
            struct Apartment;impl Drop for Apartment{fn drop(&mut self){unsafe{RoUninitialize();}}}let _apartment=Apartment;
            struct Reader(*mut c_void);impl Drop for Reader{fn drop(&mut self){unsafe{tlauncher_notifications_free(self.0);}}}let reader=Reader(tlauncher_notifications_create());
            if !reader.0.is_null(){
                if let Some(item)=&job.remove{
                    if !job.work.closed.load(Ordering::Acquire){view.command_failed=!tlauncher_notifications_remove(reader.0,item.id,item.time,wide(&item.aumid).as_ptr(),crate::trial::active()||cfg!(test));}
                }
                if !job.work.closed.load(Ordering::Acquire){
                    view.status=tlauncher_notifications_read(reader.0);
                    if matches!(view.status,1|2){
                        for index in 0..tlauncher_notifications_count(reader.0).min(64){
                            let mut id=0;let mut time=0;let mut app=[0u16;2049];let mut aumid=[0u16;2049];let mut title=[0u16;2049];let mut body=[0u16;2049];
                            if tlauncher_notifications_item(reader.0,index,&mut id,&mut time,app.as_mut_ptr(),aumid.as_mut_ptr(),title.as_mut_ptr(),body.as_mut_ptr(),2049){
                                let decode=|s:&[u16]|String::from_utf16_lossy(&s[..s.iter().position(|c|*c==0).unwrap_or(s.len())]);
                                view.items.push(Item{id,time,app:decode(&app),aumid:decode(&aumid),title:decode(&title),body:decode(&body)});
                            }else{view.status=2;}
                        }
                    }
                }
            }
        }
    }
    if !job.work.closed.load(Ordering::Acquire){
        if let Ok(mut answer)=job.work.answer.lock(){*answer=Some(view);}
        PostMessageW(job.popup as HWND,READY,job.ticket,0);
    }
}
fn text(vi:&str,en:&str,zh:&str)->String{crate::launcher::localized(vi,en,zh).into()}
fn title()->String{text("Thông báo","Notifications","通知")}
fn enabled()->bool{STATE.with(|s|s.try_borrow().is_ok_and(|s|s.prefs.enabled&&!s.suspended))}
fn merge(current:&mut Snapshot,mut incoming:Snapshot){
    if incoming.status==2{
        for old in &current.items{
            if incoming.items.len()>=64{break;}
            if !incoming.items.iter().any(|i|i.id==old.id){incoming.items.push(old.clone());}
        }
        incoming.items.sort_by_key(|i|std::cmp::Reverse(i.time));
    }else if incoming.status==3{incoming.items=std::mem::take(&mut current.items);}
    *current=incoming;
}
impl Panel{
    unsafe fn submit(&mut self,remove:Option<Item>){
        if self.pending{return;}self.pending=true;
        if WORKER.swap(true,Ordering::AcqRel){self.pending=false;self.view.status=3;return;}
        let raw=Box::into_raw(Box::new(Job{popup:self.popup,ticket:self.ticket,work:self.work.clone(),remove}));
        if TrySubmitThreadpoolCallback(Some(worker),raw.cast(),ptr::null())==0{
            drop(Box::from_raw(raw));WORKER.store(false,Ordering::Release);self.pending=false;self.view.status=3;
        }
    }
    fn view(&self)->View{
        let hint=if self.pending{text("Đang đọc…","Reading…","正在读取…")}
            else if self.view.command_failed{text("Chưa bỏ được thông báo. Hãy làm mới danh sách.","Could not dismiss this notification. Refresh the list.","无法清除此通知，请刷新列表。")}
            else{match self.view.status{
                0=>text("Quyền truy cập thông báo Windows chưa khả dụng cho ứng dụng này","Windows notification access is unavailable for this app","此应用暂时无法访问 Windows 通知"),
                2=>text("Danh sách chưa đầy đủ · tối đa 64 thông báo","Partial list · up to 64 notifications","列表不完整 · 最多 64 条通知"),
                3=>text("Chưa đọc được thông báo mới. Nội dung cũ có thể đã thay đổi.","Could not refresh notifications. Previous entries may have changed.","无法刷新通知，先前条目可能已更改。"),
                _=>String::new(),
            }};
        let action=|a|(!self.pending).then_some(Command::Notifications(a));
        let mut cards=Vec::new();
        if let Some(item)=self.selected.and_then(|id|self.view.items.iter().find(|i|i.id==id)){
            cards.push(Card{icon:None,title:item.title.clone(),hint:item.app.clone(),action:None});
            let body=item.body.chars().collect::<Vec<_>>();
            for line in body.chunks(36){cards.push(Card{icon:None,title:line.iter().collect(),hint:String::new(),action:None});}
            cards.push(Card{icon:None,title:text("Bỏ thông báo","Dismiss notification","清除此通知"),hint:String::new(),
                action:(!crate::trial::active()&&!cfg!(test)&&matches!(self.view.status,1|2)&&item.time>0&&!item.aumid.is_empty()).then(||action(Action::Dismiss(self.generation,item.id))).flatten()});
            cards.push(Card{icon:None,title:text("Trở lại","Back","返回"),hint:String::new(),action:action(Action::Back(self.generation))});
        }else{
            for item in &self.view.items{
                cards.push(Card{icon:None,title:if item.title.is_empty(){item.app.clone()}else{item.title.clone()},
                    hint:format!("{} · {}",item.app,item.body),action:action(Action::Details(self.generation,item.id))});
            }
            if cards.is_empty()&&!self.pending&&self.view.status==1{cards.push(Card{icon:None,title:text("Không có thông báo","No notifications","没有通知"),hint:String::new(),action:None});}
        }
        cards.push(Card{icon:None,title:text("Làm mới","Refresh","刷新"),hint:String::new(),action:action(Action::Refresh(self.generation))});
        cards.push(Card{icon:None,title:text("Quyền thông báo trong Windows","Windows notification permissions","Windows 通知权限"),hint:String::new(),action:action(Action::WindowsSettings)});
        View{title:title(),hint,cards,metrics:None}
    }
}
pub(super) fn detach(popup:HWND){
    PANEL.with(|s|{let mut s=s.borrow_mut();if s.as_ref().is_some_and(|p|p.popup==popup as usize){s.take();}});
}
pub(super) unsafe fn receive(popup:HWND,ticket:usize){
    let update=PANEL.with(|s|{
        let mut s=s.borrow_mut();let p=s.as_mut().filter(|p|p.popup==popup as usize&&p.ticket==ticket)?;
        let incoming=p.work.answer.lock().ok()?.take()?;p.pending=false;p.generation=p.generation.wrapping_add(1);merge(&mut p.view,incoming);
        Some((p.owner,p.ticket,p.view()))
    });
    if let Some((owner,ticket,view))=update{if enabled(){control_center::update(owner as HWND,ticket,view);}}
}
pub(super) unsafe fn perform(owner:HWND,popup:HWND,action:Action){
    if !enabled()||IsWindow(owner)==0{return;}
    if matches!(action,Action::WindowsSettings){
        if !cfg!(test){ShellExecuteW(owner,wide("open").as_ptr(),wide("ms-settings:privacy-notifications").as_ptr(),ptr::null(),ptr::null(),SW_SHOWNORMAL);}return;
    }
    if matches!(action,Action::Open){
        super::utilities::close_panels();
        control_center::show_list_capacity(owner,super::dock_app_anchor(owner,None),View{title:title(),hint:text("Đang đọc…","Reading…","正在读取…"),cards:Vec::new(),metrics:None},66);
        let (Some(window),Some(ticket))=(control_center::window(owner),control_center::ticket(owner))else{return;};
        let mut panel=Panel{owner:owner as usize,popup:window as usize,ticket,generation:1,view:Snapshot::default(),selected:None,pending:false,work:Arc::default()};
        panel.submit(None);let view=panel.view();PANEL.with(|s|s.replace(Some(panel)));control_center::update(owner,ticket,view);return;
    }
    let update=PANEL.with(|s|{
        let mut s=s.borrow_mut();let p=s.as_mut().filter(|p|p.owner==owner as usize&&p.popup==popup as usize&&!p.pending)?;
        match action{
            Action::Refresh(g) if g==p.generation=>{p.submit(None);},
            Action::Back(g) if g==p.generation=>p.selected=None,
            Action::Details(g,id) if g==p.generation=>{if p.view.items.iter().any(|i|i.id==id){p.selected=Some(id);}},
            Action::Dismiss(g,id) if g==p.generation&&!crate::trial::active()&&!cfg!(test)&&matches!(p.view.status,1|2)=>{
                if let Some(item)=p.view.items.iter().find(|i|i.id==id).cloned(){p.submit(Some(item));p.selected=None;}
            },_=>return None,
        }
        Some((p.ticket,p.view()))
    });
    if let Some((ticket,view))=update{control_center::first_page(owner,ticket);control_center::update(owner,ticket,view);}
}

#[cfg(test)]mod tests{
    use super::*;
    fn item(id:u32,time:i64)->Item{Item{id,time,app:"Synthetic app".into(),aumid:"Synthetic.App".into(),title:format!("Notification {id}"),body:"Synthetic notification content".into()}}
    #[test]fn partial_reads_preserve_missing_entries_but_revocation_discards_content(){
        let mut snapshot=Snapshot{status:1,items:vec![item(1,10),item(2,20)],command_failed:false};
        merge(&mut snapshot,Snapshot{status:2,items:vec![item(2,21),item(3,30)],command_failed:false});
        assert_eq!(snapshot.items.iter().map(|i|(i.id,i.time)).collect::<Vec<_>>(),vec![(3,30),(2,21),(1,10)]);
        merge(&mut snapshot,Snapshot{status:3,..Default::default()});assert_eq!(snapshot.items.len(),3);
        merge(&mut snapshot,Snapshot{status:1,items:vec![item(3,30)],command_failed:false});assert_eq!(snapshot.items.len(),1);
        merge(&mut snapshot,Snapshot::default());assert!(snapshot.items.is_empty());
        snapshot.items=(0..64).map(|i|item(i,i64::from(i))).collect();
        merge(&mut snapshot,Snapshot{status:2,items:vec![item(80,90)],command_failed:false});assert_eq!(snapshot.items.len(),64);
    }
    #[cfg(target_env="msvc")]
    #[test]fn dismiss_requires_the_same_observed_id_timestamp_and_application(){unsafe{
        assert!(tlauncher_notifications_identity_fixture());
        assert!(!tlauncher_notifications_remove(ptr::null_mut(),7,123,wide("Synthetic.App").as_ptr(),true));
    }}
    #[cfg(target_env="msvc")]
    #[test]#[ignore="hosted on-demand notification read/lifetime and synthetic native panel; never dismisses Windows notifications"]
    fn native_notification_history_reads_on_demand_and_rejects_stale_actions(){
        if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
        use std::time::{Duration,Instant};
        unsafe{
            let enabled_before=STATE.with(|s|{let mut s=s.borrow_mut();let old=s.prefs.enabled;s.prefs.enabled=true;old});
            struct Reset(bool);impl Drop for Reset{fn drop(&mut self){unsafe{control_center::close_for(ptr::null_mut());}STATE.with(|s|s.borrow_mut().prefs.enabled=self.0);}}let _reset=Reset(enabled_before);
            let owner=CreateWindowExW(WS_EX_TOOLWINDOW,wide("STATIC").as_ptr(),wide("Notifications fixture").as_ptr(),WS_POPUP,
                40,40,160,48,ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),ptr::null());assert!(!owner.is_null());
            struct Owner(HWND);impl Drop for Owner{fn drop(&mut self){unsafe{control_center::close_for(self.0);DestroyWindow(self.0);}}}let _owner=Owner(owner);
            let pump_until=|condition:&dyn Fn()->bool|{let started=Instant::now();while !condition(){
                let mut message:MSG=std::mem::zeroed();for _ in 0..64{if PeekMessageW(&mut message,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&message);DispatchMessageW(&message);}
                assert!(started.elapsed()<Duration::from_secs(8),"bounded notification worker");std::thread::sleep(Duration::from_millis(10));
            }};
            for cycle in 0..3{
                perform(owner,ptr::null_mut(),Action::Open);let popup=control_center::window(owner).unwrap();
                pump_until(&||PANEL.with(|s|s.borrow().as_ref().is_some_and(|p|!p.pending))&&!WORKER.load(Ordering::Acquire));
                let (generation,ticket,shared)=PANEL.with(|s|{let s=s.borrow();let p=s.as_ref().unwrap();
                    println!("FLUIDWIN_NOTIFICATIONS read_status={} content_exported=NONE",p.view.status);
                    (p.generation,p.ticket,p.work.clone())
                });
                // The saved panel contains only synthetic rows, even on a runner
                // that has granted access to unrelated notifications.
                PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().unwrap();p.view=Snapshot{status:1,items:vec![item(7,123)],command_failed:false};});
                perform(owner,popup,Action::Details(generation.wrapping_sub(1),7));assert!(PANEL.with(|s|s.borrow().as_ref().unwrap().selected.is_none()));
                perform(owner,popup,Action::Details(generation,7));assert_eq!(PANEL.with(|s|s.borrow().as_ref().unwrap().selected),Some(7));
                perform(owner,popup,Action::Dismiss(generation,7));assert!(!PANEL.with(|s|s.borrow().as_ref().unwrap().pending),"TEST cannot dismiss a real notification");
                if cycle==0{
                    use windows_sys::Win32::Graphics::{Dwm::DwmFlush,Gdi::*};
                    let painted=Instant::now();while painted.elapsed()<Duration::from_millis(450){
                        let mut message:MSG=std::mem::zeroed();for _ in 0..64{if PeekMessageW(&mut message,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&message);DispatchMessageW(&message);}
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    UpdateWindow(popup);DwmFlush();let mut bounds:RECT=std::mem::zeroed();GetWindowRect(popup,&mut bounds);
                    let (w,h)=(bounds.right-bounds.left,bounds.bottom-bounds.top);let mut layer=super::super::glass_probe::Layer::new(w,h).unwrap();
                    let screen=GetDC(ptr::null_mut());assert_ne!(BitBlt(layer.dc(),0,0,w,h,screen,bounds.left,bounds.top,SRCCOPY|CAPTUREBLT),0);ReleaseDC(ptr::null_mut(),screen);GdiFlush();
                    let rgba=layer.pixels().chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],255]).collect::<Vec<_>>();
                    if let Some(root)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){image::save_buffer(std::path::PathBuf::from(root).join("notifications-synthetic-panel.png"),&rgba,w as u32,h as u32,image::ColorType::Rgba8).unwrap();}
                }
                perform(owner,popup,Action::Back(generation));assert!(PANEL.with(|s|s.borrow().as_ref().unwrap().selected.is_none()));
                control_center::close_for(owner);assert!(PANEL.with(|s|s.borrow().is_none()));assert!(shared.closed.load(Ordering::Acquire));
                receive(popup,ticket);assert!(PANEL.with(|s|s.borrow().is_none()));
            }
            // A closed panel cannot be reopened by a worker already queued.
            perform(owner,ptr::null_mut(),Action::Open);let popup=control_center::window(owner).unwrap();control_center::close_for(owner);
            pump_until(&||!WORKER.load(Ordering::Acquire));assert!(PANEL.with(|s|s.borrow().is_none()));assert_eq!(IsWindow(popup),0);
        }
        println!("FLUIDWIN_NOTIFICATIONS history_panel=PASS stale_generation=REFUSED test_dismiss=REFUSED closed_result=IGNORED worker_teardown=PASS actual_dismiss=NOT_INVOKED banner_badge_silence=NOT_IMPLEMENTED");
    }
}
