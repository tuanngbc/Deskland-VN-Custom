use crate::desktop::wide;
use std::{
    env, mem, ptr, thread,
    io::{self,Write},
    path::PathBuf,
    os::windows::process::CommandExt,
    process::{Child,Command,Stdio},
    time::{Duration,Instant,SystemTime,UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::{GetMonitorInfoW,MonitorFromWindow,MONITORINFO,MONITOR_DEFAULTTONEAREST},
    System::Threading::{CreateEventW,OpenEventW,OpenProcess,QueryFullProcessImageNameW,SetEvent,WaitForSingleObject,PROCESS_QUERY_LIMITED_INFORMATION},
    UI::{Input::KeyboardAndMouse::{RegisterHotKey,UnregisterHotKey,MOD_ALT,MOD_CONTROL,MOD_NOREPEAT,MOD_SHIFT,VK_F12},
        Shell::{SHAppBarMessage,APPBARDATA,ABM_GETSTATE,ABS_AUTOHIDE},WindowsAndMessaging::*},
};

pub(crate) const ARG:&str="--dock-taskbar-guard";
const SYNC:u32=0x0010_0000; // SYNCHRONIZE access
const SIGNAL:u32=0x0002; // EVENT_MODIFY_STATE access
const HOTKEY:i32=1;
const QUIET_PROCESS:u32=0x0800_0000; // CREATE_NO_WINDOW

struct Handle(HANDLE);
impl Handle{
    fn from_raw(value:HANDLE)->io::Result<Self>{if value.is_null(){Err(io::Error::last_os_error())}else{Ok(Self(value))}}
    fn fired(&self)->bool{unsafe{WaitForSingleObject(self.0,0)==WAIT_OBJECT_0}}
}
impl Drop for Handle{fn drop(&mut self){unsafe{CloseHandle(self.0);}}}

#[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord)]
struct WindowId{handle:usize,pid:u32,thread:u32}
impl WindowId{
    unsafe fn read(hwnd:HWND)->Option<Self>{
        if hwnd.is_null(){return None;}
        let mut pid=0;
        let thread=GetWindowThreadProcessId(hwnd,&mut pid);
        (pid!=0&&thread!=0).then_some(Self{handle:hwnd as usize,pid,thread})
    }
    fn hwnd(self)->HWND{self.handle as HWND}
    unsafe fn matches(self)->bool{
        Self::read(self.hwnd())==Some(self)&&is_taskbar(self.hwnd())
    }
}
unsafe fn is_taskbar(hwnd:HWND)->bool{
    let mut class=[0;64];
    let len=GetClassNameW(hwnd,class.as_mut_ptr(),class.len() as i32).max(0) as usize;
    matches!(String::from_utf16_lossy(&class[..len]).as_str(),"Shell_TrayWnd"|"Shell_SecondaryTrayWnd")
}
unsafe extern "system" fn enum_taskbars(hwnd:HWND,raw:LPARAM)->BOOL{
    let (shell,ids)=&mut *(raw as *mut (u32,Vec<WindowId>));
    if is_taskbar(hwnd){
        if let Some(id)=WindowId::read(hwnd).filter(|id|id.pid==*shell){ids.push(id);}
    }
    1
}
unsafe fn taskbars()->Option<Vec<WindowId>>{
    let shell=WindowId::read(GetShellWindow())?;
    let primary=WindowId::read(FindWindowW(wide("Shell_TrayWnd").as_ptr(),ptr::null())).filter(|v|v.pid==shell.pid)?;
    let mut query=(shell.pid,Vec::new());
    if EnumWindows(Some(enum_taskbars),(&mut query as *mut (u32,Vec<WindowId>)) as LPARAM)==0{return None;}
    query.1.sort_unstable();
    query.1.dedup();
    if query.1.binary_search(&primary).is_err(){return None;}
    Some(query.1)
}
fn name(pid:u32,token:u128,role:&str)->Vec<u16>{
    wide(&format!("Local\\TuanpcDesk_Taskbar_{pid}_{token:032x}_{role}"))
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
struct Snapshot {id:WindowId,visible:bool}
unsafe fn snapshot()->Option<Vec<Snapshot>>{
    taskbars().map(|ids|ids.into_iter().map(|id|Snapshot{id,visible:IsWindowVisible(id.hwnd())!=0}).collect())
}
unsafe fn appbar_state()->u32{
    let mut bar:APPBARDATA=mem::zeroed();
    bar.cbSize=mem::size_of::<APPBARDATA>() as u32;
    SHAppBarMessage(ABM_GETSTATE,&mut bar) as u32
}
unsafe fn geometry(id:WindowId)->(Option<RECT>,Option<RECT>){
    let mut rect:RECT=mem::zeroed();
    let bounds=(GetWindowRect(id.hwnd(),&mut rect)!=0).then_some(rect);
    let monitor=MonitorFromWindow(id.hwnd(),MONITOR_DEFAULTTONEAREST);
    let mut info:MONITORINFO=mem::zeroed();info.cbSize=mem::size_of::<MONITORINFO>() as u32;
    let output=(GetMonitorInfoW(monitor,&mut info)!=0).then_some(info.rcMonitor);
    (bounds,output)
}
fn parked_at_bottom(bar:RECT,display:RECT)->bool{
    let span=bar.right.min(display.right)-bar.left.max(display.left);
    span>=(display.right-display.left)/2&&span>0&&bar.bottom-bar.top>=16
        &&(display.bottom-3..=display.bottom+3).contains(&bar.top)&&bar.bottom>display.bottom
}
fn pointer_over_bar(point:POINT,bar:RECT,display:RECT)->bool{
    // Use the actual on-screen strip, including the revealed area above
    // the bottom edge. A half off-screen bar must not claim another output.
    point.x>=bar.left.max(display.left)&&point.x<bar.right.min(display.right)
        &&point.y>=bar.top.max(display.top)&&point.y<bar.bottom.min(display.bottom)
}

// This timer starts after the pointer leaves the revealed bar, not when
// Explorer starts to show it. Re-entering the bar cancels the countdown.
const AUTO_HIDE_SETTLE:Duration=Duration::from_secs(3);
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum Reveal{Idle,Started,Waiting,Ended,Persistent}
#[derive(Clone,Copy,Default)]
struct AutoHideReveal{active:bool,away_since:Option<Instant>}
impl AutoHideReveal{
    fn observe(&mut self,visible:bool,over_bar:bool,now:Instant)->Reveal{
        if !visible{
            let ended=std::mem::take(self).active;
            return if ended{Reveal::Ended}else{Reveal::Idle};
        }
        let started=!std::mem::replace(&mut self.active,true);
        if over_bar{self.away_since=None;}
        else if now.saturating_duration_since(*self.away_since.get_or_insert(now))>=AUTO_HIDE_SETTLE{
            return Reveal::Persistent;
        }
        if started{Reveal::Started}else{Reveal::Waiting}
    }
}
unsafe fn visual_snapshot(raw:&[Snapshot],auto_hide:bool)->Vec<Snapshot>{
    raw.iter().map(|row|{
        let mut row=*row;
        if row.visible&&auto_hide{
            if let (Some(bar),Some(display))=geometry(row.id){
                if parked_at_bottom(bar,display){row.visible=false;}
            }
        }
        row
    }).collect()
}
pub(super) unsafe fn current()->super::taskbar_state::Observation{
    let Some(rows)=snapshot()else{return Default::default();};
    let visible=visual_snapshot(&rows,appbar_state()&ABS_AUTOHIDE!=0).iter().filter(|row|row.visible).count();
    super::taskbar_state::Observation{known:true,total:rows.len(),visible,
        parked:rows.iter().filter(|row|row.visible).count().saturating_sub(visible)}
}
unsafe fn observation(snapshot:Option<&[Snapshot]>)->serde_json::Value{
    let state=appbar_state();
    let mut cursor:POINT=mem::zeroed();
    let point=(GetCursorPos(&mut cursor)!=0).then(||[cursor.x,cursor.y]);
    let foreground=WindowId::read(GetForegroundWindow());
    let foreground_class=foreground.and_then(|id|{
        let mut buf=[0u16;96];let len=GetClassNameW(id.hwnd(),buf.as_mut_ptr(),buf.len() as i32);
        (len>0).then(||String::from_utf16_lossy(&buf[..len as usize]))
    });
    let windows=snapshot.map(|rows|rows.iter().map(|row|{
        let (bar,output)=geometry(row.id);
        let array=|r:RECT|[r.left,r.top,r.right,r.bottom];
        let parked=state&ABS_AUTOHIDE!=0&&bar.zip(output).is_some_and(|(b,o)|parked_at_bottom(b,o));
        serde_json::json!({"hwnd":row.id.handle,"pid":row.id.pid,"tid":row.id.thread,"visible":row.visible,
            "parkedAtEdge":parked,"rect":bar.map(array),"display":output.map(array)})
    }).collect::<Vec<_>>());
    serde_json::json!({"autoHide":state&ABS_AUTOHIDE!=0,"rawAppBarState":state,"cursor":point,
        "foregroundPid":foreground.map(|id|id.pid),"foregroundClass":foreground_class,"taskbars":windows})
}
fn trace_path()->Option<PathBuf>{
    let root=PathBuf::from(env::var_os("TUANPC_DESK_TASKBAR_TRIAL_TRACE")?);
    if !root.is_absolute()||std::fs::read(root.join(".tuanpc-desk-trial")).ok()?.as_slice()!=b"tuanpc-desk isolated native smoke profile v1\n"{return None;}
    Some(root.join("taskbar-watchdog.jsonl"))
}
unsafe fn trace(path:Option<&std::path::Path>,event:&str,started:Instant,snapshot:Option<&[Snapshot]>){
    let Some(path)=path else{return;};
    let value=serde_json::json!({"event":event,"elapsedMs":started.elapsed().as_millis() as u64,
        "utcMs":SystemTime::now().duration_since(UNIX_EPOCH).map_or(0,|t|t.as_millis() as u64),
        "observation":observation(snapshot)});
    if let Ok(mut file)=std::fs::OpenOptions::new().create(true).append(true).open(path){let _=writeln!(file,"{value}");}
}
pub(crate) fn report_trial_observation()->std::io::Result<()>{
    let profile=crate::trial::data_dir().ok_or_else(||io::Error::other("A marked trial profile is required"))?;
    let report=unsafe{let current=snapshot();observation(current.as_deref())};
    std::fs::write(profile.join("taskbar-observation.json"),report.to_string())
}

unsafe fn request_hide(hwnd:HWND)->bool{
    // Queue a visibility-only change without activating or moving Explorer.
    SetWindowPos(hwnd,ptr::null_mut(),0,0,0,0,
        SWP_HIDEWINDOW|SWP_NOACTIVATE|SWP_NOZORDER|SWP_NOOWNERZORDER|SWP_NOMOVE|SWP_NOSIZE|SWP_ASYNCWINDOWPOS)!=0
}
unsafe fn restore(ids:&[WindowId]){
    for &id in ids{if id.matches()&&IsWindowVisible(id.hwnd())==0{ShowWindowAsync(id.hwnd(),SW_SHOWNA);}}
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
#[repr(i32)]
pub(super) enum Exit { Requested=0, Emergency=20, ExplorerChanged=21, ShellRevealed=22,
    DockLost=23, HotkeyUnavailable=24, HideFailed=25, LinkLost=26, HideRejected=27, ReadyTimeout=28 }
impl Exit{
    fn from_code(code:Option<i32>)->Self{
        match code{Some(0)=>Self::Requested,Some(20)=>Self::Emergency,Some(21)=>Self::ExplorerChanged,
            Some(22)=>Self::ShellRevealed,Some(23)=>Self::DockLost,Some(24)=>Self::HotkeyUnavailable,
            Some(25)=>Self::HideFailed,Some(27)=>Self::HideRejected,Some(28)=>Self::ReadyTimeout,_=>Self::LinkLost}
    }
    pub(super) fn message(self)->&'static str{
        match self{
            Self::Requested=>"Đã chuyển sang chế độ tương thích Windows.",
            Self::Emergency=>"Đã hiện thanh tác vụ theo phím khôi phục Ctrl+Alt+Shift+F12.",
            Self::ExplorerChanged=>"Explorer đã thay đổi thanh tác vụ. Đã về chế độ tương thích; bật lại chế độ Dock nếu cần.",
            Self::ShellRevealed=>"Windows đã tự hiện thanh tác vụ. Đã về chế độ tương thích.",
            Self::DockLost=>"Dock không còn phản hồi. Đã khôi phục thanh tác vụ Windows.",
            Self::HotkeyUnavailable=>"Không đăng ký được Ctrl+Alt+Shift+F12. Giữ thanh tác vụ Windows.",
            Self::HideFailed=>"Explorer vẫn giữ ít nhất một thanh tác vụ hiện. Đã trả về chế độ tương thích.",
            Self::HideRejected=>"Windows từ chối yêu cầu ẩn thanh tác vụ.",
            Self::ReadyTimeout=>"Bộ khôi phục không xác nhận kịp. Đã giữ lại thanh tác vụ.",
            Self::LinkLost=>"Mất liên lạc với bộ khôi phục. Đã trả thanh tác vụ cho Windows.",
        }
    }
}

pub(super) struct Watch {
    child:Child, stop:Handle, ready:Handle, armed:Handle, pulse:Handle, hidden:Vec<WindowId>, started:Instant,
}
impl Watch {
    pub(super) fn restore_count(&self)->usize{self.hidden.len()}
    pub(super) unsafe fn start()->Result<Self,String>{
        let all=snapshot().ok_or("Thanh tác vụ Windows chưa sẵn sàng.")?;
        let hidden=all.iter().filter(|item|item.visible).map(|item|item.id).collect::<Vec<_>>();
        if hidden.is_empty(){return Err("Không thấy thanh tác vụ để ẩn.".into());}
        let pid=std::process::id();
        let token=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos();
        let event=|role,manual|Handle::from_raw(CreateEventW(ptr::null(),manual,0,name(pid,token,role).as_ptr())).map_err(|e|e.to_string());
        let stop=event("Stop",1)?;
        let armed=event("Armed",1)?;
        let ready=event("Ready",1)?;
        let pulse=event("Pulse",0)?;
        let expected=all.iter().map(|v|format!("{}:{}:{}:{}",v.id.handle,v.id.pid,v.id.thread,u8::from(v.visible))).collect::<Vec<_>>().join(",");
        let exe=env::current_exe().map_err(|e|e.to_string())?;
        let mut command=Command::new(exe);
        command.arg(ARG).arg(pid.to_string()).arg(format!("{token:032x}")).arg(expected)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).creation_flags(QUIET_PROCESS)
            .env_remove("TUANPC_DESK_TASKBAR_TRIAL_TRACE");
        if let Some(profile)=crate::trial::data_dir(){command.env("TUANPC_DESK_TASKBAR_TRIAL_TRACE",profile);}
        let child=command.spawn().map_err(|e|e.to_string())?;
        Ok(Self{child,stop,ready,armed,pulse,hidden,started:Instant::now()})
    }
    pub(super) fn poll(&mut self)->Result<bool,Exit>{
        match self.child.try_wait(){
            Ok(Some(status))=>return Err(Exit::from_code(status.code())),
            Err(_)=>return Err(Exit::LinkLost),
            Ok(None)=>{}
        }
        if unsafe{SetEvent(self.pulse.0)}==0{return Err(Exit::LinkLost);}
        if self.ready.fired(){return Ok(true);}
        if self.started.elapsed()>Duration::from_secs(4){return Err(Exit::ReadyTimeout);}
        Ok(false)
    }
}
impl Drop for Watch{
    fn drop(&mut self){
        unsafe{
            SetEvent(self.stop.0);
            if self.armed.fired(){restore(&self.hidden);}
        }
    }
}

unsafe fn same_executable(handle:HANDLE)->bool{
    let mut path=vec![0u16;32768];let mut len=path.len() as u32;
    if QueryFullProcessImageNameW(handle,0,path.as_mut_ptr(),&mut len)==0{return false;}
    let candidate=String::from_utf16_lossy(&path[..len as usize]);
    env::current_exe().is_ok_and(|own|own.to_string_lossy().eq_ignore_ascii_case(&candidate))
}

pub(crate) fn watchdog(args:&[String])->bool{
    if args.get(1).map(String::as_str)!=Some(ARG){return false;}
    if args.len()!=5{return true;}
    let Some(pid)=args[2].parse::<u32>().ok().filter(|&p|p!=0&&p!=std::process::id())else{return true;};
    let Ok(token)=u128::from_str_radix(&args[3],16)else{return true;};
    let parse=|item:&str|->Option<Snapshot>{
        let mut f=item.split(':');
        let id=WindowId{handle:f.next()?.parse().ok()?,pid:f.next()?.parse().ok()?,thread:f.next()?.parse().ok()?};
        let visible=match f.next()?{"0"=>false,"1"=>true,_=>return None};
        f.next().is_none().then_some(Snapshot{id,visible})
    };
    let Some(expected)=args[4].split(',').map(parse).collect::<Option<Vec<_>>>()else{return true;};
    unsafe{
        let Ok(parent)=Handle::from_raw(OpenProcess(SYNC|PROCESS_QUERY_LIMITED_INFORMATION,0,pid))else{return true;};
        if !same_executable(parent.0){return true;}
        let event=|role,access|Handle::from_raw(OpenEventW(access,0,name(pid,token,role).as_ptr()));
        let (Ok(stop),Ok(ready),Ok(armed),Ok(pulse))=(event("Stop",SYNC),event("Ready",SIGNAL),
            event("Armed",SIGNAL),event("Pulse",SYNC))else{return true;};
        let started=Instant::now();
        let proof=trace_path();
        let reason=watch(parent,stop,ready,armed,pulse,&expected,proof.as_deref(),started);
        if proof.is_some(){let current=snapshot();trace(proof.as_deref(),&format!("exit:{reason:?}"),started,current.as_deref());}
        std::process::exit(reason as i32);
    }
}

fn health(expected:&[Snapshot],current:Option<&[Snapshot]>)->Option<Exit>{
    let Some(current)=current else{return Some(Exit::ExplorerChanged);};
    if !current.iter().map(|v|v.id).eq(expected.iter().map(|v|v.id)){Some(Exit::ExplorerChanged)}
    else if current.iter().any(|v|v.visible){Some(Exit::ShellRevealed)}
    else{None}
}

const INVENTORY_SETTLE:Duration=Duration::from_secs(2);
#[derive(Default)]
struct InventoryGap{unverified_since:Option<Instant>}
impl InventoryGap{
    fn expired(&mut self,verified:bool,now:Instant)->bool{
        if verified{self.unverified_since=None;return false;}
        now.saturating_duration_since(*self.unverified_since.get_or_insert(now))>=INVENTORY_SETTLE
    }
}

unsafe fn watch(parent:Handle,stop:Handle,ready:Handle,armed:Handle,pulse:Handle,expected:&[Snapshot],proof:Option<&std::path::Path>,started:Instant)->Exit{
    let Some(starting)=snapshot()else{return Exit::ExplorerChanged;};
    if starting!=expected{return Exit::ExplorerChanged;}
    let auto_hide=appbar_state()&ABS_AUTOHIDE!=0;
    trace(proof,"before-hide",started,Some(&starting));
    let hidden=starting.iter().filter(|item|item.visible).map(|item|item.id).collect::<Vec<_>>();
    if hidden.is_empty()||parent.fired()||stop.fired(){return Exit::Requested;}
    let mut msg:MSG=mem::zeroed();
    PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_NOREMOVE);
    if RegisterHotKey(ptr::null_mut(),HOTKEY,MOD_CONTROL|MOD_ALT|MOD_SHIFT|MOD_NOREPEAT,VK_F12 as u32)==0{return Exit::HotkeyUnavailable;}
    struct Restore(Vec<WindowId>);
    impl Drop for Restore{
        fn drop(&mut self){unsafe{restore(&self.0);UnregisterHotKey(ptr::null_mut(),HOTKEY);}}
    }
    let mut rollback=Restore(Vec::new());
    if SetEvent(armed.0)==0{return Exit::LinkLost;}
    for &id in &hidden{
        if parent.fired()||stop.fired(){return Exit::Requested;}
        if !id.matches(){return Exit::ExplorerChanged;}
        if !request_hide(id.hwnd()){return Exit::HideRejected;}
        rollback.0.push(id);
    }
    for _ in 0..60{
        if parent.fired()||stop.fired(){return Exit::Requested;}
        match health(expected,snapshot().as_deref()){
            None=>break,
            Some(Exit::ShellRevealed)=>thread::sleep(Duration::from_millis(24)),
            Some(reason)=>return reason,
        }
    }
    if let Some(reason)=health(expected,snapshot().as_deref()){
        return if reason==Exit::ShellRevealed{Exit::HideFailed}else{reason};
    }
    if proof.is_some(){let current=snapshot();trace(proof,"hidden-confirmed",started,current.as_deref());}
    if SetEvent(ready.0)==0{return Exit::LinkLost;}
    let signals=[parent.0,stop.0,pulse.0];
    // Each original bar has an independent reveal timer. A pointer over
    // one output cannot keep a different, persistently visible bar alive.
    let mut reveals=vec![AutoHideReveal::default();expected.len()];
    let mut inventory_gap=InventoryGap::default();
    let mut inventory_missing=false;
    let interval=if proof.is_some(){250}else if auto_hide{750}else{1000};
    wait_for_restore(&signals,interval,Duration::from_secs(6),||{
        let mut current=snapshot();
        let now=Instant::now();
        if current.is_none(){
            if !inventory_missing{trace(proof,"inventory-unavailable",started,None);}
            inventory_missing=true;
            // Search/Start can briefly make the shell inventory unavailable.
            // Continue with only the original, revalidated HWNDs; never
            // adopt another window from a partial snapshot.
            let originals=expected.iter().map(|row|{
                row.id.matches().then(||Snapshot{id:row.id,visible:IsWindowVisible(row.id.hwnd())!=0})
            }).collect::<Option<Vec<_>>>();
            if inventory_gap.expired(originals.is_some(),now){
                trace(proof,"before-restore:ExplorerChanged",started,None);
                return Some(Exit::ExplorerChanged);
            }
            current=originals;
            if current.is_none(){return None;}
        }else{
            inventory_gap.expired(true,now);
            if std::mem::take(&mut inventory_missing){trace(proof,"inventory-recovered",started,current.as_deref());}
        }
        let raw_status=health(expected,current.as_deref());
        let status=if raw_status==Some(Exit::ShellRevealed)&&auto_hide&&appbar_state()&ABS_AUTOHIDE!=0{
            let mut pos:POINT=mem::zeroed();
            let cursor=if GetCursorPos(&mut pos)!=0{Some(pos)}else{None};
            let now=Instant::now();
            let mut began=false;
            let mut ended=false;
            let mut persistent=false;
            for (row,peek) in current.as_deref().unwrap_or_default().iter().zip(&mut reveals){
                let (visible,over)=if row.visible{
                    let pair=geometry(row.id);
                    let on_screen=pair.0.zip(pair.1);
                    (!on_screen.is_some_and(|(bar,display)|parked_at_bottom(bar,display)),
                        cursor.zip(on_screen).is_some_and(|(point,(bar,display))|pointer_over_bar(point,bar,display)))
                }else{(false,false)};
                match peek.observe(visible,over,now){
                    Reveal::Started=>began=true,
                    Reveal::Ended=>ended=true,
                    Reveal::Persistent=>persistent=true,
                    Reveal::Waiting|Reveal::Idle=>{}
                }
            }
            if began{trace(proof,"edge-reveal-observed",started,current.as_deref());}
            if ended{trace(proof,"edge-reveal-ended",started,current.as_deref());}
            persistent.then_some(Exit::ShellRevealed)
        }else{
            if raw_status.is_none(){
                let mut ended=false;
                for peek in &mut reveals{ended|=std::mem::take(peek).active;}
                if ended{trace(proof,"edge-reveal-ended",started,current.as_deref());}
            }
            raw_status
        };
        if let Some(reason)=status{trace(proof,&format!("before-restore:{reason:?}"),started,current.as_deref());}
        status
    })
}

