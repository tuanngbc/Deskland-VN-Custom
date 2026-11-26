//! Original launcher.h Shell verbs, scoped to the current Spotlight UI thread.
use super::Entry;
use windows_sys::Win32::{Foundation::*,UI::{Shell::ShellExecuteW,WindowsAndMessaging::*}};
#[cfg(target_env="msvc")]
extern "C"{fn tlauncher_spotlight_app_menu(owner:HWND,point:POINT,path:*const u16,uninstall:*const u16,invoke:bool)->i32;}
pub(super) fn available(entry:&Entry)->bool{
    if !super::app_entry(entry){return false;}
    let raw=entry.path.to_string_lossy();(entry.path.is_absolute()||raw.starts_with("shell:AppsFolder\\"))
        &&crate::dock::command_pin::decode(&raw).is_none()
}
pub(super) unsafe fn show(owner:HWND,point:POINT,entry:&Entry){
    if !available(entry){return;}
    #[cfg(target_env="msvc")]
    let result=tlauncher_spotlight_app_menu(owner,point,crate::desktop::wide(&entry.path.to_string_lossy()).as_ptr(),
        crate::desktop::wide(crate::launcher::localized("Gỡ cài đặt…","Uninstall…","卸载…")).as_ptr(),!crate::trial::active()&&!cfg!(test));
    #[cfg(not(target_env="msvc"))]let result=-1;
    if result<0{
        // Shell did not provide a usable app identity/menu. Keep the native
        // installed-apps UI available; never guess an uninstall command.
        ShellExecuteW(owner,crate::desktop::wide("open").as_ptr(),crate::desktop::wide("ms-settings:appsfeatures").as_ptr(),std::ptr::null(),std::ptr::null(),SW_SHOWNORMAL);
    }
}
#[cfg(all(test,target_env="msvc"))]mod tests{
    use super::*;use std::ptr;
    #[test]#[ignore="hosted native Shell verb enumeration on a synthetic file; never invokes a Shell command"]
    fn native_app_menu_enumerates_and_releases_without_uninstalling(){
        if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
        unsafe{
            let owner=CreateWindowExW(0,crate::desktop::wide("STATIC").as_ptr(),ptr::null(),WS_POPUP,0,0,1,1,ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),ptr::null());assert!(!owner.is_null());
            let path=std::path::PathBuf::from(std::env::var_os("RUNNER_TEMP").unwrap()).join(format!("app-menu-fixture-{}.txt",std::process::id()));std::fs::write(&path,"Synthetic menu target; never executed").unwrap();
            for _ in 0..3{assert_eq!(tlauncher_spotlight_app_menu(owner,POINT{x:120,y:120},crate::desktop::wide(&path.to_string_lossy()).as_ptr(),crate::desktop::wide("Uninstall").as_ptr(),false),1);}
            std::fs::remove_file(path).unwrap();DestroyWindow(owner);
        }
        println!("FLUIDWIN_APP_MENU Shell_binding=PASS repeated_teardown=PASS actual_uninstall=NOT_INVOKED");
    }
}
