//! One bounded, on-demand provider for all Dock displays. No probes while
//! idle, no WLAN or Bluetooth scans, and no Shell/COM calls on the UI thread.
use super::{WM_UTILITY_READY,open};
use crate::desktop::wide;
use serde_json::{json,Value};
use std::{cell::Cell,mem,ptr,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}}};
use windows::Win32::Media::Audio::{*,Endpoints::IAudioEndpointVolume};
use windows::Win32::System::Com::*;
use windows_sys::Win32::{
    Devices::Bluetooth::*,
    Foundation::*,
    Graphics::Gdi::{GetStockObject,DEFAULT_GUI_FONT,GetMonitorInfoW,MonitorFromPoint,MONITORINFO,MONITOR_DEFAULTTONEAREST,COLOR_WINDOW},
    NetworkManagement::{IpHelper::*,Ndis::IfOperStatusUp,WiFi::*},
    System::{LibraryLoader::GetModuleHandleW,Power::*,SystemInformation::GetLocalTime,Threading::{TrySubmitThreadpoolCallback,PTP_CALLBACK_INSTANCE}},
    UI::{Controls::{InitCommonControlsEx,INITCOMMONCONTROLSEX,ICC_BAR_CLASSES,ICC_DATE_CLASSES,MCM_GETMINREQRECT,TBM_SETPOS,TBM_SETRANGEMAX,TB_THUMBTRACK,TBS_NOTICKS},Input::KeyboardAndMouse::*,WindowsAndMessaging::*},
};

#[derive(Clone, Copy, Debug)]
enum Link { None, Wired, WiFi(Option<u8>), Both(Option<u8>) }
enum BluetoothState { Unknown, Adapter, Connected(Vec<String>) }
#[derive(Clone,Copy)]
enum BatteryState { Unknown, Absent, Present{level:Option<u8>,ac:Option<bool>,charging:bool} }
pub(super) struct Audio { level:u32, muted:bool }
pub(super) struct Snapshot { network:Option<Link>, bluetooth:BluetoothState, battery:BatteryState, audio:Option<Audio>, pub(super) local:SYSTEMTIME }

#[derive(Clone,Copy)]
enum JobKind { Read, Volume(u32), Mute }

/// Fixture-only endpoint. Neither moving the fixture slider nor rapidly
/// toggling its mute can write to a Windows audio device.
static DEMO_VOLUME: Mutex<Audio> = Mutex::new(Audio { level: 46, muted: false });
fn fixture_audio(value:&mut Audio, kind:JobKind)->Audio {
    match kind {
        JobKind::Read=>{},
        JobKind::Volume(value_percent)=>value.level=value_percent.min(100),
        JobKind::Mute=>value.muted=!value.muted,
    }
    Audio {level:value.level,muted:value.muted}
}
#[derive(Default)]
struct State {
    pending:AtomicBool, result:Mutex<Option<Snapshot>>,
    audio_result:Mutex<Option<Option<Audio>>>, closed:AtomicBool,
}
static WORK_ACTIVE:AtomicBool=AtomicBool::new(false);
struct Permit;
impl Drop for Permit{fn drop(&mut self){WORK_ACTIVE.store(false,Ordering::Release);}}
struct Job { shared:Arc<State>, owner:usize, kind:JobKind, permit:Permit }
pub(super) struct Provider { state:Arc<State> }
impl Provider {
    pub fn new()->Self{Self{state:Arc::default()}}
    fn submit(&self,owner:HWND,kind:JobKind)->bool {
        if owner.is_null()||self.state.pending.swap(true,Ordering::AcqRel){return false;}
        if WORK_ACTIVE.swap(true,Ordering::AcqRel){
            self.state.pending.store(false,Ordering::Release);
            return false;
        }
        let raw=Box::into_raw(Box::new(Job{shared:self.state.clone(),owner:owner as usize,kind,permit:Permit}));
        if unsafe{TrySubmitThreadpoolCallback(Some(worker),raw.cast(),ptr::null())}==0 {
            unsafe{drop(Box::from_raw(raw));}
            self.state.pending.store(false,Ordering::Release);
            return false;
        }
        true
    }
    pub fn read(&self,owner:HWND)->bool{self.submit(owner,JobKind::Read)}
    pub fn take(&self)->Option<Snapshot>{self.state.result.lock().ok()?.take()}
    pub fn take_audio_result(&self)->Option<Option<Audio>>{self.state.audio_result.lock().ok()?.take()}
    pub fn apply(&self,owner:HWND,action:Action)->bool{
        let kind=match action{Action::SetVolume(p)=>JobKind::Volume(p),Action::Mute=>JobKind::Mute,_=>return false};
        self.submit(owner,kind)
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
            let mut local=mem::zeroed();GetLocalTime(&mut local);
            let mut bluetooth=bluetooth();
            if crate::trial::demo_active() {
                if let BluetoothState::Connected(names)=&mut bluetooth {
                    for (i,name) in names.iter_mut().enumerate(){*name=format!("Fixture device {}",i+1);}
                }
            }
            let snapshot=Snapshot{network:network(),bluetooth,battery:battery(),audio,local};
            if let Ok(mut result)=state.result.lock(){*result=Some(snapshot);}
        }else if let Ok(mut result)=state.audio_result.lock(){
            *result=Some(audio);
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
    if crate::trial::active() && !matches!(kind,JobKind::Read){return None;}
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
            JobKind::Read=>{}
        }
        let level=(volume.GetMasterVolumeLevelScalar()?.clamp(0.0,1.0)*100.0).round() as u32;
        Ok(Audio{level,muted:volume.GetMute()?.as_bool()})
    })().ok();
    CoUninitialize();
    value
}

