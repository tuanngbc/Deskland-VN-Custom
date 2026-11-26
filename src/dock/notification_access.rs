//! Explicit UI-thread Windows notification consent. No startup request,
//! identity registration, permission bypass, background reader or policy write.
use std::{cell::Cell,time::Instant};
use super::permission_result::{outcome,TIMEOUT};
use windows::{core::{factory,Interface},UI::Notifications::Management::{UserNotificationListener,IUserNotificationListenerStatics,UserNotificationListenerAccessStatus}};
use windows_future::IAsyncOperation;
pub(super) const TIMER:usize=37;
const READ_ONLY:i32=0x80070005u32 as i32;
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
