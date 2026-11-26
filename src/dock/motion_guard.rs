//! The existing detached Guardian in motion-only compatibility mode. Exclusive
//! with its taskbar-hiding role; switching roles waits for the old process.
use super::{taskbar_guard::{Watch,GuardProcess},motion,CONTROL};
use std::cell::{Cell,RefCell};
use windows_sys::Win32::{Foundation::*,UI::WindowsAndMessaging::*};
pub(super) const TIMER:usize=28;
thread_local!{
    static WATCH:RefCell<Option<Watch>>=const{RefCell::new(None)};
    static FAILED:Cell<bool>=const{Cell::new(false)};
}
pub(super) fn session()->Option<(u32,String)>{
    super::TASKBAR_WATCH.with(|w|w.borrow().as_ref().and_then(Watch::motion_session))
        .or_else(||WATCH.with(|w|w.borrow().as_ref().and_then(Watch::motion_session)))
}
pub(super) unsafe fn stop(){
    let old=WATCH.with(|w|w.borrow_mut().take());
    if let Some(watch)=old{
        super::window_motion::cancel();
        super::window_effect::cancel();
        super::TASKBAR_STOPPING.with(|s|s.replace(watch.stop()));
    }
    KillTimer(CONTROL.with(Cell::get),TIMER);
}
pub(super) unsafe fn sync(){
    if cfg!(test){return;}
    let wanted=super::STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended&&
        (s.prefs.jelly_drag||motion::settings().window_effect!=motion::WindowEffect::Windows)})&&motion::allowed();
    if !wanted{super::window_motion::cancel();stop();FAILED.with(|f|f.set(false));return;}
    let taskbar=super::taskbar_active()||super::taskbar_retry_pending()
        ||super::TASKBAR_STATUS.with(|s|s.borrow().desired)||super::TASKBAR_RESTORE.with(|s|s.borrow().is_some());
    if taskbar{stop();return;}
    if FAILED.with(Cell::get){stop();return;}
    if WATCH.with(|w|w.borrow().is_none())&&super::stopping_watch_done(){
        match Watch::start_motion(){
            Ok(watch)=>{WATCH.with(|w|w.replace(Some(watch)));},
            Err(reason)=>{FAILED.with(|f|f.set(true));crate::log!("MOTION_GUARD unavailable: {reason}");return;},
        }
    }
    if SetTimer(CONTROL.with(Cell::get),TIMER,750,None)==0{stop();return;}
    let failed=WATCH.with(|w|w.borrow_mut().as_mut().and_then(|w|w.poll().err()));
    if let Some(reason)=failed{
        FAILED.with(|f|f.set(true));crate::log!("MOTION_GUARD ended: {reason:?}");stop();
    }
}
#[cfg(test)]
pub(super) fn install_fixture(watch:Watch){assert!(super::TASKBAR_WATCH.with(|w|w.borrow().is_none()));WATCH.with(|w|w.replace(Some(watch)));}
#[cfg(test)]
pub(super) fn take_fixture()->Option<GuardProcess>{WATCH.with(|w|w.borrow_mut().take().and_then(Watch::stop))}
#[cfg(test)]
pub(super) fn pulse_fixture(){WATCH.with(|w|if let Some(w)=w.borrow_mut().as_mut(){w.poll().expect("compatibility Guardian stays healthy");});}
