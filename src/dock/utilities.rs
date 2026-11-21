//! One bounded, on-demand provider for all Dock displays. No probes while
//! idle, no WLAN or Bluetooth scans, and no Shell/COM calls on the UI thread.
use super::{WM_UTILITY_READY,open};
use crate::desktop::wide;
use std::{mem,ptr,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}}};
use windows::Win32::Media::Audio::{*,Endpoints::IAudioEndpointVolume};
use windows::Win32::System::Com::*;
use windows_sys::Win32::{
    Devices::Bluetooth::*,
    Foundation::*,
    NetworkManagement::{IpHelper::*,Ndis::IfOperStatusUp,WiFi::*},
    System::{Power::*,SystemInformation::GetLocalTime,Threading::{TrySubmitThreadpoolCallback,PTP_CALLBACK_INSTANCE}},
    UI::{Input::KeyboardAndMouse::*,WindowsAndMessaging::*},
};

#[derive(Clone, Copy, Debug)]
enum Link { None, Wired, WiFi(Option<u8>), Both(Option<u8>) }
enum Bluetooth { Unknown, Adapter, Connected }
#[derive(Clone,Copy)]
enum Battery { Unknown, Absent, Present{level:Option<u8>,ac:Option<bool>,charging:bool} }
struct Audio { level:u32, muted:bool }
pub(super) struct Snapshot { network:Option<Link>, bluetooth:Bluetooth, battery:Battery, audio:Option<Audio>, local:SYSTEMTIME }

