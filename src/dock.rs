//! Native Windows multi-monitor dock (opt-in; does not impersonate Shell_TrayWnd).
//! Managed UI thread, started/configured/stopped by Deskland's existing main loop.
//! Compatibility mode deliberately leaves Explorer's notification area intact.
use crate::desktop::wide;
use std::{cell::{Cell,RefCell},collections::{HashMap,HashSet},mem,ptr,sync::mpsc,time::{Duration,Instant}};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Devices::Display::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute,DWMWA_CLOAKED,DWMWA_EXTENDED_FRAME_BOUNDS};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{OpenProcess,QueryFullProcessImageNameW,PROCESS_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::Win32::UI::Accessibility::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent,TRACKMOUSEEVENT,TME_LEAVE,SetFocus,SetCapture,GetCapture,ReleaseCapture,VK_LEFT,VK_RIGHT,VK_UP,VK_DOWN,VK_RETURN,VK_ESCAPE,VK_HOME,VK_END,VK_PRIOR,VK_NEXT,VK_APPS,VK_F10,VK_SHIFT,GetKeyState};

mod options;
mod runtime;
mod glass_probe;
mod appearance;
mod geometry;
mod motion;
mod surface;
mod icon_atlas;
mod previews;
mod workspaces;
mod perf;
mod stage_surface;
mod utilities;
mod wifi;
mod control_center;
mod panel_chrome;
mod appbar;
pub use utilities::{HostAction,Profile as UtilityProfile,Widget as UtilityWidget};
pub use control_center::MetricOptions;
pub use glass_probe::probe as glass_probe;
pub use appearance::{Appearance,DockSide,Theme};
pub use options::{Options,Edit,DisplayMode,DisplayTarget,StageStyle,WindowMode,WindowPolicy};
pub use runtime::{Runtime,Event};
use runtime::Command;
type Preferences=Options;

const CLASS:&str="TuanpcDeskNativeDock";
const PREVIEW_CLASS:&str="TuanpcDeskStagePreview";
const WM_TICK:u32=WM_APP+50;
const WM_CONFIG:u32=WM_APP+51;
const WM_ICON_CHANGED:u32=WM_APP+52;
const WM_UTILITY_READY:u32=WM_APP+53;
const MOTION_TIMER:usize=4;
const HIDE_TIMER:usize=5;
const WORKSPACE_TIMER:usize=6;
const STAGE_MOTION_TIMER:usize=7;
const STATUS_TIP_TIMER:usize=8;
const FEEDBACK_TIMER:usize=9;
const WM_DOCK_MOUSELEAVE:u32=0x02A3;
// winuser.h macro: HSHELL_REDRAW | HSHELL_HIGHBIT.
const HSHELL_FLASH:usize=6 | 0x8000;
const MOUSE_LEFT_DOWN:WPARAM=0x0001; // MK_LBUTTON in WM_MOUSEMOVE
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
const MENU_EXPAND:usize=519;
const MENU_ADD_APP:usize=520;
const MENU_MORE_APPS:usize=521;
const MENU_WINDOW_FIRST:usize=2000;
const MENU_MOVE_FIRST:usize=2100;
const STAGE_RAIL_CLASS:&str="TuanpcDeskStageReveal";

#[derive(Clone)]
pub struct Display {
    pub id: String, pub name: String, pub bounds: RECT, pub work: RECT,
    pub primary: bool, pub dpi: u32,
}
type Screen = Display;
#[derive(Clone)]
struct AppWindow { hwnd:HWND, title:String, path:String, rect:RECT }
#[derive(Clone,Debug,PartialEq,Eq)]
struct Item { path:String, title:String, hwnd:HWND, count:usize, pinned:bool }
#[derive(Clone)]
struct Bar {
    hwnd:HWND, bounds:RECT, items:Vec<Item>, hovered:i32, hidden:bool,
    layout:Option<geometry::Layout>, pinned_widgets:Vec<UtilityWidget>, motion:Motion, collapsed:bool, auto_hide:bool,
    status_hover:Option<Option<UtilityWidget>>, pressed:Option<Press>, keyboard:Option<usize>, previous_focus:isize, pointer:Option<(f32,f32)>,
    reveal:motion::Spring, feedback:Option<Feedback>, active:Option<usize>, minimized:u16, attention:u16, metric_repaint:Option<Instant>,
}
#[derive(Clone,Debug,PartialEq,Eq)]
enum DockTarget { App(String), Launcher, Overflow, Utility(UtilityWidget) }
#[derive(Clone)]
struct Press { target:DockTarget, x:i32, y:i32, dragged:bool }
impl Press {
    fn new(target:DockTarget,x:i32,y:i32)->Self {
        Self{target,x,y,dragged:false}
    }
    fn moved(&self,x:f32,y:f32,scale:f32)->bool {
        self.dragged||(x-self.x as f32).abs()+(y-self.y as f32).abs()>9.0*scale
    }
    fn track(&mut self,x:f32,y:f32,scale:f32){
        self.dragged=self.moved(x,y,scale);
    }
    fn click(&self,released:Option<DockTarget>,x:f32,y:f32,scale:f32)->Option<DockTarget>{
        released.filter(|target|*target==self.target && !self.moved(x,y,scale))
    }
}
#[derive(Clone, Copy)]
enum FeedbackKind { Launch, Restore, Minimize, Failed }
#[derive(Clone)]
struct Feedback { path:String, kind:FeedbackKind, start:std::time::Instant }
#[derive(Clone,Default)]
struct Motion {
    focus:Option<motion::Spring>, target:Option<f32>, zoom:motion::Spring, at:Option<std::time::Instant>, scheduled:bool,
}
impl Motion {
    fn focus_value(&self)->Option<f32>{self.focus.map(|f|f.value)}
    fn tick(&mut self, now:std::time::Instant, speed:u32, reveal:&mut motion::Spring, shown:bool)->bool{
        let dt=self.at.replace(now).map_or(0.016,|t|now.duration_since(t).as_secs_f32().clamp(0.001,0.045));
        let speed=speed as f32/100.0;
        let goal=if self.target.is_some(){1.0}else{0.0};
        let mut unsettled=self.zoom.advance(goal,dt,25.0*speed,0.0025);
        if let Some(target)=self.target{
            let focus=self.focus.get_or_insert_with(||motion::Spring::rest(target));
            unsettled|=focus.advance(target,dt,33.0*speed,0.28);
        }
        if self.target.is_none() && self.zoom.value==0.0 { self.focus=None; }
        unsettled|=reveal.advance(if shown{1.0}else{0.0},dt,28.0*speed,0.003);
        unsettled
    }
}
impl Bar {
    fn target(&self,x:f32,y:f32)->Option<DockTarget>{
        let layout=self.layout.as_ref()?;
        if self.hidden||self.collapsed{return None;}
        if let Some(item)=layout.hit(x,y).and_then(|i|self.items.get(i)){return Some(DockTarget::App(item.path.clone()));}
        if layout.is_launch_button(x,y){return Some(DockTarget::Launcher);}
        if layout.is_utility_button(x,y){return Some(DockTarget::Overflow);}
        layout.inline_at(x,y).map(DockTarget::Utility)
    }
    fn fallback_target(&self,x:i32,y:i32,width:i32,utilities:bool)->Option<DockTarget>{
        if self.hidden|| !(9..60).contains(&y)||x<0||x>=width{return None;}
        if utilities&&x>=width-52{return Some(DockTarget::Overflow);}
        let slot=x.checked_sub(12).filter(|x|*x>=0)?/ITEM_WIDTH;
        self.items.get(slot as usize).map(|item|DockTarget::App(item.path.clone()))
    }
    fn drop_at(&self,x:f32,y:f32)->Option<usize>{
        let press=self.pressed.as_ref()?;
        let DockTarget::App(path)=&press.target else{return None;};
        let layout=self.layout.as_ref()?;
        let pins=self.items.iter().take_while(|item|item.pinned).count();
        let from=self.items[..pins].iter().position(|item|item.path==*path)?;
        if !press.moved(x,y,layout.scale)||self.hidden||self.collapsed
            ||layout.is_launch_button(x,y)||layout.is_utility_button(x,y)||layout.inline_at(x,y).is_some()
            ||layout.hit(x,y).is_some_and(|i|i>=pins){return None;}
        layout.pin_boundary(x,y,pins).filter(|&i|i!=from&&i!=from+1)
    }
    fn drop_target(&self)->Option<usize>{
        let (x,y)=self.pointer?;
        self.drop_at(x,y)
    }
    fn schedule(&mut self)->bool{
        let start=!self.motion.scheduled;
        self.motion.scheduled=true;
        start
    }
    fn collapse(&mut self, collapsed:bool, reduced:bool)->bool {
        if self.collapsed==collapsed{return false;}
        self.collapsed=collapsed;
        self.metric_repaint=None;
        if reduced {self.reveal=motion::Spring::rest(if collapsed{0.0}else{1.0});false}
        else {self.schedule()}
    }
}
#[derive(Default)]
struct StageEmphasis {
    index:Option<usize>,
    strength:u8,
    from:u8,
    start:Option<std::time::Instant>,
}
impl StageEmphasis {
    fn change(&mut self,hover:Option<usize>,reduced:bool)->bool{
        if reduced {
            self.index=hover;self.strength=if hover.is_some(){255}else{0};self.start=None;return false;
        }
        if let Some(i)=hover{
            if self.index!=Some(i){self.strength=0;}
            self.index=Some(i);
        }
        if self.index.is_none(){return false;}
        self.from=self.strength;
        self.start=Some(std::time::Instant::now());
        true
    }
    fn tick(&mut self,hover:Option<usize>,elapsed:f32)->bool{
        let t=elapsed.clamp(0.0,1.0);
        let progress=1.0-(1.0-t).powi(3);
        let goal=if hover.is_some(){255.0}else{0.0};
        self.strength=(self.from as f32+(goal-self.from as f32)*progress).round() as u8;
        if t>=1.0{self.start=None;self.index=hover;false}else{true}
    }
    fn highlight(&self,keyboard:Option<usize>)->Option<(usize,u8)>{
        keyboard.map(|i|(i,255)).or_else(||self.index.map(|i|(i,self.strength)))
    }
}
struct Stage { hwnd:HWND, rail:HWND, collapsed:bool, auto_hide:bool, preview_hwnd:HWND, preview_boxes:Vec<RECT>, preview_visible:bool, layered:bool, opacity:u32, bounds:RECT, display:String, items:Vec<Item>, groups:Vec<Option<u64>>, selected:Option<u64>, layout:Option<stage_surface::Layout>, page:usize, pages:usize, hidden:bool, hover:Option<usize>, emphasis:StageEmphasis, keyboard:Option<usize>, previous_focus:usize, previews:previews::Previews,
    resting:RECT, slide:Option<(std::time::Instant,i32)>, offset:i32, pressed:Option<StageTarget> }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum StageTarget { Group(u64), Window(usize,u32,u32), Page(usize) }
impl Stage {
    unsafe fn target(&self,x:i32,y:i32)->Option<StageTarget>{
        if self.hidden{return None;}
        let layout=self.layout?;
        if let Some(i)=layout.hit(x,y) {
            if let Some(id)=self.groups.get(i).copied().flatten(){return Some(StageTarget::Group(id));}
            let hwnd=self.items.get(i)?.hwnd;
            let (pid,tid)=window_owner(hwnd)?;
            return Some(StageTarget::Window(hwnd as usize,pid,tid));
        }
        layout.pages(self.pages)?.target(x,y,self.page).map(StageTarget::Page)
    }
    unsafe fn begin_slide(&mut self,reduced:bool){
        let Some(layout)=self.layout else{return;};
        if reduced{return;}
        let dpi=windows_sys::Win32::UI::HiDpi::GetDpiForWindow(self.hwnd).clamp(72,288) as i32;
        let travel=(6*dpi+48)/96;
        self.offset=if layout.right{travel}else{-travel};
        self.slide=Some((std::time::Instant::now(),self.offset));
        if SetTimer(self.hwnd,STAGE_MOTION_TIMER,16,None)==0{self.offset=0;self.slide=None;}
    }
    unsafe fn position(&self){
        position_stage(self.hwnd,self.preview_hwnd,self.resting,self.offset);
    }
}
unsafe fn position_stage(hwnd:HWND,preview:HWND,rect:RECT,offset:i32){
    let x=rect.left+offset;
    let (width,height)=(rect.right-rect.left,rect.bottom-rect.top);
    let flags=SWP_NOACTIVATE|SWP_NOZORDER|SWP_NOOWNERZORDER;
    if !preview.is_null(){
        let mut batch=BeginDeferWindowPos(2);
        if !batch.is_null(){batch=DeferWindowPos(batch,hwnd,ptr::null_mut(),x,rect.top,width,height,flags);}
        if !batch.is_null(){batch=DeferWindowPos(batch,preview,ptr::null_mut(),x,rect.top,width,height,flags);}
        if !batch.is_null()&&EndDeferWindowPos(batch)!=0{return;}
        SetWindowPos(preview,ptr::null_mut(),x,rect.top,width,height,flags);
    }
    SetWindowPos(hwnd,ptr::null_mut(),x,rect.top,width,height,flags);
}
#[derive(Default)]
struct State { bars:Vec<Bar>, stages:Vec<Stage>, windows:Vec<AppWindow>, prefs:Preferences, screens:Vec<Screen>, suspended:bool, foreground:isize, workspaces:workspaces::Workspaces, material:Option<surface::Material>, appbars:appbar::Bars, reservation_failed:bool }
thread_local! {
    static STATE:RefCell<State>=RefCell::new(State::default());
    static CONTROL:Cell<HWND>=const{Cell::new(ptr::null_mut())};
    static OWNER:Cell<HWND>=const{Cell::new(ptr::null_mut())};
    static DIRTY:Cell<bool>=const{Cell::new(false)};
    static SHELL_RESTARTED:Cell<bool>=const{Cell::new(false)};
    static SHELL_MESSAGE:Cell<u32>=const{Cell::new(0)};
    static APPBAR_DIRTY:Cell<bool>=const{Cell::new(false)};
    static DISPLAY_DIRTY:Cell<bool>=const{Cell::new(false)};
    static WORKSPACE_TIMER_ARMED:Cell<bool>=const{Cell::new(false)};
    static COMMANDS:RefCell<Option<mpsc::Receiver<Command>>>=const{RefCell::new(None)};
    static EVENTS:RefCell<Option<mpsc::Sender<Event>>>=const{RefCell::new(None)};
    static SURFACES:RefCell<HashMap<usize,surface::Surface>>=RefCell::new(HashMap::new());
    static STAGE_SURFACES:RefCell<HashMap<usize,stage_surface::Panel>>=RefCell::new(HashMap::new());
    static UTILITIES:RefCell<Option<utilities::Provider>>=const{RefCell::new(None)};
    static UTILITY_ANCHOR:Cell<Option<(usize,POINT,Option<UtilityWidget>)>>=const{Cell::new(None)};
    static ATTENTION_MSG:Cell<u32>=const{Cell::new(0)};
    static ATTENTION:RefCell<HashMap<usize,(u32,u32)>>=RefCell::new(HashMap::new());
    static STATUS_TIP:RefCell<Option<StatusTip>>=const{RefCell::new(None)};
    static ATLAS:RefCell<Option<icon_atlas::Worker>>=const{RefCell::new(None)};
}

// Detach state before any synchronous Win32 operation. Nested WM_PAINT sees an
// empty snapshot rather than reborrowing RefCell across Create/DestroyWindow.
fn update_state(f:impl FnOnce(&mut State)) {
    let mut state=STATE.with(|s|s.take());
    f(&mut state);
    let pending=state.workspaces.pending();
    STATE.with(|s|{s.replace(state);});
    WORKSPACE_TIMER_ARMED.with(|armed| unsafe {
        let control=CONTROL.with(Cell::get);
        if control.is_null() || armed.get()==pending {return;}
        if pending {
            armed.set(SetTimer(control,WORKSPACE_TIMER,25,None)!=0);
        } else {
            KillTimer(control,WORKSPACE_TIMER);
            armed.set(false);
        }
    });
}
fn emit(event:Event) {
    EVENTS.with(|s|{if let Some(tx)=s.borrow().as_ref(){let _=tx.send(event);}});
    let owner=OWNER.with(|o|o.get());
    if !owner.is_null(){unsafe{PostMessageW(owner,WM_NULL,0,0);}}
}

fn utf16(z: &[u16]) -> String {
    String::from_utf16_lossy(&z[..z.iter().position(|&c| c == 0).unwrap_or(z.len())])
}

fn active_targets() -> HashMap<String, (String, String)> {
    let mut results = HashMap::new();
    const FLAGS: u32 = QDC_ONLY_ACTIVE_PATHS;
    unsafe {
        // A hotplug between the size probe and query can change array lengths.
        for _ in 0..3 {
            let (mut count, mut modes) = (0, 0);
            if GetDisplayConfigBufferSizes(FLAGS, &mut count, &mut modes) != ERROR_SUCCESS
                || !(1..=128).contains(&count) || !(1..=512).contains(&modes) { break; }
            let mut paths = vec![mem::zeroed::<DISPLAYCONFIG_PATH_INFO>(); count as usize];
            let mut info = vec![mem::zeroed::<DISPLAYCONFIG_MODE_INFO>(); modes as usize];
            match QueryDisplayConfig(FLAGS, &mut count, paths.as_mut_ptr(), &mut modes, info.as_mut_ptr(), ptr::null_mut()) {
                ERROR_SUCCESS => {}
                ERROR_INSUFFICIENT_BUFFER => continue,
                _ => break,
            }
            for link in paths.into_iter().take(count as usize) {
                let mut source: DISPLAYCONFIG_SOURCE_DEVICE_NAME = mem::zeroed();
                source.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    size: mem::size_of_val(&source) as u32,
                    adapterId: link.sourceInfo.adapterId, id: link.sourceInfo.id,
                };
                if DisplayConfigGetDeviceInfo(&mut source.header) != 0 { continue; }
                let mut target: DISPLAYCONFIG_TARGET_DEVICE_NAME = mem::zeroed();
                target.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                    size: mem::size_of_val(&target) as u32,
                    adapterId: link.targetInfo.adapterId, id: link.targetInfo.id,
                };
                if DisplayConfigGetDeviceInfo(&mut target.header) != 0 { continue; }
                let path = utf16(&target.monitorDevicePath);
                let name = utf16(&target.monitorFriendlyDeviceName);
                if path.is_empty() && name.is_empty() { continue; }
                // Cloned displays share one drawable source, therefore one overlay.
                // Use the first active target in Windows' documented priority order.
                results.entry(utf16(&source.viewGdiDeviceName).to_lowercase()).or_insert((path, name));
            }
            break;
        }
    }
    results
}

