//! Original sensori.h battery thresholds, driven by Windows power events.
//! Read-only: no polling loop, power-plan write, wake lock or worker.
use std::cell::RefCell;
use windows_sys::Win32::{Foundation::*,System::Power::{GetSystemPowerStatus,SYSTEM_POWER_STATUS}};
#[link(name="user32")]extern "system"{
    #[link_name="RegisterPowerSettingNotification"]fn subscribe(hwnd:HWND,guid:*const windows_sys::core::GUID,flags:u32)->HANDLE;
    #[link_name="UnregisterPowerSettingNotification"]fn unsubscribe(handle:HANDLE)->i32;
}
// Documented Windows power event IDs: https://learn.microsoft.com/en-us/windows/win32/power/power-setting-guids
const AC:windows_sys::core::GUID=windows_sys::core::GUID::from_u128(0x5d3e9a59_e9d5_4b00_a6bd_ff34ff516548);
const LEVEL:windows_sys::core::GUID=windows_sys::core::GUID::from_u128(0xa7ad8041_b45a_4cae_87a3_eecbb468a9e1);
#[derive(Clone,Copy,Debug,PartialEq,Eq)]struct Reading{level:u8,ac:bool}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]enum Notice{Charging,Unplugged,Low}
#[derive(Default)]struct Tracker{previous:Option<Reading>,threshold:u8}
impl Tracker{
    fn observe(&mut self,reading:Option<Reading>)->Vec<Notice>{
        let Some(reading)=reading else{self.previous=None;return Vec::new();};
        let Some(previous)=self.previous.replace(reading)else{self.threshold=if reading.level<=10{10}else if reading.level<=20{20}else{101};return Vec::new();};
        let mut notices=Vec::with_capacity(2);
        if reading.ac!=previous.ac{notices.push(if reading.ac{Notice::Charging}else{Notice::Unplugged});if reading.ac{self.threshold=101;}}
        let threshold=if reading.level<=10{10}else{20};
        if !reading.ac&&reading.level<=20&&self.threshold>threshold{self.threshold=threshold;notices.push(Notice::Low);}
        notices
    }
}
struct Subscription{ac:HANDLE,level:HANDLE,tracker:Tracker}
impl Drop for Subscription{fn drop(&mut self){unsafe{if !self.ac.is_null(){unsubscribe(self.ac);}if !self.level.is_null(){unsubscribe(self.level);}}}}
thread_local!{static WATCH:RefCell<Option<Subscription>>=const{RefCell::new(None)};}
fn reading()->Option<Reading>{unsafe{
    let mut status:SYSTEM_POWER_STATUS=std::mem::zeroed();
    if GetSystemPowerStatus(&mut status)==0||status.BatteryFlag==255||status.BatteryFlag&128!=0||status.BatteryLifePercent>100{return None;}
    Some(Reading{level:status.BatteryLifePercent,ac:match status.ACLineStatus{0=>false,1=>true,_=>return None}})
}}
fn text(notice:Notice,level:u8)->String{
    let label=match notice{
        Notice::Charging=>crate::launcher::localized("Đã cắm sạc","Power connected","已连接电源"),
        Notice::Unplugged=>crate::launcher::localized("Đã rút sạc","Unplugged","已断开电源"),
        Notice::Low=>crate::launcher::localized("Pin yếu","Low battery","电量低"),
    };format!("{label} · {level}%")
}
pub(super) unsafe fn sync(){
    let enabled=super::STATE.with(|s|s.try_borrow().is_ok_and(|s|s.prefs.enabled&&!s.suspended));
    if !enabled{reset();return;}
    if WATCH.with(|w|w.borrow().is_some()){return;}
    let hwnd=super::CONTROL.get();if hwnd.is_null(){return;}
    let mut watch=Subscription{ac:subscribe(hwnd,&AC,0),level:subscribe(hwnd,&LEVEL,0),tracker:Tracker::default()};
    if watch.ac.is_null()||watch.level.is_null(){return;}
    watch.tracker.observe(reading());WATCH.with(|w|w.replace(Some(watch)));
}
pub(super) unsafe fn changed(){
    if WATCH.with(|w|w.borrow().is_none()){return;}
    let reading=reading();let notices=WATCH.with(|w|w.borrow_mut().as_mut().map(|w|w.tracker.observe(reading)).unwrap_or_default());
    if let Some(reading)=reading{for notice in notices{super::volume_osd::battery(reading.level,reading.ac,&text(notice,reading.level));}}
}
pub(super) fn reset(){WATCH.with(|w|w.borrow_mut().take());}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn original_thresholds_do_not_repeat_or_invent_unknown_transitions(){
        let mut t=Tracker::default();let r=|level,ac|Some(Reading{level,ac});
        assert!(t.observe(r(67,false)).is_empty());assert_eq!(t.observe(r(20,false)),vec![Notice::Low]);assert!(t.observe(r(19,false)).is_empty());
        assert_eq!(t.observe(r(10,false)),vec![Notice::Low]);assert!(t.observe(r(5,false)).is_empty());
        assert_eq!(t.observe(r(5,true)),vec![Notice::Charging]);assert!(t.observe(r(8,true)).is_empty());assert_eq!(t.observe(r(8,false)),vec![Notice::Unplugged,Notice::Low]);
        assert!(t.observe(None).is_empty());assert!(t.observe(r(95,true)).is_empty());assert_eq!(t.observe(r(95,false)),vec![Notice::Unplugged]);
        let mut low=Tracker::default();assert!(low.observe(r(4,false)).is_empty());assert!(low.observe(r(3,false)).is_empty());
        for notice in [Notice::Charging,Notice::Unplugged,Notice::Low]{assert!(!text(notice,42).is_empty());}
    }
    #[test]
    #[ignore="native read-only power subscriptions and repeated unregister on hosted Windows; no simulated real power change"]
    fn native_power_subscriptions_release_without_power_policy_writes(){
        if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
        unsafe{
            use windows_sys::Win32::UI::WindowsAndMessaging::*;let hwnd=CreateWindowExW(0,crate::desktop::wide("STATIC").as_ptr(),crate::desktop::wide("Battery event fixture").as_ptr(),0,0,0,1,1,HWND_MESSAGE,std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null());assert!(!hwnd.is_null());
            let before=super::super::CONTROL.replace(hwnd);let old=super::super::STATE.with(|s|{let mut s=s.borrow_mut();let old=(s.prefs.enabled,s.suspended);s.prefs.enabled=true;s.suspended=false;old});
            struct Cleanup{hwnd:HWND,control:HWND,old:(bool,bool)}impl Drop for Cleanup{fn drop(&mut self){unsafe{reset();super::super::CONTROL.set(self.control);super::super::STATE.with(|s|{let mut s=s.borrow_mut();s.prefs.enabled=self.old.0;s.suspended=self.old.1;});DestroyWindow(self.hwnd);}}}let _cleanup=Cleanup{hwnd,control:before,old};
            for _ in 0..6{sync();assert!(WATCH.with(|w|w.borrow().is_some()));changed();reset();assert!(WATCH.with(|w|w.borrow().is_none()));}
            super::super::STATE.with(|s|s.borrow_mut().suspended=true);sync();assert!(WATCH.with(|w|w.borrow().is_none()));
            println!("FLUIDWIN_BATTERY notices=ORIGINAL_THRESHOLDS native_subscriptions=RELEASED source=READ_ONLY physical_power_change=NOT_INVOKED background_poll=NONE");
        }
    }
}
