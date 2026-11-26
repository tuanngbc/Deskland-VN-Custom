//! Native city search, sharing the profile and transport with Settings/weather.
//! Original widget/centro semantics: 60 UTF-16 units, 300 ms debounce, six
//! results, and immediate invalidation before any action on a changed query.
use super::{panel_chrome as chrome,utilities};
use crate::{desktop::wide,weather};
use serde_json::Value;
use std::{cell::{Cell,RefCell},ffi::c_void,ptr,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}}};
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::*,System::{LibraryLoader::GetModuleHandleW,
    Threading::{TrySubmitThreadpoolCallback,PTP_CALLBACK_INSTANCE}},UI::{Controls::*,Input::KeyboardAndMouse::*,WindowsAndMessaging::*}};
const READY:u32=WM_APP+162;
const DEBOUNCE:usize=1;
const QUERY:i32=6101;const SEARCH:i32=6102;const RESULTS:i32=6103;const APPLY:i32=6104;const STATUS:i32=6105;
static ACTIVE:AtomicBool=AtomicBool::new(false);
thread_local!{static PANEL:RefCell<Option<Panel>>=const{RefCell::new(None)};static SERIAL:Cell<usize>=const{Cell::new(0)};}
fn text(vi:&str,en:&str,zh:&str)->String{crate::launcher::localized(vi,en,zh).into()}
fn title()->String{text("Chọn thành phố","Choose a city","选择城市")}
fn enabled()->bool{super::STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended})}
#[derive(Default)]struct Search{query:String,generation:u64,pending:bool,ready:bool,rows:Vec<Value>,failed:bool}
impl Search{
    fn edit(&mut self,query:String){
        self.query=query;self.generation=self.generation.wrapping_add(1);
        self.rows.clear();self.pending=false;self.ready=false;self.failed=false;
    }
    fn publish(&mut self,generation:u64,result:Result<Vec<Value>,()>)->bool{
        if generation!=self.generation{return false;}
        self.pending=false;self.ready=result.is_ok();self.failed=result.is_err();self.rows=result.unwrap_or_default();true
    }
    fn select(&self,generation:u64,index:usize)->Option<Value>{
        if !self.ready||self.pending||generation!=self.generation{return None;}
        self.rows.get(index).filter(|row|weather::Location::read(row).is_some()).cloned()
    }
}
struct Answer{generation:u64,result:Result<Vec<Value>,()>}
struct Delivery{closed:AtomicBool,answer:Mutex<Option<Answer>>}
struct Job{hwnd:usize,ticket:usize,generation:u64,query:String,language:String,delivery:Arc<Delivery>}
unsafe extern "system" fn worker(_:PTP_CALLBACK_INSTANCE,raw:*mut c_void){
    let job=Box::from_raw(raw.cast::<Job>());
    let result=if job.delivery.closed.load(Ordering::Acquire){Err(())}else{weather::geocode(&job.query,&job.language)};
    if job.delivery.closed.load(Ordering::Acquire){ACTIVE.store(false,Ordering::Release);return;}
    if let Ok(mut answer)=job.delivery.answer.lock(){*answer=Some(Answer{generation:job.generation,result});}
    // Publish before releasing admission: an earlier worker must never
    // overwrite a newer result after the next query has started.
    ACTIVE.store(false,Ordering::Release);
    PostMessageW(job.hwnd as HWND,READY,job.ticket,0);
}
struct Panel{hwnd:HWND,owner:HWND,ticket:usize,dark:bool,font:HFONT,model:Search,delivery:Arc<Delivery>,return_widgets:bool}
impl Drop for Panel{fn drop(&mut self){self.delivery.closed.store(true,Ordering::Release);unsafe{DeleteObject(self.font);}}}
impl Panel{
    unsafe fn clear_results(&self){
        SendMessageW(GetDlgItem(self.hwnd,RESULTS),LB_RESETCONTENT,0,0);
        EnableWindow(GetDlgItem(self.hwnd,APPLY),0);
    }
    unsafe fn status(&self,value:&str){SetWindowTextW(GetDlgItem(self.hwnd,STATUS),wide(value).as_ptr());}
    unsafe fn changed(&mut self){
        KillTimer(self.hwnd,DEBOUNCE);let mut value=[0u16;61];let n=GetWindowTextW(GetDlgItem(self.hwnd,QUERY),value.as_mut_ptr(),61).max(0) as usize;
        self.model.edit(String::from_utf16_lossy(&value[..n]));self.clear_results();
        self.status(&text("Nhập tên thành phố","Type a city name","输入城市名称"));
        if self.model.query.trim().chars().count()>=2{SetTimer(self.hwnd,DEBOUNCE,300,None);}
    }
    unsafe fn request(&mut self){
        if self.model.pending||self.model.query.trim().chars().count()<2{return;}
        self.status(&text("Đang tìm…","Searching…","正在搜索…"));
        if ACTIVE.swap(true,Ordering::AcqRel){SetTimer(self.hwnd,DEBOUNCE,300,None);return;}
        KillTimer(self.hwnd,DEBOUNCE);self.model.pending=true;self.model.ready=false;self.clear_results();
        let job=Box::into_raw(Box::new(Job{hwnd:self.hwnd as usize,ticket:self.ticket,generation:self.model.generation,
            query:self.model.query.clone(),language:text("vi","en","zh"),delivery:self.delivery.clone()}));
        if TrySubmitThreadpoolCallback(Some(worker),job.cast(),ptr::null())==0{
            drop(Box::from_raw(job));ACTIVE.store(false,Ordering::Release);
            self.model.publish(self.model.generation,Err(()));self.render_results();
        }
    }
    unsafe fn render_results(&self){
        self.clear_results();let list=GetDlgItem(self.hwnd,RESULTS);
        for row in &self.model.rows{
            let name=row["name"].as_str().unwrap_or("");let region=row["admin1"].as_str().unwrap_or("");let country=row["country"].as_str().unwrap_or("");
            let details:String=if region==country||country.is_empty(){region.into()}else if region.is_empty(){country.into()}else{format!("{region}, {country}")};
            let label:String=if details.is_empty(){name.into()}else{format!("{name} · {details}")};
            SendMessageW(list,LB_ADDSTRING,0,wide(&label).as_ptr() as LPARAM);
        }
        self.status(&if self.model.failed{text("Chưa tìm được. Bấm Tìm để thử lại.","Search unavailable. Press Search to retry.","搜索暂不可用，请重试。")}
            else if self.model.rows.is_empty(){text("Không tìm thấy thành phố","No city found","未找到城市")}
            else{text("Chọn thành phố rồi bấm Áp dụng","Select a city, then press Apply","选择城市，然后按应用")});
    }
    unsafe fn receive(&mut self){
        let answer=self.delivery.answer.lock().ok().and_then(|mut a|a.take());
        if let Some(answer)=answer{if self.model.publish(answer.generation,answer.result){self.render_results();}}
    }
    unsafe fn selection(&self)->Option<Value>{
        let index=SendMessageW(GetDlgItem(self.hwnd,RESULTS),LB_GETCURSEL,0,0);
        if index<0{return None;}self.model.select(self.model.generation,index as usize)
    }
}
pub(super) fn open_for(owner:HWND)->bool{PANEL.with(|s|s.borrow().as_ref().is_some_and(|p|p.owner==owner))}
pub(super) unsafe fn close_for(owner:HWND){
    let hwnd=PANEL.with(|s|s.borrow().as_ref().filter(|p|owner.is_null()||p.owner==owner).map(|p|p.hwnd));
    if let Some(hwnd)=hwnd{DestroyWindow(hwnd);}
}
pub(super) unsafe fn open(owner:HWND){open_inner(owner,false);}
pub(super) unsafe fn open_widgets(owner:HWND){open_inner(owner,true);}
unsafe fn open_inner(owner:HWND,return_widgets:bool){
    if !enabled()||IsWindow(owner)==0{return;}
    utilities::close_panels();let class=wide("TLauncher.WeatherCity");if !chrome::register(&class,Some(proc)){return;}
    let anchor=super::dock_app_anchor(owner,None);let Some(work)=anchor.work()else{return;};
    let dpi=windows_sys::Win32::UI::HiDpi::GetDpiForWindow(owner).clamp(72,288);let px=|n|chrome::scale(dpi,n);
    let (width,height)=(px(460),px(378));if width>work.right-work.left||height>work.bottom-work.top{return;}
    let (x,y)=chrome::placement(work,anchor,width,height,px(8));
    let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_TOPMOST|WS_EX_LAYERED,class.as_ptr(),wide(&title()).as_ptr(),WS_POPUP|WS_CLIPCHILDREN,
        x,y,width,height,owner,ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());if hwnd.is_null(){return;}
    chrome::frame(hwnd,width,height,dpi,anchor);
    // Native edit/list controls need a solid backing; uniform window alpha
    // otherwise lets unrelated text behind the picker bleed into their rows.
    SetLayeredWindowAttributes(hwnd,0,255,LWA_ALPHA);let font=chrome::choose_font(dpi,14,400);
    let control=|class:&str,label:&str,id:i32,style:u32,x,y,w,h|{
        let child=CreateWindowExW(0,wide(class).as_ptr(),wide(label).as_ptr(),WS_CHILD|WS_VISIBLE|style,px(x),px(y),px(w),px(h),hwnd,id as usize as HMENU,GetModuleHandleW(ptr::null()),ptr::null());
        if !child.is_null(){SendMessageW(child,WM_SETFONT,font as usize,0);}child
    };
    control("STATIC",&text("&Thành phố","&City","城市(&C)"),6100,0,16,60,334,22);
    let edit=control("EDIT","",QUERY,WS_TABSTOP|WS_BORDER|ES_AUTOHSCROLL as u32,16,86,334,30);
    SendMessageW(edit,EM_SETLIMITTEXT,60,0);
    control("BUTTON",&text("Tìm","Search","搜索"),SEARCH,WS_TABSTOP|BS_OWNERDRAW as u32,358,85,86,32);
    let list=control("LISTBOX","",RESULTS,WS_TABSTOP|WS_BORDER|WS_VSCROLL|LBS_NOTIFY as u32|LBS_NOINTEGRALHEIGHT as u32,16,126,428,158);
    control("STATIC",&text("Nhập tên thành phố","Type a city name","输入城市名称"),STATUS,0,16,292,428,32);
    let apply=control("BUTTON",&text("Áp dụng","Apply","应用"),APPLY,WS_TABSTOP|BS_OWNERDRAW as u32,244,334,96,30);
    control("BUTTON",&text("Hủy","Cancel","取消"),IDCANCEL,WS_TABSTOP|BS_OWNERDRAW as u32,348,334,96,30);
    if [edit,list,apply].iter().any(|h|h.is_null()){DestroyWindow(hwnd);DeleteObject(font);return;}
    EnableWindow(apply,0);let ticket=SERIAL.with(|s|{let n=s.get().wrapping_add(1);s.set(n);n});
    PANEL.with(|s|s.replace(Some(Panel{hwnd,owner,ticket,dark:anchor.dark,font,model:Search::default(),return_widgets,
        delivery:Arc::new(Delivery{closed:AtomicBool::new(false),answer:Mutex::new(None)})})));
    ShowWindow(hwnd,SW_SHOW);SetForegroundWindow(hwnd);SetFocus(edit);
}
unsafe fn cancel(hwnd:HWND){
    let owner=PANEL.with(|s|s.borrow().as_ref().filter(|p|p.hwnd==hwnd&&p.return_widgets).map(|p|p.owner));
    DestroyWindow(hwnd);
    if let Some(owner)=owner{super::weather_panel::perform(owner,ptr::null_mut(),super::weather_panel::Action::Widgets);}
}
pub(super) unsafe fn keyboard(msg:&MSG)->bool{
    let hwnd=PANEL.with(|s|s.borrow().as_ref().map(|p|p.hwnd));let Some(hwnd)=hwnd else{return false;};
    if msg.hwnd!=hwnd&&IsChild(hwnd,msg.hwnd)==0{return false;}
    if msg.message==WM_KEYDOWN{
        if msg.wParam as u16==VK_ESCAPE{cancel(hwnd);return true;}
        if msg.wParam as u16==VK_RETURN{
            let focus=GetDlgCtrlID(GetFocus());
            let command=if focus==IDCANCEL{IDCANCEL}else if focus==RESULTS||focus==APPLY{APPLY}else{SEARCH};
            SendMessageW(hwnd,WM_COMMAND,command as usize,0);return true;
        }
    }
    IsDialogMessageW(hwnd,msg)!=0
}
unsafe extern "system" fn proc(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
    if msg==WM_NCDESTROY{
        KillTimer(hwnd,DEBOUNCE);let old=PANEL.with(|s|{let mut s=s.borrow_mut();if s.as_ref().is_some_and(|p|p.hwnd==hwnd){s.take()}else{None}});
        if let Some(old)=old{super::panel_closed(old.owner);drop(old);}return DefWindowProcW(hwnd,msg,w,l);
    }
    if msg==WM_ACTIVATE&&w&0xffff==WA_INACTIVE as usize{PostMessageW(hwnd,WM_CLOSE,0,0);return 0;}
    if msg==WM_COMMAND&&w&0xffff==IDCANCEL as usize{cancel(hwnd);return 0;}
    let mut chosen=None;
    let result=PANEL.with(|s|{
        let Ok(mut s)=s.try_borrow_mut()else{return DefWindowProcW(hwnd,msg,w,l);};let Some(p)=s.as_mut().filter(|p|p.hwnd==hwnd)else{return DefWindowProcW(hwnd,msg,w,l);};
        match msg{
            READY if w==p.ticket&&enabled()=>{p.receive();0},
            WM_TIMER if w==DEBOUNCE=>{KillTimer(hwnd,DEBOUNCE);if enabled(){p.request();}0},
            WM_COMMAND=>{match ((w&0xffff) as i32,(w>>16) as u32){
                (QUERY,EN_CHANGE)=>p.changed(),(SEARCH,_)=>{KillTimer(hwnd,DEBOUNCE);p.request();},
                (RESULTS,LBN_SELCHANGE)=>{EnableWindow(GetDlgItem(hwnd,APPLY),i32::from(p.selection().is_some()));},
                (APPLY,_) if enabled()=>chosen=p.selection(),_=>{},
            }0},
            WM_PAINT=>{utilities::paint_shell(hwnd,p.dark,&title(),"Open-Meteo");0},WM_ERASEBKGND=>1,
            WM_CTLCOLORSTATIC|WM_CTLCOLORBTN|WM_CTLCOLOREDIT|WM_CTLCOLORLISTBOX=>chrome::child_color(w as HDC,p.dark),
            WM_DRAWITEM if l!=0=>{utilities::paint_button(&*(l as *const DRAWITEMSTRUCT),p.dark);1},
            _=>DefWindowProcW(hwnd,msg,w,l),
        }
    });
    if let Some(row)=chosen{
        if let Some(location)=weather::Location::read(&row){
            let origin=PANEL.with(|s|s.borrow().as_ref().map(|p|(p.owner,p.return_widgets)));
            weather::configure(Some(location));super::emit(super::Event::WeatherLocation(row));DestroyWindow(hwnd);
            if let Some((owner,widgets))=origin{super::weather_panel::perform(owner,ptr::null_mut(),if widgets{super::weather_panel::Action::Widgets}else{super::weather_panel::Action::Open});}
        }
    }
    result
}