pub fn displays() -> Vec<Display> {
    let targets = active_targets();
    crate::monitors().into_iter().enumerate().map(|(index, monitor)| {
        let source = utf16(&monitor.device);
        let (mut path, mut name) = targets.get(&source.to_lowercase()).cloned().unwrap_or_default();
        // Preserve the same per-monitor interface ID across the fallback so a
        // temporary failure to query DisplayConfig does not change a saved target.
        if path.is_empty() || name.is_empty() {
            unsafe {
                for i in 0..32 {
                    let mut device: DISPLAY_DEVICEW = mem::zeroed();
                    device.cb = mem::size_of::<DISPLAY_DEVICEW>() as u32;
                    if EnumDisplayDevicesW(monitor.device.as_ptr(), i, &mut device, 1) == 0 { break; }
                    if device.StateFlags & 1 == 0 { continue; }
                    if path.is_empty() { path = utf16(&device.DeviceID); }
                    if name.is_empty() { name = utf16(&device.DeviceString); }
                    if !path.is_empty() { break; }
                }
            }
        }
        let id = if path.is_empty() { format!("source:{source}") } else { format!("monitor:{path}") }.to_lowercase();
        if name.is_empty() { name = if source.is_empty() { format!("Display {}", index+1) } else { source }; }
        Display { id, name, bounds: monitor.rect, work: monitor.work, primary: monitor.primary,
            dpi: (monitor.dpr * 96.0).round() as u32 }
    }).collect()
}
pub fn display_json() -> serde_json::Value {
    serde_json::Value::Array(displays().into_iter().map(|d| serde_json::json!({
        "id":d.id,"name":d.name,"primary":d.primary,"dpi":d.dpi,
        "x":d.bounds.left,"y":d.bounds.top,"w":d.bounds.right-d.bounds.left,"h":d.bounds.bottom-d.bounds.top
    })).collect())
}
fn same_rect(a:RECT,b:RECT)->bool { a.left==b.left&&a.right==b.right&&a.top==b.top&&a.bottom==b.bottom }
fn rect_intersects(a:RECT,b:RECT)->bool {
    a.left<b.right && b.left<a.right && a.top<b.bottom && b.top<a.bottom
}
unsafe fn stage_window_is_local(hwnd:HWND,bounds:RECT)->bool {
    let hmon=MonitorFromWindow(hwnd,MONITOR_DEFAULTTONEAREST);
    let mut info:MONITORINFO=mem::zeroed();
    info.cbSize=mem::size_of::<MONITORINFO>() as u32;
    !hmon.is_null() && GetMonitorInfoW(hmon,&mut info)!=0 && same_rect(bounds,info.rcMonitor)
}
fn basename(path:&str)->String {
    path.rsplit(['\\','/']).next().unwrap_or(path).trim_end_matches(".exe").to_string()
}
fn is_shell_class(class:&str)->bool {
    matches!(class,"Progman"|"WorkerW"|"Shell_TrayWnd"|"Shell_SecondaryTrayWnd"|"NotifyIconOverflowWindow"|"ApplicationFrameInputSinkWindow"|"TuanpcDeskNativeDock"|"TuanpcDeskStagePreview"|"TuanpcDeskStageReveal"|"TuanpcDeskMsg"|"TuanpcDeskWall")
}
fn covers_display(class:&str,window:RECT,screen:RECT)->bool {
    !is_shell_class(class) && window.left<=screen.left && window.top<=screen.top
        && window.right>=screen.right && window.bottom>=screen.bottom
}
fn fullscreen_style(style:u32,zoomed:bool)->bool{
    style&WS_CAPTION!=WS_CAPTION&&(!zoomed||style&WS_THICKFRAME==0)
}
/// A maximized framed window can have invisible resize borders extending
/// beyond rcMonitor. Check its visible DWM bounds and its native show state.
unsafe fn window_fullscreen(hwnd:HWND,screen:RECT)->bool {
    if hwnd.is_null()||IsIconic(hwnd)!=0||!stage_window_is_local(hwnd,screen){return false;}
    let style=GetWindowLongW(hwnd,GWL_STYLE) as u32;
    if !fullscreen_style(style,IsZoomed(hwnd)!=0){return false;}
    let mut pid=0;
    GetWindowThreadProcessId(hwnd,&mut pid);
    if pid==std::process::id(){return false;}
    let mut class=[0u16;128];let n=GetClassNameW(hwnd,class.as_mut_ptr(),128);
    let mut bounds:RECT=mem::zeroed();
    if DwmGetWindowAttribute(hwnd,DWMWA_EXTENDED_FRAME_BOUNDS as u32,&mut bounds as *mut _ as _,mem::size_of::<RECT>() as u32)<0
        && GetWindowRect(hwnd,&mut bounds)==0{return false;}
    covers_display(&String::from_utf16_lossy(&class[..n.max(0) as usize]),bounds,screen)
}
unsafe fn display_fullscreen(screen:RECT,windows:&[AppWindow])->bool{
    window_fullscreen(GetForegroundWindow(),screen)||
        windows.iter().find(|w|rect_intersects(w.rect,screen)&&IsIconic(w.hwnd)==0&&stage_window_is_local(w.hwnd,screen))
            .is_some_and(|w|window_fullscreen(w.hwnd,screen))
}
// GetApplicationUserModelId is a local process query, so packaged apps retain
// distinct identities even when a generic process owns their frame window.
unsafe fn packaged_identity(process:HANDLE)->Option<String>{
    use windows_sys::Win32::Storage::Packaging::Appx::GetApplicationUserModelId;
    let mut id=[0u16;256];
    let mut size=id.len() as u32;
    if GetApplicationUserModelId(process,&mut size,id.as_mut_ptr())!=0||size<=1||size as usize>id.len(){return None;}
    Some(format!("shell:AppsFolder\\{}",utf16(&id[..size as usize])))
}
unsafe extern "system" fn frame_app(hwnd:HWND,out:LPARAM)->i32{
    let mut class=[0u16;48];
    let n=GetClassNameW(hwnd,class.as_mut_ptr(),class.len() as i32);
    if String::from_utf16_lossy(&class[..n.max(0) as usize])!="Windows.UI.Core.CoreWindow"{return 1;}
    let mut pid=0;GetWindowThreadProcessId(hwnd,&mut pid);
    let process=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,0,pid);
    if process.is_null(){return 1;}
    let app=packaged_identity(process);CloseHandle(process);
    if let Some(app)=app{*(out as *mut String)=app;0}else{1}
}
unsafe extern "system" fn enumerate(hwnd:HWND,param:LPARAM)->i32 {
    let out=&mut *(param as *mut Vec<AppWindow>);
    // GetWindowTextW sends WM_GETTEXT for another thread in this process.
    // The Settings owner can be waiting for Runtime::new or Drop to join us:
    // skip every own HWND before any title lookup, not just Dock/Shell classes.
    let mut pid=0u32;GetWindowThreadProcessId(hwnd,&mut pid);
    if pid==0 || pid==std::process::id() || !crate::trial::demo_allows(pid){return 1;}
    if IsWindowVisible(hwnd)==0 || GetWindow(hwnd,GW_OWNER)!=ptr::null_mut() {return 1;}
    if GetWindowLongW(hwnd,GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW!=0 {return 1;}
    let mut cloaked=0u32;
    if DwmGetWindowAttribute(hwnd,DWMWA_CLOAKED as u32,&mut cloaked as *mut _ as *mut _,mem::size_of::<u32>() as u32)>=0 && cloaked!=0{return 1;}
    let mut class=[0u16;128];let c=GetClassNameW(hwnd,class.as_mut_ptr(),128);
    if is_shell_class(&String::from_utf16_lossy(&class[..c.max(0) as usize])) {return 1;}
    let mut title=[0u16;260];let n=GetWindowTextW(hwnd,title.as_mut_ptr(),260);
    if n<=0{return 1;}
    let title=String::from_utf16_lossy(&title[..n as usize]);
    if crate::trial::demo_active() && !title.starts_with("Deskland Fixture "){return 1;}
    let process=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,0,pid);
    let mut path=String::new();
    if !process.is_null() {
        if let Some(id)=packaged_identity(process){path=id;}else{
            let mut buf=[0u16;1024];let mut len=buf.len() as u32;
            if QueryFullProcessImageNameW(process,0,buf.as_mut_ptr(),&mut len)!=0 {
                path=String::from_utf16_lossy(&buf[..len as usize]);
            }
        }
        CloseHandle(process);
    }
    if path.rsplit('\\').next().is_some_and(|p|p.eq_ignore_ascii_case("ApplicationFrameHost.exe")){
        EnumChildWindows(hwnd,Some(frame_app),(&mut path as *mut String) as LPARAM);
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
    out.push(AppWindow{hwnd,title,path,rect});
    1
}
fn windows()->Vec<AppWindow>{
    let mut v=Vec::new();unsafe {EnumWindows(Some(enumerate),&mut v as *mut _ as LPARAM)};v
}
// Prefer a window on this monitor for pinned apps, and keep counts consistent with
// the selected local/all-monitors scope. An off-screen pinned app stays launchable.
fn item_list(all:&[AppWindow],screen:RECT,prefs:&Preferences)->Vec<Item>{
    let mut items=available_items(all,screen,prefs);
    items.truncate(MAX_ITEMS);
    items
}
fn available_items(all:&[AppWindow],screen:RECT,prefs:&Preferences)->Vec<Item>{
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
    result
}
fn dock_depth(screen:&Screen,style:&Appearance,cluster:geometry::Cluster)->i32 {
    let scale=(screen.dpi as f32/96.0).clamp(0.75,3.0);
    let zoom=if style.reduced_motion{0.0}else{style.magnification as f32/100.0};
    (8.0*scale+((style.icon_size as f32*(1.0+zoom)+18.0).max(cluster.cross())*scale+12.0*scale).ceil()).ceil() as i32
}
fn stage_width(screen:&Screen,style:StageStyle)->i32{
    let scale=(screen.dpi as f32/96.0).clamp(0.75,3.0);
    (160.0*style.scale as f32/100.0*scale).round() as i32
}
fn stage_depth(screen:&Screen,style:StageStyle)->i32{
    let margin=(8.0*(screen.dpi as f32/96.0).clamp(0.75,3.0)).round() as i32;
    stage_width(screen,style).min(((screen.bounds.right-screen.bounds.left)/5).max(62))+margin
}
unsafe fn sync_appbars(state:&mut State,topology:bool){
    let notify=APPBAR_DIRTY.with(|flag|flag.replace(false));
    let reset=SHELL_RESTARTED.with(|flag|flag.replace(false));
    let mut requests=Vec::new();
    for screen in &state.screens {
        let policy=state.prefs.window_policy(&screen.id);
        if policy.dock==WindowMode::Reserve&&state.bars.iter().any(|b|same_rect(b.bounds,screen.bounds)){
            requests.push(appbar::Request{display:screen.id.clone(),stage:false,monitor:screen.bounds,
                side:state.prefs.appearance.position,depth:dock_depth(screen,&state.prefs.appearance,state.prefs.utility_cluster(&screen.id))});
        }
        if policy.stage==WindowMode::Reserve&&state.stages.iter().any(|p|p.display==screen.id){
            let (cards,_)=state.workspaces.cards(&screen.id,screen.bounds,&state.windows,state.prefs.all_windows);
            if !cards.is_empty() || (state.suspended&&state.appbars.contains(&screen.id,true)) {
                let style=state.prefs.stage_style(&screen.id);
                requests.push(appbar::Request{display:screen.id.clone(),stage:true,monitor:screen.bounds,
                    side:if style.right==Some(true){DockSide::Right}else{DockSide::Left},depth:stage_depth(screen,style)});
            }
        }
    }
    let outcome=state.appbars.sync(requests,notify||topology,reset);
    if !outcome.healthy&&!state.reservation_failed{
        emit(Event::Error("Windows không cấp được vùng dành riêng cho Dock hoặc Stage; tạm dùng chế độ nổi.".into()));
    }
    state.reservation_failed=!outcome.healthy;
    if !outcome.work_changed&&!notify&&!reset{return;}
    // rcWork must come from Explorer after registration/removal, not before.
    for screen in &mut state.screens{
        let monitor=MonitorFromRect(&screen.bounds,MONITOR_DEFAULTTONULL);
        let mut info=MONITORINFO{cbSize:mem::size_of::<MONITORINFO>() as u32,..mem::zeroed()};
        if !monitor.is_null()&&GetMonitorInfoW(monitor,&mut info)!=0&&same_rect(screen.bounds,info.rcMonitor){ screen.work=info.rcWork; }
    }
}
fn stage_rect(screen:&Screen,style:StageStyle)->RECT { stage_rect_with_depth(screen,style,None) }
fn stage_rect_with_depth(screen:&Screen,style:StageStyle,depth:Option<i32>)->RECT {
    let scale=(screen.dpi as f32/96.0).clamp(0.75,3.0);
    let px=|n:f32|(n*scale).round() as i32;
    let margin=px(8.0);
    let width=depth.map_or_else(||stage_width(screen,style).min(((screen.work.right-screen.work.left)/5).max(62)),|d|d-margin);
    let edge=if style.right==Some(true){screen.work.right-width-margin}else{screen.work.left+margin};
    let top=screen.work.top+((screen.work.bottom-screen.work.top)/12).min(px(90.0));
    let height=((screen.work.bottom-screen.work.top)-px(150.0)).clamp(px(112.0),px(650.0*style.scale as f32/100.0).max(px(112.0)));
    RECT{left:edge,top,right:edge+width,bottom:(top+height).min(screen.work.bottom-margin)}
}
fn stage_gap(screen:&Screen,style:StageStyle)->i32 {
    (style.spacing as f32 * (screen.dpi as f32/96.0).clamp(0.75,3.0)).round() as i32
}
fn bar_rect(screen:&Screen,items:usize)->RECT {
    let width=(items.min(MAX_ITEMS) as i32*ITEM_WIDTH+220).clamp(296,(screen.bounds.right-screen.bounds.left-24).max(296));
    let x=screen.bounds.left+(screen.bounds.right-screen.bounds.left-width)/2;
    // Compatibility mode respects Explorer's work area (taskbar may be bottom or sides).
    let bottom=screen.work.bottom-7;
    RECT{left:x,top:bottom-HEIGHT,right:x+width,bottom}
}
unsafe fn destroy_stage(stage:&mut Stage){
    stage.previews.clear();
    if !stage.rail.is_null(){DestroyWindow(stage.rail);}
    if !stage.preview_hwnd.is_null(){DestroyWindow(stage.preview_hwnd);}
    DestroyWindow(stage.hwnd);
}
unsafe fn recreate(state:&mut State) {
    let screens = &state.screens;
    let dock_targets = if state.prefs.enabled { state.prefs.dock_display.outputs(screens.iter().map(|s| (s.id.as_str(), s.primary))) } else { Vec::new() };
    let stage_targets = if state.prefs.stage_enabled { state.prefs.stage_display.outputs(screens.iter().map(|s| (s.id.as_str(), s.primary))) } else { Vec::new() };
    state.bars.retain(|b| {
        let keep = dock_targets.iter().any(|&i| same_rect(screens[i].bounds, b.bounds));
        if !keep { DestroyWindow(b.hwnd); }
        keep
    });
    state.stages.retain_mut(|s| {
        let keep = stage_targets.iter().any(|&i| screens[i].id==s.display&&same_rect(screens[i].bounds, s.bounds));
        if !keep { destroy_stage(s); }
        keep
    });
    let class=wide(CLASS);
    for (i, screen) in screens.iter().enumerate() {
        if dock_targets.contains(&i) && !state.bars.iter().any(|b|same_rect(b.bounds,screen.bounds)) {
        let items=item_list(&state.windows,screen.bounds,&state.prefs);
        let pinned_widgets=state.prefs.utility_profiles.get(&screen.id).map_or_else(Vec::new,UtilityProfile::pinned);
        let layout=geometry::measure_cluster(screen,&state.prefs.appearance,items.len(),None,0.0,&pinned_widgets,state.prefs.utility_cluster(&screen.id));
        let r=layout.window;
        let hwnd=CreateWindowExW(WS_EX_TOPMOST|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_LAYERED,
            class.as_ptr(),wide("tuanpc-desk Dock").as_ptr(),WS_POPUP,
            r.left,r.top,r.right-r.left,r.bottom-r.top,
            ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
        if !hwnd.is_null(){
        let auto_hide=state.prefs.dock_auto_hide(&screen.id);
        state.bars.push(Bar{hwnd,bounds:screen.bounds,items,hovered:-1,hidden:true,layout:Some(layout),pinned_widgets,motion:Motion::default(),collapsed:auto_hide,auto_hide,status_hover:None,pressed:None,keyboard:None,previous_focus:0,pointer:None,
            reveal:motion::Spring::rest(if auto_hide{0.0}else{1.0}),feedback:None,active:None,minimized:0,attention:0,metric_repaint:None});
        }else{emit(Event::Error(format!("Create Dock window: {}",GetLastError())));}
        }
        if stage_targets.contains(&i) && !state.stages.iter().any(|s|same_rect(s.bounds,screen.bounds)) {
            let style=state.prefs.stage_style(&screen.id);
            let r=stage_rect(screen,style);
            let stage_hwnd=CreateWindowExW(WS_EX_TOPMOST|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_LAYERED,
                class.as_ptr(),wide("tuanpc-desk Stage Manager").as_ptr(),WS_POPUP,
                r.left,r.top,r.right-r.left,r.bottom-r.top,
                ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
            if !stage_hwnd.is_null(){
                let preview_hwnd=CreateWindowExW(WS_EX_TOPMOST|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,
                    wide(PREVIEW_CLASS).as_ptr(),ptr::null(),WS_POPUP,
                    r.left,r.top,r.right-r.left,r.bottom-r.top,
                    stage_hwnd,ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
                let layered=!preview_hwnd.is_null();
                if !layered{
                    SetWindowLongPtrW(stage_hwnd,GWL_EXSTYLE,GetWindowLongPtrW(stage_hwnd,GWL_EXSTYLE)& !(WS_EX_LAYERED as isize));
                    previews::style_destination(stage_hwnd);
                }
                state.stages.push(Stage{hwnd:stage_hwnd,rail:ptr::null_mut(),collapsed:false,auto_hide:false,preview_hwnd,preview_boxes:Vec::new(),preview_visible:false,layered,opacity:style.opacity,bounds:screen.bounds,display:screen.id.clone(),items:Vec::new(),groups:Vec::new(),selected:None,layout:None,page:0,pages:1,hidden:true,hover:None,emphasis:StageEmphasis::default(),keyboard:None,previous_focus:0,previews:previews::Previews::default(),
                    resting:r,slide:None,offset:0,pressed:None});
            } else { emit(Event::Error(format!("Create Stage window: {}",GetLastError()))); }
        }
    }
    let active_outputs=state.stages.iter().map(|stage|stage.display.clone()).collect();
    state.workspaces.restore_offscreen(&active_outputs);
}
unsafe fn refresh(state:&mut State, topology:bool) {
    if topology || state.screens.is_empty() {
        state.screens = displays();
        state.workspaces.dissolve_missing_displays(state.screens.iter().map(|s|s.id.as_str()));
    }
    let material_changed=if topology||state.material.is_none(){
        let style=&state.prefs.appearance;
        let material=surface::Material::system(style.theme,style.transparency,style.reduced_motion);
        state.material.replace(material)!=Some(material)
    }else{false};
    state.windows=if state.suspended||!state.prefs.any_enabled(){Vec::new()}else{windows()};
    recreate(state);
    sync_appbars(state,topology);
    let fullscreen_outputs:HashSet<String>=state.screens.iter().filter(|s|!state.suspended&&display_fullscreen(s.bounds,&state.windows)).map(|s|s.id.clone()).collect();
    let foreground=GetForegroundWindow() as isize;let active_changed=foreground!=state.foreground;state.foreground=foreground;
    ATTENTION.with(|queue|queue.borrow_mut().retain(|&hwnd,id|{
        hwnd!=foreground as usize && state.prefs.enabled && !state.suspended && window_owner(hwnd as HWND)==Some(*id)
    }));
    for b in &mut state.bars {
        let items=item_list(&state.windows,b.bounds,&state.prefs);
        let mut changed=b.items!=items || active_changed || material_changed;
        b.items=items;
        let active_path=state.windows.iter().find(|w|w.hwnd as isize==foreground
            &&(state.prefs.all_windows||rect_intersects(b.bounds,w.rect))).map(|w|w.path.as_str());
        let active=b.items.iter().position(|i|active_path.is_some_and(|p|p.eq_ignore_ascii_case(&i.path)));
        let minimized=b.items.iter().enumerate().fold(0u16,|mask,(i,item)|{
            if item.count!=0&&!state.windows.iter().any(|w|w.path.eq_ignore_ascii_case(&item.path)
                &&(state.prefs.all_windows||rect_intersects(b.bounds,w.rect))&&IsIconic(w.hwnd)==0){mask|(1<<i)}else{mask}
        });
        let attention=ATTENTION.with(|queue|{
            let queue=queue.borrow();
            b.items.iter().enumerate().fold(0u16,|mask,(i,item)|{
                if active!=Some(i)&&state.windows.iter().any(|w|w.path.eq_ignore_ascii_case(&item.path)
                    &&(state.prefs.all_windows||rect_intersects(b.bounds,w.rect))&&queue.contains_key(&(w.hwnd as usize))){
                    mask|(1<<i)
                }else{mask}
            })
        });
        changed|=b.active!=active||b.minimized!=minimized||b.attention!=attention;
        b.active=active;b.minimized=minimized;b.attention=attention;
        let screen=state.screens.iter().find(|s|same_rect(s.bounds,b.bounds));
        if let Some(screen)=screen {
            let (placed,_)=state.appbars.placement(screen,false);
            let auto=state.prefs.dock_auto_hide(&screen.id);
            if auto!=b.auto_hide{
                KillTimer(b.hwnd,HIDE_TIMER);
                b.auto_hide=auto;b.collapsed=auto&&b.layout.is_some();
                b.reveal=motion::Spring::rest(if b.collapsed{0.0}else{1.0});changed=true;
            }
            let r=if b.layout.is_some(){
                b.pinned_widgets=state.prefs.utility_profiles.get(&screen.id).map_or_else(Vec::new,UtilityProfile::pinned);
                let layout=geometry::measure_cluster(&placed,&state.prefs.appearance,b.items.len(),b.motion.focus_value(),b.motion.zoom.value,
                    &b.pinned_widgets,state.prefs.utility_cluster(&screen.id));
                changed|=b.layout.as_ref().is_some_and(|old|old.glass!=layout.glass||old.slots!=layout.slots||old.utility!=layout.utility||old.inline!=layout.inline);
                let r=layout.window;b.layout=Some(layout);r
            }else{bar_rect(&placed,b.items.len())};
            let mut actual:RECT=mem::zeroed();
            if GetWindowRect(b.hwnd,&mut actual)==0 || !same_rect(r,actual){
                KillTimer(b.hwnd,STATUS_TIP_TIMER);close_status_tip(b.hwnd);b.status_hover=None;
                SetWindowPos(b.hwnd,HWND_TOPMOST,r.left,r.top,r.right-r.left,r.bottom-r.top,SWP_NOACTIVATE);
            }
        }
        let fullscreen=state.suspended || screen.is_some_and(|s|fullscreen_outputs.contains(&s.id));
        if b.hidden!=fullscreen {
            ShowWindow(b.hwnd,if fullscreen{SW_HIDE}else{SW_SHOWNOACTIVATE});b.hidden=fullscreen;
            if fullscreen{utilities::close_panels_for(b.hwnd);close_status_tip(b.hwnd);KillTimer(b.hwnd,STATUS_TIP_TIMER);b.status_hover=None;KillTimer(b.hwnd,MOTION_TIMER);KillTimer(b.hwnd,HIDE_TIMER);KillTimer(b.hwnd,FEEDBACK_TIMER);b.motion=Motion::default();b.hovered=-1;b.keyboard=None;b.pressed=None;b.pointer=None;b.collapsed=b.auto_hide;b.reveal=motion::Spring::rest(if b.collapsed{0.0}else{1.0});b.feedback=None;}
            changed=true;
        }
        if changed{InvalidateRect(b.hwnd,ptr::null(),0);}
    }
    if state.bars.iter().all(|b|b.hidden)||state.suspended{utilities::close_panels();UTILITY_ANCHOR.with(|a|a.set(None));UTILITIES.with(|u|u.borrow_mut().take());}
    state.workspaces.prune();
    let benchmark_sources=crate::trial::benchmark_stage_sources();
    let mut stage_report=Vec::new();
    for stage in &mut state.stages {
        let Some(screen)=state.screens.iter().find(|s|s.id==stage.display) else{continue;};
        let style=state.prefs.stage_style(&screen.id);
        let (placed,depth)=state.appbars.placement(screen,true);
        let mut bounds=stage_rect_with_depth(&placed,style,depth);
        let auto=state.prefs.window_policy(&screen.id).stage==WindowMode::AutoHide;
        if auto!=stage.auto_hide{
            KillTimer(stage.hwnd,HIDE_TIMER);
            stage.auto_hide=auto;stage.collapsed=auto;
        }
        if !auto&&!stage.rail.is_null(){DestroyWindow(stage.rail);stage.rail=ptr::null_mut();}
        let (items,groups)=state.workspaces.cards(&stage.display,stage.bounds,&state.windows,state.prefs.all_windows);
        let mut layout=stage_surface::Layout::new(bounds.right-bounds.left,bounds.bottom-bounds.top,screen.dpi,stage_gap(screen,style),
            style.group_limit as usize,items.len(),style.right==Some(true));
        let rows=layout.rows;
        let selected=state.workspaces.active(&stage.display);
        let old_pages=stage.pages;
        stage.pages=items.len().div_ceil(rows).max(1);
        stage.page=if selected!=stage.selected {
            selected.and_then(|id|groups.iter().position(|group|*group==Some(id)))
                .map_or(stage.page.min(stage.pages-1),|row|row/rows)
        }else{stage.page.min(stage.pages-1)};
        let start=stage.page*rows;
        let model_count=items.len();
        let fixture_positions=benchmark_sources.map(|sources| sources.iter().map(|&handle|
            items.iter().position(|item| item.hwnd as usize==handle)).collect::<Vec<_>>());
        let items:Vec<_>=items.into_iter().skip(start).take(rows).collect();
        let groups:Vec<_>=groups.into_iter().skip(start).take(rows).collect();
        let mut changed=stage.items!=items||stage.groups!=groups||stage.selected!=selected||active_changed||material_changed||old_pages!=stage.pages;
        changed|=stage.opacity!=style.opacity;stage.opacity=style.opacity;
        layout.count=items.len();
        layout.height=layout.content_height();
        bounds.bottom=bounds.top+layout.height;
        if topology || state.prefs.appearance.reduced_motion {
            stage.slide=None;stage.offset=0;
        }
        stage.resting=bounds;
        let mut actual:RECT=mem::zeroed();
        let shifted=RECT{left:bounds.left+stage.offset,right:bounds.right+stage.offset,..bounds};
        if GetWindowRect(stage.hwnd,&mut actual)==0 || !same_rect(shifted,actual){
            stage.position();
            changed=true;
        }
        if style.stack_groups {
            layout.stack=(0..items.len()).filter(|&i|items[i].count>1).fold(0u8,|bits,i|bits|(1<<i));
        }
        if stage.items!=items || stage.groups!=groups {
            KillTimer(stage.hwnd,STATUS_TIP_TIMER);close_status_tip(stage.hwnd);
            stage.hover=None;stage.emphasis=StageEmphasis::default();
            if stage.slide.is_none(){KillTimer(stage.hwnd,STAGE_MOTION_TIMER);}
            stage.keyboard=stage.keyboard.and_then(|index|{
                let previous=stage.items.get(index);
                previous.and_then(|w|items.iter().position(|i|i.hwnd==w.hwnd))
                    .or_else(||items.len().checked_sub(1).map(|last|index.min(last)))
            });
        }
        stage.items=items;stage.groups=groups;stage.selected=selected;
        if stage.layout!=Some(layout){
            stage.layout=if stage.layered||layout.apply(stage.hwnd){Some(layout)}else{None};
            changed=true;
        }
        let unavailable=stage.layout.is_none() || stage.items.is_empty() || state.suspended || fullscreen_outputs.contains(&screen.id);
        if unavailable&&auto{stage.collapsed=true;KillTimer(stage.hwnd,HIDE_TIMER);}
        let hide=unavailable||stage.collapsed;
        if auto&&!unavailable&&stage.rail.is_null(){
            stage.rail=CreateWindowExW(WS_EX_TOPMOST|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,
                wide(STAGE_RAIL_CLASS).as_ptr(),wide("Hiện Stage Manager").as_ptr(),WS_POPUP,
                0,0,1,1,ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
            if stage.rail.is_null(){stage.collapsed=false;}
        }
        if !stage.rail.is_null(){
            if unavailable||!stage.collapsed{
                if IsWindowVisible(stage.rail)!=0{ShowWindow(stage.rail,SW_HIDE);}
            }else{
                let dpi=screen.dpi.clamp(72,288) as i32;
                let w=(5*dpi+48)/96;
                let h=(64*dpi+48)/96;
                let x=if style.right==Some(true){screen.work.right-w}else{screen.work.left};
                let y=bounds.top;
                let mut old:RECT=mem::zeroed();GetWindowRect(stage.rail,&mut old);
                if IsWindowVisible(stage.rail)==0||!same_rect(old,RECT{left:x,top:y,right:x+w,bottom:y+h}){
                    SetWindowPos(stage.rail,HWND_TOPMOST,x,y,w,h,SWP_NOACTIVATE|SWP_SHOWWINDOW);
                }
            }
        }
        let hide=hide&&(unavailable||stage.collapsed);
        let mut steps=[previews::PreviewStep::NotRun;8];
        if hide!=stage.hidden{ShowWindow(stage.hwnd,if hide{SW_HIDE}else{SW_SHOWNOACTIVATE});stage.hidden=hide;changed=true;}
        if hide {
            if stage.emphasis.start.is_some()||stage.slide.is_some(){KillTimer(stage.hwnd,STAGE_MOTION_TIMER);}
            stage.slide=None;stage.offset=0;
            stage.hover=None;stage.emphasis=StageEmphasis::default();stage.keyboard=None;stage.pressed=None;KillTimer(stage.hwnd,STATUS_TIP_TIMER);close_status_tip(stage.hwnd);changed|=stage.previews.clear();
        }
        else if layout.width>0&&layout.height>0{
            let host=if stage.preview_hwnd.is_null(){stage.hwnd}else{stage.preview_hwnd};
            changed|=stage.previews.update_traced(host,&stage.items,layout,benchmark_sources.map(|_| steps.as_mut_slice()));
        }
        let mut clip_ok=true;
        if !stage.preview_hwnd.is_null(){
            let areas=stage.previews.areas();
            let reshape=areas.len()!=stage.preview_boxes.len()||areas.iter().zip(&stage.preview_boxes).any(|(&a,&b)|!same_rect(a,b));
            let clipped=!reshape||previews::clip_destination(stage.preview_hwnd,&areas);
            clip_ok=clipped;
            let show=clipped&&!areas.is_empty()&&!hide;
            if clipped&&reshape{stage.preview_boxes=areas;}
            if stage.preview_visible!=show{
                ShowWindow(stage.preview_hwnd,if show{SW_SHOWNOACTIVATE}else{SW_HIDE});
                stage.preview_visible=show;
                changed=true;
            }
            if !clipped {changed|=stage.previews.clear();}
        }
        if let Some((sources, positions))=benchmark_sources.zip(fixture_positions) {
            let owner=crate::trial::benchmark_stage_fixture().unwrap().0;
            let host=if stage.preview_hwnd.is_null(){stage.hwnd}else{stage.preview_hwnd};
            let host_visible=IsWindowVisible(host)!=0;
            let cards=sources.iter().zip(positions).enumerate().map(|(i,(&handle,index))|{
                let hwnd=handle as HWND;
                let mut pid=0;
                GetWindowThreadProcessId(hwnd,&mut pid);
                if pid!=owner {return serde_json::json!({"source":i,"identity":"Missing"});}
                let enumerated=state.windows.iter().find(|item|item.hwnd==hwnd);
                let mut rect:RECT=mem::zeroed();
                let positioned=GetWindowRect(hwnd,&mut rect)!=0;
                let on_monitor=|r:RECT| positioned&&rect_intersects(rect,r);
                let source_display=state.screens.iter().position(|display|on_monitor(display.bounds));
                let slot=stage.items.iter().position(|item| item.hwnd==hwnd);
                serde_json::json!({"source":i,"sourceOutput":source_display.map(|n| if state.screens[n].primary {"primary"}else{"secondary"}),
                    "visible":IsWindowVisible(hwnd)!=0,"local":on_monitor(screen.bounds),"enumerated":enumerated.is_some(),
                    "eligibility":match previews::preview_pid(hwnd){Ok(_)=>"Eligible".to_owned(),Err(reason)=>format!("{reason:?}")},
                    "cardIndex":index,"cardPage":index.map(|n|n/rows),"slot":slot,
                    "dwm":slot.map(|n| format!("{:?}",steps[n])),
                    "finalLink":host_visible&&clip_ok&&stage.previews.includes(hwnd)})
            }).collect::<Vec<_>>();
            stage_report.push(serde_json::json!({"output":if screen.primary {"primary"}else{"secondary"},
                "allWindows":state.prefs.all_windows,"target":state.prefs.stage_display.mode.key(),
                "limit":style.group_limit,"modelCount":model_count,"rows":rows,"shownPage":stage.page,"pages":stage.pages,
                "previewHostVisible":host_visible,"clippingOk":clip_ok,"cards":cards}));
        }
        if changed{InvalidateRect(stage.hwnd,ptr::null(),0);}
    }
    if benchmark_sources.is_some(){crate::trial::benchmark_stage_diagnostic(&stage_report);}
    if let Some((pid, anchor)) = crate::trial::benchmark_stage_fixture() {
        let (fixture, other) = state.stages.iter()
            .filter(|s| s.preview_visible && IsWindowVisible(s.preview_hwnd) != 0)
            .map(|s| s.previews.benchmark_counts(pid, anchor))
            .fold((0, 0), |(a, b), (c, d)| (a + c, b + d));
        crate::trial::benchmark_stage_links(fixture, other);
    }
    let live_profiles=state.bars.iter().filter(|b|!b.hidden&&!b.collapsed&&b.layout.as_ref().is_some_and(|l|l.metrics.is_some()))
        .filter_map(|b|state.screens.iter().find(|d|same_rect(d.bounds,b.bounds)))
        .filter_map(|d|state.prefs.utility_profiles.get(&d.id)).map(|p|p.metrics)
        .filter(|p|p.enabled&&p.inline&&p.mask!=0);
    let inline=live_profiles.fold(None,|current:Option<MetricOptions>,p|Some(match current{
        None=>p,
        Some(v)=>MetricOptions{mask:v.mask|p.mask,interval:v.interval.min(p.interval),..v},
    }));
    control_center::inline_config(CONTROL.with(Cell::get),inline);
    // Pick at most one cacheful across both modules; otherwise icon-complete
    // messages could continually evict and re-request visible icons.
    ATLAS.with(|a|{
        let mut worker=a.borrow_mut();
        if state.bars.is_empty()&&state.stages.is_empty(){worker.take();return;}
        let worker=worker.get_or_insert_with(||icon_atlas::Worker::new(CONTROL.with(Cell::get)));
        let dock=state.bars.iter().filter(|b|!b.hidden).flat_map(|b|&b.items);
        let stage=state.stages.iter().filter(|s|!s.hidden).flat_map(|s|&s.items);
        let style=&state.prefs.appearance;
        let magnify=if style.reduced_motion{1.0}else{1.0+style.magnification as f32/100.0};
        let dock_edge=state.bars.iter().filter_map(|b|b.layout.as_ref())
            .map(|layout|style.icon_size as f32*layout.scale*magnify*0.84)
            .fold(0.0,f32::max);
        let stage_edge=state.stages.iter().filter_map(|stage|stage.layout)
            .map(|layout|layout.caption as f32*1.2).fold(0.0,f32::max);
        worker.restrict_visible(dock.chain(stage),icon_atlas::source_request(dock_edge.max(stage_edge)));
    });
}
unsafe fn fill(hdc:HDC,r:RECT,color:u32) {
    let brush=CreateSolidBrush(color);FillRect(hdc,&r,brush);DeleteObject(brush as HGDIOBJ);
}
unsafe fn label(hdc:HDC,x:i32,y:i32,width:i32,text:&str,color:u32) {
    SetTextColor(hdc,color);SetBkMode(hdc,TRANSPARENT as i32);
    let mut r=RECT{left:x,top:y,right:x+width,bottom:y+26};
    DrawTextW(hdc,wide(text).as_ptr(),-1,&mut r,DT_CENTER|DT_SINGLELINE|DT_END_ELLIPSIS|DT_VCENTER|DT_NOPREFIX);
}
fn color(seed:&str)->u32 {
    let n=seed.bytes().fold(0x811c9dc5u32,|x,b|(x^b as u32).wrapping_mul(0x01000193));
    0x00353535|((n&0x3f)<<16)|(((n>>8)&0x3f)<<8)
}
struct StatusTip{owner:HWND,window:HWND,_label:Vec<u16>}
impl Drop for StatusTip {
    fn drop(&mut self){unsafe{DestroyWindow(self.window);}}
}
fn close_status_tip(owner:HWND){
    let old=STATUS_TIP.with(|tip|{
        let mut tip=tip.borrow_mut();
        if owner.is_null()||tip.as_ref().is_some_and(|t|t.owner==owner){tip.take()}else{None}
    });
    drop(old);
}
unsafe fn show_status_tip(owner:HWND){
    use windows_sys::Win32::UI::Controls::*;
    let label=STATE.with(|s|{
        let state=s.borrow();
        if let Some(stage)=state.stages.iter().find(|p|p.hwnd==owner&&!p.hidden&&!p.collapsed){
            let index=stage.hover?;
            let item=stage.items.get(index)?;
            return Some(if stage.groups.get(index).is_some_and(Option::is_some){
                format!("{} · {} cửa sổ",item.title,item.count)
            }else{item.title.chars().take(512).collect()});
        }
        let item=state.bars.iter().find(|b|b.hwnd==owner&&!b.hidden&&!b.collapsed)?.status_hover?;
        Some(item.map_or("Bảng tiện ích",UtilityWidget::title).to_owned())
    });
    let Some(label)=label.filter(|label|!label.is_empty())else{return;};
    close_status_tip(ptr::null_mut());
    let mut tip=StatusTip{
        owner,window:CreateWindowExW(WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST,
            wide("tooltips_class32").as_ptr(),ptr::null(),WS_POPUP|TTS_NOPREFIX|TTS_ALWAYSTIP,
            CW_USEDEFAULT,CW_USEDEFAULT,CW_USEDEFAULT,CW_USEDEFAULT,owner,ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null()),
        _label:wide(&label)
    };
    if tip.window.is_null(){return;}
    let mut tool:TTTOOLINFOW=mem::zeroed();
    tool.cbSize=mem::size_of_val(&tool) as u32;
    tool.uFlags=TTF_TRACK|TTF_ABSOLUTE;tool.hwnd=owner;tool.uId=1;tool.lpszText=tip._label.as_mut_ptr();
    if SendMessageW(tip.window,TTM_ADDTOOLW,0,(&tool as *const TTTOOLINFOW) as isize)==0{return;}
    let mut pt=POINT{x:0,y:0};GetCursorPos(&mut pt);
    let coords=(((pt.y+18) as u16 as u32)<<16)|((pt.x+14) as u16 as u32);
    let dpi=windows_sys::Win32::UI::HiDpi::GetDpiForWindow(owner).clamp(72,288);
    SendMessageW(tip.window,TTM_SETMAXTIPWIDTH,0,(360*dpi/96) as isize);
    SendMessageW(tip.window,TTM_TRACKPOSITION,0,coords as isize);
    SendMessageW(tip.window,TTM_TRACKACTIVATE,1,(&tool as *const TTTOOLINFOW) as isize);
    STATUS_TIP.with(|value|value.replace(Some(tip)));
}
fn stage_caption<'a>(title:&str,peers:impl Iterator<Item=&'a str>)->String{
    let mut best=0;
    for peer in peers{
        if title==peer{continue;}
        let mut common=0;
        for (a,b) in title.chars().zip(peer.chars()){
            if a!=b{break;}
            common+=a.len_utf8();
        }
        if common==title.len()||common==peer.len(){continue;}
        // Cut only at a word boundary; never choose a partially shared word.
        let cut=title[..common].char_indices().filter(|&(_,c)|c.is_whitespace()||matches!(c,'-'|'–'|'—'|'·'|'|'|':'|'/'|'»'))
            .map(|(i,c)|i+c.len_utf8()).next_back().unwrap_or(0);
        best=best.max(cut);
    }
    if title[..best].chars().count()>=12 {
        let tail=title[best..].trim();
        if !tail.is_empty(){return tail.to_owned();}
    }
    title.to_owned()
}
unsafe fn paint(hwnd:HWND){
    let layered=STATE.with(|s|{
        let state=s.borrow();
        let bar=state.bars.iter().find(|b|b.hwnd==hwnd)?;
        let feedback=bar.feedback.as_ref().and_then(|f|bar.items.iter().position(|i|i.path.eq_ignore_ascii_case(&f.path))
            .map(|i|(i,f.kind,if state.prefs.appearance.reduced_motion{0.2}else{f.start.elapsed().as_secs_f32()/0.42})));
        Some((bar.layout.clone()?,bar.items.clone(),bar.hovered,bar.keyboard,state.material?,bar.active,bar.minimized,bar.attention,bar.drop_target(),bar.reveal.value.clamp(0.0,1.0),feedback,bar.motion.at,
            state.screens.iter().find(|d|same_rect(d.bounds,bar.bounds))
                .and_then(|d|state.prefs.utility_profiles.get(&d.id)).map(|p|p.metrics)))
    });
    let mut ps:PAINTSTRUCT=mem::zeroed();let dc=BeginPaint(hwnd,&mut ps);
    if let Some((layout,items,hovered,keyboard,material,active,minimized,attention,drop_target,openness,feedback,animation_tick,metrics))=layered{
        EndPaint(hwnd,&ps);
        let key=hwnd as usize;
        let paint=animation_tick.filter(|_|perf::active()).map(|tick|(tick,std::time::Instant::now()));
        let size=(layout.window.right-layout.window.left,layout.window.bottom-layout.window.top);
        let canvas=SURFACES.with(|s|s.borrow_mut().remove(&key));
        let canvas=match canvas{Some(c) if c.size()==size=>Some(c),_=>surface::Surface::new(size.0,size.1,layout.glass,material)};
        if let Some(mut canvas)=canvas{
            canvas.set_material(material);
            // Clone Arc handles only; do not hold the atlas borrow across
            // UpdateLayeredWindow, which may dispatch nested Win32 messages.
            let atlas=ATLAS.with(|a|{
                let mut a=a.borrow_mut();
                a.as_mut().map(|atlas|atlas.request(&items)).unwrap_or_default()
            });
            let applied=if openness<=0.003 {canvas.rail(hwnd,&layout)} else {canvas.paint(hwnd,&layout,&items,&atlas,active,minimized,attention,keyboard.or_else(||usize::try_from(hovered).ok()),drop_target,openness,feedback,metrics)};
            if applied{
                if let Some((tick,paint))=paint{perf::submitted(key,tick,paint);}
                SURFACES.with(|s|s.borrow_mut().insert(key,canvas));
                return;
            }
        }
        // A driver or remote session can reject per-pixel presentation. Keep
        // app switching available using the previous native paint path.
        STATE.with(|s|{if let Some(bar)=s.borrow_mut().bars.iter_mut().find(|b|b.hwnd==hwnd){bar.layout=None;bar.collapsed=false;bar.pressed=None;}});
        SetWindowLongPtrW(hwnd,GWL_EXSTYLE,GetWindowLongPtrW(hwnd,GWL_EXSTYLE)& !(WS_EX_LAYERED as isize));
        KillTimer(hwnd,MOTION_TIMER);
        queue_refresh();
        InvalidateRect(hwnd,ptr::null(),0);
        return;
    }
    let mut area:RECT=mem::zeroed();GetClientRect(hwnd,&mut area);
    let stage=STATE.with(|cell|{
        let state=cell.borrow();
        let stage=state.stages.iter().find(|stage|stage.hwnd==hwnd)?;
        let ready=stage.items.iter().map(|item|(stage.preview_hwnd.is_null()||stage.preview_visible)&&stage.previews.includes(item.hwnd)).collect::<Vec<_>>();
        let dpi=state.screens.iter().find(|s|s.id==stage.display).map_or(96,|s|s.dpi);
        let dark=state.material.is_none_or(|m|m.dark);
        Some((stage.items.clone(),ready,stage.groups.clone(),stage.selected,stage.layout?,stage.page,stage.pages,state.foreground,dpi,dark,stage.emphasis.highlight(stage.keyboard),stage.layered,stage.opacity))
    });
    if let Some((items,ready,groups,active,layout,page,pages,foreground,dpi,dark,hover,layered,opacity))=stage {
        let key=hwnd as usize;
        let size=(area.right,area.bottom,dpi);
        let panel=STAGE_SURFACES.with(|s|s.borrow_mut().remove(&key));
        let mut panel=match panel{Some(p) if p.geometry()==size=>Some(p),_=>stage_surface::Panel::new(size.0,size.1,size.2)};
        let selected=|i:usize|items[i].hwnd as isize==foreground || (active.is_some()&&active==groups[i]);
        let active_mask=(0..items.len()).filter(|&i|selected(i)).fold(0u8,|bits,i|bits|(1<<i));
        let atlas=ATLAS.with(|a|a.borrow_mut().as_mut().map(|w|w.request(&items)).unwrap_or_default());
        let labels=items.iter().enumerate().map(|(i,item)|{
            if groups[i].is_some(){return item.title.clone();}
            stage_caption(&item.title,items.iter().enumerate().filter(|(j,o)|*j!=i&&groups[*j].is_none()&&o.path==item.path).map(|(_,o)|o.title.as_str()))
        }).collect::<Vec<_>>();
        let look=stage_surface::Look{dark,layout,active:active_mask,alpha:layered.then_some((opacity*255/100) as u8)};
        if layered{EndPaint(hwnd,&ps);}
        let rendered=panel.as_mut().is_some_and(|p|p.paint(dc,look,&items,&atlas,&ready,hover,layered.then_some((hwnd,&labels[..],page,pages))));
        if layered{
            if rendered{
                if let Some(panel)=panel{STAGE_SURFACES.with(|s|s.borrow_mut().insert(key,panel));}
            }else{
                // Preserve input and live previews when a driver rejects ULW_ALPHA.
                STATE.with(|s|{if let Some(p)=s.borrow_mut().stages.iter_mut().find(|p|p.hwnd==hwnd){p.layered=false;p.layout=None;}});
                SetWindowLongPtrW(hwnd,GWL_EXSTYLE,GetWindowLongPtrW(hwnd,GWL_EXSTYLE)& !(WS_EX_LAYERED as isize));
                queue_refresh();
            }
            return;
        }
        if !rendered{fill(dc,area,if dark{0x00201b18}else{0x00f4f2ee});}
        let font=panel.as_ref().map_or(ptr::null_mut(),stage_surface::Panel::font);
        let previous=if font.is_null(){ptr::null_mut()}else{SelectObject(dc,font)};
        let ink=if dark{0x00f1ebe7}else{0x00342823};
        let muted=if dark{0x00a09d98}else{0x007e7268};
        for (i,title) in labels.iter().enumerate(){stage_surface::caption(dc,layout,i,title,ink);}
        if let Some(nav)=layout.pages(pages){
            stage_surface::nav_label(dc,nav.previous,"‹",if page>0{ink}else{muted});
            stage_surface::nav_label(dc,nav.next,"›",if page+1<pages{ink}else{muted});
            stage_surface::nav_label(dc,nav.middle,&format!("{}/{pages}",page+1),ink);
        }
        if !previous.is_null(){SelectObject(dc,previous);}
        EndPaint(hwnd,&ps);
        if let Some(panel)=panel{STAGE_SURFACES.with(|s|s.borrow_mut().insert(key,panel));}
        return;
    }
    fill(dc,area,0x00201b18);
    let bar=STATE.with(|cell|cell.borrow().bars.iter().find(|b|b.hwnd==hwnd).cloned());
    if let Some(bar)=bar{
        for (i,item) in bar.items.iter().enumerate() {
            let left=12+i as i32*ITEM_WIDTH;
            let zoom=if bar.hovered==i as i32{6}else{0};
            let box_rect=RECT{left:left+zoom/2,top:9-zoom/2,right:left+48+zoom,bottom:50+zoom/2};
            fill(dc,box_rect,color(&item.path));
            let title=basename(&item.path);
            // A driver that rejects the layered surface still gets a
            // clickable label. Never call the Shell synchronously during paint.
            let initial=title.chars().take(2).collect::<String>().to_uppercase();
            label(dc,left,18,54,&initial,0x00ffffff);
            if item.count>0 {
                fill(dc,RECT{left:left+23,top:56,right:left+29,bottom:60},0x00ffffff);
            }
            if item.pinned && item.count==0 {
                fill(dc,RECT{left:left+24,top:56,right:left+28,bottom:60},0x00888888);
            }
        }
        if utility_profile(hwnd).is_some_and(|p|p.enabled){label(dc,area.right-52,20,42,"⋯",0x00ffffff);}
    }
    EndPaint(hwnd,&ps);
}
unsafe fn request_utilities(hwnd:HWND,widget:Option<UtilityWidget>){
    request_utilities_with_change(hwnd,widget,None);
}
unsafe fn request_utilities_with_change(hwnd:HWND,widget:Option<UtilityWidget>,change:Option<wifi::Change>){
    if utility_profile(hwnd).is_none_or(|p|!p.enabled || widget.is_some_and(|w|p.hidden.contains(&w))){return;}
    if let Some(action)=widget.filter(|w|*w!=UtilityWidget::QuickSettings).and_then(UtilityWidget::simple){
        match action {utilities::Action::Host(action)=>emit(Event::Utility(action)),_=>utilities::delegated(action)}
        return;
    }
    let mut point=POINT{x:0,y:0};
    let location=STATE.with(|s|{
        let state=s.borrow();
        let bar=state.bars.iter().find(|b|b.hwnd==hwnd&&!b.hidden)?;
        bar.layout.as_ref().map(|l|{
            let r=widget.and_then(|id|l.inline.iter().find(|(w,_)|*w==id).map(|(_,r)|*r)).unwrap_or(l.utility);
            let (x,y)=match l.side{
                DockSide::Bottom=>(r.x+r.w*0.5,l.glass.y),
                DockSide::Left=>(l.glass.x+l.glass.w,r.y+r.h*0.5),
                DockSide::Right=>(l.glass.x,r.y+r.h*0.5),
            };
            (x.round() as i32,y.round() as i32)
        })
    });
    if let Some((x,y))=location{
        point.x=x;point.y=y;
        ClientToScreen(hwnd,&mut point);
    }else{GetCursorPos(&mut point);}
    if widget==Some(UtilityWidget::Metrics){
        if let Some(p)=utility_profile(hwnd).filter(|p|p.metrics.enabled){
            utilities::show_metrics(hwnd,panel_chrome::Anchor::for_owner(hwnd,point),p.metrics);
        }
        return;
    }
    let control=CONTROL.with(Cell::get);
    let submitted=UTILITIES.with(|u|u.borrow_mut().get_or_insert_with(utilities::Provider::new).read(control,widget==Some(UtilityWidget::Network),change));
    if submitted{UTILITY_ANCHOR.with(|a|a.set(Some((hwnd as usize,point,widget))));}
}
fn utility_profile(hwnd:HWND)->Option<UtilityProfile>{
    STATE.with(|s|{
        let state=s.borrow();
        let bar=state.bars.iter().find(|b|b.hwnd==hwnd&&!b.hidden)?;
        let output=state.screens.iter().find(|d|same_rect(d.bounds,bar.bounds))?;
        Some(state.prefs.utility_profiles.get(&output.id).cloned().unwrap_or_default())
    })
}
fn apply_audio(panel:HWND,action:Option<utilities::Action>)->bool{
    UTILITIES.with(|u|u.borrow().as_ref().is_some_and(|p|p.apply(CONTROL.with(Cell::get),panel,action)))
}
unsafe fn show_utilities(){
    let result=UTILITIES.with(|u|u.borrow().as_ref().and_then(utilities::Provider::take_audio_result));
    if let Some((panel,result))=result{utilities::reflect_audio(panel as HWND,result);}
    let Some(snapshot)=UTILITIES.with(|u|u.borrow().as_ref().and_then(utilities::Provider::take)) else{return;};
    let target=UTILITY_ANCHOR.with(|a|a.take());
    let Some((bar,point,widget))=target else{return;};
    let hwnd=bar as HWND;
    let Some(profile)=utility_profile(hwnd).filter(|p|p.enabled) else{return;};
    utilities::show_view(hwnd,point,&snapshot,&profile,widget);
}
fn queue_inline_refresh_if_needed(hwnd:HWND){
    let exists=STATE.with(|s|s.borrow().bars.iter().any(|b|b.hwnd==hwnd&&b.layout.as_ref().is_some_and(|l|l.metrics.is_some())));
    // The timer tick only changes the sampling demand on a reveal.
    if exists{unsafe{SetTimer(CONTROL.with(Cell::get),INLINE_WAKE,35,None);}}
}
const INLINE_WAKE:usize=11;
unsafe fn panel_closed(owner:HWND){
    if !owner.is_null()&&IsWindow(owner)!=0{SetTimer(owner,HIDE_TIMER,650,None);}
}
unsafe fn perform_control(owner:HWND,command:control_center::Command){
    use control_center::Command;
    match command{
        Command::Widget(widget)=>request_utilities(owner,Some(widget)),
        Command::App{path,operation}=>perform_app_action(owner,&path,operation),
        Command::AddApp=>pin_from_file(owner),
        Command::Wifi(change)=>request_utilities_with_change(owner,Some(UtilityWidget::Network),Some(change)),
        Command::Run(utilities::Action::Host(action))=>emit(Event::Utility(action)),
        Command::Run(action)=>utilities::delegated(action),
        Command::Window{hwnd,pid,tid}=>{
            let hwnd=hwnd as HWND;
            if window_owner(hwnd)!=Some((pid,tid)){return;}
            let item=STATE.with(|s|s.borrow().windows.iter().find(|w|w.hwnd==hwnd)
                .map(|w|Item{path:w.path.clone(),title:w.title.clone(),hwnd,count:1,pinned:false}));
            if let Some(item)=item{activate(owner,&item);}
        }
    }
}
unsafe fn open_launcher(){
    let owner=OWNER.with(Cell::get);
    if !owner.is_null(){PostMessageW(owner,crate::WM_OPEN_LAUNCHER,0,0);}
}
unsafe fn open(uri:&str) {
    ShellExecuteW(ptr::null_mut(),wide("open").as_ptr(),wide(uri).as_ptr(),ptr::null(),ptr::null(),SW_SHOWNORMAL);
}
unsafe fn show_feedback(hwnd:HWND,item:&Item,kind:FeedbackKind){
    let (start,reduced)=STATE.with(|s|{
        let mut s=s.borrow_mut();
        let reduced=s.prefs.appearance.reduced_motion;
        let Some(bar)=s.bars.iter_mut().find(|b|b.hwnd==hwnd&&!b.hidden&&b.layout.is_some())else{return(false,reduced);};
        bar.feedback=Some(Feedback{path:item.path.clone(),kind,start:std::time::Instant::now()});
        (if reduced{true}else{bar.schedule()},reduced)
    });
    if start {SetTimer(hwnd,if reduced{FEEDBACK_TIMER}else{MOTION_TIMER},if reduced{420}else{16},None);}
    InvalidateRect(hwnd,ptr::null(),0);
}
unsafe fn activate(hwnd:HWND,item:&Item) {
    if !item.hwnd.is_null() && IsWindow(item.hwnd)!=0 {
        if IsIconic(item.hwnd)!=0{
            let restored=ShowWindowAsync(item.hwnd,SW_RESTORE)!=0;
            show_feedback(hwnd,item,if restored{FeedbackKind::Restore}else{FeedbackKind::Failed});
        }
        SetForegroundWindow(item.hwnd);
    } else {
        let result=ShellExecuteW(ptr::null_mut(),wide("open").as_ptr(),wide(&item.path).as_ptr(),ptr::null(),ptr::null(),SW_SHOWNORMAL) as isize;
        show_feedback(hwnd,item,if result>32{FeedbackKind::Launch}else{FeedbackKind::Failed});
    }
}
#[derive(Clone)]
enum AppOperation{
    Open, NewWindow, ShowMenu, TogglePin, Close(Vec<WindowIdentity>), Advanced,
}
#[derive(Clone,Copy)]
struct WindowIdentity{hwnd:usize,pid:u32,tid:u32}
impl WindowIdentity{
    unsafe fn read(hwnd:HWND)->Option<Self>{
        let (pid,tid)=window_owner(hwnd)?;
        Some(Self{hwnd:hwnd as usize,pid,tid})
    }
    unsafe fn matches(self,hwnd:HWND)->bool{
        self.hwnd==hwnd as usize&&window_owner(hwnd)==Some((self.pid,self.tid))
    }
}

unsafe fn pin_from_file(hwnd:HWND){
    use windows_sys::Win32::UI::Controls::Dialogs::*;
    let mut file=[0u16;32768];
    let filter=wide("Ứng dụng và lối tắt (*.exe;*.lnk)\0*.exe;*.lnk\0\0");
    let title=wide("Thêm ứng dụng vào Dock");
    let mut dialog:OPENFILENAMEW=mem::zeroed();
    dialog.lStructSize=mem::size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner=hwnd;
    dialog.lpstrFilter=filter.as_ptr();
    dialog.lpstrFile=file.as_mut_ptr();
    dialog.nMaxFile=file.len() as u32;
    dialog.lpstrTitle=title.as_ptr();
    dialog.Flags=OFN_FILEMUSTEXIST|OFN_PATHMUSTEXIST|OFN_NOCHANGEDIR|OFN_NODEREFERENCELINKS|OFN_EXPLORER;
    if GetOpenFileNameW(&mut dialog)==0{return;}
    let path=utf16(&file);
    let selected=std::path::Path::new(&path);
    if selected.is_absolute()&&selected.is_file()&&selected.extension().and_then(|e|e.to_str())
        .is_some_and(|v|v.eq_ignore_ascii_case("exe")||v.eq_ignore_ascii_case("lnk")){
        emit(Event::Edit(Edit::Pin(path)));
    }
}
unsafe fn dock_app_anchor(owner:HWND,path:Option<&str>)->panel_chrome::Anchor{
    let mut point=POINT{x:0,y:0};
    GetCursorPos(&mut point);
    if let Some(local)=STATE.with(|s|{
        let state=s.borrow();
        let bar=state.bars.iter().find(|b|b.hwnd==owner&&!b.hidden)?;
        let layout=bar.layout.as_ref()?;
        let slot=path.and_then(|path|bar.items.iter().position(|i|i.path.eq_ignore_ascii_case(path)))
            .and_then(|index|layout.slots.get(index).copied()).unwrap_or(layout.scan);
        let (x,y)=match layout.side{
            DockSide::Bottom=>(slot.x+slot.w*0.5,layout.glass.y),
            DockSide::Left=>(layout.glass.x+layout.glass.w,slot.y+slot.h*0.5),
            DockSide::Right=>(layout.glass.x,slot.y+slot.h*0.5),
        };
        Some(POINT{x:x.round() as i32,y:y.round() as i32})
    }){
        point=local;
        ClientToScreen(owner,&mut point);
    }
    panel_chrome::Anchor::for_owner(owner,point)
}

unsafe fn app_menu(owner:HWND,path:&str,picker:bool){
    use control_center::{Card,Command,View};
    let Some((item,mut windows,advanced))=STATE.with(|s|{
        let state=s.borrow();
        let bar=state.bars.iter().find(|b|b.hwnd==owner&&!b.hidden)?;
        let visible=bar.items.iter().find(|i|i.path.eq_ignore_ascii_case(path)).cloned();
        let advanced=visible.is_some();
        let item=visible.or_else(||available_items(&state.windows,bar.bounds,&state.prefs).into_iter().find(|i|i.path.eq_ignore_ascii_case(path)))?;
        let windows=state.windows.iter().filter(|w|w.path.eq_ignore_ascii_case(path)&&
            (state.prefs.all_windows||rect_intersects(w.rect,bar.bounds))).take(24)
            .filter_map(|w|Some((w.clone(),WindowIdentity::read(w.hwnd)?))).collect::<Vec<_>>();
        Some((item,windows,advanced))
    })else{return;};
    let foreground=GetForegroundWindow();
    windows.sort_by_key(|(window,_)|(window.hwnd!=foreground,IsIconic(window.hwnd)!=0));
    let name=basename(&item.path);
    let mut cards=Vec::new();
    let mut add=|title:&str,hint:&str,operation|cards.push(Card{icon:None,title:title.into(),hint:hint.into(),
        action:Some(Command::App{path:item.path.clone(),operation})});
    if !picker{
        if windows.len()<2{
            add(if windows.is_empty(){"Mở ứng dụng"}else{"Chuyển đến cửa sổ"},"",AppOperation::Open);
        }
        if !windows.is_empty(){
            add("Mở thêm","Ứng dụng quyết định có mở cửa sổ mới hay không",AppOperation::NewWindow);
        }
        add(if item.pinned{"Bỏ ghim"}else{"Ghim vào Dock"},"",AppOperation::TogglePin);
        if let Some((_,id))=windows.first(){
            if advanced{
                add("Quản lý cửa sổ","Thu nhỏ, phóng to, chia màn hình",AppOperation::Advanced);
            }
            add(if windows.len()>1{"Đóng cửa sổ đầu tiên"}else{"Đóng cửa sổ"},
                "Ứng dụng sẽ hỏi lưu nếu cần",AppOperation::Close(vec![*id]));
        }
        if windows.len()>1{
            add("Đóng các cửa sổ đang liệt kê","Ứng dụng sẽ hỏi lưu nếu cần",
                AppOperation::Close(windows.iter().map(|(_,id)|*id).collect()));
        }
    }
    if !picker{cards.push(Card{icon:None,title:"Thêm ứng dụng…".into(),hint:String::new(),action:Some(Command::AddApp)});}
    let all=windows.len();
    for (i,(w,id)) in windows.into_iter().enumerate(){
        let status=if w.hwnd==foreground{"Đang dùng"}else if IsIconic(w.hwnd)!=0{"Đã thu nhỏ"}else{"Đang mở"};
        cards.push(Card{icon:Some(UtilityWidget::Windows),
            title:if w.title.trim().is_empty(){name.clone()}else{w.title},
            hint:if all>1{format!("Cửa sổ {} · {status}",i+1)}else{status.into()},
            action:Some(Command::Window{hwnd:id.hwnd,pid:id.pid,tid:id.tid})});
    }
    if cards.is_empty(){return;}
    let hint=if all==0{if item.pinned{"Đã ghim".into()}else{String::new()}}
        else{format!("{all} cửa sổ{}",if item.pinned{" · Đã ghim"}else{""})};
    control_center::show(owner,dock_app_anchor(owner,Some(path)),View{title:if picker{"Chọn cửa sổ".into()}else{name},hint,cards,metrics:None});
}
unsafe fn more_apps(owner:HWND){
    use control_center::{Card,Command,View};
    let cards=STATE.with(|s|{
        let state=s.borrow();
        let Some(bar)=state.bars.iter().find(|b|b.hwnd==owner&&!b.hidden)else{return Vec::new();};
        available_items(&state.windows,bar.bounds,&state.prefs).into_iter().skip(MAX_ITEMS).take(48).map(|item|Card{
            icon:Some(UtilityWidget::Windows), title:basename(&item.path),
            hint:if item.pinned{"Đã ghim"}else{"Đang mở"}.into(),
            action:Some(Command::App{path:item.path,operation:AppOperation::ShowMenu}),
        }).collect::<Vec<_>>()
    });
    if !cards.is_empty(){
        control_center::show(owner,dock_app_anchor(owner,None),View{title:"Ứng dụng khác".into(),hint:"Ứng dụng ngoài hàng chính của Dock".into(),cards,metrics:None});
    }
}
unsafe fn perform_app_action(owner:HWND,path:&str,operation:AppOperation){
    let selected=STATE.with(|s|{
        let state=s.borrow();
        let bar_index=state.bars.iter().position(|bar|bar.hwnd==owner&&!bar.hidden)?;
        let bar=&state.bars[bar_index];
        let index=bar.items.iter().position(|item|item.path.eq_ignore_ascii_case(path));
        let item=index.map(|i|bar.items[i].clone()).or_else(||available_items(&state.windows,bar.bounds,&state.prefs).into_iter()
            .find(|item|item.path.eq_ignore_ascii_case(path)))?;
        Some((bar_index,index,item))
    });
    let Some((bar,index,mut item))=selected else{return;};
    match operation{
        AppOperation::ShowMenu=>app_menu(owner,path,false),
        AppOperation::Open=>activate(owner,&item),
        AppOperation::NewWindow=>{item.hwnd=ptr::null_mut();activate(owner,&item);},
        AppOperation::TogglePin=>emit(Event::Edit(if item.pinned{Edit::Unpin(item.path)}else{Edit::Pin(item.path)})),
        AppOperation::Close(ids)=>{
            let safe=STATE.with(|s|{
                let state=s.borrow();
                let Some(bar)=state.bars.iter().find(|b|b.hwnd==owner&&!b.hidden)else{return Vec::new();};
                state.windows.iter().filter(|w|w.path.eq_ignore_ascii_case(path)&&
                    (state.prefs.all_windows||rect_intersects(w.rect,bar.bounds))&&ids.iter().any(|id|id.matches(w.hwnd)))
                    .map(|w|w.hwnd).collect::<Vec<_>>()
            });
            for hwnd in safe{PostMessageW(hwnd,WM_CLOSE,0,0);}
        },
        AppOperation::Advanced=>{if let Some(index)=index{native_menu(owner,bar,index);}},
    }
}
unsafe fn open_dock_item(owner:HWND,item:&Item){
    if item.count>1{app_menu(owner,&item.path,true);}else{activate(owner,item);}
}
unsafe fn dock_context(owner:HWND,bar:usize,index:usize,advanced:bool){
    let path=STATE.with(|s|s.borrow().bars.get(bar).filter(|b|b.hwnd==owner)
        .and_then(|b|b.items.get(index)).map(|item|item.path.clone()));
    if let Some(path)=path.filter(|_|!advanced){app_menu(owner,&path,false);}
    else{native_menu(owner,bar,index);}
}
unsafe fn native_menu(hwnd:HWND,bar_index:usize,item_index:usize) {
    let menu=CreatePopupMenu();if menu.is_null(){return;}
    let prefs=STATE.with(|s|s.borrow().prefs.clone());
    let (selected,choices)=STATE.with(|s|{
        let state=s.borrow();
        let Some(bar)=state.bars.get(bar_index)else{return (None,Vec::new());};
        let selected=bar.items.get(item_index).cloned();
        let choices=selected.as_ref().map(|item|state.windows.iter().filter(|w|w.path.eq_ignore_ascii_case(&item.path)
            &&(prefs.all_windows||rect_intersects(w.rect,bar.bounds))).take(24)
            .map(|w|(w.clone(),window_owner(w.hwnd))).collect::<Vec<_>>()).unwrap_or_default();
        (selected,choices)
    });
    let mut destinations=Vec::new();
    let mut owner=None;
    if let Some(item)=selected.as_ref(){
        AppendMenuW(menu,MF_STRING,if item.pinned{MENU_UNPIN}else{MENU_PIN},wide(if item.pinned{"Bỏ ghim"}else{"Ghim ứng dụng"}).as_ptr());
        if !item.hwnd.is_null() {
            owner=window_owner(item.hwnd);
            if choices.len()>1{
                AppendMenuW(menu,MF_STRING,MENU_WINDOW_FIRST,wide("Chọn cửa sổ…").as_ptr());
            }
            AppendMenuW(menu,MF_STRING,MENU_MINIMIZE,wide("Thu nhỏ cửa sổ").as_ptr());
            AppendMenuW(menu,MF_STRING,MENU_MAXIMIZE,wide("Phóng to theo vùng làm việc").as_ptr());
            AppendMenuW(menu,MF_STRING,MENU_RESTORE,wide("Khôi phục kích thước").as_ptr());
            AppendMenuW(menu,MF_STRING,MENU_SNAP_LEFT,wide("Chia nửa trái").as_ptr());
            AppendMenuW(menu,MF_STRING,MENU_SNAP_RIGHT,wide("Chia nửa phải").as_ptr());
            AppendMenuW(menu,MF_STRING,MENU_EXPAND,wide("Lấp vùng làm việc (giữ kiểu cửa sổ)").as_ptr());
            if owner.is_some()&&IsIconic(item.hwnd)==0&&IsZoomed(item.hwnd)==0{
                let outputs=displays();
                if let Some(source)=outputs.iter().find(|d|stage_window_is_local(item.hwnd,d.bounds)){
                    if !window_fullscreen(item.hwnd,source.bounds){
                        destinations=outputs.iter().filter(|d|d.id!=source.id).take(24).cloned().collect();
                        if !destinations.is_empty(){
                            let list=CreatePopupMenu();
                            if !list.is_null(){
                                for (i,d) in destinations.iter().enumerate(){
                                    AppendMenuW(list,MF_STRING,MENU_MOVE_FIRST+i,wide(&d.name.replace('&',"&&")).as_ptr());
                                }
                                if AppendMenuW(menu,MF_POPUP,list as usize,wide("Chuyển cửa sổ sang màn hình").as_ptr())==0{DestroyMenu(list);}
                            }
                        }
                    }
                }
            }
        }
        AppendMenuW(menu,MF_SEPARATOR,0,ptr::null());
    }
    AppendMenuW(menu,MF_STRING,MENU_ADD_APP,wide("Thêm ứng dụng…").as_ptr());
    let extra=STATE.with(|s|{
        let state=s.borrow();
        state.bars.get(bar_index).is_some_and(|b|available_items(&state.windows,b.bounds,&prefs).len()>MAX_ITEMS)
    });
    if extra{AppendMenuW(menu,MF_STRING,MENU_MORE_APPS,wide("Ứng dụng khác…").as_ptr());}
    AppendMenuW(menu,MF_SEPARATOR,0,ptr::null());
    AppendMenuW(menu,MF_STRING|if prefs.all_windows{MF_CHECKED}else{0},MENU_ALL,wide("Hiện ứng dụng từ mọi màn hình").as_ptr());
    AppendMenuW(menu,MF_STRING|if !prefs.all_windows{MF_CHECKED}else{0},MENU_LOCAL,wide("Chỉ ứng dụng trên màn hình này").as_ptr());
    AppendMenuW(menu,MF_SEPARATOR,0,ptr::null());
    for (id,text) in [(MENU_STAGE_LEFT,"Stage Manager bên trái"),(MENU_STAGE_RIGHT,"Stage Manager bên phải"),(MENU_STAGE_OFF,"Tắt Stage Manager")] {
        AppendMenuW(menu,MF_STRING|if (id==MENU_STAGE_LEFT && prefs.stage_enabled && prefs.stage_side=="left")||
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
    let key_target=STATE.with(|state|{
        let state=state.borrow();
        let bar=state.bars.get(bar_index)?;
        if bar.keyboard!=Some(item_index){return None;}
        let item=bar.layout.as_ref()?.slots.get(item_index)?;
        Some(POINT{x:(item.x+item.w*0.5) as i32,y:item.y as i32})
    });
    if let Some(at)=key_target{point=at;ClientToScreen(hwnd,&mut point);}
    SetForegroundWindow(hwnd);
    let cmd=TrackPopupMenu(menu,TPM_RETURNCMD|TPM_RIGHTBUTTON,point.x,point.y,0,hwnd,ptr::null()) as usize;
    DestroyMenu(menu);
    match cmd {
        MENU_ADD_APP=>pin_from_file(hwnd),
        MENU_MORE_APPS=>more_apps(hwnd),
        MENU_WINDOW_FIRST=>{
            let mut anchor=panel_chrome::Anchor::for_owner(hwnd,point);
            if let Some(at)=STATE.with(|s|{
                let s=s.borrow();
                let b=s.bars.iter().find(|b|b.hwnd==hwnd)?;
                let layout=b.layout.as_ref()?;
                let path=&selected.as_ref()?.path;
                let index=b.items.iter().position(|i|&i.path==path)?;
                let r=layout.slots[index];
                let (x,y)=match layout.side{
                    DockSide::Bottom=>(r.x+r.w*0.5,layout.glass.y),
                    DockSide::Left=>(layout.glass.x+layout.glass.w,r.y+r.h*0.5),
                    DockSide::Right=>(layout.glass.x,r.y+r.h*0.5),
                };
                Some(POINT{x:x as i32,y:y as i32})
            }) {
                anchor.point=at;ClientToScreen(hwnd,&mut anchor.point);
            }
            let cards=choices.into_iter().filter_map(|(w,id)|{
                let (pid,tid)=id?;
                Some(control_center::Card{icon:Some(UtilityWidget::Windows),title:w.title,
                    hint:if IsIconic(w.hwnd)!=0{"Đã thu nhỏ"}else{"Đang mở"}.into(),
                    action:Some(control_center::Command::Window{hwnd:w.hwnd as usize,pid,tid})})
            }).collect();
            control_center::show(hwnd,anchor,control_center::View{title:"Chọn cửa sổ".into(),
                hint:selected.map_or(String::new(),|i|basename(&i.path)),cards,metrics:None});
        },
        id if (MENU_MOVE_FIRST..MENU_MOVE_FIRST+24).contains(&id)=>{
            if let Some((item,target))=selected.as_ref().zip(destinations.get(id-MENU_MOVE_FIRST)){
                if owner.is_some()&&window_owner(item.hwnd)==owner{
                    move_normal_window(item.hwnd,&target.id);
                }
            }
        },
        MENU_ALL|MENU_LOCAL => emit(Event::Edit(Edit::AllWindows(cmd==MENU_ALL))),
        MENU_PIN|MENU_UNPIN => if let Some(item)=selected {
            emit(Event::Edit(if cmd==MENU_PIN{Edit::Pin(item.path)}else{Edit::Unpin(item.path)}));
        },
        MENU_STAGE_LEFT|MENU_STAGE_RIGHT=>emit(Event::Edit(Edit::StageSide(if cmd==MENU_STAGE_LEFT{"left"}else{"right"}.into()))),
        MENU_STAGE_OFF=>emit(Event::Edit(Edit::StageEnabled(false))),
        MENU_MINIMIZE|MENU_MAXIMIZE|MENU_RESTORE|MENU_SNAP_LEFT|MENU_SNAP_RIGHT|MENU_EXPAND=>if let Some(item)=selected{
            if owner.is_some()&&window_owner(item.hwnd)==owner {
                match cmd {
                    MENU_MINIMIZE=>{let ok=ShowWindowAsync(item.hwnd,SW_MINIMIZE)!=0;show_feedback(hwnd,&item,if ok{FeedbackKind::Minimize}else{FeedbackKind::Failed});}
                    MENU_MAXIMIZE=>{ShowWindowAsync(item.hwnd,SW_MAXIMIZE);}
                    MENU_RESTORE=>{let ok=ShowWindowAsync(item.hwnd,SW_RESTORE)!=0;show_feedback(hwnd,&item,if ok{FeedbackKind::Restore}else{FeedbackKind::Failed});}
                    MENU_SNAP_LEFT|MENU_SNAP_RIGHT|MENU_EXPAND=>{
                        let screen=displays().into_iter().find(|s|stage_window_is_local(item.hwnd,s.bounds));
                        if let Some(screen)=screen {
                            ShowWindowAsync(item.hwnd,SW_RESTORE);
                            let mut r=screen.work;
                            if cmd!=MENU_EXPAND{
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
unsafe fn window_owner(hwnd:HWND)->Option<(u32,u32)>{
    let mut process=0;
    let thread=GetWindowThreadProcessId(hwnd,&mut process);
    (process!=0&&thread!=0&&process!=std::process::id()).then_some((process,thread))
}
unsafe fn move_normal_window(hwnd:HWND,target_id:&str){
    if IsIconic(hwnd)!=0||IsZoomed(hwnd)!=0||IsWindowVisible(hwnd)==0{return;}
    let screens=displays();
    let Some(source)=screens.iter().find(|d|stage_window_is_local(hwnd,d.bounds))else{return;};
    let Some(target)=screens.iter().find(|d|d.id==target_id&&d.id!=source.id)else{return;};
    if window_fullscreen(hwnd,source.bounds){return;}
    let mut from:RECT=mem::zeroed();
    if GetWindowRect(hwnd,&mut from)==0{return;}
    if let Some(to)=geometry::transfer_window(from,source,target){
        SetWindowPos(hwnd,ptr::null_mut(),to.left,to.top,to.right-to.left,to.bottom-to.top,
            SWP_ASYNCWINDOWPOS|SWP_NOACTIVATE|SWP_NOZORDER);
    }
}
unsafe fn stage_menu(hwnd:HWND,x:i32,y:i32){
    let card=STATE.with(|s|{
        let state=s.borrow();
        let stage=state.stages.iter().find(|p|p.hwnd==hwnd)?;
        let i=stage.layout?.hit(x,y)?;
        let item=stage.items.get(i)?;
        let local=stage_window_is_local(item.hwnd,stage.bounds);
        let group=stage.groups.get(i).copied().flatten();
        let members=group.map(|id|state.workspaces.members(id)).unwrap_or_default().into_iter()
            .filter_map(|hwnd|{
                let owner=window_owner(hwnd)?;
                state.windows.iter().find(|w|w.hwnd==hwnd).map(|w|(hwnd,owner,w.title.clone()))
            })
            .collect::<Vec<_>>();
        Some((item.hwnd,window_owner(item.hwnd)?,group,stage.display.clone(),stage.selected,local,state.workspaces.can_create(&stage.display),members))
    });
    let Some((item,owner,group,display,current,local,can_create,members))=card else{return;};
    let menu=CreatePopupMenu();
    if menu.is_null(){return;}
    const DETACH_FIRST:usize=100;
    let add=|id,title:&str,active|{
        AppendMenuW(menu,MF_STRING|if active{0}else{MF_GRAYED},id,wide(title).as_ptr());
    };
    add(1,"Tạo nhóm từ cửa sổ này",local&&group.is_none()&&can_create);
    add(2,"Thêm vào nhóm đang mở",local&&group.is_none()&&current.is_some());
    if !members.is_empty(){
        let submenu=CreatePopupMenu();
        if !submenu.is_null(){
            for (index,(_,_,title)) in members.iter().enumerate(){
                let label=title.replace('&',"&&").chars().take(90).collect::<String>();
                AppendMenuW(submenu,MF_STRING,DETACH_FIRST+index,wide(&label).as_ptr());
            }
            if AppendMenuW(menu,MF_POPUP,submenu as usize,wide("Tách cửa sổ khỏi nhóm").as_ptr())==0{
                DestroyMenu(submenu);
            }
        }
    }
    add(4,"Giải tán nhóm và khôi phục cửa sổ",group.is_some());
    AppendMenuW(menu,MF_SEPARATOR,0,ptr::null());
    add(5,"Điều hướng Stage bằng bàn phím",true);
    let mut point=POINT{x,y};ClientToScreen(hwnd,&mut point);
    // A no-activate tool window needs a temporary foreground owner while its
    // native menu is open, so Escape and clicks outside dismiss the menu.
    let previous=GetForegroundWindow();
    SetForegroundWindow(hwnd);
    let result=TrackPopupMenu(menu,TPM_RETURNCMD|TPM_NONOTIFY,point.x,point.y,0,hwnd,ptr::null()) as usize;
    DestroyMenu(menu);
    if GetForegroundWindow()==hwnd && IsWindow(previous)!=0 {SetForegroundWindow(previous);}
    if result==5{
        let selected=STATE.with(|s|{
            let mut state=s.borrow_mut();
            let Some(p)=state.stages.iter_mut().find(|p|p.hwnd==hwnd&&!p.hidden)else{return false;};
            let Some(i)=p.items.iter().position(|e|e.hwnd==item)else{return false;};
            p.keyboard=Some(i);p.previous_focus=previous as usize;true
        });
        if selected{SetForegroundWindow(hwnd);SetFocus(hwnd);InvalidateRect(hwnd,ptr::null(),0);}
        return;
    }
    if result==0{return;}
    update_state(|state|{
        // The menu can run a nested loop. Revalidate the original source and
        // display against the current enumeration before changing a workspace.
        let Some(stage)=state.stages.iter().find(|stage|stage.hwnd==hwnd&&stage.display==display)else{return;};
        let current_card=if let Some(id)=group {
            stage.groups.contains(&Some(id))
        }else{
            window_owner(item)==Some(owner)&&stage.items.iter().enumerate()
                .any(|(i,entry)|entry.hwnd==item&&stage.groups[i].is_none())
        };
        if !current_card{return;}
        let present=state.windows.iter().any(|w|w.hwnd==item);
        let still_local=present && stage_window_is_local(item,stage.bounds);
        match result{
            1 if still_local&&group.is_none()=>{state.workspaces.create(&display,item);}
            2 if still_local&&group.is_none()&&current==stage.selected=>state.workspaces.add(&display,item),
            4=>{if let Some(id)=group{state.workspaces.dissolve(id);}},
            id if id>=DETACH_FIRST=>{
                if let Some((group,(target,owner,_)))=group.zip(members.get(id-DETACH_FIRST)){
                    if window_owner(*target)==Some(*owner){state.workspaces.remove_from(group,*target);}
                }
            }
            _=>{}
        }
        refresh(state,false);
    });
}
unsafe fn activate_stage(hwnd:HWND,i:usize){
    let selected=STATE.with(|s|{
        let state=s.borrow();
        let stage=state.stages.iter().find(|p|p.hwnd==hwnd)?;
        Some((stage.items.get(i)?.clone(),stage.groups.get(i).copied().flatten(),stage.display.clone()))
    });
    if let Some((item,group,display))=selected {
        if let Some(id)=group{
            let mut focus=None;
            update_state(|state|{
                focus=state.workspaces.switch(&display,id);
                let reduced=state.prefs.appearance.reduced_motion;
                if let Some(p)=state.stages.iter_mut().find(|p|p.hwnd==hwnd&&p.selected!=Some(id)){p.begin_slide(reduced);}
                refresh(state,false);
            });
            if let Some(hwnd)=focus{SetForegroundWindow(hwnd);}
        }else{activate(hwnd,&item);}
    }
}
unsafe fn turn_stage_page(hwnd:HWND,forward:bool)->bool{
    let next=STATE.with(|s|{
        let s=s.borrow();
        let p=s.stages.iter().find(|p|p.hwnd==hwnd)?;
        Some(if forward{p.page+1}else{p.page.saturating_sub(1)})
    });
    next.is_some_and(|n|go_to_stage_page(hwnd,n))
}
unsafe fn go_to_stage_page(hwnd:HWND,next:usize)->bool{
    let changed=STATE.with(|s|{
        let mut state=s.borrow_mut();
        let reduced=state.prefs.appearance.reduced_motion;
        let Some(stage)=state.stages.iter_mut().find(|p|p.hwnd==hwnd) else{return false;};
        let next=next.min(stage.pages-1);
        if next==stage.page{return false;}
        stage.page=next;
        stage.begin_slide(reduced);
        true
    });
    if changed{update_state(|state|refresh(state,false));InvalidateRect(hwnd,ptr::null(),0);}
    changed
}
unsafe extern "system" fn proc(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
    if msg!=0&&SHELL_MESSAGE.with(Cell::get)==msg&&CONTROL.with(Cell::get)==hwnd{
        SHELL_RESTARTED.with(|f|f.set(true));DISPLAY_DIRTY.with(|f|f.set(true));queue_refresh();return 0;
    }
    if msg!=0&&ATTENTION_MSG.with(Cell::get)==msg&&CONTROL.with(Cell::get)==hwnd{
        if w==HSHELL_FLASH&&STATE.with(|state|state.borrow().prefs.enabled){
            let target=l as HWND;
            if target!=GetForegroundWindow(){
                if let Some(id)=window_owner(target){
                    ATTENTION.with(|queue|{
                        let mut queue=queue.borrow_mut();
                        if queue.len()<64{queue.insert(l as usize,id);}
                    });
                    queue_refresh();
                }
            }
        }
        return 0;
    }
    match msg {
        appbar::CALLBACK=>{if w as u32==ABN_POSCHANGED{APPBAR_DIRTY.with(|f|f.set(true));queue_refresh();}0},
        WM_CONFIG=>{drain_commands();0},
        WM_TICK=>{SetTimer(hwnd,2,60,None);0},
        WM_UTILITY_READY=>{show_utilities();0},
        WM_ICON_CHANGED=>{
            let changed=ATLAS.with(|a|a.borrow_mut().as_mut().is_some_and(|worker|worker.receive()));
            if changed{for h in STATE.with(|s|{let state=s.borrow();state.bars.iter().filter(|b|!b.hidden).map(|b|b.hwnd)
                .chain(state.stages.iter().filter(|p|!p.hidden).map(|p|p.hwnd)).collect::<Vec<_>>()}){
                InvalidateRect(h,ptr::null(),0);
            }}
            0
        },
        WM_NCDESTROY=>{
            close_status_tip(hwnd);
            perf::forget(hwnd as usize);
            STAGE_SURFACES.with(|s|s.borrow_mut().remove(&(hwnd as usize)));
            SURFACES.with(|s|s.borrow_mut().remove(&(hwnd as usize)));
            DefWindowProcW(hwnd,msg,w,l)
        },
        WM_DOCK_MOUSELEAVE=>{
            KillTimer(hwnd,STATUS_TIP_TIMER);close_status_tip(hwnd);
            let stage=STATE.with(|s|{
                let mut state=s.borrow_mut();
                let reduced=state.prefs.appearance.reduced_motion;
                state.stages.iter_mut().find(|p|p.hwnd==hwnd).map(|p|{
                    let changed=p.hover.take().is_some();
                    p.pressed=None;
                    (changed,changed&&p.emphasis.change(None,reduced),p.auto_hide&&p.keyboard.is_none())
                })
            });
            if let Some((changed,animate,auto))=stage{
                if auto{SetTimer(hwnd,HIDE_TIMER,650,None);}
                if animate{SetTimer(hwnd,STAGE_MOTION_TIMER,16,None);}
                if changed{InvalidateRect(hwnd,ptr::null(),0);}
                return 0;
            }
            let tracked=STATE.with(|s|{
                let mut state=s.borrow_mut();
                let animate=!state.prefs.appearance.reduced_motion;
                if let Some(b)=state.bars.iter_mut().find(|b|b.hwnd==hwnd){
                    b.hovered=-1;b.status_hover=None;b.motion.target=None;b.pointer=None;
                    // Outside the visible surface is never a valid drop.
                    // Cancel here even if Win32 won't deliver the release.
                    b.pressed=None;
                    if !animate{b.motion=Motion::default();}
                    let start=animate&&b.layout.is_some()&&!b.motion.scheduled&&b.motion.zoom.value>0.001;
                    b.motion.scheduled|=start;
                    (true,start,b.auto_hide&&b.layout.is_some()&&b.keyboard.is_none()&&b.pressed.is_none())
                }else{(false,false,false)}
            });
            if GetCapture()==hwnd{ReleaseCapture();}
            if tracked.1{SetTimer(hwnd,MOTION_TIMER,16,None);}
            if tracked.2{SetTimer(hwnd,HIDE_TIMER,650,None);}
            if tracked.0{InvalidateRect(hwnd,ptr::null(),0);}
            0
        },
        WM_TIMER if w==STAGE_MOTION_TIMER=>{
            let step=STATE.with(|s|{
                let mut state=s.borrow_mut();
                let reduced=state.prefs.appearance.reduced_motion;
                let speed=state.prefs.appearance.animation_speed;
                let p=state.stages.iter_mut().find(|p|p.hwnd==hwnd&&!p.hidden)?;
                let duration=|start:std::time::Instant,ms:f32|if reduced{1.0}else{(start.elapsed().as_secs_f32()*speed as f32*10.0/ms).clamp(0.0,1.0)};
                let emphasis=p.emphasis.start.map(|s|p.emphasis.tick(p.hover,duration(s,160.0)));
                let mut next_position=None;
                if let Some((start,from))=p.slide {
                    let t=duration(start,195.0);
                    let offset=(from as f32*(1.0-t).powi(3)).round() as i32;
                    if offset!=p.offset{
                        p.offset=offset;
                        next_position=Some((p.preview_hwnd,p.resting,offset));
                    }
                    if t>=1.0{p.slide=None;}
                }
                Some((emphasis.is_some(),emphasis==Some(true)||p.slide.is_some(),next_position))
            });
            if let Some((redraw,again,position))=step{
                if let Some((preview,rect,offset))=position{position_stage(hwnd,preview,rect,offset);}
                if redraw{InvalidateRect(hwnd,ptr::null(),0);}
                if !again{KillTimer(hwnd,STAGE_MOTION_TIMER);}
            }else{KillTimer(hwnd,STAGE_MOTION_TIMER);}
            0
        },
        WM_TIMER if w==STATUS_TIP_TIMER=>{KillTimer(hwnd,STATUS_TIP_TIMER);show_status_tip(hwnd);0},
        WM_TIMER if w==FEEDBACK_TIMER=>{
            KillTimer(hwnd,FEEDBACK_TIMER);
            STATE.with(|s|{if let Some(bar)=s.borrow_mut().bars.iter_mut().find(|b|b.hwnd==hwnd){bar.feedback=None;}});
            InvalidateRect(hwnd,ptr::null(),0);0
        },
        WM_TIMER if w==HIDE_TIMER=>{
            KillTimer(hwnd,HIDE_TIMER);
            let stage=STATE.with(|state|state.borrow().stages.iter().any(|p|p.hwnd==hwnd&&p.auto_hide&&p.keyboard.is_none()));
            if stage{
                let mut point=POINT{x:0,y:0};GetCursorPos(&mut point);
                let mut bounds:RECT=mem::zeroed();GetWindowRect(hwnd,&mut bounds);
                if point.x>=bounds.left&&point.x<bounds.right&&point.y>=bounds.top&&point.y<bounds.bottom{return 0;}
                update_state(|s|{if let Some(p)=s.stages.iter_mut().find(|p|p.hwnd==hwnd){p.collapsed=true;}refresh(s,false);});
                return 0;
            }
            let open=utilities::panel_open_for(hwnd);
            let mut pt:POINT=mem::zeroed();GetCursorPos(&mut pt);
            if open||WindowFromPoint(pt)==hwnd{return 0;}
            let start=STATE.with(|state|{
                let mut state=state.borrow_mut();
                let reduced=state.prefs.appearance.reduced_motion;
                state.bars.iter_mut().find(|b|b.hwnd==hwnd&&b.auto_hide&&b.layout.is_some()&&b.keyboard.is_none())
                    .is_some_and(|b|b.collapse(true,reduced))
            });
            if start{SetTimer(hwnd,MOTION_TIMER,16,None);}
            queue_refresh();
            InvalidateRect(hwnd,ptr::null(),0);
            0
        },
        WM_TIMER if w==MOTION_TIMER=>{
            let motion=STATE.with(|s|{
                let mut state=s.borrow_mut();
                let State{screens,prefs,bars,appbars,..}=&mut *state;
                let b=bars.iter_mut().find(|b|b.hwnd==hwnd)?;
                if b.hidden || b.layout.is_none(){return None;}
                let display=screens.iter().find(|d|same_rect(d.bounds,b.bounds))?;
                let now=std::time::Instant::now();
                let old_zoom=b.motion.zoom.value;
                let old_focus=b.motion.focus_value();
                let mut again=b.motion.tick(now,prefs.appearance.animation_speed,&mut b.reveal,!b.collapsed);
                if old_zoom!=b.motion.zoom.value||old_focus!=b.motion.focus_value(){
                    let (placement,_)=appbars.placement(display,false);
                    let layout=geometry::measure_cluster(&placement,&prefs.appearance,b.items.len(),b.motion.focus_value(),b.motion.zoom.value,&b.pinned_widgets,prefs.utility_cluster(&display.id));
                    b.hovered=b.pointer.and_then(|(x,y)|layout.hot(x,y)).map_or(-1,|i|i as i32);
                    b.layout=Some(layout);
                }
                if b.feedback.as_ref().is_some_and(|f|now.duration_since(f.start).as_millis()>=420){b.feedback=None;}
                again|=b.feedback.is_some();
                if !again{b.motion.at=None;b.motion.scheduled=false;}
                Some(again)
            });
            if motion.is_some(){InvalidateRect(hwnd,ptr::null(),0);}
            if motion!=Some(true){KillTimer(hwnd,MOTION_TIMER);}
            0
        },
        WM_ERASEBKGND=>1,
        WM_PAINT=>{paint(hwnd);0},
        WM_MOUSEACTIVATE=>MA_NOACTIVATE as LRESULT,
        WM_MOUSEWHEEL if STATE.with(|s|s.borrow().stages.iter().any(|stage|stage.hwnd==hwnd))=>{
            let delta=(w>>16) as i16;
            if delta!=0{turn_stage_page(hwnd,delta<0);}
            0
        },
        WM_TIMER if w==INLINE_WAKE=>{KillTimer(hwnd,INLINE_WAKE);update_state(|s|refresh(s,false));0},
        WM_TIMER if w==control_center::INLINE_TIMER=>{control_center::inline_tick(hwnd);0},
        control_center::INLINE_UPDATE=>{
            if control_center::inline_receive(w){
                STATE.with(|s|{
                    let mut state=s.borrow_mut();
                    let State{bars,screens,prefs,..}=&mut *state;
                    let now=Instant::now();
                    for b in bars.iter_mut().filter(|b|!b.hidden&&!b.collapsed&&b.layout.as_ref().is_some_and(|l|l.metrics.is_some())){
                        let Some(interval)=screens.iter().find(|d|same_rect(d.bounds,b.bounds))
                            .and_then(|d|prefs.utility_profiles.get(&d.id)).map(|p|p.metrics.interval)else{continue;};
                        if b.metric_repaint.is_none_or(|at|now.duration_since(at)>=Duration::from_millis(interval*1000-100)){
                            b.metric_repaint=Some(now);
                            InvalidateRect(b.hwnd,ptr::null(),0);
                        }
                    }
                });
            }
            0
        },
        WM_TIMER if w==WORKSPACE_TIMER=>{update_state(|state|{state.workspaces.advance();});0},
        WM_TIMER=>{if w==1||w==2{if w==2{KillTimer(hwnd,2);DIRTY.with(|d|d.set(false));}let changed = DISPLAY_DIRTY.with(|d|d.replace(false));update_state(|s|refresh(s,w==1 || changed));}0},
        WM_DISPLAYCHANGE|WM_SETTINGCHANGE|WM_DPICHANGED=>{DISPLAY_DIRTY.with(|d|d.set(true));queue_refresh();0},
        WM_MOUSEMOVE=>{
            let x=l as i16 as i32;
            let y=(l>>16) as i16 as i32;
            let stage=STATE.with(|s|{
                let mut state=s.borrow_mut();
                let reduced=state.prefs.appearance.reduced_motion;
                let p=state.stages.iter_mut().find(|p|p.hwnd==hwnd)?;
                let hit=p.layout.and_then(|layout|layout.hit(x,y));
                let old=std::mem::replace(&mut p.hover,hit);
                let changed=old!=hit;
                Some((changed,changed&&p.emphasis.change(hit,reduced),hit.is_some()))
            });
            if let Some((changed,animate,over_card))=stage{
                KillTimer(hwnd,HIDE_TIMER);
                if changed{
                    KillTimer(hwnd,STATUS_TIP_TIMER);close_status_tip(hwnd);
                    if over_card{SetTimer(hwnd,STATUS_TIP_TIMER,650,None);}
                }
                if animate{SetTimer(hwnd,STAGE_MOTION_TIMER,16,None);}
                let mut event=TRACKMOUSEEVENT{cbSize:mem::size_of::<TRACKMOUSEEVENT>() as u32,dwFlags:TME_LEAVE,hwndTrack:hwnd,dwHoverTime:0};
                TrackMouseEvent(&mut event);
                if changed{InvalidateRect(hwnd,ptr::null(),0);}
                return 0;
            }
            let (changed,animate,track,reveal)=STATE.with(|s|{
                let mut state=s.borrow_mut();
                let reduced=state.prefs.appearance.reduced_motion;
                let magnify=!reduced&&state.prefs.appearance.magnification!=0;
                let Some(b)=state.bars.iter_mut().find(|b|b.hwnd==hwnd) else{return (false,false,false,false)};
                let prev=b.hovered;
                let prev_drop=b.drop_target();
                // Capture on a non-activating window can miss an off-window
                // release. The next hover must not revive an old press.
                if w&MOUSE_LEFT_DOWN==0{b.pressed=None;}
                if let Some(press)=b.pressed.as_mut(){
                    press.track(x as f32,y as f32,b.layout.as_ref().map_or(1.0,|l|l.scale));
                }
                b.pointer=Some((x as f32,y as f32));
                let reveal=b.collapsed;
                let start_reveal=b.collapse(false,reduced);
                if let Some(layout)=b.layout.as_ref(){
                    b.hovered=layout.hot(x as f32,y as f32).map_or(-1,|i|i as i32);
                    let desired=if magnify&&layout.hit(x as f32,y as f32).is_some(){Some(layout.axis(x as f32,y as f32))}else{None};
                    let changed=desired!=b.motion.target;
                    b.motion.target=desired;
                    let start=start_reveal||(changed&&b.schedule());
                    (reveal||prev!=b.hovered||prev_drop!=b.drop_target(),start,true,reveal)
                }else{
                    b.hovered=if x>=12&&(x-12)/ITEM_WIDTH<b.items.len() as i32{(x-12)/ITEM_WIDTH}else{-1};
                    (prev!=b.hovered,false,true,reveal)
                }
            });
            if track{
                KillTimer(hwnd,HIDE_TIMER);
                let mut event=TRACKMOUSEEVENT{cbSize:mem::size_of::<TRACKMOUSEEVENT>() as u32,dwFlags:TME_LEAVE,hwndTrack:hwnd,dwHoverTime:0};
                TrackMouseEvent(&mut event);
                let change=STATE.with(|s|{
                    let mut s=s.borrow_mut();
                    let b=s.bars.iter_mut().find(|b|b.hwnd==hwnd)?;
                    let layout=b.layout.as_ref()?;
                    let status=if layout.is_utility_button(x as f32,y as f32){Some(None)}
                        else{layout.inline_at(x as f32,y as f32).map(Some)};
                    if b.status_hover==status{return None;}
                    b.status_hover=status;
                    Some(status.is_some())
                });
                if let Some(show)=change{
                    KillTimer(hwnd,STATUS_TIP_TIMER);close_status_tip(hwnd);
                    if show{SetTimer(hwnd,STATUS_TIP_TIMER,650,None);}
                }
            }
            if animate{SetTimer(hwnd,MOTION_TIMER,16,None);}
            if reveal{queue_inline_refresh_if_needed(hwnd);}
            if changed{InvalidateRect(hwnd,ptr::null(),0);}
            0
        }
        WM_LBUTTONDOWN=>{
            KillTimer(hwnd,STATUS_TIP_TIMER);close_status_tip(hwnd);
            let (x,y)=(l as i16 as i32,(l>>16) as i16 as i32);
            let mut area:RECT=mem::zeroed();GetClientRect(hwnd,&mut area);
            let utility=utility_profile(hwnd).is_some_and(|p|p.enabled);
            let capture=STATE.with(|s|{
                let mut state=s.borrow_mut();
                if let Some(stage)=state.stages.iter_mut().find(|p|p.hwnd==hwnd){
                    stage.pressed=stage.target(x,y);
                    return false;
                }
                let Some(bar)=state.bars.iter_mut().find(|b|b.hwnd==hwnd&&!b.hidden&&!b.collapsed)else{return false;};
                let target=if bar.layout.is_some(){bar.target(x as f32,y as f32)}
                    else{bar.fallback_target(x,y,area.right,utility)};
                let capture=target.as_ref().is_some_and(|target|matches!(target,DockTarget::App(path)
                    if bar.items.iter().any(|item|item.pinned&&item.path==*path)));
                bar.pressed=target.map(|target|Press::new(target,x,y));
                capture
            });
            if capture{SetCapture(hwnd);}
            0
        },
        WM_CAPTURECHANGED|WM_CANCELMODE=>{
            let redraw=STATE.with(|s|{
                let mut state=s.borrow_mut();
                if let Some(stage)=state.stages.iter_mut().find(|p|p.hwnd==hwnd){stage.pressed=None;}
                state.bars.iter_mut().find(|b|b.hwnd==hwnd).is_some_and(|b|b.pressed.take().is_some())
            });
            if msg==WM_CANCELMODE&&GetCapture()==hwnd{ReleaseCapture();}
            if redraw{InvalidateRect(hwnd,ptr::null(),0);}
            if msg==WM_CANCELMODE{DefWindowProcW(hwnd,msg,w,l)}else{0}
        },
        WM_KILLFOCUS=>{
            let stage_auto=STATE.with(|s|s.borrow_mut().stages.iter_mut().find(|p|p.hwnd==hwnd)
                .is_some_and(|p|{p.keyboard=None;p.auto_hide&&!p.hidden}));
            if stage_auto{SetTimer(hwnd,HIDE_TIMER,650,None);}
            let collapse=STATE.with(|s|{
                let mut state=s.borrow_mut();
                if let Some(bar)=state.bars.iter_mut().find(|b|b.hwnd==hwnd&&b.keyboard.is_some()){
                    bar.keyboard=None;
                    bar.auto_hide&&bar.layout.is_some()
                }else{false}
            });
            if collapse{SetTimer(hwnd,HIDE_TIMER,650,None);}
            InvalidateRect(hwnd,ptr::null(),0);
            0
        },
        WM_KEYDOWN=>{
            let key=w as u16;
            let stage_action=STATE.with(|s|{
                let mut state=s.borrow_mut();
                let p=state.stages.iter_mut().find(|p|p.hwnd==hwnd)?;
                let i=p.keyboard?;
                let last=p.items.len().saturating_sub(1);
                match key{
                    VK_UP=>p.keyboard=Some(i.saturating_sub(1)),
                    VK_DOWN=>p.keyboard=Some((i+1).min(last)),
                    VK_HOME=>p.keyboard=Some(0),VK_END=>p.keyboard=Some(last),
                    _=>{}
                }
                Some((i,p.previous_focus))
            });
            if let Some((i,previous))=stage_action{
                match key{
                    VK_LEFT|VK_RIGHT|VK_PRIOR|VK_NEXT=>{if turn_stage_page(hwnd,matches!(key,VK_RIGHT|VK_NEXT)){
                        STATE.with(|s|{if let Some(p)=s.borrow_mut().stages.iter_mut().find(|p|p.hwnd==hwnd){p.keyboard=Some(0);}});
                    }},
                    VK_RETURN=>activate_stage(hwnd,i),
                    VK_APPS|VK_F10 if key==VK_APPS||GetKeyState(VK_SHIFT as i32)<0=>{
                        let point=STATE.with(|s|s.borrow().stages.iter().find(|p|p.hwnd==hwnd)
                            .and_then(|p|p.layout).filter(|l|i<l.count).map(|l|l.card(i).center()));
                        if let Some((x,y))=point{stage_menu(hwnd,x,y);}
                    },
                    VK_ESCAPE if previous!=0&&IsWindow(previous as HWND)!=0=>{SetForegroundWindow(previous as HWND);},
                    _=>{}
                }
                InvalidateRect(hwnd,ptr::null(),0);
                return 0;
            }
            if key==VK_APPS||(key==VK_F10&&GetKeyState(VK_SHIFT as i32)<0){
                let cursor=STATE.with(|s|s.borrow().bars.iter().enumerate().find(|(_,b)|b.hwnd==hwnd)
                    .and_then(|(i,b)|b.keyboard.map(|cell|(i,cell,b.items.len()))));
                if let Some((bar,cell,count))=cursor{
                    if cell<count{dock_context(hwnd,bar,cell,false);}else{request_utilities(hwnd,None);}
                }
                return 0;
            }
            let action=STATE.with(|s|{
                let mut state=s.borrow_mut();
                let bar=state.bars.iter_mut().find(|b|b.hwnd==hwnd)?;
                let current=bar.keyboard?;
                let has_utilities=bar.layout.as_ref().is_none_or(|l|l.utility.w>0.0);
                let order=bar.layout.as_ref().map_or_else(||(0..=bar.items.len()+usize::from(has_utilities)).collect(),|l|l.key_order());
                let i=order.iter().position(|id|*id==current).unwrap_or(0);
                let current=order[i];
                let next=match key{
                    VK_LEFT|VK_UP=>order[i.saturating_sub(1)],VK_RIGHT|VK_DOWN=>order[(i+1).min(order.len()-1)],
                    VK_HOME=>order[0],VK_END=>order[order.len()-1],
                    VK_RETURN=>return Some((bar.items.get(current).cloned(),current==bar.items.len(),has_utilities&&current==bar.items.len()+1,
                        current.checked_sub(bar.items.len()+2).and_then(|i|bar.layout.as_ref()?.inline.get(i)).map(|(w,_)|*w),0)),
                    VK_ESCAPE=>return Some((None,false,false,None,bar.previous_focus)),
                    _=>return None,
                };
                bar.keyboard=Some(next);
                Some((None,false,false,None,0))
            });
            if let Some((item,search,utilities,widget,previous))=action{
                if let Some(item)=item{open_dock_item(hwnd,&item);}
                if search {open_launcher();}
                if utilities||widget.is_some(){request_utilities(hwnd,widget);}
                if previous!=0 && IsWindow(previous as _)!=0{SetForegroundWindow(previous as _);}
                InvalidateRect(hwnd,ptr::null(),0);
            }
            0
        },
        WM_LBUTTONUP=>{
            let y=(l>>16) as i16 as i32;
            let px=l as i16 as i32;
            let stage_hit=STATE.with(|s|{
                let mut a=s.borrow_mut();
                let stage=a.stages.iter_mut().find(|stage|stage.hwnd==hwnd)?;
                let pressed=stage.pressed.take();
                let target=stage.target(px,y).filter(|target|Some(*target)==pressed);
                Some((target,stage.layout.and_then(|l|l.hit(px,y))))
            });
            if let Some((target,item))=stage_hit {
                match target{
                    Some(StageTarget::Page(page))=>{go_to_stage_page(hwnd,page);}
                    Some(_)=>{if let Some(i)=item{activate_stage(hwnd,i);}}
                    None=>{}
                }
                return 0;
            }
            let mut area:RECT=mem::zeroed();GetClientRect(hwnd,&mut area);
            let utility=utility_profile(hwnd).is_some_and(|p|p.enabled);
            let action=STATE.with(|s|{
                let mut state=s.borrow_mut();let bar=state.bars.iter_mut().find(|b|b.hwnd==hwnd)?;
                let scale=bar.layout.as_ref().map_or(1.0,|layout|layout.scale);
                let target=if bar.layout.is_some(){bar.target(px as f32,y as f32)}
                    else{bar.fallback_target(px,y,area.right,utility)};
                let drop=bar.drop_at(px as f32,y as f32).and_then(|index|{
                    let pinned=bar.items.iter().take_while(|item|item.pinned).count();
                    let after=index==pinned;
                    Some((bar.items.get(index-usize::from(after))?.path.clone(),after))
                });
                let press=bar.pressed.take()?;
                let click=press.click(target,px as f32,y as f32,scale);
                let item=match &click{
                    Some(DockTarget::App(path))=>bar.items.iter().find(|item|item.path==*path).cloned(),
                    _=>None,
                };
                Some((press,click,item,drop,bar.auto_hide&&bar.keyboard.is_none()))
            });
            if GetCapture()==hwnd{ReleaseCapture();}
            if let Some((press,click,item,drop,auto))=action{
                InvalidateRect(hwnd,ptr::null(),0);
                if let (DockTarget::App(path),Some((anchor,after)))=(press.target,drop){
                    emit(Event::Edit(Edit::MovePin{path,anchor,after}));
                }
                if auto&&(px<0||y<0||px>=area.right||y>=area.bottom){SetTimer(hwnd,HIDE_TIMER,650,None);}
                match click{
                    Some(DockTarget::App(_))=>{if let Some(item)=item{open_dock_item(hwnd,&item);}},
                    Some(DockTarget::Launcher)=>open_launcher(),
                    Some(DockTarget::Overflow)=>request_utilities(hwnd,None),
                    Some(DockTarget::Utility(widget))=>request_utilities(hwnd,Some(widget)),
                    None=>{},
                }
            }
            0
        }
        WM_RBUTTONUP=>{
            if STATE.with(|s|s.borrow().stages.iter().any(|stage|stage.hwnd==hwnd)){
                stage_menu(hwnd,l as i16 as i32,(l>>16) as i16 as i32);return 0;
            }
            let x=(l as u32&0xffff) as i16 as i32;
            let y=(l>>16) as i16 as i32;
            let on_status=STATE.with(|s|s.borrow().bars.iter().find(|b|b.hwnd==hwnd&&!b.collapsed)
                .and_then(|b|b.layout.as_ref())
                .is_some_and(|layout|layout.is_utility_button(x as f32,y as f32)||layout.inline_at(x as f32,y as f32).is_some()));
            if on_status{KillTimer(hwnd,STATUS_TIP_TIMER);close_status_tip(hwnd);request_utilities(hwnd,None);return 0;}
            let (bar,index)=STATE.with(|s|{let a=s.borrow();let i=a.bars.iter().position(|b|b.hwnd==hwnd).unwrap_or(0);
              let y=(l>>16) as i16 as f32;
              let hit=a.bars.get(i).and_then(|b|
                  if let Some(layout)=b.layout.as_ref(){layout.hit(x as f32,y)}
                  else if x>=12{Some(((x-12)/ITEM_WIDTH) as usize)}else{None});
              (i,hit.unwrap_or(usize::MAX))});
            dock_context(hwnd,bar,index,GetKeyState(VK_SHIFT as i32)<0);0
        }
        WM_CLOSE=>{
            if CONTROL.with(|c|c.get())==hwnd{EndMenu();PostQuitMessage(0);}
            else if STATE.with(|s|s.borrow().stages.iter().any(|stage|stage.hwnd==hwnd)){emit(Event::Edit(Edit::StageEnabled(false)));}
            else{emit(Event::Edit(Edit::DockEnabled(false)));}0
        },
        _=>DefWindowProcW(hwnd,msg,w,l),
    }
}
unsafe extern "system" fn stage_rail_proc(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
    match msg{
        WM_MOUSEACTIVATE=>MA_NOACTIVATE as LRESULT,
        WM_PAINT=>{
            let mut paint:PAINTSTRUCT=mem::zeroed();
            let dc=BeginPaint(hwnd,&mut paint);
            let mut area:RECT=mem::zeroed();GetClientRect(hwnd,&mut area);
            fill(dc,area,0x00917d68);
            EndPaint(hwnd,&paint);0
        }
        WM_MOUSEMOVE=>{
            let mut stage=ptr::null_mut();
            update_state(|s|{
                let reduced=s.prefs.appearance.reduced_motion;
                if let Some(p)=s.stages.iter_mut().find(|p|p.rail==hwnd){p.collapsed=false;p.begin_slide(reduced);stage=p.hwnd;}
                if !stage.is_null(){refresh(s,false);}
            });
            if !stage.is_null(){SetTimer(stage,HIDE_TIMER,850,None);}
            0
        }
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
                close_status_tip(ptr::null_mut());
                if state.prefs.appearance!=options.appearance||state.prefs.utility_profiles!=options.utility_profiles{utilities::close_panels();}
                for b in &mut state.bars{KillTimer(b.hwnd,STATUS_TIP_TIMER);b.status_hover=None;b.metric_repaint=None;}
                let repaint=state.prefs.appearance!=options.appearance;
                if repaint{state.material=None;}
                if repaint { for b in &mut state.bars{
                    KillTimer(b.hwnd,MOTION_TIMER);KillTimer(b.hwnd,HIDE_TIMER);KillTimer(b.hwnd,FEEDBACK_TIMER);
                    b.motion=Motion::default();b.feedback=None;b.reveal=motion::Spring::rest(if b.collapsed{0.0}else{1.0});
                } }
                state.prefs=options;refresh(state,false);
                if repaint{for b in &state.bars{InvalidateRect(b.hwnd,ptr::null(),0);}}
            }),
            Command::Suspend(on)=>update_state(|state|{if state.suspended!=on{state.suspended=on;refresh(state,!on);}}),
            Command::Stop=>{control_center::inline_config(CONTROL.with(Cell::get),None);EndMenu();PostQuitMessage(0);},
            Command::Focus=>focus_dock(),
            #[cfg(test)] Command::Inspect{reply,paint}=>{
                let handles:Vec<usize>=STATE.with(|s|{let s=s.borrow();s.bars.iter().map(|b|b.hwnd as usize)
                    .chain(s.stages.iter().map(|s|s.hwnd as usize)).collect()});
                if paint {
                    for &handle in &handles {
                        let hwnd=handle as HWND;
                        if IsWindowVisible(hwnd)!=0 {
                            InvalidateRect(hwnd,ptr::null(),0);
                            UpdateWindow(hwnd);
                        }
                    }
                }
                let _=reply.send(handles);
            },
        }
    }
}
unsafe fn focus_dock(){
    let foreground=GetForegroundWindow();
    let mut pos:POINT=mem::zeroed();GetCursorPos(&mut pos);
    let target=STATE.with(|s|{
        let mut state=s.borrow_mut();
        let idx=state.bars.iter().position(|bar|!bar.hidden&&pos.x>=bar.bounds.left&&pos.x<bar.bounds.right&&pos.y>=bar.bounds.top&&pos.y<bar.bounds.bottom)
            .or_else(||state.bars.iter().position(|b|!b.hidden))?;
        let bar=&mut state.bars[idx];
        let reveal=bar.collapsed;
        bar.collapse(false,true);bar.keyboard=Some(bar.layout.as_ref().map_or(0,|l|l.key_order()[0]));bar.previous_focus=foreground as isize;
        Some((bar.hwnd,reveal))
    });
    if let Some((hwnd,reveal))=target{
        if reveal{queue_inline_refresh_if_needed(hwnd);}
        KillTimer(hwnd,HIDE_TIMER);
        SetForegroundWindow(hwnd);
        SetFocus(hwnd);
        InvalidateRect(hwnd,ptr::null(),0);
    }
}
fn run(options:Options,owner:usize,commands:mpsc::Receiver<Command>,events:mpsc::Sender<Event>,ready:mpsc::SyncSender<Result<usize,String>>) {
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
        let preview_class=wide(PREVIEW_CLASS);
        wc.lpszClassName=preview_class.as_ptr();wc.lpfnWndProc=Some(previews::destination_proc);
        // Failure here uses the single non-layered Stage path.
        RegisterClassExW(&wc);
        let rail_class=wide(STAGE_RAIL_CLASS);
        wc.lpszClassName=rail_class.as_ptr();wc.lpfnWndProc=Some(stage_rail_proc);RegisterClassExW(&wc);
        SHELL_MESSAGE.with(|id|id.set(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr())));
        let control=CreateWindowExW(WS_EX_TOOLWINDOW,class.as_ptr(),wide("Deskland Dock controller").as_ptr(),WS_POPUP,0,0,0,0,ptr::null_mut(),ptr::null_mut(),wc.hInstance,ptr::null());
        if control.is_null(){let _=ready.send(Err(format!("Create Dock controller: {}",GetLastError())));if com.is_ok(){CoUninitialize();}return;}
        CONTROL.with(|c|c.set(control));DIRTY.with(|c|c.set(false));
        let shell_hook=RegisterWindowMessageW(wide("SHELLHOOK").as_ptr());
        if shell_hook!=0&&RegisterShellHookWindow(control)!=0{
            ATTENTION_MSG.with(|id|id.set(shell_hook));
        }
        perf::start();
        OWNER.with(|o|o.set(owner as HWND));
        COMMANDS.with(|c|{c.replace(Some(commands));});EVENTS.with(|e|{e.replace(Some(events));});
        update_state(|state|{state.prefs=options;refresh(state,false);});
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
        while GetMessageW(&mut msg,ptr::null_mut(),0,0)>0{
            if !utilities::route_panel_keyboard(&msg){TranslateMessage(&msg);DispatchMessageW(&msg);}
        }
        for hook in hooks{UnhookWinEvent(hook);}KillTimer(control,1);KillTimer(control,2);
        update_state(|s|{s.appbars.clear();s.workspaces.recover_for_exit();for b in s.bars.drain(..){DestroyWindow(b.hwnd);}for mut b in s.stages.drain(..){destroy_stage(&mut b);}});
        SURFACES.with(|s|s.borrow_mut().clear());STAGE_SURFACES.with(|s|s.borrow_mut().clear());ATLAS.with(|a|a.borrow_mut().take());
        utilities::close_panels();UTILITIES.with(|u|u.borrow_mut().take());UTILITY_ANCHOR.with(|a|a.set(None));
        if ATTENTION_MSG.with(|id|id.replace(0))!=0{DeregisterShellHookWindow(control);}
        ATTENTION.with(|queue|queue.borrow_mut().clear());
        CONTROL.with(|c|c.set(ptr::null_mut()));DestroyWindow(control);
        OWNER.with(|o|o.set(ptr::null_mut()));
        COMMANDS.with(|c|{c.take();});EVENTS.with(|e|{e.take();});STATE.with(|s|{s.take();});
        perf::finish();
        if com.is_ok(){CoUninitialize();}
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    fn rect(a:[i32;4])->RECT{RECT{left:a[0],top:a[1],right:a[2],bottom:a[3]}}
    #[test] fn stage_titles_keep_different_names_after_shared_unicode_prefix(){
        let titles=["Bản thiết kế của nhóm - Đà Nẵng","Bản thiết kế của nhóm - Hải Phòng","Bản thiết kế của nhóm - New York"];
        assert_eq!(stage_caption(titles[0],titles[1..].iter().copied()),"Đà Nẵng");
        assert_eq!(stage_caption("Sơn",["Sơn Tùng"].into_iter()),"Sơn");
        assert_eq!(stage_caption("Untitled",["Untitled"].into_iter()),"Untitled");
    }
    #[test] fn pointer_release_cannot_activate_a_control_that_shifted_under_it(){
        let pin=DockTarget::App("a.exe".into());
        let mut down=Press::new(pin.clone(),100,12);
        assert_eq!(down.click(Some(DockTarget::App("b.exe".into())),100.0,12.0,1.5),None);
        assert_eq!(down.click(Some(DockTarget::Overflow),100.0,12.0,1.5),None);
        assert_eq!(down.click(Some(pin.clone()),110.0,12.0,1.5),Some(pin.clone()));
        assert_eq!(down.click(Some(pin.clone()),114.0,12.0,1.5),None);
        down.track(114.0,12.0,1.5);
        down.track(100.0,12.0,1.5);
        assert_eq!(down.click(Some(pin),100.0,12.0,1.5),None,"returning to the icon does not turn a drag into a click");
        let down=Press::new(DockTarget::Utility(UtilityWidget::Sound),20,20);
        assert_eq!(down.click(Some(DockTarget::Utility(UtilityWidget::Sound)),21.0,20.0,1.0),Some(down.target));
    }
    #[test] fn desktop_foreground_never_hides_independent_stage_or_dock(){
        let screen=rect([-1920,-120,0,1080]);let desktop=rect([-1920,-120,2560,1440]);
        for class in ["Progman","WorkerW",CLASS,"Shell_TrayWnd","Shell_SecondaryTrayWnd"]{assert!(!covers_display(class,desktop,screen),"{class}");}
        assert!(covers_display("RealApp",screen,screen));
        assert!(!covers_display("RealApp",rect([-1920,-120,0,1040]),screen));
    }
    #[test] fn maximized_resize_borders_are_not_fullscreen() {
        assert!(!fullscreen_style(WS_OVERLAPPEDWINDOW,true));
        assert!(!fullscreen_style(WS_THICKFRAME,true));
        assert!(fullscreen_style(WS_POPUP,false));
        assert!(fullscreen_style(WS_POPUP,true));
    }
    #[test] fn mixed_coordinate_overlap(){assert!(rect_intersects(rect([-1200,-200,0,900]),rect([-100,0,100,100])));assert!(!rect_intersects(rect([-1200,-200,0,900]),rect([0,0,100,100])));}
    #[test] fn distinct_monitor_geometry(){let s=Screen{bounds:rect([-1920,0,0,1080]),work:rect([-1920,0,0,1040]),primary:false,id:"test".into(),name:"Test".into(),dpi:96};let r=bar_rect(&s,5);assert!(r.left<0 && r.right<=0 && r.bottom<=1040);}
    #[test] fn stage_geometry_on_negative_monitor(){
        let sc=Screen{bounds:rect([-1920,-120,0,1080]),work:rect([-1920,-120,0,1040]),primary:false,id:"test".into(),name:"Test".into(),dpi:96};
        let left=stage_rect(&sc,StageStyle::default());
        let right=stage_rect(&sc,StageStyle{right:Some(true),scale:140,spacing:12,..Default::default()});
        assert!(left.left<right.left && left.left>=sc.work.left && right.right<=sc.work.right);
        assert!(left.top>=sc.work.top && left.bottom<=sc.work.bottom);
        assert!(right.right-right.left>left.right-left.left);
    }
    #[test] fn stage_keeps_distinct_windows_even_with_the_same_executable(){
        let make=|hwnd:usize,r:[i32;4]|AppWindow{
            hwnd:hwnd as HWND,title:format!("window {hwnd}"),path:"C:\\Apps\\editor.exe".into(),rect:rect(r)};
        let windows=vec![make(1,[-800,0,-400,800]),make(2,[-700,0,-300,800]),make(3,[50,0,500,800])];
        let stage=workspaces::Workspaces::default();
        let (local,groups)=stage.cards("portrait",rect([-1920,0,0,1080]),&windows,false);
        assert_eq!(local.iter().map(|item|item.hwnd as usize).collect::<Vec<_>>(),vec![1,2]);
        assert_eq!(groups,vec![None,None]);
        let (global,_)=stage.cards("portrait",rect([-1920,0,0,1080]),&windows,true);
        assert_eq!(global.len(),3);
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
