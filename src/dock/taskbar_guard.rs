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
unsafe fn taskbars()->Option<(WindowId,Vec<WindowId>)>{
    let shell=WindowId::read(GetShellWindow())?;
    let primary=WindowId::read(FindWindowW(wide("Shell_TrayWnd").as_ptr(),ptr::null())).filter(|v|v.pid==shell.pid)?;
    let mut query=(shell.pid,Vec::new());
    if EnumWindows(Some(enum_taskbars),(&mut query as *mut (u32,Vec<WindowId>)) as LPARAM)==0{return None;}
    query.1.sort_unstable();
    query.1.dedup();
    if query.1.binary_search(&primary).is_err(){return None;}
    Some((primary,query.1))
}
fn names(pid:u32,token:u128)->([u16;96],[u16;96]){
    fn name(pid:u32,token:u128,role:&str)->[u16;96]{
        let mut out=[0;96];
        let text=format!("Local\\TuanpcDesk_Taskbar_{pid}_{token:032x}_{role}");
        for (cell,value) in out.iter_mut().zip(text.encode_utf16()){*cell=value;}
        out
    }
    (name(pid,token,"Stop"),name(pid,token,"Ready"))
}
unsafe fn visible(ids:&[WindowId])->Vec<WindowId>{
    ids.iter().copied().filter(|id|id.matches()&&IsWindowVisible(id.hwnd())!=0).collect()
}
unsafe fn restore(ids:&[WindowId]){
    // Never show previously hidden or re-used HWNDs. There are no appbar,
    // registry, preference or Explorer process changes to undo here.
    for &id in ids{if id.matches(){ShowWindowAsync(id.hwnd(),SW_SHOWNA);}}
}

