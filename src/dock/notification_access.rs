//! Explicit UI-thread Windows notification consent. No startup request,
//! identity registration, permission bypass, background reader or policy write.
use std::{cell::Cell,time::{Duration,Instant}};
use windows::{core::{factory,Interface},UI::Notifications::Management::{UserNotificationListener,IUserNotificationListenerStatics,UserNotificationListenerAccessStatus}};
use windows_future::IAsyncOperation;
pub(super) const TIMER:usize=37;
const READ_ONLY:i32=0x80070005u32 as i32;
const TIMEOUT:i32=0x800705b4u32 as i32;
thread_local!{static STARTING:Cell<usize>=const{Cell::new(0)};}
pub(super) fn starting(popup:usize)->bool{STARTING.get()==popup&&popup!=0}
pub(super) fn read_only()->bool{crate::trial::active()||cfg!(test)}
fn request_if<T>(blocked:bool,request:impl FnOnce()->Result<T,i32>)->Result<T,i32>{if blocked{Err(READ_ONLY)}else{request()}}
fn listener()->Result<UserNotificationListener,i32>{unsafe{
    // Scoped factory rather than the generated process-wide static cache.
    let statics=factory::<UserNotificationListener,IUserNotificationListenerStatics>().map_err(|e|e.code().0)?;let mut raw=std::ptr::null_mut();
    (statics.vtable().Current)(statics.as_raw(),&mut raw).ok().map_err(|e|e.code().0)?;
    if raw.is_null(){return Err(0x80004003u32 as i32);}Ok(UserNotificationListener::from_raw(raw))
}}
pub(super) struct Consent{operation:IAsyncOperation<UserNotificationListenerAccessStatus>,_listener:UserNotificationListener,started:Instant,finished:bool}
fn outcome(status:Result<i32,i32>,elapsed:Duration,results:impl FnOnce()->Result<i32,i32>,error:impl FnOnce()->Result<i32,i32>)->Option<Result<i32,i32>>{
    match status{
        Ok(0) if elapsed<Duration::from_secs(30)=>None,
        Ok(0)=>Some(Err(TIMEOUT)),
        Ok(1)=>Some(results()),
        Ok(2)=>Some(Err(0x80004004u32 as i32)),
        // Error is distinct from Canceled. Preserve the async operation's
        // actual HRESULT, including a failure while reading ErrorCode.
        Ok(3)=>Some(Err(match error(){Ok(code)|Err(code)=>if code<0{code}else{0x8000ffffu32 as i32}})),
        Ok(_)=>Some(Err(0x8000ffffu32 as i32)),
        Err(error)=>Some(Err(error)),
    }
}
impl Consent{
    pub(super) fn begin(popup:usize)->Result<Self,i32>{
        request_if(read_only(),||{
            unsafe{use windows_sys::Win32::{System::Threading::GetCurrentThreadId,UI::WindowsAndMessaging::GetWindowThreadProcessId};
                if popup==0||GetWindowThreadProcessId(popup as _,std::ptr::null_mut())!=GetCurrentThreadId(){return Err(0x8001010eu32 as i32);}
            }
            STARTING.set(popup);struct Starting;impl Drop for Starting{fn drop(&mut self){STARTING.set(0);}}let _starting=Starting;
            let listener=listener()?;
            let operation=listener.RequestAccessAsync().map_err(|e|e.code().0)?;
            Ok(Self{operation,_listener:listener,started:Instant::now(),finished:false})
        })
    }
    pub(super) fn poll(&mut self)->Option<Result<i32,i32>>{
        let result=outcome(self.operation.Status().map(|s|s.0).map_err(|e|e.code().0),self.started.elapsed(),||self.operation.GetResults().map(|s|s.0).map_err(|e|e.code().0),||self.operation.ErrorCode().map(|code|code.0).map_err(|e|e.code().0))?;
        if result==Err(TIMEOUT){let _=self.operation.Cancel();}
        self.finished=true;Some(result)
    }
}
impl Drop for Consent{fn drop(&mut self){if !self.finished{let _=self.operation.Cancel();}}}
#[cfg(test)]pub(super) fn during_request(popup:usize,probe:impl FnOnce()){
    STARTING.set(popup);struct Clear;impl Drop for Clear{fn drop(&mut self){STARTING.set(0);}}let _clear=Clear;probe();
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn test_and_trial_cannot_invoke_notification_consent(){
        assert_eq!(request_if::<()>(true,||panic!("read-only must refuse before activation/request")),Err(READ_ONLY));
        assert!(Consent::begin(7).is_err());assert!(!starting(7));
        for result in [Ok(0),Ok(1),Ok(2),Err(TIMEOUT)]{assert_eq!(request_if(false,||result),result);}
        let no_results=||panic!("only Completed may call GetResults");
        let no_error=||panic!("only Error may call ErrorCode");
        assert_eq!(outcome(Ok(0),Duration::from_secs(2),no_results,no_error),None);
        assert_eq!(outcome(Ok(0),Duration::from_secs(30),no_results,no_error),Some(Err(TIMEOUT)));
        for result in [Ok(0),Ok(1),Ok(2),Err(0x80073d54u32 as i32)]{assert_eq!(outcome(Ok(1),Duration::ZERO,||result,no_error),Some(result));}
        assert_eq!(outcome(Ok(2),Duration::ZERO,no_results,no_error),Some(Err(0x80004004u32 as i32)));
        for code in [0x80070005u32 as i32,0x80073d54u32 as i32]{
            assert_eq!(outcome(Ok(3),Duration::ZERO,no_results,||Ok(code)),Some(Err(code)));
            assert_eq!(outcome(Ok(3),Duration::ZERO,no_results,||Err(code)),Some(Err(code)));
        }
        assert_eq!(outcome(Ok(3),Duration::ZERO,no_results,||Ok(0)),Some(Err(0x8000ffffu32 as i32)));
        assert_eq!(outcome(Ok(99),Duration::ZERO,no_results,no_error),Some(Err(0x8000ffffu32 as i32)));
        assert_eq!(outcome(Err(0x80004005u32 as i32),Duration::ZERO,no_results,no_error),Some(Err(0x80004005u32 as i32)));
    }
    #[test]#[ignore="hosted read-only scoped notification listener status, no RequestAccessAsync or notification content"]
    fn native_access_preflight_retains_exact_status_without_consent(){
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(),Ok("true"));
        unsafe{use windows::Win32::System::WinRT::{RoInitialize,RoUninitialize,RO_INIT_SINGLETHREADED};
            RoInitialize(RO_INIT_SINGLETHREADED).unwrap();struct Apartment;impl Drop for Apartment{fn drop(&mut self){unsafe{RoUninitialize();}}}let _apartment=Apartment;
            for _ in 0..4{let status=listener().and_then(|l|l.GetAccessStatus().map(|s|s.0).map_err(|e|e.code().0));println!("NOTIFICATION_ACCESS_PREFLIGHT {status:?} consent=NOT_INVOKED content=NOT_READ");if let Ok(status)=status{assert!((0..=2).contains(&status));}}
            assert!(Consent::begin(123).is_err());assert!(!starting(123));
        }
    }
}
