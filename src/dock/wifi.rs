//! Native WLAN queries are bounded and only run from the utility worker.
use std::{mem,ptr};
use windows_sys::{core::GUID,Win32::{Foundation::*,NetworkManagement::WiFi::*}};

struct Client(HANDLE);
impl Client {
    unsafe fn open()->Result<Self,u32>{
        let (mut version,mut handle)=(0,ptr::null_mut());
        let status=WlanOpenHandle(2,ptr::null(),&mut version,&mut handle);
        if status!=0{return Err(status);}
        if handle.is_null(){return Err(ERROR_INVALID_HANDLE);}
        Ok(Self(handle))
    }
}
impl Drop for Client{fn drop(&mut self){unsafe{WlanCloseHandle(self.0,ptr::null());}}}
struct Buffer<T>(*mut T);
impl<T> Drop for Buffer<T>{
    fn drop(&mut self){if !self.0.is_null(){unsafe{WlanFreeMemory(self.0.cast());}}}
}

#[derive(Clone)]
pub(super) enum Change {
    Connect{interface:GUID,profile:Vec<u16>,kind:DOT11_BSS_TYPE},
    Open{interface:GUID,ssid:Vec<u8>,kind:DOT11_BSS_TYPE},
    Disconnect{interface:GUID,ssid:Vec<u8>,kind:DOT11_BSS_TYPE},
}
pub(super) struct Network {
    pub name:String,
    pub signal:u32,
    pub connected:bool,
    pub protected:bool,
    pub change:Option<Change>,
}
pub(super) struct View {
    pub networks:Vec<Network>,
    pub status:Option<u32>,
    pub no_adapter:bool,
    pub outcome:Option<Result<(),u32>>,
}
fn ssid_bytes(ssid:&DOT11_SSID)->&[u8]{&ssid.ucSSID[..(ssid.uSSIDLength as usize).min(ssid.ucSSID.len())]}
fn name(bytes:&[u8])->String{
    let name=String::from_utf8_lossy(bytes).chars()
        .map(|c|if c.is_control()||matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'){'�'}else{c})
        .collect::<String>();
    if name.trim().is_empty(){"Mạng ẩn".into()}else{name}
}
fn candidate(interface:GUID,n:&WLAN_AVAILABLE_NETWORK,read_only:bool)->Network{
    let ssid=ssid_bytes(&n.dot11Ssid);
    let connected=n.dwFlags&WLAN_AVAILABLE_NETWORK_CONNECTED!=0;
    let change=if read_only{None}
    else if connected{
        Some(Change::Disconnect{interface,ssid:ssid.to_vec(),kind:n.dot11BssType})
    }else if n.bNetworkConnectable!=0&&n.dwFlags&WLAN_AVAILABLE_NETWORK_HAS_PROFILE!=0{
        n.strProfileName.iter().position(|&c|c==0).filter(|&length|length>0)
            .map(|length|Change::Connect{interface,profile:n.strProfileName[..=length].to_vec(),kind:n.dot11BssType})
    }else if n.bNetworkConnectable!=0&&open_network(n)&&n.dot11BssType==dot11_BSS_type_infrastructure{
        Some(Change::Open{interface,ssid:ssid.to_vec(),kind:n.dot11BssType})
    }else{None};
    Network{name:name(ssid),signal:n.wlanSignalQuality.min(100),connected,protected:n.bSecurityEnabled!=0,change}
}
fn open_network(n:&WLAN_AVAILABLE_NETWORK)->bool{
    n.dot11Ssid.uSSIDLength>0&&n.dot11Ssid.uSSIDLength<=32&&n.bSecurityEnabled==0
        &&n.dot11DefaultAuthAlgorithm==DOT11_AUTH_ALGO_80211_OPEN&&n.dot11DefaultCipherAlgorithm==DOT11_CIPHER_ALGO_NONE
}
fn open_profile(ssid:&[u8])->Option<Vec<u16>>{
    if ssid.is_empty()||ssid.len()>32{return None;}
    let hex=ssid.iter().map(|byte|format!("{byte:02X}")).collect::<String>();
    // Original wifi.h::profiloAperto: only hexadecimal SSID bytes enter XML.
    // Temporary manual profile, without credentials or persistent WLAN writes.
    Some(crate::desktop::wide(&format!("<?xml version=\"1.0\"?><WLANProfile xmlns=\"http://www.microsoft.com/networking/WLAN/profile/v1\"><name>TLauncher-{hex}</name><SSIDConfig><SSID><hex>{hex}</hex></SSID></SSIDConfig><connectionType>ESS</connectionType><connectionMode>manual</connectionMode><MSM><security><authEncryption><authentication>open</authentication><encryption>none</encryption><useOneX>false</useOneX></authEncryption></security></MSM></WLANProfile>")))
}
unsafe fn apply(client:&Client,change:&Change)->Result<(),u32>{
    // Recheck here as well as in the UI; an isolated build must never submit a WLAN write.
    if crate::trial::active(){return Err(ERROR_ACCESS_DENIED);}
    let status=match change{
        Change::Connect{interface,profile,kind}=>{
            let request=WLAN_CONNECTION_PARAMETERS{wlanConnectionMode:wlan_connection_mode_profile,
                strProfile:profile.as_ptr(),pDot11Ssid:ptr::null_mut(),pDesiredBssidList:ptr::null_mut(),dot11BssType:*kind,dwFlags:0};
            WlanConnect(client.0,interface,&request,ptr::null())
        }
        Change::Open{interface,ssid,kind}=>{
            let Some(profile)=open_profile(ssid)else{return Err(ERROR_INVALID_DATA);};
            // Re-enumerate the clicked interface; do not downgrade a saved or
            // newly secured network to a temporary open profile with its name.
            let mut raw=ptr::null_mut();let status=WlanGetAvailableNetworkList(client.0,interface,0,ptr::null(),&mut raw);let list=Buffer(raw);
            if status!=0{return Err(status);}if raw.is_null(){return Err(ERROR_INVALID_DATA);}
            let mut found=false;for i in 0..(*list.0).dwNumberOfItems.min(256) as usize{
                let n=&*ptr::addr_of!((*list.0).Network).cast::<WLAN_AVAILABLE_NETWORK>().add(i);
                if ssid_bytes(&n.dot11Ssid)==ssid.as_slice()&&n.dot11BssType==*kind{
                    if !open_network(n)||n.dwFlags&WLAN_AVAILABLE_NETWORK_HAS_PROFILE!=0{return Err(ERROR_RETRY);}found|=n.bNetworkConnectable!=0;
                }
            }if !found{return Err(ERROR_NOT_FOUND);}
            let mut native:DOT11_SSID=mem::zeroed();native.uSSIDLength=ssid.len() as u32;native.ucSSID[..ssid.len()].copy_from_slice(ssid);
            let request=WLAN_CONNECTION_PARAMETERS{wlanConnectionMode:wlan_connection_mode_temporary_profile,strProfile:profile.as_ptr(),pDot11Ssid:&mut native,
                pDesiredBssidList:ptr::null_mut(),dot11BssType:*kind,dwFlags:0};WlanConnect(client.0,interface,&request,ptr::null())
        }
        Change::Disconnect{interface,ssid,kind}=>{
            let (mut size,mut raw)=(0,ptr::null_mut());
            let status=WlanQueryInterface(client.0,interface,wlan_intf_opcode_current_connection,ptr::null(),
                &mut size,&mut raw,ptr::null_mut());
            let data=Buffer(raw);
            if status!=0{return Err(status);}
            if raw.is_null()||(size as usize)<mem::size_of::<WLAN_CONNECTION_ATTRIBUTES>(){return Err(ERROR_INVALID_DATA);}
            let current=&*data.0.cast::<WLAN_CONNECTION_ATTRIBUTES>();
            let current=&current.wlanAssociationAttributes;
            if ssid_bytes(&current.dot11Ssid)!=ssid.as_slice()||current.dot11BssType!=*kind{return Err(ERROR_RETRY);}
            WlanDisconnect(client.0,interface,ptr::null())
        }
    };
    if status==0{Ok(())}else{Err(status)}
}
pub(super) unsafe fn read(change:Option<Change>)->View{
    let mut view=View{networks:Vec::new(),status:None,no_adapter:false,outcome:None};
    let client=match Client::open(){Ok(client)=>client,Err(status)=>{view.status=Some(status);return view;}};
    view.outcome=change.as_ref().map(|action|apply(&client,action));
    let mut raw=ptr::null_mut();
    let status=WlanEnumInterfaces(client.0,ptr::null(),&mut raw);
    let interfaces=Buffer(raw);
    if status!=0||raw.is_null(){view.status=Some(if status!=0{status}else{ERROR_INVALID_DATA});return view;}
    let count=(*raw).dwNumberOfItems.min(8) as usize;
    view.no_adapter=count==0;
    let mut failed=None;
    let mut succeeded=false;
    for i in 0..count {
        let interface=&*ptr::addr_of!((*interfaces.0).InterfaceInfo).cast::<WLAN_INTERFACE_INFO>().add(i);
        let mut raw=ptr::null_mut();
        let status=WlanGetAvailableNetworkList(client.0,&interface.InterfaceGuid,0,ptr::null(),&mut raw);
        let list=Buffer(raw);
        if status!=0||raw.is_null(){
            failed=Some(if status!=0{status}else{ERROR_INVALID_DATA});
            continue;
        }
        succeeded=true;
        for j in 0..(*raw).dwNumberOfItems.min(128) as usize{
            let n=&*ptr::addr_of!((*list.0).Network).cast::<WLAN_AVAILABLE_NETWORK>().add(j);
            let mut item=candidate(interface.InterfaceGuid,n,crate::trial::active());
            if crate::trial::demo_active(){item.name=format!("Mạng đọc từ Windows {}",j+1);}
            view.networks.push(item);
        }
    }
    if !succeeded{view.status=failed;}
    view.networks.sort_by_key(|n|(!n.connected,std::cmp::Reverse(n.signal)));
    view.networks.truncate(16);
    view
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn only_saved_connectable_profiles_get_a_connect_action(){
        let mut row:WLAN_AVAILABLE_NETWORK=unsafe{mem::zeroed()};
        let interface:GUID=unsafe{mem::zeroed()};
        row.dot11Ssid.uSSIDLength=4;
        row.dot11Ssid.ucSSID[..4].copy_from_slice(b"a\x00bc");
        row.wlanSignalQuality=120;
        row.bNetworkConnectable=1;
        assert_eq!(candidate(interface,&row,false).name,"a�bc");
        assert_eq!(candidate(interface,&row,false).signal,100);
        assert!(candidate(interface,&row,false).change.is_none());
        row.dwFlags=WLAN_AVAILABLE_NETWORK_HAS_PROFILE;
        row.strProfileName[0]=b'a' as u16;
        assert!(matches!(candidate(interface,&row,false).change,Some(Change::Connect{..})));
        assert!(candidate(interface,&row,true).change.is_none());
        row.dwFlags|=WLAN_AVAILABLE_NETWORK_CONNECTED;
        assert!(matches!(candidate(interface,&row,false).change,Some(Change::Disconnect{..})));
    }
    #[test]fn temporary_open_profile_preserves_bytes_and_never_downgrades_security(){
        let raw=b"<&\"\0\xff";let profile=open_profile(raw).unwrap();let xml=String::from_utf16_lossy(&profile[..profile.len()-1]);
        assert!(xml.contains("<hex>3C262200FF</hex>"));assert!(xml.contains("<connectionMode>manual</connectionMode>"));assert!(!xml.contains("<keyMaterial>"));assert!(open_profile(&[]).is_none());assert!(open_profile(&[0;33]).is_none());
        let mut row:WLAN_AVAILABLE_NETWORK=unsafe{mem::zeroed()};let interface:GUID=unsafe{mem::zeroed()};
        row.bNetworkConnectable=1;row.dot11Ssid.uSSIDLength=3;row.dot11Ssid.ucSSID[..3].copy_from_slice(b"abc");row.dot11BssType=dot11_BSS_type_infrastructure;
        row.dot11DefaultAuthAlgorithm=DOT11_AUTH_ALGO_80211_OPEN;row.dot11DefaultCipherAlgorithm=DOT11_CIPHER_ALGO_NONE;
        assert!(matches!(candidate(interface,&row,false).change,Some(Change::Open{..})));assert!(candidate(interface,&row,true).change.is_none());
        row.bSecurityEnabled=1;assert!(candidate(interface,&row,false).change.is_none());row.bSecurityEnabled=0;row.dot11DefaultCipherAlgorithm=DOT11_CIPHER_ALGO_WEP;assert!(candidate(interface,&row,false).change.is_none());
    }
}
