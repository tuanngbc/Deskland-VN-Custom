//! Opt-in Win tap detection. Both Win edges and all physical chords pass
//! through unchanged. An inert mask at keydown prevents native Start.
use std::{cell::Cell, mem::{size_of, zeroed}, ptr::null_mut, sync::{Arc,mpsc}, thread::JoinHandle};
use windows_sys::Win32::{Foundation::*, System::{LibraryLoader::GetModuleHandleW,Threading::{CreateEventW,SetEvent,INFINITE}},
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*}};

pub(super) const NOTICE:u32=WM_APP+57;
// The x64 UI message carries full signed desktop coordinates. Reserve
// values outside the desktop range for a missing pointer and mask failure.
const TAP_WITHOUT_CURSOR:isize=isize::MIN;
const MASK_ERROR:isize=isize::MAX;
pub(super) fn cursor_now()->Option<POINT>{
    let mut point=POINT{x:0,y:0};
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
const MASK_TAG:usize=0x44534b48;

fn menu_mask()->[INPUT;2]{
    let keyboard=|flags|INPUT{r#type:INPUT_KEYBOARD,Anonymous:INPUT_0{
        ki:KEYBDINPUT{wVk:MENU_MASK,wScan:0,dwFlags:flags,time:0,dwExtraInfo:MASK_TAG},
    }};
    [keyboard(0),keyboard(KEYEVENTF_KEYUP)]
}

#[derive(Debug,PartialEq,Eq)]
enum Edge{Pass,Mask,Tap}

#[derive(Clone,Copy,Default)]
struct Keys {
    candidate:Option<(u32,usize)>,
}

