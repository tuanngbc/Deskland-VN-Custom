//! Windows owns Recycle Bin operations and confirmation. Dock only exposes
//! the original FluidWin routes and delegates its drop target to the Shell.
use super::{CONTROL,STATE,UtilityWidget,control_center::{self,Card,View,Command}};
use std::{cell::RefCell,collections::HashMap,ffi::c_void,ptr::{self,NonNull},sync::{atomic::{AtomicBool,Ordering},mpsc}};
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::ScreenToClient,UI::{Shell::*,WindowsAndMessaging::*}};
pub(super) const TIMER:usize=27;
#[derive(Clone,Copy)]pub(super) enum Action{Menu,Open,Empty}
#[cfg(target_env="msvc")]
extern "C"{
    fn tlauncher_recycle_register(owner:HWND,hit:unsafe extern "C" fn(HWND,i32,i32)->bool)->*mut c_void;
    fn tlauncher_recycle_unregister(raw:*mut c_void);
    #[cfg(test)]fn tlauncher_recycle_registration_fixture(owner:HWND)->i32;
}
struct Registration(NonNull<c_void>);
impl Drop for Registration{fn drop(&mut self){#[cfg(target_env="msvc")]unsafe{tlauncher_recycle_unregister(self.0.as_ptr());}}}
struct Pending{owner:usize,ticket:usize,answer:mpsc::Receiver<bool>}
thread_local!{
    static TARGETS:RefCell<HashMap<usize,Registration>>=RefCell::new(HashMap::new());
    static PENDING:RefCell<Option<Pending>>=const{RefCell::new(None)};
}
static EMPTYING:AtomicBool=AtomicBool::new(false);
unsafe extern "C" fn hit(owner:HWND,x:i32,y:i32)->bool{
    let mut point=POINT{x,y};ScreenToClient(owner,&mut point);
    STATE.with(|s|{
        let Ok(s)=s.try_borrow()else{return false;};if s.suspended{return false;}
        let Some(bar)=s.bars.iter().find(|b|b.hwnd==owner&&!b.hidden&&!b.collapsed)else{return false;};
        bar.layout.as_ref().is_some_and(|l|l.inline_at(point.x as f32,point.y as f32)==Some(UtilityWidget::RecycleBin))
    })
}
pub(super) unsafe fn register(owner:HWND){
    #[cfg(target_env="msvc")]
    if let Some(raw)=NonNull::new(tlauncher_recycle_register(owner,hit)){TARGETS.with(|t|t.borrow_mut().insert(owner as usize,Registration(raw)));}
}
pub(super) fn unregister(owner:HWND){let old=TARGETS.with(|t|t.borrow_mut().remove(&(owner as usize)));drop(old);}
pub(super) fn query()->Option<(u64,u64)>{unsafe{
    let mut info:SHQUERYRBINFO=std::mem::zeroed();info.cbSize=std::mem::size_of::<SHQUERYRBINFO>() as u32;
    (SHQueryRecycleBinW(ptr::null(),&mut info)>=0&&info.i64NumItems>=0&&info.i64Size>=0).then_some((info.i64NumItems as u64,info.i64Size as u64))
}}
fn text(vi:&str,en:&str,zh:&str)->String{crate::launcher::localized(vi,en,zh).into()}
unsafe fn menu(owner:HWND,failed:bool){
    let count=super::utilities::recycle_status();
    let hint=if failed{text("Windows chưa hoàn tất yêu cầu","Windows could not complete the request","Windows 无法完成请求")}
        else{count.map_or_else(String::new,|(n,_)|format!("{n} {}",text("mục","items","项")))};
    let mut cards=vec![Card{title:text("Mở Thùng rác","Open Recycle Bin","打开回收站"),hint:String::new(),icon:Some(UtilityWidget::RecycleBin),action:Some(Command::Recycle(Action::Open))}];
    if count.is_none_or(|(n,_)|n>0){cards.push(Card{title:text("Dọn sạch Thùng rác…","Empty Recycle Bin…","清空回收站…"),
        hint:text("Windows sẽ yêu cầu xác nhận","Windows will ask for confirmation","Windows 将要求确认"),icon:None,action:Some(Command::Recycle(Action::Empty))});}
    control_center::show_list(owner,super::dock_app_anchor(owner,None),View{title:UtilityWidget::RecycleBin.title().into(),hint,cards,metrics:None});
}
pub(super) unsafe fn perform(owner:HWND,action:Action){
    match action{
        Action::Menu=>menu(owner,false),
        Action::Open=>super::open("shell:RecycleBinFolder"),
        Action::Empty=>{
            if EMPTYING.swap(true,Ordering::AcqRel){return;}
            menu(owner,false);let Some(ticket)=control_center::ticket(owner)else{EMPTYING.store(false,Ordering::Release);return;};
            let h=owner as usize;let (send,answer)=mpsc::sync_channel(1);
            let worker=std::thread::Builder::new().name("TLauncher Recycle Bin".into()).spawn(move||{
                let success=unsafe{
                    use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
                    if CoInitializeEx(None,COINIT_APARTMENTTHREADED).is_err(){false}else{
                        // Flags zero retains Windows' own irreversible-action
                        // confirmation. Never call from the hosted fixture.
                        let result=IsWindow(h as HWND)!=0&&SHEmptyRecycleBinW(h as HWND,ptr::null(),0)>=0;
                        CoUninitialize();result
                    }
                };
                let _=send.send(success);EMPTYING.store(false,Ordering::Release);
            });
            if worker.is_err(){EMPTYING.store(false,Ordering::Release);menu(owner,true);return;}
            PENDING.with(|p|p.replace(Some(Pending{owner:h,ticket,answer})));
            SetTimer(CONTROL.with(std::cell::Cell::get),TIMER,200,None);
        }
    }
}
pub(super) unsafe fn poll(){
    let pending=PENDING.with(|p|p.borrow_mut().take());
    if let Some(p)=pending{match p.answer.try_recv(){
        Ok(success)=>{super::request_dock_status();if control_center::ticket(p.owner as HWND)==Some(p.ticket){menu(p.owner as HWND,!success);}},
        Err(mpsc::TryRecvError::Empty)=>{PENDING.with(|slot|slot.replace(Some(p)));},
        Err(_)=>{},
    }}
    if PENDING.with(|p|p.borrow().is_none()){KillTimer(CONTROL.with(std::cell::Cell::get),TIMER);}
}
#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="hosted OLE registration/revocation and rejected drops; no file operations"]
fn native_recycle_target_rejects_unrelated_drops_and_releases(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    unsafe{
        let owner=CreateWindowExW(0,crate::desktop::wide("STATIC").as_ptr(),ptr::null(),WS_POPUP,0,0,80,80,ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),ptr::null());
        assert!(!owner.is_null());
        for _ in 0..4{assert_eq!(tlauncher_recycle_registration_fixture(owner),0);}
        let _=query();DestroyWindow(owner);
    }
    println!("FLUIDWIN_RECYCLE drop_registration=RELEASED unrelated_drop=REJECTED file_operations=NONE");
}
