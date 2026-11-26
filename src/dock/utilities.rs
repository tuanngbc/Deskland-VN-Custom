//! One bounded, on-demand provider for all Dock displays. No probes while
//! when no status surface is visible; no active WLAN or Bluetooth scans, and no Shell/COM queries on the UI thread.
use super::{WM_UTILITY_READY,open,
    control_center::{self,View,Card,Command,MetricOptions},panel_chrome as chrome,
    surface::Palette,wifi};
use crate::desktop::wide;
use serde_json::{json,Value};
use std::{cell::{Cell,RefCell},mem,ptr,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}}};
use windows::Win32::Media::Audio::{*,Endpoints::IAudioEndpointVolume};
use windows::Win32::System::Com::*;
use windows_sys::Win32::{
    Devices::Bluetooth::*,
    Foundation::*,
    Graphics::Gdi::*,
    NetworkManagement::{IpHelper::*,Ndis::IfOperStatusUp,WiFi::*},
    System::{LibraryLoader::GetModuleHandleW,Power::*,Threading::{TrySubmitThreadpoolCallback,PTP_CALLBACK_INSTANCE}},
    UI::{Accessibility::{HIGHCONTRASTW,HCF_HIGHCONTRASTON},Controls::{InitCommonControlsEx,INITCOMMONCONTROLSEX,ICC_BAR_CLASSES,DRAWITEMSTRUCT,ODS_FOCUS,ODS_SELECTED,ODS_DISABLED,TBM_SETPOS,TBM_SETRANGEMAX,TB_THUMBTRACK,TBS_NOTICKS,NMHDR,NMCUSTOMDRAW,
        NM_CUSTOMDRAW,CDDS_PREPAINT,CDDS_ITEMPREPAINT,CDRF_NOTIFYITEMDRAW,CDRF_DODEFAULT,CDRF_SKIPDEFAULT,
        TBCD_CHANNEL,TBCD_THUMB,TBM_GETTHUMBRECT},Input::KeyboardAndMouse::*,WindowsAndMessaging::*},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Link { None, Disconnected, Disabled, Wired, WiFi(Option<u8>), Both(Option<u8>) }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) enum NetworkGlyph{Unknown,Disconnected,Disabled,Wired,Wifi(u8)}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) enum VolumeGlyph{Unknown,Muted,Waves(u8)}
#[derive(Clone,Debug,Default,PartialEq,Eq)]
pub(super) struct Status{network:Option<Link>,ssid:Option<String>,audio:Option<Audio>,recycle:Option<(u64,u64)>}
thread_local!{static STATUS:RefCell<Status>=RefCell::new(Status::default());}
impl Status{
    fn read(audio:Option<Audio>)->Self{
        let (network,ssid)=unsafe{network()};
        Self{network,ssid,audio,recycle:super::recycle_bin::query()}
    }
    fn network_glyph(&self)->NetworkGlyph{
        match self.network{
            None=>NetworkGlyph::Unknown,
            Some(Link::None|Link::Disconnected)=>NetworkGlyph::Disconnected,
            Some(Link::Disabled)=>NetworkGlyph::Disabled,
            Some(Link::Wired)=>NetworkGlyph::Wired,
            Some(Link::WiFi(q)|Link::Both(q))=>NetworkGlyph::Wifi(q.map_or(0,|p|(p/25+1).min(4))),
        }
    }
    fn volume_glyph(&self)->VolumeGlyph{
        self.audio.map_or(VolumeGlyph::Unknown,|a|if a.muted||a.level==0{VolumeGlyph::Muted}
            else{VolumeGlyph::Waves(match a.level{1..=33=>1,34..=66=>2,_=>3})})
    }
    fn detail(&self,widget:Widget)->Option<String>{
        match widget{
            Widget::Network=>{
                let full=network_label(self.network);
                let state=full.split_once(" · ").map_or(full.as_str(),|(_,s)|s);
                Some(self.ssid.as_ref().map_or_else(||state.into(),|name|format!("{name} · {state}")))
            }
            Widget::Sound=>Some(self.audio.map_or("Không đọc được âm lượng".into(),|a|
                if a.muted{format!("Tắt tiếng · {}%",a.level)}else{format!("{}%",a.level)})),
            Widget::RecycleBin=>self.recycle.map(|(n,_)|format!("{n} {}",crate::launcher::localized("mục","items","项"))),
            _=>None,
        }
    }
}
pub(super) fn status_glyphs()->(NetworkGlyph,VolumeGlyph){STATUS.with(|s|{let s=s.borrow();(s.network_glyph(),s.volume_glyph())})}
pub(super) fn status_detail(w:Widget)->Option<String>{STATUS.with(|s|s.borrow().detail(w))}
pub(super) fn replace_status(status:Status)->bool{STATUS.with(|s|{
    let mut s=s.borrow_mut();if *s==status{return false;}*s=status;true
})}
pub(super) fn replace_audio(audio:Option<Audio>)->bool{STATUS.with(|s|{
    let mut s=s.borrow_mut();if s.audio==audio{return false;}s.audio=audio;true
})}
pub(super) fn live_widget(w:Widget)->bool{matches!(w,Widget::Network|Widget::Sound|Widget::RecycleBin)}
pub(super) fn recycle_status()->Option<(u64,u64)>{STATUS.with(|s|s.borrow().recycle)}
struct ClassicDevice { name:String, connected:bool, paired:bool }
enum BluetoothState { Unknown, Adapter(Vec<ClassicDevice>) }
#[derive(Clone,Copy)]
enum BatteryState { Unknown, Absent, Present{level:Option<u8>,ac:Option<bool>,charging:bool} }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) struct Audio { level:u32, muted:bool }
pub(super) struct Snapshot { pub status:Status, wlan:Option<wifi::View>, bluetooth:BluetoothState, battery:BatteryState, radios:Option<super::radios::View>, brightness:Option<super::brightness::View> }
#[derive(Clone)]pub(super) enum Change{Network(wifi::Change),Radio(super::radios::Change),Brightness(super::brightness::Query)}

#[derive(Clone,Copy)]
enum JobKind { Read, ReadStatus, AudioRead, Volume(u32), Mute }

