//! sistema.h radio flow, on the existing utility worker with typed WinRT.
//! No scans, permanent worker, optimistic state, or administrator operations.
use std::{sync::{Arc,atomic::{AtomicI32,Ordering}},time::{Duration,Instant}};
use windows::{core::Result,Devices::Radios::{Radio,RadioKind,RadioState,RadioAccessStatus}};
const DENIED:i32=0x80070005u32 as i32;
const STALE:i32=0x800704d5u32 as i32;
const TIMEOUT:i32=0x800705b4u32 as i32;
const UNAVAILABLE:i32=0x80070490u32 as i32;
static ACCESS:AtomicI32=AtomicI32::new(0);
#[derive(Clone,Copy,PartialEq,Eq)]pub(super) enum Kind{Wifi,Bluetooth}
impl Kind{fn native(self)->RadioKind{match self{Self::Wifi=>RadioKind::WiFi,Self::Bluetooth=>RadioKind::Bluetooth}}}
#[derive(Clone)]pub(super) struct Request{pub kind:Kind,radio:Radio,observed:RadioState,on:bool}
#[derive(Clone)]pub(super) struct Change{request:Request,permission:Arc<dyn Fn()->std::result::Result<(),i32>+Send+Sync>}
pub(super) struct Row{pub name:String,pub on:bool,pub disabled:bool,pub request:Option<Request>}
pub(super) struct View{pub rows:Vec<Row>,pub error:Option<i32>,pub outcome:Option<std::result::Result<(),i32>>}
pub(super) fn invalidate_access(){ACCESS.store(0,Ordering::Release);}
fn wait<T>(mut poll:impl FnMut()->Result<Option<T>>,cancel:impl Fn(),limit:Duration)->std::result::Result<T,i32>{
    let start=Instant::now();loop{
        match poll(){Ok(Some(value))=>return Ok(value),Err(e)=>return Err(e.code().0),_=>{}}
        if start.elapsed()>=limit{cancel();return Err(TIMEOUT);}std::thread::sleep(Duration::from_millis(12));
    }
}
/// Initiate possible consent UI only in direct response to a Dock UI click.
/// The operation's completion is awaited by the existing utility worker.
pub(super) fn request(request:Request)->Change{
    let permission:Arc<dyn Fn()->std::result::Result<(),i32>+Send+Sync>=if crate::trial::active()||cfg!(test){Arc::new(||Err(DENIED))}
    else{match ACCESS.load(Ordering::Acquire){
        1=>Arc::new(||Ok(())),n if n!=0=>Arc::new(||Err(DENIED)),
        _=>match Radio::RequestAccessAsync(){
            Err(e)=>{let error=e.code().0;Arc::new(move||Err(error))},
            Ok(op)=>Arc::new(move||{
                let result=wait(||if op.Status()?.0==0{Ok(None)}else{op.GetResults().map(Some)},||{let _=op.Cancel();},Duration::from_secs(15))?;
                ACCESS.store(result.0,Ordering::Release);if result==RadioAccessStatus::Allowed{Ok(())}else{Err(DENIED)}
            }),
        },
    }};Change{request,permission}
}
fn apply_checked(read_only:bool,expected:i32,target:i32,mut state:impl FnMut()->std::result::Result<i32,i32>,
    permission:impl FnOnce()->std::result::Result<(),i32>,submit:impl FnOnce()->std::result::Result<(),i32>,limit:Duration)->std::result::Result<(),i32>{
    if read_only{return Err(DENIED);}if !matches!(expected,1|2)||!matches!(target,1|2){return Err(DENIED);}
    permission()?;if state()?!=expected{return Err(STALE);}submit()?;
    let start=Instant::now();loop{let actual=state()?;if actual==target{return Ok(());}if actual==3{return Err(DENIED);}
        if start.elapsed()>=limit{return Err(TIMEOUT);}std::thread::sleep(Duration::from_millis(20));}
}
fn apply(change:Change)->std::result::Result<(),i32>{
    let Request{radio,observed,on,..}=change.request;let target=if on{RadioState::On}else{RadioState::Off};
    apply_checked(crate::trial::active()||cfg!(test),observed.0,target.0,||radio.State().map(|s|s.0).map_err(|e|e.code().0),||{(change.permission)()},||{
        let op=radio.SetStateAsync(target).map_err(|e|e.code().0)?;
        let status=wait(||if op.Status()?.0==0{Ok(None)}else{op.GetResults().map(Some)},||{let _=op.Cancel();},Duration::from_secs(3))?;
        if status==RadioAccessStatus::Allowed{Ok(())}else{Err(DENIED)}
    },Duration::from_secs(2))
}
pub(super) fn read(kind:Kind,change:Option<Change>)->View{
    use windows::Win32::System::WinRT::{RoInitialize,RoUninitialize,RO_INIT_MULTITHREADED};
    let mut view=View{rows:Vec::new(),error:None,outcome:None};
    if let Err(error)=unsafe{RoInitialize(RO_INIT_MULTITHREADED)}{view.error=Some(error.code().0);return view;}
    struct Apartment;impl Drop for Apartment{fn drop(&mut self){unsafe{RoUninitialize();}}}let _apartment=Apartment;
    view.outcome=change.map(apply);
    let result=(||->std::result::Result<Vec<Row>,i32>{
        let op=Radio::GetRadiosAsync().map_err(|e|e.code().0)?;
        let list=wait(||if op.Status()?.0==0{Ok(None)}else{op.GetResults().map(Some)},||{let _=op.Cancel();},Duration::from_secs(3))?;
        let mut rows=Vec::new();for i in 0..list.Size().map_err(|e|e.code().0)?.min(32){
            let radio=list.GetAt(i).map_err(|e|e.code().0)?;if radio.Kind().map_err(|e|e.code().0)?!=kind.native(){continue;}
            let state=radio.State().map_err(|e|e.code().0)?;
            let name=radio.Name().map_err(|e|e.code().0)?.to_string().chars().filter(|c|!c.is_control()&&!matches!(*c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')).take(128).collect::<String>();
            let on=state==RadioState::On;let disabled=!matches!(state,RadioState::On|RadioState::Off);
            let request=(!disabled&&!crate::trial::active()&&!cfg!(test)).then(||Request{kind,radio,observed:state,on:!on});
            rows.push(Row{name,on,disabled,request});
        }Ok(rows)
    })();match result{Ok(rows)=>view.rows=rows,Err(error)=>view.error=Some(error)}view
}
pub(super) fn error_text(error:i32)->String{
    let (v,e,z)=match error{
        DENIED=>("Windows hoặc phần cứng không cho phép đổi trạng thái","Windows or hardware denied the change","Windows 或硬件拒绝了更改"),
        STALE=>("Trạng thái đã thay đổi. Hãy làm mới.","State changed. Refresh and try again.","状态已改变，请刷新后重试。"),
        TIMEOUT=>("Chưa xác nhận được trạng thái mới. Hãy làm mới.","New state not confirmed. Refresh to check.","尚未确认新状态，请刷新检查。"),
        UNAVAILABLE=>("Thiết bị không còn khả dụng","Device is no longer available","设备已不可用"),
        _=>("Không đọc được trạng thái thiết bị","Device state unavailable","设备状态不可用"),
    };format!("{} ({:08X})",crate::launcher::localized(v,e,z),error as u32)
}

#[cfg(test)]mod tests{
    use super::*;use std::cell::Cell;
    #[test]fn radio_write_checks_access_identity_and_effective_state(){
        let writes=Cell::new(0);let submit=||{writes.set(writes.get()+1);Ok(())};
        assert_eq!(apply_checked(true,1,2,||Ok(1),||Ok(()),submit,Duration::ZERO),Err(DENIED));
        assert_eq!(apply_checked(false,1,2,||Ok(1),||Err(DENIED),submit,Duration::ZERO),Err(DENIED));
        assert_eq!(apply_checked(false,1,2,||Ok(2),||Ok(()),submit,Duration::ZERO),Err(STALE));assert_eq!(writes.get(),0);
        let calls=Cell::new(0);assert_eq!(apply_checked(false,1,2,||{calls.set(calls.get()+1);Ok(if calls.get()==1{1}else{2})},||Ok(()),submit,Duration::ZERO),Ok(()));assert_eq!(writes.get(),1);
        assert_eq!(apply_checked(false,1,2,||Ok(1),||Ok(()),submit,Duration::ZERO),Err(TIMEOUT));
        assert_eq!(apply_checked(false,1,2,||Ok(1),||Ok(()),||Err(DENIED),Duration::ZERO),Err(DENIED));
        let calls=Cell::new(0);assert_eq!(apply_checked(false,1,2,||{calls.set(calls.get()+1);Ok(if calls.get()==1{1}else{3})},||Ok(()),submit,Duration::ZERO),Err(DENIED));
        assert_eq!(apply_checked(false,1,2,||Err(UNAVAILABLE),||Ok(()),submit,Duration::ZERO),Err(UNAVAILABLE));
    }
    #[test]#[ignore="hosted read-only WinRT radio discovery; never requests consent or writes hardware"]
    fn native_read_only_radios_release_without_an_assumed_adapter(){
        if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
        for _ in 0..3{for kind in [Kind::Wifi,Kind::Bluetooth]{let view=read(kind,None);assert!(view.rows.iter().all(|r|r.request.is_none()));
            println!("FLUIDWIN_RADIOS_READ kind={} rows={} status={:?} writes=NONE",if kind==Kind::Wifi{"wifi"}else{"bluetooth"},view.rows.len(),view.error);}}
    }
}
