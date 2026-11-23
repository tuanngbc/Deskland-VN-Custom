//! Opt-in Win tap detection. Physical inputs always continue through the
//! hook chain; the only synthesized events are a one-shot menu mask pair.
use std::{cell::Cell, mem::{size_of, zeroed}, ptr::null_mut, sync::{Arc,mpsc}, thread::JoinHandle};
use windows_sys::Win32::{Foundation::*, System::{LibraryLoader::GetModuleHandleW,Threading::{CreateEventW,SetEvent,INFINITE}},
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*}};

pub(super) const NOTICE:u32=WM_APP+57;
// The x64 UI message carries full signed desktop coordinates. Reserve
// values outside the desktop range for a missing pointer and mask failure.
const TAP_WITHOUT_CURSOR:isize=isize::MIN;
const MASK_ERROR:isize=isize::MAX;
pub(super) fn cursor_now()->Option<POINT>{
    let mut point=POINT::default();
    (unsafe{GetCursorPos(&mut point)}!=0).then_some(point)
}
fn encode_tap(point:Option<POINT>)->LPARAM{
    point.map_or(TAP_WITHOUT_CURSOR,|p|((u64::from(p.y as u32)<<32)|u64::from(p.x as u32)) as LPARAM)
}
pub(super) fn decode_tap(message:LPARAM)->Result<Option<POINT>,()>{
    match message{
        MASK_ERROR=>Err(()),
        TAP_WITHOUT_CURSOR=>Ok(None),
        p=>Ok(Some(POINT{x:p as i32,y:((p as u64)>>32) as i32})),
    }
}
const MENU_MASK:u16=0xe8;

#[derive(Clone,Copy,Default)]
struct Keys {
    down:[u64;4],
    candidate:Option<(u32,usize)>,
}

impl Keys {
    fn edge(&mut self,key:u32,down:bool,injected:bool,foreground:usize)->bool{
        if injected||key>=256{return false;}
        let index=(key/64) as usize;
        let bit=1u64<<(key%64);
        let repeated=self.down[index]&bit!=0;
        let win=key==u32::from(VK_LWIN)||key==u32::from(VK_RWIN);
        if down {
            if repeated{return false;}
            self.candidate=(win&&self.down.iter().all(|&slot|slot==0)).then_some((key,foreground));
            self.down[index]|=bit;
            false
        }else{
            self.down[index]&=!bit;
            win&&self.candidate.take()==Some((key,foreground))&&repeated
        }
    }
    fn cancel(&mut self){self.candidate=None;}
}

thread_local! {
    static KEYS:Cell<Keys>=Cell::new(Keys::default());
    static TARGET:Cell<(usize,usize)>=const{Cell::new((0,0))};
    static MASK_FAILED:Cell<bool>=const{Cell::new(false)};
}

struct Quit(usize);
impl Drop for Quit{fn drop(&mut self){unsafe{CloseHandle(self.0 as HANDLE);}}}

