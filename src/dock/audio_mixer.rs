//! On-demand FluidWin mixer. All endpoint/session COM ownership stays on one
//! temporary worker; native sliders use the shared utility panel drawing.
use super::{panel_chrome as chrome,utilities};
use crate::desktop::wide;
use std::{cell::{Cell,RefCell},mem,ptr,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering},mpsc},time::Duration};
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::*,System::LibraryLoader::GetModuleHandleW,
    UI::{Controls::*,Input::KeyboardAndMouse::*,WindowsAndMessaging::*}};

const READY:u32=WM_APP+141;
const TIMER:usize=1;
const OUTPUT:i32=5101;
const PREV:i32=5102;
const NEXT:i32=5103;
const SETTINGS:i32=5104;
const ROW_BASE:i32=5200;
const PAGE:usize=4;
const TBM_GETPOS:u32=WM_USER;
static WORKER:AtomicBool=AtomicBool::new(false);
thread_local!{static PANEL:RefCell<Option<Panel>>=const{RefCell::new(None)};static SERIAL:Cell<usize>=const{Cell::new(0)};}

#[cfg(target_env="msvc")]
extern "C"{
    fn tlauncher_audio_create()->*mut std::ffi::c_void;
    fn tlauncher_audio_destroy(model:*mut std::ffi::c_void);
    fn tlauncher_audio_read(model:*mut std::ffi::c_void)->u64;
    fn tlauncher_audio_count(model:*mut std::ffi::c_void,outputs:bool)->u32;
    fn tlauncher_audio_row(model:*mut std::ffi::c_void,index:u32,label:*mut u16,capacity:u32,level:*mut i32,flags:*mut i32)->bool;
    fn tlauncher_audio_output(model:*mut std::ffi::c_void,index:u32,label:*mut u16,capacity:u32,is_default:*mut bool)->bool;
    fn tlauncher_audio_key(model:*mut std::ffi::c_void,index:u32,key:*mut u16,capacity:u32)->bool;
    fn tlauncher_audio_change(model:*mut std::ffi::c_void,generation:u64,row:u32,operation:i32,value:i32)->bool;
    #[cfg(test)] fn tlauncher_audio_group_fixture()->i32;
}
#[derive(Clone)]
struct Row{label:String,key:String,level:i32,flags:i32}
#[derive(Default)]
struct Snapshot{generation:u64,rows:Vec<Row>,outputs:Vec<(String,bool)>,failed:bool}
enum Job{Read,Change{generation:u64,row:u32,operation:i32,value:i32}}
struct Worker{tx:mpsc::SyncSender<Job>,closed:Arc<AtomicBool>,result:Arc<Mutex<Option<Snapshot>>>}
impl Drop for Worker{fn drop(&mut self){self.closed.store(true,Ordering::Release);}}
impl Worker{
    #[cfg(target_env="msvc")]
    fn start(hwnd:HWND,tag:usize)->Option<Self>{
        if WORKER.swap(true,Ordering::AcqRel){return None;}
        let (tx,rx)=mpsc::sync_channel(1);
        let closed=Arc::new(AtomicBool::new(false));let result=Arc::new(Mutex::new(None));
        let (stop,answer,h)=(closed.clone(),result.clone(),hwnd as usize);
        let thread=std::thread::Builder::new().name("TLauncher volume mixer".into()).spawn(move||{
            struct Permit;impl Drop for Permit{fn drop(&mut self){WORKER.store(false,Ordering::Release);}}
            let _permit=Permit;
            unsafe{
                use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_MULTITHREADED};
                if CoInitializeEx(None,COINIT_MULTITHREADED).is_err(){
                    if let Ok(mut r)=answer.lock(){*r=Some(Snapshot{failed:true,..Default::default()});}
                    PostMessageW(h as HWND,READY,tag,0);return;
                }
                struct Apartment;impl Drop for Apartment{fn drop(&mut self){unsafe{CoUninitialize();}}}
                let _com=Apartment;
                struct Model(*mut std::ffi::c_void);impl Drop for Model{fn drop(&mut self){unsafe{tlauncher_audio_destroy(self.0);}}}
                let model=Model(tlauncher_audio_create());
                while !stop.load(Ordering::Acquire){
                    let job=match rx.recv_timeout(Duration::from_millis(200)){
                        Ok(job)=>job,Err(mpsc::RecvTimeoutError::Timeout)=>continue,Err(_)=>break,
                    };
                    if stop.load(Ordering::Acquire){break;}
                    let mut view=Snapshot::default();
                    if model.0.is_null(){view.failed=true;}else{
                        if let Job::Change{generation,row,operation,value}=job{
                            if read_only()||!tlauncher_audio_change(model.0,generation,row,operation,value){view.failed=true;}
                        }
                        view.generation=tlauncher_audio_read(model.0);view.failed|=view.generation==0;
                        for i in 0..tlauncher_audio_count(model.0,false).min(256){
                            let mut label=[0u16;512];let mut key=[0u16;4096];let mut level=0;let mut flags=0;
                            if tlauncher_audio_row(model.0,i,label.as_mut_ptr(),512,&mut level,&mut flags){
                                if !tlauncher_audio_key(model.0,i,key.as_mut_ptr(),4096){view.failed=true;break;}
                                view.rows.push(Row{label:decode(&label),key:decode(&key),level,flags});
                            }
                        }
                        for i in 0..tlauncher_audio_count(model.0,true).min(64){
                            let mut label=[0u16;512];let mut default=false;
                            if tlauncher_audio_output(model.0,i,label.as_mut_ptr(),512,&mut default){view.outputs.push((decode(&label),default));}
                        }
                    }
                    if stop.load(Ordering::Acquire){break;}
                    if let Ok(mut slot)=answer.lock(){*slot=Some(view);}
                    PostMessageW(h as HWND,READY,tag,0);
                }
            }
        });
        if thread.is_err(){WORKER.store(false,Ordering::Release);return None;}
        Some(Self{tx,closed,result})
    }
    #[cfg(not(target_env="msvc"))]
    fn start(_:HWND,_:usize)->Option<Self>{None}
}
fn decode(s:&[u16])->String{String::from_utf16_lossy(&s[..s.iter().position(|&c|c==0).unwrap_or(s.len())])}
fn read_only()->bool{crate::trial::active()}
fn text(vi:&str,en:&str,zh:&str)->String{crate::launcher::localized(vi,en,zh).into()}
struct Panel{hwnd:HWND,owner:HWND,tag:usize,dark:bool,dpi:u32,worker:Worker,view:Snapshot,page:usize,pending:bool,dragging:bool,wanted:Option<(usize,i32)>}
impl Panel{
    unsafe fn submit(&mut self,job:Job)->bool{
        if self.pending||self.worker.tx.try_send(job).is_err(){return false;}
        self.pending=true;
        EnableWindow(GetDlgItem(self.hwnd,OUTPUT),0);
        for slot in 0..PAGE{EnableWindow(GetDlgItem(self.hwnd,ROW_BASE+slot as i32*3+2),0);}
        true
    }
    unsafe fn control(&self,class:&str,label:&str,id:i32,style:u32,x:i32,y:i32,w:i32,h:i32)->HWND{
        let px=|n|chrome::scale(self.dpi,n);
        let child=CreateWindowExW(0,wide(class).as_ptr(),wide(label).as_ptr(),WS_CHILD|WS_VISIBLE|style,
            px(x),px(y),px(w),px(h),self.hwnd,id as usize as HMENU,GetModuleHandleW(ptr::null()),ptr::null());
        if !child.is_null(){SendMessageW(child,WM_SETFONT,GetStockObject(DEFAULT_GUI_FONT) as usize,0);}child
    }
    unsafe fn rebuild(&mut self){
        let focus=GetFocus();let focus_id=if IsChild(self.hwnd,focus)!=0{GetDlgCtrlID(focus)}else{0};
        for id in [OUTPUT,PREV,NEXT].into_iter().chain(ROW_BASE..ROW_BASE+PAGE as i32*3){let h=GetDlgItem(self.hwnd,id);if !h.is_null(){DestroyWindow(h);}}
        let pages=self.view.rows.len().div_ceil(PAGE).max(1);self.page=self.page.min(pages-1);
        let output=self.control("COMBOBOX","",OUTPUT,WS_TABSTOP|CBS_DROPDOWNLIST as u32|WS_VSCROLL,16,66,364,240);
        let mut selected=0;
        for (i,(label,is_default)) in self.view.outputs.iter().enumerate(){
            SendMessageW(output,CB_ADDSTRING,0,wide(label).as_ptr() as LPARAM);if *is_default{selected=i;}
        }
        SendMessageW(output,CB_SETCURSEL,selected,0);EnableWindow(output,i32::from(!read_only()&&!self.pending&&!self.view.outputs.is_empty()));
        let start=self.page*PAGE;
        for (slot,row) in self.view.rows.iter().skip(start).take(PAGE).enumerate(){
            let id=ROW_BASE+slot as i32*3;let y=104+slot as i32*76;
            let label=if row.flags&1!=0{format!("{} · {}%",row.label,row.level)}else{format!("    {} · {}%",row.label,row.level)};
            self.control("STATIC",&label,id,SS_LEFT as u32,16,y,364,21);
            let slider=self.control("msctls_trackbar32",&row.label,id+1,WS_TABSTOP|TBS_NOTICKS,16,y+25,262,32);
            SendMessageW(slider,TBM_SETRANGEMAX,1,100);SendMessageW(slider,TBM_SETPOS,1,row.level as isize);
            let label=if row.flags&2!=0{text("Bật tiếng","Unmute","取消静音")}else{text("Tắt tiếng","Mute","静音")};
            let mute=self.control("BUTTON",&label,id+2,WS_TABSTOP|BS_OWNERDRAW as u32,288,y+23,92,34);
            EnableWindow(slider,i32::from(!read_only()));EnableWindow(mute,i32::from(!read_only()&&!self.pending));
        }
        self.control("BUTTON","‹",PREV,WS_TABSTOP|BS_OWNERDRAW as u32,16,416,50,32);
        self.control("BUTTON","›",NEXT,WS_TABSTOP|BS_OWNERDRAW as u32,74,416,50,32);
        EnableWindow(GetDlgItem(self.hwnd,PREV),i32::from(self.page>0));EnableWindow(GetDlgItem(self.hwnd,NEXT),i32::from(self.page+1<pages));
        if focus_id!=0{let next=GetDlgItem(self.hwnd,focus_id);if !next.is_null()&&IsWindowEnabled(next)!=0{SetFocus(next);}}
        InvalidateRect(self.hwnd,ptr::null(),0);
    }
    unsafe fn receive(&mut self){
        let answer=self.worker.result.lock().ok().and_then(|mut r|r.take());
        let Some(view)=answer else{return;};self.pending=false;
        // Keep the displayed generation and row controls stable while a
        // slider gesture is in progress. Apply its latest position first.
        if let Some((row,value))=self.wanted{
            if view.generation==self.view.generation{
                if self.submit(Job::Change{generation:view.generation,row:row as u32,operation:0,value}){self.wanted=None;}return;
            }
            // Refresh/previous command changed the model generation. Resolve
            // only if the same endpoint/app row still occupies this index.
            if self.view.rows.get(row).zip(view.rows.get(row)).is_some_and(|(a,b)|a.key==b.key){
                if self.submit(Job::Change{generation:view.generation,row:row as u32,operation:0,value}){self.wanted=None;}
            }else{self.wanted=None;}
        }
        if self.dragging&&(self.view.rows.len()!=view.rows.len()||self.view.rows.iter().zip(&view.rows).any(|(a,b)|a.key!=b.key)){
            ReleaseCapture();self.dragging=false;self.wanted=None;
        }
        self.view=view;if !self.dragging{self.rebuild();}
    }
}
pub(super) unsafe fn open(owner:HWND,point:POINT){
    utilities::close_panels();
    let class=wide("TLauncher.AudioMixer");if !chrome::register(&class,Some(proc)){return;}
    InitCommonControlsEx(&INITCOMMONCONTROLSEX{dwSize:mem::size_of::<INITCOMMONCONTROLSEX>() as u32,dwICC:ICC_BAR_CLASSES});
    let anchor=chrome::Anchor::for_owner(owner,point);let Some(work)=anchor.work()else{return;};
    let dpi=windows_sys::Win32::UI::HiDpi::GetDpiForWindow(owner);let px=|n|chrome::scale(dpi,n);
    let (width,height)=(px(396),px(466));if height>work.bottom-work.top||width>work.right-work.left{return;}
    let (x,y)=chrome::placement(work,anchor,width,height,px(8));
    let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_TOPMOST|WS_EX_LAYERED,class.as_ptr(),wide(&text("Bộ trộn âm lượng","Volume mixer","音量混合器")).as_ptr(),
        WS_POPUP|WS_CLIPCHILDREN,x,y,width,height,owner,ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
    if hwnd.is_null(){return;}chrome::frame(hwnd,width,height,dpi,anchor);
    let tag=SERIAL.with(|s|{let n=s.get().wrapping_add(1);s.set(n);n});
    let Some(worker)=Worker::start(hwnd,tag)else{DestroyWindow(hwnd);return;};
    let mut panel=Panel{hwnd,owner,tag,dark:anchor.dark,dpi,worker,view:Snapshot::default(),page:0,pending:false,dragging:false,wanted:None};
    panel.control("BUTTON",&text("Cài đặt âm thanh…","Sound settings…","声音设置…"),SETTINGS,WS_TABSTOP|BS_OWNERDRAW as u32,144,416,236,32);
    panel.rebuild();panel.submit(Job::Read);PANEL.with(|p|{p.replace(Some(panel));});
    SetTimer(hwnd,TIMER,1000,None);ShowWindow(hwnd,SW_SHOW);SetForegroundWindow(hwnd);SetFocus(GetDlgItem(hwnd,SETTINGS));
}
pub(super) fn open_for(owner:HWND)->bool{PANEL.with(|p|p.borrow().as_ref().is_some_and(|p|p.owner==owner))}
pub(super) unsafe fn close_for(owner:HWND){
    let h=PANEL.with(|p|p.borrow().as_ref().filter(|p|owner.is_null()||p.owner==owner).map(|p|p.hwnd));
    if let Some(h)=h{DestroyWindow(h);}
}
pub(super) unsafe fn keyboard(msg:&MSG)->bool{
    let h=PANEL.with(|p|p.borrow().as_ref().map(|p|p.hwnd));
    h.is_some_and(|h|(msg.hwnd==h||IsChild(h,msg.hwnd)!=0)&&IsDialogMessageW(h,msg)!=0)
}
unsafe extern "system" fn proc(h:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
    if msg==WM_NCDESTROY{
        KillTimer(h,TIMER);let panel=PANEL.with(|p|p.borrow_mut().take());
        if let Some(p)=panel{super::panel_closed(p.owner);drop(p);}
        return DefWindowProcW(h,msg,w,l);
    }
    if msg==WM_ACTIVATE&&w&0xffff==WA_INACTIVE as usize{PostMessageW(h,WM_CLOSE,0,0);return 0;}
    // Defer destructive actions until after releasing the panel borrow.
    if msg==WM_COMMAND&&w&0xffff==IDCANCEL as usize{PostMessageW(h,WM_CLOSE,0,0);return 0;}
    if msg==WM_COMMAND&&w&0xffff==SETTINGS as usize{super::open("ms-settings:sound");PostMessageW(h,WM_CLOSE,0,0);return 0;}
    PANEL.with(|slot|{
        let Ok(mut slot)=slot.try_borrow_mut()else{return DefWindowProcW(h,msg,w,l);};
        let Some(p)=slot.as_mut().filter(|p|p.hwnd==h)else{return DefWindowProcW(h,msg,w,l);};
        match msg{
            READY if w==p.tag=>{p.receive();0}
            WM_TIMER if w==TIMER=>{if !p.dragging&&p.wanted.is_none(){p.submit(Job::Read);}0}
            WM_ERASEBKGND=>1,
            WM_PAINT=>{
                let hint=if p.view.failed{text("Windows chưa thực hiện được yêu cầu","Windows could not complete the request","Windows 无法完成请求")}
                    else if read_only(){text("Chỉ xem trong bản thử","Read-only in trial","试用版仅供查看")}
                    else if p.view.rows.is_empty(){text("Không có phiên âm thanh khả dụng","No audio sessions available","没有可用的音频会话")}
                    else{text("Thiết bị và từng ứng dụng","Outputs and application volumes","输出设备和应用音量")};
                utilities::paint_shell(h,p.dark,&text("Bộ trộn âm lượng","Volume mixer","音量混合器"),&hint);0
            }
            WM_CTLCOLORSTATIC|WM_CTLCOLORBTN=>chrome::child_color(w as HDC,p.dark),
            WM_DRAWITEM if l!=0=>{utilities::paint_button(&*(l as *const DRAWITEMSTRUCT),p.dark);1}
            WM_NOTIFY if l!=0=>{
                let hdr=&*(l as *const NMHDR);
                if hdr.code==NM_CUSTOMDRAW&&!utilities::high_contrast(){utilities::track_color(&*(l as *const NMCUSTOMDRAW),p.dark)}else{DefWindowProcW(h,msg,w,l)}
            }
            WM_HSCROLL if l!=0=>{
                let id=GetDlgCtrlID(l as HWND);let slot=(id-ROW_BASE)/3;
                if id>=ROW_BASE&&(id-ROW_BASE)%3==1&&slot<PAGE as i32&&!read_only(){
                    let row=p.page*PAGE+slot as usize;let value=SendMessageW(l as HWND,TBM_GETPOS,0,0).clamp(0,100) as i32;
                    p.dragging=w as u32&0xffff==TB_THUMBTRACK;
                    p.wanted=Some((row,value));
                    if p.submit(Job::Change{generation:p.view.generation,row:row as u32,operation:0,value}){p.wanted=None;}
                    if !p.dragging&&!p.pending{p.rebuild();}
                }0
            }
            WM_COMMAND=>{
                let id=(w&0xffff) as i32;let code=(w>>16) as u32;
                if id==OUTPUT&&code==CBN_SELCHANGE&&!p.pending&&!read_only(){
                    let i=SendMessageW(GetDlgItem(h,OUTPUT),CB_GETCURSEL,0,0);
                    if i>=0{p.submit(Job::Change{generation:p.view.generation,row:i as u32,operation:2,value:0});}
                }else if id==PREV||id==NEXT{
                    if !p.pending&&!p.dragging{if id==PREV{p.page=p.page.saturating_sub(1);}else{p.page+=1;}p.rebuild();}
                }else if id>=ROW_BASE&&(id-ROW_BASE)%3==2&&!read_only(){
                    let row=p.page*PAGE+((id-ROW_BASE)/3) as usize;
                    p.submit(Job::Change{generation:p.view.generation,row:row as u32,operation:1,value:0});
                }0
            }
            _=>DefWindowProcW(h,msg,w,l),
        }
    })
}

