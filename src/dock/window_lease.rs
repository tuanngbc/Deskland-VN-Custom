//! One recovery owner: foreign visibility leases use the existing Guardian.
use std::{ffi::c_void,ptr::NonNull};
use windows_sys::Win32::{Foundation::*,System::Threading::*};
#[cfg(target_env="msvc")]
extern "C" {
    fn tlauncher_window_lease(hwnd:HWND,guardian:HANDLE,marker:*const u16)->*mut c_void;
    fn tlauncher_window_lease_poll(lease:*mut c_void)->i32;
    fn tlauncher_window_lease_restore(lease:*mut c_void);
    fn tlauncher_window_lease_free(lease:*mut c_void);
    fn tlauncher_window_restore_session(marker:*const u16)->u32;
}
pub(super) struct Lease(NonNull<c_void>);
impl Lease{
    pub(super) unsafe fn begin(hwnd:HWND)->Option<Self>{
        #[cfg(target_env="msvc")]
        {
            let (pid,marker)=super::TASKBAR_WATCH.with(|w|w.borrow().as_ref().and_then(super::taskbar_guard::Watch::motion_session))?;
            let process=OpenProcess(0x0010_0000,0,pid);
            if process.is_null(){return None;}
            let lease=tlauncher_window_lease(hwnd,process,crate::desktop::wide(&marker).as_ptr());
            CloseHandle(process);NonNull::new(lease).map(Self)
        }
        #[cfg(not(target_env="msvc"))]{let _=hwnd;None}
    }
    pub(super) unsafe fn status(&self)->i32{
        #[cfg(target_env="msvc")]{tlauncher_window_lease_poll(self.0.as_ptr())}
        #[cfg(not(target_env="msvc"))]{-1}
    }
    pub(super) unsafe fn restore(&self){
        #[cfg(target_env="msvc")] tlauncher_window_lease_restore(self.0.as_ptr());
    }
}
impl Drop for Lease{fn drop(&mut self){
    #[cfg(target_env="msvc")]unsafe{tlauncher_window_lease_free(self.0.as_ptr());}
}}
pub(super) unsafe fn recover(marker:&str)->u32{
    #[cfg(target_env="msvc")]{tlauncher_window_restore_session(crate::desktop::wide(marker).as_ptr())}
    #[cfg(not(target_env="msvc"))]{let _=marker;0}
}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="foreign-state journal/worker/recovery exercised only on hosted Windows"]
fn native_lease_restores_style_and_respects_existing_layered_windows(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use std::time::{Duration,Instant};
    unsafe{
        let hwnd=CreateWindowExW(0,crate::desktop::wide("STATIC").as_ptr(),crate::desktop::wide("TLauncher recovery synthetic window").as_ptr(),
            WS_OVERLAPPEDWINDOW|WS_VISIBLE,40,40,240,160,std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null());
        assert!(!hwnd.is_null());
        let before=GetWindowLongPtrW(hwnd,GWL_EXSTYLE);
        let pump_until=|predicate:&dyn Fn()->bool|{
            let start=Instant::now();let mut msg=std::mem::zeroed();
            loop{
                while PeekMessageW(&mut msg,std::ptr::null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}
                if predicate(){break;}
                assert!(start.elapsed()<Duration::from_secs(5),"bounded journal fixture timed out");
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        for mode in ["release","drop","guardian"]{
            let marker=format!("TLauncher.Motion.Fixture.{}.{}",std::process::id(),mode);
            let raw=tlauncher_window_lease(hwnd,GetCurrentProcess(),crate::desktop::wide(&marker).as_ptr());
            let lease=Lease(NonNull::new(raw).expect("journal before mutation"));
            pump_until(&||lease.status()!=0);
            assert_eq!(lease.status(),1);
            let mut alpha=255u8;let mut color=0;let mut flags=0;
            assert_ne!(GetLayeredWindowAttributes(hwnd,&mut color,&mut alpha,&mut flags),0);
            assert_eq!((alpha,flags),(0,LWA_ALPHA));
            match mode{
                "release"=>{lease.restore();pump_until(&||lease.status()==2);drop(lease);}
                "drop"=>drop(lease),
                _=>{
                    // Guardian style removal is asynchronous to this HWND's UI;
                    // keep pumping while its separate recovery thread waits.
                    let marker2=marker.clone();let helper=std::thread::spawn(move||recover(&marker2));
                    pump_until(&||helper.is_finished());
                    assert_eq!(helper.join().unwrap(),0);drop(lease);
                }
            }
            pump_until(&||GetWindowLongPtrW(hwnd,GWL_EXSTYLE)&WS_EX_LAYERED as isize==0);
            assert_eq!(GetWindowLongPtrW(hwnd,GWL_EXSTYLE),before);
            assert!(GetPropW(hwnd,crate::desktop::wide(&marker).as_ptr()).is_null());
        }
        SetWindowLongPtrW(hwnd,GWL_EXSTYLE,before|WS_EX_LAYERED as isize);
        SetLayeredWindowAttributes(hwnd,0x00010203,137,LWA_ALPHA|LWA_COLORKEY);
        let marker=crate::desktop::wide("TLauncher.Motion.Fixture.ExistingLayered");
        assert!(tlauncher_window_lease(hwnd,GetCurrentProcess(),marker.as_ptr()).is_null());
        let mut alpha=0;let mut color=0;let mut flags=0;
        assert_ne!(GetLayeredWindowAttributes(hwnd,&mut color,&mut alpha,&mut flags),0);
        assert_eq!((alpha,color,flags),(137,0x00010203,LWA_ALPHA|LWA_COLORKEY));
        DestroyWindow(hwnd);
        println!("WINDOW_RECOVERY_JOURNAL release=PASS drop=PASS guardian_restore=PASS original_layered_fallback=PASS crash_process_tree=NOT_VERIFIED");
    }
}
