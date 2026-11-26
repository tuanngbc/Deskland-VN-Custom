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
    fn tlauncher_window_restore_orphans()->u32;
}
pub(super) struct Lease(NonNull<c_void>);
impl Lease{
    pub(super) unsafe fn begin(hwnd:HWND)->Option<Self>{
        #[cfg(target_env="msvc")]
        {
            let (pid,marker)=super::motion_guard::session()?;
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
pub(super) fn recover_orphans_once(){
    static ONCE:std::sync::Once=std::sync::Once::new();
    ONCE.call_once(||{
        let _=std::thread::Builder::new().name("TLauncher stale window recovery".into()).spawn(||{
            #[cfg(target_env="msvc")]
            unsafe{let unresolved=tlauncher_window_restore_orphans();if unresolved!=0{crate::log!("WINDOW_RECOVERY unresolved_orphans={unresolved}");}}
        });
    });
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
            pump_until(&||GetWindowLongPtrW(hwnd,GWL_EXSTYLE)&WS_EX_LAYERED as isize==0
                &&GetPropW(hwnd,crate::desktop::wide(&marker).as_ptr()).is_null());
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

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="synthetic lease owner started only by the hosted crash recovery fixture"]
fn native_lease_owner_fixture(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    let Ok(request)=std::env::var("TUANPC_WINDOW_LEASE_FIXTURE")else{return;};
    let values:serde_json::Value=serde_json::from_str(&request).unwrap();
    unsafe{
        let guardian=OpenProcess(0x0010_0000,0,values["guardian"].as_u64().unwrap() as u32);
        let key=crate::desktop::wide(values["marker"].as_str().unwrap());
        let raw=tlauncher_window_lease(values["hwnd"].as_u64().unwrap() as usize as HWND,guardian,key.as_ptr());
        CloseHandle(guardian);
        let lease=Lease(NonNull::new(raw).unwrap());
        let start=std::time::Instant::now();
        while lease.status()==0{
            assert!(start.elapsed()<std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(lease.status(),1);
        std::fs::write(values["ready"].as_str().unwrap(),"hidden-and-journaled").unwrap();
        loop{std::thread::sleep(std::time::Duration::from_secs(1));}
    }
}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="abrupt synthetic process death and persistent foreign-state journal on hosted Windows"]
fn native_crash_recovers_foreign_window_without_owner_memory(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use std::{process::{Child,Command,Stdio},time::{Duration,Instant}};
    struct ChildFixture(Child);
    impl Drop for ChildFixture{fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
    unsafe{
        let hwnd=CreateWindowExW(0,crate::desktop::wide("STATIC").as_ptr(),crate::desktop::wide("Synthetic foreign crash target").as_ptr(),WS_OVERLAPPEDWINDOW|WS_VISIBLE,
            50,50,280,180,std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null());
        assert!(!hwnd.is_null());
        let original=GetWindowLongPtrW(hwnd,GWL_EXSTYLE);
        let ready=std::path::PathBuf::from(std::env::var_os("RUNNER_TEMP").unwrap()).join(format!("window-lease-ready-{}.txt",std::process::id()));
        let marker=format!("Local\\TuanpcDesk_Taskbar_{}_{:032x}_Motion",std::process::id(),std::process::id());
        let request=serde_json::json!({"guardian":std::process::id(),"marker":marker,"hwnd":hwnd as usize,"ready":ready});
        let mut owner=ChildFixture(Command::new(std::env::current_exe().unwrap())
            .args(["--ignored","--exact","dock::window_lease::native_lease_owner_fixture","--nocapture"])
            .env("TUANPC_WINDOW_LEASE_FIXTURE",request.to_string()).stdout(Stdio::inherit()).spawn().unwrap());
        let start=Instant::now();let mut msg=std::mem::zeroed();
        while !ready.is_file(){
            while PeekMessageW(&mut msg,std::ptr::null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}
            assert!(start.elapsed()<Duration::from_secs(8),"synthetic owner could not acquire journaled lease");
            assert!(owner.0.try_wait().unwrap().is_none());
            std::thread::sleep(Duration::from_millis(5));
        }
        owner.0.kill().unwrap();owner.0.wait().unwrap(); // only this known child fixture, never an application
        let key=crate::desktop::wide(&marker);
        assert!(!GetPropW(hwnd,key.as_ptr()).is_null(),"recovery journal survives owner process termination");
        let helper=std::thread::spawn(move||tlauncher_window_restore_orphans());
        let start=Instant::now();
        while !helper.is_finished(){
            while PeekMessageW(&mut msg,std::ptr::null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}
            assert!(start.elapsed()<Duration::from_secs(5));std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(helper.join().unwrap(),0);
        assert_eq!(GetWindowLongPtrW(hwnd,GWL_EXSTYLE),original);
        assert!(GetPropW(hwnd,key.as_ptr()).is_null());
        assert_ne!(IsWindowVisible(hwnd),0);
        DestroyWindow(hwnd);let _=std::fs::remove_file(ready);
        println!("WINDOW_RECOVERY_JOURNAL abrupt_owner_exit=PASS foreign_style=RESTORED identity=VERIFIED original_visibility=PRESERVED guardian_process_tree=NOT_VERIFIED");
    }
}
