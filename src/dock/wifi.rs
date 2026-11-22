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
    }else{None};
    Network{name:name(ssid),signal:n.wlanSignalQuality.min(100),connected,protected:n.bSecurityEnabled!=0,change}
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
}