pub(super) struct Watch {
    child:Child, stop:Handle, ready:Handle, hidden:Vec<WindowId>, started:Instant,
}
impl Watch {
    pub(super) unsafe fn start()->Result<Self,String>{
        let (_,ids)=taskbars().ok_or("Windows taskbar chưa sẵn sàng.")?;
        let hidden=visible(&ids);
        if hidden.is_empty(){return Err("Không thấy thanh tác vụ để ẩn.".into());}
        let pid=std::process::id();
        let token=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos();
        let (stop,ready)=names(pid,token);
        let stop=Handle::from_raw(CreateEventW(ptr::null(),1,0,stop.as_ptr())).map_err(|e|e.to_string())?;
        let ready=Handle::from_raw(CreateEventW(ptr::null(),1,0,ready.as_ptr())).map_err(|e|e.to_string())?;
        let expected=hidden.iter().map(|v|format!("{}:{}:{}",v.handle,v.pid,v.thread)).collect::<Vec<_>>().join(",");
        let exe=env::current_exe().map_err(|e|e.to_string())?;
        let child=Command::new(exe).arg(ARG).arg(pid.to_string()).arg(format!("{token:032x}")).arg(expected)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).creation_flags(QUIET_PROCESS)
            .spawn().map_err(|e|e.to_string())?;
        Ok(Self{child,stop,ready,hidden,started:Instant::now()})
    }
    pub(super) fn poll(&mut self)->Result<bool,()>{
        if !matches!(self.child.try_wait(),Ok(None)){return Err(());}
        if self.ready.fired(){return Ok(true);}
        if self.started.elapsed()>Duration::from_secs(3){return Err(());}
        Ok(false)
    }
}
impl Drop for Watch{
    fn drop(&mut self){
        unsafe{
            SetEvent(self.stop.0);
            restore(&self.hidden);
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
    let parse=|id:&str|->Option<WindowId>{
        let mut fields=id.split(':');
        let id=WindowId{handle:fields.next()?.parse().ok()?,pid:fields.next()?.parse().ok()?,thread:fields.next()?.parse().ok()?};
        fields.next().is_none().then_some(id)
    };
    let Some(expected)=args[4].split(',').map(parse).collect::<Option<Vec<_>>>()else{return true;};
    unsafe{
        // No public hide/restore CLI: a verified Deskland parent and its
        // active IPC lease must be acknowledged before touching any window.
        let Ok(parent)=Handle::from_raw(OpenProcess(SYNC|PROCESS_QUERY_LIMITED_INFORMATION,0,pid))else{return true;};
        if !same_executable(parent.0){return true;}
        let (stop,ready)=names(pid,token);
        let Ok(stop)=Handle::from_raw(OpenEventW(SYNC,0,stop.as_ptr()))else{return true;};
        let Ok(ready)=Handle::from_raw(OpenEventW(SIGNAL,0,ready.as_ptr()))else{return true;};
        watch(parent,stop,ready,&expected);
    }
    true
}

unsafe fn watch(parent:Handle,stop:Handle,ready:Handle,expected:&[WindowId]){
    let Some((primary,all))=taskbars()else{return;};
    let starting=visible(&all);
    if starting.is_empty()||starting!=expected||parent.fired()||stop.fired(){return;}
    let mut msg:MSG=mem::zeroed();
    PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_NOREMOVE);
    if RegisterHotKey(ptr::null_mut(),HOTKEY,MOD_CONTROL|MOD_ALT|MOD_SHIFT|MOD_NOREPEAT,VK_F12 as u32)==0{return;}
    struct Restore<'a>(&'a [WindowId]);
    impl Drop for Restore<'_>{
        fn drop(&mut self){unsafe{restore(self.0);UnregisterHotKey(ptr::null_mut(),HOTKEY);}}
    }
    let _restore=Restore(&starting);
    for &id in &starting{
        if !id.matches()||stop.fired()||parent.fired()||ShowWindowAsync(id.hwnd(),SW_HIDE)==0{return;}
    }
    let signals=[parent.0,stop.0];
    for _ in 0..24 {
        if parent.fired()||stop.fired(){return;}
        if visible(&starting).is_empty(){break;}
        thread::sleep(Duration::from_millis(25));
    }
    if !visible(&starting).is_empty()||SetEvent(ready.0)==0{return;}
    wait_for_restore(&signals,1000,|| {
        // Fail open when Explorer recreates a bar, or if Windows shows
        // one itself. We never chase fresh shell HWNDs to hide.
        primary.matches()&&starting.iter().all(|id|id.matches()&&IsWindowVisible(id.hwnd())==0)
    });
}
unsafe fn wait_for_restore(signals:&[HANDLE;2],interval:u32,mut healthy:impl FnMut()->bool){
    let mut msg:MSG=mem::zeroed();
    loop{
        match MsgWaitForMultipleObjects(signals.len() as u32,signals.as_ptr(),0,interval,QS_HOTKEY){
            WAIT_OBJECT_0|WAIT_FAILED=>break,
            n if n==WAIT_OBJECT_0+1=>break,
            n if n==WAIT_OBJECT_0+2=>{
                if PeekMessageW(&mut msg,ptr::null_mut(),WM_HOTKEY,WM_HOTKEY,PM_REMOVE)!=0&&msg.wParam==HOTKEY as usize{break;}
            }
            WAIT_TIMEOUT=>if !healthy(){break;},
            _=>break,
        }
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn watchdog_exits_on_either_signal_or_loss_of_shell() {
        unsafe{
            for index in [0,1,2] {
                let first=Handle::from_raw(CreateEventW(ptr::null(),1,0,ptr::null())).unwrap();
                let stop=Handle::from_raw(CreateEventW(ptr::null(),1,0,ptr::null())).unwrap();
                let handles=[first.0,stop.0];
                if index<2{assert_ne!(SetEvent(handles[index]),0);}
                let mut checks=0;
                wait_for_restore(&handles,1,||{checks+=1;false});
                assert_eq!(checks,usize::from(index==2));
            }
        }
    }
}