/// Fixture-only endpoint. Neither moving the fixture slider nor rapidly
/// toggling its mute can write to a Windows audio device.
static DEMO_VOLUME: Mutex<Audio> = Mutex::new(Audio { level: 46, muted: false });
fn fixture_audio(value:&mut Audio, kind:JobKind)->Audio {
    match kind {
        JobKind::Read|JobKind::ReadStatus|JobKind::AudioRead=>{},
        JobKind::Volume(value_percent)=>value.level=value_percent.min(100),
        JobKind::Mute=>value.muted=!value.muted,
    }
    Audio {level:value.level,muted:value.muted}
}
#[derive(Default)]
struct State {
    pending:AtomicBool, result:Mutex<Option<Snapshot>>, status_result:Mutex<Option<Status>>,
    audio_result:Mutex<Option<(usize,Option<Audio>)>>, closed:AtomicBool,
}
static WORK_ACTIVE:AtomicBool=AtomicBool::new(false);
struct Permit;
impl Drop for Permit{fn drop(&mut self){WORK_ACTIVE.store(false,Ordering::Release);}}
struct Job { shared:Arc<State>, owner:usize, panel:usize, kind:JobKind, widget:Option<Widget>, change:Option<Change>, permit:Permit }
pub(super) struct Provider { state:Arc<State> }
impl Provider {
    pub fn new()->Self{Self{state:Arc::default()}}
    fn submit(&self,owner:HWND,panel:HWND,kind:JobKind,widget:Option<Widget>,change:Option<Change>)->bool {
        if owner.is_null()||self.state.pending.swap(true,Ordering::AcqRel){return false;}
        if WORK_ACTIVE.swap(true,Ordering::AcqRel){
            self.state.pending.store(false,Ordering::Release);
            return false;
        }
        let raw=Box::into_raw(Box::new(Job{shared:self.state.clone(),owner:owner as usize,panel:panel as usize,kind,widget,change,permit:Permit}));
        if unsafe{TrySubmitThreadpoolCallback(Some(worker),raw.cast(),ptr::null())}==0 {
            unsafe{drop(Box::from_raw(raw));}
            self.state.pending.store(false,Ordering::Release);
            return false;
        }
        true
    }
    pub fn read(&self,owner:HWND,widget:Option<Widget>,change:Option<Change>)->bool{
        // An unread answer belongs to the previous panel; do not let a new
        // request replace it before the Dock processes WM_UTILITY_READY.
        if self.state.result.lock().map_or(true,|slot|slot.is_some()){return false;}
        self.submit(owner,ptr::null_mut(),JobKind::Read,widget,change)
    }
    pub fn read_status(&self,owner:HWND)->bool{
        if self.state.status_result.lock().map_or(true,|slot|slot.is_some()){return false;}
        self.submit(owner,ptr::null_mut(),JobKind::ReadStatus,None,None)
    }
    pub fn take_status(&self)->Option<Status>{self.state.status_result.lock().ok()?.take()}
    pub fn take(&self)->Option<Snapshot>{self.state.result.lock().ok()?.take()}
    pub fn take_audio_result(&self)->Option<(usize,Option<Audio>)>{self.state.audio_result.lock().ok()?.take()}
    pub fn apply(&self,owner:HWND,panel:HWND,action:Option<Action>)->bool{
        let kind=match action{None=>JobKind::AudioRead,Some(Action::SetVolume(p))=>JobKind::Volume(p),
            Some(Action::Mute)=>JobKind::Mute,_=>return false};
        self.submit(owner,panel,kind,None,None)
    }
}
impl Drop for Provider {fn drop(&mut self){self.state.closed.store(true,Ordering::Release);}}
unsafe extern "system" fn worker(_:PTP_CALLBACK_INSTANCE,raw:*mut std::ffi::c_void){
    let job=*Box::from_raw(raw.cast::<Job>());
    let state=&job.shared;
    if !state.closed.load(Ordering::Acquire){
        // Controls target the current default endpoint, not an old ID.
        let audio=audio(job.kind);
        if matches!(job.kind,JobKind::Read){
            let mut bluetooth=if matches!(job.widget,None|Some(Widget::Bluetooth)){bluetooth()}else{BluetoothState::Unknown};
            if crate::trial::demo_active() {
                if let BluetoothState::Adapter(devices)=&mut bluetooth {
                    for (i,device) in devices.iter_mut().enumerate(){device.name=format!("Fixture device {}",i+1);}
                }
            }
            let (network_change,radio_change,brightness_query)=match job.change{Some(Change::Network(change))=>(Some(change),None,None),Some(Change::Radio(change))=>(None,Some(change),None),Some(Change::Brightness(query))=>(None,None,Some(query)),None=>(None,None,None)};
            let brightness=brightness_query.map(super::brightness::read);
            let radio_kind=match job.widget{Some(Widget::Network)=>Some(super::radios::Kind::Wifi),Some(Widget::Bluetooth)=>Some(super::radios::Kind::Bluetooth),_=>None};
            let radios=radio_kind.map(|kind|super::radios::read(kind,radio_change));
            let wlan=(job.widget==Some(Widget::Network)).then(||wifi::read(network_change));
            let snapshot=Snapshot{status:Status::read(audio),wlan,bluetooth,battery:battery(),radios,brightness};
            if let Ok(mut result)=state.result.lock(){*result=Some(snapshot);}
        }else if matches!(job.kind,JobKind::ReadStatus){
            if let Ok(mut result)=state.status_result.lock(){*result=Some(Status::read(audio));}
        }else if let Ok(mut result)=state.audio_result.lock(){
            *result=Some((job.panel,audio));
        }
    }
    drop(job.permit);
    state.pending.store(false,Ordering::Release);
    if !state.closed.load(Ordering::Acquire){
        PostMessageW(job.owner as HWND,WM_UTILITY_READY,0,0);
    }
}
unsafe fn audio(kind:JobKind)->Option<Audio>{
    if crate::trial::demo_active(){return DEMO_VOLUME.lock().ok().map(|mut value|fixture_audio(&mut value,kind));}
    if audio_read_only() && matches!(kind,JobKind::Volume(_)|JobKind::Mute){return None;}
    let com=CoInitializeEx(None,COINIT_MULTITHREADED);
    if com.is_err(){return None;}
    let value=(||->windows::core::Result<Audio>{
        let devices:IMMDeviceEnumerator=CoCreateInstance(&MMDeviceEnumerator,None,CLSCTX_ALL)?;
        let endpoint=devices.GetDefaultAudioEndpoint(eRender,eMultimedia)?;
        let volume:IAudioEndpointVolume=endpoint.Activate(CLSCTX_ALL,None)?;
        let muted=volume.GetMute()?.as_bool();
        match kind {
            JobKind::Volume(percent)=>volume.SetMasterVolumeLevelScalar(percent.min(100) as f32/100.0,ptr::null())?,
            JobKind::Mute=>volume.SetMute(!muted,ptr::null())?,
            JobKind::Read|JobKind::ReadStatus|JobKind::AudioRead=>{}
        }
        let level=(volume.GetMasterVolumeLevelScalar()?.clamp(0.0,1.0)*100.0).round() as u32;
        Ok(Audio{level,muted:volume.GetMute()?.as_bool()})
    })().ok();
    CoUninitialize();
    value
}

