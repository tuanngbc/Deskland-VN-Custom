//! Trial-only cooperating notification app. This protocol does not read the
//! Explorer tray and is never started by the regular app/runtime.
use crate::desktop::wide;
use std::{
    cell::Cell, io::{self, Read, Write}, mem, os::windows::process::CommandExt, path::Path,
    process::{Child, ChildStdin, Command, Stdio}, ptr, sync::mpsc, thread, time::Duration,
};
use windows_sys::Win32::{Foundation::*, Graphics::Gdi::{COLOR_WINDOW,UpdateWindow}, System::LibraryLoader::GetModuleHandleW, UI::{Shell::*, WindowsAndMessaging::*}};

const VERSION:u8=1;
const ADD:u8=1;
const UPDATE:u8=2;
const LEFT:u8=3;
const RIGHT:u8=4;
const DOUBLE:u8=5;
const REMOVE:u8=6;
const EXIT:u8=7;
const OPEN_MENU:u8=8;
const SHOW_PEER_MENU:u32=WM_APP+172;
const KEY_SELECT:u32=NIN_SELECT|1; // NINF_KEY from ShellAPI.h
const SHELL_CALLBACK:u32=WM_APP+170;
const FIXTURE_CALLBACK:u32=WM_APP+171;
const FIXTURE_ICON:u32=1;
const NO_WINDOW:u32=0x0800_0000;

#[derive(Clone,Copy,Default)]
struct ActionState{left:u8,menu_items:u8,double:u8}
thread_local!{static ACTIONS:Cell<ActionState>=const{Cell::new(ActionState{left:0,menu_items:0,double:0})};}

unsafe fn app_menu()->HMENU{
    let menu=CreatePopupMenu();
    if !menu.is_null(){
        AppendMenuW(menu,MF_STRING,1,wide("Reload fixture").as_ptr());
        AppendMenuW(menu,MF_STRING,2,wide("Exit fixture").as_ptr());
    }
    menu
}
unsafe extern "system" fn peer_window(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
    let code=match msg{
        SHOW_PEER_MENU=>WM_CONTEXTMENU,
        FIXTURE_CALLBACK if w as u32==FIXTURE_ICON=>l as u32,
        SHELL_CALLBACK if (l as u32>>16)==FIXTURE_ICON=>l as u32&0xffff,
        _=>return DefWindowProcW(hwnd,msg,w,l),
    };
    ACTIONS.with(|a|{
        let mut state=a.get();
        match code{
            WM_LBUTTONUP|NIN_SELECT|KEY_SELECT=>state.left=state.left.saturating_add(1),
            WM_LBUTTONDBLCLK=>state.double=state.double.saturating_add(1),
            WM_CONTEXTMENU=>{
                let menu=app_menu();
                if !menu.is_null(){
                    state.menu_items=GetMenuItemCount(menu).max(0) as u8;
                    if msg==SHELL_CALLBACK||msg==SHOW_PEER_MENU{
                        let packed=if msg==SHELL_CALLBACK{w}else{l as usize};
                        let x=packed as i16 as i32;let y=(packed>>16) as i16 as i32;
                        SetForegroundWindow(hwnd);
                        let choice=TrackPopupMenu(menu,TPM_RIGHTBUTTON|TPM_RETURNCMD,x,y,0,hwnd,ptr::null());
                        if choice==1{PostMessageW(hwnd,FIXTURE_CALLBACK,FIXTURE_ICON as usize,WM_LBUTTONUP as isize);}
                        if choice==2{PostQuitMessage(0);}
                        PostMessageW(hwnd,WM_NULL,0,0);
                    }
                    DestroyMenu(menu);
                }
            }
            _=>{}
        }
        a.set(state);
    });
    0
}

