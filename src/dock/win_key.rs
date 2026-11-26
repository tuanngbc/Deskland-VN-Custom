//! Keep standalone physical Win taps out of Explorer. Defer the first Win
//! down until a real chord arrives; an isolated tap never injects any input.
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
const REPLAY_TAG:usize=0x44534b48;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
struct Stroke{vk:u16,scan:u16,extended:bool}
impl Stroke{
    fn down(self)->INPUT{
        INPUT{r#type:INPUT_KEYBOARD,Anonymous:INPUT_0{ki:KEYBDINPUT{
            wVk:self.vk,wScan:self.scan,dwFlags:if self.extended{KEYEVENTF_EXTENDEDKEY}else{0},
            time:0,dwExtraInfo:REPLAY_TAG,
        }}}
    }
    fn win_index(self)->Option<usize>{match self.vk{VK_LWIN=>Some(0),VK_RWIN=>Some(1),_=>None}}
}

#[derive(Debug,PartialEq,Eq)]
enum Edge{Pass,Hold,Tap,Replay(usize,Stroke)}

#[derive(Clone,Copy,Default)]
struct Keys {
    blocked:[Option<Stroke>;2],
    tap:Option<(usize,usize)>,
}

impl Keys {
    fn edge(&mut self,key:Stroke,down:bool,foreground:usize,win_already_down:bool,other_held:bool)->Edge{
        if let Some(i)=key.win_index(){
            if !down{
                if self.blocked[i].take().is_none(){return Edge::Pass;}
                return if self.tap.take()==Some((i,foreground)){Edge::Tap}else{Edge::Hold};
            }
            if self.blocked[i].is_some(){return Edge::Hold;}
            if win_already_down{return Edge::Pass;}
            let second=self.blocked.iter().any(Option::is_some);
            self.tap=(!second&&!other_held).then_some((i,foreground));
            if other_held&&!second{return Edge::Pass;}
            self.blocked[i]=Some(key);
            return Edge::Hold;
        }
        self.cancel();
        if down{
            if let Some((i,win))=self.blocked.iter().enumerate().find_map(|(i,&win)|win.map(|k|(i,k))){
                return Edge::Replay(i,win);
            }
        }
        Edge::Pass
    }
    fn cancel(&mut self){self.tap=None;}
    fn replayed(&mut self,i:usize,sent:u32){
        // If a Win down was queued, leave its actual release to Windows.
        if sent>=1{self.blocked[i]=None;}
    }
}

thread_local! {
    static KEYS:Cell<Keys>=Cell::new(Keys::default());
    static TARGET:Cell<(usize,usize)>=const{Cell::new((0,0))};
    static REPLAY_FAILED:Cell<bool>=const{Cell::new(false)};
    static MODE:Cell<(bool,bool)>=const{Cell::new((false,false))};
    static SHELL_CLICK:Cell<Option<super::shell_menu::Click>>=const{Cell::new(None)};
}

struct Quit(usize);
impl Drop for Quit{fn drop(&mut self){unsafe{CloseHandle(self.0 as HANDLE);}}}