unsafe fn network()->(Option<Link>,Option<String>){
    let mut table=ptr::null_mut();
    if GetIfTable2(&mut table)!=0||table.is_null(){return (None,None);}
    let (mut lan,mut wifi,mut wifi_enabled,mut wifi_present)=(false,false,false,false);
    for i in 0..(*table).NumEntries.min(1024) as usize {
        let row=&*ptr::addr_of!((*table).Table).cast::<MIB_IF_ROW2>().add(i);
        if row.InterfaceAndOperStatusFlags._bitfield&1==0{continue;}
        if row.Type==71{
            wifi_present=true;
            wifi_enabled|=row.AdminStatus==1; // NET_IF_ADMIN_STATUS_UP
        }
        if row.OperStatus!=IfOperStatusUp{continue;}
        // IANA ifType: 6 ethernetCsmacd, 71 ieee80211. Only physical adapters.
        lan|=row.Type==6;wifi|=row.Type==71;
    }
    FreeMibTable(table.cast());
    let (signal,ssid)=if wifi{wifi_signal()}else{(None,None)};
    let link=match (lan,wifi){
        (true,true)=>Link::Both(signal),(true,false)=>Link::Wired,(false,true)=>Link::WiFi(signal),
        _ if wifi_enabled=>Link::Disconnected,_ if wifi_present=>Link::Disabled,_=>Link::None,
    };
    (Some(link),ssid)
}
unsafe fn wifi_signal()->(Option<u8>,Option<String>){
    let mut h=ptr::null_mut();let mut version=0;
    if WlanOpenHandle(2,ptr::null(),&mut version,&mut h)!=0||h.is_null(){return (None,None);}
    let mut list=ptr::null_mut();let mut signal=None;let mut ssid=None;
    if WlanEnumInterfaces(h,ptr::null(),&mut list)==0&&!list.is_null(){
        for i in 0..(*list).dwNumberOfItems.min(32) as usize {
            let iface=&*ptr::addr_of!((*list).InterfaceInfo).cast::<WLAN_INTERFACE_INFO>().add(i);
            if iface.isState!=wlan_interface_state_connected{continue;}
            let (mut length,mut info)=(0,ptr::null_mut());
            if WlanQueryInterface(h,&iface.InterfaceGuid,wlan_intf_opcode_current_connection,ptr::null(),&mut length,&mut info,ptr::null_mut())==0&&!info.is_null(){
                if length as usize>=mem::size_of::<WLAN_CONNECTION_ATTRIBUTES>(){
                    let association=&(*info.cast::<WLAN_CONNECTION_ATTRIBUTES>()).wlanAssociationAttributes;
                    let strength=association.wlanSignalQuality.min(100) as u8;
                    if signal.is_none_or(|v|strength>v){
                        signal=Some(strength);
                        let raw=&association.dot11Ssid;
                        let name=String::from_utf8_lossy(&raw.ucSSID[..(raw.uSSIDLength as usize).min(raw.ucSSID.len())])
                            .chars().filter(|c|!c.is_control()).collect::<String>();
                        ssid=(!name.trim().is_empty()&&!crate::trial::demo_active()).then_some(name);
                    }
                }
                WlanFreeMemory(info);
            }
        }
        WlanFreeMemory(list.cast());
    }
    WlanCloseHandle(h,ptr::null());
    (signal,ssid)
}
unsafe fn bluetooth()->BluetoothState{
    let mut radio=ptr::null_mut();
    let params=BLUETOOTH_FIND_RADIO_PARAMS{dwSize:mem::size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32};
    let find=BluetoothFindFirstRadio(&params,&mut radio);
    if find.is_null(){return BluetoothState::Unknown;}
    let mut devices:Vec<ClassicDevice>=Vec::new();
    for index in 0..16 {
        if radio.is_null(){break;}
        let mut params:BLUETOOTH_DEVICE_SEARCH_PARAMS=mem::zeroed();
        params.dwSize=mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32;
        params.fReturnConnected=1;params.fReturnAuthenticated=1;params.fReturnRemembered=1;params.hRadio=radio;
        let mut info:BLUETOOTH_DEVICE_INFO=mem::zeroed();
        info.dwSize=mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
        let device=BluetoothFindFirstDevice(&params,&mut info);
        if !device.is_null(){
            for _ in 0..48{
                let length=info.szName.iter().position(|&c|c==0).unwrap_or(info.szName.len());
                let name=String::from_utf16_lossy(&info.szName[..length]).chars().filter(|c|!c.is_control()).take(64).collect::<String>();
                let name=if name.trim().is_empty(){"Thiết bị chưa có tên".into()}else{name};
                devices.push(ClassicDevice{name,connected:info.fConnected!=0,paired:info.fAuthenticated!=0});
                if BluetoothFindNextDevice(device,&mut info)==0{break;}
            }
            BluetoothFindDeviceClose(device);
        }
        CloseHandle(radio);
        if index==15||BluetoothFindNextRadio(find,&mut radio)==0{break;}
    }
    BluetoothFindRadioClose(find);
    devices.sort_by_key(|device|(!device.connected,!device.paired));
    devices.truncate(12);
    BluetoothState::Adapter(devices)
}
unsafe fn battery()->BatteryState{
    let mut s:SYSTEM_POWER_STATUS=mem::zeroed();
    if GetSystemPowerStatus(&mut s)==0||s.BatteryFlag==255{return BatteryState::Unknown;}
    if s.BatteryFlag&128!=0{return BatteryState::Absent;}
    BatteryState::Present {level:(s.BatteryLifePercent<=100).then_some(s.BatteryLifePercent),
        ac:match s.ACLineStatus{0=>Some(false),1=>Some(true),_=>None},charging:s.BatteryFlag&8!=0}
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Widget {
    Network, Sound, Bluetooth, Battery, Notifications, QuickSettings,
    Spotlight, Explorer, Desktop, Capture, Record, Stats, Wallpaper, Stage, Settings, Windows, Metrics, Downloads, RecycleBin,
}
impl Widget {
    pub const ALL:[Self;19]=[Self::Network,Self::Sound,Self::Bluetooth,Self::Battery,
        Self::Notifications,Self::QuickSettings,Self::Spotlight,Self::Explorer,Self::Desktop,
        Self::Capture,Self::Record,Self::Stats,Self::Wallpaper,Self::Stage,Self::Settings,Self::Windows,Self::Metrics,Self::Downloads,Self::RecycleBin];
    pub fn key(self)->&'static str{
        use Widget::*;
        match self {Network=>"network",Sound=>"audio",Bluetooth=>"bluetooth",Battery=>"battery",
            Notifications=>"notifications",QuickSettings=>"quickSettings",Spotlight=>"spotlight",Explorer=>"explorer",
            Desktop=>"desktop",Capture=>"capture",Record=>"record",Stats=>"stats",Wallpaper=>"wallpaper",Stage=>"stage",
            Settings=>"settings",Windows=>"windows",Metrics=>"metrics",Downloads=>"downloads",RecycleBin=>"recycleBin"}
    }
    pub fn title(self)->&'static str{
        use Widget::*;
        match self{Network=>"Mạng",Sound=>"Âm thanh",Bluetooth=>"Bluetooth",Battery=>"Pin",
            Notifications=>"Thông báo Windows",QuickSettings=>"Cài đặt nhanh",Spotlight=>"Spotlight",
            Explorer=>"File Explorer",Desktop=>"Hiện/ẩn desktop",Capture=>"Chụp màn hình",Record=>"Quay màn hình",
            Stats=>"Chỉ số taskbar",Wallpaper=>"Bật/tắt hình nền",Stage=>"Stage Manager",
            Settings=>"Cài đặt TLauncher",Windows=>"Chuyển cửa sổ",Metrics=>"Chỉ số máy",
            Downloads=>crate::launcher::localized("Tải xuống","Downloads","下载"),RecycleBin=>crate::launcher::localized("Thùng rác","Recycle Bin","回收站")}
    }
    pub(super) fn simple(self)->Option<Action>{
        use Widget::*;
        Some(match self{
            Notifications=>Action::Notifications,QuickSettings=>Action::QuickSettings,
            Spotlight=>Action::Host(HostAction::Launcher),Explorer=>Action::Explorer,Desktop=>Action::Desktop,
            Capture=>Action::Host(HostAction::Capture),Record=>Action::Host(HostAction::Record),
            Stats=>Action::Host(HostAction::SystemMonitor),Wallpaper=>Action::Host(HostAction::Wallpaper),
            Stage=>Action::Host(HostAction::Stage),Settings=>Action::Host(HostAction::Settings),Windows=>Action::Windows,
            RecycleBin=>Action::RecycleBin,
            _=>return None,
        })
    }
    pub(super) fn glyph(self)->&'static [(f32,f32,f32,f32)]{
        use Widget::*;
        match self {
            Network=>&[(0.20,0.39,0.34,0.29),(0.34,0.29,0.66,0.29),(0.66,0.29,0.80,0.39),(0.30,0.52,0.43,0.46),(0.43,0.46,0.57,0.46),(0.57,0.46,0.70,0.52),(0.41,0.64,0.50,0.60),(0.50,0.60,0.59,0.64),(0.49,0.75,0.51,0.75)],
            Sound=>&[(0.20,0.43,0.34,0.43),(0.20,0.43,0.20,0.64),(0.20,0.64,0.34,0.64),(0.34,0.43,0.51,0.28),(0.51,0.28,0.51,0.79),(0.51,0.79,0.34,0.64),(0.62,0.41,0.72,0.54),(0.72,0.54,0.62,0.67),(0.72,0.29,0.83,0.54),(0.83,0.54,0.72,0.79)],
            Bluetooth=>&[(0.50,0.18,0.50,0.82),(0.50,0.18,0.70,0.34),(0.70,0.34,0.31,0.68),(0.31,0.32,0.70,0.66),(0.70,0.66,0.50,0.82)],
            Battery=>&[(0.18,0.35,0.73,0.35),(0.73,0.35,0.73,0.68),(0.73,0.68,0.18,0.68),(0.18,0.68,0.18,0.35),(0.79,0.45,0.79,0.58),(0.28,0.51,0.58,0.51)],
            Notifications=>&[(0.21,0.68,0.79,0.68),(0.24,0.65,0.34,0.51),(0.34,0.51,0.34,0.38),(0.34,0.38,0.50,0.27),(0.50,0.27,0.66,0.38),(0.66,0.38,0.66,0.51),(0.66,0.51,0.76,0.65),(0.44,0.77,0.56,0.77)],
            QuickSettings|Settings=>&[(0.22,0.35,0.78,0.35),(0.22,0.65,0.78,0.65),(0.40,0.28,0.40,0.42),(0.61,0.58,0.61,0.72)],
            Spotlight=>&[(0.30,0.38,0.40,0.28),(0.40,0.28,0.55,0.28),(0.55,0.28,0.65,0.38),(0.65,0.38,0.65,0.53),(0.65,0.53,0.55,0.63),(0.55,0.63,0.40,0.63),(0.40,0.63,0.30,0.53),(0.30,0.53,0.30,0.38),(0.61,0.59,0.77,0.76)],
            Explorer|Downloads=>&[(0.20,0.38,0.20,0.72),(0.20,0.72,0.80,0.72),(0.80,0.72,0.80,0.38),(0.80,0.38,0.48,0.38),(0.48,0.38,0.39,0.28),(0.39,0.28,0.20,0.28),(0.20,0.28,0.20,0.38),(0.20,0.45,0.80,0.45)],
            RecycleBin=>&[(0.23,0.30,0.77,0.30),(0.30,0.30,0.34,0.80),(0.34,0.80,0.66,0.80),(0.66,0.80,0.70,0.30),(0.40,0.30,0.40,0.20),(0.40,0.20,0.60,0.20),(0.60,0.20,0.60,0.30),(0.44,0.42,0.44,0.66),(0.56,0.42,0.56,0.66)],
            Desktop|Wallpaper=>&[(0.20,0.28,0.80,0.28),(0.80,0.28,0.80,0.68),(0.80,0.68,0.20,0.68),(0.20,0.68,0.20,0.28),(0.50,0.68,0.50,0.79),(0.36,0.79,0.64,0.79)],
            Capture=>&[(0.23,0.34,0.43,0.34),(0.23,0.34,0.23,0.70),(0.23,0.70,0.77,0.70),(0.77,0.70,0.77,0.34),(0.77,0.34,0.58,0.34),(0.43,0.34,0.49,0.26),(0.49,0.26,0.56,0.26),(0.56,0.26,0.58,0.34),(0.42,0.48,0.58,0.48),(0.42,0.48,0.42,0.60),(0.42,0.60,0.58,0.60),(0.58,0.60,0.58,0.48)],
            Record=>&[(0.33,0.32,0.66,0.32),(0.66,0.32,0.75,0.41),(0.75,0.41,0.75,0.66),(0.75,0.66,0.66,0.73),(0.66,0.73,0.33,0.73),(0.33,0.73,0.25,0.66),(0.25,0.66,0.25,0.41),(0.25,0.41,0.33,0.32),(0.44,0.49,0.56,0.49),(0.44,0.55,0.56,0.55)],
            Stats|Metrics=>&[(0.24,0.73,0.76,0.73),(0.32,0.73,0.32,0.53),(0.50,0.73,0.50,0.39),(0.68,0.73,0.68,0.26)],
            Stage=>&[(0.25,0.31,0.72,0.31),(0.72,0.31,0.72,0.68),(0.72,0.68,0.25,0.68),(0.25,0.68,0.25,0.31),(0.32,0.24,0.79,0.24),(0.79,0.24,0.79,0.59)],
            Windows=>&[(0.22,0.34,0.56,0.34),(0.56,0.34,0.56,0.68),(0.56,0.68,0.22,0.68),(0.22,0.68,0.22,0.34),(0.61,0.34,0.78,0.34),(0.78,0.34,0.78,0.68),(0.78,0.68,0.61,0.68)],
        }
    }
}
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct Profile {
    pub enabled:bool,pub leading:bool,pub details:bool,pub cell:u32,pub two_rows:bool,pub gap:u32,
    pub network_on_dock:bool,pub sound_on_dock:bool,
    pub metrics:MetricOptions,pub order:Vec<Widget>,pub hidden:Vec<Widget>,pub inline:Vec<Widget>,
}
impl Default for Profile {
    fn default()->Self{Self{enabled:true,leading:false,details:true,cell:28,two_rows:true,gap:4,network_on_dock:true,sound_on_dock:true,metrics:MetricOptions::default(),order:Widget::ALL.to_vec(),hidden:Vec::new(),inline:Vec::new()}}
}
impl Profile {
    pub fn from_json(v:&Value)->Self{
        let read=|key:&str|{
            let mut out=Vec::new();
            for item in v[key].as_array().into_iter().flatten().take(64){
                if let Some(w)=Widget::ALL.iter().copied().find(|w|item.as_str()==Some(w.key())){
                    if !out.contains(&w){out.push(w);}
                }
            }
            out
        };
        let mut order=read("order");
        for w in Widget::ALL {if !order.contains(&w){order.push(w);}}
        let hidden=read("hidden");
        let inline=read("inline").into_iter().take(10).collect();
        let legacy=v["enabled"].as_bool().unwrap_or(true);
        Self{enabled:legacy,network_on_dock:v["networkOnDock"].as_bool().unwrap_or(legacy&&!hidden.contains(&Widget::Network)),
            sound_on_dock:v["soundOnDock"].as_bool().unwrap_or(legacy&&!hidden.contains(&Widget::Sound)),leading:v["side"]=="left",
            details:v["details"].as_bool().unwrap_or(true),cell:v["cell"].as_u64().map_or(28,|n|n.clamp(24,32) as u32),
            two_rows:v["rows"].as_u64()!=Some(1),gap:v["gap"].as_u64().map_or(4,|n|n.clamp(2,8) as u32),metrics:MetricOptions::from_json(&v["metrics"]),order,hidden,inline}
    }
    pub fn json(&self)->Value{
        json!({"enabled":self.enabled,"networkOnDock":self.network_on_dock,"soundOnDock":self.sound_on_dock,"side":if self.leading{"left"}else{"right"},"details":self.details,"cell":self.cell,"rows":if self.two_rows{2}else{1},"gap":self.gap,"metrics":self.metrics.json(),
            "order":self.order.iter().map(|w|w.key()).collect::<Vec<_>>(),
            "hidden":self.hidden.iter().map(|w|w.key()).collect::<Vec<_>>(),
            "inline":self.inline.iter().map(|w|w.key()).collect::<Vec<_>>()})
    }
    pub(super) fn dock_signals(&self)->usize{usize::from(self.network_on_dock)+usize::from(self.sound_on_dock)}
    pub(super) fn dock_metrics(&self)->bool{self.metrics.enabled&&self.metrics.inline&&self.metrics.mask!=0&&!self.hidden.contains(&Widget::Metrics)}
    pub fn pinned(&self)->Vec<Widget>{
        if !self.enabled{return Vec::new();}
        let show_battery=self.inline.contains(&Widget::Battery)&&unsafe{matches!(battery(),BatteryState::Present{..})};
        self.order.iter().copied().filter(|w|!self.hidden.contains(w)&&match *w{
            Widget::Metrics=>self.metrics.enabled,
            Widget::Battery=>show_battery,
            _=>self.inline.contains(w),
        }).collect()
    }
    pub fn toggle(&mut self,widget:Widget){if self.hidden.contains(&widget){self.hidden.retain(|&w|w!=widget);}else{self.hidden.push(widget);}}
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum HostAction { Settings, Capture, Record, SystemMonitor, Wallpaper, Stage, Launcher }
#[derive(Clone,Copy)]
pub(super) enum Action {
    Host(HostAction), Network, Bluetooth, Sound, Battery, Notifications,
    QuickSettings, Explorer, Desktop, Windows, RecycleBin, SetVolume(u32), Mute, AudioPanel{level:u32,muted:bool},
}
fn network_label(state:Option<Link>)->String{
    match state {
        None=>"Mạng · Không đọc được trạng thái".into(),
        Some(Link::None)=>"Mạng · Không có kết nối vật lý".into(),
        Some(Link::Wired)=>"Mạng · Ethernet đã kết nối".into(),
        Some(Link::Disconnected)=>"Mạng · Wi-Fi chưa kết nối".into(),
        Some(Link::Disabled)=>"Mạng · Adapter Wi-Fi đã tắt".into(),
        Some(Link::WiFi(signal)|Link::Both(signal))=>{
            let kind=if matches!(state,Some(Link::Both(_))){"Ethernet và Wi-Fi"}else{"Wi-Fi"};
            signal.map_or_else(||format!("Mạng · {kind} đã kết nối"),|q|format!("Mạng · {kind}, tín hiệu {q}%"))
        }
    }
}
fn device_label(state:&BluetoothState)->String{
    match state{
        BluetoothState::Unknown=>"Windows chưa cung cấp trạng thái".into(),
        BluetoothState::Adapter(devices)=>{
            let connected=devices.iter().filter(|d|d.connected).count();
            if connected==0{"Không thấy thiết bị Classic đang kết nối".into()}
            else{format!("{connected} thiết bị Classic đang kết nối")}
        },
    }
}
fn utility_hint(widget:Widget,snapshot:&Snapshot)->String{
    match widget{
        Widget::Network|Widget::Sound|Widget::RecycleBin=>snapshot.status.detail(widget).unwrap_or_default(),
        Widget::Bluetooth=>device_label(&snapshot.bluetooth),
        Widget::Battery=>match snapshot.battery{
            BatteryState::Present{level,charging,..}=>level.map_or("".into(),|n|format!("{n}%{}",if charging{" · Đang sạc"}else{""})),
            _=>String::new(),
        },
        Widget::QuickSettings=>"Mạng · Âm thanh · Windows".into(),
        Widget::Metrics=>"CPU · RAM · GPU · Mạng".into(),
        _=>String::new(),
    }
}
pub(super) fn available_in_dock_mode(widget:Option<Widget>)->bool{
    !matches!(widget,Some(Widget::Notifications|Widget::Stats))
}
pub(super) fn local_action(action:Action)->bool{
    !matches!(action,Action::Notifications|Action::QuickSettings|Action::Windows|Action::Host(HostAction::SystemMonitor))
}
pub(super) unsafe fn show_hub(owner:HWND,anchor:chrome::Anchor,profile:&Profile,dock_mode:bool){
    // The overview is interactive immediately. Device-specific views request
    // their readings on demand; this click never waits on WLan / COM.
    let initial=Snapshot{status:STATUS.with(|s|s.borrow().clone()),wlan:None,bluetooth:BluetoothState::Unknown,battery:battery(),radios:None,brightness:None};
    control_center::show_hub(owner,anchor,hub_view(&initial,profile,dock_mode,false));
}
pub(super) fn hub_view(snapshot:&Snapshot,profile:&Profile,dock_mode:bool,live:bool)->View{
    let mut cards=profile.order.iter().copied().filter(|w|!dock_mode||available_in_dock_mode(Some(*w))).filter(|w|!profile.hidden.contains(w)
        && (*w!=Widget::Metrics||profile.metrics.enabled)
        && (*w!=Widget::Battery||matches!(snapshot.battery,BatteryState::Present{..})))
        .map(|w|Card{title:w.title().into(),hint:if profile.details&&live{utility_hint(w,snapshot)}else{String::new()},
            icon:Some(w),action:Some(Command::Widget(w))}).collect::<Vec<_>>();
    if dock_mode{
        for w in [Widget::Settings,Widget::Windows,Widget::Bluetooth,Widget::Sound,Widget::Network]{
            if !profile.order.contains(&w)||profile.hidden.contains(&w){
                cards.insert(0,Card{title:w.title().into(),hint:if live{utility_hint(w,snapshot)}else{String::new()},
                    icon:Some(w),action:Some(Command::Widget(w))});
            }
        }
    }
    cards.insert(0,Card{title:"Ứng dụng chạy nền / Khay ứng dụng".into(),
        hint:"Cửa sổ đã thu nhỏ và khay thông báo Windows".into(),
        icon:Some(Widget::Windows),action:Some(Command::BackgroundApps)});
    cards.push(Card{title:crate::launcher::localized("Ứng dụng ẩn khỏi Dock","Apps hidden from Dock","从 Dock 隐藏的应用").into(),
        hint:String::new(),icon:None,action:Some(Command::HiddenApps)});
    cards.push(Card{title:crate::launcher::localized("Bộ trộn âm lượng","Volume mixer","音量混合器").into(),
        hint:crate::launcher::localized("Thiết bị và từng ứng dụng","Outputs and application volumes","输出设备和应用音量").into(),
        icon:Some(Widget::Sound),action:Some(Command::AudioMixer)});
    if super::clipboard_history::active(){
        cards.insert(1,Card{title:crate::launcher::localized("Lịch sử clipboard","Clipboard history","剪贴板历史").into(),
            hint:crate::launcher::localized("Văn bản và ảnh","Text and images","文字与图片").into(),
            icon:None,action:Some(Command::Clipboard)});
    }
    cards.push(Card{title:crate::launcher::localized("Giữ máy thức và Hẹn giờ","Keep awake and Countdown","保持唤醒与倒计时").into(),
        hint:String::new(),icon:None,action:Some(Command::Comfort(super::comfort::Action::Open))});
    cards.push(Card{title:crate::launcher::localized("Đang phát","Now playing","正在播放").into(),
        hint:crate::launcher::localized("Bài hát, ảnh bìa và điều khiển phát","Track, artwork and playback controls","曲目、封面与播放控制").into(),icon:None,action:Some(Command::Media)});
    cards.push(Card{title:crate::launcher::localized("Lịch","Calendar","日历").into(),hint:String::new(),icon:None,action:Some(Command::Calendar)});
    cards.push(Card{title:crate::launcher::localized("Tin tức","News","新闻").into(),hint:String::new(),icon:None,action:Some(Command::News(super::news::Action::Open))});
    cards.push(Card{title:crate::launcher::localized("Thời tiết","Weather","天气").into(),hint:"Open-Meteo".into(),icon:None,action:Some(Command::Weather(super::weather_panel::Action::Open))});
    cards.push(Card{title:crate::launcher::localized("Thông báo","Notifications","通知").into(),
        hint:crate::launcher::localized("Lịch sử thông báo do Windows cho phép","Notification history allowed by Windows","Windows 允许访问的通知历史").into(),icon:None,
        action:Some(Command::Notifications(super::notifications::Action::Open))});
    if !profile.hidden.contains(&Widget::QuickSettings){cards.push(Card{title:crate::launcher::localized("Độ sáng màn hình","Display brightness","显示器亮度").into(),hint:String::new(),icon:None,action:Some(Command::Brightness(None))});}
    cards.push(Card{title:crate::launcher::localized("Tổng quan cửa sổ","Window overview","窗口概览").into(),
        hint:crate::launcher::localized("Cửa sổ đang mở trên màn hình này","Open windows on this display","此显示器上打开的窗口").into(),icon:None,action:Some(Command::Overview)});
    cards.push(Card{title:crate::launcher::localized("Bố cục cửa sổ","Snap layouts","窗口布局").into(),
        hint:crate::launcher::localized("Chọn cửa sổ và bố cục chia màn hình","Choose a window and a screen layout","选择窗口和屏幕布局").into(),icon:None,action:Some(Command::SnapLayouts)});
    cards.push(Card{title:crate::launcher::localized("Luôn ở trên cùng","Always on top","窗口置顶").into(),
        hint:crate::launcher::localized("Ghim cửa sổ cho đến khi bỏ ghim hoặc thoát","Pin a window until unpinned or exit","置顶窗口，直到取消或退出").into(),icon:None,action:Some(Command::Topmost(None))});
    cards.push(Card{title:crate::launcher::localized("Kệ tệp","File Shelf","文件暂存架").into(),
        hint:crate::launcher::localized("Giữ tham chiếu để kéo tệp giữa các ứng dụng","Hold references to drag files between apps","暂存引用以在应用间拖放文件").into(),icon:None,action:Some(Command::Shelf(super::shelf::Action::Open))});
    cards.push(Card{title:crate::launcher::localized("Viền chụp cửa sổ","Window capture border","窗口捕获边框").into(),
        hint:crate::launcher::localized("Quyền ẩn viền do Windows quản lý","Borderless permission is managed by Windows","无边框权限由 Windows 管理").into(),icon:None,action:Some(Command::CaptureAccess(super::capture_access::Action::Open))});
    if !cards.iter().any(|c|matches!(c.action,Some(Command::Widget(Widget::Desktop)))){
        cards.push(Card{title:crate::launcher::localized("Hiện Desktop","Show desktop","显示桌面").into(),hint:String::new(),icon:Some(Widget::Desktop),action:Some(Command::Desktop(false))});
    }
    if unsafe{super::show_desktop::can_restore()}{
        cards.push(Card{title:crate::launcher::localized("Khôi phục cửa sổ","Restore windows","还原窗口").into(),hint:String::new(),icon:Some(Widget::Windows),action:Some(Command::Desktop(true))});
    }
    View{title:"Trung tâm điều khiển".into(),
        hint:"Tiện ích Dock · Chọn một mục để mở".into(),
        cards,metrics:None}
}
pub(super) unsafe fn show_view(owner:HWND,point:POINT,snapshot:&Snapshot,profile:&Profile,widget:Option<Widget>,dock_mode:bool){
    let anchor=chrome::Anchor::for_owner(owner,point);
    if let Some(w)=widget{
        if profile.hidden.contains(&w)&&!(dock_mode&&matches!(w,Widget::Network|Widget::Bluetooth|Widget::Sound|Widget::Settings|Widget::Windows)){return;}
        match w{
            Widget::Sound if audio_from_snapshot(owner,point,snapshot)=>return,
            Widget::Metrics=>{if profile.metrics.enabled{show_metrics(owner,anchor,profile.metrics);}return;},
            _=>{}
        }
    }
    if let Some(view)=&snapshot.brightness{control_center::show(owner,anchor,super::brightness::cards(view));return;}
    let make=|title:String,hint:String,icon:Option<Widget>,action|Card{title,hint,icon,action};
    let (title,hint,cards)=if let Some(widget)=widget{
        let mut cards=Vec::new();
        let mut label=utility_hint(widget,snapshot);
        if widget==Widget::Bluetooth{
            if let BluetoothState::Adapter(devices)=&snapshot.bluetooth{
                cards.extend(devices.iter().map(|d|make(d.name.clone(),
                    if d.connected{"Đang kết nối"}else if d.paired{"Đã ghép đôi"}else{"Đã lưu"}.into(),Some(Widget::Bluetooth),None)));
            }
        }
        if let Some(wlan)=snapshot.wlan.as_ref().filter(|_|widget==Widget::Network){
            label=match (wlan.outcome,wlan.status,wlan.no_adapter,wlan.networks.is_empty()){
                (Some(Ok(())),_,_,_)=>"Đã gửi yêu cầu. Làm mới để xem kết quả.".into(),
                (Some(Err(code)),_,_,_)=>format!("Windows chưa thực hiện yêu cầu ({code})."),
                (_,Some(ERROR_ACCESS_DENIED),_,_)=>"Windows chưa cấp quyền đọc danh sách Wi‑Fi.".into(),
                (_,Some(code),_,_)=>format!("Không đọc được danh sách Wi‑Fi ({code})."),
                (_,_,true,_)=>"Không tìm thấy bộ điều hợp Wi‑Fi.".into(),
                (_,_,_,true)=>"Windows chưa trả về mạng khả dụng.".into(),
                _=>label,
            };
            for n in &wlan.networks{
                let action=n.change.clone().map(Command::Wifi).or_else(||{
                    (!n.connected&&!crate::trial::active()).then_some(Command::Run(Action::Network))
                });
                let state=if n.connected{
                    if n.change.is_some(){"Đã kết nối · Ngắt"}else{"Đã kết nối"}
                }else if n.change.is_some(){"Kết nối"}else if action.is_some(){"Mở Windows"}else if n.protected{"Có mật khẩu"}else{"Mạng mở"};
                cards.push(make(n.name.clone(),format!("{}% · {state}",n.signal),Some(Widget::Network),action));
            }
            cards.insert(0,make("Làm mới danh sách".into(),String::new(),Some(Widget::Network),Some(Command::Widget(Widget::Network))));
        }
        if let Some(radios)=snapshot.radios.as_ref(){
            let localized=crate::launcher::localized;
            for radio in &radios.rows{
                let name=if radio.name.trim().is_empty(){if widget==Widget::Network{"Wi-Fi".into()}else{"Bluetooth".into()}}else{radio.name.clone()};
                let state=if radio.disabled{localized("Bị phần cứng hoặc chính sách vô hiệu hóa","Disabled by hardware or policy","已被硬件或策略禁用")}
                    else if radio.on{localized("Đang bật · Tắt","On · Turn off","已开启 · 关闭")}else{localized("Đang tắt · Bật","Off · Turn on","已关闭 · 开启")};
                let state=if crate::trial::active(){format!("{} · {}",if radio.on{localized("Đang bật","On","已开启")}else{localized("Đang tắt","Off","已关闭")},localized("Bản TEST chỉ đọc","Read-only test build","只读测试版本"))}else{state.into()};
                cards.insert(0,make(name,state,Some(widget),radio.request.clone().map(Command::Radio)));
            }
            if let Some(outcome)=radios.outcome{label=match outcome{
                Ok(())=>localized("Đã xác nhận trạng thái mới","New state confirmed","已确认新状态").into(),Err(error)=>super::radios::error_text(error),
            };}else if let Some(error)=radios.error{label=super::radios::error_text(error);}
            else if radios.rows.is_empty(){label=localized("Không có radio tương thích","No compatible radio","没有兼容的无线设备").into();}
            if widget==Widget::Bluetooth{cards.push(make(localized("Làm mới","Refresh","刷新").into(),String::new(),Some(widget),Some(Command::Widget(widget))));}
        }
        let delegated=match widget{
            Widget::Network=>Some(Action::Network),Widget::Bluetooth=>Some(Action::Bluetooth),
            Widget::Sound=>Some(Action::Sound),Widget::Battery=>Some(Action::Battery),
            Widget::QuickSettings if !dock_mode=>Some(Action::QuickSettings),_=>None,
        };
        if widget==Widget::QuickSettings{
            for w in [Widget::Network,Widget::Sound,Widget::Bluetooth]{
                if !profile.hidden.contains(&w){
                    cards.push(make(w.title().into(),utility_hint(w,snapshot),Some(w),Some(Command::Widget(w))));
                }
            }
        }
        if let Some(action)=delegated {
            let hint=match action{
                Action::Network=>"Kết nối mạng mới và nhập mật khẩu",
                Action::Bluetooth=>"Ghép đôi và quản lý thiết bị",
                _=>"Mở cài đặt hệ thống",
            };
            cards.push(make("Mở trong Windows".into(),hint.into(),Some(Widget::Settings),Some(Command::Run(action))));
        }
        (widget.title().to_owned(),label,cards)
    }else{return;};
    if !cards.is_empty(){control_center::show(owner,anchor,View{title,hint,cards,metrics:None});}
}
pub(super) unsafe fn show_metrics(owner:HWND,anchor:chrome::Anchor,cfg:MetricOptions){
    control_center::show(owner,anchor,View{title:"Chỉ số máy".into(),hint:format!("Làm mới mỗi {} giây",cfg.interval),cards:Vec::new(),metrics:Some(cfg)});
}
pub(super) unsafe fn delegated(action:Action){
    use Action::*;
    match action{
        Network=>open("ms-settings:network-wifi"), Bluetooth=>open("ms-settings:bluetooth"),
        Sound=>open("ms-settings:sound"),Battery=>open("ms-settings:powersleep"), Explorer=>open("explorer.exe"),
        RecycleBin=>open("shell:RecycleBinFolder"),
        Notifications=>super::shell_access::win_chord(b'N' as u16),QuickSettings=>super::shell_access::win_chord(b'A' as u16),
        Desktop=>{},Windows=>super::shell_access::win_chord(VK_TAB),
        _=>{}
    }
}
const AUDIO_CLASS:&str="TuanpcDeskAudioControl";
const TBM_GETPOS:u32=WM_USER;
const TRACK:usize=4601;
const MUTE:usize=4602;
const OUTPUT:usize=4603;
const LABEL:usize=4604;
const AUDIO_TIMER:usize=1;
fn audio_read_only()->bool{crate::trial::active()&&!crate::trial::demo_active()}
thread_local! {
    static AUDIO_WINDOW:Cell<HWND>=const{Cell::new(ptr::null_mut())};
    static AUDIO_DEFAULT_COLORS:Cell<bool>=const{Cell::new(false)};
    static AUDIO_DARK:Cell<bool>=const{Cell::new(false)};
    static TRACK_LAST:Cell<u32>=const{Cell::new(0)};
    static AUDIO_PENDING:Cell<bool>=const{Cell::new(false)};
    static TRACK_DRAGGING:Cell<bool>=const{Cell::new(false)};
    static WANTED_LEVEL:Cell<Option<u32>>=const{Cell::new(None)};
}

pub(super) unsafe fn panel_open_for(owner:HWND)->bool{
    if super::weather_city::open_for(owner){return true;}
    if super::overview::open_for(owner){return true;}
    if super::snap_layout::open_for(owner){return true;}
    if super::media::open_for(owner){return true;}
    if super::app_previews::open_for(owner){return true;}
    super::folder_stack::open_for(owner)||super::audio_mixer::open_for(owner)||super::calendar::open_for(owner)||control_center::open_for(owner)||[AUDIO_WINDOW.with(Cell::get)]
        .into_iter().any(|h|!h.is_null()&&GetWindow(h,GW_OWNER)==owner)
}
pub(super) unsafe fn paint_shell(hwnd:HWND,dark:bool,title:&str,hint:&str){
    let mut ps:PAINTSTRUCT=mem::zeroed();let dc=BeginPaint(hwnd,&mut ps);
    let dpi=windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd);
    let px=|n|chrome::scale(dpi,n);
    let mut r:RECT=mem::zeroed();GetClientRect(hwnd,&mut r);
    let p=Palette::new(dark);
    super::fill(dc,r,chrome::color(p.panel));
    chrome::box_fill(dc,r,super::corners::radius(px(32) as f32).round() as i32,p.panel,Some(p.edge));
    let title_font=chrome::choose_font(dpi,19,650);
    let description=chrome::choose_font(dpi,12,450);
    let orig=SelectObject(dc,title_font);
    chrome::text(dc,RECT{left:px(16),top:px(9),right:r.right-px(16),bottom:px(39)},title,p.ink,DT_VCENTER|DT_SINGLELINE|DT_END_ELLIPSIS);
    SelectObject(dc,description);
    chrome::text(dc,RECT{left:px(17),top:px(42),right:r.right-px(16),bottom:px(60)},hint,p.muted,DT_VCENTER|DT_SINGLELINE|DT_END_ELLIPSIS);
    if !orig.is_null(){SelectObject(dc,orig);}
    DeleteObject(title_font);DeleteObject(description);
    EndPaint(hwnd,&ps);
}
// Keep the actual Win32 controls and honor Windows contrast themes. Only
// change how controls owned by this process draw themselves.
pub(super) unsafe fn high_contrast()->bool{
    let mut options:HIGHCONTRASTW=mem::zeroed();
    options.cbSize=mem::size_of::<HIGHCONTRASTW>() as u32;
    SystemParametersInfoW(SPI_GETHIGHCONTRAST,options.cbSize,(&mut options as *mut HIGHCONTRASTW).cast(),0)!=0
        && options.dwFlags&HCF_HIGHCONTRASTON!=0
}
pub(super) unsafe fn track_color(draw:&NMCUSTOMDRAW,dark:bool)->LRESULT{
    if draw.dwDrawStage==CDDS_PREPAINT{return CDRF_NOTIFYITEMDRAW as LRESULT;}
    if draw.dwDrawStage!=CDDS_ITEMPREPAINT{return CDRF_DODEFAULT as LRESULT;}
    let p=Palette::new(dark);
    let size=|n|chrome::scale(windows_sys::Win32::UI::HiDpi::GetDpiForWindow(draw.hdr.hwndFrom),n);
    let r=draw.rc;
    if r.right<=r.left||r.bottom<=r.top{return CDRF_DODEFAULT as LRESULT;}
    let x=(r.left+r.right)/2;
    let y=(r.top+r.bottom)/2;
    match draw.dwItemSpec {
        n if n==TBCD_CHANNEL as usize=>{
            let bar=RECT{top:y-size(2),bottom:y+size(2)+1,..r};
            let radius=size(5);
            chrome::box_fill(draw.hdc,bar,radius,p.edge,None);
            let mut thumb:RECT=mem::zeroed();
            SendMessageW(draw.hdr.hwndFrom,TBM_GETTHUMBRECT,0,(&mut thumb as *mut RECT) as LPARAM);
            let right=((thumb.left+thumb.right)/2).clamp(r.left,r.right);
            if right>r.left{chrome::box_fill(draw.hdc,RECT{right,..bar},radius,p.accent,None);}
        }
        n if n==TBCD_THUMB as usize=>{
            let radius=size(8).min((r.right-r.left).max(2)/2);
            let focused=GetFocus()==draw.hdr.hwndFrom;
            let thumb=RECT{left:x-radius,top:y-radius,right:x+radius+1,bottom:y+radius+1};
            chrome::box_fill(draw.hdc,thumb,2*radius+1,if focused{p.accent}else{p.ink},Some(p.panel));
        }
        _=>return CDRF_DODEFAULT as LRESULT,
    }
    CDRF_SKIPDEFAULT as LRESULT
}
pub(super) unsafe fn paint_button(item:&DRAWITEMSTRUCT,dark:bool){
    let p=Palette::new(dark);
    let active=item.itemState&(ODS_FOCUS|ODS_SELECTED)!=0;
    let dpi=windows_sys::Win32::UI::HiDpi::GetDpiForWindow(item.hwndItem);
    let base=if dark{[73,56,43]}else{[243,234,225]};
    chrome::box_fill(item.hDC,item.rcItem,chrome::scale(dpi,16),base,Some(if active{p.accent}else{p.edge}));
    let mut text=[0u16;128];
    let count=GetWindowTextW(item.hwndItem,text.as_mut_ptr(),text.len() as i32).max(0) as usize;
    chrome::text(item.hDC,item.rcItem,&String::from_utf16_lossy(&text[..count]),
        if item.itemState&ODS_DISABLED!=0{p.muted}else{p.ink},DT_CENTER|DT_SINGLELINE|DT_VCENTER|DT_END_ELLIPSIS);
}
pub(super) unsafe fn close_panels(){
    super::weather_city::close_for(ptr::null_mut());
    super::snap_layout::close_for(ptr::null_mut());
    super::overview::close_for(ptr::null_mut());
    super::media::close_for(ptr::null_mut());
    super::app_previews::close_for(ptr::null_mut());
    super::folder_stack::close_for(ptr::null_mut());
    super::jump_list::close();
    super::audio_mixer::close_for(ptr::null_mut());
    for hwnd in [AUDIO_WINDOW.with(Cell::get)] {
        if !hwnd.is_null(){DestroyWindow(hwnd);}
    }
    control_center::close_for(ptr::null_mut());
    super::calendar::close_for(ptr::null_mut());
}
pub(super) unsafe fn audio_from_snapshot(owner:HWND,point:POINT,snapshot:&Snapshot)->bool{
    let Some(audio)=snapshot.status.audio.as_ref() else{return false;};
    audio_panel(owner,point,audio.level,audio.muted);
    true
}
pub(super) unsafe fn audio_panel(owner:HWND,point:POINT,level:u32,muted:bool){
    close_panels();
    let class=wide(AUDIO_CLASS);
    let instance=GetModuleHandleW(ptr::null());
    if !chrome::register(&class,Some(audio_proc)){return;}
    InitCommonControlsEx(&INITCOMMONCONTROLSEX{dwSize:mem::size_of::<INITCOMMONCONTROLSEX>() as u32,dwICC:ICC_BAR_CLASSES});
    let dpi=windows_sys::Win32::UI::HiDpi::GetDpiForWindow(owner);
    let px=|n|chrome::scale(dpi,n);
    let (width,height)=(px(326),px(169));
    let anchor=chrome::Anchor::for_owner(owner,point);
    let Some(work)=anchor.work()else{return;};
    if width>work.right-work.left||height>work.bottom-work.top{return;}
    let (x,y)=chrome::placement(work,anchor,width,height,px(8));
    AUDIO_DARK.with(|d|d.set(anchor.dark));
    AUDIO_DEFAULT_COLORS.with(|s|s.set(high_contrast()));
    let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_TOPMOST|WS_EX_LAYERED,class.as_ptr(),wide("Âm lượng hệ thống").as_ptr(),
        WS_POPUP|WS_CLIPCHILDREN,x,y,width,height,owner,ptr::null_mut(),instance,ptr::null());
    if hwnd.is_null(){return;}
    chrome::frame(hwnd,width,height,dpi,anchor);
    let control=|name:&str,text:&str,id:usize,flags:u32,x,y,w,h|{
        let child=CreateWindowExW(0,wide(name).as_ptr(),wide(text).as_ptr(),WS_CHILD|WS_VISIBLE|flags,
            px(x),px(y),px(w),px(h),hwnd,id as HMENU,instance,ptr::null());
        if !child.is_null(){
            SendMessageW(child,WM_SETFONT,GetStockObject(DEFAULT_GUI_FONT) as usize,0);
        }
        child
    };
    control("STATIC",&format!("{level}%"),LABEL,2 /* SS_RIGHT */,218,43,88,18);
    let slider=control("msctls_trackbar32","",TRACK,WS_TABSTOP|TBS_NOTICKS,15,68,296,33);
    if slider.is_null(){DestroyWindow(hwnd);return;}
    SendMessageW(slider,TBM_SETRANGEMAX,1,100);
    SendMessageW(slider,TBM_SETPOS,1,level as isize);
    control("BUTTON",if muted{"Bật tiếng"}else{"Tắt tiếng"},MUTE,WS_TABSTOP|BS_OWNERDRAW as u32,16,114,107,36);
    control("BUTTON","Thiết bị đầu ra…",OUTPUT,WS_TABSTOP|BS_OWNERDRAW as u32,132,114,178,36);
    TRACK_LAST.with(|s|s.set(level));AUDIO_PENDING.with(|s|s.set(false));TRACK_DRAGGING.with(|s|s.set(false));
    WANTED_LEVEL.with(|s|s.set(None));AUDIO_WINDOW.with(|s|s.set(hwnd));
    let mute=GetDlgItem(hwnd,MUTE as i32);
    if audio_read_only(){
        EnableWindow(slider,0);
        EnableWindow(mute,0);
    }
    SetTimer(hwnd,AUDIO_TIMER,900,None);
    ShowWindow(hwnd,SW_SHOW);
    SetForegroundWindow(hwnd);
    SetFocus(if audio_read_only(){GetDlgItem(hwnd,OUTPUT as i32)}else{slider});
}

