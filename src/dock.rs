//! Native Windows multi-monitor dock (opt-in; does not impersonate Shell_TrayWnd).
//! Managed UI thread, started/configured/stopped by Deskland's existing main loop.
//! Compatibility mode deliberately leaves Explorer's notification area intact.
use crate::desktop::wide;
use std::{cell::{Cell,RefCell},collections::{HashMap,HashSet},mem,ptr,sync::mpsc};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute,DWMWA_CLOAKED};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{OpenProcess,QueryFullProcessImageNameW,PROCESS_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::Win32::UI::Accessibility::*;

mod options;
mod runtime;
pub use options::{Options,Edit};
pub use runtime::{Runtime,Event};
use runtime::Command;
type Preferences=Options;

const CLASS:&str="TuanpcDeskNativeDock";
const WM_TICK:u32=WM_APP+50;
const WM_CONFIG:u32=WM_APP+51;
const ITEM_WIDTH:i32=66;
const HEIGHT:i32=70;
const MAX_ITEMS:usize=10;
const MENU_ALL:usize=501;
const MENU_LOCAL:usize=502;
const MENU_PIN:usize=503;
const MENU_UNPIN:usize=504;
const MENU_QUIT:usize=505;
const MENU_WIFI:usize=506;
const MENU_BLUETOOTH:usize=507;
const MENU_SOUND:usize=508;
const MENU_NOTIFICATIONS:usize=509;
const MENU_EXPLORER:usize=510;
const MENU_STAGE_LEFT:usize=511;
const MENU_STAGE_RIGHT:usize=512;
const MENU_STAGE_OFF:usize=513;
const MENU_MINIMIZE:usize=514;
const MENU_MAXIMIZE:usize=515;
const MENU_RESTORE:usize=516;
const MENU_SNAP_LEFT:usize=517;
const MENU_SNAP_RIGHT:usize=518;
const MENU_FULLSCREEN:usize=519;

#[derive(Clone)]
struct Screen { bounds:RECT, work:RECT, primary:bool }
#[derive(Clone)]
struct AppWindow { hwnd:HWND, title:String, path:String, rect:RECT }
#[derive(Clone,Debug,PartialEq,Eq)]
struct Item { path:String, title:String, hwnd:HWND, count:usize, pinned:bool }
#[derive(Clone)]
struct Bar { hwnd:HWND, bounds:RECT, items:Vec<Item>, hovered:i32, hidden:bool }
#[derive(Clone)]
struct Stage { hwnd:HWND, bounds:RECT, items:Vec<Item>, hidden:bool }
#[derive(Default)]
struct State { bars:Vec<Bar>, stages:Vec<Stage>, windows:Vec<AppWindow>, prefs:Preferences, screens:Vec<Screen>, suspended:bool, foreground:isize }
thread_local! {
    static STATE:RefCell<State>=RefCell::new(State::default());
    static CONTROL:Cell<HWND>=const{Cell::new(ptr::null_mut())};
    static DIRTY:Cell<bool>=const{Cell::new(false)};
    static COMMANDS:RefCell<Option<mpsc::Receiver<Command>>>=const{RefCell::new(None)};
    static EVENTS:RefCell<Option<mpsc::Sender<Event>>>=const{RefCell::new(None)};
    static ICONS:RefCell<HashMap<(String,bool),HICON>>=RefCell::new(HashMap::new());
}

// Detach state before any synchronous Win32 operation. Nested WM_PAINT sees an
// empty snapshot rather than reborrowing RefCell across Create/DestroyWindow.
fn update_state(f:impl FnOnce(&mut State)) {
    let mut state=STATE.with(|s|s.take());
    f(&mut state);
    STATE.with(|s|{s.replace(state);});
}
fn emit(event:Event) {EVENTS.with(|s|{if let Some(tx)=s.borrow().as_ref(){let _=tx.send(event);}});}

fn monitors()->Vec<Screen> {
    // Reuse Deskland's monitor detection/DPI owner; retrieve current work area
    // without a competing display enumeration implementation.
    crate::monitors().into_iter().map(|monitor|{
        let bounds=monitor.rect;let mut work=bounds;
        unsafe {let h=MonitorFromRect(&bounds,MONITOR_DEFAULTTONEAREST);let mut info:MONITORINFO=mem::zeroed();
            info.cbSize=mem::size_of::<MONITORINFO>() as u32;if GetMonitorInfoW(h,&mut info)!=0{work=info.rcWork;}}
        Screen{bounds,work,primary:monitor.primary}
    }).collect()
}
fn same_rect(a:RECT,b:RECT)->bool { a.left==b.left&&a.right==b.right&&a.top==b.top&&a.bottom==b.bottom }
fn rect_intersects(a:RECT,b:RECT)->bool {
    a.left<b.right && b.left<a.right && a.top<b.bottom && b.top<a.bottom
}
fn basename(path:&str)->String {
    path.rsplit(['\\','/']).next().unwrap_or(path).trim_end_matches(".exe").to_string()
}
fn is_shell_class(class:&str)->bool {
    matches!(class,"Progman"|"WorkerW"|"Shell_TrayWnd"|"Shell_SecondaryTrayWnd"|"NotifyIconOverflowWindow"|"ApplicationFrameInputSinkWindow"|"TuanpcDeskNativeDock"|"TuanpcDeskMsg"|"TuanpcDeskWall")
}
unsafe extern "system" fn enumerate(hwnd:HWND,param:LPARAM)->i32 {
    let out=&mut *(param as *mut Vec<AppWindow>);
    if IsWindowVisible(hwnd)==0 || GetWindow(hwnd,GW_OWNER)!=ptr::null_mut() {return 1;}
    if GetWindowLongW(hwnd,GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW!=0 {return 1;}
    let mut cloaked=0u32;
    if DwmGetWindowAttribute(hwnd,DWMWA_CLOAKED as u32,&mut cloaked as *mut _ as *mut _,mem::size_of::<u32>() as u32)>=0 && cloaked!=0{return 1;}
    let mut class=[0u16;128];let c=GetClassNameW(hwnd,class.as_mut_ptr(),128);
    if is_shell_class(&String::from_utf16_lossy(&class[..c.max(0) as usize])) {return 1;}
    let mut title=[0u16;260];let n=GetWindowTextW(hwnd,title.as_mut_ptr(),260);
    if n<=0{return 1;}
    let mut pid=0u32;GetWindowThreadProcessId(hwnd,&mut pid);
    if pid==0{return 1;}
    let process=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,0,pid);
    let mut path=String::new();
    if !process.is_null() {
        let mut buf=[0u16;1024];let mut len=buf.len() as u32;
        if QueryFullProcessImageNameW(process,0,buf.as_mut_ptr(),&mut len)!=0 {
            path=String::from_utf16_lossy(&buf[..len as usize]);
        }
        windows_sys::Win32::Foundation::CloseHandle(process);
    }
    if path.is_empty(){return 1;}
    let mut rect:RECT=mem::zeroed();
    if GetWindowRect(hwnd,&mut rect)==0{return 1;}
    // Minimized HWND bounds can be (-32000,-32000). MonitorFromWindow uses
    // the restored placement for iconic windows; don't classify them off-screen.
    if IsIconic(hwnd)!=0 {
        let monitor=MonitorFromWindow(hwnd,MONITOR_DEFAULTTONEAREST);
        let mut info:MONITORINFO=mem::zeroed();info.cbSize=mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor,&mut info)!=0{rect=info.rcMonitor;}
    }
    if rect.right<=rect.left || rect.bottom<=rect.top {return 1;}
    out.push(AppWindow{hwnd,title:String::from_utf16_lossy(&title[..n as usize]),path,rect});
    1
}
fn windows()->Vec<AppWindow>{
    let mut v=Vec::new();unsafe {EnumWindows(Some(enumerate),&mut v as *mut _ as LPARAM)};v
}
// Prefer a window on this monitor for pinned apps, and keep counts consistent with
// the selected local/all-monitors scope. An off-screen pinned app stays launchable.
fn item_list(all:&[AppWindow],screen:RECT,prefs:&Preferences)->Vec<Item>{
    let mut result=Vec::<Item>::new();let mut seen=HashSet::<String>::new();
    for path in &prefs.pinned {
        if !seen.insert(path.to_lowercase()){continue;}
        let matching=all.iter().filter(|w|w.path.eq_ignore_ascii_case(path));
        let current=matching.clone().find(|w|rect_intersects(screen,w.rect))
            .or_else(||if prefs.all_windows{matching.clone().next()}else{None});
        let count=matching.filter(|w|prefs.all_windows||rect_intersects(screen,w.rect)).count();
        result.push(Item{path:path.clone(),title:current.map(|w|w.title.clone()).unwrap_or_else(||basename(path)),
            hwnd:current.map(|w|w.hwnd).unwrap_or(ptr::null_mut()),count,pinned:true});
    }
    for w in all {
        if !prefs.all_windows && !rect_intersects(screen,w.rect){continue;}
        if !seen.insert(w.path.to_lowercase()){continue;}
        let selected=all.iter().find(|a|a.path.eq_ignore_ascii_case(&w.path) && rect_intersects(screen,a.rect)).unwrap_or(w);
        result.push(Item{path:selected.path.clone(),title:selected.title.clone(),hwnd:selected.hwnd,
            count:all.iter().filter(|v|v.path.eq_ignore_ascii_case(&w.path) && (prefs.all_windows || rect_intersects(screen,v.rect))).count(),
            pinned:false});
    }
    result.truncate(MAX_ITEMS);result
}
// Stage follows the same per-monitor visibility preference as Dock; deduplicate by executable.
fn stage_item_list(all:&[AppWindow],screen:RECT,all_windows:bool)->Vec<Item>{
    // In global mode an application may have windows on multiple monitors.
    // Prefer the local HWND for this Stage's click target, just as Dock does.
    // Keep the first-seen executable ordering stable while selecting that target.
    let mut seen=HashSet::<String>::new();
    let mut result=Vec::new();
    for w in all.iter().filter(|w|all_windows || rect_intersects(screen,w.rect)) {
        if !seen.insert(w.path.to_lowercase()){continue;}
        let local=all.iter().find(|a|a.path.eq_ignore_ascii_case(&w.path)
            && rect_intersects(screen,a.rect));
        let selected=local.unwrap_or(w);
        let count=all.iter().filter(|a|a.path.eq_ignore_ascii_case(&w.path)
            && (all_windows || rect_intersects(screen,a.rect))).count();
        result.push(Item{path:selected.path.clone(),title:selected.title.clone(),
            hwnd:selected.hwnd,count,pinned:false});
        if result.len()==5{break;}
    }
    result
}
fn stage_rect(screen:&Screen,side:&str)->RECT {
    let width=120.min(((screen.work.right-screen.work.left)/5).max(56));
    let edge=if side=="right"{screen.work.right-width-8}else{screen.work.left+8};
    let top=screen.work.top+((screen.work.bottom-screen.work.top)/6).min(160);
    let height=((screen.work.bottom-screen.work.top)-200).clamp(90,560);
    RECT{left:edge,top,right:edge+width,bottom:(top+height).min(screen.work.bottom-8)}
}
fn bar_rect(screen:&Screen,items:usize)->RECT {
    let width=(items.min(MAX_ITEMS) as i32*ITEM_WIDTH+220).clamp(296,(screen.bounds.right-screen.bounds.left-24).max(296));
    let x=screen.bounds.left+(screen.bounds.right-screen.bounds.left-width)/2;
    // Compatibility mode respects Explorer's work area (taskbar may be bottom or sides).
    let bottom=screen.work.bottom-7;
    RECT{left:x,top:bottom-HEIGHT,right:x+width,bottom}
}
unsafe fn recreate(state:&mut State) {
    let screens=monitors();
    let changed=screens.len()!=state.screens.len() || !screens.iter().zip(&state.screens).all(|(s,b)|same_rect(s.bounds,b.bounds)&&same_rect(s.work,b.work)&&s.primary==b.primary);
    if changed || !state.prefs.enabled{for b in state.bars.drain(..){DestroyWindow(b.hwnd);}}
    if changed || !state.prefs.stage_enabled{for s in state.stages.drain(..){DestroyWindow(s.hwnd);}}
    let class=wide(CLASS);
    state.screens=screens.clone();
    for screen in screens {
        if state.prefs.enabled && !state.bars.iter().any(|b|same_rect(b.bounds,screen.bounds)) {
        let items=item_list(&state.windows,screen.bounds,&state.prefs);
        let r=bar_rect(&screen,items.len());
        let hwnd=CreateWindowExW(WS_EX_TOPMOST|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,
            class.as_ptr(),wide("tuanpc-desk Dock").as_ptr(),WS_POPUP,
            r.left,r.top,r.right-r.left,r.bottom-r.top,
            ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
        if !hwnd.is_null(){
        ShowWindow(hwnd,SW_SHOWNOACTIVATE);
        state.bars.push(Bar{hwnd,bounds:screen.bounds,items,hovered:-1,hidden:false});
        }else{emit(Event::Error(format!("Create Dock window: {}",GetLastError())));}
        }
        if state.prefs.stage_enabled && !state.stages.iter().any(|s|same_rect(s.bounds,screen.bounds)) {
            let r=stage_rect(&screen,&state.prefs.stage_side);
            let stage_hwnd=CreateWindowExW(WS_EX_TOPMOST|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,
                class.as_ptr(),wide("tuanpc-desk Stage Manager").as_ptr(),WS_POPUP,
                r.left,r.top,r.right-r.left,r.bottom-r.top,
                ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
            if !stage_hwnd.is_null(){
                ShowWindow(stage_hwnd,SW_SHOWNOACTIVATE);
                state.stages.push(Stage{hwnd:stage_hwnd,bounds:screen.bounds,items:Vec::new(),hidden:false});
            }
        }
    }
}
unsafe fn refresh(state:&mut State) {
    state.windows=if state.suspended{Vec::new()}else{windows()};
    recreate(state);
    let foreground=GetForegroundWindow() as isize;let active_changed=foreground!=state.foreground;state.foreground=foreground;
    for b in &mut state.bars {
        let items=item_list(&state.windows,b.bounds,&state.prefs);
        let changed=b.items!=items || active_changed;
        b.items=items;
        let screen=state.screens.iter().find(|s|same_rect(s.bounds,b.bounds));
        if let Some(screen)=screen {
            let r=bar_rect(&screen,b.items.len());
            let mut actual:RECT=mem::zeroed();
            if GetWindowRect(b.hwnd,&mut actual)==0 || !same_rect(r,actual){SetWindowPos(b.hwnd,HWND_TOPMOST,r.left,r.top,r.right-r.left,r.bottom-r.top,SWP_NOACTIVATE);}
        }
        let fg=GetForegroundWindow();
        let mut fg_rect:RECT=mem::zeroed();
        let fg_class=if fg.is_null(){String::new()}else{
            let mut cls=[0u16;128];let n=GetClassNameW(fg,cls.as_mut_ptr(),128);
            String::from_utf16_lossy(&cls[..n.max(0) as usize])
        };
        let fullscreen=state.suspended || (!fg.is_null()&&!is_shell_class(&fg_class)&&GetWindowRect(fg,&mut fg_rect)!=0
            && fg_rect.left<=b.bounds.left && fg_rect.top<=b.bounds.top
            && fg_rect.right>=b.bounds.right && fg_rect.bottom>=b.bounds.bottom);
        if b.hidden!=fullscreen { ShowWindow(b.hwnd,if fullscreen{SW_HIDE}else{SW_SHOWNOACTIVATE});b.hidden=fullscreen; }
        if changed{InvalidateRect(b.hwnd,ptr::null(),0);}
    }
    let own:Vec<HWND>=state.bars.iter().map(|b|b.hwnd).chain(state.stages.iter().map(|s|s.hwnd)).collect();
    for stage in &mut state.stages {
        if let Some(screen)=state.screens.iter().find(|s|same_rect(s.bounds,stage.bounds)) {
            let r=stage_rect(screen,&state.prefs.stage_side);let mut actual:RECT=mem::zeroed();
            if GetWindowRect(stage.hwnd,&mut actual)==0 || !same_rect(r,actual){SetWindowPos(stage.hwnd,HWND_TOPMOST,r.left,r.top,r.right-r.left,r.bottom-r.top,SWP_NOACTIVATE);}
        }
        let items=stage_item_list(&state.windows,stage.bounds,state.prefs.all_windows);
        let changed=stage.items!=items || active_changed;stage.items=items;
        let fg=GetForegroundWindow();let mut rect:RECT=mem::zeroed();
        let hide=state.suspended || (!fg.is_null() && !own.contains(&fg)
            && GetWindowRect(fg,&mut rect)!=0 && rect.left<=stage.bounds.left && rect.top<=stage.bounds.top && rect.right>=stage.bounds.right && rect.bottom>=stage.bounds.bottom);
        if hide!=stage.hidden{ShowWindow(stage.hwnd,if hide{SW_HIDE}else{SW_SHOWNOACTIVATE});stage.hidden=hide;}
        if changed{InvalidateRect(stage.hwnd,ptr::null(),0);}
    }
}
unsafe fn fill(hdc:HDC,r:RECT,color:u32) {
    let brush=CreateSolidBrush(color);FillRect(hdc,&r,brush);DeleteObject(brush as HGDIOBJ);
}
unsafe fn label(hdc:HDC,x:i32,y:i32,width:i32,text:&str,color:u32) {
    SetTextColor(hdc,color);SetBkMode(hdc,TRANSPARENT as i32);
    let mut r=RECT{left:x,top:y,right:x+width,bottom:y+26};
    DrawTextW(hdc,wide(text).as_ptr(),-1,&mut r,DT_CENTER|DT_SINGLELINE|DT_END_ELLIPSIS|DT_VCENTER);
}
fn color(seed:&str)->u32 {
    let n=seed.bytes().fold(0x811c9dc5u32,|x,b|(x^b as u32).wrapping_mul(0x01000193));
    0x00353535|((n&0x3f)<<16)|(((n>>8)&0x3f)<<8)
}
unsafe fn icon(path:&str,small:bool)->HICON {
    let key=(path.to_lowercase(),small);
    if let Some(icon)=ICONS.with(|cache|cache.borrow().get(&key).copied()){return icon;}
    let mut file:SHFILEINFOW=mem::zeroed();
    let got=SHGetFileInfoW(wide(path).as_ptr(),0,&mut file,mem::size_of::<SHFILEINFOW>() as u32,
        SHGFI_ICON|if small{SHGFI_SMALLICON}else{SHGFI_LARGEICON});
    let icon=if got==0{ptr::null_mut()}else{file.hIcon};
    ICONS.with(|cache|{let mut cache=cache.borrow_mut();
        if cache.len()>=256{for (_,handle) in cache.drain(){if !handle.is_null(){DestroyIcon(handle);}}}
        cache.insert(key,icon);
    });icon
}
unsafe fn paint(hwnd:HWND){
    let mut ps:PAINTSTRUCT=mem::zeroed();let dc=BeginPaint(hwnd,&mut ps);
    let mut area:RECT=mem::zeroed();GetClientRect(hwnd,&mut area);
    fill(dc,area,0x00201b18);
    let stage=STATE.with(|cell|cell.borrow().stages.iter().find(|stage|stage.hwnd==hwnd).cloned());
        if let Some(stage)=stage {
            let foreground=GetForegroundWindow();
            let slot=((area.bottom-28)/5).clamp(24,95);
            label(dc,6,7,area.right-12,"STAGE",0x00eeeeee);
            for (i,item) in stage.items.iter().enumerate() {
                let y=34+i as i32*slot;
                fill(dc,RECT{left:7,top:y,right:area.right-7,bottom:y+slot-5},
                    if item.hwnd==foreground {0x007d655a}else{0x00332d28});
                let icon=icon(&item.path,true);
                if !icon.is_null(){
                    DrawIconEx(dc,13,y+5,icon,20,20,0,ptr::null_mut(),DI_NORMAL);
                }
                let n=basename(&item.path);
                label(dc,7,y+29,area.right-14,&n,0x00eeeeee);
                if item.count>1{label(dc,area.right-29,y+3,22,&item.count.to_string(),0x00ffffff);}
            }
        }
    let bar=STATE.with(|cell|cell.borrow().bars.iter().find(|b|b.hwnd==hwnd).cloned());
    if let Some(bar)=bar{
        for (i,item) in bar.items.iter().enumerate() {
            let left=12+i as i32*ITEM_WIDTH;
            let zoom=if bar.hovered==i as i32{6}else{0};
            let box_rect=RECT{left:left+zoom/2,top:9-zoom/2,right:left+48+zoom,bottom:50+zoom/2};
            fill(dc,box_rect,color(&item.path));
            let title=basename(&item.path);
            let icon=icon(&item.path,false);
            if !icon.is_null() {
                DrawIconEx(dc,left+7,11,icon,34,34,0,ptr::null_mut(),DI_NORMAL);
            } else {
                let initial=title.chars().take(2).collect::<String>().to_uppercase();
                label(dc,left,18,54,&initial,0x00ffffff);
            }
            if item.count>0 {
                fill(dc,RECT{left:left+23,top:56,right:left+29,bottom:60},0x00ffffff);
            }
            if item.pinned && item.count==0 {
                fill(dc,RECT{left:left+24,top:56,right:left+28,bottom:60},0x00888888);
            }
        }
        let x=area.right-210;
        label(dc,x,20,42,"Wi-Fi",0x00ffffff);
        label(dc,x+46,20,40,"BT",0x00ffffff);
        label(dc,x+90,20,45,"Âm",0x00ffffff);
        label(dc,x+142,20,62,"Taskbar",0x00ffffff);
    }
    EndPaint(hwnd,&ps);
}
unsafe fn open(uri:&str) {
    ShellExecuteW(ptr::null_mut(),wide("open").as_ptr(),wide(uri).as_ptr(),ptr::null(),ptr::null(),SW_SHOWNORMAL);
}
unsafe fn activate(item:&Item) {
    if !item.hwnd.is_null() && IsWindow(item.hwnd)!=0 {
        if IsIconic(item.hwnd)!=0{ShowWindowAsync(item.hwnd,SW_RESTORE);}
        SetForegroundWindow(item.hwnd);
    } else {open(&item.path);}
}
unsafe fn native_menu(hwnd:HWND,bar_index:usize,item_index:usize) {
    let menu=CreatePopupMenu();if menu.is_null(){return;}
    let prefs=STATE.with(|s|s.borrow().prefs.clone());
    let selected=STATE.with(|s|s.borrow().bars.get(bar_index).and_then(|b|b.items.get(item_index)).cloned());
    if let Some(item)=selected.as_ref(){
        AppendMenuW(menu,MF_STRING,if item.pinned{MENU_UNPIN}else{MENU_PIN},wide(if item.pinned{"Bỏ ghim"}else{"Ghim ứng dụng"}).as_ptr());
        if !item.hwnd.is_null() {
            AppendMenuW(menu,MF_STRING,MENU_MINIMIZE,wide("Thu nhỏ cửa sổ").as_ptr());
            AppendMenuW(menu,MF_STRING,MENU_MAXIMIZE,wide("Phóng to").as_ptr());
            AppendMenuW(menu,MF_STRING,MENU_RESTORE,wide("Khôi phục kích thước").as_ptr());
            AppendMenuW(menu,MF_STRING,MENU_SNAP_LEFT,wide("Chia nửa trái").as_ptr());
            AppendMenuW(menu,MF_STRING,MENU_SNAP_RIGHT,wide("Chia nửa phải").as_ptr());
            AppendMenuW(menu,MF_STRING,MENU_FULLSCREEN,wide("Đầy màn hình (borderless thử nghiệm)").as_ptr());
        }
        AppendMenuW(menu,MF_SEPARATOR,0,ptr::null());
    }
    AppendMenuW(menu,MF_STRING|if prefs.all_windows{MF_CHECKED}else{0},MENU_ALL,wide("Hiện ứng dụng từ mọi màn hình").as_ptr());
    AppendMenuW(menu,MF_STRING|if !prefs.all_windows{MF_CHECKED}else{0},MENU_LOCAL,wide("Chỉ ứng dụng trên màn hình này").as_ptr());
    AppendMenuW(menu,MF_SEPARATOR,0,ptr::null());
    for (id,text) in [(MENU_STAGE_LEFT,"Stage Manager bên trái"),(MENU_STAGE_RIGHT,"Stage Manager bên phải"),(MENU_STAGE_OFF,"Tắt Stage Manager")] {
        AppendMenuW(menu,MF_STRING|if (id==MENU_STAGE_LEFT && prefs.stage_side=="left")||
            (id==MENU_STAGE_RIGHT && prefs.stage_enabled && prefs.stage_side=="right")||(id==MENU_STAGE_OFF && !prefs.stage_enabled){MF_CHECKED}else{0},
            id,wide(text).as_ptr());
    }
    AppendMenuW(menu,MF_SEPARATOR,0,ptr::null());
    for (id,title) in [(MENU_WIFI,"Cài đặt Wi-Fi"),(MENU_BLUETOOTH,"Bluetooth"),(MENU_SOUND,"Âm thanh"),
      (MENU_NOTIFICATIONS,"Thông báo"),(MENU_EXPLORER,"Mở File Explorer")] {
        AppendMenuW(menu,MF_STRING,id,wide(title).as_ptr());
    }
    AppendMenuW(menu,MF_SEPARATOR,0,ptr::null());
    AppendMenuW(menu,MF_STRING,MENU_QUIT,wide("Đóng Dock (giữ hình nền)").as_ptr());
    let mut point:POINT=mem::zeroed();GetCursorPos(&mut point);
    SetForegroundWindow(hwnd);
    let cmd=TrackPopupMenu(menu,TPM_RETURNCMD|TPM_RIGHTBUTTON,point.x,point.y,0,hwnd,ptr::null()) as usize;
    DestroyMenu(menu);
    match cmd {
        MENU_ALL|MENU_LOCAL => emit(Event::Edit(Edit::AllWindows(cmd==MENU_ALL))),
        MENU_PIN|MENU_UNPIN => if let Some(item)=selected {
            emit(Event::Edit(if cmd==MENU_PIN{Edit::Pin(item.path)}else{Edit::Unpin(item.path)}));
        },
        MENU_STAGE_LEFT|MENU_STAGE_RIGHT=>emit(Event::Edit(Edit::StageSide(if cmd==MENU_STAGE_LEFT{"left"}else{"right"}.into()))),
        MENU_STAGE_OFF=>emit(Event::Edit(Edit::StageEnabled(false))),
        MENU_MINIMIZE|MENU_MAXIMIZE|MENU_RESTORE|MENU_SNAP_LEFT|MENU_SNAP_RIGHT|MENU_FULLSCREEN=>if let Some(item)=selected{
            if !item.hwnd.is_null()&&IsWindow(item.hwnd)!=0 {
                match cmd {
                    MENU_MINIMIZE=>{ShowWindowAsync(item.hwnd,SW_MINIMIZE);}
                    MENU_MAXIMIZE=>{ShowWindowAsync(item.hwnd,SW_MAXIMIZE);}
                    MENU_RESTORE=>{ShowWindowAsync(item.hwnd,SW_RESTORE);}
                    MENU_SNAP_LEFT|MENU_SNAP_RIGHT|MENU_FULLSCREEN=>{
                        let screen=monitors().into_iter().find(|s|{
                            let mut r:RECT=mem::zeroed();GetWindowRect(item.hwnd,&mut r)!=0&&rect_intersects(r,s.bounds)
                        });
                        if let Some(screen)=screen {
                            ShowWindowAsync(item.hwnd,SW_RESTORE);
                            let mut r=if cmd==MENU_FULLSCREEN{screen.bounds}else{screen.work};
                            if cmd!=MENU_FULLSCREEN{
                                let middle=r.left+(r.right-r.left)/2;
                                if cmd==MENU_SNAP_LEFT{r.right=middle;}else{r.left=middle;}
                            }
                            SetWindowPos(item.hwnd,HWND_TOP,r.left,r.top,r.right-r.left,r.bottom-r.top,SWP_SHOWWINDOW|SWP_ASYNCWINDOWPOS);
                        }
                    }
                    _=>{}
                }
            }
        },
        MENU_WIFI=>open("ms-settings:network-wifi"),
        MENU_BLUETOOTH=>open("ms-settings:bluetooth"),
        MENU_SOUND=>open("ms-settings:sound"),
        MENU_NOTIFICATIONS=>open("ms-settings:notifications"),
        MENU_EXPLORER=>open("explorer.exe"),
        MENU_QUIT=>emit(Event::Edit(Edit::DockEnabled(false))),
        _=>{}
    }
}
unsafe extern "system" fn proc(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
    match msg {
        WM_CONFIG=>{drain_commands();0},
        WM_TICK=>{SetTimer(hwnd,2,60,None);0},
        WM_ERASEBKGND=>1,
        WM_PAINT=>{paint(hwnd);0},
        WM_MOUSEACTIVATE=>MA_NOACTIVATE as LRESULT,
        WM_TIMER=>{if w==1||w==2{if w==2{KillTimer(hwnd,2);DIRTY.with(|d|d.set(false));}update_state(|s|refresh(s));}0},
        WM_DISPLAYCHANGE|WM_SETTINGCHANGE|WM_DPICHANGED=>{queue_refresh();0},
        WM_MOUSEMOVE=>{
            let x=(l as u32&0xffff) as i16 as i32;
            let changed=STATE.with(|s|{let mut a=s.borrow_mut();if let Some(b)=a.bars.iter_mut().find(|b|b.hwnd==hwnd){
                let idx=if x>=12 && x<12+MAX_ITEMS as i32*ITEM_WIDTH{(x-12)/ITEM_WIDTH}else{-1};
                if idx!=b.hovered {b.hovered=idx;return true;}
            }false});if changed{InvalidateRect(hwnd,ptr::null(),0);}0
        }
        WM_LBUTTONUP=>{
            let y=((l as u32>>16)&0xffff) as i16 as i32;
            let mut area:RECT=mem::zeroed();GetClientRect(hwnd,&mut area);let slot=((area.bottom-28)/5).clamp(24,95);
            let stage_item=STATE.with(|s|{
                let a=s.borrow();a.stages.iter().find(|stage|stage.hwnd==hwnd).and_then(|stage|{
                    if y<34 {None}else{stage.items.get(((y-34)/slot) as usize).cloned()}
                })
            });
            if let Some(item)=stage_item {activate(&item);return 0;}
            let is_stage=STATE.with(|s|s.borrow().stages.iter().any(|stage|stage.hwnd==hwnd));
            if is_stage{return 0;}
            let x=(l as u32&0xffff) as i16 as i32;
            let selected=STATE.with(|s|s.borrow().bars.iter().find(|b|b.hwnd==hwnd).and_then(|b|b.items.get(((x-12).max(0)/ITEM_WIDTH) as usize)).cloned());
            let max=STATE.with(|s|s.borrow().bars.iter().find(|b|b.hwnd==hwnd).map(|b|b.items.len()).unwrap_or(0));
            if x>=12 && (x-12)/ITEM_WIDTH <max as i32 {if let Some(item)=selected{activate(&item);}}
            else {
                let mut area:RECT=mem::zeroed();GetClientRect(hwnd,&mut area);
                let x0=area.right-210;
                if x>=x0 && x<x0+46{open("ms-settings:network-wifi");}
                else if x>=x0+46 && x<x0+88{open("ms-settings:bluetooth");}
                else if x>=x0+88 && x<x0+140{open("ms-settings:sound");}
                else if x>=x0+140{open("ms-settings:taskbar");}
            }
            0
        }
        WM_RBUTTONUP=>{
            if STATE.with(|s|s.borrow().stages.iter().any(|stage|stage.hwnd==hwnd)){return 0;}
            let x=(l as u32&0xffff) as i16 as i32;
            let (bar,index)=STATE.with(|s|{let a=s.borrow();let i=a.bars.iter().position(|b|b.hwnd==hwnd).unwrap_or(0);
              (i,if x>=12{((x-12)/ITEM_WIDTH) as usize}else{usize::MAX})});
            native_menu(hwnd,bar,index);0
        }
        WM_CLOSE=>{
            if CONTROL.with(|c|c.get())==hwnd{EndMenu();PostQuitMessage(0);}
            else if STATE.with(|s|s.borrow().stages.iter().any(|stage|stage.hwnd==hwnd)){emit(Event::Edit(Edit::StageEnabled(false)));}
            else{emit(Event::Edit(Edit::DockEnabled(false)));}0
        },
        _=>DefWindowProcW(hwnd,msg,w,l),
    }
}
fn queue_refresh() {
    if !DIRTY.with(|d|d.replace(true)) {
        let control=CONTROL.with(|c|c.get());
        if !control.is_null(){unsafe{PostMessageW(control,WM_TICK,0,0);}}
    }
}
unsafe extern "system" fn window_event(_:HWINEVENTHOOK,_event:u32,_hwnd:HWND,object:i32,_child:i32,_thread:u32,_time:u32) {
    if object==0 {queue_refresh();}
}
unsafe fn drain_commands() {
    let commands:Vec<_>=COMMANDS.with(|c|c.borrow().as_ref().map(|rx|rx.try_iter().collect()).unwrap_or_default());
    for command in commands {
        match command {
            Command::Configure(options)=>update_state(|state|{
                state.prefs=options;refresh(state);
            }),
            Command::Suspend(on)=>update_state(|state|{if state.suspended!=on{state.suspended=on;refresh(state);}}),
            Command::Stop=>{EndMenu();PostQuitMessage(0);},
            #[cfg(test)] Command::Inspect(tx)=>{let handles=STATE.with(|s|{let s=s.borrow();s.bars.iter().map(|b|b.hwnd as usize).chain(s.stages.iter().map(|s|s.hwnd as usize)).collect()});let _=tx.send(handles);},
        }
    }
}
fn run(options:Options,commands:mpsc::Receiver<Command>,events:mpsc::Sender<Event>,ready:mpsc::SyncSender<Result<usize,String>>) {
    unsafe {
        use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
        let com=CoInitializeEx(None,COINIT_APARTMENTTHREADED);
        let class=wide(CLASS);
        let mut wc:WNDCLASSEXW=mem::zeroed();
        wc.cbSize=mem::size_of::<WNDCLASSEXW>() as u32;wc.lpfnWndProc=Some(proc);
        wc.hInstance=GetModuleHandleW(ptr::null());wc.lpszClassName=class.as_ptr();
        wc.hCursor=LoadCursorW(ptr::null_mut(),IDC_ARROW);
        if RegisterClassExW(&wc)==0 && GetLastError()!=ERROR_CLASS_ALREADY_EXISTS{
            let _=ready.send(Err(format!("Register Dock class: {}",GetLastError())));if com.is_ok(){CoUninitialize();}return;
        }
        let control=CreateWindowExW(WS_EX_TOOLWINDOW,class.as_ptr(),wide("Deskland Dock controller").as_ptr(),WS_POPUP,0,0,0,0,ptr::null_mut(),ptr::null_mut(),wc.hInstance,ptr::null());
        if control.is_null(){let _=ready.send(Err(format!("Create Dock controller: {}",GetLastError())));if com.is_ok(){CoUninitialize();}return;}
        CONTROL.with(|c|c.set(control));DIRTY.with(|c|c.set(false));
        COMMANDS.with(|c|{c.replace(Some(commands));});EVENTS.with(|e|{e.replace(Some(events));});
        update_state(|state|{state.prefs=options;refresh(state);});
        let mut hooks=Vec::new();
        for (low,high) in [(EVENT_SYSTEM_FOREGROUND,EVENT_SYSTEM_FOREGROUND),(EVENT_SYSTEM_MINIMIZESTART,EVENT_SYSTEM_MINIMIZEEND),
            (EVENT_OBJECT_CREATE,EVENT_OBJECT_HIDE),(EVENT_OBJECT_LOCATIONCHANGE,EVENT_OBJECT_NAMECHANGE)] {
            let hook=SetWinEventHook(low,high,ptr::null_mut(),Some(window_event),0,0,WINEVENT_OUTOFCONTEXT|WINEVENT_SKIPOWNPROCESS);
            if !hook.is_null(){hooks.push(hook);}else{emit(Event::Error(format!("Window event hook fallback: {}",GetLastError())));}
        }
        // One low-frequency recovery timer; ordinary changes arrive via coalesced
        // WinEvent messages. Unchanged windows do not invalidate any pixels.
        SetTimer(control,1,5000,None);
        if ready.send(Ok(control as usize)).is_err(){PostQuitMessage(0);}
        let mut msg:MSG=mem::zeroed();
        while GetMessageW(&mut msg,ptr::null_mut(),0,0)>0{TranslateMessage(&msg);DispatchMessageW(&msg);}
        for hook in hooks{UnhookWinEvent(hook);}KillTimer(control,1);KillTimer(control,2);
        update_state(|s|{for b in s.bars.drain(..){DestroyWindow(b.hwnd);}for b in s.stages.drain(..){DestroyWindow(b.hwnd);}});
        ICONS.with(|c|{for (_,icon) in c.borrow_mut().drain(){if !icon.is_null(){DestroyIcon(icon);}}});
        CONTROL.with(|c|c.set(ptr::null_mut()));DestroyWindow(control);
        COMMANDS.with(|c|{c.take();});EVENTS.with(|e|{e.take();});STATE.with(|s|{s.take();});
        if com.is_ok(){CoUninitialize();}
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    fn rect(a:[i32;4])->RECT{RECT{left:a[0],top:a[1],right:a[2],bottom:a[3]}}
    #[test] fn mixed_coordinate_overlap(){assert!(rect_intersects(rect([-1200,-200,0,900]),rect([-100,0,100,100])));assert!(!rect_intersects(rect([-1200,-200,0,900]),rect([0,0,100,100])));}
    #[test] fn distinct_monitor_geometry(){let s=Screen{bounds:rect([-1920,0,0,1080]),work:rect([-1920,0,0,1040]),primary:false};let r=bar_rect(&s,5);assert!(r.left<0 && r.right<=0 && r.bottom<=1040);}
    #[test] fn stage_geometry_on_negative_monitor(){
        let sc=Screen{bounds:rect([-1920,-120,0,1080]),work:rect([-1920,-120,0,1040]),primary:false};
        let left=stage_rect(&sc,"left");let right=stage_rect(&sc,"right");
        assert!(left.left<right.left && left.left>=sc.work.left && right.right<=sc.work.right);
        assert!(left.top>=sc.work.top && left.bottom<=sc.work.bottom);
    }
    #[test] fn stage_respects_monitor_filter_and_counts(){
        let make=|hwnd:usize,path:&str,r:[i32;4]|AppWindow{
            hwnd:hwnd as HWND,title:path.into(),path:path.into(),rect:rect(r)};
        let windows=vec![
            make(1,"C:\\\\Apps\\\\alpha.exe",[-1800,0,-100,900]),
            make(2,"c:\\\\apps\\\\ALPHA.EXE",[100,0,700,900]),
            make(3,"C:\\\\Apps\\\\beta.exe",[100,0,700,900])];
        let local=stage_item_list(&windows,rect([-1920,0,0,1080]),false);
        assert_eq!(local.len(),1);assert_eq!(local[0].count,1);
        let global=stage_item_list(&windows,rect([-1920,0,0,1080]),true);
        assert_eq!(global.len(),2);assert_eq!(global[0].count,2);
    }
    #[test] fn global_stage_prefers_local_window_for_activation(){
        let make=|hwnd:usize,path:&str,r:[i32;4]|AppWindow{
            hwnd:hwnd as HWND,title:format!("window {hwnd}"),path:path.into(),rect:rect(r)};
        let windows=vec![
            make(1,"C:\\Apps\\alpha.exe",[100,0,700,900]),
            make(2,"c:\\apps\\ALPHA.EXE",[-1800,0,-100,900]),
            make(3,"C:\\Apps\\beta.exe",[100,0,700,900])];
        let secondary=rect([-1920,0,0,1080]);
        let global=stage_item_list(&windows,secondary,true);
        assert_eq!(global.len(),2);
        assert_eq!(global[0].hwnd,2 as HWND);
        assert_eq!(global[0].count,2);
        assert_eq!(global[1].hwnd,3 as HWND);
        let local=stage_item_list(&windows,secondary,false);
        assert_eq!(local.len(),1);
        assert_eq!(local[0].hwnd,2 as HWND);
        assert_eq!(local[0].count,1);
    }
    #[test] fn pinned_app_respects_local_monitor_and_deduplicates(){
        let make=|hwnd:usize,r:[i32;4]|AppWindow{hwnd:hwnd as HWND,title:format!("window {hwnd}"),path:"C:\\\\Apps\\\\alpha.exe".into(),rect:rect(r)};
        let windows=vec![make(1,[100,0,700,900]),make(2,[-1800,0,-100,900])];
        let mut prefs=Preferences{pinned:vec!["c:\\\\apps\\\\ALPHA.EXE".into(),"C:\\\\Apps\\\\alpha.exe".into()],all_windows:false,stage_side:"left".into(),..Default::default()};
        let local=item_list(&windows,rect([-1920,0,0,1080]),&prefs);
        assert_eq!(local.len(),1);assert_eq!(local[0].hwnd,2 as HWND);assert_eq!(local[0].count,1);
        let empty=item_list(&windows,rect([2000,0,3000,1080]),&prefs);
        assert_eq!(empty.len(),1);assert!(empty[0].hwnd.is_null());assert_eq!(empty[0].count,0);
        prefs.all_windows=true;
        let global=item_list(&windows,rect([-1920,0,0,1080]),&prefs);
        assert_eq!(global.len(),1);assert_eq!(global[0].hwnd,2 as HWND);assert_eq!(global[0].count,2);
    }
    #[test] fn settings_serialization(){let p=Preferences{pinned:vec!["C:\\A\\foo.exe".into()],all_windows:true,stage_side:"right".into(),..Default::default()};assert_eq!(Preferences::from_json(&p.json()),p);}
    #[test] fn unpinned_global_dock_prefers_local_window_and_keeps_remote_fallback(){
        let windows=vec![
            AppWindow{hwnd:1 as HWND,title:"remote".into(),path:"C:\\Apps\\alpha.exe".into(),rect:rect([100,0,700,900])},
            AppWindow{hwnd:2 as HWND,title:"local".into(),path:"c:\\apps\\ALPHA.EXE".into(),rect:rect([-1800,0,-100,900])},
            AppWindow{hwnd:3 as HWND,title:"remote beta".into(),path:"C:\\Apps\\beta.exe".into(),rect:rect([100,0,700,900])}];
        let prefs=Preferences{all_windows:true,stage_side:"left".into(),..Default::default()};
        let global=item_list(&windows,rect([-1920,0,0,1080]),&prefs);
        assert_eq!(global.len(),2);assert_eq!(global[0].hwnd,2 as HWND);
        assert_eq!(global[0].title,"local");assert_eq!(global[0].count,2);
        assert_eq!(global[1].hwnd,3 as HWND);
        let local=item_list(&windows,rect([-1920,0,0,1080]),&Preferences{all_windows:false,..prefs});
        assert_eq!(local.len(),1);assert_eq!(local[0].hwnd,2 as HWND);assert_eq!(local[0].count,1);
    }
}
