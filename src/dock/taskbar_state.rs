//! Runtime-only taskbar state. No shell mode is persisted or automatically re-armed.
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum Mode {
    #[default] Compatibility,
    HidePending,
    Hidden,
    Restoring,
    RestoredByWindows,
    RestoredByWatchdog,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
impl Mode{
    pub fn key(self)->&'static str{
        match self{
            Self::Compatibility=>"COMPATIBILITY_ACTIVE",Self::HidePending=>"DOCK_HIDE_PENDING",
            Self::Hidden=>"DOCK_HIDDEN_ACTIVE",Self::Restoring=>"RESTORING",
            Self::RestoredByWindows=>"RESTORED_BY_WINDOWS",Self::RestoredByWatchdog=>"RESTORED_BY_WATCHDOG",
            Self::Error=>"ERROR",
        }
    }
    pub fn pending(self)->bool{matches!(self,Self::HidePending|Self::Restoring)}
    pub fn hidden(self)->bool{self==Self::Hidden}
    pub fn label(self)->&'static str{
        match self{
            Self::Compatibility=>"Tương thích Windows", Self::HidePending=>"Chờ xác nhận ẩn",
            Self::Hidden=>"Deskland đang ẩn taskbar",Self::Restoring=>"Đang khôi phục",
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
        value["dockTaskbarHidden"]=self.effective.hidden().into();
        value["dockTaskbarPending"]=self.effective.pending().into();
        value["dockTaskbarDesired"]=if self.desired{"dock"}else{"windows"}.into();
        value["dockTaskbarEffective"]=self.effective.key().into();
        value["dockTaskbarActual"]=self.actual.key().into();
        value["dockTaskbarEffectiveLabel"]=self.effective.label().into();
        value["dockTaskbarActualLabel"]=self.actual.label().into();
        value["dockTaskbarWindows"]=serde_json::json!({"total":self.actual.total,"visible":self.actual.visible,"parked":self.actual.parked});
        value["dockTaskbarReason"]=self.reason.into();
        value["dockTaskbarMessage"]=self.message.clone().into();
    }
}
