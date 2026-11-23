//! Observed taskbar state for this session; startup preference lives in Options.
use std::time::{Duration,Instant};

#[derive(Default)]
pub(super) struct Recovery {
    attempts:u8,
    scheduled:Option<(Instant,Instant)>,
    healthy_since:Option<Instant>,
}
#[derive(PartialEq,Eq,Debug)]
pub(super) enum Retry { Idle, Wait, Start, Expired }
impl Recovery{
    pub fn schedule(&mut self,now:Instant)->bool{
        self.healthy_since=None;
        if self.attempts>=2{return false;}
        let delay=Duration::from_millis(if self.attempts==0{1800}else{3500});
        self.scheduled=Some((now+delay,now+Duration::from_secs(14)));
        true
    }
    pub fn pending(&self)->bool{self.scheduled.is_some()}
    pub fn attempted(&self)->bool{self.attempts>0}
    pub fn confirmed(&mut self,now:Instant){
        let since=*self.healthy_since.get_or_insert(now);
        if now.saturating_duration_since(since)>=Duration::from_secs(30){self.attempts=0;}
    }
    pub fn interrupted(&mut self){self.healthy_since=None;}
    pub fn check(&mut self,now:Instant,ready:bool)->Retry{
        let Some((due,expires))=self.scheduled else{return Retry::Idle;};
        if now>=expires{self.scheduled=None;return Retry::Expired;}
        if !ready||now<due{return Retry::Wait;}
        self.scheduled=None;
        self.attempts+=1;
        Retry::Start
    }
}

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum Mode {
    #[default] Compatibility,
    HidePending,
    Hidden,
    Parked,
    EdgePeek,
    Restoring,
    RestoredByWindows,
    RestoredByWatchdog,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_waits_for_restore_and_is_bounded_until_healthy_or_cancelled(){
        let t=Instant::now();
        let after=|secs|t+Duration::from_secs(secs);
        let mut r=Recovery::default();
        assert!(r.schedule(t));
        assert_eq!(r.check(after(1),true),Retry::Wait);
        assert_eq!(r.check(after(3),false),Retry::Wait);
        assert_eq!(r.check(after(4),true),Retry::Start);
        assert!(r.schedule(after(5)));
        assert_eq!(r.check(after(8),true),Retry::Wait);
        assert_eq!(r.check(after(9),true),Retry::Start);
        assert!(!r.schedule(after(10)));
        r.confirmed(after(10));
        r.confirmed(after(40));
        assert!(r.schedule(after(41)));
        assert_eq!(r.check(after(55),false),Retry::Expired);
        assert!(r.schedule(after(56)));
        r=Recovery::default(); // Compatibility / emergency / module disabled.
        assert_eq!(r.check(after(59),true),Retry::Idle);
    }

    #[test]
    fn windows_restore_clears_legacy_hidden_and_reports_parked_secondary() {
        let status=Status{desired:false,effective:Mode::RestoredByWindows,
            actual:Observation{known:true,total:2,visible:1,parked:1},reason:"ShellRevealed",message:String::new()};
        let mut json=serde_json::json!({"dockTaskbarHidden":true});
        status.add_fields(&mut json);
        assert_eq!(json["dockTaskbarHidden"],false);
        assert_eq!(json["dockTaskbarEffective"],"RESTORED_BY_WINDOWS");
        assert_eq!(json["dockTaskbarReason"],"ShellRevealed");
        assert_eq!(json["dockTaskbarWindows"]["parked"],1);
        assert_eq!(json["dockTaskbarActual"],"VISIBLE");
    }

    #[test]
    fn parked_and_edge_peek_keep_dock_engaged_without_claiming_a_visible_bar_is_hidden(){
        for (visible,parked,mode,hidden) in [(0,2,Mode::Parked,true),(1,1,Mode::EdgePeek,false)]{
            let actual=Observation{known:true,total:2,visible,parked};
            assert_eq!(Mode::confirmed(actual),mode);
            assert!(mode.engaged());
            let mut output=serde_json::json!({});
            Status{desired:true,effective:mode,actual,..Default::default()}.add_fields(&mut output);
            assert_eq!(output["dockTaskbarHidden"],hidden);
            assert_eq!(output["dockTaskbarWatchdog"],"ARMED");
        }
    }
}
impl Mode{
    pub fn key(self)->&'static str{
        match self{
            Self::Compatibility=>"COMPATIBILITY_ACTIVE",Self::HidePending=>"DOCK_HIDE_PENDING",
            Self::Hidden=>"DOCK_HIDDEN_ACTIVE",Self::Parked=>"DOCK_PARKED_ACTIVE",Self::EdgePeek=>"DOCK_EDGE_PEEK",Self::Restoring=>"RESTORING",
            Self::RestoredByWindows=>"RESTORED_BY_WINDOWS",Self::RestoredByWatchdog=>"RESTORED_BY_WATCHDOG",
            Self::Error=>"ERROR",
        }
    }
    pub fn pending(self)->bool{matches!(self,Self::HidePending|Self::Restoring)}
    pub fn confirmed(actual:Observation)->Self{
        if !actual.known||actual.visible>0{Self::EdgePeek}else if actual.parked>0{Self::Parked}else{Self::Hidden}
    }
    pub fn engaged(self)->bool{matches!(self,Self::Hidden|Self::Parked|Self::EdgePeek)}
    fn watchdog(self)->&'static str{
        match self{
            Self::HidePending=>"STARTING",Self::Hidden|Self::Parked|Self::EdgePeek=>"ARMED",Self::Restoring=>"STOPPING",
            Self::Compatibility|Self::RestoredByWindows|Self::RestoredByWatchdog|Self::Error=>"INACTIVE",
        }
    }
    pub fn label(self)->&'static str{
        match self{
            Self::Compatibility=>"Tương thích Windows", Self::HidePending=>"Chờ xác nhận ẩn",
            Self::Hidden=>"TLauncher đang ẩn thanh tác vụ",Self::Parked=>"Chế độ Dock · thanh Windows đang tự ẩn",
            Self::EdgePeek=>"Chế độ Dock · đang xác minh thanh Windows",Self::Restoring=>"Đang khôi phục",
            Self::RestoredByWindows=>"Windows đã lấy lại thanh tác vụ",
            Self::RestoredByWatchdog=>"Bộ bảo vệ đã khôi phục",Self::Error=>"Chưa xác minh được chế độ",
        }
    }
}
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub struct Observation{pub known:bool,pub total:usize,pub visible:usize,pub parked:usize}
impl Observation{
    pub fn label(self)->&'static str{
        if !self.known{"Chưa xác minh"}else if self.visible>0{"Đang hiện"}else if self.parked>0{"Windows đang tự ẩn"}else{"Đang ẩn"}
    }
    pub fn key(self)->&'static str{
        if !self.known{"UNKNOWN"}else if self.visible>0{"VISIBLE"}else if self.parked>0{"AUTO_HIDE_PARKED"}else{"HIDDEN"}
    }
}
#[derive(Clone,Debug,Default,PartialEq,Eq)]
pub struct Status{
    pub desired:bool, pub effective:Mode, pub actual:Observation,
    pub reason:&'static str, pub message:String,
}
impl Status{
    pub fn add_fields(&self,value:&mut serde_json::Value){
        value["dockTaskbarHidden"]=(self.effective.engaged()&&self.actual.known&&self.actual.visible==0).into();
        value["dockTaskbarPending"]=self.effective.pending().into();
        value["dockTaskbarDesired"]=if self.desired{"dock"}else{"windows"}.into();
        value["dockTaskbarEffective"]=self.effective.key().into();
        value["dockTaskbarActual"]=self.actual.key().into();
        value["dockTaskbarEffectiveLabel"]=self.effective.label().into();
        value["dockTaskbarActualLabel"]=self.actual.label().into();
        value["dockTaskbarWindows"]=serde_json::json!({"total":self.actual.total,"visible":self.actual.visible,"parked":self.actual.parked});
        value["dockTaskbarReason"]=self.reason.into();
        value["dockTaskbarWatchdog"]=self.effective.watchdog().into();
        value["dockTaskbarMessage"]=self.message.clone().into();
    }
}