unsafe fn wait_for_restore(signals:&[HANDLE;3],interval:u32,lease:Duration,mut health:impl FnMut()->Option<Exit>)->Exit{
    let mut msg:MSG=mem::zeroed();
    let mut pulse=Instant::now();
    let mut observed=Instant::now();
    loop{
        match MsgWaitForMultipleObjects(signals.len() as u32,signals.as_ptr(),0,interval,QS_HOTKEY){
            WAIT_OBJECT_0=>return Exit::DockLost,
            n if n==WAIT_OBJECT_0+1=>return Exit::Requested,
            n if n==WAIT_OBJECT_0+2=>pulse=Instant::now(),
            n if n==WAIT_OBJECT_0+3=>{
                if PeekMessageW(&mut msg,ptr::null_mut(),WM_HOTKEY,WM_HOTKEY,PM_REMOVE)!=0&&msg.wParam==HOTKEY as usize{return Exit::Emergency;}
            }
            WAIT_TIMEOUT=>{},
            _=>return Exit::LinkLost,
        }
        if pulse.elapsed()>=lease{return Exit::DockLost;}
        if observed.elapsed()>=Duration::from_millis(interval as u64){
            observed=Instant::now();
            if let Some(reason)=health(){return reason;}
        }
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn transient_missing_shell_inventory_is_not_an_explorer_restart(){
        let start=Instant::now();
        let mut gap=InventoryGap::default();
        assert!(!gap.expired(false,start));
        assert!(!gap.expired(false,start+Duration::from_millis(1600)));
        assert!(!gap.expired(true,start+Duration::from_millis(1700)));
        assert!(!gap.expired(true,start+Duration::from_secs(6)));
        let missing_again=start+Duration::from_secs(7);
        assert!(!gap.expired(false,missing_again));
        assert!(gap.expired(false,missing_again+INVENTORY_SETTLE));
    }
    #[test]
    fn auto_hide_sliver_is_not_the_full_taskbar_even_on_a_negative_coordinate_monitor(){
        let bounds=|r:[i32;4]|RECT{left:r[0],top:r[1],right:r[2],bottom:r[3]};
        let portrait=bounds([-1080,-731,0,1189]);
        assert!(parked_at_bottom(bounds([-1080,1187,0,1235]),portrait));
        assert!(!parked_at_bottom(bounds([-1080,1141,0,1189]),portrait));
        assert!(!parked_at_bottom(bounds([0,1078,2560,1126]),portrait));
        assert!(!parked_at_bottom(bounds([-1080,1187,-1000,1235]),portrait));
    }
    #[test]
    fn owner_recording_reveal_stays_in_dock_while_hovering_and_during_explorer_slide(){
        let main=RECT{left:0,top:0,right:2560,bottom:1080};
        let bar=RECT{left:0,top:1032,right:2560,bottom:1080};
        let point=POINT{x:2473,y:1071};
        // The old bottom-eight-pixel check returned false here.
        assert!(pointer_over_bar(point,bar,main));
        assert!(!pointer_over_bar(POINT{x:2473,y:1080},bar,main));
        let t=Instant::now();
        let mut reveal=AutoHideReveal::default();
        assert_eq!(reveal.observe(true,true,t),Reveal::Started);
        assert_eq!(reveal.observe(true,true,t+Duration::from_secs(15)),Reveal::Waiting);
        let away=t+Duration::from_secs(16);
        assert_eq!(reveal.observe(true,false,away),Reveal::Waiting);
        assert_eq!(reveal.observe(false,false,away+Duration::from_millis(240)),Reveal::Ended);
        // First sample may already be after the pointer left. It still gets
        // a chance to settle; this is not immediate Explorer replacement.
        assert_eq!(reveal.observe(true,false,away+Duration::from_secs(4)),Reveal::Started);
        assert_eq!(reveal.observe(false,false,away+Duration::from_secs(5)),Reveal::Ended);
    }
    #[test]
    fn persistent_reveal_is_independent_of_pointer_activity_on_the_other_screen(){
        let main=RECT{left:0,top:0,right:2560,bottom:1080};
        let main_bar=RECT{top:1032,..main};
        let portrait=RECT{left:-1080,top:-731,right:0,bottom:1189};
        let portrait_bar=RECT{top:1141,..portrait};
        let pointer=POINT{x:-300,y:1180};
        assert!(pointer_over_bar(pointer,portrait_bar,portrait));
        assert!(!pointer_over_bar(pointer,main_bar,main));
        let now=Instant::now();
        let mut a=AutoHideReveal::default();
        let mut b=AutoHideReveal::default();
        assert_eq!(a.observe(true,false,now),Reveal::Started);
        assert_eq!(b.observe(true,true,now),Reveal::Started);
        assert_eq!(a.observe(true,false,now+AUTO_HIDE_SETTLE-Duration::from_millis(1)),Reveal::Waiting);
        assert_eq!(a.observe(true,false,now+AUTO_HIDE_SETTLE),Reveal::Persistent);
        assert_eq!(b.observe(true,true,now+AUTO_HIDE_SETTLE),Reveal::Waiting);
    }
    #[test]
    fn returning_to_the_revealed_bar_restarts_its_settle_window(){
        let start=Instant::now();
        let mut reveal=AutoHideReveal::default();
        assert_eq!(reveal.observe(true,false,start),Reveal::Started);
        assert_eq!(reveal.observe(true,true,start+Duration::from_secs(2)),Reveal::Waiting);
        assert_eq!(reveal.observe(true,false,start+Duration::from_secs(4)),Reveal::Waiting);
        assert_eq!(reveal.observe(true,false,start+Duration::from_secs(7)),Reveal::Persistent);
    }
    #[test]
    fn distinguishes_explicit_restore_parent_exit_and_missing_dock_heartbeat() {
        unsafe{
            for (index,reason) in [(Some(0),Exit::DockLost),(Some(1),Exit::Requested),(None,Exit::DockLost)]{
                let events=[1,1,0].map(|manual|Handle::from_raw(CreateEventW(ptr::null(),manual,0,ptr::null())).unwrap());
                let handles=events.each_ref().map(|event|event.0);
                if let Some(index)=index{assert_ne!(SetEvent(handles[index]),0);}
                assert_eq!(wait_for_restore(&handles,1,Duration::from_millis(8),||None),reason);
            }
        }
    }
    #[test]
    #[ignore = "uses an isolated cross-thread Win32 window on the Actions desktop"]
    fn native_visibility_request_hides_a_window_without_moving_it(){
        let (send,receive)=std::sync::mpsc::sync_channel(1);
        let fixture=thread::spawn(move||unsafe{
            let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,wide("STATIC").as_ptr(),
                wide("Deskland guard fixture").as_ptr(),WS_POPUP,50,50,260,120,
                ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),ptr::null());
            assert!(!hwnd.is_null());
            ShowWindow(hwnd,SW_SHOWNOACTIVATE);
            send.send(hwnd as usize).unwrap();
            let deadline=Instant::now()+Duration::from_secs(4);
            while Instant::now()<deadline{
                let mut msg:MSG=mem::zeroed();
                while PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)!=0{
                    if msg.message==WM_CLOSE{DestroyWindow(hwnd);return;}
                    DispatchMessageW(&msg);
                }
                thread::sleep(Duration::from_millis(10));
            }
            DestroyWindow(hwnd);
        });
        unsafe{
            let hwnd=receive.recv_timeout(Duration::from_secs(3)).unwrap() as HWND;
            let mut before:RECT=mem::zeroed();
            assert_ne!(GetWindowRect(hwnd,&mut before),0);
            assert_ne!(IsWindowVisible(hwnd),0);
            assert!(request_hide(hwnd));
            let started=Instant::now();
            while IsWindowVisible(hwnd)!=0&&started.elapsed()<Duration::from_secs(2){
                thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(IsWindowVisible(hwnd),0);
            let mut after:RECT=mem::zeroed();
            assert_ne!(GetWindowRect(hwnd,&mut after),0);
            assert_eq!((before.left,before.top,before.right,before.bottom),(after.left,after.top,after.right,after.bottom));
            PostMessageW(hwnd,WM_CLOSE,0,0);
        }
        fixture.join().unwrap();
    }
    #[test]
    fn startup_reports_api_rejection_visibility_failure_and_readiness_timeout_separately(){
        for reason in [Exit::HideRejected,Exit::HideFailed,Exit::ReadyTimeout]{
            assert_eq!(Exit::from_code(Some(reason as i32)),reason);
        }
        assert_ne!(Exit::HideFailed.message(),Exit::ReadyTimeout.message());
    }
    #[test]
    fn both_bars_and_previously_hidden_bars_are_checked_without_adopting_new_windows(){
        let main=Snapshot{id:WindowId{handle:1,pid:4,thread:2},visible:true};
        let secondary=Snapshot{id:WindowId{handle:2,pid:4,thread:3},visible:false};
        let expected=[main,secondary];
        let mut actual=[Snapshot{visible:false,..main},secondary];
        assert_eq!(health(&expected,Some(&actual)),None);
        actual[1].visible=true;
        assert_eq!(health(&expected,Some(&actual)),Some(Exit::ShellRevealed));
        actual[1].id.thread=8;
        assert_eq!(health(&expected,Some(&actual)),Some(Exit::ExplorerChanged));
        assert_eq!(health(&expected,Some(&actual[..1])),Some(Exit::ExplorerChanged));
        assert_eq!(health(&expected,None),Some(Exit::ExplorerChanged));
    }
}