#[cfg(test)]mod tests{
    use super::*;
    fn city()->Value{serde_json::json!({"name":"Synthetic city","admin1":"Test region","country":"Test country","latitude":10.0,"longitude":106.0})}
    #[test]fn editing_query_immediately_refuses_old_results_and_failed_retry(){
        let mut model=Search::default();model.edit("First".into());let old=model.generation;
        assert!(model.publish(old,Ok(vec![city()])));assert!(model.select(old,0).is_some());
        model.edit("Second".into());assert!(model.select(old,0).is_none());assert!(model.rows.is_empty());
        assert!(!model.publish(old,Ok(vec![city()])));assert!(!model.ready);
        let current=model.generation;assert!(model.publish(current,Err(())));assert!(model.select(current,0).is_none());
        assert!(model.publish(current,Ok(vec![city()])));assert!(model.select(current,0).is_some());assert!(model.select(current,6).is_none());
        model.edit("S".into());assert!(model.rows.is_empty());assert!(!model.ready);
    }
    #[test]
    #[ignore="native city controls and synthetic generations; GitHub-hosted Windows only"]
    fn native_city_picker_clears_stale_rows_routes_keyboard_and_cancels_delivery(){
        use windows_sys::Win32::Graphics::Dwm::DwmFlush;
        use std::time::{Duration,Instant};
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(),Ok("true"));
        struct Locale(String);impl Drop for Locale{fn drop(&mut self){crate::launcher::spotlight_locale(&self.0);}}
        let _locale=Locale(text("vi","en","zh-CN"));
        unsafe{
            let (events_tx,events_rx)=std::sync::mpsc::channel();
            struct Events(Option<std::sync::mpsc::Sender<super::super::Event>>);
            impl Drop for Events{fn drop(&mut self){super::super::EVENTS.with(|s|s.replace(self.0.take()));}}
            let _events=Events(super::super::EVENTS.with(|s|s.replace(Some(events_tx))));
            super::super::STATE.with(|s|s.borrow_mut().prefs.enabled=true);
            let owner=CreateWindowExW(WS_EX_TOOLWINDOW,wide("STATIC").as_ptr(),wide("City search fixture").as_ptr(),WS_POPUP,
                10,10,260,80,ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());assert!(!owner.is_null());
            for locale in ["vi","en","zh-CN"]{
                crate::launcher::spotlight_locale(locale);open(owner);
                let hwnd=PANEL.with(|s|s.borrow().as_ref().unwrap().hwnd);assert!(open_for(owner));
                let (mut color,mut alpha,mut flags)=(0,0,0);assert_ne!(GetLayeredWindowAttributes(hwnd,&mut color,&mut alpha,&mut flags),0);
                assert_eq!((alpha,flags),(255,LWA_ALPHA),"native text controls retain opaque readable backing");
                assert_eq!(SendMessageW(GetDlgItem(hwnd,QUERY),EM_GETLIMITTEXT,0,0),60);
                SetWindowTextW(GetDlgItem(hwnd,QUERY),wide("Synthetic city").as_ptr());KillTimer(hwnd,DEBOUNCE);
                let (generation,ticket,delivery)=PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().unwrap();
                    assert!(p.model.publish(p.model.generation,Ok(vec![city()])));p.render_results();
                    (p.model.generation,p.ticket,p.delivery.clone())
                });
                assert_eq!(SendMessageW(GetDlgItem(hwnd,RESULTS),LB_GETCOUNT,0,0),1);
                SendMessageW(GetDlgItem(hwnd,RESULTS),LB_SETCURSEL,0,0);
                SendMessageW(hwnd,WM_COMMAND,RESULTS as usize|((LBN_SELCHANGE as usize)<<16),0);
                assert_ne!(IsWindowEnabled(GetDlgItem(hwnd,APPLY)),0);
                let until=Instant::now()+Duration::from_millis(200);while Instant::now()<until{
                    let mut msg:MSG=std::mem::zeroed();for _ in 0..32{if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&msg);DispatchMessageW(&msg);}
                    std::thread::sleep(Duration::from_millis(10));
                }
                UpdateWindow(hwnd);DwmFlush();let mut bounds:RECT=std::mem::zeroed();GetWindowRect(hwnd,&mut bounds);
                let (w,h)=(bounds.right-bounds.left,bounds.bottom-bounds.top);let mut layer=super::super::glass_probe::Layer::new(w,h).unwrap();
                let dc=GetDC(ptr::null_mut());assert_ne!(BitBlt(layer.dc(),0,0,w,h,dc,bounds.left,bounds.top,SRCCOPY|CAPTUREBLT),0);ReleaseDC(ptr::null_mut(),dc);GdiFlush();
                if let Some(root)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){
                    let rgba=layer.pixels().chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],255]).collect::<Vec<_>>();
                    image::save_buffer(std::path::PathBuf::from(root).join(format!("weather-city-synthetic-{locale}.png")),&rgba,w as u32,h as u32,image::ColorType::Rgba8).unwrap();
                }
                SetWindowTextW(GetDlgItem(hwnd,QUERY),wide("Changed query").as_ptr());KillTimer(hwnd,DEBOUNCE);
                assert_eq!(SendMessageW(GetDlgItem(hwnd,RESULTS),LB_GETCOUNT,0,0),0);assert_eq!(IsWindowEnabled(GetDlgItem(hwnd,APPLY)),0);
                *delivery.answer.lock().unwrap()=Some(Answer{generation,result:Ok(vec![city()])});
                SendMessageW(hwnd,READY,ticket,0);assert_eq!(SendMessageW(GetDlgItem(hwnd,RESULTS),LB_GETCOUNT,0,0),0);
                SendMessageW(hwnd,WM_COMMAND,APPLY as usize,0);assert_ne!(IsWindow(hwnd),0);
                let esc=MSG{hwnd:GetDlgItem(hwnd,QUERY),message:WM_KEYDOWN,wParam:VK_ESCAPE as usize,..std::mem::zeroed()};
                assert!(keyboard(&esc));assert_eq!(IsWindow(hwnd),0);assert!(delivery.closed.load(Ordering::Acquire));assert!(!open_for(owner));
                // A retained completion cannot attach to a later panel even if
                // Windows reuses its HWND: each opening has a different ticket.
                open(owner);let new_hwnd=PANEL.with(|s|s.borrow().as_ref().unwrap().hwnd);
                SendMessageW(new_hwnd,READY,ticket,0);assert_eq!(SendMessageW(GetDlgItem(new_hwnd,RESULTS),LB_GETCOUNT,0,0),0);
                assert!(events_rx.try_recv().is_err());
                weather::fixture_reading();
                SetWindowTextW(GetDlgItem(new_hwnd,QUERY),wide("Synthetic city").as_ptr());KillTimer(new_hwnd,DEBOUNCE);
                PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().unwrap();assert!(p.model.publish(p.model.generation,Ok(vec![city()])));p.render_results();});
                SendMessageW(GetDlgItem(new_hwnd,RESULTS),LB_SETCURSEL,0,0);SetFocus(GetDlgItem(new_hwnd,RESULTS));
                assert!(keyboard(&MSG{hwnd:GetDlgItem(new_hwnd,RESULTS),message:WM_KEYDOWN,wParam:VK_RETURN as usize,..std::mem::zeroed()}));
                assert_eq!(IsWindow(new_hwnd),0);
                match events_rx.try_recv().unwrap(){super::super::Event::WeatherLocation(row)=>assert_eq!(row,city()),other=>panic!("unexpected event: {other:?}")}
                assert_eq!(weather::snapshot().location.unwrap().name,"Synthetic city");
                super::super::control_center::close_for(owner);assert_eq!(weather::fixture_subscribers(),0);
                // The shared widget route must survive its existing separate
                // native picker, on both explicit cancellation and selection.
                let widgets=super::super::weather_panel::Action::Widgets;
                weather::fixture_reading();let shared=weather::current().unwrap();
                super::super::weather_panel::perform(owner,ptr::null_mut(),widgets);
                let old_popup=super::super::control_center::window(owner).unwrap();
                let old_ticket=super::super::control_center::ticket(owner).unwrap();
                super::super::weather_panel::perform(owner,old_popup,super::super::weather_panel::Action::City);
                let picker=PANEL.with(|s|{let s=s.borrow();let p=s.as_ref().unwrap();assert!(p.return_widgets);p.hwnd});
                assert_eq!(IsWindow(old_popup),0);assert_eq!(weather::fixture_subscribers(),0);
                assert!(keyboard(&MSG{hwnd:GetDlgItem(picker,QUERY),message:WM_KEYDOWN,wParam:VK_ESCAPE as usize,..std::mem::zeroed()}));
                let combined=super::super::control_center::window(owner).unwrap();let ticket=super::super::control_center::ticket(owner).unwrap();
                assert_ne!(old_ticket,ticket);assert!(super::super::weather_panel::combined_for(owner,combined,ticket));
                assert!(Arc::ptr_eq(&shared,&weather::current().unwrap()));assert_eq!(weather::fixture_subscribers(),1);
                assert!(events_rx.try_recv().is_err(),"cancel must not persist a location");
                super::super::weather_panel::perform(owner,combined,super::super::weather_panel::Action::City);
                let picker=PANEL.with(|s|s.borrow().as_ref().unwrap().hwnd);
                let mut replacement=city();replacement["name"]=Value::from("Changed synthetic city");
                SetWindowTextW(GetDlgItem(picker,QUERY),wide("Changed synthetic city").as_ptr());KillTimer(picker,DEBOUNCE);
                PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().unwrap();assert!(p.model.publish(p.model.generation,Ok(vec![replacement.clone()])));p.render_results();});
                SendMessageW(GetDlgItem(picker,RESULTS),LB_SETCURSEL,0,0);SendMessageW(picker,WM_COMMAND,APPLY as usize,0);
                let popup=super::super::control_center::window(owner).unwrap();let ticket=super::super::control_center::ticket(owner).unwrap();
                assert!(super::super::weather_panel::combined_for(owner,popup,ticket));
                assert_eq!(weather::snapshot().location.unwrap().name,"Changed synthetic city");assert!(weather::current().is_none());
                match events_rx.try_recv().unwrap(){super::super::Event::WeatherLocation(row)=>assert_eq!(row,replacement),other=>panic!("unexpected combined event: {other:?}")}
                super::super::control_center::close_for(owner);assert_eq!(weather::fixture_subscribers(),0);
                assert!(super::super::news::view_for(popup,ticket).is_none());
            }
            weather::configure(None);assert!(!ACTIVE.load(Ordering::Acquire));DestroyWindow(owner);
        }
        println!("FLUIDWIN_WEATHER_CITY locales=vi,en,zh native_edit_list_buttons=PASS query_limit=60 stale_query=REFUSED stale_ticket=REFUSED keyboard_escape=PASS keyboard_select=PASS existing_profile_event=PASS combined_cancel_and_select=RESTORED combined_old_forecast=CLEARED combined_subscription=RELEASED closed_delivery=CANCELLED network=NOT_INVOKED profile_write=NOT_INVOKED");
    }
}
