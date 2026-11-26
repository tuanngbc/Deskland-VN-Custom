//! FluidWin's bounded Shell parser/matcher, read on demand. No permanent
//! recent-file index, paths in logs, or execution before a menu selection.
use super::{control_center::{self,Card,View},WindowIdentity,CONTROL};
use crate::desktop::wide;
use std::{cell::RefCell,ffi::c_void,ptr::NonNull,sync::{atomic::{AtomicBool,Ordering},mpsc},time::{Duration,Instant}};
use windows_sys::Win32::{Foundation::*,UI::WindowsAndMessaging::*};
pub(super) const TIMER:usize=23;
static WORKER:AtomicBool=AtomicBool::new(false);
thread_local!{static PENDING:RefCell<Option<Pending>>=const{RefCell::new(None)};}
#[cfg(target_env="msvc")]
extern "C"{
    fn tlauncher_jump_prepare(path:*const u16,hwnd:HWND,pid:u32,tid:u32,other_ids:*const u16)->*mut c_void;
    fn tlauncher_jump_free(model:*mut c_void);
    fn tlauncher_jump_show(model:*mut c_void,owner:HWND,x:i32,y:i32,tasks:*const u16,recent:*const u16,empty:*const u16)->u32;
    fn tlauncher_jump_launch(model:*mut c_void,selected:u32)->bool;
    #[cfg(test)] fn tlauncher_jump_parser_fixture(path:*const u16)->i32;
    #[cfg(test)] fn tlauncher_jump_menu_fixture(path:*const u16,owner:HWND)->u32;
}
struct Model(NonNull<c_void>);
// The prepared object has owned strings/PIDL bytes/pixels, no COM pointers.
unsafe impl Send for Model{}
impl Drop for Model{fn drop(&mut self){#[cfg(target_env="msvc")]unsafe{tlauncher_jump_free(self.0.as_ptr());}}}
impl Model{
    #[cfg(target_env="msvc")]
    unsafe fn launch(self,selected:u32){
        use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
        if CoInitializeEx(None,COINIT_APARTMENTTHREADED).is_ok(){let _=tlauncher_jump_launch(self.0.as_ptr(),selected);CoUninitialize();}
    }
}
struct Pending{owner:usize,ticket:usize,point:POINT,answer:mpsc::Receiver<Option<Model>>,started:Instant}
fn label(vi:&str,en:&str,zh:&str)->String{crate::launcher::localized(vi,en,zh).into()}
pub(super) unsafe fn open(owner:HWND,path:&str,source:Option<WindowIdentity>,other_ids:Vec<String>){
    close();let anchor=super::dock_app_anchor(owner,Some(path));
    let title=label("Lệnh ứng dụng và tệp gần đây","App commands and recent files","应用命令和最近文件");
    control_center::show_list(owner,anchor,View{title:title.clone(),hint:String::new(),cards:vec![Card{icon:None,
        title:label("Đang đọc danh sách Windows…","Reading Windows lists…","正在读取 Windows 列表…"),hint:String::new(),action:None}],metrics:None});
    let Some(ticket)=control_center::ticket(owner)else{return;};
    let control=CONTROL.with(std::cell::Cell::get);
    let failed=||control_center::update(owner,ticket,View{title:title.clone(),hint:String::new(),metrics:None,cards:vec![Card{icon:None,
        title:label("Windows chưa cung cấp danh sách. Thử mở lại sau.","Windows lists are unavailable. Try again shortly.","Windows 列表暂不可用，请稍后重试。"),hint:String::new(),action:None}]});
    if WORKER.swap(true,Ordering::AcqRel){failed();return;}
    let (send,answer)=mpsc::sync_channel(1);let path=wide(path);
    let mut others=Vec::new();for id in other_ids.into_iter().take(256){others.extend(id.encode_utf16().filter(|&v|v!=0));others.push(0);}others.push(0);
    let worker=std::thread::Builder::new().name("TLauncher Jump Lists".into()).spawn(move||{
        struct Permit;impl Drop for Permit{fn drop(&mut self){WORKER.store(false,Ordering::Release);}}
        let _permit=Permit;
        #[cfg(target_env="msvc")]
        unsafe{
            use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
            if CoInitializeEx(None,COINIT_APARTMENTTHREADED).is_err(){let _=send.send(None);return;}
            let id=source.filter(|id|id.matches(id.hwnd as HWND));
            let raw=tlauncher_jump_prepare(path.as_ptr(),id.map_or(std::ptr::null_mut(),|v|v.hwnd as HWND),id.map_or(0,|v|v.pid),id.map_or(0,|v|v.tid),others.as_ptr());
            let result=NonNull::new(raw).map(Model);CoUninitialize();let _=send.send(result);
        }
        #[cfg(not(target_env="msvc"))]
        {let _=(path,source,others);let _=send.send(None);}
    });
    if worker.is_err(){WORKER.store(false,Ordering::Release);failed();return;}
    if SetTimer(control,TIMER,50,None)==0{failed();return;}
    PENDING.with(|p|p.replace(Some(Pending{owner:owner as usize,ticket,point:anchor.point,answer,started:Instant::now()})));
}
pub(super) unsafe fn poll(){
    let pending=PENDING.with(|p|p.borrow_mut().take());let Some(p)=pending else{return;};
    let owner=p.owner as HWND;
    if control_center::ticket(owner)!=Some(p.ticket){KillTimer(CONTROL.with(std::cell::Cell::get),TIMER);return;}
    match p.answer.try_recv(){
        Ok(Some(model))=>{
            KillTimer(CONTROL.with(std::cell::Cell::get),TIMER);control_center::close_for(owner);
            #[cfg(target_env="msvc")]
            {
                let selected=tlauncher_jump_show(model.0.as_ptr(),owner,p.point.x,p.point.y,
                    wide(&label("Lệnh ứng dụng","Tasks","任务")).as_ptr(),wide(&label("Gần đây","Recent","最近")).as_ptr(),
                    wide(&label("Ứng dụng chưa cung cấp danh sách","This app has no available lists","此应用没有可用列表")).as_ptr());
                if selected!=0{
                    let _=std::thread::Builder::new().name("TLauncher Shell command".into()).spawn(move||{
                        model.launch(selected);
                    });
                }
            }
        }
        Err(mpsc::TryRecvError::Empty) if p.started.elapsed()<Duration::from_secs(5)=>{PENDING.with(|slot|slot.replace(Some(p)));}
        _=>{
            KillTimer(CONTROL.with(std::cell::Cell::get),TIMER);
            control_center::update(owner,p.ticket,View{title:label("Lệnh ứng dụng","App commands","应用命令"),hint:String::new(),metrics:None,
                cards:vec![Card{icon:None,title:label("Windows chưa cung cấp danh sách. Thử mở lại sau.","Windows lists are unavailable. Try again shortly.","Windows 列表暂不可用，请稍后重试。"),hint:String::new(),action:None}]});
        }
    }
}
pub(super) unsafe fn close(){PENDING.with(|p|{p.borrow_mut().take();});KillTimer(CONTROL.with(std::cell::Cell::get),TIMER);}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="actual ShellLink stream parsing and keyboard menu selection on hosted Windows; never launches a command"]
fn native_original_parser_preserves_tasks_and_menu_dispatch(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_DOWN,VK_RETURN,VK_ESCAPE};
    unsafe{
        CoInitializeEx(None,COINIT_APARTMENTTHREADED).unwrap();
        let path=std::path::PathBuf::from(std::env::var_os("RUNNER_TEMP").unwrap()).join(format!("synthetic-jumplist-{}.customDestinations-ms",std::process::id()));
        assert_eq!(tlauncher_jump_parser_fixture(wide(&path.to_string_lossy()).as_ptr()),0,"original ShellLink parser, arguments over 4096, truncation rejection");
        let owner=CreateWindowExW(WS_EX_TOOLWINDOW,wide("STATIC").as_ptr(),std::ptr::null(),WS_OVERLAPPEDWINDOW|WS_VISIBLE,20,20,300,200,
            std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null());assert!(!owner.is_null());
        let thread=windows_sys::Win32::System::Threading::GetCurrentThreadId();
        let input=std::thread::spawn(move||{
            for key in [VK_DOWN,VK_RETURN]{std::thread::sleep(Duration::from_millis(250));PostThreadMessageW(thread,WM_KEYDOWN,key as usize,0);}
            std::thread::sleep(Duration::from_secs(2));PostThreadMessageW(thread,WM_KEYDOWN,VK_ESCAPE as usize,0);
        });
        let selected=tlauncher_jump_menu_fixture(wide(&path.to_string_lossy()).as_ptr(),owner);input.join().unwrap();
        DestroyWindow(owner);let _=std::fs::remove_file(path);CoUninitialize();
        assert_eq!(selected,1,"menu dispatches the actual prepared task id; no task was executed");
        println!("FLUIDWIN_JUMPLIST parser=PASS long_arguments=PASS truncated_category=REJECTED native_menu_selection=PASS shell_launch=NOT_EXERCISED");
    }
}