unsafe fn network()->Option<Link>{
    let mut table=ptr::null_mut();
    if GetIfTable2(&mut table)!=0||table.is_null(){return None;}
    let mut lan=false;let mut wifi=false;
    for i in 0..(*table).NumEntries.min(1024) as usize {
        let row=&*ptr::addr_of!((*table).Table).cast::<MIB_IF_ROW2>().add(i);
        if row.OperStatus!=IfOperStatusUp||row.InterfaceAndOperStatusFlags._bitfield&1==0{continue;}
        // IANA ifType: 6 ethernetCsmacd, 71 ieee80211. Only physical adapters.
        lan|=row.Type==6;wifi|=row.Type==71;
    }
    FreeMibTable(table.cast());
    let signal=if wifi{wifi_signal()}else{None};
    Some(match (lan,wifi){(true,true)=>Link::Both(signal),(true,false)=>Link::Wired,(false,true)=>Link::WiFi(signal),_=>Link::None})
}
unsafe fn wifi_signal()->Option<u8>{
    let mut h=ptr::null_mut();let mut version=0;
    if WlanOpenHandle(2,ptr::null(),&mut version,&mut h)!=0||h.is_null(){return None;}
    let mut list=ptr::null_mut();let mut signal=None;
    if WlanEnumInterfaces(h,ptr::null(),&mut list)==0&&!list.is_null(){
        for i in 0..(*list).dwNumberOfItems.min(32) as usize {
            let iface=&*ptr::addr_of!((*list).InterfaceInfo).cast::<WLAN_INTERFACE_INFO>().add(i);
            if iface.isState!=wlan_interface_state_connected{continue;}
            let (mut length,mut info)=(0,ptr::null_mut());
            if WlanQueryInterface(h,&iface.InterfaceGuid,wlan_intf_opcode_current_connection,ptr::null(),&mut length,&mut info,ptr::null_mut())==0&&!info.is_null(){
                if length as usize>=mem::size_of::<WLAN_CONNECTION_ATTRIBUTES>(){
                    let strength=(*info.cast::<WLAN_CONNECTION_ATTRIBUTES>()).wlanAssociationAttributes.wlanSignalQuality.min(100) as u8;
                    signal=Some(signal.map_or(strength,|v:u8|v.max(strength)));
                }
                WlanFreeMemory(info);
            }
        }
        WlanFreeMemory(list.cast());
    }
    WlanCloseHandle(h,ptr::null());
    signal
}
unsafe fn bluetooth()->BluetoothState{
    let mut radio=ptr::null_mut();
    let params=BLUETOOTH_FIND_RADIO_PARAMS{dwSize:mem::size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32};
    let find=BluetoothFindFirstRadio(&params,&mut radio);
    if find.is_null(){return BluetoothState::Unknown;}
    let mut names=Vec::new();
    for index in 0..16 {
        if radio.is_null(){break;}
        let mut params:BLUETOOTH_DEVICE_SEARCH_PARAMS=mem::zeroed();
        params.dwSize=mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32;
        params.fReturnConnected=1;params.hRadio=radio;
        let mut info:BLUETOOTH_DEVICE_INFO=mem::zeroed();
        info.dwSize=mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
        let device=BluetoothFindFirstDevice(&params,&mut info);
        if !device.is_null(){
            for _ in 0..48{
                if info.fConnected!=0{
                    let length=info.szName.iter().position(|&c|c==0).unwrap_or(info.szName.len());
                    let name=String::from_utf16_lossy(&info.szName[..length]).trim().to_owned();
                    if name.is_empty(){names.push("Thiết bị chưa có tên".into());}
                    else if !names.contains(&name){names.push(name);}
                }
                if names.len()>=8||BluetoothFindNextDevice(device,&mut info)==0{break;}
            }
            BluetoothFindDeviceClose(device);
        }
        CloseHandle(radio);
        if names.len()>=8||index==15||BluetoothFindNextRadio(find,&mut radio)==0{break;}
    }
    BluetoothFindRadioClose(find);
    if names.is_empty(){BluetoothState::Adapter}else{BluetoothState::Connected(names)}
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
    Network, Sound, Bluetooth, Battery, Calendar, Notifications, QuickSettings,
    Spotlight, Explorer, Desktop, Capture, Record, Stats, Wallpaper, Stage, Settings, Windows,
}
impl Widget {
    pub const ALL:[Self;17]=[Self::Network,Self::Sound,Self::Bluetooth,Self::Battery,Self::Calendar,
        Self::Notifications,Self::QuickSettings,Self::Spotlight,Self::Explorer,Self::Desktop,
        Self::Capture,Self::Record,Self::Stats,Self::Wallpaper,Self::Stage,Self::Settings,Self::Windows];
    pub fn key(self)->&'static str{
        use Widget::*;
        match self {Network=>"network",Sound=>"audio",Bluetooth=>"bluetooth",Battery=>"battery",Calendar=>"calendar",
            Notifications=>"notifications",QuickSettings=>"quickSettings",Spotlight=>"spotlight",Explorer=>"explorer",
            Desktop=>"desktop",Capture=>"capture",Record=>"record",Stats=>"stats",Wallpaper=>"wallpaper",Stage=>"stage",
            Settings=>"settings",Windows=>"windows"}
    }
    pub fn title(self)->&'static str{
        use Widget::*;
        match self{Network=>"Mạng",Sound=>"Âm thanh",Bluetooth=>"Bluetooth",Battery=>"Pin",Calendar=>"Ngày giờ và lịch",
            Notifications=>"Trung tâm thông báo Windows",QuickSettings=>"Cài đặt nhanh Windows",Spotlight=>"Spotlight",
            Explorer=>"File Explorer",Desktop=>"Hiện/ẩn desktop",Capture=>"Chụp màn hình",Record=>"Quay màn hình",
            Stats=>"Bật/tắt chỉ số taskbar",Wallpaper=>"Bật/tắt hình nền động",Stage=>"Bật/tắt Stage Manager",
            Settings=>"Cài đặt tuanpc-desk",Windows=>"Chuyển cửa sổ Windows"}
    }
    pub(super) fn simple(self)->Option<Action>{
        use Widget::*;
        Some(match self{
            Notifications=>Action::Notifications,QuickSettings=>Action::QuickSettings,
            Spotlight=>Action::Host(HostAction::Launcher),Explorer=>Action::Explorer,Desktop=>Action::Desktop,
            Capture=>Action::Host(HostAction::Capture),Record=>Action::Host(HostAction::Record),
            Stats=>Action::Host(HostAction::SystemMonitor),Wallpaper=>Action::Host(HostAction::Wallpaper),
            Stage=>Action::Host(HostAction::Stage),Settings=>Action::Host(HostAction::Settings),Windows=>Action::Windows,
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
            Calendar=>&[(0.24,0.30,0.76,0.30),(0.76,0.30,0.76,0.77),(0.76,0.77,0.24,0.77),(0.24,0.77,0.24,0.30),(0.24,0.45,0.76,0.45),(0.38,0.24,0.38,0.35),(0.62,0.24,0.62,0.35),(0.40,0.60,0.47,0.60),(0.54,0.60,0.61,0.60)],
            Notifications=>&[(0.21,0.68,0.79,0.68),(0.24,0.65,0.34,0.51),(0.34,0.51,0.34,0.38),(0.34,0.38,0.50,0.27),(0.50,0.27,0.66,0.38),(0.66,0.38,0.66,0.51),(0.66,0.51,0.76,0.65),(0.44,0.77,0.56,0.77)],
            QuickSettings|Settings=>&[(0.22,0.35,0.78,0.35),(0.22,0.65,0.78,0.65),(0.40,0.28,0.40,0.42),(0.61,0.58,0.61,0.72)],
            Spotlight=>&[(0.30,0.38,0.40,0.28),(0.40,0.28,0.55,0.28),(0.55,0.28,0.65,0.38),(0.65,0.38,0.65,0.53),(0.65,0.53,0.55,0.63),(0.55,0.63,0.40,0.63),(0.40,0.63,0.30,0.53),(0.30,0.53,0.30,0.38),(0.61,0.59,0.77,0.76)],
            Explorer=>&[(0.20,0.38,0.20,0.72),(0.20,0.72,0.80,0.72),(0.80,0.72,0.80,0.38),(0.80,0.38,0.48,0.38),(0.48,0.38,0.39,0.28),(0.39,0.28,0.20,0.28),(0.20,0.28,0.20,0.38),(0.20,0.45,0.80,0.45)],
            Desktop|Wallpaper=>&[(0.20,0.28,0.80,0.28),(0.80,0.28,0.80,0.68),(0.80,0.68,0.20,0.68),(0.20,0.68,0.20,0.28),(0.50,0.68,0.50,0.79),(0.36,0.79,0.64,0.79)],
            Capture=>&[(0.23,0.34,0.43,0.34),(0.23,0.34,0.23,0.70),(0.23,0.70,0.77,0.70),(0.77,0.70,0.77,0.34),(0.77,0.34,0.58,0.34),(0.43,0.34,0.49,0.26),(0.49,0.26,0.56,0.26),(0.56,0.26,0.58,0.34),(0.42,0.48,0.58,0.48),(0.42,0.48,0.42,0.60),(0.42,0.60,0.58,0.60),(0.58,0.60,0.58,0.48)],
            Record=>&[(0.33,0.32,0.66,0.32),(0.66,0.32,0.75,0.41),(0.75,0.41,0.75,0.66),(0.75,0.66,0.66,0.73),(0.66,0.73,0.33,0.73),(0.33,0.73,0.25,0.66),(0.25,0.66,0.25,0.41),(0.25,0.41,0.33,0.32),(0.44,0.49,0.56,0.49),(0.44,0.55,0.56,0.55)],
            Stats=>&[(0.24,0.73,0.76,0.73),(0.32,0.73,0.32,0.53),(0.50,0.73,0.50,0.39),(0.68,0.73,0.68,0.26)],
            Stage=>&[(0.25,0.31,0.72,0.31),(0.72,0.31,0.72,0.68),(0.72,0.68,0.25,0.68),(0.25,0.68,0.25,0.31),(0.32,0.24,0.79,0.24),(0.79,0.24,0.79,0.59)],
            Windows=>&[(0.22,0.34,0.56,0.34),(0.56,0.34,0.56,0.68),(0.56,0.68,0.22,0.68),(0.22,0.68,0.22,0.34),(0.61,0.34,0.78,0.34),(0.78,0.34,0.78,0.68),(0.78,0.68,0.61,0.68)],
        }
    }
    fn command(self)->usize{301+Self::ALL.iter().position(|v|*v==self).unwrap_or(0)}
}
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct Profile {
    pub enabled:bool,pub leading:bool,pub details:bool,pub cell:u32,
    pub order:Vec<Widget>,pub hidden:Vec<Widget>,pub inline:Vec<Widget>,
}
impl Default for Profile {
    fn default()->Self{Self{enabled:true,leading:false,details:true,cell:28,order:Widget::ALL.to_vec(),hidden:Vec::new(),inline:Vec::new()}}
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
        let inline=read("inline").into_iter().take(5).collect();
        Self{enabled:v["enabled"].as_bool().unwrap_or(true),leading:v["side"]=="left",
            details:v["details"].as_bool().unwrap_or(true),cell:v["cell"].as_u64().map_or(28,|n|n.clamp(24,32) as u32),order,hidden,inline}
    }
    pub fn json(&self)->Value{
        json!({"enabled":self.enabled,"side":if self.leading{"left"}else{"right"},"details":self.details,"cell":self.cell,
            "order":self.order.iter().map(|w|w.key()).collect::<Vec<_>>(),
            "hidden":self.hidden.iter().map(|w|w.key()).collect::<Vec<_>>(),
            "inline":self.inline.iter().map(|w|w.key()).collect::<Vec<_>>()})
    }
    pub fn pinned(&self)->Vec<Widget>{
        if !self.enabled{return Vec::new();}
        let show_battery=self.inline.contains(&Widget::Battery)&&unsafe{matches!(battery(),BatteryState::Present{..})};
        self.order.iter().copied().filter(|w|self.inline.contains(w)&&!self.hidden.contains(w)&&(*w!=Widget::Battery||show_battery)).collect()
    }
    pub fn toggle(&mut self,widget:Widget){if self.hidden.contains(&widget){self.hidden.retain(|&w|w!=widget);}else{self.hidden.push(widget);}}
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum HostAction { Settings, Capture, Record, SystemMonitor, Wallpaper, Stage, Launcher }
#[derive(Clone,Copy)]
pub(super) enum Action {
    Host(HostAction), Network, Bluetooth, Sound, Battery, Calendar, Notifications,
    QuickSettings, Explorer, Desktop, Windows, SetVolume(u32), Mute, AudioPanel{level:u32,muted:bool},
}
const SET_LEVEL:usize=100;
fn network_label(state:Option<Link>)->String{
    match state {
        None=>"Mạng · Không đọc được trạng thái".into(),
        Some(Link::None)=>"Mạng · Không có kết nối vật lý".into(),
        Some(Link::Wired)=>"Mạng · Ethernet đã kết nối".into(),
        Some(Link::WiFi(signal)|Link::Both(signal))=>{
            let kind=if matches!(state,Some(Link::Both(_))){"Ethernet và Wi-Fi"}else{"Wi-Fi"};
            signal.map_or_else(||format!("Mạng · {kind} đã kết nối"),|q|format!("Mạng · {kind}, tín hiệu {q}%"))
        }
    }
}
pub(super) unsafe fn menu(owner:HWND,point:POINT,snapshot:&Snapshot,profile:&Profile)->Option<Action>{
    let menu=CreatePopupMenu();
    if menu.is_null(){return None;}
    let row=|id,text:&str|{AppendMenuW(menu,MF_STRING,id,wide(text).as_ptr());};
    for &widget in &profile.order {
        if profile.hidden.contains(&widget){continue;}
        let id=widget.command();
        match widget {
            Widget::Network=>if profile.details{row(id,&network_label(snapshot.network));}else{row(id,widget.title());},
            Widget::Sound=>{
                let sound=CreatePopupMenu();
                if sound.is_null(){continue;}
                if let Some(a)=snapshot.audio.as_ref() {
                    AppendMenuW(sound,MF_STRING,9,wide("Điều chỉnh âm lượng…").as_ptr());
                    AppendMenuW(sound,MF_STRING,10,wide(if a.muted{"Bật tiếng"}else{"Tắt tiếng"}).as_ptr());
                    for p in [0u32,10,20,30,40,50,60,70,80,90,100]{
                        AppendMenuW(sound,MF_STRING|if a.level==p{MF_CHECKED}else{0},SET_LEVEL+p as usize,wide(&format!("{p}%")).as_ptr());
                    }
                    AppendMenuW(sound,MF_SEPARATOR,0,ptr::null());
                }
                AppendMenuW(sound,MF_STRING,id,wide("Chọn thiết bị âm thanh trong Windows…").as_ptr());
                let title=if profile.details{
                    snapshot.audio.as_ref().map_or_else(||"Âm thanh · Không đọc được thiết bị".into(),|a|{
                        if a.muted{format!("Âm thanh · Đang tắt ({0}%)",a.level)}else{format!("Âm thanh · {}%",a.level)}
                    })
                }else{widget.title().into()};
                if AppendMenuW(menu,MF_POPUP,sound as usize,wide(&title).as_ptr())==0{DestroyMenu(sound);}
            }
            Widget::Bluetooth if profile.details=>match &snapshot.bluetooth{
                BluetoothState::Unknown=>row(id,"Bluetooth · Mở cài đặt Windows…"),
                BluetoothState::Adapter=>row(id,"Bluetooth · Có bộ điều hợp"),
                BluetoothState::Connected(names)=>{
                    let sub=CreatePopupMenu();
                    if sub.is_null(){row(id,"Bluetooth · Thiết bị Classic đang kết nối");continue;}
                    for name in names{
                        let label=name.chars().filter(|c|!c.is_control()).take(64).collect::<String>().replace('&',"&&");
                        AppendMenuW(sub,MF_STRING|MF_DISABLED,0,wide(&label).as_ptr());
                    }
                    AppendMenuW(sub,MF_SEPARATOR,0,ptr::null());
                    AppendMenuW(sub,MF_STRING,id,wide("Quản lý Bluetooth trong Windows…").as_ptr());
                    if AppendMenuW(menu,MF_POPUP,sub as usize,wide("Bluetooth · Thiết bị Classic đang kết nối").as_ptr())==0{
                        DestroyMenu(sub);row(id,"Bluetooth · Mở cài đặt Windows…");
                    }
                }
            },
            Widget::Battery=>if let BatteryState::Present{level,ac,charging}=snapshot.battery {
                let label=if profile.details{
                    let value=level.map_or("—".into(),|p|format!("{p}%"));
                    let mode=if charging{"Đang sạc"}else{match ac{Some(true)=>"Nguồn AC",Some(false)=>"Dùng pin",None=>"Không rõ nguồn"}};
                    format!("Pin · {value} · {mode}")
                }else{widget.title().into()};
                row(id,&label);
            },
            Widget::Calendar if profile.details=>{
                let d=&snapshot.local;
                row(id,&format!("{:02}:{:02} · {:02}/{:02}/{} · Lịch",d.wHour,d.wMinute,d.wDay,d.wMonth,d.wYear));
            }
            _=>row(id,widget.title()),
        }
    }
    if GetMenuItemCount(menu)==0{DestroyMenu(menu);return None;}
    let previous=GetForegroundWindow();
    SetForegroundWindow(owner);
    let cmd=TrackPopupMenu(menu,TPM_RETURNCMD|TPM_NONOTIFY|TPM_BOTTOMALIGN,point.x,point.y,0,owner,ptr::null()) as usize;
    DestroyMenu(menu);
    if GetForegroundWindow()==owner&&!previous.is_null()&&IsWindow(previous)!=0{SetForegroundWindow(previous);}
    use Action::*;
    if cmd==9{return snapshot.audio.as_ref().map(|a|AudioPanel{level:a.level,muted:a.muted});}
    if cmd==10{return Some(Mute);}
    if (SET_LEVEL..=200).contains(&cmd){return Some(SetVolume((cmd-SET_LEVEL) as u32));}
    Some(match *Widget::ALL.get(cmd.checked_sub(301)?)? {
        Widget::Network=>Network,Widget::Sound=>Sound,Widget::Bluetooth=>Bluetooth,Widget::Battery=>Battery,
        Widget::Calendar=>Calendar,Widget::Notifications=>Notifications,Widget::QuickSettings=>QuickSettings,
        Widget::Spotlight=>Host(HostAction::Launcher),Widget::Explorer=>Explorer,Widget::Desktop=>Desktop,
        Widget::Capture=>Host(HostAction::Capture),Widget::Record=>Host(HostAction::Record),
        Widget::Stats=>Host(HostAction::SystemMonitor),Widget::Wallpaper=>Host(HostAction::Wallpaper),
        Widget::Stage=>Host(HostAction::Stage),Widget::Settings=>Host(HostAction::Settings),Widget::Windows=>Windows,
    })
}
pub(super) unsafe fn delegated(action:Action){
    use Action::*;
    match action{
        Network=>open("ms-settings:network-status"), Bluetooth=>open("ms-settings:bluetooth"),
        Sound=>open("ms-settings:sound"),Battery=>open("ms-settings:powersleep"), Explorer=>open("explorer.exe"),
        Notifications=>shell_key(b'N' as u16),QuickSettings=>shell_key(b'A' as u16),
        Desktop=>shell_key(b'D' as u16),Windows=>shell_key(VK_TAB),
        _=>{}
    }
}
unsafe fn shell_key(key:u16){
    // These are explicit user clicks. Avoid injecting a chord into an
    // existing held modifier state and releasing the user's physical keys.
    if [VK_LWIN,VK_RWIN,VK_SHIFT,VK_CONTROL,VK_MENU,key].into_iter().any(|vk|GetAsyncKeyState(vk as i32)<0){return;}
    let event=|vk,flags| INPUT{r#type:INPUT_KEYBOARD,Anonymous:INPUT_0{ki:KEYBDINPUT{
        wVk:vk,wScan:0,dwFlags:flags,time:0,dwExtraInfo:0}}};
    let input=[event(VK_LWIN,0),event(key,0),event(key,KEYEVENTF_KEYUP),event(VK_LWIN,KEYEVENTF_KEYUP)];
    let n=SendInput(input.len() as u32,input.as_ptr(),mem::size_of::<INPUT>() as i32);
    if n>0&&n<4 {
        let tail=if n==2{&input[2..]}else{&input[3..]};
        SendInput(tail.len() as u32,tail.as_ptr(),mem::size_of::<INPUT>() as i32);
    }
}

