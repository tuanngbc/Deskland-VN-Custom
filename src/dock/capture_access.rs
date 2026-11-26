//! Borderless capture consent belongs to Windows, never a guessed session flag.
//! No startup request, registry override, package emulation or permanent worker.
use std::{cell::RefCell,sync::atomic::{AtomicI32,Ordering},time::{Duration,Instant}};
use windows::{core::{factory,Interface},Graphics::Capture::{GraphicsCaptureAccess,GraphicsCaptureAccessKind,IGraphicsCaptureAccessStatics},
    Security::Authorization::AppCapabilityAccess::AppCapabilityAccessStatus};
use windows_future::IAsyncOperation;
use windows_sys::Win32::{Foundation::*,UI::{Shell::ShellExecuteW,WindowsAndMessaging::*}};
use super::{CONTROL,STATE,control_center::{self,Card,View,Command}};
pub(super) const TIMER:usize=34;
const UNKNOWN:i32=-1;const PENDING:i32=-2;const READ_ONLY:i32=-3;const TIMEOUT:i32=0x800705b4u32 as i32;
static STATUS:AtomicI32=AtomicI32::new(UNKNOWN);
#[derive(Clone,Copy)]pub(super) enum Action{Open,Request,WindowsSettings}
struct Pending{op:IAsyncOperation<AppCapabilityAccessStatus>,owner:usize,ticket:usize,started:Instant}
thread_local!{static WAIT:RefCell<Option<Pending>>=const{RefCell::new(None)};}
fn text(vi:&str,en:&str,zh:&str)->String{crate::launcher::localized(vi,en,zh).into()}
fn read_only()->bool{crate::trial::active()||cfg!(test)}
fn request_if<T>(read_only:bool,start:impl FnOnce()->Result<T,i32>)->Result<T,i32>{if read_only{Err(READ_ONLY)}else{start()}}
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
    let pending=WAIT.with(|s|s.borrow_mut().take());if let Some(pending)=pending{let _=pending.op.Cancel();}KillTimer(CONTROL.get(),TIMER);STATUS.store(UNKNOWN,Ordering::Release);
}
pub(super) unsafe fn perform(owner:HWND,action:Action){
    if !STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended})||IsWindow(owner)==0{return;}
    if matches!(action,Action::WindowsSettings){
        if !cfg!(test){ShellExecuteW(owner,crate::desktop::wide("open").as_ptr(),crate::desktop::wide("ms-settings:privacy-graphicscapturewithoutborder").as_ptr(),std::ptr::null(),std::ptr::null(),SW_SHOWNORMAL);}return;
    }
    if WAIT.with(|s|s.borrow().is_some()){return;}
    if matches!(action,Action::Request){
        let result=request_if(read_only(),operation);
        match result{
            Err(error)=>STATUS.store(error,Ordering::Release),
            Ok(op)=>{
                STATUS.store(PENDING,Ordering::Release);control_center::show(owner,super::dock_app_anchor(owner,None),view());
                let Some(ticket)=control_center::ticket(owner)else{let _=op.Cancel();STATUS.store(UNKNOWN,Ordering::Release);return;};
                if SetTimer(CONTROL.get(),TIMER,80,None)==0{let _=op.Cancel();STATUS.store(0x80004005u32 as i32,Ordering::Release);}else{
                    WAIT.with(|s|s.replace(Some(Pending{op,owner:owner as usize,ticket,started:Instant::now()})));return;
                }
            }
        }
    }
    control_center::show(owner,super::dock_app_anchor(owner,None),view());
}
pub(super) unsafe fn poll(){
    let Some(pending)=WAIT.with(|s|s.borrow_mut().take())else{KillTimer(CONTROL.get(),TIMER);return;};
    let owner=pending.owner as HWND;
    if control_center::ticket(owner)!=Some(pending.ticket)||!STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended}){
        let _=pending.op.Cancel();KillTimer(CONTROL.get(),TIMER);STATUS.store(UNKNOWN,Ordering::Release);return;
    }
    let result=match pending.op.Status(){
        Ok(status) if status.0==0&&pending.started.elapsed()<Duration::from_secs(30)=>{WAIT.with(|s|s.replace(Some(pending)));return;},
        Ok(status) if status.0==0=>{let _=pending.op.Cancel();Err(TIMEOUT)},
        Ok(_)=>pending.op.GetResults().map(|value|value.0).map_err(|e|e.code().0),Err(error)=>Err(error.code().0),
    };
    let status=result.unwrap_or_else(|e|e);STATUS.store(status,Ordering::Release);KillTimer(CONTROL.get(),TIMER);
    crate::log!("CAPTURE_ACCESS completed status={status}");control_center::update(owner,pending.ticket,view());
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn test_never_requests_or_guesses_borderless_consent(){
        assert_eq!(request_if::<()>(true,||panic!("TEST must not invoke Windows consent")),Err(READ_ONLY));
        assert_eq!(request_if(false,||Ok(AppCapabilityAccessStatus::DeniedBySystem.0)),Ok(AppCapabilityAccessStatus::DeniedBySystem.0));
        assert_eq!(request_if::<()>(false,||Err(TIMEOUT)),Err(TIMEOUT));
        assert_ne!(label(AppCapabilityAccessStatus::Allowed.0),label(AppCapabilityAccessStatus::DeniedBySystem.0));
    }
}