struct Icon {
    hwnd:HWND,
    registered:bool,
    allow_shell:bool,
    revision:u8,
}
impl Icon {
    unsafe fn new(allow_shell:bool)->io::Result<Self>{
        let class=wide("TuanpcDeskTrayPocOwnedWindow");
        let hinstance=GetModuleHandleW(ptr::null());
        let wc=WNDCLASSW{lpfnWndProc:Some(peer_window),hInstance:hinstance,lpszClassName:class.as_ptr(),..mem::zeroed()};
        if RegisterClassW(&wc)==0&&GetLastError()!=ERROR_CLASS_ALREADY_EXISTS{return Err(io::Error::last_os_error());}
        let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW,class.as_ptr(),ptr::null(),WS_POPUP,0,0,1,1,ptr::null_mut(),ptr::null_mut(),hinstance,ptr::null());
        if hwnd.is_null(){return Err(io::Error::last_os_error());}
        Ok(Self{hwnd,registered:false,allow_shell,revision:0})
    }
    unsafe fn data(&self)->NOTIFYICONDATAW{
        let mut data:NOTIFYICONDATAW=mem::zeroed();
        data.cbSize=mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd=self.hwnd;data.uID=FIXTURE_ICON;
        data.uFlags=NIF_ICON|NIF_MESSAGE|NIF_TIP|NIF_SHOWTIP;
        data.uCallbackMessage=SHELL_CALLBACK;
        let (icon,tooltip)=if self.revision>=2{(IDI_INFORMATION,"Deskland trial tray: updated")}else{(IDI_APPLICATION,"Deskland trial tray: ready")};
        data.hIcon=LoadIconW(ptr::null_mut(),icon);
        for (d,s) in data.szTip.iter_mut().zip(tooltip.encode_utf16()){*d=s;}
        data
    }
    unsafe fn act(&mut self,op:u8,point:i32)->io::Result<[u8;6]>{
        match op{
            ADD if self.revision==0=>{
                self.revision=1;
                self.registered=self.allow_shell&&Shell_NotifyIconW(NIM_ADD,&self.data())!=0;
                if self.registered{
                    let mut data=self.data();
                    data.Anonymous.uVersion=NOTIFYICON_VERSION_4;
                    Shell_NotifyIconW(NIM_SETVERSION,&data);
                }
            }
            UPDATE if self.revision>0=>{
                self.revision=2;
                if self.registered&&Shell_NotifyIconW(NIM_MODIFY,&self.data())==0{return Err(io::Error::other("Shell refused the fixture update"));}
            }
            LEFT|RIGHT|DOUBLE if self.revision>0=>{
                let code=match op{LEFT=>WM_LBUTTONUP,RIGHT=>WM_CONTEXTMENU,_=>WM_LBUTTONDBLCLK};
                // The cooperative app dispatches to its own callback. Never
                // send messages to owner HWNDs supplied by another process.
                SendMessageW(self.hwnd,FIXTURE_CALLBACK,FIXTURE_ICON as usize,code as isize);
            }
            OPEN_MENU if self.revision>0=>{PostMessageW(self.hwnd,SHOW_PEER_MENU,0,point as isize);}
            REMOVE|EXIT=>self.remove(),
            _=>return Err(io::Error::other("Unexpected tray fixture operation")),
        }
        let a=ACTIONS.with(Cell::get);
        Ok([VERSION,self.revision,u8::from(self.registered),a.left,a.menu_items,a.double])
    }
    unsafe fn remove(&mut self){
        if self.registered{Shell_NotifyIconW(NIM_DELETE,&self.data());self.registered=false;}
        self.revision=0;
    }
}
impl Drop for Icon{fn drop(&mut self){unsafe{self.remove();DestroyWindow(self.hwnd);}}}

