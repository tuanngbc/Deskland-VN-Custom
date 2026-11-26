//! On-demand Windows notification history, using original notifiche.h.
//! No listener/poll loop or persisted content exists while the panel is closed.
use super::{STATE,control_center::{self,Card,Command,View}};
use crate::desktop::wide;
use std::{cell::RefCell,ffi::c_void,ptr,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}}};
use windows_sys::Win32::{Foundation::*,System::Threading::{TrySubmitThreadpoolCallback,PTP_CALLBACK_INSTANCE},UI::{Shell::ShellExecuteW,WindowsAndMessaging::*}};
pub(super) const READY:u32=WM_APP+149;
#[derive(Clone)]pub(super) enum Action{Open,Refresh(u64),Request(u64),Details(u64,u32),Dismiss(u64,u32),Clear(u64),Back(u64),WindowsSettings}
#[derive(Clone,Debug,PartialEq,Eq)]struct Item{id:u32,time:i64,app:String,aumid:String,title:String,body:String}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]struct AccessDiagnostic{operation:i32,result:i32,access:i32}
#[derive(Default)]struct Snapshot{status:i32,items:Vec<Item>,command_failed:bool,diagnostic:Option<AccessDiagnostic>}
#[derive(Default)]struct Shared{closed:AtomicBool,answer:Mutex<Option<Snapshot>>}
struct Panel{owner:usize,popup:usize,ticket:usize,generation:u64,view:Snapshot,selected:Option<u32>,pending:bool,work:Arc<Shared>,consent:Option<super::notification_access::Consent>}
impl Drop for Panel{fn drop(&mut self){self.work.closed.store(true,Ordering::Release);if self.consent.is_some(){unsafe{KillTimer(super::CONTROL.get(),super::notification_access::TIMER);}}}}
thread_local!{static PANEL:RefCell<Option<Panel>>=const{RefCell::new(None)};}
static WORKER:AtomicBool=AtomicBool::new(false);
#[cfg(target_env="msvc")]
unsafe extern "C"{
    fn tlauncher_notifications_create()->*mut c_void;fn tlauncher_notifications_free(raw:*mut c_void);
    fn tlauncher_notifications_read(raw:*mut c_void)->i32;fn tlauncher_notifications_count(raw:*mut c_void)->u32;
    fn tlauncher_notifications_diagnostic(raw:*mut c_void,operation:*mut i32,result:*mut i32,access:*mut i32)->bool;
    fn tlauncher_notifications_item(raw:*mut c_void,index:u32,id:*mut u32,time:*mut i64,app:*mut u16,aumid:*mut u16,title:*mut u16,body:*mut u16,size:u32)->bool;
    fn tlauncher_notifications_remove(raw:*mut c_void,id:u32,time:i64,aumid:*const u16,read_only:bool)->bool;
    #[cfg(test)]fn tlauncher_notifications_identity_fixture()->bool;
}
struct Job{popup:usize,ticket:usize,work:Arc<Shared>,remove:Vec<Item>}
fn removal_plan(snapshot:&Snapshot,id:Option<u32>,read_only:bool)->Option<Vec<Item>>{
    if read_only||!matches!(snapshot.status,1|2)||id.is_none()&&snapshot.status!=1{return None;}
    let rows=snapshot.items.iter().filter(|item|id.is_none_or(|id|id==item.id)).cloned().collect::<Vec<_>>();
    if rows.is_empty()||rows.len()>64||rows.iter().any(|item|item.time<=0||item.aumid.is_empty()){return None;}
    let mut seen=std::collections::HashSet::new();
    if !rows.iter().all(|item|seen.insert(item.id)){return None;}Some(rows)
}
fn remove_rows(rows:&[Item],closed:impl Fn()->bool,mut remove:impl FnMut(&Item)->bool)->bool{
    if rows.len()>64{return false;}
    for row in rows{if closed()||!remove(row){return false;}}true
}
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
                view.command_failed=!remove_rows(&job.remove,||job.work.closed.load(Ordering::Acquire),|item|
                    tlauncher_notifications_remove(reader.0,item.id,item.time,wide(&item.aumid).as_ptr(),crate::trial::active()||cfg!(test)));
                if !job.work.closed.load(Ordering::Acquire){
                    view.status=tlauncher_notifications_read(reader.0);
                    let mut diagnostic=AccessDiagnostic::default();
                    if tlauncher_notifications_diagnostic(reader.0,&mut diagnostic.operation,&mut diagnostic.result,&mut diagnostic.access){view.diagnostic=Some(diagnostic);}
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
fn access_hint(diagnostic:Option<AccessDiagnostic>)->String{
    match diagnostic{
        Some(d) if d.result as u32==0x80073d54=>text("Windows cần bản cài đặt đã đăng ký để đọc thông báo","Windows needs a registered app installation to read notifications","Windows 需要已注册的应用安装才能读取通知"),
        Some(d) if d.result>=0&&d.access==2=>text("Quyền đọc thông báo đã bị từ chối trong Windows","Notification access was denied in Windows","Windows 已拒绝通知访问权限"),
        Some(d) if d.result>=0&&d.access==0=>text("Chưa cấp quyền đọc thông báo trong Windows","Notification access has not been granted in Windows","尚未授予 Windows 通知访问权限"),
        _=>text("Quyền truy cập thông báo Windows chưa khả dụng cho ứng dụng này","Windows notification access is unavailable for this app","此应用暂时无法访问 Windows 通知"),
    }
}
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
        self.submit_batch(remove.into_iter().collect());
    }
    unsafe fn submit_batch(&mut self,remove:Vec<Item>){
        if self.pending{return;}self.pending=true;
        if WORKER.swap(true,Ordering::AcqRel){self.pending=false;self.view.status=3;return;}
        let raw=Box::into_raw(Box::new(Job{popup:self.popup,ticket:self.ticket,work:self.work.clone(),remove}));
        if TrySubmitThreadpoolCallback(Some(worker),raw.cast(),ptr::null())==0{
            drop(Box::from_raw(raw));WORKER.store(false,Ordering::Release);self.pending=false;self.view.status=3;
        }
    }
    fn view(&self)->View{
        let hint=if self.consent.is_some(){text("Đang chờ quyền từ Windows…","Waiting for Windows permission…","正在等待 Windows 授权…")}
            else if self.pending{text("Đang đọc…","Reading…","正在读取…")}
            else if self.view.command_failed{text("Chưa bỏ được một số thông báo. Hãy làm mới danh sách.","Some notifications could not be dismissed. Refresh the list.","部分通知无法清除，请刷新列表。")}
            else{match self.view.status{
                0=>access_hint(self.view.diagnostic),
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
            if !self.view.items.is_empty(){cards.push(Card{icon:None,title:text("Bỏ tất cả thông báo","Dismiss all notifications","清除所有通知"),
                hint:if self.view.status==1{format!("{} · {}",self.view.items.len(),text("Trong danh sách này","In this list","当前列表"))}
                    else{text("Làm mới để có danh sách đầy đủ","Refresh to get a complete list","请刷新以获取完整列表")},
                action:removal_plan(&self.view,None,crate::trial::active()||cfg!(test)).and_then(|_|action(Action::Clear(self.generation)))});}
        }
        cards.push(Card{icon:None,title:text("Làm mới","Refresh","刷新"),hint:String::new(),action:action(Action::Refresh(self.generation))});
        if self.view.status==0{
            cards.push(Card{icon:None,title:text("Yêu cầu quyền từ Windows","Request permission from Windows","向 Windows 请求权限"),
                hint:if super::notification_access::read_only(){text("Bản TEST không thay đổi quyền","TEST does not change permissions","TEST 不更改权限")}
                    else{text("Cho phép đọc thông báo để hiển thị trong TLauncher","Allow notification reading for display in TLauncher","允许读取通知以在 TLauncher 中显示")},
                action:(!super::notification_access::read_only()&&self.consent.is_none()).then(||action(Action::Request(self.generation))).flatten()});
        }
        cards.push(Card{icon:None,title:text("Quyền thông báo trong Windows","Windows notification permissions","Windows 通知权限"),hint:String::new(),action:action(Action::WindowsSettings)});
        View{title:title(),hint,cards,metrics:None}
    }
}
pub(super) fn detach(popup:HWND){
    let old=PANEL.with(|s|{let mut s=s.borrow_mut();if s.as_ref().is_some_and(|p|p.popup==popup as usize){s.take()}else{None}});
    drop(old); // Async cancellation runs after releasing the panel borrow.
}
pub(super) fn consent_pending(popup:HWND)->bool{
    super::notification_access::starting(popup as usize)||PANEL.with(|s|s.try_borrow().is_ok_and(|s|s.as_ref().is_some_and(|p|p.popup==popup as usize&&p.consent.is_some())))
}
pub(super) unsafe fn poll_consent(){
    let operation=PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut()?;
        Some((p.owner,p.popup,p.ticket,p.generation,p.consent.take()?))
    });
    let Some((owner,popup,ticket,generation,mut consent))=operation else{KillTimer(super::CONTROL.get(),super::notification_access::TIMER);return;};
    // Status/GetResults can enter COM. Keep neither PANEL nor a panel reference
    // across those calls; the consent's popup marker protects activation.
    let result=consent.poll();let mut returning=Some(consent);
    let update=PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().filter(|p|p.owner==owner&&p.popup==popup&&p.ticket==ticket&&p.generation==generation&&p.pending&&p.consent.is_none())?;
        let Some(result)=result else{p.consent=returning.take();return None;};
        p.pending=false;p.generation=p.generation.wrapping_add(1);p.view.items.clear();p.selected=None;
        p.view.status=0;p.view.diagnostic=Some(match result{Ok(access)=>AccessDiagnostic{operation:8,result:0,access},Err(result)=>AccessDiagnostic{operation:8,result,access:-1}});
        if result==Ok(1)&&enabled(){p.submit(None);}
        Some((p.owner,p.ticket,p.view()))
    });
    drop(returning); // Closed/replaced operations are canceled outside PANEL.
    if !PANEL.with(|s|s.borrow().as_ref().is_some_and(|p|p.consent.is_some())){KillTimer(super::CONTROL.get(),super::notification_access::TIMER);}
    if let Some((owner,ticket,view))=update{if enabled(){control_center::update(owner as HWND,ticket,view);}}
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
    if let Action::Request(generation)=action{
        let current=PANEL.with(|s|s.borrow().as_ref().is_some_and(|p|p.owner==owner as usize&&p.popup==popup as usize&&p.generation==generation&&p.view.status==0&&!p.pending&&p.consent.is_none()));
        if !current||super::notification_access::read_only(){return;}
        // Never hold PANEL across a Windows consent API call: Windows can
        // synchronously reenter activation/focus handlers on this UI thread.
        let operation=super::notification_access::Consent::begin(popup as usize);
        let update=PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().filter(|p|p.owner==owner as usize&&p.popup==popup as usize&&p.generation==generation)?;
            match operation{
                Ok(consent)=>{if SetTimer(super::CONTROL.get(),super::notification_access::TIMER,80,None)!=0{p.consent=Some(consent);p.pending=true;}else{p.view.diagnostic=Some(AccessDiagnostic{operation:8,result:0x80004005u32 as i32,access:-1});}},
                Err(result)=>p.view.diagnostic=Some(AccessDiagnostic{operation:8,result,access:-1}),
            }
            p.generation=p.generation.wrapping_add(1);Some((p.ticket,p.view()))
        });
        if let Some((ticket,view))=update{control_center::update(owner,ticket,view);}return;
    }
    if matches!(action,Action::Open){
        super::utilities::close_panels();
        control_center::show_list_capacity(owner,super::dock_app_anchor(owner,None),View{title:title(),hint:text("Đang đọc…","Reading…","正在读取…"),cards:Vec::new(),metrics:None},67);
        let (Some(window),Some(ticket))=(control_center::window(owner),control_center::ticket(owner))else{return;};
        let mut panel=Panel{owner:owner as usize,popup:window as usize,ticket,generation:1,view:Snapshot::default(),selected:None,pending:false,work:Arc::default(),consent:None};
        panel.submit(None);let view=panel.view();PANEL.with(|s|s.replace(Some(panel)));control_center::update(owner,ticket,view);return;
    }
    let update=PANEL.with(|s|{
        let mut s=s.borrow_mut();let p=s.as_mut().filter(|p|p.owner==owner as usize&&p.popup==popup as usize&&!p.pending)?;
        match action{
            Action::Refresh(g) if g==p.generation=>{p.submit(None);},
            Action::Back(g) if g==p.generation=>p.selected=None,
            Action::Details(g,id) if g==p.generation=>{if p.view.items.iter().any(|i|i.id==id){p.selected=Some(id);}},
            Action::Dismiss(g,id) if g==p.generation=>{
                if let Some(items)=removal_plan(&p.view,Some(id),crate::trial::active()||cfg!(test)){p.submit_batch(items);p.selected=None;}
            },
            Action::Clear(g) if g==p.generation=>{
                if let Some(items)=removal_plan(&p.view,None,crate::trial::active()||cfg!(test)){p.submit_batch(items);p.selected=None;}
            },_=>return None,
        }
        Some((p.ticket,p.view()))
    });
    if let Some((ticket,view))=update{control_center::first_page(owner,ticket);control_center::update(owner,ticket,view);}
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn denied_unspecified_and_registration_failure_remain_distinct(){
        let diagnostic=|result,access|Some(AccessDiagnostic{operation:7,result,access});
        let denied=access_hint(diagnostic(0,2));let unspecified=access_hint(diagnostic(0,0));
        let registration=access_hint(diagnostic(0x80073d54u32 as i32,-1));let failed=access_hint(diagnostic(0x80004005u32 as i32,2));
        assert_ne!(denied,unspecified);assert_ne!(denied,registration);assert_ne!(unspecified,registration);
        assert_ne!(failed,denied);assert_eq!(failed,access_hint(None));
    }
    fn item(id:u32,time:i64)->Item{Item{id,time,app:"Synthetic app".into(),aumid:"Synthetic.App".into(),title:format!("Notification {id}"),body:"Synthetic notification content".into()}}
    #[test]fn dismiss_all_requires_complete_list_valid_identity_and_stops_at_cancellation_or_failure(){
        let mut view=Snapshot{status:1,items:vec![item(1,10),item(2,20)],command_failed:false,..Default::default()};
        assert!(removal_plan(&view,None,true).is_none());let plan=removal_plan(&view,None,false).unwrap();
        // Later arrivals are outside the explicit list the user selected.
        view.items.push(item(3,30));assert_eq!(plan.len(),2);
        let mut removed=Vec::new();assert!(remove_rows(&plan,||false,|row|{removed.push((row.id,row.time,row.aumid.clone()));true}));
        assert_eq!(removed.iter().map(|r|r.0).collect::<Vec<_>>(),vec![1,2]);
        let closed=std::cell::Cell::new(false);let mut calls=0;
        assert!(!remove_rows(&plan,||closed.get(),|_|{calls+=1;closed.set(true);true}));assert_eq!(calls,1);
        calls=0;assert!(!remove_rows(&plan,||false,|_|{calls+=1;false}));assert_eq!(calls,1);
        view.status=2;assert!(removal_plan(&view,None,false).is_none());assert_eq!(removal_plan(&view,Some(2),false).unwrap().len(),1);
        view.status=1;view.items[1].time=0;assert!(removal_plan(&view,None,false).is_none());
        view.items=vec![item(1,10),item(1,11)];assert!(removal_plan(&view,None,false).is_none());
        view.items=vec![item(1,10);65];assert!(removal_plan(&view,None,false).is_none());
        assert!(!remove_rows(&view.items,||false,|_|panic!("oversized batch must not remove anything")));
    }
    #[test]fn partial_reads_preserve_missing_entries_but_revocation_discards_content(){
        let mut snapshot=Snapshot{status:1,items:vec![item(1,10),item(2,20)],command_failed:false,..Default::default()};
        merge(&mut snapshot,Snapshot{status:2,items:vec![item(2,21),item(3,30)],command_failed:false,..Default::default()});
        assert_eq!(snapshot.items.iter().map(|i|(i.id,i.time)).collect::<Vec<_>>(),vec![(3,30),(2,21),(1,10)]);
        merge(&mut snapshot,Snapshot{status:3,..Default::default()});assert_eq!(snapshot.items.len(),3);
        merge(&mut snapshot,Snapshot{status:1,items:vec![item(3,30)],command_failed:false,..Default::default()});assert_eq!(snapshot.items.len(),1);
        merge(&mut snapshot,Snapshot::default());assert!(snapshot.items.is_empty());
        snapshot.items=(0..64).map(|i|item(i,i64::from(i))).collect();
        merge(&mut snapshot,Snapshot{status:2,items:vec![item(80,90)],command_failed:false,..Default::default()});assert_eq!(snapshot.items.len(),64);
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
                    println!("FLUIDWIN_NOTIFICATIONS read_status={} access_diagnostic={:?} content_exported=NONE",p.view.status,p.view.diagnostic);
                    (p.generation,p.ticket,p.work.clone())
                });
                perform(owner,popup,Action::Request(generation.wrapping_sub(1)));perform(owner,popup,Action::Request(generation));
                assert_eq!(PANEL.with(|s|s.borrow().as_ref().unwrap().generation),generation,"TEST must not start consent or change its generation");
                assert!(!consent_pending(popup));assert!(PANEL.with(|s|s.borrow().as_ref().unwrap().consent.is_none()));
                super::super::notification_access::during_request(popup as usize,||{
                    assert!(consent_pending(popup));SendMessageW(popup,WM_ACTIVATE,WA_INACTIVE as usize,0);
                    let mut close:MSG=std::mem::zeroed();assert_eq!(PeekMessageW(&mut close,popup,WM_CLOSE,WM_CLOSE,PM_REMOVE),0,"Windows consent activation cannot close its originating panel");
                    super::super::notification_access::during_request(7,||assert!(super::super::notification_access::starting(7)));
                    assert!(consent_pending(popup),"nested native call must restore its outer popup marker");
                });assert!(!consent_pending(popup));
                super::super::capture_access::during_request(||{
                    assert!(super::super::capture_access::pending());SendMessageW(popup,WM_ACTIVATE,WA_INACTIVE as usize,0);
                    let mut close:MSG=std::mem::zeroed();assert_eq!(PeekMessageW(&mut close,popup,WM_CLOSE,WM_CLOSE,PM_REMOVE),0,"capture permission activation must preserve its originating panel before the async operation exists");
                });assert!(!super::super::capture_access::pending());
                // The saved panel contains only synthetic rows, even on a runner
                // that has granted access to unrelated notifications.
                PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().unwrap();p.view=Snapshot{status:1,items:vec![item(7,123)],command_failed:false,..Default::default()};});
                perform(owner,popup,Action::Details(generation.wrapping_sub(1),7));assert!(PANEL.with(|s|s.borrow().as_ref().unwrap().selected.is_none()));
                perform(owner,popup,Action::Details(generation,7));assert_eq!(PANEL.with(|s|s.borrow().as_ref().unwrap().selected),Some(7));
                perform(owner,popup,Action::Dismiss(generation,7));assert!(!PANEL.with(|s|s.borrow().as_ref().unwrap().pending),"TEST cannot dismiss a real notification");
                perform(owner,popup,Action::Clear(generation.wrapping_sub(1)));perform(owner,popup,Action::Clear(generation));
                assert!(!PANEL.with(|s|s.borrow().as_ref().unwrap().pending),"TEST cannot dismiss all notifications");
                assert_eq!(PANEL.with(|s|s.borrow().as_ref().unwrap().view.items.len()),1);
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
        println!("FLUIDWIN_NOTIFICATIONS history_panel=PASS stale_generation=REFUSED test_dismiss=REFUSED test_clear_all=REFUSED closed_result=IGNORED worker_teardown=PASS actual_dismiss=NOT_INVOKED banner_badge_silence=NOT_IMPLEMENTED");
    }
}
