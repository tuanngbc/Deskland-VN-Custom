//! Existing snapshot MSAA provider; no COM callback calls into Rust or executes
//! a command. UI messages carry a revision checked against the current layout.
use windows_sys::Win32::Foundation::{HWND,RECT,WPARAM,LRESULT};
pub(crate) const SELECT:u32=windows_sys::Win32::UI::WindowsAndMessaging::WM_APP+145;
pub(crate) const CHOOSE:u32=windows_sys::Win32::UI::WindowsAndMessaging::WM_APP+146;
pub(crate) struct Row{pub name:String,pub rect:RECT,pub enabled:bool,pub button:bool,pub action:String}
#[cfg(target_env="msvc")]mod native{
    use super::*;use std::{ffi::c_void,ptr::NonNull};
    #[repr(C)]struct Entry{name:*const u16,rect:RECT,role:i32,state:i32,action:*const u16}
    extern "C"{
        fn tlauncher_control_access_create()->*mut c_void;
        fn tlauncher_control_access_free(raw:*mut c_void);
        fn tlauncher_control_access_close(raw:*mut c_void);
        fn tlauncher_control_access_update(raw:*mut c_void,window:HWND,title:*const u16,rows:*const Entry,count:u32,selected:i32,token:isize);
        fn tlauncher_control_access_object(raw:*mut c_void,parameter:WPARAM)->LRESULT;
        #[cfg(test)]pub(crate) fn tlauncher_control_access_get(window:HWND)->*mut c_void;
        #[cfg(test)]pub(crate) fn tlauncher_control_access_release(raw:*mut c_void);
        #[cfg(test)]pub(crate) fn tlauncher_control_access_probe(raw:*mut c_void,expected:u32,selected:i32,mode:i32)->i32;
    }
    pub(crate) struct Provider(NonNull<c_void>);
    impl Provider{
        pub(crate) fn new()->Option<Self>{NonNull::new(unsafe{tlauncher_control_access_create()}).map(Self)}
        pub(crate) fn update(&self,window:HWND,title:&str,rows:&[Row],selected:Option<usize>,token:usize){
            let names=rows.iter().map(|r|crate::desktop::wide(&r.name)).collect::<Vec<_>>();
            let actions=rows.iter().map(|r|crate::desktop::wide(&r.action)).collect::<Vec<_>>();
            let entries=rows.iter().enumerate().map(|(i,r)|Entry{name:names[i].as_ptr(),rect:r.rect,role:if r.button{0x2b}else{0x29},state:if r.enabled{0}else{1},action:actions[i].as_ptr()}).collect::<Vec<_>>();
            unsafe{tlauncher_control_access_update(self.0.as_ptr(),window,crate::desktop::wide(title).as_ptr(),entries.as_ptr(),entries.len() as u32,selected.map_or(-1,|i|i as i32),token as isize);}
        }
        pub(crate) fn close(&self){unsafe{tlauncher_control_access_close(self.0.as_ptr());}}
        pub(crate) unsafe fn object(&self,w:WPARAM)->LRESULT{tlauncher_control_access_object(self.0.as_ptr(),w)}
    }
    impl Drop for Provider{fn drop(&mut self){unsafe{tlauncher_control_access_free(self.0.as_ptr());}}}
}
#[cfg(target_env="msvc")]pub(crate) use native::Provider;
#[cfg(all(test,target_env="msvc"))]pub(crate) use native::{tlauncher_control_access_get as get,tlauncher_control_access_release as release,tlauncher_control_access_probe as probe};
#[cfg(not(target_env="msvc"))]pub(crate) struct Provider;
#[cfg(not(target_env="msvc"))]impl Provider{
    pub(crate) fn new()->Option<Self>{None}
    pub(crate) fn update(&self,_:HWND,_:&str,_:&[Row],_:Option<usize>,_:usize){}
    pub(crate) fn close(&self){}
    pub(crate) unsafe fn object(&self,_:WPARAM)->LRESULT{0}
}