pub(super) struct Hook { quit:Arc<Quit>, thread:Option<JoinHandle<()>>, pub ticket:usize }
impl Hook {
    pub fn install(target:HWND,ticket:usize)->Result<Self,String>{
        let target=target as usize;
        let handle=unsafe{CreateEventW(std::ptr::null(),1,0,std::ptr::null())};
        if handle.is_null(){return Err(format!("CreateEventW: {}",unsafe{GetLastError()}));}
        let quit=Arc::new(Quit(handle as usize));let thread_quit=quit.clone();
        let (ready,answer)=mpsc::sync_channel(1);
        let thread=std::thread::Builder::new().name("Deskland Win tap".into()).spawn(move||unsafe{
            let mut msg:MSG=zeroed();
            PeekMessageW(&mut msg,null_mut(),0,0,PM_NOREMOVE);
            let module=GetModuleHandleW(null_mut());
            let keyboard=SetWindowsHookExW(WH_KEYBOARD_LL,Some(keyboard),module,0);
            if keyboard.is_null(){let _=ready.send(Err(format!("WH_KEYBOARD_LL: {}",GetLastError())));return;}
            let mouse=SetWindowsHookExW(WH_MOUSE_LL,Some(mouse),module,0);
            if mouse.is_null(){
                let error=GetLastError();UnhookWindowsHookEx(keyboard);
                let _=ready.send(Err(format!("WH_MOUSE_LL: {error}")));return;
            }
            let mut keys=Keys::default();
            for vk in 8..256{
                if !matches!(vk,0x10..=0x12)&&GetAsyncKeyState(vk)<0{keys.edge(vk as u32,true,false,0);}
            }
            keys.cancel();KEYS.set(keys);TARGET.set((target,ticket));
            if ready.send(Ok(())).is_ok(){
                let quit=thread_quit.0 as HANDLE;
                'messages:loop{
                    let r=MsgWaitForMultipleObjectsEx(1,&quit,INFINITE,QS_ALLINPUT,MWMO_INPUTAVAILABLE);
                    if r!=WAIT_OBJECT_0+1{break;}
                    while PeekMessageW(&mut msg,null_mut(),0,0,PM_REMOVE)!=0{
                        if msg.message==WM_QUIT{break 'messages;}
                        DispatchMessageW(&msg);
                    }
                }
            }
            TARGET.set((0,0));UnhookWindowsHookEx(mouse);UnhookWindowsHookEx(keyboard);
        }).map_err(|e|e.to_string())?;
        match answer.recv(){
            Ok(Ok(()))=>Ok(Self{quit,thread:Some(thread),ticket}),
            other=>{let _=thread.join();Err(other.ok().and_then(Result::err).unwrap_or_else(||"Luồng phím Win đã dừng.".into()))}
        }
    }
    pub fn finished(&self)->bool{self.thread.as_ref().is_none_or(JoinHandle::is_finished)}
}
impl Drop for Hook {
    fn drop(&mut self){
        unsafe{SetEvent(self.quit.0 as HANDLE);}
        if let Some(thread)=self.thread.take(){let _=thread.join();}
    }
}

