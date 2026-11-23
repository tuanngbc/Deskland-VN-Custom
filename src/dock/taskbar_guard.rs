use crate::desktop::wide;
use std::{
    env, mem, ptr, thread,
    io,
    os::windows::process::CommandExt,
    process::{Child,Command,Stdio},
    time::{Duration,Instant,SystemTime,UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::*,
    System::Threading::{CreateEventW,OpenEventW,OpenProcess,QueryFullProcessImageNameW,SetEvent,WaitForSingleObject,PROCESS_QUERY_LIMITED_INFORMATION},
    UI::{Input::KeyboardAndMouse::{RegisterHotKey,UnregisterHotKey,MOD_ALT,MOD_CONTROL,MOD_NOREPEAT,MOD_SHIFT,VK_F12},
        WindowsAndMessaging::*},
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
        let child=Command::new(exe).arg(ARG).arg(pid.to_string()).arg(format!("{token:032x}")).arg(expected)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).creation_flags(QUIET_PROCESS)
            .spawn().map_err(|e|e.to_string())?;
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
        let reason=watch(parent,stop,ready,armed,pulse,&expected);
        std::process::exit(reason as i32);
    }
}

fn health(expected:&[Snapshot],current:Option<&[Snapshot]>)->Option<Exit>{
    let Some(current)=current else{return Some(Exit::ExplorerChanged);};
    if !current.iter().map(|v|v.id).eq(expected.iter().map(|v|v.id)){Some(Exit::ExplorerChanged)}
    else if current.iter().any(|v|v.visible){Some(Exit::ShellRevealed)}
    else{None}
}

unsafe fn watch(parent:Handle,stop:Handle,ready:Handle,armed:Handle,pulse:Handle,expected:&[Snapshot])->Exit{
    let Some(starting)=snapshot()else{return Exit::ExplorerChanged;};
    if starting!=expected{return Exit::ExplorerChanged;}
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
    if SetEvent(ready.0)==0{return Exit::LinkLost;}
    let signals=[parent.0,stop.0,pulse.0];
    wait_for_restore(&signals,1000,Duration::from_secs(6),||health(expected,snapshot().as_deref()))
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
