//! One recovery owner: foreign visibility leases use the existing Guardian.
use std::{ffi::c_void,ptr::NonNull};
use windows_sys::Win32::{Foundation::*,System::Threading::*};
#[cfg(target_env="msvc")]
extern "C" {
    fn tlauncher_window_lease(hwnd:HWND,guardian:HANDLE,marker:*const u16)->*mut c_void;
    fn tlauncher_window_lease_alpha(hwnd:HWND,guardian:HANDLE,marker:*const u16,alpha:u8)->*mut c_void;
    fn tlauncher_window_lease_topmost(hwnd:HWND,guardian:HANDLE,marker:*const u16)->*mut c_void;
    fn tlauncher_window_lease_poll(lease:*mut c_void)->i32;
    fn tlauncher_window_lease_restore(lease:*mut c_void);
    fn tlauncher_window_lease_free(lease:*mut c_void);
    fn tlauncher_window_restore_session(marker:*const u16)->u32;
    fn tlauncher_window_restore_orphans()->u32;
    #[cfg(test)]fn tlauncher_window_lease_fixture_pause(reached:HANDLE,resume:HANDLE);
}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="hosted pending foreign-style cancellation and journal reservation; synthetic HWND only"]
fn native_pending_style_begin_keeps_journal_until_worker_acknowledges(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use std::{ptr,time::{Duration,Instant}};
    for topmost in [false,true]{unsafe{
        let h=CreateWindowExW(WS_EX_TOOLWINDOW,crate::desktop::wide("STATIC").as_ptr(),crate::desktop::wide("Synthetic pending lease").as_ptr(),
            WS_POPUP,20,20,180,100,ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),ptr::null());assert!(!h.is_null());ShowWindow(h,SW_SHOWNOACTIVATE);
        struct Window(HWND);impl Drop for Window{fn drop(&mut self){unsafe{DestroyWindow(self.0);}}}let _window=Window(h);
        let reached=CreateEventW(ptr::null(),1,0,ptr::null());let resume=CreateEventW(ptr::null(),1,0,ptr::null());assert!(!reached.is_null()&&!resume.is_null());
        struct Pause(HANDLE,HANDLE);impl Drop for Pause{fn drop(&mut self){unsafe{SetEvent(self.1);tlauncher_window_lease_fixture_pause(ptr::null_mut(),ptr::null_mut());CloseHandle(self.0);CloseHandle(self.1);}}}
        let pause=Pause(reached,resume);tlauncher_window_lease_fixture_pause(reached,resume);
        let key=format!("TLauncher.Motion.Fixture.Pending.{}",std::process::id());let marker=crate::desktop::wide(&key);let original=GetWindowLongPtrW(h,GWL_EXSTYLE);
        let begin=if topmost{tlauncher_window_lease_topmost}else{tlauncher_window_lease};
        let lease=Lease(NonNull::new(begin(h,GetCurrentProcess(),marker.as_ptr())).unwrap());
        let pump=||{let mut msg:MSG=std::mem::zeroed();for _ in 0..64{if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&msg);DispatchMessageW(&msg);}std::thread::sleep(Duration::from_millis(5));};
        let started=Instant::now();while WaitForSingleObject(reached,0)!=WAIT_OBJECT_0{pump();assert!(started.elapsed()<Duration::from_secs(3),"worker must reach the controlled pre-style boundary");}
        let marker_key=key.clone();let recovery=std::thread::spawn(move||recover(&marker_key));let started=Instant::now();
        while !recovery.is_finished(){pump();assert!(started.elapsed()<Duration::from_secs(4));}
        assert_eq!(recovery.join().unwrap(),1,"live pending worker must retain an unresolved reserved journal");
        assert_eq!(GetWindowLongPtrW(h,GWL_EXSTYLE),original);
        assert!(!GetPropW(h,marker.as_ptr()).is_null(),"recovery cannot erase a live pending begin");
        let new=begin(h,GetCurrentProcess(),marker.as_ptr());assert!(new.is_null(),"a new lease must not share an old pending operation's identity");
        SetEvent(resume);let started=Instant::now();
        while lease.status()!=2||!GetPropW(h,marker.as_ptr()).is_null(){pump();assert!(started.elapsed()<Duration::from_secs(4),"cancelled begin must acknowledge and finish cleanup");}
        assert_eq!(GetWindowLongPtrW(h,GWL_EXSTYLE),original);assert!(GetPropW(h,crate::desktop::wide(&(key+".pending")).as_ptr()).is_null());
        drop(lease);drop(pause);
    }}
    println!("WINDOW_PENDING_STYLE kinds=visibility,topmost guardian_revoke=PASS journal_held_until_ack=PASS new_lease=REFUSED late_begin=CANCELLED original_style=RESTORED journal=CLEARED");
}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="hosted synthetic HWND topmost lease, exact style preservation and Guardian session cleanup"]
fn native_topmost_lease_preserves_original_state_and_restores_only_its_bit(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use std::{ptr,time::{Duration,Instant}};
    unsafe{
        let h=CreateWindowExW(WS_EX_LAYERED,crate::desktop::wide("STATIC").as_ptr(),crate::desktop::wide("Synthetic topmost journal").as_ptr(),
            WS_OVERLAPPEDWINDOW|WS_VISIBLE,60,60,260,140,ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),ptr::null());assert!(!h.is_null());
        struct Window(HWND);impl Drop for Window{fn drop(&mut self){unsafe{DestroyWindow(self.0);}}}let _window=Window(h);
        assert_ne!(SetLayeredWindowAttributes(h,0x00030201,173,LWA_ALPHA|LWA_COLORKEY),0);
        let original=GetWindowLongPtrW(h,GWL_EXSTYLE);let mut rect=std::mem::zeroed();GetWindowRect(h,&mut rect);
        let pump_until=|condition:&dyn Fn()->bool|{let start=Instant::now();while !condition(){let mut msg:MSG=std::mem::zeroed();for _ in 0..64{if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&msg);DispatchMessageW(&msg);}assert!(start.elapsed()<Duration::from_secs(5));std::thread::sleep(Duration::from_millis(5));}};
        for mode in ["release","drop","guardian"]{
            let marker=format!("TLauncher.Topmost.Fixture.{}.{mode}",std::process::id());let key=crate::desktop::wide(&(marker.clone()+"_Topmost"));
            let lease=Lease(NonNull::new(tlauncher_window_lease_topmost(h,GetCurrentProcess(),key.as_ptr())).unwrap());
            pump_until(&||lease.status()!=0);assert_eq!(lease.status(),1);
            assert_ne!(GetWindowLongPtrW(h,GWL_EXSTYLE)&WS_EX_TOPMOST as isize,0);
            // An unrelated style change made while pinned must survive release.
            SetWindowLongPtrW(h,GWL_EXSTYLE,GetWindowLongPtrW(h,GWL_EXSTYLE)|WS_EX_TOOLWINDOW as isize);
            match mode{
                "release"=>{lease.restore();pump_until(&||lease.status()==2);drop(lease);},
                "drop"=>drop(lease),
                _=>{let helper=std::thread::spawn(move||recover(&marker));pump_until(&||helper.is_finished());assert_eq!(helper.join().unwrap(),0);drop(lease);}
            }
            pump_until(&||GetPropW(h,key.as_ptr()).is_null());
            assert_eq!(GetWindowLongPtrW(h,GWL_EXSTYLE),original|WS_EX_TOOLWINDOW as isize);
            let mut alpha=0;let mut color=0;let mut flags=0;assert_ne!(GetLayeredWindowAttributes(h,&mut color,&mut alpha,&mut flags),0);
            assert_eq!((alpha,color,flags),(173,0x00030201,LWA_ALPHA|LWA_COLORKEY));
            let mut current=std::mem::zeroed();GetWindowRect(h,&mut current);assert!(super::same_rect(rect,current));
            SetWindowLongPtrW(h,GWL_EXSTYLE,original);
        }
        assert_ne!(SetWindowPos(h,HWND_TOPMOST,0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE),0);
        let key=crate::desktop::wide("TLauncher.Topmost.Fixture.AlreadyTopmost");
        assert!(tlauncher_window_lease_topmost(h,GetCurrentProcess(),key.as_ptr()).is_null());
        assert!(GetPropW(h,key.as_ptr()).is_null());assert_ne!(GetWindowLongPtrW(h,GWL_EXSTYLE)&WS_EX_TOPMOST as isize,0);
    }
    println!("WINDOW_TOPMOST release=PASS drop=PASS guardian_session=PASS original_topmost=PRESERVED layered_alpha=PRESERVED unrelated_style=PRESERVED geometry=PRESERVED journal=CLEARED");
}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="hosted released lease retained across replacement, both foreign mutation kinds"]
fn native_old_lease_drop_cannot_revoke_a_replacement_journal(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use std::{ptr,time::{Duration,Instant}};
    unsafe{
        let h=CreateWindowExW(0,crate::desktop::wide("STATIC").as_ptr(),crate::desktop::wide("Synthetic lease epoch").as_ptr(),WS_OVERLAPPEDWINDOW|WS_VISIBLE,
            40,40,240,140,ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),ptr::null());assert!(!h.is_null());
        struct Window(HWND);impl Drop for Window{fn drop(&mut self){unsafe{DestroyWindow(self.0);}}}let _window=Window(h);
        let original=GetWindowLongPtrW(h,GWL_EXSTYLE);
        let pump_until=|condition:&dyn Fn()->bool|{let start=Instant::now();while !condition(){let mut msg:MSG=std::mem::zeroed();for _ in 0..64{if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&msg);DispatchMessageW(&msg);}assert!(start.elapsed()<Duration::from_secs(4));std::thread::sleep(Duration::from_millis(5));}};
        for topmost in [false,true]{
            let begin=if topmost{tlauncher_window_lease_topmost}else{tlauncher_window_lease};let key=crate::desktop::wide(&format!("TLauncher.Fixture.Epoch.{}.{topmost}",std::process::id()));
            let old=Lease(NonNull::new(begin(h,GetCurrentProcess(),key.as_ptr())).unwrap());pump_until(&||old.status()!=0);assert_eq!(old.status(),1);
            old.restore();pump_until(&||old.status()==2&&GetPropW(h,key.as_ptr()).is_null());
            let new=Lease(NonNull::new(begin(h,GetCurrentProcess(),key.as_ptr())).unwrap());pump_until(&||new.status()!=0);assert_eq!(new.status(),1);
            drop(old); // A delayed destructor must not revoke the replacement.
            for _ in 0..8{assert_eq!(new.status(),1);let mut msg:MSG=std::mem::zeroed();while PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}std::thread::sleep(Duration::from_millis(5));}
            assert_ne!(GetWindowLongPtrW(h,GWL_EXSTYLE)&if topmost{WS_EX_TOPMOST as isize}else{WS_EX_LAYERED as isize},0);
            new.restore();pump_until(&||new.status()==2&&GetPropW(h,key.as_ptr()).is_null());drop(new);assert_eq!(GetWindowLongPtrW(h,GWL_EXSTYLE),original);
        }
    }
    println!("WINDOW_LEASE_EPOCH kinds=visibility,topmost old_drop=ISOLATED replacement=ACTIVE original_style=RESTORED journal=CLEARED");
}
pub(super) struct Lease(NonNull<c_void>);
impl Lease{
    pub(super) unsafe fn begin(hwnd:HWND)->Option<Self>{
        Self::begin_alpha(hwnd,0)
    }
    pub(super) unsafe fn begin_interactive(hwnd:HWND)->Option<Self>{Self::begin_alpha(hwnd,1)}
    pub(super) unsafe fn begin_topmost(hwnd:HWND)->Option<Self>{
        #[cfg(target_env="msvc")]{
            let (pid,marker)=super::motion_guard::session()?;
            let process=OpenProcess(0x0010_0000,0,pid);if process.is_null(){return None;}
            let raw=tlauncher_window_lease_topmost(hwnd,process,crate::desktop::wide(&(marker+"_Topmost")).as_ptr());
            CloseHandle(process);NonNull::new(raw).map(Self)
        }
        #[cfg(not(target_env="msvc"))]{let _=hwnd;None}
    }
    unsafe fn begin_alpha(hwnd:HWND,alpha:u8)->Option<Self>{
        #[cfg(target_env="msvc")]
        {
            let (pid,marker)=super::motion_guard::session()?;
            let process=OpenProcess(0x0010_0000,0,pid);
            if process.is_null(){return None;}
            let lease=tlauncher_window_lease_alpha(hwnd,process,crate::desktop::wide(&marker).as_ptr(),alpha);
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
        let topmost_raw=tlauncher_window_lease_topmost(values["hwnd"].as_u64().unwrap() as usize as HWND,guardian,crate::desktop::wide(&(values["marker"].as_str().unwrap().to_owned()+"_Topmost")).as_ptr());
        CloseHandle(guardian);
        let lease=Lease(NonNull::new(raw).unwrap());
        let topmost=Lease(NonNull::new(topmost_raw).unwrap());
        let start=std::time::Instant::now();
        while lease.status()==0||topmost.status()==0{
            assert!(start.elapsed()<std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(lease.status(),1);
        assert_eq!(topmost.status(),1);
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
        let top_key=crate::desktop::wide(&(marker.clone()+"_Topmost"));
        assert!(!GetPropW(hwnd,key.as_ptr()).is_null(),"recovery journal survives owner process termination");
        assert!(!GetPropW(hwnd,top_key.as_ptr()).is_null());
        let helper=std::thread::spawn(move||tlauncher_window_restore_orphans());
        let start=Instant::now();
        while !helper.is_finished(){
            while PeekMessageW(&mut msg,std::ptr::null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}
            assert!(start.elapsed()<Duration::from_secs(5));std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(helper.join().unwrap(),0);
        assert_eq!(GetWindowLongPtrW(hwnd,GWL_EXSTYLE),original);
        assert!(GetPropW(hwnd,key.as_ptr()).is_null());
        assert!(GetPropW(hwnd,top_key.as_ptr()).is_null());
        assert_ne!(IsWindowVisible(hwnd),0);
        DestroyWindow(hwnd);let _=std::fs::remove_file(ready);
        println!("WINDOW_RECOVERY_JOURNAL abrupt_owner_exit=PASS foreign_style=RESTORED identity=VERIFIED original_visibility=PRESERVED guardian_process_tree=NOT_VERIFIED");
    }
}
