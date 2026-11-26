//! One on-demand worker for original FluidWin media metadata, art and commands.
use super::{panel_chrome as chrome,utilities};
use crate::desktop::wide;
use std::{cell::{Cell,RefCell},ffi::c_void,ptr,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering},mpsc},time::Duration};
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::*,System::LibraryLoader::GetModuleHandleW,
    UI::{Controls::*,Input::KeyboardAndMouse::*,WindowsAndMessaging::*}};
const READY:u32=WM_APP+142;const TIMER:usize=1;
const ART:i32=5400;const TITLE:i32=5401;const ARTIST:i32=5402;const PREV:i32=5403;const PLAY:i32=5404;const NEXT:i32=5405;
const STATIC_OWNER_DRAW:u32=0x0000000d; // SS_OWNERDRAW
static WORKER:AtomicBool=AtomicBool::new(false);
thread_local!{static PANEL:RefCell<Option<Panel>>=const{RefCell::new(None)};static SERIAL:Cell<usize>=const{Cell::new(0)};}
#[cfg(target_env="msvc")]
extern "C"{
    fn tlauncher_media_create()->*mut c_void;fn tlauncher_media_free(raw:*mut c_void);
    fn tlauncher_media_read(raw:*mut c_void)->u64;
    fn tlauncher_media_snapshot(raw:*mut c_void,title:*mut u16,artist:*mut u16,identity:*mut u16,capacity:u32,art:*mut u32,pixels:u32)->u32;
    fn tlauncher_media_command(raw:*mut c_void,generation:u64,identity:*const u16,command:i32)->bool;
}
#[derive(Default)]struct Snapshot{generation:u64,title:String,artist:String,identity:String,art:Vec<u32>,flags:u32,failed:bool}
enum Job{Read,Command{generation:u64,identity:String,command:i32}}
struct Worker{tx:mpsc::SyncSender<Job>,closed:Arc<AtomicBool>,answer:Arc<Mutex<Option<Snapshot>>>}
impl Drop for Worker{fn drop(&mut self){self.closed.store(true,Ordering::Release);}}
impl Worker{
    fn start(hwnd:HWND,tag:usize)->Option<Self>{
        if WORKER.swap(true,Ordering::AcqRel){return None;}
        let (tx,rx)=mpsc::sync_channel(1);let closed=Arc::new(AtomicBool::new(false));let answer=Arc::new(Mutex::new(None));
        let (stop,result,h)=(closed.clone(),answer.clone(),hwnd as usize);
        if std::thread::Builder::new().name("TLauncher media controls".into()).spawn(move||{
            struct Permit;impl Drop for Permit{fn drop(&mut self){WORKER.store(false,Ordering::Release);}}let _permit=Permit;
            #[cfg(target_env="msvc")]
            unsafe{
                use windows::Win32::System::WinRT::{RoInitialize,RoUninitialize,RO_INIT_MULTITHREADED};
                if RoInitialize(RO_INIT_MULTITHREADED).is_err(){
                    if let Ok(mut r)=result.lock(){*r=Some(Snapshot{failed:true,..Default::default()});}PostMessageW(h as HWND,READY,tag,0);return;
                }
                struct Apartment;impl Drop for Apartment{fn drop(&mut self){unsafe{RoUninitialize();}}}let _apartment=Apartment;
                struct Model(*mut c_void);impl Drop for Model{fn drop(&mut self){unsafe{tlauncher_media_free(self.0);}}}let model=Model(tlauncher_media_create());
                while !stop.load(Ordering::Acquire){
                    let job=match rx.recv_timeout(Duration::from_millis(200)){Ok(job)=>job,Err(mpsc::RecvTimeoutError::Timeout)=>continue,Err(_)=>break};
                    if stop.load(Ordering::Acquire){break;}
                    let mut view=Snapshot::default();
                    if model.0.is_null(){view.failed=true;}else{
                        if let Job::Command{generation,identity,command}=job{
                            view.failed=!tlauncher_media_command(model.0,generation,wide(&identity).as_ptr(),command);
                        }
                        view.generation=tlauncher_media_read(model.0);view.failed|=view.generation==0;
                        let mut title=[0u16;1024];let mut artist=[0u16;1024];let mut identity=[0u16;1024];view.art.resize(96*96,0);
                        view.flags=tlauncher_media_snapshot(model.0,title.as_mut_ptr(),artist.as_mut_ptr(),identity.as_mut_ptr(),1024,view.art.as_mut_ptr(),view.art.len() as u32);
                        let decode=|s:&[u16]|String::from_utf16_lossy(&s[..s.iter().position(|&c|c==0).unwrap_or(s.len())]);
                        view.title=decode(&title);view.artist=decode(&artist);view.identity=decode(&identity);if view.flags&4==0{view.art.clear();}
                    }
                    if stop.load(Ordering::Acquire){break;}
                    if let Ok(mut r)=result.lock(){*r=Some(view);}PostMessageW(h as HWND,READY,tag,0);
                }
            }
        }).is_err(){WORKER.store(false,Ordering::Release);return None;}
        Some(Self{tx,closed,answer})
    }
}
fn text(vi:&str,en:&str,zh:&str)->String{crate::launcher::localized(vi,en,zh).into()}
struct Panel{hwnd:HWND,owner:HWND,tag:usize,dark:bool,worker:Worker,view:Snapshot,pending:bool}
impl Panel{
    unsafe fn submit(&mut self,job:Job){
        if self.pending||self.worker.tx.try_send(job).is_err(){return;}
        self.pending=true;for id in [PREV,PLAY,NEXT]{EnableWindow(GetDlgItem(self.hwnd,id),0);}
    }
    unsafe fn receive(&mut self){
        let Some(view)=self.worker.answer.lock().ok().and_then(|mut a|a.take())else{return;};
        self.view=view;self.pending=false;
        let title=if self.view.flags&1==0{text("Không có phiên phát đang hoạt động","No active media session","没有活动的媒体会话")}
            else if self.view.title.is_empty(){text("Đang phát nội dung đa phương tiện","Media session","媒体会话")}else{self.view.title.clone()};
        SetWindowTextW(GetDlgItem(self.hwnd,TITLE),wide(&title).as_ptr());SetWindowTextW(GetDlgItem(self.hwnd,ARTIST),wide(&self.view.artist).as_ptr());
        SetWindowTextW(GetDlgItem(self.hwnd,PLAY),wide(&if self.view.flags&2!=0{text("Tạm dừng","Pause","暂停")}else{text("Phát","Play","播放")}).as_ptr());
        for id in [PREV,PLAY,NEXT]{EnableWindow(GetDlgItem(self.hwnd,id),i32::from(self.view.flags&1!=0&&!self.view.identity.is_empty()));}
        InvalidateRect(self.hwnd,ptr::null(),0);InvalidateRect(GetDlgItem(self.hwnd,ART),ptr::null(),0);
    }
}
pub(super) unsafe fn open(owner:HWND,point:POINT){
    utilities::close_panels();let class=wide("TLauncher.MediaControls");if !chrome::register(&class,Some(proc)){return;}
    let anchor=chrome::Anchor::for_owner(owner,point);let Some(work)=anchor.work()else{return;};
    let dpi=windows_sys::Win32::UI::HiDpi::GetDpiForWindow(owner);let px=|n|chrome::scale(dpi,n);let (width,height)=(px(396),px(292));
    if width>work.right-work.left||height>work.bottom-work.top{return;}
    let (x,y)=chrome::placement(work,anchor,width,height,px(8));
    let h=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_TOPMOST|WS_EX_LAYERED,class.as_ptr(),wide(&text("Đang phát","Now playing","正在播放")).as_ptr(),
        WS_POPUP|WS_CLIPCHILDREN,x,y,width,height,owner,ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());if h.is_null(){return;}
    chrome::frame(h,width,height,dpi,anchor);
    let tag=SERIAL.with(|s|{let n=s.get().wrapping_add(1);s.set(n);n});let Some(worker)=Worker::start(h,tag)else{DestroyWindow(h);return;};
    let child=|class:&str,label:&str,id:i32,style:u32,x,y,w,hg|{
        let control=CreateWindowExW(0,wide(class).as_ptr(),wide(label).as_ptr(),WS_CHILD|WS_VISIBLE|style,
            px(x),px(y),px(w),px(hg),h,id as usize as HMENU,GetModuleHandleW(ptr::null()),ptr::null());
        SendMessageW(control,WM_SETFONT,GetStockObject(DEFAULT_GUI_FONT) as usize,0);control
    };
    child("STATIC",&text("Ảnh bìa","Album art","专辑封面"),ART,STATIC_OWNER_DRAW,16,76,112,112);
    child("STATIC",&text("Đang đọc…","Reading…","正在读取…"),TITLE,0,144,76,236,52);child("STATIC","",ARTIST,0,144,136,236,52);
    for (id,label,x) in [(PREV,text("Trước","Previous","上一首"),16),(PLAY,text("Phát","Play","播放"),140),(NEXT,text("Tiếp","Next","下一首"),264)]{
        child("BUTTON",&label,id,WS_TABSTOP|BS_OWNERDRAW as u32,x,220,116,40);EnableWindow(GetDlgItem(h,id),0);
    }
    let mut panel=Panel{hwnd:h,owner,tag,dark:anchor.dark,worker,view:Snapshot::default(),pending:false};panel.submit(Job::Read);
    PANEL.with(|p|p.replace(Some(panel)));SetTimer(h,TIMER,1000,None);ShowWindow(h,SW_SHOW);SetForegroundWindow(h);SetFocus(GetDlgItem(h,PLAY));
}
pub(super) fn open_for(owner:HWND)->bool{PANEL.with(|p|p.borrow().as_ref().is_some_and(|p|p.owner==owner))}
pub(super) unsafe fn close_for(owner:HWND){let window=PANEL.with(|p|p.borrow().as_ref().filter(|p|owner.is_null()||p.owner==owner).map(|p|p.hwnd));if let Some(h)=window{DestroyWindow(h);}}
pub(super) unsafe fn keyboard(msg:&MSG)->bool{let window=PANEL.with(|p|p.borrow().as_ref().map(|p|p.hwnd));window.is_some_and(|h|(msg.hwnd==h||IsChild(h,msg.hwnd)!=0)&&IsDialogMessageW(h,msg)!=0)}
unsafe extern "system" fn proc(h:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
    if msg==WM_NCDESTROY{KillTimer(h,TIMER);let old=PANEL.with(|p|p.borrow_mut().take());if let Some(old)=old{super::panel_closed(old.owner);drop(old);}return DefWindowProcW(h,msg,w,l);}
    if msg==WM_ACTIVATE&&w&0xffff==WA_INACTIVE as usize{PostMessageW(h,WM_CLOSE,0,0);return 0;}
    if msg==WM_COMMAND&&w&0xffff==IDCANCEL as usize{PostMessageW(h,WM_CLOSE,0,0);return 0;}
    PANEL.with(|slot|{
        let Ok(mut slot)=slot.try_borrow_mut()else{return DefWindowProcW(h,msg,w,l);};let Some(p)=slot.as_mut().filter(|p|p.hwnd==h)else{return DefWindowProcW(h,msg,w,l);};
        match msg{
            READY if w==p.tag=>{p.receive();0},WM_TIMER if w==TIMER=>{p.submit(Job::Read);0},WM_ERASEBKGND=>1,
            WM_PAINT=>{utilities::paint_shell(h,p.dark,&text("Đang phát","Now playing","正在播放"),&if p.view.failed{text("Windows chưa thực hiện được yêu cầu","Windows could not complete the request","Windows 无法完成请求")}else{String::new()});0},
            WM_CTLCOLORSTATIC|WM_CTLCOLORBTN=>chrome::child_color(w as HDC,p.dark),
            WM_DRAWITEM if l!=0=>{
                let item=&*(l as *const DRAWITEMSTRUCT);
                if item.CtlID==ART as u32{
                    let background=super::surface::Palette::new(p.dark).panel;
                    super::fill(item.hDC,item.rcItem,chrome::color(background));
                    if p.view.art.len()==96*96{
                        let pixels=p.view.art.iter().map(|&pixel|{
                            let alpha=pixel>>24;let channel=|shift:u32,back:u8|(((pixel>>shift)&255u32)+(u32::from(back)*(255-alpha)+127)/255).min(255);
                            0xff000000|(channel(16,background[0])<<16)|(channel(8,background[1])<<8)|channel(0,background[2])
                        }).collect::<Vec<u32>>();
                        let mut info:BITMAPINFO=std::mem::zeroed();info.bmiHeader.biSize=std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                        info.bmiHeader.biWidth=96;info.bmiHeader.biHeight=-96;info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;
                        StretchDIBits(item.hDC,item.rcItem.left,item.rcItem.top,item.rcItem.right-item.rcItem.left,item.rcItem.bottom-item.rcItem.top,0,0,96,96,pixels.as_ptr().cast(),&info,DIB_RGB_COLORS,SRCCOPY);
                    }
                }else{utilities::paint_button(item,p.dark);}1
            },
            WM_COMMAND if !p.pending=>{
                let command=match (w&0xffff) as i32{PLAY=>1,NEXT=>2,PREV=>3,_=>return 0};
                if p.view.flags&1!=0{p.submit(Job::Command{generation:p.view.generation,identity:p.view.identity.clone(),command});}0
            },
            _=>DefWindowProcW(h,msg,w,l),
        }
    })
}

