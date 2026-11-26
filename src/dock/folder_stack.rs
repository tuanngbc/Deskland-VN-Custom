//! Original FluidWin folder stack, prepared off-thread and hosted on the Dock
//! STA. Its own Windows IDataObject/DoDragDrop path provides file dragging.
use super::{control_center::{self,View,Card},panel_chrome::Anchor,CONTROL,DockSide};
use crate::desktop::wide;
use std::{cell::RefCell,ffi::c_void,ptr::NonNull,sync::{atomic::{AtomicBool,Ordering},mpsc},time::{Duration,Instant}};
use windows_sys::Win32::{Foundation::*,UI::WindowsAndMessaging::*};
pub(super) const TIMER:usize=24;
static WORKER:AtomicBool=AtomicBool::new(false);
thread_local!{static PENDING:RefCell<Option<Pending>>=const{RefCell::new(None)};static OPEN:RefCell<Option<Stack>>=const{RefCell::new(None)};}
#[cfg(target_env="msvc")]
extern "C"{
    fn tlauncher_stack_prepare(fixture_folder:*const u16)->*mut c_void;
    fn tlauncher_stack_open(raw:*mut c_void,icon:*const RECT,side:i32,title:*const u16,empty:*const u16,explorer:*const u16,factor:f64,reduced:bool)->bool;
    fn tlauncher_stack_window(raw:*mut c_void)->HWND;
    fn tlauncher_stack_active(raw:*mut c_void)->bool;
    fn tlauncher_stack_free(raw:*mut c_void);
    #[cfg(test)] fn tlauncher_stack_count(raw:*mut c_void)->u32;
    #[cfg(test)] fn tlauncher_stack_thumbnails_idle()->bool;
}
struct Prepared(NonNull<c_void>);
// Prepared has only directory strings and file records; no HWND/COM graphics
// until the Dock thread opens it. Active Stack never leaves that thread.
unsafe impl Send for Prepared{}
impl Drop for Prepared{fn drop(&mut self){#[cfg(target_env="msvc")]unsafe{tlauncher_stack_free(self.0.as_ptr());}}}
struct Stack{model:Prepared,owner:usize}
struct Pending{answer:mpsc::Receiver<Option<Prepared>>,owner:usize,ticket:usize,anchor:Anchor,started:Instant}
fn text(vi:&str,en:&str,zh:&str)->String{crate::launcher::localized(vi,en,zh).into()}
pub(super) unsafe fn open(owner:HWND,anchor:Anchor){
    super::utilities::close_panels();
    control_center::show_list(owner,anchor,View{title:text("Tải xuống","Downloads","下载"),hint:String::new(),cards:vec![Card{icon:None,
        title:text("Đang đọc thư mục…","Reading folder…","正在读取文件夹…"),hint:String::new(),action:None}],metrics:None});
    let Some(ticket)=control_center::ticket(owner)else{return;};
    let fail=||control_center::update(owner,ticket,unavailable());
    if WORKER.swap(true,Ordering::AcqRel){fail();return;}
    let (send,answer)=mpsc::sync_channel(1);
    let worker=std::thread::Builder::new().name("TLauncher Downloads Stack".into()).spawn(move||{
        struct Permit;impl Drop for Permit{fn drop(&mut self){WORKER.store(false,Ordering::Release);}}
        let _permit=Permit;
        #[cfg(target_env="msvc")]
        unsafe{let _=send.send(NonNull::new(tlauncher_stack_prepare(std::ptr::null())).map(Prepared));}
        #[cfg(not(target_env="msvc"))]let _=send.send(None);
    });
    if worker.is_err(){WORKER.store(false,Ordering::Release);fail();return;}
    if SetTimer(CONTROL.with(std::cell::Cell::get),TIMER,70,None)==0{fail();return;}
    PENDING.with(|p|p.replace(Some(Pending{answer,owner:owner as usize,ticket,anchor,started:Instant::now()})));
}
fn unavailable()->View{View{title:text("Tải xuống","Downloads","下载"),hint:String::new(),metrics:None,
    cards:vec![Card{icon:None,title:text("Thư mục chưa khả dụng hoặc đã chuyển tới ổ mạng","Folder unavailable or redirected to a network drive","文件夹不可用或已重定向到网络驱动器"),hint:String::new(),action:None}]}}
pub(super) unsafe fn poll(){
    let pending=PENDING.with(|p|p.borrow_mut().take());
    if let Some(p)=pending{
        let owner=p.owner as HWND;
        if control_center::ticket(owner)==Some(p.ticket){
            match p.answer.try_recv(){
                Ok(Some(model))=>{
                    control_center::close_for(owner);
                    #[cfg(target_env="msvc")]
                    {
                        let point=p.anchor.point;let icon=RECT{left:point.x-16,top:point.y-16,right:point.x+16,bottom:point.y+16};
                        let side=match p.anchor.side{DockSide::Bottom=>0,DockSide::Left=>2,DockSide::Right=>3};
                        if tlauncher_stack_open(model.0.as_ptr(),&icon,side,wide(&text("Tải xuống","Downloads","下载")).as_ptr(),
                            wide(&text("Thư mục trống","The folder is empty","文件夹为空")).as_ptr(),wide(&text("Mở trong File Explorer","Open in File Explorer","在文件资源管理器中打开")).as_ptr(),
                            f64::from(super::motion::settings().speed)/100.0,!super::motion::allowed()){
                            OPEN.with(|s|s.replace(Some(Stack{model,owner:p.owner})));
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) if p.started.elapsed()<Duration::from_secs(5)=>{PENDING.with(|s|s.replace(Some(p)));}
                _=>control_center::update(owner,p.ticket,unavailable()),
            }
        }
    }
    #[cfg(target_env="msvc")]
    OPEN.with(|s|{
        let closed=s.borrow().as_ref().is_some_and(|s|!tlauncher_stack_active(s.model.0.as_ptr()));
        if closed{let old=s.borrow_mut().take();if let Some(old)=old{super::panel_closed(old.owner as HWND);drop(old);}}
    });
    if PENDING.with(|p|p.borrow().is_none())&&OPEN.with(|p|p.borrow().is_none()){KillTimer(CONTROL.with(std::cell::Cell::get),TIMER);}
}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="original D2D folder stack on a synthetic local directory on hosted Windows; no file execution or drag injection"]
fn native_original_stack_reads_navigates_and_releases(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_HOME,VK_END,VK_TAB,VK_ESCAPE};
    use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
    unsafe{
        CoInitializeEx(None,COINIT_APARTMENTTHREADED).unwrap();
        let dir=std::path::PathBuf::from(std::env::var_os("RUNNER_TEMP").unwrap()).join(format!("stack-fixture-{}",std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..15{std::fs::write(dir.join(format!("Synthetic file {i}.txt")),"Folder stack fixture; never executed").unwrap();}
        let pump_until=|condition:&dyn Fn()->bool|{
            let started=Instant::now();let mut msg:MSG=std::mem::zeroed();
            while !condition(){
                while PeekMessageW(&mut msg,std::ptr::null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}
                assert!(started.elapsed()<Duration::from_secs(8),"stack teardown timed out");std::thread::sleep(Duration::from_millis(10));
            }
        };
        for _ in 0..4{
            let prepared=std::thread::spawn({let path=wide(&dir.to_string_lossy());move||NonNull::new(tlauncher_stack_prepare(path.as_ptr())).map(Prepared)}).join().unwrap().unwrap();
            assert_eq!(tlauncher_stack_count(prepared.0.as_ptr()),12);
            let anchor=RECT{left:440,top:690,right:480,bottom:730};
            assert!(tlauncher_stack_open(prepared.0.as_ptr(),&anchor,0,wide("Downloads fixture").as_ptr(),wide("Empty").as_ptr(),wide("Open folder").as_ptr(),0.70,false));
            let window=tlauncher_stack_window(prepared.0.as_ptr());assert!(!window.is_null());
            for key in [VK_HOME,VK_END,VK_TAB,VK_ESCAPE]{PostMessageW(window,WM_KEYDOWN,key as usize,0);}
            pump_until(&||!tlauncher_stack_active(prepared.0.as_ptr()));
            drop(prepared);assert_eq!(IsWindow(window),0);pump_until(&||tlauncher_stack_thumbnails_idle());
        }
        for i in 0..15{std::fs::remove_file(dir.join(format!("Synthetic file {i}.txt"))).unwrap();}std::fs::remove_dir(dir).unwrap();
        CoUninitialize();
    }
    println!("FLUIDWIN_DOWNLOADS_STACK count=12 keyboard=PASS repeated_teardown=PASS native_drag_path=IMPLEMENTED drag_interaction=NOT_VERIFIED");
}
pub(super) fn open_for(owner:HWND)->bool{OPEN.with(|s|s.borrow().as_ref().is_some_and(|s|s.owner==owner as usize))}
pub(super) unsafe fn close_for(owner:HWND){
    PENDING.with(|p|{let mut p=p.borrow_mut();if owner.is_null()||p.as_ref().is_some_and(|p|p.owner==owner as usize){p.take();}});
    let old=OPEN.with(|s|{let mut s=s.borrow_mut();if owner.is_null()||s.as_ref().is_some_and(|s|s.owner==owner as usize){s.take()}else{None}});
    drop(old);
    if PENDING.with(|p|p.borrow().is_none())&&OPEN.with(|p|p.borrow().is_none()){KillTimer(CONTROL.with(std::cell::Cell::get),TIMER);}
}