pub(super) struct Hook { quit:Arc<Quit>, thread:Option<JoinHandle<()>>, pub ticket:usize, pub mode:(bool,bool) }
impl Hook {
    pub fn install(target:HWND,ticket:usize)->Result<Self,String>{
        Self::install_mode(target,ticket,true,false)
    }
    pub fn install_mode(target:HWND,ticket:usize,win_tap:bool,shell_menu:bool)->Result<Self,String>{
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
            KEYS.set(Keys::default());TARGET.set((target,ticket));MODE.set((win_tap,shell_menu));
            REPLAY_FAILED.set(false);SHELL_CLICK.set(None);
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
            Ok(Ok(()))=>Ok(Self{quit,thread:Some(thread),ticket,mode:(win_tap,shell_menu)}),
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
        let (target,ticket)=TARGET.get();
        if target!=0&&MODE.get().0&&!REPLAY_FAILED.get(){
            let mut keys=KEYS.get();
            let win=matches!(e.vkCode as u16,VK_LWIN|VK_RWIN);
            // Ordinary typing, including injected IME events, should not
            // touch the Win-key state if no physical Win key is pending.
            if !win&&keys.blocked.iter().all(Option::is_none){
                return CallNextHookEx(null_mut(),code,w,l);
            }
            if injected{
                if e.dwExtraInfo!=REPLAY_TAG{keys.cancel();KEYS.set(keys);}
                return CallNextHookEx(null_mut(),code,w,l);
            }
            let key=Stroke{vk:e.vkCode as u16,scan:e.scanCode as u16,extended:e.flags&LLKHF_EXTENDED!=0};
            let down=matches!(w as u32,WM_KEYDOWN|WM_SYSKEYDOWN);
            let foreground=if win{GetForegroundWindow() as usize}else{0};
            let repeat=win&&down&&GetAsyncKeyState(e.vkCode as i32)<0;
            let modifier_held=win&&down&&!repeat&&
                [VK_LWIN,VK_RWIN,VK_MENU,VK_CONTROL,VK_SHIFT,VK_LBUTTON,VK_RBUTTON,VK_MBUTTON,VK_XBUTTON1,VK_XBUTTON2]
                    .into_iter().any(|vk|u32::from(vk)!=e.vkCode&&GetAsyncKeyState(i32::from(vk))<0);
            let edge=keys.edge(key,down,foreground,repeat,modifier_held);
            KEYS.set(keys);
            match edge{
                Edge::Hold=>return 1,
                Edge::Tap=>{
                    PostMessageW(target as HWND,NOTICE,ticket,encode_tap(cursor_now()));
                    return 1;
                }
                Edge::Replay(i,win)=>{
                    // Queue both downs in order. The original up events still
                    // come from the keyboard and are never synthesized.
                    let input=[win.down(),key.down()];
                    let sent=SendInput(input.len() as u32,input.as_ptr(),size_of::<INPUT>() as i32);
                    keys.replayed(i,sent);KEYS.set(keys);
                    if sent==2{return 1;}
                    REPLAY_FAILED.set(true);
                    PostMessageW(target as HWND,NOTICE,ticket,MASK_ERROR);
                }
                Edge::Pass=>{}
            }
        }
    }
    CallNextHookEx(null_mut(),code,w,l)
}
unsafe extern "system" fn mouse(code:i32,w:WPARAM,l:LPARAM)->LRESULT{
    if code==HC_ACTION as i32&&l!=0&&MODE.get().1 {
        let event=&*(l as *const MSLLHOOKSTRUCT);
        if event.flags&LLMHF_INJECTED==0 {
            match w as u32 {
                WM_RBUTTONDOWN=>SHELL_CLICK.set(super::shell_menu::Click::begin(event.pt)),
                WM_RBUTTONUP=>{
                    if let Some(click)=SHELL_CLICK.take(){
                        let (target,ticket)=TARGET.get();
                        // Down already selected the actual Shell item. Only consume
                        // up after a bounded request is successfully queued.
                        if target!=0&&click.queue(event.pt,target as HWND,ticket){return 1;}
                    }
                }
                WM_LBUTTONDOWN|WM_MBUTTONDOWN|WM_XBUTTONDOWN=>SHELL_CLICK.set(None),
                _=>{}
            }
        }
    }
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
    fn stroke(vk:u16)->Stroke{Stroke{vk,scan:0,extended:matches!(vk,VK_LWIN|VK_RWIN|VK_UP|VK_DOWN|VK_LEFT|VK_RIGHT)}}
    fn key(k:&mut Keys,v:u16,d:bool)->Edge{k.edge(stroke(v),d,11,false,false)}

    #[test] fn either_win_tap_and_repeats_never_reach_windows(){
        let mut keys=Keys::default();
        for win in [VK_LWIN,VK_RWIN,VK_LWIN]{
            for _ in 0..20{assert_eq!(key(&mut keys,win,true),Edge::Hold);}
            assert_eq!(key(&mut keys,win,false),Edge::Tap);
            assert_eq!(key(&mut keys,win,false),Edge::Pass);
        }
    }
    #[test] fn physical_space_switches_language_only_if_actually_pressed_with_win(){
        let mut k=Keys::default();
        assert_eq!(key(&mut k,VK_LWIN,true),Edge::Hold);
        assert_eq!(key(&mut k,VK_LWIN,false),Edge::Tap);
        assert_eq!(key(&mut k,VK_SPACE,true),Edge::Pass);
        assert_eq!(key(&mut k,VK_SPACE,false),Edge::Pass);

        for win in [VK_LWIN,VK_RWIN]{
            assert_eq!(key(&mut k,win,true),Edge::Hold);
            assert_eq!(key(&mut k,VK_SPACE,true),Edge::Replay(stroke(win).win_index().unwrap(),stroke(win)));
            let batch=[stroke(win).down(),stroke(VK_SPACE).down()].map(|input|unsafe{input.Anonymous.ki});
            assert_eq!(batch.map(|k|(k.wVk,k.dwFlags,k.dwExtraInfo)),
                [(win,KEYEVENTF_EXTENDEDKEY,REPLAY_TAG),(VK_SPACE,0,REPLAY_TAG)]);
            k.replayed(stroke(win).win_index().unwrap(),2);
            assert_eq!(key(&mut k,VK_SPACE,false),Edge::Pass);
            assert_eq!(key(&mut k,win,false),Edge::Pass);
        }
    }
    #[test] fn shell_chords_replay_once_and_never_inject_key_up(){
        for (modifier,chord) in [(None,b'E' as u16),(None,b'R' as u16),(None,b'D' as u16),(None,VK_UP),
            (Some(VK_LSHIFT),b'S' as u16),(Some(VK_LCONTROL),VK_LEFT)]{
            let mut k=Keys::default();
            assert_eq!(key(&mut k,VK_LWIN,true),Edge::Hold);
            let first=modifier.unwrap_or(chord);
            assert_eq!(key(&mut k,first,true),Edge::Replay(0,stroke(VK_LWIN)));
            k.replayed(0,2);
            if modifier.is_some(){assert_eq!(key(&mut k,chord,true),Edge::Pass);}
            assert_eq!(key(&mut k,chord,false),Edge::Pass);
            assert_eq!(key(&mut k,VK_LWIN,false),Edge::Pass);
        }
    }
    #[test] fn partial_replay_only_passes_the_release_if_win_down_was_sent(){
        for sent in [0,1]{
            let mut k=Keys::default();
            key(&mut k,VK_RWIN,true);key(&mut k,VK_SPACE,true);
            k.replayed(1,sent);
            assert_eq!(key(&mut k,VK_RWIN,false),if sent==0{Edge::Hold}else{Edge::Pass});
        }
    }
    #[test] fn overlap_mouse_focus_and_lifecycle_do_not_create_taps(){
        let mut k=Keys::default();
        assert_eq!(k.edge(stroke(VK_LWIN),true,11,false,true),Edge::Pass);
        assert_eq!(key(&mut k,VK_LWIN,false),Edge::Pass);
        assert_eq!(key(&mut k,VK_LWIN,true),Edge::Hold);
        assert_eq!(key(&mut k,VK_RWIN,true),Edge::Hold);
        assert_eq!(key(&mut k,VK_LWIN,false),Edge::Hold);
        assert_eq!(key(&mut k,VK_RWIN,false),Edge::Hold);
        assert_eq!(key(&mut k,VK_RWIN,true),Edge::Hold);
        k.cancel();assert_eq!(key(&mut k,VK_RWIN,false),Edge::Hold);
        assert_eq!(key(&mut k,VK_RWIN,true),Edge::Hold);
        assert_eq!(k.edge(stroke(VK_RWIN),false,88,false,false),Edge::Hold);
        key(&mut k,VK_LWIN,true);k=Keys::default();
        assert_eq!(key(&mut k,VK_LWIN,false),Edge::Pass);
    }
}