#[cfg(all(test,target_env="msvc"))]
#[test]
fn media_rejects_unobserved_and_invalid_commands_without_playback_writes(){unsafe{
    let model=tlauncher_media_create();assert!(!model.is_null());
    assert!(!tlauncher_media_command(model,0,wide("unobserved-session").as_ptr(),1));
    assert!(!tlauncher_media_command(model,1,wide("unobserved-session").as_ptr(),99));
    tlauncher_media_free(model);
}}
#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="native on-demand media reads and panel teardown on hosted Windows; no playback commands"]
fn native_media_panel_reads_and_releases_without_playback_commands(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use std::time::Instant;
    unsafe{
        let owner=CreateWindowExW(0,wide("STATIC").as_ptr(),wide("Media fixture owner").as_ptr(),WS_POPUP|WS_VISIBLE,
            10,10,80,60,ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());assert!(!owner.is_null());
        let pump_until=|condition:&dyn Fn()->bool|{
            let start=Instant::now();let mut msg:MSG=std::mem::zeroed();while !condition(){
                for _ in 0..64{if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}if !keyboard(&msg){TranslateMessage(&msg);DispatchMessageW(&msg);}}
                assert!(start.elapsed()<Duration::from_secs(12),"media worker bounded lifetime");std::thread::sleep(Duration::from_millis(10));
            }
        };
        for _ in 0..3{
            open(owner,POINT{x:500,y:700});assert!(open_for(owner));
            pump_until(&||PANEL.with(|p|p.borrow().as_ref().is_some_and(|p|!p.pending)));
            let window=PANEL.with(|p|p.borrow().as_ref().unwrap().hwnd);
            assert_ne!(IsWindow(GetDlgItem(window,TITLE)),0);assert_ne!(IsWindow(GetDlgItem(window,PLAY)),0);
            close_for(owner);assert_eq!(IsWindow(window),0);pump_until(&||!WORKER.load(Ordering::Acquire));
        }
        DestroyWindow(owner);
    }
    println!("FLUIDWIN_MEDIA panel=PASS metadata_read=EXERCISED artwork_path=IMPLEMENTED playback_commands=NONE worker_teardown=PASS");
}