const AUDIO_CLASS:&str="TuanpcDeskAudioControl";
const CALENDAR_CLASS:&str="TuanpcDeskCalendar";
const TBM_GETPOS:u32=WM_USER;
const TRACK:usize=4601;
const MUTE:usize=4602;
const OUTPUT:usize=4603;
const LABEL:usize=4604;
thread_local! {
    static AUDIO_WINDOW:Cell<HWND>=const{Cell::new(ptr::null_mut())};
    static CALENDAR_WINDOW:Cell<HWND>=const{Cell::new(ptr::null_mut())};
    static TRACK_LAST:Cell<u32>=const{Cell::new(0)};
    static AUDIO_PENDING:Cell<bool>=const{Cell::new(false)};
    static TRACK_DRAGGING:Cell<bool>=const{Cell::new(false)};
    static WANTED_LEVEL:Cell<Option<u32>>=const{Cell::new(None)};
}

pub(super) unsafe fn close_panels(){
    for hwnd in [AUDIO_WINDOW.with(Cell::get),CALENDAR_WINDOW.with(Cell::get)] {
        if !hwnd.is_null(){DestroyWindow(hwnd);}
    }
}
pub(super) unsafe fn audio_from_snapshot(owner:HWND,point:POINT,snapshot:&Snapshot)->bool{
    let Some(audio)=snapshot.audio.as_ref() else{return false;};
    audio_panel(owner,point,audio.level,audio.muted);
    true
}
pub(super) unsafe fn audio_panel(owner:HWND,point:POINT,level:u32,muted:bool){
    close_panels();
    let class=wide(AUDIO_CLASS);
    let instance=GetModuleHandleW(ptr::null());
    let mut wc:WNDCLASSW=mem::zeroed();
    wc.hInstance=instance;wc.lpszClassName=class.as_ptr();wc.lpfnWndProc=Some(audio_proc);
    wc.hbrBackground=(COLOR_WINDOW as usize+1) as _;
    wc.hCursor=LoadCursorW(ptr::null_mut(),IDC_ARROW);
    if RegisterClassW(&wc)==0&&GetLastError()!=ERROR_CLASS_ALREADY_EXISTS{return;}
    InitCommonControlsEx(&INITCOMMONCONTROLSEX{dwSize:mem::size_of::<INITCOMMONCONTROLSEX>() as u32,dwICC:ICC_BAR_CLASSES});
    let scale=(windows_sys::Win32::UI::HiDpi::GetDpiForWindow(owner).clamp(72,288) as f32)/96.0;
    let px=|n:i32|(n as f32*scale).round() as i32;
    let (width,height)=(px(286),px(156));
    let mut monitor:MONITORINFO=mem::zeroed();
    monitor.cbSize=mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(MonitorFromPoint(point,MONITOR_DEFAULTTONEAREST),&mut monitor)==0{return;}
    let work=monitor.rcWork;
    let x=point.x.clamp(work.left,(work.right-width).max(work.left));
    let y=(point.y-height).clamp(work.top,(work.bottom-height).max(work.top));
    let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_TOPMOST,class.as_ptr(),wide("Âm lượng hệ thống").as_ptr(),
        WS_POPUP|WS_CAPTION|WS_SYSMENU,x,y,width,height,owner,ptr::null_mut(),instance,ptr::null());
    if hwnd.is_null(){return;}
    let control=|name:&str,text:&str,id:usize,flags:u32,x,y,w,h|{
        let child=CreateWindowExW(0,wide(name).as_ptr(),wide(text).as_ptr(),WS_CHILD|WS_VISIBLE|flags,
            px(x),px(y),px(w),px(h),hwnd,id as HMENU,instance,ptr::null());
        if !child.is_null(){
            SendMessageW(child,WM_SETFONT,GetStockObject(DEFAULT_GUI_FONT) as usize,0);
        }
        child
    };
    control("STATIC",&format!("{level}%"),LABEL,0,14,8,248,22);
    let slider=control("msctls_trackbar32","",TRACK,WS_TABSTOP|TBS_NOTICKS,10,30,250,36);
    if slider.is_null(){DestroyWindow(hwnd);return;}
    SendMessageW(slider,TBM_SETRANGEMAX,1,100);
    SendMessageW(slider,TBM_SETPOS,1,level as isize);
    control("BUTTON",if muted{"Bật tiếng"}else{"Tắt tiếng"},MUTE,WS_TABSTOP,14,75,96,29);
    control("BUTTON","Thiết bị đầu ra…",OUTPUT,WS_TABSTOP,116,75,136,29);
    TRACK_LAST.with(|s|s.set(level));AUDIO_PENDING.with(|s|s.set(false));TRACK_DRAGGING.with(|s|s.set(false));
    WANTED_LEVEL.with(|s|s.set(None));AUDIO_WINDOW.with(|s|s.set(hwnd));
    ShowWindow(hwnd,SW_SHOW);SetForegroundWindow(hwnd);SetFocus(slider);
}