pub(super) unsafe fn close_panels_for(owner:HWND){
    super::weather_city::close_for(owner);
    super::snap_layout::close_for(owner);
    super::overview::close_for(owner);
    super::media::close_for(owner);
    super::app_previews::close_for(owner);
    super::folder_stack::close_for(owner);
    super::audio_mixer::close_for(owner);
    for hwnd in [AUDIO_WINDOW.with(Cell::get)] {
        if !hwnd.is_null()&&GetWindow(hwnd,GW_OWNER)==owner{DestroyWindow(hwnd);}
    }
    control_center::close_for(owner);
    super::calendar::close_for(owner);
}

pub(super) unsafe fn route_panel_keyboard(message:&MSG)->bool{
    if super::weather_city::keyboard(message){return true;}
    if super::media::keyboard(message){return true;}
    if !(WM_KEYFIRST..=WM_KEYLAST).contains(&message.message){return false;}
    if super::audio_mixer::keyboard(message){return true;}
    let panel=[AUDIO_WINDOW.with(Cell::get)].into_iter()
        .find(|&p|!p.is_null()&&(message.hwnd==p||IsChild(p,message.hwnd)!=0)).unwrap_or(ptr::null_mut());
    if panel.is_null(){return false;}
    if message.message==WM_KEYDOWN&&message.wParam as u16==VK_ESCAPE{
        DestroyWindow(panel);return true;
    }
    IsDialogMessageW(panel,message)!=0
}