#[derive(Clone,Copy)]
enum JobKind { Read, Volume(u32), Mute }
#[derive(Default)]
struct State { pending:AtomicBool, result:Mutex<Option<Snapshot>>, closed:AtomicBool }
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
            let snapshot=Snapshot{network:network(),bluetooth:bluetooth(),battery:battery(),audio,local};
            if let Ok(mut result)=state.result.lock(){*result=Some(snapshot);}
        }
    }
    drop(job.permit);
    state.pending.store(false,Ordering::Release);
    if matches!(job.kind,JobKind::Read)&&!state.closed.load(Ordering::Acquire){
        PostMessageW(job.owner as HWND,WM_UTILITY_READY,0,0);
    }
}
unsafe fn audio(kind:JobKind)->Option<Audio>{
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
unsafe fn bluetooth()->Bluetooth{
    let mut radio=ptr::null_mut();
    let params=BLUETOOTH_FIND_RADIO_PARAMS{dwSize:mem::size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32};
    let find=BluetoothFindFirstRadio(&params,&mut radio);
    if find.is_null(){return Bluetooth::Unknown;}
    let mut state=Bluetooth::Adapter;
    for index in 0..16 {
        if radio.is_null(){break;}
        let mut params:BLUETOOTH_DEVICE_SEARCH_PARAMS=mem::zeroed();
        params.dwSize=mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32;
        params.fReturnConnected=1;params.hRadio=radio;
        let mut info:BLUETOOTH_DEVICE_INFO=mem::zeroed();
        info.dwSize=mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
        let device=BluetoothFindFirstDevice(&params,&mut info);
        if !device.is_null(){
            if info.fConnected!=0{state=Bluetooth::Connected;}
            BluetoothFindDeviceClose(device);
        }
        CloseHandle(radio);
        if matches!(state,Bluetooth::Connected)||index==15||BluetoothFindNextRadio(find,&mut radio)==0{break;}
    }
    BluetoothFindRadioClose(find);
    state
}
unsafe fn battery()->Battery{
    let mut s:SYSTEM_POWER_STATUS=mem::zeroed();
    if GetSystemPowerStatus(&mut s)==0||s.BatteryFlag==255{return Battery::Unknown;}
    if s.BatteryFlag&128!=0{return Battery::Absent;}
    Battery::Present {level:(s.BatteryLifePercent<=100).then_some(s.BatteryLifePercent),
        ac:match s.ACLineStatus{0=>Some(false),1=>Some(true),_=>None},charging:s.BatteryFlag&8!=0}
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum HostAction { Settings, Capture, Record, SystemMonitor, Wallpaper, Stage, Launcher }
#[derive(Clone,Copy)]
pub(super) enum Action {
    Host(HostAction), Network, Bluetooth, Sound, Battery, Calendar, Notifications,
    QuickSettings, Explorer, Desktop, Windows, SetVolume(u32), Mute,
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
pub(super) unsafe fn menu(owner:HWND,point:POINT,snapshot:&Snapshot)->Option<Action>{
    let menu=CreatePopupMenu();
    if menu.is_null(){return None;}
    let row=|id,text:&str|{AppendMenuW(menu,MF_STRING,id,wide(text).as_ptr());};
    row(1,&network_label(snapshot.network));
    let sound=CreatePopupMenu();
    if !sound.is_null(){
        if let Some(a)=snapshot.audio.as_ref() {
            AppendMenuW(sound,MF_STRING,10,wide(if a.muted{"Bật tiếng"}else{"Tắt tiếng"}).as_ptr());
            for p in [0u32,10,20,30,40,50,60,70,80,90,100]{
                AppendMenuW(sound,MF_STRING|if a.level==p{MF_CHECKED}else{0},SET_LEVEL+p as usize,wide(&format!("{p}%")).as_ptr());
            }
            AppendMenuW(sound,MF_SEPARATOR,0,ptr::null());
        }
        AppendMenuW(sound,MF_STRING,2,wide("Chọn thiết bị âm thanh trong Windows…").as_ptr());
        let title=snapshot.audio.as_ref().map_or_else(||"Âm thanh · Không đọc được thiết bị".into(),|a|{
            if a.muted{format!("Âm thanh · Đang tắt ({0}%)",a.level)}else{format!("Âm thanh · {}%",a.level)}
        });
        if AppendMenuW(menu,MF_POPUP,sound as usize,wide(&title).as_ptr())==0{DestroyMenu(sound);}
    }
    row(3,match snapshot.bluetooth{
        Bluetooth::Unknown=>"Bluetooth · Mở cài đặt Windows…",
        Bluetooth::Adapter=>"Bluetooth · Có bộ điều hợp",
        Bluetooth::Connected=>"Bluetooth · Có thiết bị Classic đang kết nối",
    });
    if let Battery::Present{level,ac,charging}=snapshot.battery {
        let value=level.map_or("—".into(),|p|format!("{p}%"));
        let mode=if charging{"Đang sạc"}else{match ac{Some(true)=>"Nguồn AC",Some(false)=>"Dùng pin",None=>"Không rõ nguồn"}};
        row(4,&format!("Pin · {value} · {mode}"));
    }
    let d=&snapshot.local;
    row(5,&format!("{:02}:{:02} · {:02}/{:02}/{} · Lịch",d.wHour,d.wMinute,d.wDay,d.wMonth,d.wYear));
    row(6,"Trung tâm thông báo Windows");
    row(7,"Cài đặt nhanh Windows");
    AppendMenuW(menu,MF_SEPARATOR,0,ptr::null());
    for (id,label) in [(11,"Spotlight"),(12,"File Explorer"),(13,"Hiện/ẩn desktop"),(14,"Chụp màn hình"),
        (15,"Quay màn hình"),(16,"Bật/tắt chỉ số taskbar"),(17,"Bật/tắt hình nền động"),
        (18,"Bật/tắt Stage Manager"),(19,"Cài đặt tuanpc-desk"),(20,"Chuyển cửa sổ Windows")] {row(id,label);}
    let previous=GetForegroundWindow();
    SetForegroundWindow(owner);
    let cmd=TrackPopupMenu(menu,TPM_RETURNCMD|TPM_NONOTIFY|TPM_BOTTOMALIGN,point.x,point.y,0,owner,ptr::null()) as usize;
    DestroyMenu(menu);
    if GetForegroundWindow()==owner&&!previous.is_null()&&IsWindow(previous)!=0{SetForegroundWindow(previous);}
    use Action::*;
    Some(match cmd {
        1=>Network,2=>Sound,3=>Bluetooth,4=>Battery,5=>Calendar,6=>Notifications,7=>QuickSettings,10=>Mute,
        11=>Host(HostAction::Launcher),12=>Explorer,13=>Desktop,14=>Host(HostAction::Capture),
        15=>Host(HostAction::Record),16=>Host(HostAction::SystemMonitor),17=>Host(HostAction::Wallpaper),
        18=>Host(HostAction::Stage),19=>Host(HostAction::Settings),20=>Windows,
        SET_LEVEL..=200=>SetVolume((cmd-SET_LEVEL) as u32),
        _=>return None,
    })
}
pub(super) unsafe fn delegated(action:Action){
    use Action::*;
    match action{
        Network=>open("ms-settings:network-status"), Bluetooth=>open("ms-settings:bluetooth"),
        Sound=>open("ms-settings:sound"),Battery=>open("ms-settings:powersleep"), Explorer=>open("explorer.exe"),
        Calendar|Notifications=>shell_key(b'N' as u16),QuickSettings=>shell_key(b'A' as u16),
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

#[cfg(test)]
mod tests {
    use super::*;
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
            if let Battery::Present{level:Some(level),..}=report.battery{assert!(level<=100);}
        }
    }
}