pub(super) unsafe fn close_panels_for(owner:HWND){
    for hwnd in [AUDIO_WINDOW.with(Cell::get),CALENDAR_WINDOW.with(Cell::get)] {
        if !hwnd.is_null()&&GetWindow(hwnd,GW_OWNER)==owner{DestroyWindow(hwnd);}
    }
}

pub(super) unsafe fn route_panel_keyboard(message:&MSG)->bool{
    if !(WM_KEYFIRST..=WM_KEYLAST).contains(&message.message){return false;}
    let panel=AUDIO_WINDOW.with(Cell::get);
    if panel.is_null()||(message.hwnd!=panel&&IsChild(panel,message.hwnd)==0){return false;}
    IsDialogMessageW(panel,message)!=0
}

unsafe fn submit_audio(hwnd:HWND,action:Action)->bool{
    if AUDIO_PENDING.with(Cell::get)||!super::apply_audio(action){return false;}
    AUDIO_PENDING.with(|s|s.set(true));
    EnableWindow(GetDlgItem(hwnd,MUTE as i32),0);
    true
}
unsafe fn flush_volume(hwnd:HWND){
    if AUDIO_PENDING.with(Cell::get){return;}
    if let Some(value)=WANTED_LEVEL.with(Cell::get){
        if value==TRACK_LAST.with(Cell::get)||submit_audio(hwnd,Action::SetVolume(value)){
            WANTED_LEVEL.with(|s|s.set(None));
        }
    }
}
pub(super) unsafe fn reflect_audio(result:Option<Audio>){
    let hwnd=AUDIO_WINDOW.with(Cell::get);
    if hwnd.is_null()||!AUDIO_PENDING.with(|s|s.replace(false)){return;}
    EnableWindow(GetDlgItem(hwnd,MUTE as i32),1);
    let label=GetDlgItem(hwnd,LABEL as i32);
    if let Some(audio)=result{
        TRACK_LAST.with(|s|s.set(audio.level));
        SetWindowTextW(GetDlgItem(hwnd,MUTE as i32),wide(if audio.muted{"Bật tiếng"}else{"Tắt tiếng"}).as_ptr());
        if !TRACK_DRAGGING.with(Cell::get)&&WANTED_LEVEL.with(Cell::get).is_none(){
            SendMessageW(GetDlgItem(hwnd,TRACK as i32),TBM_SETPOS,1,audio.level as isize);
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
                submit_audio(hwnd,Action::Mute);
                0
            }
            OUTPUT=>{open("ms-settings:sound");DestroyWindow(hwnd);0},
            _=>DefWindowProcW(hwnd,msg,w,l),
        }
        WM_ACTIVATE if w&0xffff==WA_INACTIVE as usize=>{PostMessageW(hwnd,WM_CLOSE,0,0);0},
        WM_NCDESTROY=>{
            if AUDIO_WINDOW.with(Cell::get)==hwnd{AUDIO_WINDOW.with(|s|s.set(ptr::null_mut()));}
            DefWindowProcW(hwnd,msg,w,l)
        }
        _=>DefWindowProcW(hwnd,msg,w,l),
    }
}

