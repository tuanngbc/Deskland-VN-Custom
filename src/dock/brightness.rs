//! sistema.h DDC/CI brightness, scoped to the display chosen at the click.
use std::{mem,ptr};
use windows_sys::Win32::{Foundation::*,Devices::Display::*,Graphics::Gdi::*};
#[derive(Clone)]pub(super) struct Target{monitor:usize,device:[u16;32],interface:[u16;128],bounds:(i32,i32,i32,i32)}
#[derive(Clone)]pub(super) struct Query{pub target:Target,pub change:Option<Change>}
#[derive(Clone)]pub(super) struct Change{description:[u16;128],minimum:u32,maximum:u32,observed:u32,value:u32}
pub(super) struct View{pub query:Query,pub name:String,pub percent:Option<u32>,pub error:Option<u32>,pub outcome:Option<Result<(),u32>>,change:Option<Change>}
struct Physical(Vec<PHYSICAL_MONITOR>);
impl Drop for Physical{fn drop(&mut self){if !self.0.is_empty(){unsafe{DestroyPhysicalMonitors(self.0.len() as u32,self.0.as_ptr());}}}}
fn bounds(r:RECT)->(i32,i32,i32,i32){(r.left,r.top,r.right,r.bottom)}
pub(super) unsafe fn target(monitor:HMONITOR)->Option<Target>{
    let mut info:MONITORINFOEXW=mem::zeroed();info.monitorInfo.cbSize=mem::size_of::<MONITORINFOEXW>() as u32;
    if monitor.is_null()||GetMonitorInfoW(monitor,(&mut info as *mut MONITORINFOEXW).cast())==0{return None;}
    let mut device:DISPLAY_DEVICEW=mem::zeroed();device.cb=mem::size_of::<DISPLAY_DEVICEW>() as u32;
    if EnumDisplayDevicesW(info.szDevice.as_ptr(),0,&mut device,EDD_GET_DEVICE_INTERFACE_NAME)==0||device.DeviceID[0]==0{return None;}
    Some(Target{monitor:monitor as usize,device:info.szDevice,interface:device.DeviceID,bounds:bounds(info.monitorInfo.rcMonitor)})
}
impl Target{unsafe fn matches(&self)->bool{target(self.monitor as HMONITOR).is_some_and(|now|now.device==self.device&&now.interface==self.interface&&now.bounds==self.bounds)}}
fn percent(minimum:u32,current:u32,maximum:u32)->Option<u32>{
    (maximum>minimum&&current>=minimum&&current<=maximum).then(||((u64::from(current-minimum)*100)/u64::from(maximum-minimum)) as u32)
}
fn value(minimum:u32,maximum:u32,percent:u32)->Option<u32>{
    (maximum>minimum&&percent<=100).then(||minimum+((u64::from(maximum-minimum)*u64::from(percent)+50)/100) as u32)
}
fn apply_checked(read_only:bool,identity:bool,requested:&Change,current:(u32,u32,u32),mut set:impl FnMut(u32)->Result<(),u32>,read:impl FnOnce()->Result<(u32,u32,u32),u32>)->Result<(),u32>{
    if read_only{return Err(ERROR_ACCESS_DENIED);}if !identity{return Err(ERROR_RETRY);}
    if current!=(requested.minimum,requested.observed,requested.maximum){return Err(ERROR_RETRY);}
    let Some(target)=value(current.0,current.2,requested.value)else{return Err(ERROR_INVALID_DATA);};set(target)?;
    let actual=read()?;if actual.0==current.0&&actual.2==current.2&&actual.1.abs_diff(target)<=1{Ok(())}else{Err(ERROR_RETRY)}
}
pub(super) fn read(query:Query)->View{unsafe{
    let mut view=View{query:Query{target:query.target.clone(),change:None},name:String::new(),percent:None,error:None,outcome:None,change:None};
    let result=(||->Result<(),u32>{
        if !query.target.matches(){return Err(ERROR_DEVICE_NOT_CONNECTED);}
        let mut count=0;if GetNumberOfPhysicalMonitorsFromHMONITOR(query.target.monitor as HMONITOR,&mut count)==0{return Err(GetLastError());}
        // A logical display can cover multiple physical targets. Refuse an
        // ambiguous composite rather than silently selecting physical[0].
        if count!=1{return Err(ERROR_NOT_SUPPORTED);}
        let mut physical=Physical(vec![mem::zeroed()]);
        if GetPhysicalMonitorsFromHMONITOR(query.target.monitor as HMONITOR,1,physical.0.as_mut_ptr())==0{physical.0.clear();return Err(GetLastError());}
        let monitor=physical.0[0].hPhysicalMonitor;let description=physical.0[0].szPhysicalMonitorDescription;
        let length=description.iter().position(|c|*c==0).unwrap_or(description.len());view.name=String::from_utf16_lossy(&description[..length]).chars().filter(|c|!c.is_control()).collect();
        let (mut caps,mut temperatures)=(0,0);if GetMonitorCapabilities(monitor,&mut caps,&mut temperatures)==0{return Err(GetLastError());}
        if caps&MC_CAPS_BRIGHTNESS==0{return Err(ERROR_NOT_SUPPORTED);}
        let get=||{let (mut minimum,mut current,mut maximum)=(0,0,0);
            if GetMonitorBrightness(monitor,&mut minimum,&mut current,&mut maximum)==0{Err(GetLastError())}else{Ok((minimum,current,maximum))}};
        let current=get()?;if percent(current.0,current.1,current.2).is_none(){return Err(ERROR_INVALID_DATA);}
        if let Some(request)=query.change{
            view.outcome=Some(apply_checked(crate::trial::active()||cfg!(test),query.target.matches()&&request.description==description,&request,current,
                |new|if SetMonitorBrightness(monitor,new)!=0{Ok(())}else{Err(GetLastError())},get));
        }
        let current=get()?;view.percent=percent(current.0,current.1,current.2);
        if !crate::trial::active()&&!cfg!(test)&&view.percent.is_some(){view.change=Some(Change{description,minimum:current.0,observed:current.1,maximum:current.2,value:0});}Ok(())
    })();if let Err(error)=result{view.error=Some(error);}view
}}
impl View{pub(super) fn step(&self,delta:i32)->Option<Query>{let mut change=self.change.clone()?;change.value=(self.percent? as i32+delta).clamp(0,100) as u32;Some(Query{target:self.query.target.clone(),change:Some(change)})}}
pub(super) fn cards(view:&View)->super::control_center::View{
    use super::control_center::{Card,Command,View as Panel};let text=crate::launcher::localized;
    let mut hint=match view.percent{Some(value)=>format!("{} · {value}%",view.name),None=>text("Màn hình này không cung cấp điều khiển độ sáng tương thích","This display does not provide compatible brightness control","此显示器不提供兼容的亮度控制").into()};
    if let Some(Err(error))=view.outcome{hint=format!("{} ({error})",text("Chưa xác nhận thay đổi. Hãy làm mới.","Change not confirmed. Refresh to check.","尚未确认更改，请刷新检查。"));}
    let mut cards=Vec::new();for (delta,title) in [(-10,text("Giảm 10%","Decrease 10%","降低 10%")),(10,text("Tăng 10%","Increase 10%","提高 10%"))]{
        if view.percent.is_some(){cards.push(Card{icon:None,title:title.into(),hint:String::new(),action:view.step(delta).map(|q|Command::Brightness(Some(q)))});}
    }
    cards.push(Card{icon:None,title:text("Làm mới","Refresh","刷新").into(),hint:String::new(),action:Some(Command::Brightness(Some(view.query.clone())))});
    cards.push(Card{icon:None,title:text("Cài đặt màn hình Windows","Windows display settings","Windows 显示设置").into(),hint:String::new(),action:Some(Command::DisplaySettings)});
    Panel{title:text("Độ sáng màn hình","Display brightness","显示器亮度").into(),hint,cards,metrics:None}
}
#[cfg(test)]mod tests{
    use super::*;use std::cell::Cell;
    #[test]fn brightness_identity_range_stale_state_and_confirmation(){
        assert_eq!(percent(20,60,100),Some(50));assert_eq!(value(20,100,50),Some(60));assert_eq!(value(0,u32::MAX,100),Some(u32::MAX));
        assert_eq!(percent(20,19,100),None);assert_eq!(value(100,100,50),None);assert_eq!(value(0,100,101),None);
        let change=Change{description:[0;128],minimum:20,maximum:100,observed:60,value:75};let writes=Cell::new(0);let set=|new|{writes.set(writes.get()+1);assert_eq!(new,80);Ok(())};
        assert_eq!(apply_checked(true,true,&change,(20,60,100),set,||Ok((20,80,100))),Err(ERROR_ACCESS_DENIED));
        assert_eq!(apply_checked(false,false,&change,(20,60,100),set,||Ok((20,80,100))),Err(ERROR_RETRY));
        assert_eq!(apply_checked(false,true,&change,(20,61,100),set,||Ok((20,80,100))),Err(ERROR_RETRY));assert_eq!(writes.get(),0);
        assert_eq!(apply_checked(false,true,&change,(20,60,100),set,||Ok((20,80,100))),Ok(()));assert_eq!(writes.get(),1);
        assert_eq!(apply_checked(false,true,&change,(20,60,100),set,||Ok((20,60,100))),Err(ERROR_RETRY));
        assert_eq!(apply_checked(false,true,&change,(20,60,100),|_|Err(ERROR_DEVICE_NOT_CONNECTED),||Ok((20,80,100))),Err(ERROR_DEVICE_NOT_CONNECTED));
    }
    #[test]#[ignore="hosted DDC read/physical handle cleanup; no brightness writes or assumed support"]
    fn native_brightness_read_releases_physical_handles(){
        if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
        for _ in 0..3{unsafe{if let Some(target)=target(MonitorFromPoint(POINT{x:0,y:0},MONITOR_DEFAULTTOPRIMARY)){
            let view=read(Query{target,change:None});assert!(view.change.is_none());println!("FLUIDWIN_BRIGHTNESS_READ percent={:?} status={:?} writes=NONE",view.percent,view.error);
        }}}
    }
}