fn peer(allow_shell:bool)->io::Result<()>{
    // Inherited stdio connects only to the spawned child. No named endpoint,
    // client-selected executable, or external window handle crosses the pipe.
    let (tx,rx)=mpsc::sync_channel(1);
    thread::spawn(move||{
        let mut input=io::stdin().lock();
        let mut frame=[0u8;6];
        while input.read_exact(&mut frame).is_ok(){
            let point=i32::from_le_bytes([frame[2],frame[3],frame[4],frame[5]]);
            if frame[0]!=VERSION||!(ADD..=OPEN_MENU).contains(&frame[1])||tx.send((frame[1],point)).is_err(){break;}
        }
    });
    let mut icon=unsafe{Icon::new(allow_shell)?};
    let mut output=io::stdout().lock();
    loop{
        let op=match rx.recv_timeout(Duration::from_millis(18)){
            Ok(op)=>Some(op), Err(mpsc::RecvTimeoutError::Timeout)=>None,
            Err(mpsc::RecvTimeoutError::Disconnected)=>break,
        };
        unsafe{
            let mut message:MSG=mem::zeroed();
            while PeekMessageW(&mut message,ptr::null_mut(),0,0,PM_REMOVE)!=0{
                if message.message==WM_QUIT{return Ok(());}
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            if let Some((op,point))=op{
                output.write_all(&icon.act(op,point)?)?;output.flush()?;
                if op==EXIT{break;}
            }
        }
    }
    Ok(())
}
struct Bridge{child:Child,write:ChildStdin,answers:mpsc::Receiver<[u8;6]>}
impl Bridge{
    fn start(dir:&Path,allow_shell:bool)->io::Result<Self>{
        let mut command=Command::new(std::env::current_exe()?);
        command.arg("--trial-dir").arg(dir).arg("--trial-tray-poc-peer");
        if allow_shell{command.arg("--trial-tray-poc-shell");}
        let mut child=command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
            .creation_flags(NO_WINDOW).spawn()?;
        let mut read=child.stdout.take().expect("piped stdout");
        let write=child.stdin.take().expect("piped stdin");
        let (tx,answers)=mpsc::sync_channel(1);
        thread::spawn(move||{
            let mut frame=[0;6];
            while read.read_exact(&mut frame).is_ok()&&tx.send(frame).is_ok(){}
        });
        Ok(Self{child,write,answers})
    }
    fn call(&mut self,op:u8)->io::Result<[u8;6]>{
        self.at(op,0)
    }
    fn at(&mut self,op:u8,point:i32)->io::Result<[u8;6]>{
        let [a,b,c,d]=point.to_le_bytes();
        self.write.write_all(&[VERSION,op,a,b,c,d])?;
        let answer=self.answers.recv_timeout(Duration::from_secs(3)).map_err(io::Error::other)?;
        if answer[0]!=VERSION||answer[1]>2||answer[2]>1 || self.child.try_wait()?.is_some_and(|status|op!=EXIT||!status.success()){
            return Err(io::Error::other("Invalid tray fixture response"));
        }
        Ok(answer)
    }
}
impl Drop for Bridge{
    fn drop(&mut self){
        let start=std::time::Instant::now();
        while matches!(self.child.try_wait(),Ok(None))&&start.elapsed()<Duration::from_millis(300){
            thread::sleep(Duration::from_millis(15));
        }
        if matches!(self.child.try_wait(),Ok(None)){
            // Only the exact child we spawned. Never stop Explorer or an app.
            let _=self.child.kill();
        }
        let _=self.child.wait();
    }
}

fn run(dir:&Path,allow_shell:bool)->io::Result<()>{
    let mut bridge=Bridge::start(dir,allow_shell)?;
    let initial=bridge.call(ADD)?;
    let updated=bridge.call(UPDATE)?;
    let left=bridge.call(LEFT)?;
    let right=bridge.call(RIGHT)?;
    let double=bridge.call(DOUBLE)?;
    let removed=bridge.call(REMOVE)?;
    let _=bridge.call(EXIT)?;
    drop(bridge);
    let mut interrupted=Bridge::start(dir,false)?;
    let _=interrupted.call(ADD)?;
    interrupted.child.kill()?;
    interrupted.child.wait()?;
    let observed_crash=interrupted.call(LEFT).is_err();
    drop(interrupted);
    let mut recovered=Bridge::start(dir,false)?;
    let again=recovered.call(ADD)?;
    let _=recovered.call(EXIT)?;
    if initial[1]!=1||updated[1]!=2||left[3]!=1||right[4]!=2||double[5]!=1||removed[1]!=0||removed[2]!=0||again[1]!=1||!observed_crash{
        return Err(io::Error::other("Tray fixture did not complete its lifecycle"));
    }
    let report=serde_json::json!({
        "protocol":VERSION,"cooperativePeer":true,"added":true,"tooltipRevision":updated[1],"iconRevision":updated[1],
        "leftCallback":left[3],"doubleCallback":double[5],"appOwnedContextMenuItems":right[4],
        "removed":true,"cleanRestart":true,"recoveredAfterPeerExit":observed_crash,"registeredWithExplorer":initial[2]!=0,
        "externalAppEnumeration":false,"interactiveRightClickForwarding":false
    });
    std::fs::write(dir.join("cooperative-tray-poc.json"),report.to_string())
}

struct Demo{bridge:Bridge,revision:u8,status:String}
unsafe fn peer_icon(hwnd:HWND,state:&Demo){
    let id=if state.revision>=2{IDI_INFORMATION}else{IDI_APPLICATION};
    let icon=if state.revision==0{ptr::null_mut()}else{LoadIconW(ptr::null_mut(),id)};
    SendMessageW(GetDlgItem(hwnd,5),0x0170,icon as usize,0); // STM_SETICON
    SetWindowTextW(GetDlgItem(hwnd,6),wide(&state.status).as_ptr());
}
unsafe extern "system" fn demo_window(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
    let state=GetWindowLongPtrW(hwnd,GWLP_USERDATA) as *mut Demo;
    match msg{
        WM_NCCREATE=>{
            let create=&*(l as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd,GWLP_USERDATA,create.lpCreateParams as isize);
            DefWindowProcW(hwnd,msg,w,l)
        }
        WM_COMMAND if !state.is_null()=>{
            let op=match w&0xffff{1=>ADD,2=>UPDATE,3=>REMOVE,4=>DOUBLE,5=>LEFT,_=>return 0};
            let state=&mut *state;
            let result=state.bridge.call(op);
            reflect_demo(hwnd,state,result);0
        }
        WM_CONTEXTMENU if !state.is_null()=>{
            let state=&mut *state;
            if state.revision!=0&&w as HWND==GetDlgItem(hwnd,5){
                let mut point=POINT{x:0,y:0};GetCursorPos(&mut point);
                AllowSetForegroundWindow(state.bridge.child.id());
                let packed=((point.y as u16 as u32)<<16)|(point.x as u16 as u32);
                let result=state.bridge.at(OPEN_MENU,packed as i32);
                reflect_demo(hwnd,state,result);
            }
            0
        }
        WM_DESTROY=>{PostQuitMessage(0);0}
        _=>DefWindowProcW(hwnd,msg,w,l),
    }
}
unsafe fn reflect_demo(hwnd:HWND,state:&mut Demo,result:io::Result<[u8;6]>){
    state.status=match result{
        Ok(r)=>{
            state.revision=r[1];
            let tooltip=match r[1]{0=>"Đã ẩn icon",1=>"Tooltip: Deskland trial tray: ready",_=>"Tooltip: Deskland trial tray: updated"};
            format!("{tooltip} · click trái: {} · double: {}",r[3],r[5])
        }
        Err(_)=>"Ứng dụng mẫu đã đóng hoặc không nhận thao tác này.".into(),
    };
    peer_icon(hwnd,state);
}
unsafe fn demo(dir:&Path)->io::Result<()>{
    let mut state=Box::new(Demo{bridge:Bridge::start(dir,false)?,revision:0,status:"Chưa thêm icon".into()});
    let hinstance=GetModuleHandleW(ptr::null());
    let class=wide("TuanpcDeskTrialTrayPanel");
    let wc=WNDCLASSW{lpfnWndProc:Some(demo_window),hInstance:hinstance,lpszClassName:class.as_ptr(),
        hbrBackground:(COLOR_WINDOW+1) as _,hCursor:LoadCursorW(ptr::null_mut(),IDC_ARROW),..mem::zeroed()};
    if RegisterClassW(&wc)==0&&GetLastError()!=ERROR_CLASS_ALREADY_EXISTS{return Err(io::Error::last_os_error());}
    let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW,class.as_ptr(),wide("Deskland · Khay thử nghiệm").as_ptr(),
        WS_OVERLAPPED|WS_CAPTION|WS_SYSMENU,CW_USEDEFAULT,CW_USEDEFAULT,540,224,ptr::null_mut(),ptr::null_mut(),hinstance,(&mut *state as *mut Demo).cast());
    if hwnd.is_null(){return Err(io::Error::last_os_error());}
    let ctl=|class:&str,label:&str,id:i32,x,y,w,h,style:u32|{
        CreateWindowExW(0,wide(class).as_ptr(),wide(label).as_ptr(),WS_CHILD|WS_VISIBLE|style,x,y,w,h,hwnd,id as HMENU,hinstance,ptr::null())
    };
    ctl("STATIC","Bấm vào icon hoặc bấm chuột phải để mở menu của ứng dụng mẫu.",0,20,15,490,26,0);
    ctl("STATIC","",5,24,51,40,44,0x0103); // SS_ICON | SS_NOTIFY
    ctl("STATIC","",6,75,56,424,52,0);
    for (i,label) in ["Thêm icon","Đổi icon","Ẩn icon","Double-click"].into_iter().enumerate(){
        ctl("BUTTON",label,i as i32+1,20+i as i32*126,126,113,32,WS_TABSTOP);
    }
    let initial=state.bridge.call(ADD);
    reflect_demo(hwnd,&mut state,initial);
    ShowWindow(hwnd,SW_SHOW);UpdateWindow(hwnd);
    let mut msg:MSG=mem::zeroed();
    while GetMessageW(&mut msg,ptr::null_mut(),0,0)>0{TranslateMessage(&msg);DispatchMessageW(&msg);}
    let _=state.bridge.call(EXIT);
    Ok(())
}

pub(crate) fn dispatch(args:&[String])->Option<i32>{
    let is_peer=args.iter().any(|a|a=="--trial-tray-poc-peer");
    let demo_requested=args.iter().any(|a|a=="--trial-tray-poc-demo");
    if !is_peer&&!demo_requested&&!args.iter().any(|a|a=="--trial-tray-poc"){return None;}
    let Some(dir)=crate::trial::data_dir()else{return Some(2);};
    let allow_shell=args.iter().any(|a|a=="--trial-tray-poc-shell")
        &&std::env::var("GITHUB_ACTIONS").as_deref()==Ok("true");
    let result=if is_peer{peer(allow_shell)}else if demo_requested{unsafe{demo(dir)}}else{run(dir,allow_shell)};
    Some(match result{Ok(())=>0,Err(error)=>{eprintln!("Cooperative tray fixture: {error}");3}})
}