impl Keys {
    fn edge(&mut self,key:u32,down:bool,own_mask:bool,foreground:usize,repeat:bool,modifier_held:bool,injected:bool)->Edge{
        if own_mask{return Edge::Pass;}
        let win=key==u32::from(VK_LWIN)||key==u32::from(VK_RWIN);
        // A key from another IME or accessibility tool can form a chord, but
        // can never arm Deskland Start.
        if !win||injected{self.cancel();return Edge::Pass;}
        if down {
            if repeat{return Edge::Pass;}
            self.candidate=(!modifier_held).then_some((key,foreground));
            if self.candidate.is_some(){Edge::Mask}else{Edge::Pass}
        }else if self.candidate.take()==Some((key,foreground)){
            Edge::Tap
        }else{
            Edge::Pass
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
            KEYS.set(Keys::default());TARGET.set((target,ticket));
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
        let (target,ticket)=TARGET.get();
        if target!=0&&!MASK_FAILED.get(){
            let own_mask=injected&&e.vkCode==u32::from(MENU_MASK)&&e.dwExtraInfo==MASK_TAG;
            let down=matches!(w as u32,WM_KEYDOWN|WM_SYSKEYDOWN);
            let physical_win=win&&!injected;
            let foreground=if physical_win{GetForegroundWindow() as usize}else{0};
            // LL runs before Windows updates async state for this event. At
            // Win-down the old state distinguishes repeat from the first edge.
            let repeat=physical_win&&down&&GetAsyncKeyState(e.vkCode as i32)<0;
            let modifier_held=physical_win&&down&&!repeat&&
                [VK_LWIN,VK_RWIN,VK_MENU,VK_CONTROL,VK_SHIFT,VK_LBUTTON,VK_RBUTTON,VK_MBUTTON,VK_XBUTTON1,VK_XBUTTON2]
                    .into_iter().any(|vk|u32::from(vk)!=e.vkCode&&GetAsyncKeyState(i32::from(vk))<0);
            let mut keys=KEYS.get();
            let edge=keys.edge(e.vkCode,down,own_mask,foreground,repeat,modifier_held,injected);
            KEYS.set(keys);
            if edge==Edge::Tap{
                PostMessageW(target as HWND,NOTICE,ticket,encode_tap(cursor_now()));
            }else if edge==Edge::Mask{
                // Queue the mask while processing physical down rather than
                // racing the release. The keyboard stack still owns Win-up,
                // even if this hook or a third-party hook stops mid-gesture.
                let mask=menu_mask();
                let size=size_of::<INPUT>() as i32;
                let sent=SendInput(mask.len() as u32,mask.as_ptr(),size);
                if sent!=mask.len() as u32{
                    if sent==1{SendInput(1,mask[1..].as_ptr(),size);}
                    keys.cancel();KEYS.set(keys);
                    MASK_FAILED.set(true);
                    PostMessageW(target as HWND,NOTICE,ticket,MASK_ERROR);
                }
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
    #[test] fn mask_never_synthesizes_a_modifier_or_a_language_switch(){
        let sequence=menu_mask().map(|e|unsafe{
            let k=e.Anonymous.ki;(e.r#type,k.wVk,k.wScan,k.dwFlags,k.dwExtraInfo)
        });
        assert_eq!(sequence,[(INPUT_KEYBOARD,MENU_MASK,0,0,MASK_TAG),
            (INPUT_KEYBOARD,MENU_MASK,0,KEYEVENTF_KEYUP,MASK_TAG)]);
    }
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
    fn key(k:&mut Keys,v:u16,d:bool)->Edge{k.edge(u32::from(v),d,false,11,false,false,false)}
    #[test] fn either_win_and_repeats_toggle_once_per_release(){
        let mut keys=Keys::default();
        for win in [VK_LWIN,VK_RWIN,VK_LWIN]{
            assert_eq!(key(&mut keys,win,true),Edge::Mask);
            for _ in 0..20{assert_eq!(keys.edge(u32::from(win),true,false,11,true,false,false),Edge::Pass);}
            assert_eq!(key(&mut keys,win,false),Edge::Tap);
            assert_eq!(key(&mut keys,win,false),Edge::Pass);
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
            assert_eq!(key(&mut keys,VK_LWIN,true),Edge::Mask);
            if let Some(m)=mods{assert_eq!(key(&mut keys,m,true),Edge::Pass);}
            assert_eq!(key(&mut keys,other,true),Edge::Pass);
            assert_eq!(key(&mut keys,other,false),Edge::Pass);
            assert_eq!(key(&mut keys,VK_LWIN,false),Edge::Pass);
        }
    }
    #[test] fn modifiers_overlap_mouse_focus_and_lifecycle_do_not_create_taps(){
        let mut k=Keys::default();
        assert_eq!(k.edge(u32::from(VK_LWIN),true,false,11,false,true,false),Edge::Pass);
        key(&mut k,VK_LCONTROL,false);assert_eq!(key(&mut k,VK_LWIN,false),Edge::Pass);
        key(&mut k,VK_LWIN,true);
        assert_eq!(k.edge(u32::from(VK_RWIN),true,false,11,false,true,false),Edge::Pass);
        assert_eq!(key(&mut k,VK_LWIN,false),Edge::Pass);assert_eq!(key(&mut k,VK_RWIN,false),Edge::Pass);
        assert_eq!(key(&mut k,VK_RWIN,true),Edge::Mask);
        k.cancel();assert_eq!(key(&mut k,VK_RWIN,false),Edge::Pass);
        assert_eq!(key(&mut k,VK_RWIN,true),Edge::Mask);
        assert_eq!(k.edge(u32::from(VK_RWIN),false,false,88,false,false,false),Edge::Pass);
        key(&mut k,VK_LWIN,true);k=Keys::default();assert_eq!(key(&mut k,VK_LWIN,false),Edge::Pass);
    }
    #[test] fn own_mask_keeps_the_tap_but_other_injected_keys_cannot_arm_one(){
        let mut k=Keys::default();
        assert_eq!(k.edge(0x5b,true,false,11,false,false,true),Edge::Pass);
        assert_eq!(k.edge(0x5b,false,false,11,false,false,true),Edge::Pass);
        key(&mut k,VK_LWIN,true);
        assert_eq!(k.edge(0xe8,true,true,11,false,false,true),Edge::Pass);
        assert_eq!(key(&mut k,VK_LWIN,false),Edge::Tap);
        key(&mut k,VK_LWIN,true);
        assert_eq!(k.edge(u32::from(VK_SPACE),true,false,11,false,false,true),Edge::Pass);
        assert_eq!(key(&mut k,VK_LWIN,false),Edge::Pass);
    }
}
