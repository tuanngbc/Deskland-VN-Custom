//! FluidWin keep-awake and countdown on the existing Dock thread.
use super::{control_center::{self,Card,View,Command},CONTROL};
use std::cell::RefCell;
use windows_sys::Win32::{Foundation::*,UI::WindowsAndMessaging::*};
pub(super) const TIMER:usize=26;
#[derive(Clone,Copy)]
pub(super) enum Action{Open,Awake(u64),Countdown(u64),Stop,Add}
#[cfg(target_env="msvc")]
extern "C"{
    fn tlauncher_comfort_command(operation:i32,duration:u64)->bool;
    fn tlauncher_comfort_state(timer:*mut u64,awake:*mut u64)->u32;
    #[cfg(test)] fn tlauncher_comfort_fixture()->i32;
}
thread_local!{static PANEL:RefCell<Option<(usize,usize)>>=const{RefCell::new(None)};}
fn text(vi:&str,en:&str,zh:&str)->String{crate::launcher::localized(vi,en,zh).into()}
fn state()->(u32,u64,u64){
    #[cfg(target_env="msvc")]unsafe{let mut timer=0;let mut awake=0;let flags=tlauncher_comfort_state(&mut timer,&mut awake);(flags,timer,awake)}
    #[cfg(not(target_env="msvc"))]{(0,0,0)}
}
fn duration(ms:u64)->String{let seconds=ms.div_ceil(1000);format!("{}:{:02}",seconds/60,seconds%60)}
fn view(failed:bool)->View{
    let (flags,timer,awake)=state();let mut cards=Vec::new();
    let mut add=|title:String,hint:String,action:Action|cards.push(Card{title,hint,icon:None,action:Some(Command::Comfort(action))});
    let awake_status=if flags&1==0{text("Đang tắt","Off","已关闭")}else if awake==u64::MAX{text("Cho đến khi tắt","Until turned off","直到手动关闭")}else{duration(awake)};
    for (label,ms) in [(text("Giữ máy thức · 1 giờ","Keep awake · 1 hour","保持唤醒 · 1 小时"),3_600_000),
        (text("Giữ máy thức · 2 giờ","Keep awake · 2 hours","保持唤醒 · 2 小时"),7_200_000),
        (text("Giữ máy thức · liên tục","Keep awake · until turned off","保持唤醒 · 直到关闭"),u64::MAX)]{
        add(label,awake_status.clone(),Action::Awake(ms));
    }
    if flags&1!=0{add(text("Tắt giữ máy thức","Turn off keep awake","关闭保持唤醒"),String::new(),Action::Awake(0));}
    let timer_status=if flags&4!=0{text("Hết giờ · chọn Dừng để tắt chuông","Time is up · choose Stop to silence","时间到 · 选择停止以关闭铃声")}
        else if flags&2!=0{duration(timer)}else{text("Chưa đặt giờ","No countdown","未设置倒计时")};
    for (minutes,label) in [(5,text("Hẹn giờ · 5 phút","Countdown · 5 minutes","倒计时 · 5 分钟")),
        (10,text("Hẹn giờ · 10 phút","Countdown · 10 minutes","倒计时 · 10 分钟")),
        (25,text("Hẹn giờ · 25 phút","Countdown · 25 minutes","倒计时 · 25 分钟"))]{add(label,timer_status.clone(),Action::Countdown(minutes*60_000));}
    if flags&2!=0{add(text("Thêm 5 phút","Add 5 minutes","增加 5 分钟"),timer_status.clone(),Action::Add);}
    if flags&6!=0{add(text("Dừng hẹn giờ / Tắt chuông","Stop countdown / Silence alarm","停止倒计时 / 关闭铃声"),timer_status,Action::Stop);}
    View{title:text("Giữ máy thức và Hẹn giờ","Keep awake and Countdown","保持唤醒与倒计时"),
        hint:if failed{text("Windows chưa thực hiện được yêu cầu","Windows could not complete the request","Windows 无法完成请求")}
            else{text("Kết thúc khi thoát hoặc khóa phiên","Ends on exit or session lock","退出或锁定会话时结束")},cards,metrics:None}
}
pub(super) unsafe fn perform(owner:HWND,action:Action){
    #[cfg(target_env="msvc")]
    let success=match action{Action::Open=>true,Action::Awake(ms)=>tlauncher_comfort_command(1,ms),Action::Countdown(ms)=>tlauncher_comfort_command(2,ms),
        Action::Stop=>tlauncher_comfort_command(3,0),Action::Add=>tlauncher_comfort_command(4,300_000)};
    #[cfg(not(target_env="msvc"))]let success=false;
    let anchor=super::dock_app_anchor(owner,None);
    control_center::show_list(owner,anchor,view(!success));
    PANEL.with(|p|p.replace(control_center::ticket(owner).map(|ticket|(owner as usize,ticket))));
    SetTimer(CONTROL.with(std::cell::Cell::get),TIMER,1000,None);
}
pub(super) unsafe fn poll(){
    let (flags,_,_)=state();
    let panel=PANEL.with(|p|*p.borrow());
    if let Some((owner,ticket))=panel{
        if control_center::ticket(owner as HWND)==Some(ticket){control_center::update(owner as HWND,ticket,view(false));}
        else{PANEL.with(|p|p.borrow_mut().take());}
    }
    if flags==0&&PANEL.with(|p|p.borrow().is_none()){KillTimer(CONTROL.with(std::cell::Cell::get),TIMER);}
}
pub(super) unsafe fn reset(){
    #[cfg(target_env="msvc")]{tlauncher_comfort_command(1,0);tlauncher_comfort_command(3,0);}
    PANEL.with(|p|p.borrow_mut().take());KillTimer(CONTROL.with(std::cell::Cell::get),TIMER);
}
#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="original keep-awake and countdown expiration on hosted Windows, no audible alarm"]
fn native_original_awake_and_countdown_expire_and_release(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    assert_eq!(unsafe{tlauncher_comfort_fixture()},0);
    assert_eq!(state().0,0);
    println!("FLUIDWIN_COMFORT awake=RELEASED countdown=EXPIRED extension=PASS alarm=BOUNDED audio=NONE");
}
