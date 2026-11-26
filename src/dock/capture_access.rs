//! Borderless capture consent belongs to Windows, never a guessed session flag.
//! No startup request, registry override, package emulation or permanent worker.
use std::{cell::{Cell,RefCell},sync::atomic::{AtomicI32,Ordering},time::Instant};
use super::permission_result::{outcome,TIMEOUT};
use windows::{core::{factory,Interface},Graphics::Capture::{GraphicsCaptureAccess,GraphicsCaptureAccessKind,IGraphicsCaptureAccessStatics},
    Security::Authorization::AppCapabilityAccess::AppCapabilityAccessStatus};
use windows_future::IAsyncOperation;
use windows_sys::Win32::{Foundation::*,UI::{Shell::ShellExecuteW,WindowsAndMessaging::*}};
use super::{CONTROL,STATE,control_center::{self,Card,View,Command}};
pub(super) const TIMER:usize=34;
const UNKNOWN:i32=-1;const PENDING:i32=-2;const READ_ONLY:i32=-3;
static STATUS:AtomicI32=AtomicI32::new(UNKNOWN);
#[derive(Clone,Copy)]pub(super) enum Action{Open,Request,WindowsSettings}
struct Pending{op:IAsyncOperation<AppCapabilityAccessStatus>,owner:usize,ticket:usize,revision:u64,started:Instant}
thread_local!{static WAIT:RefCell<Option<Pending>>=const{RefCell::new(None)};static REVISION:Cell<u64>=const{Cell::new(0)};}
fn advance()->u64{let n=REVISION.get().wrapping_add(1);REVISION.set(n);n}
fn current(revision:u64,valid:impl FnOnce()->bool)->bool{REVISION.get()==revision&&valid()}
fn target(owner:HWND,ticket:usize)->bool{control_center::ticket(owner)==Some(ticket)&&STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended})}
unsafe fn clear(revision:u64){if REVISION.get()==revision{KillTimer(CONTROL.get(),TIMER);STATUS.store(UNKNOWN,Ordering::Release);}}
fn text(vi:&str,en:&str,zh:&str)->String{crate::launcher::localized(vi,en,zh).into()}
fn read_only()->bool{crate::trial::active()||cfg!(test)}
fn request_if<T>(read_only:bool,start:impl FnOnce()->Result<T,i32>)->Result<T,i32>{if read_only{Err(READ_ONLY)}else{start()}}
fn start_request<T>(read_only:bool,start:impl FnOnce()->Result<T,i32>)->Result<T,i32>{
    request_if(read_only,||{STATUS.store(PENDING,Ordering::Release);start()})
}
#[cfg(test)]pub(super) fn during_request(probe:impl FnOnce()){
    struct Restore(i32);impl Drop for Restore{fn drop(&mut self){STATUS.store(self.0,Ordering::Release);}}
    let _restore=Restore(STATUS.load(Ordering::Acquire));
    start_request(false,||{probe();Ok(())}).unwrap();
}
fn operation()->Result<IAsyncOperation<AppCapabilityAccessStatus>,i32>{unsafe{
    let factory=factory::<GraphicsCaptureAccess,IGraphicsCaptureAccessStatics>().map_err(|e|e.code().0)?;let mut raw=std::ptr::null_mut();
    (factory.vtable().RequestAccessAsync)(factory.as_raw(),GraphicsCaptureAccessKind::Borderless,&mut raw).ok().map_err(|e|e.code().0)?;
    if raw.is_null(){return Err(0x80004003u32 as i32);}Ok(IAsyncOperation::from_raw(raw))
}}
fn label(status:i32)->(&'static str,&'static str,&'static str){
    match status{
        UNKNOWN=>("Chưa yêu cầu quyền trong phiên này","No permission request in this session","本次会话尚未请求权限"),
        PENDING=>("Đang chờ Windows…","Waiting for Windows…","正在等待 Windows…"),
        READ_ONLY=>("Bản TEST không thay đổi quyền Windows","TEST does not change Windows permissions","TEST 不更改 Windows 权限"),
        n if n==AppCapabilityAccessStatus::Allowed.0=>("Windows đã cấp quyền yêu cầu ẩn viền","Windows allows requesting a hidden border","Windows 允许请求隐藏边框"),
        n if n==AppCapabilityAccessStatus::DeniedBySystem.0=>("Chính sách Windows chưa cho phép","Windows policy does not allow access","Windows 策略未允许访问"),
        n if n==AppCapabilityAccessStatus::NotDeclaredByApp.0=>("Windows chưa cấp quyền cho ứng dụng này","Windows has not granted this app access","Windows 尚未授予此应用访问权限"),
        n if n==AppCapabilityAccessStatus::DeniedByUser.0=>("Quyền chưa được chấp nhận","Permission was declined","权限未获同意"),
        n if n==AppCapabilityAccessStatus::UserPromptRequired.0=>("Windows cần bạn xác nhận quyền","Windows needs your consent","Windows 需要您同意"),
        TIMEOUT=>("Chưa nhận được kết quả từ Windows","No result received from Windows","尚未收到 Windows 的结果"),
        _=>("Quyền này chưa khả dụng trong phiên hiện tại","Permission unavailable in this session","当前会话中此权限不可用"),
    }
}
fn view()->View{
    let status=STATUS.load(Ordering::Acquire);let (vi,en,zh)=label(status);
    View{title:text("Viền chụp cửa sổ","Window capture border","窗口捕获边框"),hint:text("Windows quản lý quyền ẩn viền","Windows controls borderless permission","无边框权限由 Windows 管理"),metrics:None,cards:vec![
        Card{icon:None,title:text(vi,en,zh),hint:String::new(),action:None},
        Card{icon:None,title:text("Yêu cầu quyền từ Windows","Request permission from Windows","向 Windows 请求权限"),hint:if read_only(){text("Không thay đổi quyền trong bản TEST","Permissions stay unchanged in TEST","TEST 不更改权限")}else{String::new()},action:(status!=PENDING&&!read_only()).then_some(Command::CaptureAccess(Action::Request))},
        Card{icon:None,title:text("Cài đặt quyền riêng tư Windows","Windows privacy settings","Windows 隐私设置"),hint:text("Ứng dụng khác vẫn có thể yêu cầu hiện viền","Other apps can still require a visible border","其他应用仍可能要求显示边框"),action:Some(Command::CaptureAccess(Action::WindowsSettings))},
    ]}
}
pub(super) fn diagnostics()->serde_json::Value{let status=STATUS.load(Ordering::Acquire);serde_json::json!({"lastRequestStatus":status,"allowed":status==AppCapabilityAccessStatus::Allowed.0,"effectiveScreenBorderVerified":false})}
pub(super) fn invalidate(){if STATUS.load(Ordering::Acquire)!=PENDING{STATUS.store(UNKNOWN,Ordering::Release);}}
pub(super) fn pending()->bool{STATUS.load(Ordering::Acquire)==PENDING}
pub(super) unsafe fn reset(){
    advance();let pending=WAIT.with(|s|s.borrow_mut().take());
    KillTimer(CONTROL.get(),TIMER);STATUS.store(UNKNOWN,Ordering::Release);
    // Cancellation can reenter the UI. Clear the old request first so its
    // cleanup cannot overwrite a later request started by that reentrancy.
    if let Some(pending)=pending{let _=pending.op.Cancel();}
}
pub(super) unsafe fn perform(owner:HWND,action:Action){
    if !STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended})||IsWindow(owner)==0{return;}
    if matches!(action,Action::WindowsSettings){
        if !cfg!(test){ShellExecuteW(owner,crate::desktop::wide("open").as_ptr(),crate::desktop::wide("ms-settings:privacy-graphicscapturewithoutborder").as_ptr(),std::ptr::null(),std::ptr::null(),SW_SHOWNORMAL);}return;
    }
    if pending()||WAIT.with(|s|s.borrow().is_some()){return;}
    if matches!(action,Action::Request){
        let Some(ticket)=control_center::ticket(owner)else{return;};
        let Some(popup)=control_center::window(owner)else{return;};
        if GetWindowThreadProcessId(popup,std::ptr::null_mut())!=windows_sys::Win32::System::Threading::GetCurrentThreadId(){return;}
        // RequestAccessAsync can synchronously reenter activation handlers.
        // Protect this existing panel before calling Windows, not afterward.
        let revision=advance();let result=start_request(read_only(),operation);
        if !current(revision,||target(owner,ticket)){
            clear(revision);if let Ok(op)=result{let _=op.Cancel();}return;
        }
        match result{
            Err(error)=>STATUS.store(error,Ordering::Release),
            Ok(op)=>{
                if SetTimer(CONTROL.get(),TIMER,80,None)==0{STATUS.store(0x80004005u32 as i32,Ordering::Release);let _=op.Cancel();}else{
                    WAIT.with(|s|s.replace(Some(Pending{op,owner:owner as usize,ticket,revision,started:Instant::now()})));
                }
            }
        }
        if current(revision,||target(owner,ticket)){control_center::update(owner,ticket,view());}return;
    }
    control_center::show(owner,super::dock_app_anchor(owner,None),view());
}
pub(super) unsafe fn poll(){
    let Some(pending)=WAIT.with(|s|s.borrow_mut().take())else{if !self::pending(){KillTimer(CONTROL.get(),TIMER);}return;};
    let owner=pending.owner as HWND;
    if !current(pending.revision,||target(owner,pending.ticket)){
        clear(pending.revision);let _=pending.op.Cancel();return;
    }
    let result=outcome(pending.op.Status().map(|s|s.0).map_err(|e|e.code().0),pending.started.elapsed(),
        ||pending.op.GetResults().map(|value|value.0).map_err(|e|e.code().0),||pending.op.ErrorCode().map(|code|code.0).map_err(|e|e.code().0));
    // Status/GetResults/ErrorCode are native calls and can reset, close or
    // replace the originating panel while WAIT is temporarily detached.
    if !current(pending.revision,||target(owner,pending.ticket)){
        clear(pending.revision);let _=pending.op.Cancel();return;
    }
    let Some(result)=result else{WAIT.with(|s|s.replace(Some(pending)));return;};
    let status=result.unwrap_or_else(|e|e);STATUS.store(status,Ordering::Release);KillTimer(CONTROL.get(),TIMER);
    if result==Err(TIMEOUT){let _=pending.op.Cancel();}
    if current(pending.revision,||target(owner,pending.ticket)){crate::log!("CAPTURE_ACCESS completed status={status}");control_center::update(owner,pending.ticket,view());}
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]#[ignore="hosted native consent timer/reentrant reset ownership; synthetic completion, no Windows consent"]
    fn native_reentrant_completion_preserves_new_request_and_poll_timer(){
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(),Ok("true"));
        unsafe{
            let control=CreateWindowExW(WS_EX_TOOLWINDOW,crate::desktop::wide("STATIC").as_ptr(),crate::desktop::wide("Consent revision fixture").as_ptr(),WS_POPUP,
                0,0,120,40,std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null());assert!(!control.is_null());
            struct Restore{control:HWND,previous:HWND,status:i32}
            impl Drop for Restore{fn drop(&mut self){unsafe{reset();CONTROL.with(|c|c.set(self.previous));STATUS.store(self.status,Ordering::Release);DestroyWindow(self.control);}}}
            let _restore=Restore{control,previous:CONTROL.with(|c|c.replace(control)),status:STATUS.load(Ordering::Acquire)};
            let old=advance();let replacement=Cell::new(0);
            let result=outcome(Ok(1),std::time::Duration::ZERO,||{
                // Model native GetResults reentering shutdown/reset and a new
                // direct request before the old completed value returns.
                reset();replacement.set(advance());start_request(false,||Ok(())).unwrap();
                assert_ne!(SetTimer(control,TIMER,10,None),0);Ok(AppCapabilityAccessStatus::Allowed.0)
            },||panic!("completed getter must not query ErrorCode"));
            assert_eq!(result,Some(Ok(AppCapabilityAccessStatus::Allowed.0)));
            assert!(!current(old,||panic!("old revision must be refused before consulting a replaced panel")));
            clear(old);assert!(current(replacement.get(),||true));assert!(pending(),"old completion cleanup cannot replace the new pending status");
            // A nested poll sees WAIT temporarily detached. It must leave the
            // actual native timer alive, so the outer pending op can return.
            poll();let start=Instant::now();let mut timer:MSG=std::mem::zeroed();
            while PeekMessageW(&mut timer,control,WM_TIMER,WM_TIMER,PM_REMOVE)==0{
                assert!(start.elapsed()<std::time::Duration::from_secs(1),"nested poll killed its owning consent timer");std::thread::sleep(std::time::Duration::from_millis(2));
            }
            assert_eq!(timer.wParam,TIMER);assert!(pending());clear(replacement.get());assert!(!pending());
        }
        println!("CAPTURE_CONSENT_REENTRANCY stale_completed_value=REFUSED replacement_pending=PRESERVED nested_poll_timer=PRESERVED actual_permission=NOT_REQUESTED");
    }
    #[test]fn test_never_requests_or_guesses_borderless_consent(){
        assert_eq!(request_if::<()>(true,||panic!("TEST must not invoke Windows consent")),Err(READ_ONLY));
        assert_eq!(request_if(false,||Ok(AppCapabilityAccessStatus::DeniedBySystem.0)),Ok(AppCapabilityAccessStatus::DeniedBySystem.0));
        assert_eq!(request_if::<()>(false,||Err(TIMEOUT)),Err(TIMEOUT));
        assert_ne!(label(AppCapabilityAccessStatus::Allowed.0),label(AppCapabilityAccessStatus::DeniedBySystem.0));
    }
}