unsafe extern "system" fn keyboard(code:i32,w:WPARAM,l:LPARAM)->LRESULT{
    if code==HC_ACTION as i32&&l!=0&&matches!(w as u32,WM_KEYDOWN|WM_SYSKEYDOWN|WM_KEYUP|WM_SYSKEYUP){
        let e=&*(l as *const KBDLLHOOKSTRUCT);
        let injected=e.flags&LLKHF_INJECTED!=0;
        let win=e.vkCode==u32::from(VK_LWIN)||e.vkCode==u32::from(VK_RWIN);
        let foreground=if win&&!injected{GetForegroundWindow() as usize}else{0};
        let mut keys=KEYS.get();
        let tap=keys.edge(e.vkCode,matches!(w as u32,WM_KEYDOWN|WM_SYSKEYDOWN),injected,foreground);
        KEYS.set(keys);
        let (target,ticket)=TARGET.get();
        if tap&&target!=0&&!MASK_FAILED.get(){
            let tap_message=encode_tap(cursor_now());
            let mut pair:[INPUT;2]=zeroed();
            for (i,input) in pair.iter_mut().enumerate(){
                input.r#type=INPUT_KEYBOARD;
                input.Anonymous.ki=KEYBDINPUT{wVk:MENU_MASK,dwFlags:if i==0{0}else{KEYEVENTF_KEYUP},..zeroed()};
            }
            let sent=SendInput(2,pair.as_ptr(),size_of::<INPUT>() as i32);
            if sent==2{
                PostMessageW(target as HWND,NOTICE,ticket,tap_message);
            }else{
                // Try to release the unassigned mask key if only down made
                // it through. Always allow the physical Win-up.
                if sent==1{SendInput(1,pair[1..].as_ptr(),size_of::<INPUT>() as i32);}
                MASK_FAILED.set(true);
                PostMessageW(target as HWND,NOTICE,ticket,MASK_ERROR);
            }
        }
    }
    CallNextHookEx(null_mut(),code,w,l)
}
unsafe extern "system" fn mouse(code:i32,w:WPARAM,l:LPARAM)->LRESULT{
    if code==HC_ACTION as i32&&l!=0&&matches!(w as u32,
        WM_LBUTTONDOWN|WM_RBUTTONDOWN|WM_MBUTTONDOWN|WM_XBUTTONDOWN|WM_MOUSEWHEEL|WM_MOUSEHWHEEL){
        if (*(l as *const MSLLHOOKSTRUCT)).flags&LLMHF_INJECTED==0{
            let mut keys=KEYS.get();keys.cancel();KEYS.set(keys);
        }
    }
    CallNextHookEx(null_mut(),code,w,l)
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test] fn captured_pointer_keeps_signed_monitor_coordinates(){
        for point in [POINT{x:-815,y:400},POINT{x:2570,y:-1180},POINT{x:-1,y:-1},POINT{x:0,y:0}]{
            let got=decode_tap(encode_tap(Some(point))).unwrap().unwrap();
            assert_eq!((got.x,got.y),(point.x,point.y));
        }
        assert!(decode_tap(encode_tap(None)).unwrap().is_none());
        assert!(decode_tap(MASK_ERROR).is_err());
    }
    #[test] fn preference_is_opt_in_and_round_trips(){
        let mut old=super::super::Options::from_json(&serde_json::json!({"enabled":true}));
        assert!(!old.win_key_start);
        old.win_key_start=true;
        assert!(super::super::Options::from_json(&old.json()).win_key_start);
    }
    #[test]
    #[ignore="install and remove low-level hooks only in isolated Windows CI"]
    fn native_hook_thread_installs_and_unhooks_without_synthetic_keys(){
        let hook=Hook::install(null_mut(),1).expect("hook thread and message queue available");
        assert!(!hook.finished());
        drop(hook);
        let hook=Hook::install(null_mut(),2).expect("hook can be restarted after cleanup");
        assert!(!hook.finished());
        drop(hook);
    }
    fn key(k:&mut Keys,v:u16,d:bool)->bool{k.edge(u32::from(v),d,false,11)}
    #[test] fn either_win_and_repeats_toggle_once_per_release(){
        let mut keys=Keys::default();
        for win in [VK_LWIN,VK_RWIN,VK_LWIN]{
            for _ in 0..20{assert!(!key(&mut keys,win,true));}
            assert!(key(&mut keys,win,false));
            assert!(!key(&mut keys,win,false));
        }
    }
    #[test] fn all_shell_chords_cancel_tap(){
        for (mods,other) in [
            (None,0x45),(None,0x52),(None,0x44),(None,0x49),(None,VK_TAB),(None,0x58),(None,0x4c),
            (None,0x4e),(None,0x41),(None,0x56),(Some(VK_LSHIFT),0x53),(Some(VK_LCONTROL),0x44),
            (Some(VK_LCONTROL),VK_LEFT),(Some(VK_LCONTROL),VK_RIGHT),(None,VK_LEFT),(None,VK_RIGHT),
            (None,VK_UP),(None,VK_DOWN),(Some(VK_LSHIFT),VK_LEFT),(Some(VK_LSHIFT),VK_RIGHT),(None,VK_SPACE),
        ]{
            let mut keys=Keys::default();
            assert!(!key(&mut keys,VK_LWIN,true));
            if let Some(m)=mods{assert!(!key(&mut keys,m,true));}
            assert!(!key(&mut keys,other,true));
            assert!(!key(&mut keys,other,false));
            assert!(!key(&mut keys,VK_LWIN,false));
        }
    }
    #[test] fn modifiers_overlap_mouse_focus_and_lifecycle_do_not_create_taps(){
        let mut k=Keys::default();key(&mut k,VK_LCONTROL,true);
        key(&mut k,VK_LWIN,true);key(&mut k,VK_LCONTROL,false);assert!(!key(&mut k,VK_LWIN,false));
        key(&mut k,VK_LWIN,true);key(&mut k,VK_RWIN,true);
        assert!(!key(&mut k,VK_LWIN,false));assert!(!key(&mut k,VK_RWIN,false));
        key(&mut k,VK_RWIN,true);k.cancel();assert!(!key(&mut k,VK_RWIN,false));
        key(&mut k,VK_RWIN,true);assert!(!k.edge(u32::from(VK_RWIN),false,false,88));
        key(&mut k,VK_LWIN,true);k=Keys::default();assert!(!key(&mut k,VK_LWIN,false));
        key(&mut k,VK_LWIN,true);k.cancel();assert!(!key(&mut k,VK_LWIN,false));
    }
    #[test] fn injected_keys_never_arm_or_clear_physical_state(){
        let mut k=Keys::default();
        assert!(!k.edge(0x5b,true,true,11));assert!(!k.edge(0x5b,false,true,11));
        key(&mut k,VK_LWIN,true);
        assert!(!k.edge(0xe8,true,true,11));assert!(!k.edge(0x5b,false,true,11));
        assert!(key(&mut k,VK_LWIN,false));
    }
}