pub(super) unsafe fn calendar_panel(owner:HWND,point:POINT,local:SYSTEMTIME){
    close_panels();
    let instance=GetModuleHandleW(ptr::null());
    let class=wide(CALENDAR_CLASS);
    let mut wc:WNDCLASSW=mem::zeroed();
    wc.hInstance=instance;wc.lpszClassName=class.as_ptr();wc.lpfnWndProc=Some(calendar_proc);
    wc.hbrBackground=(COLOR_WINDOW as usize+1) as _;
    wc.hCursor=LoadCursorW(ptr::null_mut(),IDC_ARROW);
    if RegisterClassW(&wc)==0&&GetLastError()!=ERROR_CLASS_ALREADY_EXISTS{return;}
    if InitCommonControlsEx(&INITCOMMONCONTROLSEX{dwSize:mem::size_of::<INITCOMMONCONTROLSEX>() as u32,dwICC:ICC_DATE_CLASSES})==0{return;}
    let window_style=WS_POPUP|WS_CAPTION|WS_SYSMENU;
    let ex_style=WS_EX_TOOLWINDOW|WS_EX_TOPMOST;
    let caption=wide(&format!("Lịch  ·  {:02}:{:02}",local.wHour,local.wMinute));
    let hwnd=CreateWindowExW(ex_style,class.as_ptr(),caption.as_ptr(),window_style,point.x,point.y,0,0,
        owner,ptr::null_mut(),instance,ptr::null());
    if hwnd.is_null(){return;}
    let cal=CreateWindowExW(0,wide("SysMonthCal32").as_ptr(),ptr::null(),WS_CHILD|WS_VISIBLE|WS_TABSTOP,
        0,0,0,0,hwnd,1usize as HMENU,instance,ptr::null());
    let mut inner:RECT=mem::zeroed();
    if cal.is_null()||SendMessageW(cal,MCM_GETMINREQRECT,0,(&mut inner as *mut RECT) as LPARAM)==0{DestroyWindow(hwnd);return;}
    let dpi=windows_sys::Win32::UI::HiDpi::GetDpiForWindow(owner).clamp(72,288);
    let pad=(9*dpi as i32+48)/96;
    let (cw,ch)=(inner.right-inner.left,inner.bottom-inner.top);
    SetWindowPos(cal,ptr::null_mut(),pad,pad,cw,ch,SWP_NOACTIVATE|SWP_NOZORDER);
    let mut frame=RECT{left:0,top:0,right:cw+pad*2,bottom:ch+pad*2};
    windows_sys::Win32::UI::HiDpi::AdjustWindowRectExForDpi(&mut frame,window_style,0,ex_style,dpi);
    let mut mon:MONITORINFO=mem::zeroed();
    mon.cbSize=mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(MonitorFromPoint(point,MONITOR_DEFAULTTONEAREST),&mut mon)==0{DestroyWindow(hwnd);return;}
    let work=mon.rcWork;
    let (width,height)=(frame.right-frame.left,frame.bottom-frame.top);
    let x=point.x.clamp(work.left,(work.right-width).max(work.left));
    let y=(point.y-height).clamp(work.top,(work.bottom-height).max(work.top));
    CALENDAR_WINDOW.with(|s|s.set(hwnd));
    SetWindowPos(hwnd,HWND_TOPMOST,x,y,width,height,SWP_SHOWWINDOW);
    SetForegroundWindow(hwnd);SetFocus(cal);
}
unsafe extern "system" fn calendar_proc(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
    match msg{
        WM_ACTIVATE if w&0xffff==WA_INACTIVE as usize=>{PostMessageW(hwnd,WM_CLOSE,0,0);0},
        WM_NCDESTROY=>{
            if CALENDAR_WINDOW.with(Cell::get)==hwnd{CALENDAR_WINDOW.with(|s|s.set(ptr::null_mut()));}
            DefWindowProcW(hwnd,msg,w,l)
        },
        _=>DefWindowProcW(hwnd,msg,w,l)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    #[ignore = "Windows calendar control and owned panel cleanup; explicitly run in CI"]
    fn native_calendar_uses_a_real_control_and_closes_with_utilities(){
        unsafe{
            let parent=CreateWindowExW(0,wide("STATIC").as_ptr(),ptr::null(),WS_POPUP,0,0,1,1,
                ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),ptr::null());
            assert!(!parent.is_null());
            let mut local=mem::zeroed();GetLocalTime(&mut local);
            calendar_panel(parent,POINT{x:120,y:250},local);
            let panel=CALENDAR_WINDOW.with(Cell::get);
            assert!(!panel.is_null());
            assert_eq!(GetWindow(panel,GW_OWNER),parent);
            let month=GetDlgItem(panel,1);
            assert!(!month.is_null());
            let mut size:RECT=mem::zeroed();
            assert_ne!(SendMessageW(month,MCM_GETMINREQRECT,0,(&mut size as *mut RECT) as LPARAM),0);
            assert!(size.right>size.left&&size.bottom>size.top);
            close_panels();
            assert_eq!(IsWindow(panel),0);
            assert!(CALENDAR_WINDOW.with(Cell::get).is_null());
            DestroyWindow(parent);
        }
    }
    #[test]
    #[ignore = "Windows on-demand hardware queries; explicitly run in CI"]
    fn native_on_demand_provider_completes_without_an_assumed_device(){
        unsafe {
            let hwnd=CreateWindowExW(0,wide("STATIC").as_ptr(),ptr::null(),WS_POPUP,0,0,1,1,
                ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),ptr::null());
            assert!(!hwnd.is_null());
            let provider=Provider::new();
            assert!(provider.read(hwnd));
            let deadline=std::time::Instant::now()+std::time::Duration::from_secs(5);
            let snapshot=loop{
                if let Some(s)=provider.take(){break Some(s);}
                if std::time::Instant::now()>=deadline{break None;}
                std::thread::sleep(std::time::Duration::from_millis(25));
            };
            DestroyWindow(hwnd);
            let report=snapshot.expect("read-only probe must complete in a bounded time");
            assert!((1..=12).contains(&report.local.wMonth));
            if let Some(audio)=report.audio{assert!(audio.level<=100);}
            if let BatteryState::Present{level:Some(level),..}=report.battery{assert!(level<=100);}
        }
    }
}