unsafe fn submit_audio(hwnd:HWND,action:Option<Action>)->bool{
    if AUDIO_PENDING.with(Cell::get)||(action.is_some()&&audio_read_only())||!super::apply_audio(hwnd,action){return false;}
    AUDIO_PENDING.with(|s|s.set(true));
    if action.is_some(){EnableWindow(GetDlgItem(hwnd,MUTE as i32),0);}
    true
}
unsafe fn flush_volume(hwnd:HWND){
    if AUDIO_PENDING.with(Cell::get){return;}
    if let Some(value)=WANTED_LEVEL.with(Cell::get){
        if value==TRACK_LAST.with(Cell::get)||submit_audio(hwnd,Some(Action::SetVolume(value))){
            WANTED_LEVEL.with(|s|s.set(None));
        }
    }
}
pub(super) unsafe fn reflect_audio(hwnd:HWND,result:Option<Audio>){
    if hwnd.is_null()||AUDIO_WINDOW.with(Cell::get)!=hwnd||!AUDIO_PENDING.with(|s|s.replace(false)){return;}
    if !audio_read_only(){EnableWindow(GetDlgItem(hwnd,MUTE as i32),1);}
    let label=GetDlgItem(hwnd,LABEL as i32);
    if let Some(audio)=result{
        TRACK_LAST.with(|s|s.set(audio.level));
        SetWindowTextW(GetDlgItem(hwnd,MUTE as i32),wide(if audio.muted{"Bật tiếng"}else{"Tắt tiếng"}).as_ptr());
        if !TRACK_DRAGGING.with(Cell::get)&&WANTED_LEVEL.with(Cell::get).is_none(){
            let slider=GetDlgItem(hwnd,TRACK as i32);
            if SendMessageW(slider,TBM_GETPOS,0,0)!=audio.level as isize{SendMessageW(slider,TBM_SETPOS,1,audio.level as isize);}
            SetWindowTextW(label,wide(&format!("{}%",audio.level)).as_ptr());
        }
    }else{
        let last=TRACK_LAST.with(Cell::get);
        if !TRACK_DRAGGING.with(Cell::get)&&WANTED_LEVEL.with(Cell::get).is_none(){
            SendMessageW(GetDlgItem(hwnd,TRACK as i32),TBM_SETPOS,1,last as isize);
        }
        SetWindowTextW(label,wide("Không cập nhật được thiết bị âm thanh").as_ptr());
    }
    flush_volume(hwnd);
}
unsafe extern "system" fn audio_proc(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
    match msg {
        WM_ERASEBKGND=>1,
        WM_PAINT=>{paint_shell(hwnd,AUDIO_DARK.with(Cell::get),"Âm lượng",
            if audio_read_only(){"Chỉ xem trong bản thử"}else{"Thiết bị đầu ra mặc định"});0},
        WM_TIMER if w==AUDIO_TIMER=>{
            if !TRACK_DRAGGING.with(Cell::get)&&WANTED_LEVEL.with(Cell::get).is_none(){submit_audio(hwnd,None);}
            else{flush_volume(hwnd);}
            0
        },
        WM_CTLCOLORSTATIC|WM_CTLCOLORBTN=>chrome::child_color(w as HDC,AUDIO_DARK.with(Cell::get)),
        WM_DRAWITEM if l!=0=>{paint_button(&*(l as *const DRAWITEMSTRUCT),AUDIO_DARK.with(Cell::get));1},
        WM_NOTIFY if l!=0=>{
            let header=&*(l as *const NMHDR);
            if header.idFrom==TRACK&&header.code==NM_CUSTOMDRAW&&!AUDIO_DEFAULT_COLORS.with(Cell::get){
                track_color(&*(l as *const NMCUSTOMDRAW),AUDIO_DARK.with(Cell::get))
            }else{DefWindowProcW(hwnd,msg,w,l)}
        },
        WM_HSCROLL if l!=0&&l as HWND==GetDlgItem(hwnd,TRACK as i32)=>{
            let value=SendMessageW(l as HWND,TBM_GETPOS,0,0).clamp(0,100) as u32;
            SetWindowTextW(GetDlgItem(hwnd,LABEL as i32),wide(&format!("{value}%")).as_ptr());
            let dragging=w as u32&0xffff==TB_THUMBTRACK;
            TRACK_DRAGGING.with(|s|s.set(dragging));
            // Respond while dragging or using the keyboard. Endpoint writes
            // remain serialized and intermediate slider positions coalesce.
            WANTED_LEVEL.with(|s|s.set(Some(value)));
            flush_volume(hwnd);
            0
        },
        WM_COMMAND=>match w&0xffff{
            n if n==IDCANCEL as usize=>{DestroyWindow(hwnd);0},
            MUTE=>{
                submit_audio(hwnd,Some(Action::Mute));
                0
            }
            OUTPUT=>{open("ms-settings:sound");DestroyWindow(hwnd);0},
            _=>DefWindowProcW(hwnd,msg,w,l),
        }
        WM_ACTIVATE if w&0xffff==WA_INACTIVE as usize=>{PostMessageW(hwnd,WM_CLOSE,0,0);0},
        WM_NCDESTROY=>{
            KillTimer(hwnd,AUDIO_TIMER);
            super::panel_closed(GetWindow(hwnd,GW_OWNER));
            if AUDIO_WINDOW.with(Cell::get)==hwnd{AUDIO_WINDOW.with(|s|s.set(ptr::null_mut()));}
            DefWindowProcW(hwnd,msg,w,l)
        }
        _=>DefWindowProcW(hwnd,msg,w,l),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_status_maps_signal_and_volume_without_guessing(){
        for (p,bars) in [(0,1),(24,1),(25,2),(50,3),(75,4),(100,4)]{
            let s=Status{network:Some(Link::WiFi(Some(p))),..Status::default()};
            assert_eq!(s.network_glyph(),NetworkGlyph::Wifi(bars));
        }
        for (link,shape) in [(Link::WiFi(None),NetworkGlyph::Wifi(0)),(Link::Wired,NetworkGlyph::Wired),
            (Link::Disconnected,NetworkGlyph::Disconnected),(Link::Disabled,NetworkGlyph::Disabled)]{
            assert_eq!(Status{network:Some(link),..Status::default()}.network_glyph(),shape);
        }
        for (level,muted,shape) in [(0,false,VolumeGlyph::Muted),(25,false,VolumeGlyph::Waves(1)),
            (50,false,VolumeGlyph::Waves(2)),(75,false,VolumeGlyph::Waves(3)),
            (100,false,VolumeGlyph::Waves(3)),(75,true,VolumeGlyph::Muted)]{
            assert_eq!(Status{audio:Some(Audio{level,muted}),..Status::default()}.volume_glyph(),shape);
        }
        assert_eq!(Status::default().volume_glyph(),VolumeGlyph::Unknown);
    }
    #[test]
    fn fixture_endpoint_writes_are_observable_and_mute_preserves_level() {
        let mut sample=Audio{level:46,muted:false};
        assert_eq!(fixture_audio(&mut sample,JobKind::Read).level,46);
        assert_eq!(fixture_audio(&mut sample,JobKind::Volume(73)).level,73);
        let muted=fixture_audio(&mut sample,JobKind::Mute);
        assert!(muted.muted);
        assert_eq!(muted.level,73);
        assert!(!fixture_audio(&mut sample,JobKind::Mute).muted);
    }
    #[test]
    fn network_does_not_invent_a_signal_when_os_has_no_data(){
        assert!(!network_label(Some(Link::WiFi(None))).contains('%'));
        assert!(network_label(Some(Link::WiFi(Some(72)))).contains("72%"));
        assert!(network_label(None).contains("Không đọc được"));
        assert_ne!(network_label(Some(Link::None)),network_label(None));
    }
    #[test]
    #[ignore = "Windows on-demand hardware queries; explicitly run in CI"]
    fn native_on_demand_provider_completes_without_an_assumed_device(){
        unsafe {
            let hwnd=CreateWindowExW(0,wide("STATIC").as_ptr(),ptr::null(),WS_POPUP,0,0,1,1,
                ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),ptr::null());
            assert!(!hwnd.is_null());
            let provider=Provider::new();
            assert!(provider.read(hwnd,Some(Widget::Network),None));
            let deadline=std::time::Instant::now()+std::time::Duration::from_secs(10);
            let snapshot=loop{
                if let Some(s)=provider.take(){break Some(s);}
                if std::time::Instant::now()>=deadline{break None;}
                std::thread::sleep(std::time::Duration::from_millis(25));
            };
            DestroyWindow(hwnd);
            let report=snapshot.expect("read-only probe must complete in a bounded time");
            assert!(report.wlan.is_some());
            if let Some(audio)=report.status.audio{assert!(audio.level<=100);}
            if let BatteryState::Present{level:Some(level),..}=report.battery{assert!(level<=100);}
        }
    }
}

#[cfg(test)]
mod dock_mode_tests{
    use super::*;
    #[test]
    fn dock_mode_has_no_windows_key_delegation_in_its_utilities(){
        for widget in Widget::ALL{
            if available_in_dock_mode(Some(widget))&&!matches!(widget,Widget::QuickSettings|Widget::Windows){
                assert!(widget.simple().is_none_or(local_action),"{:?}",widget);
            }
        }
    }
}