#[cfg(all(test,target_env="msvc"))]
#[test]
fn imported_mixer_groups_sessions_clamps_and_rejects_stale_generations(){assert_eq!(unsafe{tlauncher_audio_group_fixture()},0);}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="native on-demand audio panel/read-only enumeration on hosted Windows"]
fn native_panel_reads_devices_and_releases_worker_without_audio_writes(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use std::time::Instant;
    unsafe{
        let owner=CreateWindowExW(WS_EX_TOOLWINDOW,wide("STATIC").as_ptr(),wide("Mixer fixture owner").as_ptr(),WS_POPUP|WS_VISIBLE,
            20,20,100,80,ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
        assert!(!owner.is_null());
        let pump_until=|condition:&dyn Fn()->bool|{
            let start=Instant::now();let mut msg:MSG=mem::zeroed();
            while !condition(){
                while PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)!=0{
                    if !keyboard(&msg){TranslateMessage(&msg);DispatchMessageW(&msg);}
                }
                assert!(start.elapsed()<Duration::from_secs(8),"mixer worker/panel timeout");std::thread::sleep(Duration::from_millis(10));
            }
        };
        for _ in 0..4{
            open(owner,POINT{x:500,y:700});assert!(open_for(owner));
            pump_until(&||PANEL.with(|p|p.borrow().as_ref().is_some_and(|p|!p.pending)));
            let h=PANEL.with(|p|p.borrow().as_ref().unwrap().hwnd);
            assert_ne!(IsWindow(GetDlgItem(h,OUTPUT)),0);assert_ne!(IsWindow(GetDlgItem(h,SETTINGS)),0);
            close_for(owner);assert_eq!(IsWindow(h),0);assert!(!open_for(owner));
            pump_until(&||!WORKER.load(Ordering::Acquire));
        }
        DestroyWindow(owner);
    }
    println!("FLUIDWIN_MIXER_NATIVE panel=PASS endpoint_read=EXERCISED audio_writes=NONE worker_teardown=PASS hardware_control=NOT_VERIFIED");
}
