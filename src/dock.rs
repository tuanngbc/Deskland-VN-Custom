//! Native Windows multi-monitor dock (opt-in; does not impersonate Shell_TrayWnd).
//! Managed UI thread, started/configured/stopped by TLauncher's existing main loop.
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
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent,TRACKMOUSEEVENT,TME_LEAVE,SetFocus,SetCapture,GetCapture,ReleaseCapture,VK_LEFT,VK_RIGHT,VK_UP,VK_DOWN,VK_RETURN,VK_ESCAPE,VK_HOME,VK_END,VK_PRIOR,VK_NEXT,VK_APPS,VK_F10,VK_SHIFT,VK_SPACE,GetKeyState,GetDoubleClickTime};

mod options;
mod runtime;
mod glass_probe;
pub(crate) mod glass;
pub(crate) mod glass_menu;
mod calendar;
mod appearance;
mod geometry;
pub(crate) mod motion;
pub(crate) fn popup_motion_allowed()->bool{
    motion::allowed()
}
mod surface;
mod icon_atlas;
pub(crate) mod internal_app;
mod previews;
mod workspaces;
mod perf;
mod stage_surface;
mod stage_rail;
mod stage_projection;
mod stage_snapshot;
mod window_effect;
mod shell_menu;
mod window_lease;
mod motion_guard;
mod fluidwin_mesh;
mod stage_peek;
mod clipboard_history;
pub(crate) fn motion_diagnostics()->serde_json::Value{window_effect::diagnostics()}
mod stage_capture_lab;
pub(crate) fn stage_capture_diagnostic(path:&std::path::Path)->Result<(),String>{stage_capture_lab::inspect(path,true)}
mod utilities;
mod audio_mixer;
mod media;
mod jump_list;
mod folder_stack;
mod app_previews;
mod comfort;
mod recycle_bin;
mod wifi;
mod control_center;
pub(crate) mod panel_chrome;
mod appbar;
mod shell_access;
mod taskbar_guard;
mod taskbar_state;
pub use taskbar_state::{Mode as TaskbarMode,Status as TaskbarStatus};
mod win_key;
pub(crate) mod tray_poc;
pub(crate) mod command_pin;
pub(crate) fn create_command_pin(line:&str,label:&str)->Result<String,command_pin::PinError>{command_pin::encode(line,label)}
pub(crate) use taskbar_guard::{watchdog as taskbar_watchdog,report_trial_observation as taskbar_observation};
pub use utilities::{HostAction,Profile as UtilityProfile,Widget as UtilityWidget};
pub use control_center::MetricOptions;
pub use glass_probe::probe as glass_probe;
pub(crate) use glass_probe::Layer as PopupPixels;
pub(crate) use surface::Palette as PopupPalette;
pub use appearance::{Appearance,DockSide,StartIcon,Theme};
pub use options::{Options,Edit,DisplayMode,DisplayTarget,StageStyle,WindowMode,WindowPolicy};
pub use runtime::{Runtime,Event};
use runtime::Command;
type Preferences=Options;

const CLASS:&str="TuanpcDeskNativeDock";
const PREVIEW_CLASS:&str="TuanpcDeskStagePreview";

pub(crate) unsafe fn is_input_surface(hwnd:HWND)->bool{
    let mut name=[0u16;32];
    let n=GetClassNameW(hwnd,name.as_mut_ptr(),name.len() as i32).max(0) as usize;
    name[..n].iter().copied().eq(CLASS.encode_utf16())
}
const WM_TICK:u32=WM_APP+50;
const WM_CONFIG:u32=WM_APP+51;
const WM_ICON_CHANGED:u32=WM_APP+52;
const WM_UTILITY_READY:u32=WM_APP+53;
const WM_SHELL_FALLBACK:u32=WM_APP+55;
const WM_INTERNAL_APP:u32=WM_APP+56;
const MOTION_TIMER:usize=4;
const HIDE_TIMER:usize=5;
const WORKSPACE_TIMER:usize=6;
const STAGE_MOTION_TIMER:usize=7;
const STATUS_TIP_TIMER:usize=8;
const FEEDBACK_TIMER:usize=9;
const TASKBAR_GUARD_TIMER:usize=12;
const START_HOTKEY:i32=44;
const UTILITY_RETRY_TIMER:usize=13;
const STAGE_CAPTURE_TIMER:usize=14;
const APP_CLICK_TIMER:usize=15;
const RAIL_FADE_TIMER:usize=16;
const WM_DOCK_MOUSELEAVE:u32=0x02A3;
// winuser.h macro: HSHELL_REDRAW | HSHELL_HIGHBIT.
const HSHELL_FLASH:usize=6 | 0x8000;
const MOUSE_LEFT_DOWN:WPARAM=0x0001; // MK_LBUTTON in WM_MOUSEMOVE
const ITEM_WIDTH:i32=66;
const HEIGHT:i32=70;
const MAX_ITEMS:usize=10;
const MENU_PIN:usize=503;
const MENU_UNPIN:usize=504;
const MENU_TASK_MANAGER:usize=505;
const MENU_WIFI:usize=506;
const MENU_BLUETOOTH:usize=507;
const MENU_SOUND:usize=508;
const MENU_NOTIFICATIONS:usize=509;
const MENU_EXPLORER:usize=510;
const MENU_MINIMIZE:usize=514;
const MENU_MAXIMIZE:usize=515;
const MENU_RESTORE:usize=516;
const MENU_SNAP_LEFT:usize=517;
const MENU_SNAP_RIGHT:usize=518;
const MENU_EXPAND:usize=519;
const MENU_ADD_APP:usize=520;
const MENU_MORE_APPS:usize=521;
const MENU_APP_ICON:usize=523;
const MENU_APP_ICON_RESET:usize=524;
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
struct AppWindow { hwnd:HWND, title:String, path:String, rect:RECT, stage_aumid:Option<String> }
#[derive(Clone,Debug,PartialEq,Eq)]
struct Item { path:String, title:String, hwnd:HWND, count:usize, pinned:bool }
#[derive(Clone)]
struct Bar {
    hwnd:HWND, bounds:RECT, items:Vec<Item>, hovered:i32, hidden:bool,
    layout:Option<geometry::Layout>, pinned_widgets:Vec<UtilityWidget>, motion:Motion, collapsed:bool, auto_hide:bool,
    status_hover:Option<Option<UtilityWidget>>, pressed:Option<Press>, keyboard:Option<usize>, previous_focus:isize, pointer:Option<(f32,f32)>,
    reveal:motion::Spring, rail_face:usize, rail:RailFade, feedback:Option<Feedback>, active:Option<usize>, minimized:u16, attention:u16, metric_repaint:Option<Instant>,
}
#[derive(Clone,Debug,PartialEq,Eq)]
enum DockTarget { App(String), Launcher, Overflow, Utility(UtilityWidget), Shell(shell_access::Target) }
#[derive(Clone)]
struct Press { target:DockTarget, x:i32, y:i32, dragged:bool, double:bool }
impl Press {
    fn new(target:DockTarget,x:i32,y:i32)->Self {
        Self{target,x,y,dragged:false,double:false}
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
struct Feedback { path:String, kind:FeedbackKind, motion:motion::Span }
#[derive(Clone,Default)]
struct Motion {
    focus:Option<motion::Spring>, target:Option<f32>, zoom:motion::Spring, start:motion::Spring, at:Option<std::time::Instant>, scheduled:bool,
    resize:Option<geometry::Resize>,
}
impl Motion {
    fn focus_value(&self)->Option<f32>{self.focus.map(|f|f.value)}
    fn tick(&mut self, now:std::time::Instant, speed:u32, reveal:&mut motion::Spring, shown:bool,start_goal:f32)->bool{
        let dt=self.at.replace(now).map_or(0.016,|t|now.duration_since(t).as_secs_f32().clamp(0.001,0.090));
        let speed=speed as f32/100.0;
        // Critically damped springs reach the 0.3% rest band near
        // 8/frequency seconds. Every channel honors the entire speed range.
        let start_moving=self.start.advance(start_goal,dt,36.0*speed,0.005);
        let moving=reveal.advance(if shown{1.0}else{0.0},dt,if shown{29.0*speed}else{36.0*speed},0.003);
        // Freeze the rendered geometry during whole-Dock contraction/reveal;
        // reuse one motion bitmap rather than rebuilding glass each frame.
        if moving{return true;}
        let goal=if shown&&self.target.is_some(){1.0}else{0.0};
        if !shown{self.zoom=motion::Spring::rest(0.0);self.focus=None;return start_moving;}
        let mut unsettled=self.zoom.advance(goal,dt,36.0*speed,0.0025);
        if let Some(target)=self.target{
            let focus=self.focus.get_or_insert_with(||motion::Spring::rest(target));
            unsettled|=focus.advance(target,dt,38.0*speed,0.12);
        }
        if goal==0.0 && self.zoom.value==0.0 { self.focus=None; }
        moving||unsettled||start_moving
    }
}
#[cfg(test)]
#[test]
fn dock_motion_tracks_speed_and_reverses_from_displayed_state(){
    let start=std::time::Instant::now();
    for speed in [25,70,100,200]{
        let mut motion=Motion::default();
        let mut reveal=motion::Spring::rest(1.0);
        motion.at=Some(start);
        let mut previous=1.0;
        for step in 1..=12{
            motion.tick(start+Duration::from_millis(step*16),speed,&mut reveal,false,0.0);
            assert!(reveal.value<=previous+0.001);
            previous=reveal.value;
        }
        let reversed=reveal.value;
        let mut ticks=0;
        for step in 13..=250{
            ticks+=1;
            if !motion.tick(start+Duration::from_millis(step*16),speed,&mut reveal,true,0.0){break;}
        }
        assert_eq!(reveal.value,1.0);
        assert!(reversed<1.0&&ticks<238);
        motion.target=Some(220.0);
        let at=motion.at.unwrap();
        motion.tick(at+Duration::from_millis(16),speed,&mut reveal,true,0.0);
        assert!(motion.zoom.value>0.0&&motion.zoom.value<1.0);
    }
}
impl Bar {
    fn replace_layout(&mut self,layout:geometry::Layout){
        if let (Some(previous),Some((x,y)))=(&self.layout,self.pointer){
            self.pointer=Some((x+(previous.window.left-layout.window.left) as f32,y+(previous.window.top-layout.window.top) as f32));
        }
        self.hovered=self.pointer.and_then(|(x,y)|layout.hot(x,y)).map_or(-1,|i|i as i32);
        self.layout=Some(layout);
    }
    fn shell_target(&self,x:f32,y:f32)->Option<shell_access::Target>{
        let l=self.layout.as_ref()?;
        if self.hidden||self.collapsed{return None;}
        [l.start,l.tray,l.clock].into_iter().zip(shell_access::Target::ALL).find_map(|(r,t)|r.contains(x,y).then_some(t))
    }
    fn target(&self,x:f32,y:f32)->Option<DockTarget>{
        let layout=self.layout.as_ref()?;
        if self.hidden||self.collapsed{return None;}
        if let Some(item)=layout.hit(x,y).and_then(|i|self.items.get(i)){return Some(DockTarget::App(item.path.clone()));}
        if layout.is_launch_button(x,y){return Some(DockTarget::Launcher);}
        if layout.is_utility_button(x,y){return Some(DockTarget::Overflow);}
        layout.inline_at(x,y).map(DockTarget::Utility).or_else(||self.shell_target(x,y).map(DockTarget::Shell))
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
            ||layout.is_launch_button(x,y)||layout.is_utility_button(x,y)||layout.inline_at(x,y).is_some()||self.shell_target(x,y).is_some()
            ||layout.hit(x,y).is_some_and(|i|i>=pins){return None;}
        layout.pin_boundary(x,y,pins).filter(|&i|i!=from&&i!=from+1)
    }
    fn drop_target(&self)->Option<usize>{
        let (x,y)=self.pointer?;
        self.drop_at(x,y)
    }
    fn start_goal(&self)->f32{
        if self.collapsed{return 0.0;}
        if self.pressed.as_ref().is_some_and(|p|!p.dragged&&p.target==DockTarget::Shell(shell_access::Target::Start)){return -1.0;}
        if self.layout.as_ref().is_some_and(|l|self.hovered==l.shell_index() as i32){1.0}else{0.0}
    }
    fn schedule(&mut self)->bool{
        let start=!self.motion.scheduled;
        self.motion.scheduled=true;
        start
    }
    fn collapse(&mut self, collapsed:bool, reduced:bool)->bool {
        if self.collapsed==collapsed{return false;}
        self.collapsed=collapsed;
        self.rail=RailFade::default();
        if collapsed{self.rail_face=0;}
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
    start:Option<motion::Span>,
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
        self.start=Some(motion::Span::hover());
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
const GROUP_SWITCH_MS:u32=190;
const GROUP_SWITCH_INK:u8=190;
struct GroupSwitch{source:usize,started:motion::Span,ink:u8}
fn group_switch_ink(t:f32)->u8{
    (255.0-(255-GROUP_SWITCH_INK) as f32*(1.0-t.clamp(0.0,1.0)).powi(2)).round() as u8
}

const RAIL_DELAY_MS:u32=2000;
const RAIL_FADE_MS:u32=220;
const RAIL_DIM:f32=0.68;
#[derive(Clone)]
struct RailFade {
    opacity:f32,
    waiting:bool,
    started:Option<motion::Span>,
}
impl Default for RailFade {
    fn default()->Self{Self{opacity:1.0,waiting:false,started:None}}
}
fn rail_opacity(elapsed:std::time::Duration,reduced:bool)->f32{
    let t=(elapsed.as_secs_f32()*1000.0/RAIL_FADE_MS as f32).min(1.0);
    if reduced||t>=1.0{RAIL_DIM}else{1.0+(RAIL_DIM-1.0)*t*t*(3.0-2.0*t)}
}

struct Stage { hwnd:HWND, rail:HWND, collapsed:bool, auto_hide:bool, preview_hwnd:HWND, preview_boxes:Vec<RECT>, preview_cutouts:Vec<RECT>, preview_visible:bool, layered:bool, opacity:u32, dark:bool, bounds:RECT, display:String, items:Vec<Item>, keys:Vec<String>, session_heights:HashMap<String,u32>, representatives:HashMap<String,WindowIdentity>, groups:Vec<Option<u64>>, members:Vec<Vec<usize>>, order:Vec<String>, selected:Option<u64>, layout:Option<stage_surface::Layout>, page:usize, pages:usize, hidden:bool, hover:Option<usize>, emphasis:StageEmphasis, keyboard:Option<usize>, previous_focus:usize, previews:previews::Previews,
    resting:RECT, slide:Option<(motion::Span,i32)>, offset:i32, group_switch:Option<GroupSwitch>, pressed:Option<(StageTarget,bool)> }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum StageTarget { Picker(WindowIdentity), Cycle(WindowIdentity, bool), Window(usize,u32,u32), Close(usize,u32,u32), Page(usize) }
impl Stage {
    fn member_hit(&self,layout:stage_surface::Layout,index:usize,x:i32,y:i32)->Option<usize>{
        if let Some((target,mask))=stage_snapshot::target(self.hwnd as usize,index){
            if layout.icon_hit(index,x,y){return Some(0);}
            let strength=if self.emphasis.index==Some(index){self.emphasis.strength}else{0};
            return (0..layout.depth(index)).find(|&layer|if mask&(1<<layer)!=0{target.contains(layer,x,y,mask,strength)}else{layout.layer_hit(index,layer,x,y)});
        }
        layout.member_hit(index,x,y)
    }
    fn close_hit(&self,layout:stage_surface::Layout,index:usize,x:i32,y:i32)->bool{
        if stage_snapshot::target(self.hwnd as usize,index).is_some_and(|(_,m)|m&1!=0){
            return layout.projected_close(index).is_some_and(|b|b.hit(x,y));
        }
        layout.close_hit(index,x,y)
    }
    fn hit(&self,layout:stage_surface::Layout,x:i32,y:i32)->Option<usize>{
        (0..layout.count).find(|&i|self.member_hit(layout,i,x,y).is_some()
            ||self.members[i].len()>1&&layout.group_arrow_hit(i,x,y).is_some()
            ||self.hover==Some(i)&&self.close_hit(layout,i,x,y))
    }
    unsafe fn target(&self,x:i32,y:i32)->Option<StageTarget>{
        if self.hidden{return None;}
        let layout=self.layout?;
        if let Some(i)=self.hit(layout,x,y) {
            if self.members[i].len()>1 {
                if let Some(forward)=layout.group_arrow_hit(i,x,y){
                    return WindowIdentity::read_stage(self.items[i].hwnd).map(|id|StageTarget::Cycle(id,forward));
                }
                if layout.icon_hit(i,x,y){
                    return WindowIdentity::read_stage(self.items[i].hwnd).map(StageTarget::Picker);
                }
            }
            if self.hover==Some(i)&&self.close_hit(layout,i,x,y){
                let hwnd=self.items.get(i)?.hwnd;
                let (pid,tid)=stage_window_owner(hwnd)?;
                return Some(StageTarget::Close(hwnd as usize,pid,tid));
            }
            let layer=self.member_hit(layout,i,x,y)?;
            // Old flat compatibility cards draw decorative edges only.
            let hwnd=*self.members.get(i)?.get(if layout.stack_design{layer}else{0})? as HWND;
            let (pid,tid)=stage_window_owner(hwnd)?;
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
        self.slide=Some((motion::Span::new(195),self.offset));
        if SetTimer(self.hwnd,STAGE_MOTION_TIMER,16,None)==0{self.offset=0;self.slide=None;}
    }
    unsafe fn finish_group_switch(&mut self){
        if let Some(swap)=self.group_switch.take(){self.previews.set_opacity(swap.source,255);}
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
struct State { bars:Vec<Bar>, stages:Vec<Stage>, windows:Vec<AppWindow>, prefs:Preferences, screens:Vec<Screen>, suspended:bool, foreground:isize, workspaces:workspaces::Workspaces, material:Option<surface::Material>, system_dark:bool, appbars:appbar::Bars, reservation_failed:bool }
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
    static UTILITY_RETRY:RefCell<Option<UtilityRequest>>=const{RefCell::new(None)};
    static UTILITY_HUB:Cell<Option<(usize,usize,bool)>>=const{Cell::new(None)};
    static UTILITY_ANCHOR:Cell<Option<(usize,POINT,UtilityWidget,bool,usize)>>=const{Cell::new(None)};
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
    if path==internal_app::KEY{return internal_app::LABEL.into();}
    if let Some(command)=command_pin::decode(path){return command.label;}
    if let Some(name)=crate::launcher::cached_application_name(std::path::Path::new(path)){return name;}
    path.rsplit(['\\','/']).next().unwrap_or(path).trim_end_matches(".exe").to_string()
}
fn is_shell_class(class:&str)->bool {
    matches!(class,"Progman"|"WorkerW"|"Shell_TrayWnd"|"Shell_SecondaryTrayWnd"|"NotifyIconOverflowWindow"|"ApplicationFrameInputSinkWindow"|"TuanpcDeskNativeDock"|"TuanpcDeskStagePreview"|"TuanpcDeskStageReveal"|"TuanpcDeskMsg"|"TuanpcDeskWall")
}
fn covers_display(class:&str,window:RECT,screen:RECT)->bool {
    !is_shell_class(class) && window.left<=screen.left && window.top<=screen.top
        && window.right>=screen.right && window.bottom>=screen.bottom
}
unsafe fn display_has_maximized(screen:RECT,windows:&[AppWindow])->bool{
    windows.iter().any(|w|rect_intersects(w.rect,screen)&&IsZoomed(w.hwnd)!=0
        &&IsIconic(w.hwnd)==0&&stage_window_is_local(w.hwnd,screen))
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
        windows.iter().find(|w|w.path!=internal_app::KEY&&rect_intersects(w.rect,screen)&&IsIconic(w.hwnd)==0&&stage_window_is_local(w.hwnd,screen))
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
unsafe fn stage_identity(hwnd:HWND,path:&str)->Option<String>{
    use windows::{core::GUID,Win32::{Foundation::PROPERTYKEY,System::Com::{
        CoTaskMemFree,StructuredStorage::{PropVariantClear,PropVariantToStringAlloc}
    },UI::Shell::PropertiesSystem::{IPropertyStore,SHGetPropertyStoreForWindow}}};
    // Packaged apps already have a verified process/child AUMID. For Win32
    // windows use the public per-window identity (such as a Chromium PWA).
    if path.starts_with("shell:AppsFolder"){return None;}
    const AUMID:PROPERTYKEY=PROPERTYKEY{fmtid:GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),pid:5};
    let store:IPropertyStore=SHGetPropertyStoreForWindow(windows::Win32::Foundation::HWND(hwnd)).ok()?;
    let mut value=store.GetValue(&AUMID).ok()?;
    let id=PropVariantToStringAlloc(&value).ok().and_then(|raw|{
        let result=raw.to_string().ok().filter(|id|!id.is_empty()&&id.len()<=400);
        CoTaskMemFree(Some(raw.0.cast()));result
    });
    let _=PropVariantClear(&mut value);
    id
}
unsafe extern "system" fn enumerate(hwnd:HWND,param:LPARAM)->i32 {
    let (out,stage)=&mut *(param as *mut (Vec<AppWindow>,bool));
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
    let stage_aumid=if *stage{stage_identity(hwnd,&path)}else{None};
    out.push(AppWindow{hwnd,title,path,rect,stage_aumid});
    1
}
fn windows(stage:bool)->Vec<AppWindow>{
    let mut query=(Vec::new(),stage);unsafe {EnumWindows(Some(enumerate),&mut query as *mut _ as LPARAM)};
    let (mut v,_)=query;
    if let Some(settings)=internal_app::window(){v.insert(0,settings);}
    v
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
    let scale=style.pixel_scale(screen.dpi);
    let zoom=if style.motion_reduced(){0.0}else{style.magnification as f32/100.0};
    let cross=(screen.bounds.right-screen.bounds.left).min(screen.bounds.bottom-screen.bounds.top) as f32;
    (style.edge_pixels(screen.dpi,cross)+((style.icon_size as f32*(1.0+zoom)+18.0).max(cluster.cross())*scale+12.0*scale).ceil()).ceil() as i32
}
fn stage_width(screen:&Screen,style:StageStyle)->i32{
    let scale=(screen.dpi as f32/96.0).clamp(0.75,3.0);
    let base=if stage_legacy_layout(){160.0}else{272.0};
    (base*style.scale as f32/100.0*scale).round() as i32
}
fn dock_status_widgets(profile:Option<&UtilityProfile>)->Vec<UtilityWidget>{
    use UtilityWidget::{Network,Sound,Metrics};
    let mut cells=Vec::with_capacity(3);
    if profile.is_none_or(|p|p.network_on_dock){cells.push(Network);}
    if profile.is_none_or(|p|p.sound_on_dock){cells.push(Sound);}
    if profile.is_some_and(|p|p.dock_metrics()){cells.push(Metrics);}
    cells
}
fn stage_legacy_layout()->bool{crate::trial::benchmark_active()||crate::trial::demo_active()}
fn stage_width_cap(width:i32)->i32{
    if stage_legacy_layout(){(width/5).max(62)}else{(width*28/100).max(1)}
}
fn stage_margin(screen:&Screen,style:StageStyle)->i32{
    let scale=(screen.dpi as f32/96.0).clamp(0.75,3.0);
    ((if stage_legacy_layout(){8.0}else{style.edge_gap as f32})*scale).round() as i32
}
fn stage_app_gap(screen:&Screen,style:StageStyle)->i32{
    if stage_legacy_layout(){0}else{panel_chrome::scale(screen.dpi,style.app_gap as i32)}
}
fn stage_depth(screen:&Screen,style:StageStyle)->i32{
    let trim=if stage_legacy_layout(){0}else{panel_chrome::scale(screen.dpi,16)};
    (stage_width(screen,style).min(stage_width_cap(screen.bounds.right-screen.bounds.left))-trim).max(1)+stage_margin(screen,style)+stage_app_gap(screen,style)
}
unsafe fn sync_appbars(state:&mut State,topology:bool){
    // Don't ask Explorer to renegotiate AppBars while the watchdog is
    // waiting for it to hide the bars. Keep the existing reservations.
    if taskbar_pending(){return;}
    let notify=APPBAR_DIRTY.with(|flag|flag.replace(false));
    let reset=SHELL_RESTARTED.with(|flag|flag.replace(false));
    let mut requests=Vec::new();
    let monitors=state.screens.iter().map(|screen|screen.bounds).collect::<Vec<_>>();
    for screen in &state.screens {
        let policy=state.prefs.window_policy(&screen.id);
        if policy.dock==WindowMode::Reserve&&state.bars.iter().any(|b|same_rect(b.bounds,screen.bounds)){
            requests.push(appbar::Request{display:screen.id.clone(),stage:false,monitor:screen.bounds,
                side:state.prefs.appearance.position,depth:dock_depth(screen,&state.prefs.appearance,state.prefs.utility_cluster(&screen.id))});
        }
        if policy.stage==WindowMode::Reserve&&state.stages.iter().any(|p|p.display==screen.id){
            // Reserve based on the same monitor assignment as the Stage,
            // without building groups again just to check for a window.
            let has_cards=state.workspaces.has_cards(&screen.id,screen.bounds,&monitors,&state.windows,state.prefs.all_windows);
            if has_cards || (state.suspended&&state.appbars.contains(&screen.id,true)) {
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
    let margin=stage_margin(screen,style);
    let width=depth.map_or_else(||stage_width(screen,style).min(stage_width_cap(screen.work.right-screen.work.left)),|d|(d-margin-stage_app_gap(screen,style)).max(1));
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
    stage_snapshot::hide(stage.hwnd as usize);
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
        let pinned_widgets=dock_status_widgets(state.prefs.utility_profiles.get(&screen.id));
        let layout=geometry::measure_cluster(screen,&state.prefs.appearance,items.len(),None,0.0,&pinned_widgets,state.prefs.utility_cluster(&screen.id));
        let r=layout.window;
        let hwnd=CreateWindowExW(WS_EX_TOPMOST|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_LAYERED,
            class.as_ptr(),wide("TLauncher Dock").as_ptr(),WS_POPUP,
            r.left,r.top,r.right-r.left,r.bottom-r.top,
            ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
        if !hwnd.is_null(){
        recycle_bin::register(hwnd);
        state.bars.push(Bar{hwnd,bounds:screen.bounds,items,hovered:-1,hidden:true,layout:Some(layout),pinned_widgets,motion:Motion::default(),collapsed:false,auto_hide:false,status_hover:None,pressed:None,keyboard:None,previous_focus:0,pointer:None,
            reveal:motion::Spring::rest(1.0),rail_face:0,rail:RailFade::default(),feedback:None,active:None,minimized:0,attention:0,metric_repaint:None});
        }else{emit(Event::Error(format!("Create Dock window: {}",GetLastError())));}
        }
        if stage_targets.contains(&i) && !state.stages.iter().any(|s|same_rect(s.bounds,screen.bounds)) {
            let style=state.prefs.stage_style(&screen.id);
            let r=stage_rect(screen,style);
            let stage_hwnd=CreateWindowExW(WS_EX_TOPMOST|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_LAYERED,
                class.as_ptr(),wide("TLauncher Stage Manager").as_ptr(),WS_POPUP,
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
                state.stages.push(Stage{hwnd:stage_hwnd,rail:ptr::null_mut(),collapsed:false,auto_hide:false,preview_hwnd,preview_boxes:Vec::new(),preview_cutouts:Vec::new(),preview_visible:false,layered,opacity:style.opacity,dark:true,bounds:screen.bounds,display:screen.id.clone(),items:Vec::new(),keys:Vec::new(),session_heights:HashMap::new(),representatives:HashMap::new(),groups:Vec::new(),members:Vec::new(),order:Vec::new(),selected:None,layout:None,page:0,pages:1,hidden:true,hover:None,emphasis:StageEmphasis::default(),keyboard:None,previous_focus:0,previews:previews::Previews::default(),
                    resting:r,slide:None,offset:0,group_switch:None,pressed:None});
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
    let policy_changed=state.material.is_some_and(|m|m.reduced_motion!=state.prefs.appearance.motion_reduced());
    let material_changed=if topology||state.material.is_none()||policy_changed{
        let style=&state.prefs.appearance;
        state.system_dark=surface::Material::system_dark();
        let material=surface::Material::themed(style.theme,state.system_dark,style.transparency,style.motion_reduced());
        state.material.replace(material)!=Some(material)
    }else{false};
    state.windows=if state.suspended||!state.prefs.any_enabled(){Vec::new()}else{windows(state.prefs.stage_enabled&&!crate::trial::benchmark_active()&&!crate::trial::demo_active())};
    recreate(state);
    sync_appbars(state,topology);
    let fullscreen_outputs:HashSet<String>=state.screens.iter().filter(|s|!state.suspended&&display_fullscreen(s.bounds,&state.windows)).map(|s|s.id.clone()).collect();
    let foreground=GetForegroundWindow() as isize;let active_changed=foreground!=state.foreground;state.foreground=foreground;
    ATTENTION.with(|queue|queue.borrow_mut().retain(|&hwnd,id|{
        hwnd!=foreground as usize && state.prefs.enabled && !state.suspended && window_owner(hwnd as HWND)==Some(*id)
    }));
    for b in &mut state.bars {
        let items=item_list(&state.windows,b.bounds,&state.prefs);
        let items_changed=b.items!=items;
        let resize_mapping=(b.items.len()!=items.len()).then(||items.iter().map(|item|b.items.iter().position(|old|old.path==item.path)).collect::<Vec<_>>());
        let mut changed=items_changed || active_changed || material_changed;
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
            let placed=state.appbars.dock_placement(screen,state.prefs.appearance.position);
            // Auto-hide is armed only while this monitor has an actual
            // maximized window. Regular/Snap windows leave its Dock visible.
            let auto=state.prefs.dock_auto_hide(&screen.id)&&display_has_maximized(screen.bounds,&state.windows);
            if auto!=b.auto_hide{
                KillTimer(b.hwnd,HIDE_TIMER);
                KillTimer(b.hwnd,RAIL_FADE_TIMER);
                b.auto_hide=auto;
                let interaction=b.pointer.is_some()||b.keyboard.is_some()||b.pressed.is_some()||utilities::panel_open_for(b.hwnd);
                let collapse=auto&&b.layout.is_some()&&!interaction;
                if b.collapse(collapse,b.hidden||state.prefs.appearance.motion_reduced())
                    &&SetTimer(b.hwnd,MOTION_TIMER,16,None)==0 {
                    b.motion.scheduled=false;
                    b.reveal=motion::Spring::rest(if collapse{0.0}else{1.0});
                }
                changed=true;
            }
            if state.prefs.appearance.motion_reduced()&&b.motion.scheduled{
                b.motion=Motion::default();b.reveal=motion::Spring::rest(if b.collapsed{0.0}else{1.0});
                KillTimer(b.hwnd,MOTION_TIMER);changed=true;
            }
            let r=if b.layout.is_some(){
                b.pinned_widgets=dock_status_widgets(state.prefs.utility_profiles.get(&screen.id));
                let can_resize=!b.hidden&&!b.collapsed&&b.reveal.value>0.997&&b.pressed.is_none()
                    &&!state.prefs.appearance.motion_reduced();
                let wants_resize=!topology&&resize_mapping.is_some();
                let resizing=can_resize&&(wants_resize||b.motion.resize.is_some());
                let layout=geometry::measure_cluster(&placed,&state.prefs.appearance,b.items.len(),
                    if resizing{None}else{b.motion.focus_value()},if resizing{0.0}else{b.motion.zoom.value},
                    &b.pinned_widgets,state.prefs.utility_cluster(&screen.id));
                if let Some(mapping)=resize_mapping.as_ref().filter(|_|can_resize&&wants_resize){
                    b.motion.resize=b.layout.as_ref().and_then(|old|geometry::Resize::new(old,layout.clone(),mapping));
                    if b.motion.resize.is_some(){
                        b.motion.focus=None;b.motion.zoom=motion::Spring::rest(0.0);b.motion.target=None;
                        if SetTimer(b.hwnd,MOTION_TIMER,16,None)!=0{b.motion.scheduled=true;}
                        else{b.motion.resize=None;}
                    }
                }else if !can_resize||b.motion.resize.as_ref().is_some_and(|old|!old.matches(&layout)||items_changed){
                    b.motion.resize=None;
                }
                let layout=if let Some(resize)=&b.motion.resize{resize.frame(resize.progress())}else{layout};
                changed|=b.layout.as_ref().is_some_and(|old|old.glass!=layout.glass||old.slots!=layout.slots||old.scan!=layout.scan||old.inline!=layout.inline);
                b.replace_layout(layout);
                b.layout.as_ref().unwrap().window
            }else{bar_rect(&placed,b.items.len())};
            let mut actual:RECT=mem::zeroed();
            if GetWindowRect(b.hwnd,&mut actual)==0 || !same_rect(r,actual){
                KillTimer(b.hwnd,STATUS_TIP_TIMER);close_status_tip(b.hwnd);b.status_hover=None;
                SetWindowPos(b.hwnd,HWND_TOPMOST,r.left,r.top,r.right-r.left,r.bottom-r.top,SWP_NOACTIVATE);
            }
        }
        let fullscreen=state.suspended || screen.is_some_and(|s|fullscreen_outputs.contains(&s.id));
        if b.collapsed&&!fullscreen&&b.reveal.value<=0.24&&(topology||changed||b.hidden||b.rail_face==0){
            if let Some(layout)=&b.layout{
                let face=stage_rail::sample(surface::rail_bounds(layout),layout.side,false,b.rail_face);
                changed|=face!=b.rail_face;
                b.rail_face=face;
            }
        }
        if b.hidden!=fullscreen {
            ShowWindow(b.hwnd,if fullscreen{SW_HIDE}else{SW_SHOWNOACTIVATE});b.hidden=fullscreen;
            if fullscreen{utilities::close_panels_for(b.hwnd);close_status_tip(b.hwnd);KillTimer(b.hwnd,STATUS_TIP_TIMER);b.status_hover=None;KillTimer(b.hwnd,MOTION_TIMER);KillTimer(b.hwnd,HIDE_TIMER);KillTimer(b.hwnd,RAIL_FADE_TIMER);KillTimer(b.hwnd,FEEDBACK_TIMER);b.rail=RailFade::default();b.motion=Motion::default();b.hovered=-1;b.keyboard=None;b.pressed=None;b.pointer=None;b.collapsed=b.auto_hide;b.reveal=motion::Spring::rest(if b.collapsed{0.0}else{1.0});b.feedback=None;}
            changed=true;
        }
        if b.collapsed&&!b.hidden&&!b.rail.waiting&&b.rail.started.is_none()&&b.rail.opacity==1.0{
            b.rail.waiting=SetTimer(b.hwnd,RAIL_FADE_TIMER,RAIL_DELAY_MS,None)!=0;
        }
        if changed{InvalidateRect(b.hwnd,ptr::null(),0);}
    }
    if state.bars.iter().all(|b|b.hidden)||state.suspended{utilities::close_panels();UTILITY_ANCHOR.with(|a|a.set(None));UTILITIES.with(|u|u.borrow_mut().take());}
    state.workspaces.prune();
    let benchmark_sources=crate::trial::benchmark_stage_sources();
    let mut stage_report=Vec::new();
    let monitor_bounds=state.screens.iter().map(|d|d.bounds).collect::<Vec<_>>();
    let perspective=state.prefs.stage_enabled&&state.prefs.stage_perspective&&!stage_legacy_layout();
    let mut targets=Vec::new();
    let mut stage_updates=Vec::new();
    for (stage_index,stage) in state.stages.iter_mut().enumerate() {
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
        let automatic=!stage_legacy_layout();
        let mut cards=state.workspaces.cards(&stage.display,stage.bounds,&monitor_bounds,&state.windows,state.prefs.all_windows,automatic,foreground as HWND);
        if automatic{cards.sort_by_cached_key(|card|(card.item.path!=internal_app::KEY,
            stage.order.iter().position(|key|*key==card.key).unwrap_or(usize::MAX)));}
        stage.order=cards.iter().map(|card|card.key.clone()).collect::<Vec<_>>();
        stage.representatives.retain(|key,id|id.matches_stage()&&cards.iter().any(|c|
            &c.key==key&&c.members.len()>1&&c.members.contains(&id.hwnd)));
        for card in &mut cards{
            card.item.count=card.members.len();
            if let Some(id)=stage.representatives.get(&card.key){
                if let Some(window)=state.windows.iter().find(|w|w.hwnd as usize==id.hwnd){
                    if let Some(index)=card.members.iter().position(|&h|h==id.hwnd){
                        card.members[..=index].rotate_right(1);
                        card.item.hwnd=window.hwnd;
                        card.item.path.clone_from(&window.path);
                        if card.manual.is_none(){card.item.title.clone_from(&window.title);}
                    }
                }
            }
        }
        let items=cards.iter().map(|card|card.item.clone()).collect::<Vec<_>>();
        let groups=cards.iter().map(|card|card.manual).collect::<Vec<_>>();
        let members=cards.into_iter().map(|card|card.members).collect::<Vec<_>>();
        let mut layout=if automatic{
            stage_surface::Layout::landscape(bounds.right-bounds.left,bounds.bottom-bounds.top,screen.dpi,stage_gap(screen,style),
                style.group_limit as usize,items.len(),style.right==Some(true),style.stack_groups&&stage.layered)
        }else{
            stage_surface::Layout::new(bounds.right-bounds.left,bounds.bottom-bounds.top,screen.dpi,stage_gap(screen,style),
                style.group_limit as usize,items.len(),style.right==Some(true))
        };
        layout.align_to_work_area=depth.is_some()&&automatic;
        let rows=layout.rows;
        let keys=stage.order.clone();
        stage.session_heights.retain(|key,_|keys.contains(key));
        let default=if style.card_height!=0{style.card_height}else{state.prefs.stage_card_height};
        let heights=keys.iter().map(|key|stage.session_heights.get(key).or_else(||state.prefs.stage_card_overrides.get(key))
            .copied().unwrap_or(default)).collect::<Vec<_>>();
        let variable=automatic&&heights.iter().any(|&h|h!=0);
        let pages=layout.page_ranges(items.len());
        let selected=state.workspaces.active(&stage.display);
        let old_pages=stage.pages;
        stage.pages=pages.len();
        stage.page=if selected!=stage.selected {
            selected.and_then(|id|groups.iter().position(|group|*group==Some(id)))
                .and_then(|row|pages.iter().position(|range|range.contains(&row))).unwrap_or(stage.page.min(stage.pages-1))
        }else{stage.page.min(stage.pages-1)};
        let range=pages[stage.page].clone();
        let model_count=items.len();
        let fixture_positions=benchmark_sources.map(|sources| sources.iter().map(|&handle|
            items.iter().position(|item| item.hwnd as usize==handle)).collect::<Vec<_>>());
        if variable{layout.apply_heights(screen.dpi,&heights[range.clone()]);}
        let count=range.len();
        let start=range.start;
        let items:Vec<_>=items.into_iter().skip(start).take(count).collect();
        let groups:Vec<_>=groups.into_iter().skip(start).take(count).collect();
        let members:Vec<_>=members.into_iter().skip(start).take(count).collect();
        let visible_keys:Vec<_>=keys.into_iter().skip(start).take(count).collect();
        let keep_hover=stage.group_switch.as_ref().is_some_and(|swap|stage.hover.is_some_and(|i|
            stage.keys.get(i)==visible_keys.get(i)&&items.get(i).is_some_and(|item|item.hwnd as usize==swap.source)));
        stage.keys=visible_keys;
        let mut changed=stage.members!=members||stage.items!=items||stage.groups!=groups||stage.selected!=selected||active_changed||material_changed||old_pages!=stage.pages;
        let dark=style.theme.dark(state.system_dark);
        changed|=stage.opacity!=style.opacity||stage.dark!=dark;
        stage.opacity=style.opacity;stage.dark=dark;
        layout.count=items.len();
        layout.active=members.iter().position(|windows|windows.contains(&(foreground as usize)));
        layout.stack_design=automatic&&style.stack_groups&&stage.layered;
        layout.height=layout.content_height();
        bounds.bottom=bounds.top+layout.height;
        if topology || state.prefs.appearance.motion_reduced() {
            stage.slide=None;stage.offset=0;
            stage.finish_group_switch();
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
            for (index,group) in members.iter().enumerate(){layout.layers[index]=group.len().min(5) as u8;}
        }
        if stage.items!=items || stage.groups!=groups || stage.members!=members {
            KillTimer(stage.hwnd,STATUS_TIP_TIMER);close_status_tip(stage.hwnd);
            if !keep_hover{stage.hover=None;stage.emphasis=StageEmphasis::default();}
            if stage.slide.is_none()&&stage.group_switch.is_none(){KillTimer(stage.hwnd,STAGE_MOTION_TIMER);}
            stage.keyboard=stage.keyboard.and_then(|index|{
                let previous=stage.items.get(index);
                previous.and_then(|w|items.iter().position(|i|i.hwnd==w.hwnd))
                    .or_else(||items.len().checked_sub(1).map(|last|index.min(last)))
            });
        }
        stage.items=items;stage.groups=groups;stage.members=members;stage.selected=selected;
        if stage.group_switch.as_ref().is_some_and(|swap|!stage.items.iter().any(|i|i.hwnd as usize==swap.source)){
            stage.finish_group_switch();
        }
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
                let right=style.right==Some(true);
                let x=if right{screen.work.right-w}else{screen.work.left};
                let y=bounds.top;
                let mut old:RECT=mem::zeroed();GetWindowRect(stage.rail,&mut old);
                let rail_bounds=RECT{left:x,top:y,right:x+w,bottom:y+h};
                let moving=IsWindowVisible(stage.rail)==0||!same_rect(old,rail_bounds);
                if moving||active_changed||topology{
                    let previous=GetWindowLongPtrW(stage.rail,GWLP_USERDATA);
                    let side=if right{DockSide::Right}else{DockSide::Left};
                    let face=stage_rail::sample(rail_bounds,side,true,previous as usize) as isize;
                    if face!=previous{
                        SetWindowLongPtrW(stage.rail,GWLP_USERDATA,face);
                        InvalidateRect(stage.rail,ptr::null(),0);
                    }
                }
                if moving{
                    SetWindowPos(stage.rail,HWND_TOPMOST,x,y,w,h,SWP_NOACTIVATE|SWP_SHOWWINDOW);
                }
            }
        }
        let hide=hide&&(unavailable||stage.collapsed);
        if hide!=stage.hidden{ShowWindow(stage.hwnd,if hide{SW_HIDE}else{SW_SHOWNOACTIVATE});stage.hidden=hide;changed=true;}
        let peek_source=if hide||!state.prefs.stage_peek{None}else{
            stage.hover.and_then(|i|stage.items.get(i)).and_then(|item|WindowIdentity::read_stage(item.hwnd))
        };
        stage_peek::validate(stage.hwnd,peek_source);
        if hide {
            KillTimer(stage.hwnd,stage_peek::TIMER);
            KillTimer(stage.hwnd,STAGE_MOTION_TIMER);
            stage.finish_group_switch();
            stage.slide=None;stage.offset=0;
            stage.hover=None;stage.emphasis=StageEmphasis::default();stage.keyboard=None;stage.pressed=None;KillTimer(stage.hwnd,STATUS_TIP_TIMER);close_status_tip(stage.hwnd);changed|=stage.previews.clear();
        }else if perspective&&stage.layered{
            for (index,members) in stage.members.iter().enumerate(){
                let mut sources=[None;5];
                for (layer,&handle) in members.iter().take(layout.depth(index).min(5)).enumerate(){
                    sources[layer]=WindowIdentity::read_stage(handle as HWND);
                }
                targets.push(stage_snapshot::Target{owner:stage.hwnd as usize,layout,index,
                    reduced:state.prefs.appearance.motion_reduced(),dark,dpi:screen.dpi,sources});
            }
        }
        stage_updates.push((stage_index,layout,style,automatic,hide,changed,fixture_positions,model_count,rows));
    }
    let projection_changed=stage_snapshot::sync(&targets,CONTROL.get(),perspective,foreground as usize);
    for (stage_index,layout,style,automatic,hide,mut changed,fixture_positions,model_count,rows) in stage_updates{
        let stage=&mut state.stages[stage_index];
        let Some(screen)=state.screens.iter().find(|s|s.id==stage.display) else{continue;};
        let mut steps=[previews::PreviewStep::NotRun;8];
        if !hide&&layout.width>0&&layout.height>0{
            let projected=stage_snapshot::masks(stage.hwnd as usize);
            changed|=projection_changed.contains(&(stage.hwnd as usize));
            let host=if stage.preview_hwnd.is_null(){stage.hwnd}else{stage.preview_hwnd};
            changed|=if stage.preview_hwnd.is_null()||!automatic{
                stage.previews.update_traced(host,&stage.items,layout,benchmark_sources.map(|_| steps.as_mut_slice()))
            }else{
                stage.previews.update_stacks(host,&stage.members,layout,projected)
            };
        }
        if let Some(swap)=stage.group_switch.as_ref(){stage.previews.set_opacity(swap.source,swap.ink);}
        let mut clip_ok=true;
        if !stage.preview_hwnd.is_null(){
            let areas=stage.previews.areas();
            let mut cutouts=layout.icon_cutouts();
            cutouts.extend(stage.members.iter().enumerate().filter(|(_,members)|members.len()>1)
                .filter_map(|(i,_)|layout.group_arrows(i)).flatten().map(|r|r.win()));
            let differs=|a:&[RECT],b:&[RECT]|a.len()!=b.len()||a.iter().zip(b).any(|(&a,&b)|!same_rect(a,b));
            let reshape=differs(&areas,&stage.preview_boxes)||differs(&cutouts,&stage.preview_cutouts);
            let clipped=!reshape||previews::clip_destination_except(stage.preview_hwnd,&areas,&cutouts);
            clip_ok=clipped;
            let show=clipped&&!areas.is_empty()&&!hide;
            if clipped&&reshape{stage.preview_boxes=areas;stage.preview_cutouts=cutouts;}
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
                let slot=stage.members.iter().position(|group|group.contains(&handle));
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
    // A collapsed bar is still configured to show the inline tile. Keep its
    // feed warm, while INLINE_UPDATE skips repainting collapsed bars.
    let live_profiles=state.bars.iter().filter(|b|!b.hidden&&b.layout.as_ref().is_some_and(|l|l.metrics.is_some()))
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
        let magnify=if style.motion_reduced(){1.0}else{1.0+style.magnification as f32/100.0};
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
    let mut cursor:POINT=mem::zeroed();
    let on_client=GetCursorPos(&mut cursor)!=0&&ScreenToClient(owner,&mut cursor)!=0;
    let label=STATE.with(|s|{
        let state=s.borrow();
        if let Some(stage)=state.stages.iter().find(|p|p.hwnd==owner&&!p.hidden&&!p.collapsed){
            let index=stage.hover?;
            let item=stage.items.get(index)?;
            if let Some(layout)=stage.layout.filter(|layout|on_client&&layout.stack_design){
                if stage.close_hit(layout,index,cursor.x,cursor.y){return Some("Đóng cửa sổ phía trước".into());}
                if !layout.icon_hit(index,cursor.x,cursor.y){
                    if let Some(member)=stage.member_hit(layout,index,cursor.x,cursor.y).and_then(|i|stage.members.get(index)?.get(i)){
                        if let Some(w)=state.windows.iter().find(|w|w.hwnd as usize==*member){
                            let title:String=w.title.chars().take(512).collect();
                            let layer=stage.members[index].iter().position(|id|id==member).unwrap_or(0);
                            let cached=stage_snapshot::target(owner as usize,index).is_some_and(|(_,mask)|mask&(1<<layer)!=0);
                            let preview=if cached{"Ảnh phối cảnh đã lưu tạm"}
                                else if stage.preview_visible&&stage.previews.includes(w.hwnd){"Xem trước DWM"}
                                else if matches!(previews::permitted_pid(w.hwnd),Err(previews::PreviewStep::Protected|previews::PreviewStep::AffinityQuery(_))){
                                    "Windows không cho phép xem trước"
                                }else{"Biểu tượng ứng dụng"};
                            return Some(format!("{title} · {preview}"));
                        }
                    }
                }else if item.count>1{
                    return Some(format!("{} · {} cửa sổ",item.title,item.count));
                }
            }
            return Some(if stage.groups.get(index).is_some_and(Option::is_some){
                format!("{} · {} cửa sổ",item.title,item.count)
            }else{item.title.chars().take(512).collect()});
        }
        let bar=state.bars.iter().find(|b|b.hwnd==owner&&!b.hidden&&!b.collapsed)?;
        if let Some(target)=bar.pointer.and_then(|(x,y)|bar.shell_target(x,y)){return Some(target.title(local_shell_buttons()).to_owned());}
        let item=bar.status_hover?;
        Some(if let Some((widget,detail))=item.and_then(|w|utilities::status_detail(w).map(|d|(w,d))){
            format!("{} · {detail}",widget.title())
        }else{item.map_or("Bảng tiện ích",UtilityWidget::title).to_owned()})
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
unsafe fn retarget_start(hwnd:HWND){
    let state=STATE.with(|s|{
        let mut state=s.borrow_mut();
        let reduced=state.prefs.appearance.motion_reduced();
        let b=state.bars.iter_mut().find(|b|b.hwnd==hwnd)?;
        let goal=b.start_goal();
        if b.motion.start.value==goal{return None;}
        if reduced{b.motion.start=motion::Spring::rest(goal);Some(false)}else{Some(b.schedule())}
    });
    if let Some(start)=state{
        if start{SetTimer(hwnd,MOTION_TIMER,16,None);}
        InvalidateRect(hwnd,ptr::null(),0);
    }
}
unsafe fn paint(hwnd:HWND){
    let layered=STATE.with(|s|{
        let state=s.borrow();
        let bar=state.bars.iter().find(|b|b.hwnd==hwnd)?;
        let feedback=bar.feedback.as_ref().and_then(|f|bar.items.iter().position(|i|i.path.eq_ignore_ascii_case(&f.path))
            .map(|i|(i,f.kind,if state.prefs.appearance.motion_reduced(){0.2}else{f.motion.progress()})));
        Some((bar.layout.clone()?,bar.items.clone(),bar.hovered,bar.keyboard,state.material?,bar.active,bar.minimized,bar.attention,bar.drop_target(),bar.reveal.value.clamp(0.0,1.0),feedback,bar.motion.at,state.prefs.appearance.start_icon,
            state.screens.iter().find(|d|same_rect(d.bounds,bar.bounds))
                .and_then(|d|state.prefs.utility_profiles.get(&d.id)).map(|p|p.metrics),bar.rail_face,bar.rail.opacity,if state.prefs.appearance.motion_reduced(){bar.start_goal()}else{bar.motion.start.value}))
    });
    let mut ps:PAINTSTRUCT=mem::zeroed();let dc=BeginPaint(hwnd,&mut ps);
    if let Some((layout,items,hovered,keyboard,material,active,minimized,attention,drop_target,openness,feedback,animation_tick,start_icon,metrics,rail_face,rail_opacity,start_motion))=layered{
        EndPaint(hwnd,&ps);
        let key=hwnd as usize;
        let paint=animation_tick.filter(|_|perf::active()).map(|tick|(tick,std::time::Instant::now()));
        let size=(layout.window.right-layout.window.left,layout.window.bottom-layout.window.top);
        let canvas=SURFACES.with(|s|s.borrow_mut().remove(&key));
        let canvas=match canvas{Some(c) if c.size()==size=>Some(c),_=>surface::Surface::new(size.0,size.1,layout.glass,material)};
        if let Some(mut canvas)=canvas{
            canvas.set_material(material);
            if openness>=0.997{
                canvas.refresh_glass(&layout,windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd).clamp(72,288));
            }
            // Clone Arc handles only; do not hold the atlas borrow across
            // UpdateLayeredWindow, which may dispatch nested Win32 messages.
            let atlas=if canvas.has_motion_frame(&layout,openness)||openness<=0.003{surface::Atlas::default()}else{ATLAS.with(|a|{
                let mut a=a.borrow_mut();
                a.as_mut().map(|atlas|atlas.request(&items)).unwrap_or_default()
            })};
            let applied=if openness<=0.003 {canvas.rail(hwnd,&layout,rail_face,rail_opacity)} else {canvas.paint(hwnd,&layout,&items,&atlas,start_icon,active,minimized,attention,keyboard.or_else(||usize::try_from(hovered).ok()),drop_target,openness,feedback,metrics,rail_face,rail_opacity,keyboard,start_motion)};
            if applied{
                if let Some((tick,paint))=paint{perf::submitted(key,tick,paint);}
                let start=layout.start_anchor(openness);
                let moved=canvas.update_popup_anchor(start);
                SURFACES.with(|s|s.borrow_mut().insert(key,canvas));
                if moved{crate::launcher::start::follow_dock(start);}
                control_center::follow_hub(hwnd,layout.hub_anchor(openness));
                calendar::follow(hwnd,layout.popup_anchor(layout.clock,false,openness));
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
        let dark=stage.dark;
        let switch=stage.group_switch.as_ref().and_then(|swap|
            stage.items.iter().position(|item|item.hwnd as usize==swap.source).map(|i|(i,swap.ink)));
        Some((stage.items.clone(),ready,stage.groups.clone(),stage.members.clone(),stage.layout?,stage.page,stage.pages,state.foreground,dpi,dark,stage.emphasis.highlight(stage.keyboard),stage.hover,stage.layered,stage.opacity,state.prefs.appearance.motion_reduced(),switch,stage.resting))
    });
    if let Some((items,ready,groups,members,layout,page,pages,foreground,dpi,dark,emphasis,hover,layered,opacity,reduced,switch,resting))=stage {
        let key=hwnd as usize;
        let size=(area.right,area.bottom,dpi);
        let panel=STAGE_SURFACES.with(|s|s.borrow_mut().remove(&key));
        let mut panel=match panel{Some(p) if p.geometry()==size=>Some(p),_=>stage_surface::Panel::new(size.0,size.1,size.2)};
        let selected=|i:usize|foreground!=0&&members[i].contains(&(foreground as usize));
        let active_mask=(0..items.len()).filter(|&i|selected(i)).fold(0u8,|bits,i|bits|(1<<i));
        let atlas=ATLAS.with(|a|a.borrow_mut().as_mut().map(|w|w.request(&items)).unwrap_or_default());
        let labels=items.iter().enumerate().map(|(i,item)|{
            if groups[i].is_some(){return item.title.clone();}
            stage_caption(&item.title,items.iter().enumerate().filter(|(j,o)|*j!=i&&groups[*j].is_none()&&o.path==item.path).map(|(_,o)|o.title.as_str()))
        }).collect::<Vec<_>>();
        let still=stage_snapshot::views(hwnd as usize);
        let mut projected=[0;8];
        for view in &still{projected[view.target.index]=view.mask;}
        let look=stage_surface::Look{dark,layout,active:active_mask,alpha:layered.then_some((opacity*255/100) as u8),
            projected,reduced};
        if layered{
            EndPaint(hwnd,&ps);
            if let Some(p)=panel.as_mut(){p.refresh_glass(resting,dark,opacity);}
        }
        let rendered=panel.as_mut().is_some_and(|p|p.paint(dc,look,&items,&atlas,&ready,emphasis,layered.then_some((hwnd,&labels[..],page,pages,hover)),&still,switch));
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
        for (i,title) in labels.iter().enumerate(){
            stage_surface::caption(dc,layout,i,title,ink);
            if !rendered {
                if let Some([left,right])=layout.group_arrows(i).filter(|_|members[i].len()>1){
                    stage_surface::nav_label(dc,left,"‹",ink);
                    stage_surface::nav_label(dc,right,"›",ink);
                }
            }
        }
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
        label(dc,area.right-52,20,42,"⌃",0x00ffffff);
    }
    EndPaint(hwnd,&ps);
}
unsafe fn request_utilities(hwnd:HWND,widget:Option<UtilityWidget>){
    request_utilities_with_change(hwnd,widget,None);
}
unsafe fn request_utilities_with_change(hwnd:HWND,widget:Option<UtilityWidget>,change:Option<wifi::Change>){
    let dock_mode=taskbar_hidden();
    if dock_mode&&widget==Some(UtilityWidget::Windows){
        let anchor=dock_app_anchor(hwnd,None);
        let cards=STATE.with(|s|{
            let state=s.borrow();
            let Some(bar)=state.bars.iter().find(|b|b.hwnd==hwnd)else{return Vec::new();};
            state.windows.iter().filter(|w|state.prefs.all_windows||rect_intersects(w.rect,bar.bounds)).take(36)
                .filter_map(|w|{let id=WindowIdentity::read(w.hwnd)?;
                    Some(control_center::Card{icon:Some(UtilityWidget::Windows),title:w.title.clone(),hint:basename(&w.path),
                        action:Some(control_center::Command::Window{hwnd:id.hwnd,pid:id.pid,tid:id.tid})})
                }).collect()
        });
        control_center::show_list(hwnd,anchor,control_center::View{title:"Chọn cửa sổ".into(),hint:"Cửa sổ ứng dụng trên Dock".into(),cards,metrics:None});
        return;
    }
    if dock_mode&&!utilities::available_in_dock_mode(widget){return;}
    if utility_profile(hwnd).is_none_or(|p|widget.is_some()&&((!p.enabled&&!dock_mode)||widget.is_some_and(|w|p.hidden.contains(&w)&&!(dock_mode&&matches!(w,UtilityWidget::Network|UtilityWidget::Bluetooth|UtilityWidget::Sound|UtilityWidget::Settings))))){return;}
    if widget.is_none(){
        open_dock_hub(hwnd,dock_mode);
        return;
    }
    if widget==Some(UtilityWidget::RecycleBin){recycle_bin::perform(hwnd,recycle_bin::Action::Open);return;}
    if let Some(action)=widget.filter(|w|*w!=UtilityWidget::QuickSettings).and_then(UtilityWidget::simple){
        if !dock_mode||utilities::local_action(action){match action {utilities::Action::Host(action)=>emit(Event::Utility(action)),_=>utilities::delegated(action)}}
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
    if widget==Some(UtilityWidget::Downloads){folder_stack::open(hwnd,panel_chrome::Anchor::for_owner(hwnd,point));return;}
    let Some(widget)=widget else{return;};
    let anchor=panel_chrome::Anchor::for_owner(hwnd,point);
    use control_center::{Card,View};
    control_center::show(hwnd,anchor,View{title:widget.title().into(),hint:"Đang đọc trạng thái từ Windows…".into(),
        cards:vec![Card{title:"Đang tải…".into(),icon:Some(widget),hint:String::new(),action:None}],metrics:None});
    let Some(ticket)=control_center::ticket(hwnd)else{return;};
    let request=UtilityRequest{owner:hwnd as usize,point,widget,dock_mode,ticket,change,started:Instant::now()};
    if !submit_utility(&request){
        UTILITY_RETRY.with(|slot|slot.replace(Some(request)));
        SetTimer(CONTROL.with(Cell::get),UTILITY_RETRY_TIMER,70,None);
    }
}
struct UtilityRequest{
    owner:usize,point:POINT,widget:UtilityWidget,dock_mode:bool,ticket:usize,change:Option<wifi::Change>,started:Instant,
}
fn submit_utility(request:&UtilityRequest)->bool{
    if UTILITY_ANCHOR.with(|a|a.get().is_some())||UTILITY_HUB.with(|a|a.get().is_some()){return false;}
    let submitted=UTILITIES.with(|u|u.borrow_mut().get_or_insert_with(utilities::Provider::new)
        .read(CONTROL.with(Cell::get),request.widget==UtilityWidget::Network,request.change.clone()));
    if submitted{UTILITY_ANCHOR.with(|a|a.set(Some((request.owner,request.point,request.widget,request.dock_mode,request.ticket))));}
    submitted
}
unsafe fn retry_utility(control:HWND){
    let request=UTILITY_RETRY.with(|r|r.borrow_mut().take());
    if let Some(request)=request{
        let hwnd=request.owner as HWND;
        if control_center::ticket(hwnd)==Some(request.ticket)&&request.dock_mode==taskbar_hidden(){
            if submit_utility(&request){KillTimer(control,UTILITY_RETRY_TIMER);return;}
            if request.started.elapsed()<Duration::from_secs(3){
                UTILITY_RETRY.with(|r|r.replace(Some(request)));
                return;
            }
            control_center::update(hwnd,request.ticket,control_center::View{title:request.widget.title().into(),
                hint:"Windows chưa trả trạng thái.".into(),metrics:None,cards:vec![control_center::Card{title:"Thử lại".into(),
                icon:Some(request.widget),hint:String::new(),action:Some(control_center::Command::Widget(request.widget))}]});
        }
    }
    KillTimer(control,UTILITY_RETRY_TIMER);
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
fn dock_wants_status(bar:&Bar)->bool{
    !bar.hidden&&!bar.collapsed&&bar.layout.as_ref().is_some_and(|l|
        l.inline.iter().any(|(w,_)|utilities::live_widget(*w)))
}
fn request_dock_status(){
    let wanted=STATE.with(|s|{let s=s.borrow();!s.suspended&&(
        s.bars.iter().any(dock_wants_status)||control_center::wants_status())});
    if wanted{
        UTILITIES.with(|u|u.borrow_mut().get_or_insert_with(utilities::Provider::new).read_status(CONTROL.get()));
    }
}
unsafe fn status_changed(){
    let bars=STATE.with(|s|s.borrow().bars.iter().filter(|b|dock_wants_status(b)).map(|b|b.hwnd).collect::<Vec<_>>());
    for hwnd in bars{InvalidateRect(hwnd,ptr::null(),0);}
    control_center::status_changed();
}
unsafe fn show_utilities(){
    let status=UTILITIES.with(|u|u.borrow().as_ref().and_then(utilities::Provider::take_status));
    if status.is_some_and(utilities::replace_status){status_changed();}
    let result=UTILITIES.with(|u|u.borrow().as_ref().and_then(utilities::Provider::take_audio_result));
    if let Some((panel,result))=result{
        if utilities::replace_audio(result){status_changed();}
        utilities::reflect_audio(panel as HWND,result);
    }
    let Some(snapshot)=UTILITIES.with(|u|u.borrow().as_ref().and_then(utilities::Provider::take)) else{return;};
    if utilities::replace_status(snapshot.status.clone()){status_changed();}
    let target=UTILITY_ANCHOR.with(|a|a.take());
    let hub=UTILITY_HUB.with(|a|a.take());
    let Some((bar,point,widget,dock_mode,ticket))=target else{
        if let Some((bar,ticket,dock_mode))=hub{
            let hwnd=bar as HWND;
            if let Some(profile)=utility_profile(hwnd){
                control_center::update(hwnd,ticket,utilities::hub_view(&snapshot,&profile,dock_mode,true));
            }
        }
        return;
    };
    if dock_mode!=taskbar_hidden(){return;}
    let hwnd=bar as HWND;
    let Some(profile)=utility_profile(hwnd).filter(|p|p.enabled||dock_mode) else{return;};
    if control_center::ticket(hwnd)==Some(ticket){utilities::show_view(hwnd,point,&snapshot,&profile,Some(widget),dock_mode);}
}
fn refresh_dock_clock(){
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    thread_local!{static CLOCK:Cell<Option<(u16,u16,u16,u16,u16)>>=const{Cell::new(None)};}
    unsafe {
        let mut t:SYSTEMTIME=mem::zeroed();GetLocalTime(&mut t);
        let stamp=Some((t.wYear,t.wMonth,t.wDay,t.wHour,t.wMinute));
        if CLOCK.with(|c|c.replace(stamp))!=stamp{
            STATE.with(|s|for bar in s.borrow().bars.iter().filter(|b|!b.hidden&&!b.collapsed){InvalidateRect(bar.hwnd,ptr::null(),0);});
        }
    }
}
unsafe fn panel_closed(owner:HWND){
    if !owner.is_null()&&IsWindow(owner)!=0{SetTimer(owner,HIDE_TIMER,650,None);}
}
unsafe fn show_background_apps(owner:HWND){
    use control_center::{Card,Command,View};
    let Some(anchor)=shell_anchor(owner,shell_access::Target::Tray)else{return;};
    let in_takeover=taskbar_active();
    let mut cards=vec![Card{icon:None,title:"Biểu tượng trong notification area".into(),
        hint:if in_takeover{"Cần hiện lại thanh tác vụ để dùng icon và menu gốc"}
            else{"Mở khay ứng dụng do Explorer quản lý"}.into(),
        action:Some(Command::WindowsTray)}];
    let minimized=STATE.with(|s|{
        let s=s.borrow();s.windows.iter().filter(|w|IsIconic(w.hwnd)!=0)
            .filter_map(|w|{
                let id=WindowIdentity::read(w.hwnd)?;
                Some(Card{icon:Some(UtilityWidget::Windows),
                    title:if w.title.trim().is_empty(){basename(&w.path)}else{w.title.clone()},
                    hint:format!("Đã thu nhỏ · {}",basename(&w.path)),
                    action:Some(Command::Window{hwnd:id.hwnd,pid:id.pid,tid:id.tid})})
            }).take(64).collect::<Vec<_>>()
    });
    if minimized.is_empty(){
        cards.push(Card{icon:None,title:"Không có cửa sổ thu nhỏ".into(),hint:String::new(),action:None});
    }else{cards.extend(minimized);}
    control_center::show_list(owner,anchor,View{title:"Ứng dụng chạy nền / Khay ứng dụng".into(),
        hint:"Cửa sổ thu nhỏ và biểu tượng khay là hai loại riêng".into(),cards,metrics:None});
}
unsafe fn perform_control(owner:HWND,command:control_center::Command){
    use control_center::Command;
    match command{
        Command::Widget(widget)=>request_utilities(owner,Some(widget)),
        Command::App{path,operation}=>perform_app_action(owner,&path,operation),
        Command::AddApp=>pin_from_file(owner),
        Command::BackgroundApps=>show_background_apps(owner),
        Command::Clipboard=>show_clipboard_history(owner),
        Command::AudioMixer=>{let mut point=POINT{x:0,y:0};GetCursorPos(&mut point);audio_mixer::open(owner,point);},
        Command::Media=>{let mut point=POINT{x:0,y:0};GetCursorPos(&mut point);media::open(owner,point);},
        Command::Comfort(action)=>comfort::perform(owner,action),
        Command::Recycle(action)=>recycle_bin::perform(owner,action),
        Command::ClipboardCopy(id)=>{
            if clipboard_history::active()&&!clipboard_history::copy(owner,id){show_clipboard_history(owner);}
        }
        Command::ClipboardRemove(id)=>{
            if clipboard_history::active(){clipboard_history::remove(id);show_clipboard_history(owner);}
        }
        Command::WindowsTray=>{
            if taskbar_active(){
                show_taskbar(taskbar_guard::Exit::Requested,"Đã hiện thanh tác vụ. Mở lại ^ → Ứng dụng chạy nền / Khay ứng dụng để dùng khay Windows.");
            }else{
                let anchor=shell_anchor(owner,shell_access::Target::Tray);
                shell_access::activate(shell_access::Target::Tray,false,anchor,owner,CONTROL.with(Cell::get));
            }
        },
        Command::Wifi(change)=>request_utilities_with_change(owner,Some(UtilityWidget::Network),Some(change)),
        Command::Run(utilities::Action::Host(action))=>emit(Event::Utility(action)),
        Command::Run(action)=>if !taskbar_hidden()||utilities::local_action(action){utilities::delegated(action)},
        Command::Window{hwnd,pid,tid}=>{
            let hwnd=hwnd as HWND;
            if window_owner(hwnd)!=Some((pid,tid)){return;}
            let item=STATE.with(|s|s.borrow().windows.iter().find(|w|w.hwnd==hwnd)
                .map(|w|Item{path:w.path.clone(),title:w.title.clone(),hwnd,count:1,pinned:false}));
            if let Some(item)=item{activate(owner,&item);}
        }
        Command::StageMember(identity)=>{
            if !identity.matches(identity.hwnd as HWND){return;}
            let index=STATE.with(|s|s.borrow().stages.iter().find(|p|p.hwnd==owner&&!p.hidden)?
                .members.iter().position(|members|members.contains(&identity.hwnd)));
            if let Some(index)=index{activate_stage_member(owner,index,Some(identity.hwnd as HWND));}
        }
    }
}
unsafe fn open_launcher(bar:HWND){
    let owner=OWNER.with(Cell::get);
    let monitor=MonitorFromWindow(bar,MONITOR_DEFAULTTONEAREST);
    let mut info:MONITORINFO=std::mem::zeroed();
    info.cbSize=std::mem::size_of::<MONITORINFO>() as u32;
    if !owner.is_null() && GetMonitorInfoW(monitor,&mut info)!=0{
        let r=info.rcMonitor;
        PostMessageW(owner,crate::WM_OPEN_DOCK_SPOTLIGHT,((r.left+r.right)/2) as WPARAM,((r.top+r.bottom)/2) as LPARAM);
    }
}
unsafe fn open(uri:&str) {
    ShellExecuteW(ptr::null_mut(),wide("open").as_ptr(),wide(uri).as_ptr(),ptr::null(),ptr::null(),SW_SHOWNORMAL);
}
unsafe fn open_task_manager(){
    let mut path=[0u16;260];
    let n=windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(path.as_mut_ptr(),path.len() as u32) as usize;
    if n!=0&&n<path.len(){
        let exe=std::path::PathBuf::from(String::from_utf16_lossy(&path[..n])).join("Taskmgr.exe");
        open(&exe.to_string_lossy());
    }
}
unsafe fn show_feedback(hwnd:HWND,item:&Item,kind:FeedbackKind){
    let (start,reduced)=STATE.with(|s|{
        let mut s=s.borrow_mut();
        let reduced=s.prefs.appearance.motion_reduced();
        let Some(bar)=s.bars.iter_mut().find(|b|b.hwnd==hwnd&&!b.hidden&&b.layout.is_some())else{return(false,reduced);};
        bar.feedback=Some(Feedback{path:item.path.clone(),kind,motion:motion::Span::fixed(motion::duration(420).clamp(170,550))});
        (if reduced{true}else{bar.schedule()},reduced)
    });
    if start {SetTimer(hwnd,if reduced{FEEDBACK_TIMER}else{MOTION_TIMER},if reduced{420}else{16},None);}
    InvalidateRect(hwnd,ptr::null(),0);
}
unsafe fn activate(hwnd:HWND,item:&Item) {
    if item.path==internal_app::KEY{
        window_effect::cancel();
        PostMessageW(OWNER.with(Cell::get),crate::WM_OPEN_SETTINGS,0,0);
        return;
    }
    if !item.hwnd.is_null() && IsWindow(item.hwnd)!=0 {
        if IsIconic(item.hwnd)!=0{
            if let Some(id)=WindowIdentity::read(item.hwnd){
                if window_effect::restore(hwnd,&item.path,id){show_feedback(hwnd,item,FeedbackKind::Restore);return;}
            }
            let restored=ShowWindowAsync(item.hwnd,SW_RESTORE)!=0;
            show_feedback(hwnd,item,if restored{FeedbackKind::Restore}else{FeedbackKind::Failed});
        }else{window_effect::cancel();}
        SetForegroundWindow(item.hwnd);
    } else {
        window_effect::cancel();
        let result=command_pin::open(&item.path);
        show_feedback(hwnd,item,if result>32{FeedbackKind::Launch}else{FeedbackKind::Failed});
    }
}
#[derive(Clone)]
enum AppOperation{
    Open, NewWindow, ShowMenu, JumpList, TogglePin, Close(Vec<WindowIdentity>), Advanced, ChangeIcon, ResetIcon,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq,Hash)]
struct WindowIdentity{hwnd:usize,pid:u32,tid:u32}
impl WindowIdentity{
    unsafe fn read(hwnd:HWND)->Option<Self>{
        let (pid,tid)=window_owner(hwnd)?;
        Some(Self{hwnd:hwnd as usize,pid,tid})
    }
    unsafe fn matches(self,hwnd:HWND)->bool{
        self.hwnd==hwnd as usize&&window_owner(hwnd)==Some((self.pid,self.tid))
    }
    unsafe fn read_stage(hwnd:HWND)->Option<Self>{
        let (pid,tid)=stage_window_owner(hwnd)?;
        Some(Self{hwnd:hwnd as usize,pid,tid})
    }
    unsafe fn matches_stage(self)->bool{
        stage_window_owner(self.hwnd as HWND)==Some((self.pid,self.tid))
    }
}

#[derive(PartialEq,Eq)]
enum ClickKey{Dock(String),Stage(String)}
struct AppClick{owner:usize,key:ClickKey,window:Option<WindowIdentity>,was_open:bool,at:Instant}
enum DeferredClick{DockPicker(String),StageMember(WindowIdentity)}
thread_local!{
    static APP_CLICK:RefCell<Option<AppClick>>=const{RefCell::new(None)};
    static DEFERRED_CLICK:RefCell<Option<(usize,DeferredClick)>>=const{RefCell::new(None)};
}
// The second click uses the first window's identity and original visibility.
unsafe fn app_click(owner:HWND,key:ClickKey,target:HWND,double:bool)->Option<AppClick>{
    let window=if matches!(&key,ClickKey::Stage(_)){WindowIdentity::read_stage(target)}else{WindowIdentity::read(target)};
    APP_CLICK.with(|slot|{
        let mut slot=slot.borrow_mut();
        if double{
            if let Some(first)=slot.take().filter(|p|p.owner==owner as usize&&p.key==key
                &&(!matches!(&key,ClickKey::Stage(_))||p.window==window)
                &&p.at.elapsed()<=Duration::from_millis(GetDoubleClickTime() as u64+100)){
                return Some(first);
            }
        }
        *slot=Some(AppClick{owner:owner as usize,key,window,at:Instant::now(),
            was_open:!target.is_null()&&IsWindowVisible(target)!=0&&IsIconic(target)==0});
        None
    })
}
unsafe fn minimize_click(window:WindowIdentity)->bool{
    let hwnd=window.hwnd as HWND;
    window.matches_stage()&&IsIconic(hwnd)==0&&(internal_app::owns(hwnd)||GetWindowLongPtrW(hwnd,GWL_STYLE)&WS_MINIMIZEBOX as isize!=0)
        &&ShowWindowAsync(hwnd,if taskbar_hidden(){SW_SHOWMINNOACTIVE}else{SW_MINIMIZE})!=0
}
unsafe fn finish_double_click(click:AppClick)->Option<FeedbackKind>{
    let window=click.window?;
    let hwnd=window.hwnd as HWND;
    if !window.matches_stage(){return None;}
    if crate::trial::active()&&taskbar_hidden(){
        let surface=match &click.key{ClickKey::Dock(_)=>"Dock",ClickKey::Stage(_)=>"Stage"};
        crate::log!("DOCK_TASKBAR_APP gesture=double-click surface={} hwnd={} was_open={}",surface,window.hwnd,click.was_open);
    }
    if click.was_open{
        if let ClickKey::Dock(path)=&click.key{
            if window_effect::minimize(click.owner as HWND,path,window){return Some(FeedbackKind::Minimize);}
        }
        window_effect::cancel();
        minimize_click(window).then_some(FeedbackKind::Minimize)
    }
    else if IsIconic(hwnd)!=0{
        if matches!(&click.key,ClickKey::Dock(_))&&window_effect::restoring(window){return None;}
        window_effect::cancel();
        focus_stage_window(hwnd);Some(FeedbackKind::Restore)
    }
    else{None}
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
unsafe fn change_app_icon(owner:HWND,path:&str,reset:bool){
    use windows_sys::Win32::UI::Controls::Dialogs::*;
    let Some(key)=crate::app_icons::key(path)else{return;};
    if reset{emit(Event::Edit(Edit::AppIcon{key,file:None}));return;}
    let capped=STATE.with(|s|{
        let s=s.borrow();
        s.prefs.app_icons.len()>=crate::app_icons::MAX_ICONS&&!s.prefs.app_icons.contains_key(&key)
    });
    if capped{
        MessageBoxW(owner,wide("Đã đạt giới hạn icon riêng. Khôi phục icon của một ứng dụng trước.").as_ptr(),
            wide("Icon ứng dụng").as_ptr(),MB_OK|MB_ICONWARNING);return;
    }
    let mut file=[0u16;32768];
    let filter=wide("Ảnh và icon (*.png;*.ico;*.jpg;*.jpeg;*.bmp;*.webp)\0*.png;*.ico;*.jpg;*.jpeg;*.bmp;*.webp\0\0");
    let title=wide("Chọn icon cho ứng dụng");
    let mut dialog:OPENFILENAMEW=mem::zeroed();
    dialog.lStructSize=mem::size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner=owner;dialog.lpstrFilter=filter.as_ptr();dialog.lpstrFile=file.as_mut_ptr();
    dialog.nMaxFile=file.len() as u32;dialog.lpstrTitle=title.as_ptr();
    dialog.Flags=OFN_FILEMUSTEXIST|OFN_PATHMUSTEXIST|OFN_NOCHANGEDIR|OFN_EXPLORER;
    if GetOpenFileNameW(&mut dialog)==0{return;}
    let name=utf16(&file);
    match crate::app_icons::import(std::path::Path::new(&name)){
        Ok(file)=>emit(Event::Edit(Edit::AppIcon{key,file:Some(file)})),
        Err(error)=>{MessageBoxW(owner,wide(&error).as_ptr(),title.as_ptr(),MB_OK|MB_ICONWARNING);}
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
    let stem=basename(&item.path);
    let name=match stem.to_ascii_lowercase().as_str(){
        "explorer"=>"File Explorer".into(),"chrome"=>"Google Chrome".into(),"msedge"=>"Microsoft Edge".into(),
        "powershell"|"pwsh"=>"PowerShell".into(),"notepad"=>"Notepad".into(),_=>stem,
    };
    let mut cards=Vec::new();
    let all=windows.len();
    for (w,id) in &windows{
        let status=if w.hwnd==foreground{"Đang dùng"}else if IsIconic(w.hwnd)!=0{"Đã thu nhỏ"}else{"Đang mở"};
        cards.push(Card{icon:Some(UtilityWidget::Windows),
            title:if w.title.trim().is_empty(){name.clone()}else{w.title.clone()},
            hint:status.into(),action:Some(Command::Window{hwnd:id.hwnd,pid:id.pid,tid:id.tid})});
    }
    if !picker{
        let mut add=|title:&str,operation|cards.push(Card{icon:None,title:title.into(),hint:String::new(),
            action:Some(Command::App{path:item.path.clone(),operation})});
        if windows.is_empty(){
            add("Mở ứng dụng",AppOperation::Open);
        }else if item.path==internal_app::KEY{
            add("Hiện cài đặt",AppOperation::Open);
        }else{
            add("Mở thêm",AppOperation::NewWindow);
        }
        add(if item.pinned{"Bỏ ghim khỏi Dock"}else{"Ghim vào Dock"},AppOperation::TogglePin);
        if item.path!=internal_app::KEY{
            add(crate::launcher::localized("Lệnh ứng dụng và tệp gần đây…","App commands and recent files…","应用命令和最近文件…"),AppOperation::JumpList);
            add("Đổi icon ứng dụng…",AppOperation::ChangeIcon);
            if STATE.with(|s|crate::app_icons::has(&s.borrow().prefs.app_icons,&item.path)){
                add("Khôi phục icon mặc định",AppOperation::ResetIcon);
            }
        }
        if advanced&&!windows.is_empty(){
            add("Quản lý cửa sổ…",AppOperation::Advanced);
        }
        if !windows.is_empty(){
            add(if windows.len()==1{"Đóng cửa sổ"}else{"Đóng các cửa sổ của ứng dụng"},
                AppOperation::Close(windows.iter().map(|(_,id)|*id).collect()));
        }
    }
    if cards.is_empty(){return;}
    let hint=if all==0{if item.pinned{"Đã ghim".into()}else{String::new()}}
        else{format!("{all} cửa sổ{}",if item.pinned{" · Đã ghim"}else{""})};
    control_center::show_list(owner,dock_app_anchor(owner,Some(path)),View{title:if picker{"Chọn cửa sổ".into()}else{name},hint,cards,metrics:None});
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
        control_center::show_list(owner,dock_app_anchor(owner,None),View{title:"Ứng dụng khác".into(),hint:"Ứng dụng ngoài hàng chính của Dock".into(),cards,metrics:None});
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
        AppOperation::JumpList=>{
            let source=WindowIdentity::read(item.hwnd);
            let others=STATE.with(|s|s.borrow().windows.iter().filter(|w|!w.path.eq_ignore_ascii_case(path))
                .filter_map(|w|w.path.strip_prefix("shell:AppsFolder\\").map(str::to_owned).or_else(||w.stage_aumid.clone())).collect());
            jump_list::open(owner,path,source,others);
        }
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
            for hwnd in safe{
                if path!=internal_app::KEY||internal_app::current()==hwnd{PostMessageW(hwnd,WM_CLOSE,0,0);}
            }
        },
        AppOperation::Advanced=>{if let Some(index)=index{native_menu(owner,bar,index);}},
        AppOperation::ChangeIcon=>change_app_icon(owner,&item.path,false),
        AppOperation::ResetIcon=>change_app_icon(owner,&item.path,true),
    }
}
unsafe fn open_dock_item(owner:HWND,item:&Item){
    if item.count>1{app_menu(owner,&item.path,true);}else{activate(owner,item);}
}
unsafe fn cancel_deferred_click(owner:HWND){
    DEFERRED_CLICK.with(|p|{
        let mut pending=p.borrow_mut();
        if pending.as_ref().is_some_and(|(hwnd,_)|*hwnd==owner as usize){
            KillTimer(owner,APP_CLICK_TIMER);pending.take();
        }
    });
}
unsafe fn defer_click(owner:HWND,click:DeferredClick){
    if let Some((old,_))=DEFERRED_CLICK.with(|p|p.borrow_mut().take()){
        KillTimer(old as HWND,APP_CLICK_TIMER);
    }
    if SetTimer(owner,APP_CLICK_TIMER,GetDoubleClickTime(),None)!=0{
        DEFERRED_CLICK.with(|p|p.replace(Some((owner as usize,click))));
    }else{run_deferred_click(owner,click);}
}
unsafe fn run_deferred_click(owner:HWND,click:DeferredClick){
    match click{
        DeferredClick::DockPicker(path)=>app_menu(owner,&path,true),
        DeferredClick::StageMember(id) if id.matches(id.hwnd as HWND)=>{
            let index=STATE.with(|s|{
                let s=s.borrow();
                let stage=s.stages.iter().find(|stage|stage.hwnd==owner&&!stage.hidden)?;
                stage.members.iter().position(|m|m.contains(&id.hwnd))
            });
            if let Some(i)=index{activate_stage_member(owner,i,Some(id.hwnd as HWND));}
        }
        _=>{}
    }
}
unsafe fn dock_context(owner:HWND,bar:usize,index:usize,advanced:bool){
    let path=STATE.with(|s|s.borrow().bars.get(bar).filter(|b|b.hwnd==owner)
        .and_then(|b|b.items.get(index)).map(|item|item.path.clone()));
    if let Some(path)=path.filter(|_|!advanced){app_menu(owner,&path,false);}
    else{native_menu(owner,bar,index);}
}
thread_local!{
    static TASKBAR_RETRY:RefCell<taskbar_state::Recovery>=RefCell::new(taskbar_state::Recovery::default());
    static TASKBAR_RESUME:Cell<bool>=const{Cell::new(false)};
    static TASKBAR_STOPPING:RefCell<Option<taskbar_guard::GuardProcess>>=const{RefCell::new(None)};
    static TASKBAR_WATCH:RefCell<Option<taskbar_guard::Watch>>=const{RefCell::new(None)};
    static TASKBAR_STATUS:RefCell<TaskbarStatus>=RefCell::new(TaskbarStatus::default());
    static TASKBAR_RESTORE:RefCell<Option<(bool,TaskbarMode,&'static str,String,Instant,usize)>>=const{RefCell::new(None)};
    static START_BOUND:Cell<bool>=const{Cell::new(false)};
    static WIN_KEY:RefCell<Option<win_key::Hook>>=const{RefCell::new(None)};
    static WIN_KEY_TOKEN:Cell<usize>=const{Cell::new(0)};
    static WIN_KEY_STATE:Cell<&'static str>=const{Cell::new("")};
    static WIN_KEY_FAILED:Cell<bool>=const{Cell::new(false)};
}
fn taskbar_active()->bool{TASKBAR_WATCH.with(|w|w.borrow().is_some())}
fn taskbar_hidden()->bool{TASKBAR_STATUS.with(|s|s.borrow().effective.engaged())}
fn taskbar_pending()->bool{taskbar_active()&&!taskbar_hidden()}
fn local_shell_buttons()->bool{
    let opt_in=STATE.with(|s|s.borrow().prefs.native_shell_buttons);
    shell_access::local_buttons(taskbar_active(),opt_in)
}

unsafe fn sync_start_hotkey(control:HWND,requested:bool){
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey,UnregisterHotKey,MOD_CONTROL,MOD_ALT,MOD_NOREPEAT,VK_SPACE};
    let bound=START_BOUND.with(Cell::get);
    if requested&&!bound{
        let ok=RegisterHotKey(control,START_HOTKEY,MOD_CONTROL|MOD_ALT|MOD_NOREPEAT,VK_SPACE as u32)!=0;
        START_BOUND.with(|b|b.set(ok));
        if !ok{
            crate::log!("DOCK_START shortcut unavailable: {}",GetLastError());
            emit(Event::Error("Ctrl + Alt + Space đang được Windows hoặc ứng dụng khác dùng.".into()));
        }
    }else if !requested&&bound{
        UnregisterHotKey(control,START_HOTKEY);
        START_BOUND.with(|b|b.set(false));
    }
}
fn win_key_status(status:&'static str){
    if WIN_KEY_STATE.with(|s|s.replace(status))!=status{
        crate::log!("DOCK_WIN_KEY state={status}");
        emit(Event::WinKey(status.into()));
    }
}
fn sync_win_key(){
    let (requested,context)=STATE.with(|s|{let s=s.borrow();
        (s.prefs.enabled&&s.prefs.win_key_start&&!s.suspended,s.prefs.enabled&&s.prefs.shell_context_menu&&!s.suspended)});
    let on=requested&&taskbar_hidden();
    let allowed=(on||context)&&!crate::trial::benchmark_active()&&!crate::trial::demo_active();
    if !allowed{
        WIN_KEY.with(|s|drop(s.borrow_mut().take()));
        WIN_KEY_FAILED.set(false);
        win_key_status(if requested{"waiting"}else{"off"});
        return;
    }
    if WIN_KEY.with(|s|s.borrow().as_ref().is_some_and(win_key::Hook::finished)){
        WIN_KEY.with(|s|drop(s.borrow_mut().take()));
        WIN_KEY_FAILED.set(true);
        win_key_status("failed");
    }
    if WIN_KEY.with(|s|s.borrow().as_ref().is_some_and(|hook|hook.mode!=(on,context))){
        WIN_KEY.with(|s|drop(s.borrow_mut().take()));WIN_KEY_FAILED.set(false);
    }
    if WIN_KEY_FAILED.get()||WIN_KEY.with(|s|s.borrow().is_some()){return;}
    let generation=WIN_KEY_TOKEN.with(|n|{let next=n.get().wrapping_add(1).max(1);n.set(next);next});
    match win_key::Hook::install_mode(CONTROL.get(),generation,on,context){
        Ok(hook)=>{WIN_KEY.with(|s|s.replace(Some(hook)));win_key_status(if on{"installed"}else if requested{"waiting"}else{"off"});}
        Err(reason)=>{
            crate::log!("DOCK_WIN_KEY install failed: {reason}");
            WIN_KEY_FAILED.set(true);win_key_status("failed");
        }
    }
}
fn screen_for_start(screens:&[Display],cursor_bounds:Option<RECT>,foreground_bounds:Option<RECT>)->Option<&Display>{
    cursor_bounds.and_then(|r|screens.iter().find(|d|same_rect(d.bounds,r)))
        .or_else(||foreground_bounds.and_then(|r|screens.iter().find(|d|same_rect(d.bounds,r))))
        .or_else(||screens.iter().find(|d|d.primary))
}
unsafe fn monitor_bounds(monitor:HMONITOR)->Option<RECT>{
    if monitor.is_null(){return None;}
    let mut info=MONITORINFO{cbSize:mem::size_of::<MONITORINFO>() as u32,..mem::zeroed()};
    (GetMonitorInfoW(monitor,&mut info)!=0).then_some(info.rcMonitor)
}
unsafe fn start_from_pointer(cursor:Option<POINT>){
    let cursor_bounds=cursor.and_then(|p|monitor_bounds(MonitorFromPoint(p,MONITOR_DEFAULTTONULL)));
    let foreground=GetForegroundWindow();
    let foreground_bounds=if !foreground.is_null()&&foreground!=GetDesktopWindow()&&foreground!=GetShellWindow()
        && IsWindowVisible(foreground)!=0&&IsIconic(foreground)==0 {
        monitor_bounds(MonitorFromWindow(foreground,MONITOR_DEFAULTTONULL))
    }else{None};
    let target=STATE.with(|s|{
        let s=s.borrow();
        let screen=screen_for_start(&s.screens,cursor_bounds,foreground_bounds)?;
        let dock=s.bars.iter().find(|b|same_rect(b.bounds,screen.bounds));
        let side=s.prefs.appearance.position;
        Some(if let Some(bar)=dock{start_anchor_for(&s,bar,screen)}else{
            let q=screen.work;
            let point=POINT{x:match side{DockSide::Left=>q.left,DockSide::Right=>q.right-1,_=>q.left+(q.right-q.left)/8},
                y:if side==DockSide::Bottom{q.bottom-1}else{q.top+(q.bottom-q.top)/2}};
            crate::launcher::DockAnchor{point,side}
        })
    });
    if let Some(anchor)=target{open_dock_launcher(anchor);}
}
pub(crate) fn current_taskbar_status(message:&str)->TaskbarStatus{
    let actual=unsafe{taskbar_guard::current()};
    TaskbarStatus{actual,effective:if actual.known&&actual.parked+actual.visible==0{TaskbarMode::Error}else{TaskbarMode::Compatibility},
        message:message.into(),reason:"DockOff",..Default::default()}
}
fn publish_taskbar(desired:bool,effective:TaskbarMode,actual:taskbar_state::Observation,reason:&'static str,message:&str){
    let status=TaskbarStatus{desired,effective,actual,reason,message:message.into()};
    let changed=TASKBAR_STATUS.with(|slot|{
        let mut old=slot.borrow_mut();
        if *old==status{return false;}
        *old=status.clone();true
    });
    if changed {
        crate::log!("DOCK_TASKBAR desired={} effective={} actual={} visible={}/{} parked={} reason={}",
            if desired{"dock"}else{"windows"},effective.key(),actual.key(),actual.visible,actual.total,actual.parked,reason);
        emit(Event::Taskbar(status));
    }
}
fn stopping_watch_done()->bool{
    TASKBAR_STOPPING.with(|slot|{
        let mut child=slot.borrow_mut();
        match child.as_mut().map(taskbar_guard::GuardProcess::try_wait){
            None=>true,
            Some(Ok(Some(_)))=>{child.take();true},
            _=>false,
        }
    })
}
fn cancel_taskbar_retry(){
    TASKBAR_RETRY.with(|r|*r.borrow_mut()=taskbar_state::Recovery::default());
    TASKBAR_RESUME.set(false);
}
fn taskbar_retry_pending()->bool{TASKBAR_RETRY.with(|r|r.borrow().pending())}
unsafe fn schedule_taskbar_retry()->bool{
    let can_retry=SetTimer(CONTROL.get(),TASKBAR_GUARD_TIMER,220,None)!=0
        &&TASKBAR_RETRY.with(|r|r.borrow_mut().schedule(Instant::now()));
    can_retry
}
unsafe fn show_taskbar(reason:taskbar_guard::Exit,message:&str){
    use taskbar_guard::Exit;
    let explicit=matches!(reason,Exit::Requested|Exit::Emergency|Exit::DockLost|Exit::LinkLost);
    if explicit{
        cancel_taskbar_retry();
        // A user choice received mid-restore takes priority over any retry.
        let pending=TASKBAR_RESTORE.with(|s|{
            let mut s=s.borrow_mut();
            if let Some(restoring)=s.as_mut(){
                restoring.0=false;
                if reason==Exit::Requested{restoring.1=TaskbarMode::Compatibility;restoring.2="Requested";restoring.3=message.into();}
                return true;
            }
            false
        });
        if pending{return;}
    }
    if TASKBAR_RESTORE.with(|s|s.borrow().is_some()){return;}
    let control=CONTROL.get();
    let was_dock=taskbar_hidden();
    let desired=!explicit&&TASKBAR_STATUS.with(|s|s.borrow().desired);
    let retryable=desired&&match reason{
        Exit::ShellRevealed|Exit::ExplorerChanged=>true,
        Exit::HideFailed|Exit::HideRejected=>TASKBAR_RETRY.with(|r|r.borrow().attempted()),
        _=>false,
    };
    let retry=retryable&&schedule_taskbar_retry();
    let message=if retry{"Windows đang hiện thanh tác vụ tạm thời. TLauncher sẽ thử bật lại Dock."}else{message};
    sync_start_hotkey(control,false);
    let old=TASKBAR_WATCH.with(|w|w.borrow_mut().take());
    let before=old.as_ref().map_or(1,taskbar_guard::Watch::restore_count);
    let was_active=old.is_some();
    if let Some(watch)=old{
        utilities::close_panels();
        UTILITY_ANCHOR.with(|a|a.set(None));
        queue_refresh();
        TASKBAR_STOPPING.with(|s|s.replace(watch.stop()));
    }
    let (mode,code)=match reason{
        Exit::Requested=>(TaskbarMode::Compatibility,"Requested"),
        Exit::ShellRevealed=>(TaskbarMode::RestoredByWindows,"ShellRevealed"),
        Exit::ExplorerChanged=>(TaskbarMode::RestoredByWindows,"ExplorerChanged"),
        Exit::SessionSuspended=>(TaskbarMode::RestoredByWindows,"SessionSuspended"),
        Exit::Emergency=>(TaskbarMode::RestoredByWatchdog,"Emergency"),
        Exit::DockLost=>(TaskbarMode::RestoredByWatchdog,"DockLost"),
        Exit::LinkLost=>(TaskbarMode::RestoredByWatchdog,"LinkLost"),
        Exit::HideFailed=>(TaskbarMode::Error,"HideFailed"),
        Exit::HideRejected=>(TaskbarMode::Error,"HideRejected"),
        Exit::ReadyTimeout=>(TaskbarMode::Error,"ReadyTimeout"),
        Exit::HotkeyUnavailable=>(TaskbarMode::Error,"HotkeyUnavailable"),
    };
    let current=taskbar_guard::current();
    let expected=if was_dock&&reason!=Exit::ExplorerChanged{before}else{1};
    let incomplete=!stopping_watch_done()||!current.known||current.visible+current.parked<expected;
    if was_active&&incomplete&&SetTimer(control,TASKBAR_GUARD_TIMER,220,None)!=0{
        TASKBAR_RESTORE.with(|s|s.replace(Some((desired,mode,code,message.into(),Instant::now(),expected))));
        publish_taskbar(desired,TaskbarMode::Restoring,current,code,message);
    }else{
        if !retry{KillTimer(control,TASKBAR_GUARD_TIMER);}
        if was_active&&incomplete{
            publish_taskbar(desired,TaskbarMode::Error,current,code,"Chưa xác minh được Windows đã hiện lại đầy đủ các thanh tác vụ.");
        }else{
            publish_taskbar(desired,mode,current,code,message);
        }
    }
    sync_win_key();
}
fn eligible_for_taskbar()->bool{
    // Auto-hide leaves a live Dock HWND and its reveal affordance.
    !crate::trial::demo_active()&&!crate::trial::benchmark_active()&&STATE.with(|s|{
        let s=s.borrow();
        s.prefs.enabled&&!s.suspended&&s.bars.iter().any(|bar|unsafe{IsWindow(bar.hwnd)!=0})
    })
}
unsafe fn set_taskbar_hidden(hidden:bool){
    use taskbar_guard::Exit;
    cancel_taskbar_retry();
    if !hidden {show_taskbar(Exit::Requested,Exit::Requested.message());return;}
    motion_guard::stop();
    if taskbar_active(){return;}
    if TASKBAR_RESTORE.with(|s|s.borrow().is_some())||!stopping_watch_done(){
        schedule_taskbar_retry();
        return;
    }
    begin_taskbar_hide(taskbar_guard::current());
}
unsafe fn start_saved_taskbar_mode(){
    // The previous owner has exited, but Explorer may still be settling an
    // initially hidden secondary bar. Use the same bounded recovery on boot.
    let actual=taskbar_guard::current();
    if schedule_taskbar_retry(){
        publish_taskbar(true,TaskbarMode::HidePending,actual,"StartupWait","Đang đợi Explorer ổn định để bật chế độ Dock…");
    }else{begin_taskbar_hide(actual);}
}
unsafe fn begin_taskbar_hide(current:taskbar_state::Observation){
    motion_guard::stop();
    if !stopping_watch_done(){schedule_taskbar_retry();return;}
    let fail=|code,msg|publish_taskbar(true,TaskbarMode::Error,current,code,msg);
    if shell_access::native_busy(){
        fail("NativePanelOpen","Đợi bảng Windows đóng rồi bật lại chế độ Dock.");return;
    }
    if !eligible_for_taskbar(){
        fail("DockUnavailable","Cần bật Dock trước khi ẩn thanh tác vụ.");return;
    }
    match taskbar_guard::Watch::start(){
        Ok(watch)=>{
            utilities::close_panels();
            UTILITY_ANCHOR.with(|a|a.set(None));
            if SetTimer(CONTROL.get(),TASKBAR_GUARD_TIMER,220,None)==0 {
                TASKBAR_STOPPING.with(|s|s.replace(watch.stop()));
                fail("WatchdogTimer","Không giám sát được khôi phục; thanh tác vụ được giữ lại.");
                return;
            }
            TASKBAR_WATCH.with(|slot|slot.replace(Some(watch)));
            publish_taskbar(true,TaskbarMode::HidePending,current,"HideRequested","Đang kiểm tra đường khôi phục…");
        }
        Err(message)=>fail("WatchdogStart",&message),
    }
}
unsafe fn poll_taskbar(hwnd:HWND){
    use taskbar_guard::Exit;
    use taskbar_state::Retry;
    let actual=taskbar_guard::current();
    let stopped=stopping_watch_done();
    if let Some((desired,mode,code,message,started,expected))=TASKBAR_RESTORE.with(|s|s.borrow().clone()){
        if stopped&&actual.known&&actual.visible+actual.parked>=expected{
            TASKBAR_RESTORE.with(|s|s.borrow_mut().take());
            publish_taskbar(desired,mode,actual,code,&message);
            if !taskbar_retry_pending(){KillTimer(hwnd,TASKBAR_GUARD_TIMER);}
        }else if started.elapsed()>=Duration::from_secs(3){
            TASKBAR_RESTORE.with(|s|s.borrow_mut().take());
            cancel_taskbar_retry();
            publish_taskbar(desired,TaskbarMode::Error,actual,code,"Chưa xác minh được Windows đã hiện lại đầy đủ các thanh tác vụ.");
            KillTimer(hwnd,TASKBAR_GUARD_TIMER);
        }else{
            publish_taskbar(desired,TaskbarMode::Restoring,actual,code,&message);
        }
        return;
    }
    if taskbar_active()&&!eligible_for_taskbar(){
        show_taskbar(Exit::DockLost,Exit::DockLost.message());return;
    }
    let status=TASKBAR_WATCH.with(|w|w.borrow_mut().as_mut().map(taskbar_guard::Watch::poll));
    match status{
        Some(Ok(true))=>{
            let first=!taskbar_hidden();
            // Edge peek is not a successful healthy interval.
            TASKBAR_RETRY.with(|r|{
                let mut r=r.borrow_mut();
                if actual.known&&actual.visible==0{r.confirmed(Instant::now());}
                else{r.interrupted();}
            });
            let mode=TaskbarMode::confirmed(actual);
            let detail=if !actual.known{"Chưa đọc được trạng thái thanh Windows; bộ khôi phục vẫn đang giám sát."}
                else if actual.visible>0{"Thanh Windows vừa xuất hiện; TLauncher đang xử lý để ẩn lại."}
                else if actual.parked>0{"Windows đã đẩy thanh tác vụ ra mép tự ẩn; Dock vẫn quản lý phiên này."}
                else{"Chế độ Dock đang bật cho phiên này. Ctrl+Alt+Shift+F12 để hiện thanh Windows."};
            publish_taskbar(true,mode,actual,"HideConfirmed",detail);
            if first{
                queue_refresh();
                sync_start_hotkey(hwnd,STATE.with(|s|s.borrow().prefs.start_shortcut));
                sync_win_key();
            }
        }
        Some(Ok(false))=>publish_taskbar(true,TaskbarMode::HidePending,actual,"HideRequested","Đang kiểm tra đường khôi phục…"),
        Some(Err(reason))=>show_taskbar(reason,reason.message()),
        None=>{
            let ready=taskbar_retry_pending()&&stopped&&actual.known&&actual.total>0
                &&actual.visible+actual.parked>0&&eligible_for_taskbar()&&!shell_access::native_busy();
            match TASKBAR_RETRY.with(|r|r.borrow_mut().check(Instant::now(),ready)){
                Retry::Idle=>{KillTimer(hwnd,TASKBAR_GUARD_TIMER);}
                Retry::Wait=>{}
                Retry::Start=>{
                    begin_taskbar_hide(actual);
                    if !taskbar_active(){KillTimer(hwnd,TASKBAR_GUARD_TIMER);}
                }
                Retry::Expired=>{
                    let status=TASKBAR_STATUS.with(|s|s.borrow().clone());
                    let mode=if status.effective.pending(){TaskbarMode::Error}else{status.effective};
                    publish_taskbar(status.desired,mode,actual,status.reason,"Chưa thể bật lại Dock an toàn. Có thể thử lại từ Cài đặt.");
                    KillTimer(hwnd,TASKBAR_GUARD_TIMER);
                }
            }
        }
    }
}
unsafe fn show_clipboard_history(owner:HWND){
    if !clipboard_history::active(){return;}
    if let Some(anchor)=shell_anchor(owner,shell_access::Target::Tray){
        control_center::show_list(owner,anchor,clipboard_history::view());
    }
}
unsafe fn refresh_clipboard_listener(){
    let hwnd=CONTROL.with(Cell::get);
    let (enabled,limit)=STATE.with(|s|{
        let s=s.borrow();
        (s.prefs.enabled&&!s.suspended&&s.prefs.clipboard_history,s.prefs.clipboard_history_limit)
    });
    clipboard_history::configure(hwnd,enabled,limit as usize);
}
unsafe fn open_dock_hub(owner:HWND,dock_mode:bool){
    let dock=STATE.with(|s|s.borrow().bars.iter().find(|b|b.hwnd==owner&&!b.hidden)
        .and_then(|b|Some(b.layout.as_ref()?.hub_anchor(b.reveal.value))));
    let Some(dock)=dock else{return;};
    let anchor=panel_chrome::Anchor::for_owner(owner,dock.point);
    UTILITY_ANCHOR.with(|a|a.set(None));
    if let Some(profile)=utility_profile(owner){
        utilities::show_hub(owner,anchor,&profile,dock_mode);
        if let Some(ticket)=control_center::ticket(owner).filter(|_|profile.details&&UTILITY_HUB.with(|a|a.get().is_none())){
            let submitted=UTILITIES.with(|u|u.borrow_mut().get_or_insert_with(utilities::Provider::new)
                .read(CONTROL.with(Cell::get),false,None));
            if submitted{UTILITY_HUB.with(|h|h.set(Some((owner as usize,ticket,dock_mode))));}
        }
    }
}
unsafe fn shell_anchor(owner:HWND,target:shell_access::Target)->Option<panel_chrome::Anchor>{
    let slot=STATE.with(|s|{
        let state=s.borrow();
        let bar=state.bars.iter().find(|b|b.hwnd==owner&&!b.hidden)?;
        let layout=bar.layout.as_ref()?;
        let rect=match target{shell_access::Target::Start=>layout.start,shell_access::Target::Tray=>layout.tray,shell_access::Target::Clock=>layout.clock};
        Some(layout.popup_anchor(rect,false,bar.reveal.value).point)
    });
    slot.map(|point|panel_chrome::Anchor::for_owner(owner,point))
}
fn start_anchor_for(state:&State,bar:&Bar,screen:&Display)->crate::launcher::DockAnchor{
    if let Some(layout)=&bar.layout{return layout.start_anchor(bar.reveal.value);}
    let display=state.appbars.dock_placement(screen,state.prefs.appearance.position);
    geometry::measure_cluster(&display,&state.prefs.appearance,bar.items.len(),None,0.0,
        &bar.pinned_widgets,state.prefs.utility_cluster(&screen.id)).start_anchor(bar.reveal.value)
}
fn dock_start_anchor(owner:HWND)->Option<crate::launcher::DockAnchor>{
    STATE.with(|s|{
        let s=s.borrow();
        let bar=s.bars.iter().find(|b|b.hwnd==owner&&!b.hidden)?;
        let screen=s.screens.iter().find(|d|same_rect(d.bounds,bar.bounds))?;
        Some(start_anchor_for(&s,bar,screen))
    })
}
unsafe fn open_dock_launcher(anchor:crate::launcher::DockAnchor){
    utilities::close_panels();
    UTILITY_ANCHOR.with(|a|a.set(None));
    let (w,l)=anchor.encode();
    PostMessageW(OWNER.with(Cell::get),crate::WM_OPEN_DOCK_LAUNCHER,w,l);
}
unsafe fn activate_shell(owner:HWND,target:shell_access::Target,context:bool){
    let anchor=shell_anchor(owner,target);
    use shell_access::Route;
    // Native surfaces always require explicit opt-in and are never called
    // during takeover or while the watchdog is hiding the taskbar.
    let local=local_shell_buttons();
    crate::log!("DOCK_SHELL button={target:?} local={local} taskbar_hidden={}",taskbar_hidden());
    match shell_access::route(local,target,context){
        Route::Native=>shell_access::activate(target,context,anchor,owner,CONTROL.with(Cell::get)),
        Route::Launcher=>if let Some(anchor)=dock_start_anchor(owner){open_dock_launcher(anchor);},
        Route::Calendar=>if let Some(anchor)=anchor{
            UTILITY_ANCHOR.with(|a|a.set(None));
            calendar::show(owner,anchor);
        },
        Route::Hub=>open_dock_hub(owner,taskbar_active()),
    }
}
fn material_dark()->bool{
    STATE.with(|s|s.borrow().material.is_some_and(|m|m.dark))
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
        if item.path!=internal_app::KEY{
            AppendMenuW(menu,MF_STRING,MENU_APP_ICON,wide("Đổi icon ứng dụng…").as_ptr());
            if crate::app_icons::has(&prefs.app_icons,&item.path){
                AppendMenuW(menu,MF_STRING,MENU_APP_ICON_RESET,wide("Khôi phục icon mặc định").as_ptr());
            }
        }
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
    AppendMenuW(menu,MF_STRING,MENU_ADD_APP,wide("Ghim ứng dụng hoặc tệp .lnk…").as_ptr());
    if selected.is_none(){
        AppendMenuW(menu,MF_STRING,MENU_TASK_MANAGER,wide("Mở Trình quản lý tác vụ").as_ptr());
    }
    let extra=STATE.with(|s|{
        let state=s.borrow();
        state.bars.get(bar_index).is_some_and(|b|available_items(&state.windows,b.bounds,&prefs).len()>MAX_ITEMS)
    });
    if extra{AppendMenuW(menu,MF_STRING,MENU_MORE_APPS,wide("Ứng dụng khác…").as_ptr());}
    AppendMenuW(menu,MF_SEPARATOR,0,ptr::null());
    for (id,title) in [(MENU_WIFI,"Cài đặt Wi-Fi"),(MENU_BLUETOOTH,"Bluetooth"),(MENU_SOUND,"Âm thanh"),
      (MENU_NOTIFICATIONS,"Thông báo"),(MENU_EXPLORER,"Mở File Explorer")] {
        AppendMenuW(menu,MF_STRING,id,wide(title).as_ptr());
    }
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
    let cmd=glass_menu::track(menu,hwnd,point,TPM_RETURNCMD|TPM_RIGHTBUTTON,material_dark(),prefs.appearance.transparency) as usize;
    DestroyMenu(menu);
    match cmd {
        MENU_TASK_MANAGER=>open_task_manager(),
        MENU_ADD_APP=>pin_from_file(hwnd),
        MENU_MORE_APPS=>more_apps(hwnd),
        MENU_APP_ICON|MENU_APP_ICON_RESET=>if let Some(item)=selected.as_ref(){change_app_icon(hwnd,&item.path,cmd==MENU_APP_ICON_RESET);},
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
        MENU_PIN|MENU_UNPIN => if let Some(item)=selected {
            emit(Event::Edit(if cmd==MENU_PIN{Edit::Pin(item.path)}else{Edit::Unpin(item.path)}));
        },
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
        _=>{}
    }
}
unsafe fn window_owner(hwnd:HWND)->Option<(u32,u32)>{
    let mut process=0;
    let thread=GetWindowThreadProcessId(hwnd,&mut process);
    (process!=0&&thread!=0&&process!=std::process::id()).then_some((process,thread))
}
unsafe fn stage_window_owner(hwnd:HWND)->Option<(u32,u32)>{
    if let Some(owner)=window_owner(hwnd){return Some(owner);}
    if !internal_app::owns(hwnd){return None;}
    let mut pid=0;
    let tid=GetWindowThreadProcessId(hwnd,&mut pid);
    (pid==std::process::id()&&tid!=0&&internal_app::current()==hwnd).then_some((pid,tid))
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
fn finish_stage_snapshot(){
    if stage_snapshot::complete(){queue_refresh();}
}
unsafe fn stage_menu(hwnd:HWND,x:i32,y:i32){
    let card=STATE.with(|s|{
        let state=s.borrow();
        let stage=state.stages.iter().find(|p|p.hwnd==hwnd)?;
        let layout=stage.layout?;
        let i=stage.hit(layout,x,y)?;
        let member=if layout.stack_design{stage.member_hit(layout,i,x,y)?}else{0};
        let item=*stage.members.get(i)?.get(member)? as HWND;
        let settings=internal_app::owns(item);
        let local=!settings&&stage_window_is_local(item,stage.bounds);
        let group=stage.groups.get(i).copied().flatten();
        let members=group.map(|id|state.workspaces.members(id))
            .unwrap_or_else(||stage.members[i].iter().map(|&h|h as HWND).collect()).into_iter()
            .filter_map(|hwnd|{
                let identity=WindowIdentity::read(hwnd)?;
                state.windows.iter().find(|w|w.hwnd==hwnd).map(|w|(identity,w.title.clone()))
            })
            .collect::<Vec<_>>();
        let targets=state.screens.iter().filter(|s|!settings&&!stage_window_is_local(item,s.bounds)).map(|s|s.id.clone()).collect::<Vec<_>>();
        Some((WindowIdentity::read_stage(item)?,group,stage.display.clone(),stage.selected,local,state.workspaces.can_create(&stage.display),members,targets,state.workspaces.is_separate(item)))
    });
    let Some((identity,group,display,current,local,can_create,members,targets,separate))=card else{return;};
    let item=identity.hwnd as HWND;
    let menu=CreatePopupMenu();
    if menu.is_null(){return;}
    const FOCUS:usize=6;
    const MINIMIZE:usize=7;
    const CLOSE:usize=8;
    const CLOSE_GROUP:usize=9;
    const ACTIVATE_GROUP:usize=10;
    const SEPARATE:usize=11;
    const REJOIN:usize=12;
    const DISSOLVE_AUTO:usize=13;
    const MOVE_FIRST:usize=32;
    const DETACH_FIRST:usize=256;
    const SELECT_FIRST:usize=512;
    let append=|target,id,title:&str,active|{
        AppendMenuW(target,MF_STRING|if active{0}else{MF_GRAYED},id,wide(title).as_ptr());
    };
    let add=|id,title:&str,active|{
        append(menu,id,title,active);
    };
    add(FOCUS,if IsIconic(item)!=0{"Khôi phục cửa sổ"}else{"Đưa cửa sổ lên trước"},true);
    add(MINIMIZE,"Thu nhỏ cửa sổ",IsIconic(item)==0);
    add(CLOSE,"Đóng cửa sổ",true);
    const HEIGHT_FIRST:usize=16;
    let heights=[0,72,90,108,126,144,162,180];
    let card_key=STATE.with(|s|{
        let s=s.borrow();let stage=s.stages.iter().find(|p|p.hwnd==hwnd)?;
        let i=stage.members.iter().position(|members|members.contains(&identity.hwnd))?;
        let key=stage.keys.get(i)?.clone();
        let height=stage.session_heights.get(&key).or_else(||s.prefs.stage_card_overrides.get(&key)).copied().unwrap_or(0);
        Some((key,height))
    });
    if let Some((key,current))=&card_key{
        let sizing=CreatePopupMenu();
        if !sizing.is_null(){
            for (index,&height) in heights.iter().enumerate(){
                let name=if height==0{"Dùng chiều cao mặc định".into()}else{format!("{height} px")};
                AppendMenuW(sizing,MF_STRING|if height==*current{MF_CHECKED}else{0},HEIGHT_FIRST+index,wide(&name).as_ptr());
            }
            let name=if Preferences::stable_stage_key(key){"Chiều cao thẻ này"}else{"Chiều cao thẻ này (phiên hiện tại)"};
            if AppendMenuW(menu,MF_POPUP,sizing as usize,wide(name).as_ptr())==0{DestroyMenu(sizing);}
        }
    }
    if !targets.is_empty()&&IsIconic(item)==0&&IsZoomed(item)==0{
        let submenu=CreatePopupMenu();
        if !submenu.is_null(){
            for (index,target) in targets.iter().take(DETACH_FIRST-MOVE_FIRST).enumerate(){
                append(submenu,MOVE_FIRST+index,&target.replace('&',"&&"),true);
            }
            if AppendMenuW(menu,MF_POPUP,submenu as usize,wide("Chuyển sang màn hình").as_ptr())==0{DestroyMenu(submenu);}
        }
    }
    AppendMenuW(menu,MF_SEPARATOR,0,ptr::null());
    add(1,"Tạo nhóm từ cửa sổ này",local&&group.is_none()&&can_create);
    add(2,"Thêm vào nhóm đang mở",local&&group.is_none()&&current.is_some());
    for (base,label) in [(SELECT_FIRST,"Tất cả cửa sổ trong nhóm"),(DETACH_FIRST,"Tách cửa sổ khỏi nhóm thủ công")] {
        if members.len()<2||base==DETACH_FIRST&&group.is_none(){continue;}
        let submenu=CreatePopupMenu();
        if !submenu.is_null(){
            for (index,(_,title)) in members.iter().enumerate(){
                if base==DETACH_FIRST&&index>=SELECT_FIRST-DETACH_FIRST{break;}
                let label=title.replace('&',"&&").chars().take(90).collect::<String>();
                append(submenu,base+index,&label,true);
            }
            if AppendMenuW(menu,MF_POPUP,submenu as usize,wide(label).as_ptr())==0{DestroyMenu(submenu);}
        }
    }
    if members.len()>1||group.is_some(){add(ACTIVATE_GROUP,"Kích hoạt nhóm",true);}
    if members.len()>1{add(CLOSE_GROUP,"Đóng các cửa sổ trong nhóm…",true);}
    if group.is_some(){add(4,"Giải tán nhóm và khôi phục cửa sổ",true);}
    else if members.len()>1{
        add(SEPARATE,"Tách cửa sổ này khỏi nhóm tự động",true);
        add(DISSOLVE_AUTO,"Giải tán nhóm tự động trong phiên này",true);
    }else if separate{add(REJOIN,"Cho phép tự động gom lại",true);}
    AppendMenuW(menu,MF_SEPARATOR,0,ptr::null());
    add(5,"Điều hướng Stage bằng bàn phím",true);
    let mut point=POINT{x,y};ClientToScreen(hwnd,&mut point);
    // A no-activate tool window needs a temporary foreground owner while its
    // native menu is open, so Escape and clicks outside dismiss the menu.
    let previous=GetForegroundWindow();
    SetForegroundWindow(hwnd);
    let result=glass_menu::track(menu,hwnd,point,TPM_RETURNCMD|TPM_NONOTIFY,material_dark(),36) as usize;
    DestroyMenu(menu);
    if GetForegroundWindow()==hwnd && IsWindow(previous)!=0 {SetForegroundWindow(previous);}
    if result==5{
        let selected=STATE.with(|s|{
            let mut state=s.borrow_mut();
            let Some(p)=state.stages.iter_mut().find(|p|p.hwnd==hwnd&&!p.hidden)else{return false;};
            let Some(i)=p.members.iter().position(|members|members.contains(&identity.hwnd))else{return false;};
            p.keyboard=Some(i);p.previous_focus=previous as usize;true
        });
        if selected{SetForegroundWindow(hwnd);SetFocus(hwnd);InvalidateRect(hwnd,ptr::null(),0);}
        return;
    }
    if result==0{return;}
    if let Some((key,height))=card_key.and_then(|(key,_)|result.checked_sub(HEIGHT_FIRST)
            .and_then(|i|heights.get(i)).map(|&h|(key,h))){
        if Preferences::stable_stage_key(&key){
            emit(Event::Edit(Edit::StageGroupHeight(key,height)));
        }else{
            update_state(|s|{
                if let Some(stage)=s.stages.iter_mut().find(|p|p.hwnd==hwnd&&p.keys.contains(&key)){
                    if height==0{stage.session_heights.remove(&key);}
                    else if stage.session_heights.len()<128||stage.session_heights.contains_key(&key){stage.session_heights.insert(key,height);}
                }
                refresh(s,false);
            });
        }
        return;
    }
    if result==CLOSE_GROUP&&MessageBoxW(hwnd,
        wide(&format!("Gửi yêu cầu đóng tới {} cửa sổ trong nhóm?",members.len())).as_ptr(),
        wide("Đóng nhóm Stage Manager").as_ptr(),MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2)!=IDYES{return;}
    let card_index=STATE.with(|s|{
        let state=s.borrow();
        let stage=state.stages.iter().find(|stage|stage.hwnd==hwnd&&stage.display==display&&!stage.hidden)?;
        stage.members.iter().enumerate().find(|(i,m)|stage.groups[*i]==group&&m.contains(&identity.hwnd)&&identity.matches_stage())
            .map(|(i,_)|i)
    });
    let Some(index)=card_index else{return;};
    let valid_member=|target:WindowIdentity|STATE.with(|s|{
        let state=s.borrow();
        let Some(stage)=state.stages.iter().find(|p|p.hwnd==hwnd&&!p.hidden)else{return false;};
        target.matches(target.hwnd as HWND)&&stage.members.get(index).is_some_and(|m|m.contains(&target.hwnd))
    });
    match result{
        FOCUS=>{activate_stage_member(hwnd,index,Some(item));return;}
        MINIMIZE=>{ShowWindowAsync(item,SW_MINIMIZE);return;}
        CLOSE=>{PostMessageW(item,WM_CLOSE,0,0);return;}
        CLOSE_GROUP=>{
            for (target,_) in members{if valid_member(target){PostMessageW(target.hwnd as HWND,WM_CLOSE,0,0);}}
            return;
        }
        ACTIVATE_GROUP=>{activate_stage(hwnd,index);return;}
        n if n>=SELECT_FIRST=>{
            if let Some((target,_))=members.get(n-SELECT_FIRST){
                let target=*target;
                if valid_member(target){activate_stage_member(hwnd,index,Some(target.hwnd as HWND));}
            }
            return;
        }
        n if (MOVE_FIRST..DETACH_FIRST).contains(&n)=>{
            if let Some(target)=targets.get(n-MOVE_FIRST){move_normal_window(item,target);}
            return;
        }
        _=>{}
    }
    let dissolve=if result==DISSOLVE_AUTO{
        members.iter().map(|(target,_)|*target).filter(|&target|valid_member(target)).collect::<Vec<_>>()
    }else{Vec::new()};
    update_state(|state|{
        // The menu can run a nested loop. Revalidate the original source and
        // display against the current enumeration before changing a workspace.
        let Some(stage)=state.stages.iter().find(|stage|stage.hwnd==hwnd&&stage.display==display)else{return;};
        let current_card=if let Some(id)=group {
            stage.groups.contains(&Some(id))
        }else{
            stage.members.iter().enumerate()
                .any(|(i,entries)|entries.contains(&identity.hwnd)&&stage.groups[i].is_none())
        };
        if !current_card{return;}
        let present=state.windows.iter().any(|w|w.hwnd==item);
        let still_local=present && stage_window_is_local(item,stage.bounds);
        match result{
            1 if still_local&&group.is_none()=>{state.workspaces.create(&display,item);}
            2 if still_local&&group.is_none()&&current==stage.selected=>state.workspaces.add(&display,item),
            4=>{if let Some(id)=group{state.workspaces.dissolve(id);}},
            SEPARATE if group.is_none()=>state.workspaces.separate(item),
            REJOIN if group.is_none()=>state.workspaces.rejoin(item),
            DISSOLVE_AUTO if group.is_none()=>for target in dissolve{state.workspaces.separate(target.hwnd as HWND);},
            id if id>=DETACH_FIRST=>{
                if let Some((group,(target,_)))=group.zip(members.get(id-DETACH_FIRST)){
                    let handle=target.hwnd as HWND;
                    if target.matches(handle){state.workspaces.remove_from(group,handle);}
                }
            }
            _=>{}
        }
        refresh(state,false);
    });
}
fn cycle_group(members:&[WindowIdentity],current:WindowIdentity,forward:bool)->Option<WindowIdentity>{
    let mut ordered=members.to_vec();
    ordered.sort_unstable_by_key(|id|(id.hwnd,id.pid,id.tid));
    ordered.dedup();
    if ordered.len()<2{return None;}
    let i=ordered.iter().position(|id|*id==current)?;
    let step=if forward{1}else{ordered.len()-1};
    Some(ordered[(i+step)%ordered.len()])
}
unsafe fn cycle_stage_group(hwnd:HWND,front:WindowIdentity,forward:bool){
    update_state(|s|{
        let reduced=s.prefs.appearance.motion_reduced();
        let changed=s.stages.iter_mut().find(|stage|stage.hwnd==hwnd&&!stage.hidden).is_some_and(|stage|{
            let Some(i)=stage.items.iter().position(|item|item.hwnd as usize==front.hwnd)else{return false;};
            let live=stage.members[i].iter().filter_map(|&h|WindowIdentity::read_stage(h as HWND)).collect::<Vec<_>>();
            let Some(next)=cycle_group(&live,front,forward)else{return false;};
            stage.representatives.insert(stage.keys[i].clone(),next);
            stage.finish_group_switch();
            if !reduced&&SetTimer(hwnd,STAGE_MOTION_TIMER,16,None)!=0{
                stage.group_switch=Some(GroupSwitch{source:next.hwnd,started:motion::Span::new(GROUP_SWITCH_MS),ink:GROUP_SWITCH_INK});
            }
            true
        });
        if changed{refresh(s,false);}
    });
}
unsafe fn activate_stage(hwnd:HWND,i:usize){
    let selected=STATE.with(|s|{
        let state=s.borrow();
        let stage=state.stages.iter().find(|stage|stage.hwnd==hwnd)?;
        let id=stage.representatives.get(stage.keys.get(i)?)?;
        (id.hwnd==stage.items.get(i)?.hwnd as usize&&id.matches_stage()).then_some(id.hwnd as HWND)
    });
    activate_stage_member(hwnd,i,selected);
}
unsafe fn stage_picker(hwnd:HWND,i:usize){
    use control_center::{Card,Command,View};
    let Some((mut point,side,cards))=STATE.with(|s|{
        let s=s.borrow();let stage=s.stages.iter().find(|p|p.hwnd==hwnd&&!p.hidden)?;
        let layout=stage.layout?;
        let r=layout.card(i);
        let members=stage.members.get(i)?;
        if members.len()<2{return None;}
        let foreground=GetForegroundWindow();
        let cards=members.iter().filter_map(|&handle|{
            let w=s.windows.iter().find(|w|w.hwnd as usize==handle)?;
            let identity=WindowIdentity::read(w.hwnd)?;
            let status=if foreground==w.hwnd{"Đang dùng"}else if IsIconic(w.hwnd)!=0{"Đã thu nhỏ"}else{"Đang mở"};
            Some(Card{icon:Some(UtilityWidget::Windows),title:if w.title.trim().is_empty(){basename(&w.path)}else{w.title.clone()},
                hint:status.into(),action:Some(Command::StageMember(identity))})
        }).collect::<Vec<_>>();
        let x=if layout.right{r.x}else{r.x+r.w};
        Some((POINT{x,y:r.y+r.h/2},if layout.right{DockSide::Right}else{DockSide::Left},cards))
    })else{return;};
    if cards.is_empty()||ClientToScreen(hwnd,&mut point)==0{return;}
    let anchor=panel_chrome::Anchor{side,..panel_chrome::Anchor::for_owner(hwnd,point)};
    control_center::show_list(hwnd,anchor,View{title:"Chọn cửa sổ trong nhóm".into(),
        hint:format!("{} cửa sổ · Chọn một cửa sổ để đưa lên trước",cards.len()),cards,metrics:None});
}
unsafe fn focus_stage_window(hwnd:HWND){
    window_effect::cancel();
    if internal_app::owns(hwnd){
        let owner=OWNER.with(Cell::get);
        if !owner.is_null(){PostMessageW(owner,crate::WM_OPEN_SETTINGS,0,0);}
        return;
    }
    if window_owner(hwnd).is_none(){return;}
    if IsIconic(hwnd)!=0{ShowWindowAsync(hwnd,SW_RESTORE);}
    SetForegroundWindow(hwnd);
}
unsafe fn activate_stage_member(hwnd:HWND,i:usize,member:Option<HWND>){
    let selected=STATE.with(|s|{
        let state=s.borrow();
        let stage=state.stages.iter().find(|p|p.hwnd==hwnd)?;
        if member.is_some_and(|h|!stage.members.get(i).is_some_and(|members|members.contains(&(h as usize)))){return None;}
        let members=if member.is_none()&&stage.layout.is_some_and(|layout|layout.stack_design)&&stage.groups.get(i)==Some(&None){
            stage.members.get(i)?.iter().copied().filter_map(|h|{
                let target=h as HWND;
                stage_window_is_local(target,stage.bounds).then(||WindowIdentity::read(target)).flatten()
            }).collect()
        }else{Vec::new()};
        Some((stage.items.get(i)?.clone(),stage.groups.get(i).copied().flatten(),stage.display.clone(),members))
    });
    if let Some((item,group,display,members))=selected {
        if let Some(id)=group{
            let mut focus=None;
            update_state(|state|{
                focus=state.workspaces.switch(&display,id);
                let reduced=state.prefs.appearance.motion_reduced();
                if let Some(p)=state.stages.iter_mut().find(|p|p.hwnd==hwnd&&p.selected!=Some(id)){p.begin_slide(reduced);}
                refresh(state,false);
            });
            if let Some(hwnd)=member{focus_stage_window(hwnd);}
            else if let Some(hwnd)=focus{SetForegroundWindow(hwnd);}
        }else{
            // Group activation raises only local members in existing stacking
            // order; it never minimizes or moves unrelated windows.
            for entry in members.into_iter().rev(){
                let target=entry.hwnd as HWND;
                if target==item.hwnd||!entry.matches(target){continue;}
                if IsIconic(target)!=0{ShowWindowAsync(target,SW_SHOWNOACTIVATE);}
                SetWindowPos(target,HWND_TOP,0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE|SWP_ASYNCWINDOWPOS);
            }
            focus_stage_window(member.unwrap_or(item.hwnd));
        }
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
        let reduced=state.prefs.appearance.motion_reduced();
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
        if taskbar_active(){show_taskbar(taskbar_guard::Exit::ExplorerChanged,taskbar_guard::Exit::ExplorerChanged.message());}
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
        WM_HOTKEY if hwnd==CONTROL.with(Cell::get)&&w==START_HOTKEY as usize=>{
            if taskbar_hidden()&&START_BOUND.with(Cell::get){start_from_pointer(win_key::cursor_now());}
            0
        },
        win_key::NOTICE if hwnd==CONTROL.get()=>{
            let current=WIN_KEY.with(|s|s.borrow().as_ref().is_some_and(|hook|hook.ticket==w));
            if current&&taskbar_hidden(){
                if let Ok(cursor)=win_key::decode_tap(l){
                    if crate::trial::active(){crate::log!("DOCK_WIN_KEY standalone routed to TLauncher Start");}
                    start_from_pointer(cursor);
                }
                else{
                    WIN_KEY_FAILED.set(true);
                    WIN_KEY.with(|s|drop(s.borrow_mut().take()));
                    win_key_status("maskFailed");
                }
            }
            0
        },
        stage_snapshot::READY if hwnd==CONTROL.get()=>{window_effect::capture_ready();finish_stage_snapshot();0},
        shell_menu::NOTICE if hwnd==CONTROL.get()=>{
            let enabled=STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&s.prefs.shell_context_menu&&!s.suspended});
            let current=WIN_KEY.with(|s|s.borrow().as_ref().is_some_and(|hook|hook.ticket==w));
            let owner=STATE.with(|s|s.borrow().bars.iter().find(|b|!b.hidden).map_or(hwnd,|b|b.hwnd));
            shell_menu::receive(owner,w,enabled&&current);0
        },
        WM_CLIPBOARDUPDATE if hwnd==CONTROL.get()=>{clipboard_history::changed(hwnd);0},
        clipboard_history::READY if hwnd==CONTROL.get()=>{clipboard_history::complete(hwnd);0},
        WM_CONFIG=>{drain_commands();0},
        WM_INTERNAL_APP if hwnd==CONTROL.with(Cell::get)=>{queue_refresh();0},
        WM_TICK=>{SetTimer(hwnd,2,60,None);0},
        WM_UTILITY_READY=>{show_utilities();0},
        WM_TIMER if w==jump_list::TIMER&&hwnd==CONTROL.get()=>{jump_list::poll();0},
        WM_TIMER if w==folder_stack::TIMER&&hwnd==CONTROL.get()=>{folder_stack::poll();0},
        WM_TIMER if w==app_previews::TIMER&&hwnd==CONTROL.get()=>{app_previews::poll();0},
        WM_TIMER if w==comfort::TIMER&&hwnd==CONTROL.get()=>{comfort::poll();0},
        WM_TIMER if w==recycle_bin::TIMER&&hwnd==CONTROL.get()=>{recycle_bin::poll();0},
        WM_TIMER if w==motion_guard::TIMER&&hwnd==CONTROL.get()=>{motion_guard::sync();0},
        WM_SHELL_FALLBACK if hwnd==CONTROL.with(Cell::get)=>{
            let owner=w as HWND;
            if !local_shell_buttons()&&!control_center::open_for(owner){
                if l==0{
                    if let Some(anchor)=dock_start_anchor(owner){open_dock_launcher(anchor);}
                }else{
                    open_dock_hub(owner,taskbar_active());
                }
            }
            0
        },
        WM_ICON_CHANGED=>{
            let changed=ATLAS.with(|a|a.borrow_mut().as_mut().is_some_and(|worker|worker.receive()));
            if changed{for h in STATE.with(|s|{let state=s.borrow();state.bars.iter().filter(|b|!b.hidden).map(|b|b.hwnd)
                .chain(state.stages.iter().filter(|p|!p.hidden).map(|p|p.hwnd)).collect::<Vec<_>>()}){
                InvalidateRect(h,ptr::null(),0);
            }}
            0
        },
        WM_NCDESTROY=>{
            recycle_bin::unregister(hwnd);
            app_previews::close_for(hwnd);
            stage_peek::close(hwnd);
            cancel_deferred_click(hwnd);
            APP_CLICK.with(|p|{if p.borrow().as_ref().is_some_and(|c|c.owner==hwnd as usize){p.borrow_mut().take();}});
            close_status_tip(hwnd);
            perf::forget(hwnd as usize);
            STAGE_SURFACES.with(|s|s.borrow_mut().remove(&(hwnd as usize)));
            SURFACES.with(|s|s.borrow_mut().remove(&(hwnd as usize)));
            DefWindowProcW(hwnd,msg,w,l)
        },
        WM_DOCK_MOUSELEAVE=>{
            app_previews::leave(hwnd);
            KillTimer(hwnd,stage_peek::TIMER);stage_peek::close(hwnd);
            KillTimer(hwnd,STATUS_TIP_TIMER);close_status_tip(hwnd);
            let stage=STATE.with(|s|{
                let mut state=s.borrow_mut();
                let reduced=state.prefs.appearance.motion_reduced();
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
                let animate=!state.prefs.appearance.motion_reduced();
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
            if tracked.0{retarget_start(hwnd);InvalidateRect(hwnd,ptr::null(),0);}
            0
        },
        WM_TIMER if w==window_effect::RELEASE_TIMER&&hwnd==CONTROL.with(Cell::get)=>{window_effect::release_factory();0},
        WM_TIMER if w==window_effect::TIMER&&hwnd==CONTROL.with(Cell::get)=>{
            window_effect::timer();0
        }
        WM_TIMER if w==stage_peek::TIMER=>{stage_peek::show(hwnd);0},
        WM_TIMER if w==STAGE_MOTION_TIMER=>{
            let step=STATE.with(|s|{
                let mut state=s.borrow_mut();
                let reduced=state.prefs.appearance.motion_reduced();
                let p=state.stages.iter_mut().find(|p|p.hwnd==hwnd&&!p.hidden)?;
                let progress=|span:motion::Span|if reduced{1.0}else{span.progress()};
                let emphasis=p.emphasis.start.map(|s|p.emphasis.tick(p.hover,progress(s)));
                let mut next_position=None;
                if let Some((start,from))=p.slide {
                    let t=progress(start);
                    let offset=(from as f32*(1.0-t).powi(3)).round() as i32;
                    if offset!=p.offset{
                        p.offset=offset;
                        next_position=Some((p.preview_hwnd,p.resting,offset));
                    }
                    if t>=1.0{p.slide=None;}
                }
                let swap_redraw=if let Some(swap)=p.group_switch.as_mut() {
                    let ink=group_switch_ink(progress(swap.started));
                    swap.ink=ink;
                    p.previews.set_opacity(swap.source,ink);
                    if ink==255{p.group_switch=None;}
                    true
                }else{false};
                Some((emphasis.is_some()||swap_redraw,emphasis==Some(true)||p.slide.is_some()||p.group_switch.is_some(),next_position))
            });
            if let Some((redraw,again,position))=step{
                if let Some((preview,rect,offset))=position{position_stage(hwnd,preview,rect,offset);}
                if redraw{InvalidateRect(hwnd,ptr::null(),0);}
                if !again{KillTimer(hwnd,STAGE_MOTION_TIMER);}
            }else{KillTimer(hwnd,STAGE_MOTION_TIMER);}
            0
        },
        WM_TIMER if w==STATUS_TIP_TIMER=>{KillTimer(hwnd,STATUS_TIP_TIMER);show_status_tip(hwnd);0},
        WM_TIMER if w==APP_CLICK_TIMER=>{
            let click=DEFERRED_CLICK.with(|p|{
                let mut pending=p.borrow_mut();
                if pending.as_ref().is_some_and(|(owner,_)|*owner==hwnd as usize){
                    pending.take().map(|(_,click)|click)
                }else{None}
            });
            KillTimer(hwnd,APP_CLICK_TIMER);
            if let Some(click)=click{run_deferred_click(hwnd,click);}
            0
        }
        WM_TIMER if w==TASKBAR_GUARD_TIMER=>{poll_taskbar(hwnd);0},
        WM_TIMER if w==UTILITY_RETRY_TIMER=>{retry_utility(hwnd);0},
        WM_TIMER if w==FEEDBACK_TIMER=>{
            KillTimer(hwnd,FEEDBACK_TIMER);
            STATE.with(|s|{if let Some(bar)=s.borrow_mut().bars.iter_mut().find(|b|b.hwnd==hwnd){bar.feedback=None;}});
            InvalidateRect(hwnd,ptr::null(),0);0
        },
        WM_TIMER if w==HIDE_TIMER=>{
            KillTimer(hwnd,HIDE_TIMER);
            let stage=STATE.with(|state|state.borrow().stages.iter().any(|p|p.hwnd==hwnd&&p.auto_hide&&p.keyboard.is_none()));
            if stage{
                if utilities::panel_open_for(hwnd){return 0;}
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
                let reduced=state.prefs.appearance.motion_reduced();
                state.bars.iter_mut().find(|b|b.hwnd==hwnd&&b.auto_hide&&b.layout.is_some()&&b.keyboard.is_none())
                    .is_some_and(|b|b.collapse(true,reduced))
            });
            if start{SetTimer(hwnd,MOTION_TIMER,16,None);}
            queue_refresh();
            InvalidateRect(hwnd,ptr::null(),0);
            0
        },
        WM_TIMER if w==RAIL_FADE_TIMER=>{
            let step=STATE.with(|s|{
                let mut s=s.borrow_mut();
                let reduced=s.prefs.appearance.motion_reduced();
                let b=s.bars.iter_mut().find(|b|b.hwnd==hwnd&&b.collapsed&&!b.hidden)?;
                let first=std::mem::take(&mut b.rail.waiting);
                if first{b.rail.started=Some(motion::Span::new(RAIL_FADE_MS));}
                let progress=b.rail.started?.progress();
                b.rail.opacity=rail_opacity(Duration::from_secs_f32(progress*RAIL_FADE_MS as f32/1000.0),reduced);
                let done=b.rail.opacity<=RAIL_DIM;
                if done{b.rail.started=None;}
                Some((first,done))
            });
            if let Some((first,done))=step{
                if done{KillTimer(hwnd,RAIL_FADE_TIMER);}
                else if first&&SetTimer(hwnd,RAIL_FADE_TIMER,16,None)==0{
                    KillTimer(hwnd,RAIL_FADE_TIMER);
                    STATE.with(|s|if let Some(b)=s.borrow_mut().bars.iter_mut().find(|b|b.hwnd==hwnd){
                        b.rail.started=None;b.rail.opacity=RAIL_DIM;
                    });
                }
                InvalidateRect(hwnd,ptr::null(),0);
            }else{KillTimer(hwnd,RAIL_FADE_TIMER);}
            0
        }
        WM_TIMER if w==MOTION_TIMER=>{
            let motion=STATE.with(|s|{
                let mut state=s.borrow_mut();
                let State{screens,prefs,bars,appbars,..}=&mut *state;
                let b=bars.iter_mut().find(|b|b.hwnd==hwnd)?;
                if b.hidden || b.layout.is_none(){return None;}
                let display=screens.iter().find(|d|same_rect(d.bounds,b.bounds))?;
                let now=std::time::Instant::now();
                if let Some(resize)=&b.motion.resize{
                    let t=if prefs.appearance.motion_reduced()||b.collapsed{1.0}else{resize.progress()};
                    let next=resize.frame(t);
                    let r=next.window;
                    let reposition=b.layout.as_ref().is_some_and(|old|!same_rect(old.window,r));
                    b.replace_layout(next);
                    if reposition{SetWindowPos(hwnd,HWND_TOPMOST,r.left,r.top,r.right-r.left,r.bottom-r.top,SWP_NOACTIVATE);}
                    if t<1.0{return Some(true);}
                    b.motion.resize=None;b.motion.at=Some(now);
                    b.motion.target=if b.collapsed||prefs.appearance.motion_reduced()||prefs.appearance.magnification==0{None}else{
                        b.pointer.and_then(|(x,y)|b.layout.as_ref()?.magnification_target(x,y))
                    };
                    // Continue any freshly requested reveal/hover on the next
                    // frame. Hit testing always uses the displayed layout.
                    let again=b.collapsed||b.motion.target.is_some()||b.feedback.is_some();
                    b.motion.scheduled=again;
                    if !again{b.motion.at=None;}
                    return Some(again);
                }
                let old_zoom=b.motion.zoom.value;
                let old_focus=b.motion.focus_value();
                let goal=if prefs.appearance.motion_reduced(){0.0}else{b.start_goal()};
                let mut again=if prefs.appearance.motion_reduced(){
                    b.motion=Motion::default();b.reveal=motion::Spring::rest(if b.collapsed{0.0}else{1.0});false
                }else{b.motion.tick(now,motion::settings().speed,&mut b.reveal,!b.collapsed,goal)};
                if old_zoom!=b.motion.zoom.value||old_focus!=b.motion.focus_value(){
                    let placement=appbars.dock_placement(display,prefs.appearance.position);
                    let layout=geometry::measure_cluster(&placement,&prefs.appearance,b.items.len(),b.motion.focus_value(),b.motion.zoom.value,&b.pinned_widgets,prefs.utility_cluster(&display.id));
                    b.hovered=b.pointer.and_then(|(x,y)|layout.hot(x,y)).map_or(-1,|i|i as i32);
                    b.layout=Some(layout);
                }
                if b.collapsed&&b.rail_face==0&&b.reveal.value<=0.24{
                    if let Some(layout)=&b.layout{
                        b.rail_face=stage_rail::sample(surface::rail_bounds(layout),layout.side,false,0);
                    }
                }
                if b.feedback.as_ref().is_some_and(|f|f.motion.progress()>=1.0||!motion::allowed()){b.feedback=None;}
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
        WM_TIMER if hwnd==CONTROL.get()&&w==STAGE_CAPTURE_TIMER=>{KillTimer(hwnd,STAGE_CAPTURE_TIMER);queue_refresh();0},
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
        WM_TIMER=>{if w==1||w==2{if w==2{KillTimer(hwnd,2);DIRTY.with(|d|d.set(false));}else{refresh_dock_clock();sync_win_key();request_dock_status();motion_guard::sync();}let changed = DISPLAY_DIRTY.with(|d|d.replace(false));update_state(|s|refresh(s,w==1 || changed));}0},
        WM_TIMECHANGE=>{refresh_dock_clock();0},
        WM_DISPLAYCHANGE|WM_SETTINGCHANGE|WM_THEMECHANGED|WM_DPICHANGED=>{
            app_previews::close_for(ptr::null_mut());
            window_effect::reset();
            stage_peek::close(ptr::null_mut());
            if msg==WM_SETTINGCHANGE{motion::refresh_system();}
            if msg==WM_DISPLAYCHANGE&&taskbar_active(){show_taskbar(taskbar_guard::Exit::ExplorerChanged,"Đã hiện lại thanh tác vụ khi thay đổi màn hình.");}
            DISPLAY_DIRTY.with(|d|d.set(true));queue_refresh();0
        },
        WM_MOUSEMOVE=>{
            let x=l as i16 as i32;
            let y=(l>>16) as i16 as i32;
            let stage=STATE.with(|s|{
                let mut state=s.borrow_mut();
                let reduced=state.prefs.appearance.motion_reduced();
                let p=state.stages.iter_mut().find(|p|p.hwnd==hwnd)?;
                let hit=p.layout.and_then(|layout|p.hit(layout,x,y));
                let old=std::mem::replace(&mut p.hover,hit);
                let changed=old!=hit;
                Some((changed,changed&&p.emphasis.change(hit,reduced),hit.is_some()))
            });
            if let Some((changed,animate,over_card))=stage{
                KillTimer(hwnd,HIDE_TIMER);
                if changed{
                    KillTimer(hwnd,STATUS_TIP_TIMER);close_status_tip(hwnd);
                    KillTimer(hwnd,stage_peek::TIMER);stage_peek::close(hwnd);
                    if over_card{
                        let prefs=STATE.with(|s|{
                            let state=s.borrow();state.prefs.stage_peek.then_some(state.prefs.stage_peek_delay)
                        });
                        if let Some(delay)=prefs{SetTimer(hwnd,stage_peek::TIMER,delay,None);}
                        else{SetTimer(hwnd,STATUS_TIP_TIMER,650,None);}
                    }
                }
                if animate{SetTimer(hwnd,STAGE_MOTION_TIMER,16,None);}
                let mut event=TRACKMOUSEEVENT{cbSize:mem::size_of::<TRACKMOUSEEVENT>() as u32,dwFlags:TME_LEAVE,hwndTrack:hwnd,dwHoverTime:0};
                TrackMouseEvent(&mut event);
                if changed{InvalidateRect(hwnd,ptr::null(),0);}
                return 0;
            }
            let (changed,animate,track)=STATE.with(|s|{
                let mut state=s.borrow_mut();
                let reduced=state.prefs.appearance.motion_reduced();
                let magnify=!reduced&&state.prefs.appearance.magnification!=0;
                let Some(b)=state.bars.iter_mut().find(|b|b.hwnd==hwnd) else{return (false,false,false)};
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
                    let desired=if magnify{layout.magnification_target(x as f32,y as f32)}else{None};
                    let changed=desired!=b.motion.target;
                    b.motion.target=desired;
                    let start=start_reveal||(changed&&b.schedule());
                    (reveal||prev!=b.hovered||prev_drop!=b.drop_target(),start,true)
                }else{
                    b.hovered=if x>=12&&(x-12)/ITEM_WIDTH<b.items.len() as i32{(x-12)/ITEM_WIDTH}else{-1};
                    (prev!=b.hovered,false,true)
                }
            });
            if track{
                KillTimer(hwnd,HIDE_TIMER);KillTimer(hwnd,RAIL_FADE_TIMER);
                let mut event=TRACKMOUSEEVENT{cbSize:mem::size_of::<TRACKMOUSEEVENT>() as u32,dwFlags:TME_LEAVE,hwndTrack:hwnd,dwHoverTime:0};
                TrackMouseEvent(&mut event);
                let change=STATE.with(|s|{
                    let mut s=s.borrow_mut();
                    let b=s.bars.iter_mut().find(|b|b.hwnd==hwnd)?;
                    let layout=b.layout.as_ref()?;
                    let status=if layout.is_launch_button(x as f32,y as f32){Some(Some(UtilityWidget::Spotlight))}
                        else if layout.is_utility_button(x as f32,y as f32)||b.shell_target(x as f32,y as f32).is_some(){Some(None)}
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
            if track{retarget_start(hwnd);app_previews::mouse(hwnd);}
            if changed{InvalidateRect(hwnd,ptr::null(),0);}
            0
        }
        WM_LBUTTONDOWN|WM_LBUTTONDBLCLK=>{
            app_previews::close_for(ptr::null_mut());
            KillTimer(hwnd,stage_peek::TIMER);stage_peek::close(ptr::null_mut());
            cancel_deferred_click(hwnd);
            KillTimer(hwnd,STATUS_TIP_TIMER);close_status_tip(hwnd);
            let (x,y)=(l as i16 as i32,(l>>16) as i16 as i32);
            let mut area:RECT=mem::zeroed();GetClientRect(hwnd,&mut area);
            let utility=utility_profile(hwnd).is_some_and(|p|p.enabled);
            let (capture,start)=STATE.with(|s|{
                let mut state=s.borrow_mut();
                if let Some(stage)=state.stages.iter_mut().find(|p|p.hwnd==hwnd){
                    stage.pressed=stage.target(x,y).map(|target|(target,msg==WM_LBUTTONDBLCLK));
                    return (false,false);
                }
                let Some(bar)=state.bars.iter_mut().find(|b|b.hwnd==hwnd&&!b.hidden&&!b.collapsed)else{return (false,false);};
                let target=if bar.layout.is_some(){bar.target(x as f32,y as f32)}
                    else{bar.fallback_target(x,y,area.right,utility)};
                let capture=target.as_ref().is_some_and(|target|matches!(target,DockTarget::App(path)
                    if bar.items.iter().any(|item|item.pinned&&item.path==*path)));
                let start=target==Some(DockTarget::Shell(shell_access::Target::Start));
                bar.pressed=target.map(|target|Press{double:msg==WM_LBUTTONDBLCLK,..Press::new(target,x,y)});
                (capture,start)
            });
            if !start{crate::launcher::start::dismiss_from_dock();}else{retarget_start(hwnd);}
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
            if redraw{retarget_start(hwnd);InvalidateRect(hwnd,ptr::null(),0);}
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
                    VK_SPACE=>stage_picker(hwnd,i),
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
                    let recycle=STATE.with(|s|s.borrow().bars.get(bar).and_then(|b|b.layout.as_ref())
                        .and_then(|l|cell.checked_sub(count+2).and_then(|i|l.inline.get(i))).is_some_and(|(w,_)|*w==UtilityWidget::RecycleBin));
                    if recycle{recycle_bin::perform(hwnd,recycle_bin::Action::Menu);return 0;}
                    let target=STATE.with(|s|s.borrow().bars.get(bar).and_then(|b|b.layout.as_ref())
                        .and_then(|l|cell.checked_sub(l.shell_index())).and_then(|i|shell_access::Target::ALL.get(i)).copied());
                    if let Some(target)=target{activate_shell(hwnd,target,target==shell_access::Target::Start);}
                    else if cell<count{dock_context(hwnd,bar,cell,false);}else{request_utilities(hwnd,None);}
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
                        current.checked_sub(bar.items.len()+2).and_then(|i|bar.layout.as_ref()?.inline.get(i)).map(|(w,_)|*w),
                        bar.layout.as_ref().and_then(|l|current.checked_sub(l.shell_index())).and_then(|i|shell_access::Target::ALL.get(i)).copied(),0)),
                    VK_ESCAPE=>return Some((None,false,false,None,None,bar.previous_focus)),
                    _=>return None,
                };
                bar.keyboard=Some(next);
                Some((None,false,false,None,None,0))
            });
            if let Some((item,search,utilities,widget,shell,previous))=action{
                if let Some(target)=shell{activate_shell(hwnd,target,false);}
                if let Some(item)=item{open_dock_item(hwnd,&item);}
                if search {open_launcher(hwnd);}
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
                let target=stage.target(px,y).filter(|target|pressed.is_some_and(|(pressed,_)|*target==pressed));
                let item=stage.layout.and_then(|l|stage.hit(l,px,y));
                let key=item.and_then(|i|stage.keys.get(i)).cloned();
                Some((target,item,key,pressed.is_some_and(|(_,double)|double)))
            });
            if let Some((target,item,key,double))=stage_hit {
                match target{
                    Some(StageTarget::Page(page))=>{go_to_stage_page(hwnd,page);}
                    Some(StageTarget::Cycle(identity,forward))=>{cycle_stage_group(hwnd,identity,forward);}
                    Some(StageTarget::Picker(identity))=>{if identity.matches_stage(){
                        if let Some(i)=item{stage_picker(hwnd,i);}
                    }}
                    Some(StageTarget::Close(target,pid,tid))=>{
                        if stage_window_owner(target as HWND)==Some((pid,tid)){PostMessageW(target as HWND,WM_CLOSE,0,0);}
                    }
                    Some(StageTarget::Window(target,pid,tid))=>{
                        if stage_window_owner(target as HWND)==Some((pid,tid)){
                            if let Some(second)=key.and_then(|key|app_click(hwnd,ClickKey::Stage(key),target as HWND,double)){
                                if finish_double_click(second).is_some(){queue_refresh();}
                                return 0;
                            }
                            let grouped=item.is_some_and(|i|STATE.with(|s|s.borrow().stages.iter().find(|p|p.hwnd==hwnd)
                                .is_some_and(|p|p.items.get(i).is_some_and(|item|item.count>1)||p.groups.get(i).is_some_and(Option::is_some))));
                            // Foreground changes reorder stack layers, so wait for the
                            // second click before a single click brings a member forward.
                            if grouped{defer_click(hwnd,DeferredClick::StageMember(WindowIdentity{hwnd:target,pid,tid}));}
                            else{focus_stage_window(target as HWND);}
                        }
                    }
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
                retarget_start(hwnd);
                InvalidateRect(hwnd,ptr::null(),0);
                if let (DockTarget::App(path),Some((anchor,after)))=(press.target,drop){
                    emit(Event::Edit(Edit::MovePin{path,anchor,after}));
                }
                if auto&&(px<0||y<0||px>=area.right||y>=area.bottom){SetTimer(hwnd,HIDE_TIMER,650,None);}
                match click{
                    Some(DockTarget::App(_))=>{if let Some(item)=item{
                        match app_click(hwnd,ClickKey::Dock(item.path.to_lowercase()),item.hwnd,press.double){
                            Some(second)=>{
                                utilities::close_panels_for(hwnd);
                                if let Some(kind)=finish_double_click(second){show_feedback(hwnd,&item,kind);queue_refresh();}
                            }
                            None if item.count>1=>defer_click(hwnd,DeferredClick::DockPicker(item.path)),
                            None=>activate(hwnd,&item),
                        }
                    }},
                    Some(DockTarget::Launcher)=>open_launcher(hwnd),
                    Some(DockTarget::Overflow)=>request_utilities(hwnd,None),
                    Some(DockTarget::Utility(widget))=>request_utilities(hwnd,Some(widget)),
                    Some(DockTarget::Shell(target))=>activate_shell(hwnd,target,false),
                    None=>{},
                }
            }
            0
        }
        WM_RBUTTONUP=>{
            KillTimer(hwnd,stage_peek::TIMER);stage_peek::close(ptr::null_mut());
            cancel_deferred_click(hwnd);
            if STATE.with(|s|s.borrow().stages.iter().any(|stage|stage.hwnd==hwnd)){
                stage_menu(hwnd,l as i16 as i32,(l>>16) as i16 as i32);return 0;
            }
            let x=(l as u32&0xffff) as i16 as i32;
            let y=(l>>16) as i16 as i32;
            let system=STATE.with(|s|s.borrow().bars.iter().find(|b|b.hwnd==hwnd).and_then(|b|b.shell_target(x as f32,y as f32)));
            if system==Some(shell_access::Target::Start){activate_shell(hwnd,shell_access::Target::Start,true);return 0;}
            if system==Some(shell_access::Target::Tray){open_dock_hub(hwnd,taskbar_active());return 0;}
            if system.is_some(){return 0;}
            let on_status=STATE.with(|s|s.borrow().bars.iter().find(|b|b.hwnd==hwnd&&!b.collapsed)
                .and_then(|b|b.layout.as_ref())
                .is_some_and(|layout|layout.is_utility_button(x as f32,y as f32)||layout.inline_at(x as f32,y as f32).is_some()));
            if on_status{
                let recycle=STATE.with(|s|s.borrow().bars.iter().find(|b|b.hwnd==hwnd).and_then(|b|b.layout.as_ref())
                    .is_some_and(|l|l.inline_at(x as f32,y as f32)==Some(UtilityWidget::RecycleBin)));
                KillTimer(hwnd,STATUS_TIP_TIMER);close_status_tip(hwnd);
                if recycle{recycle_bin::perform(hwnd,recycle_bin::Action::Menu);}else{request_utilities(hwnd,None);}return 0;
            }
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
            let (rim,face)=stage_rail::colors(GetWindowLongPtrW(hwnd,GWLP_USERDATA) as usize);
            fill(dc,area,rim);
            if area.right-area.left>2&&area.bottom-area.top>2{
                area.left+=1;area.right-=1;area.top+=1;area.bottom-=1;
                fill(dc,area,face);
            }
            EndPaint(hwnd,&paint);0
        }
        WM_MOUSEMOVE=>{
            let mut stage=ptr::null_mut();
            update_state(|s|{
                let reduced=s.prefs.appearance.motion_reduced();
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
unsafe extern "system" fn window_event(_:HWINEVENTHOOK,event:u32,hwnd:HWND,object:i32,child:i32,_thread:u32,_time:u32) {
    if object==0 && child==0 {
        if event==EVENT_OBJECT_DESTROY{stage_snapshot::forget(hwnd);}
        if matches!(event,EVENT_OBJECT_DESTROY|EVENT_SYSTEM_MINIMIZEEND|EVENT_OBJECT_NAMECHANGE){window_effect::invalidate_recent(hwnd);}
        queue_refresh();
    }
}
unsafe fn drain_commands() {
    let commands:Vec<_>=COMMANDS.with(|c|c.borrow().as_ref().map(|rx|rx.try_iter().collect()).unwrap_or_default());
    for command in commands {
        match command {
            Command::Configure(options)=>{
                app_previews::close_for(ptr::null_mut());
                if !options.shell_context_menu{shell_menu::close();}
                window_effect::reset();
                stage_peek::close(ptr::null_mut());
                let enable_default=STATE.with(|s|!s.borrow().prefs.enabled&&options.enabled&&options.taskbar_default);
                let shortcut_changed=STATE.with(|s|s.borrow().prefs.start_shortcut!=options.start_shortcut);
                if shortcut_changed&&taskbar_hidden(){sync_start_hotkey(CONTROL.with(Cell::get),options.start_shortcut);}
                if !options.enabled&&(taskbar_active()||taskbar_retry_pending()||TASKBAR_RESUME.get()){show_taskbar(taskbar_guard::Exit::Requested,"Đã hiện lại thanh tác vụ khi tắt Dock.");}
                update_state(|state|{
                close_status_tip(ptr::null_mut());
                if state.prefs.appearance!=options.appearance||state.prefs.utility_profiles!=options.utility_profiles{utilities::close_panels();}
                for b in &mut state.bars{KillTimer(b.hwnd,STATUS_TIP_TIMER);b.status_hover=None;b.metric_repaint=None;}
                let repaint=state.prefs.appearance!=options.appearance;
                let icons_changed=state.prefs.app_icons!=options.app_icons;
                if repaint{state.material=None;}
                if repaint { for b in &mut state.bars{
                    KillTimer(b.hwnd,MOTION_TIMER);KillTimer(b.hwnd,HIDE_TIMER);KillTimer(b.hwnd,FEEDBACK_TIMER);
                    b.motion=Motion::default();b.feedback=None;b.reveal=motion::Spring::rest(if b.collapsed{0.0}else{1.0});
                } }
                state.prefs=options;refresh(state,false);
                if repaint||icons_changed{for b in &state.bars{InvalidateRect(b.hwnd,ptr::null(),0);}}
                if icons_changed{for stage in &state.stages{InvalidateRect(stage.hwnd,ptr::null(),0);}}
            });sync_win_key();refresh_clipboard_listener();motion_guard::sync();
                if enable_default&&!cfg!(test)&&!crate::trial::demo_active()&&!crate::trial::benchmark_active(){start_saved_taskbar_mode();}
            },
            Command::Suspend(on)=>{
                if on{motion_guard::stop();}
                if on{comfort::reset();}
                if on{app_previews::close_for(ptr::null_mut());}
                if on{window_effect::reset();shell_menu::close();}
                if on&&(taskbar_active()||taskbar_retry_pending()){
                    cancel_taskbar_retry();
                    TASKBAR_RESUME.set(true);
                    if taskbar_active(){
                        let reason=taskbar_guard::Exit::SessionSuspended;
                        show_taskbar(reason,reason.message());
                    }
                }
                update_state(|state|{if state.suspended!=on{state.suspended=on;refresh(state,!on);}});
                refresh_clipboard_listener();
                if !on&&TASKBAR_RESUME.replace(false){schedule_taskbar_retry();}
                sync_win_key();
            },
            Command::Taskbar(hidden)=>set_taskbar_hidden(hidden),
            Command::Stop=>{shell_menu::close();stage_peek::close(ptr::null_mut());control_center::inline_config(CONTROL.with(Cell::get),None);EndMenu();PostQuitMessage(0);},
            Command::Focus=>focus_dock(),
            #[cfg(test)] Command::InspectSettings{reply}=>{
                let all=windows(false);
                let items=STATE.with(|s|{
                    let s=s.borrow();
                    s.screens.iter().flat_map(|screen|{
                        item_list(&all,screen.bounds,&s.prefs).into_iter().filter(|i|i.path==internal_app::KEY)
                            .map(|i|(screen.id.clone(),i.hwnd as usize,!i.hwnd.is_null()&&IsIconic(i.hwnd)!=0,i.pinned))
                    }).collect()
                });
                let _=reply.send(items);
            },
            #[cfg(test)] Command::InspectStageSettings{reply}=>{
                let mut rows=Vec::new();
                update_state(|state|{
                    refresh(state,false);
                    for stage in &state.stages{
                        rows.extend(stage.items.iter().filter(|item|item.path==internal_app::KEY)
                            .map(|item|(stage.display.clone(),item.hwnd as usize,IsIconic(item.hwnd)!=0)));
                    }
                });
                let _=reply.send(rows);
            },
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
        bar.collapse(false,true);bar.keyboard=Some(bar.layout.as_ref().map_or(0,|l|l.key_order()[0]));bar.previous_focus=foreground as isize;
        Some(bar.hwnd)
    });
    if let Some(hwnd)=target{
        KillTimer(hwnd,HIDE_TIMER);KillTimer(hwnd,RAIL_FADE_TIMER);
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
        wc.cbSize=mem::size_of::<WNDCLASSEXW>() as u32;wc.lpfnWndProc=Some(proc);wc.style=CS_DBLCLKS;
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
        let control=CreateWindowExW(WS_EX_TOOLWINDOW,class.as_ptr(),wide("TLauncher Dock controller").as_ptr(),WS_POPUP,0,0,0,0,ptr::null_mut(),ptr::null_mut(),wc.hInstance,ptr::null());
        if control.is_null(){let _=ready.send(Err(format!("Create Dock controller: {}",GetLastError())));if com.is_ok(){CoUninitialize();}return;}
        CONTROL.with(|c|c.set(control));DIRTY.with(|c|c.set(false));
        internal_app::listen(control);
        let shell_hook=RegisterWindowMessageW(wide("SHELLHOOK").as_ptr());
        if shell_hook!=0&&RegisterShellHookWindow(control)!=0{
            ATTENTION_MSG.with(|id|id.set(shell_hook));
        }
        perf::start();
        OWNER.with(|o|o.set(owner as HWND));
        COMMANDS.with(|c|{c.replace(Some(commands));});EVENTS.with(|e|{e.replace(Some(events));});
        update_state(|state|{state.prefs=options;refresh(state,false);});
        refresh_clipboard_listener();
        publish_taskbar(false,TaskbarMode::Compatibility,taskbar_guard::current(),"Startup","");
        sync_win_key();
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
        else if !cfg!(test)&&!crate::trial::demo_active()&&!crate::trial::benchmark_active()
            && STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&s.prefs.taskbar_default}) {
            start_saved_taskbar_mode();
        }
        let mut msg:MSG=mem::zeroed();
        'dock_messages:loop{
            if window_effect::active(){
                // Follow the compositor's single-frame queue while animating.
                // Message input wakes this wait immediately. No polling at rest.
                let clock=window_effect::frame_ready();
                let count=u32::from(!clock.is_null());
                let timeout=if count==0{12}else{100};
                let result=MsgWaitForMultipleObjectsEx(count,if count==0{ptr::null()}else{&clock},timeout,QS_ALLINPUT,MWMO_INPUTAVAILABLE);
                if result==WAIT_TIMEOUT||(count==1&&result==WAIT_OBJECT_0){window_effect::timer();}
                for _ in 0..64{
                    if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}
                    if msg.message==WM_QUIT{break 'dock_messages;}
                    if !utilities::route_panel_keyboard(&msg){TranslateMessage(&msg);DispatchMessageW(&msg);}
                }
            }else{
                if GetMessageW(&mut msg,ptr::null_mut(),0,0)<=0{break;}
                if !utilities::route_panel_keyboard(&msg){TranslateMessage(&msg);DispatchMessageW(&msg);}
            }
        }
        window_effect::reset();motion_guard::stop();
        clipboard_history::configure(control,false,0);
        if taskbar_active(){show_taskbar(taskbar_guard::Exit::Requested,"Đã hiện lại thanh tác vụ khi thoát Deskland.");}
        WIN_KEY.with(|s|drop(s.borrow_mut().take()));
        shell_menu::shutdown();
        for hook in hooks{UnhookWinEvent(hook);}KillTimer(control,1);KillTimer(control,2);
        update_state(|s|{s.appbars.clear();s.workspaces.recover_for_exit();for b in s.bars.drain(..){DestroyWindow(b.hwnd);}for mut b in s.stages.drain(..){destroy_stage(&mut b);}});
        SURFACES.with(|s|s.borrow_mut().clear());STAGE_SURFACES.with(|s|s.borrow_mut().clear());ATLAS.with(|a|a.borrow_mut().take());
        comfort::reset();utilities::close_panels();UTILITIES.with(|u|u.borrow_mut().take());UTILITY_ANCHOR.with(|a|a.set(None));
        if ATTENTION_MSG.with(|id|id.replace(0))!=0{DeregisterShellHookWindow(control);}
        ATTENTION.with(|queue|queue.borrow_mut().clear());
        internal_app::unlisten(control);
        stage_snapshot::shutdown();
        CONTROL.with(|c|c.set(ptr::null_mut()));DestroyWindow(control);
        OWNER.with(|o|o.set(ptr::null_mut()));
        COMMANDS.with(|c|{c.take();});EVENTS.with(|e|{e.take();});STATE.with(|s|{s.take();});
        perf::finish();
        if com.is_ok(){CoUninitialize();}
    }
}
#[cfg(test)]
mod tests{
    #[test] fn stage_cycle_wraps_in_both_directions_independently_of_stack_order(){
        let id=|hwnd|WindowIdentity{hwnd,pid:3,tid:7};
        let members=[id(8),id(3),id(18),id(12),id(2),id(7)];
        assert_eq!(cycle_group(&members,id(18),true),Some(id(2)));
        assert_eq!(cycle_group(&members,id(2),false),Some(id(18)));
        assert_eq!(cycle_group(&members,id(8),true),Some(id(12)));
        assert_eq!(cycle_group(&members,WindowIdentity{pid:2,..id(8)},true),None);
        assert_eq!(cycle_group(&[id(3)],id(3),true),None);
    }
    #[test] fn stage_group_switch_eases_in_and_stops_at_full_opacity(){
        let [a,b,c,d]=[0.0,0.25,0.5,1.0].map(group_switch_ink);
        assert_eq!(a,GROUP_SWITCH_INK);
        assert!(b>a&&c>b&&d>c);
        assert!(b-a>c-b);
        assert_eq!(d,255);
    }
    #[test] fn dock_reveal_fades_to_readable_opacity_and_stops(){
        use std::time::Duration;
        assert_eq!(rail_opacity(Duration::ZERO,false),1.0);
        let midway=rail_opacity(Duration::from_millis(RAIL_FADE_MS as u64/2),false);
        assert!(midway>RAIL_DIM&&midway<1.0);
        assert_eq!(rail_opacity(Duration::from_secs(1),false),RAIL_DIM);
        assert_eq!(rail_opacity(Duration::ZERO,true),RAIL_DIM);
    }
    #[test]
    fn start_monitor_prefers_captured_pointer_over_foreground(){
        let main=RECT{left:0,top:0,right:1920,bottom:1080};
        let portrait=RECT{left:-1080,top:-400,right:0,bottom:1520};
        let display=|id:&str,bounds,primary,dpi|Display{id:id.into(),name:id.into(),bounds,work:bounds,primary,dpi};
        let screens=[display("main",main,true,96),display("portrait",portrait,false,144)];
        assert_eq!(screen_for_start(&screens,Some(portrait),Some(main)).unwrap().id,"portrait");
        assert_eq!(screen_for_start(&screens,Some(main),Some(portrait)).unwrap().id,"main");
        assert_eq!(screen_for_start(&screens,None,Some(portrait)).unwrap().id,"portrait");
        assert_eq!(screen_for_start(&screens,Some(portrait),None).unwrap().id,"portrait");
        assert_eq!(screen_for_start(&screens,None,None).unwrap().id,"main");
    }

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
    #[test] fn stage_padding_moves_cards_and_appbar_independently(){
        let sc=Screen{bounds:rect([-1400,-650,0,1050]),work:rect([-1400,-650,0,1010]),
            primary:false,id:"test".into(),name:"Test".into(),dpi:144};
        for right in [false,true]{
            let base=StageStyle{right:Some(right),edge_gap:0,app_gap:0,..Default::default()};
            let inset=StageStyle{edge_gap:24,..base};
            let app=StageStyle{app_gap:16,..base};
            let old=stage_rect_with_depth(&sc,base,Some(stage_depth(&sc,base)));
            let padded=stage_rect_with_depth(&sc,inset,Some(stage_depth(&sc,inset)));
            let spacing=stage_rect_with_depth(&sc,app,Some(stage_depth(&sc,app)));
            assert_eq!(padded.left-old.left,if right{-36}else{36});
            assert_eq!(padded.right-padded.left,old.right-old.left);
            assert_eq!((spacing.left,spacing.right),(old.left,old.right));
            assert_eq!(stage_depth(&sc,app)-stage_depth(&sc,base),24);
        }
    }
    #[test] fn stage_geometry_on_negative_monitor(){
        let sc=Screen{bounds:rect([-1920,-120,0,1080]),work:rect([-1920,-120,0,1040]),primary:false,id:"test".into(),name:"Test".into(),dpi:96};
        let left=stage_rect(&sc,StageStyle::default());
        let right=stage_rect(&sc,StageStyle{right:Some(true),scale:140,spacing:12,..Default::default()});
        assert!(left.left<right.left && left.left>=sc.work.left && right.right<=sc.work.right);
        assert!(left.top>=sc.work.top && left.bottom<=sc.work.bottom);
        assert!(right.right-right.left>left.right-left.left);
    }
    #[test] fn stage_groups_same_app_stays_local_and_retains_all_eight_members(){
        let make=|hwnd:usize,r:[i32;4],path:&str|AppWindow{
            hwnd:hwnd as HWND,title:format!("window {hwnd}"),path:path.into(),rect:rect(r),stage_aumid:None};
        let portrait=rect([-1920,0,0,1080]);let primary=rect([0,0,1920,1080]);
        let screens=[portrait,primary];
        let mut windows=(1..=8).map(|i|make(i,[-800,0,-300,800],r"C:\Apps\editor.exe")).collect::<Vec<_>>();
        windows.push(make(9,[50,0,500,800],r"c:\apps\EDITOR.exe"));
        windows.push(make(10,[-800,0,-300,800],r"C:\Tools\pythonw.exe"));
        windows.push(make(11,[-800,0,-300,800],r"C:\Tools\pythonw.exe"));
        let stage=workspaces::Workspaces::default();
        let local=stage.cards("portrait",portrait,&screens,&windows,false,true,4 as HWND);
        assert_eq!(local.len(),3);
        assert_eq!(local[0].item.count,8);
        assert_eq!(local[0].members.len(),8);
        assert_eq!(local[0].members[0],4);
        assert!(local[0].manual.is_none());
        assert_eq!(local[1].members,vec![10]);
        assert_eq!(local[2].members,vec![11]);
        let global=stage.cards("portrait",portrait,&screens,&windows,true,true,ptr::null_mut());
        assert_eq!(global[0].item.count,9);
        assert_eq!(global[0].members.last(),Some(&9));
        windows[9].stage_aumid=Some("AppA".into());windows[10].stage_aumid=Some("AppB".into());
        assert_eq!(stage.cards("portrait",portrait,&screens,&windows,false,true,ptr::null_mut()).len(),3);
        windows[10].stage_aumid=Some("appa".into());
        assert_eq!(stage.cards("portrait",portrait,&screens,&windows,false,true,ptr::null_mut())[1].item.count,2);
    }
    #[test] fn pinned_app_respects_local_monitor_and_deduplicates(){
        let make=|hwnd:usize,r:[i32;4]|AppWindow{hwnd:hwnd as HWND,title:format!("window {hwnd}"),path:"C:\\\\Apps\\\\alpha.exe".into(),rect:rect(r),stage_aumid:None};
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
            AppWindow{hwnd:1 as HWND,title:"remote".into(),path:"C:\\Apps\\alpha.exe".into(),rect:rect([100,0,700,900]),stage_aumid:None},
            AppWindow{hwnd:2 as HWND,title:"local".into(),path:"c:\\apps\\ALPHA.EXE".into(),rect:rect([-1800,0,-100,900]),stage_aumid:None},
            AppWindow{hwnd:3 as HWND,title:"remote beta".into(),path:"C:\\Apps\\beta.exe".into(),rect:rect([100,0,700,900]),stage_aumid:None}];
        let prefs=Preferences{all_windows:true,stage_side:"left".into(),..Default::default()};
        let global=item_list(&windows,rect([-1920,0,0,1080]),&prefs);
        assert_eq!(global.len(),2);assert_eq!(global[0].hwnd,2 as HWND);
        assert_eq!(global[0].title,"local");assert_eq!(global[0].count,2);
        assert_eq!(global[1].hwnd,3 as HWND);
        let local=item_list(&windows,rect([-1920,0,0,1080]),&Preferences{all_windows:false,..prefs});
        assert_eq!(local.len(),1);assert_eq!(local[0].hwnd,2 as HWND);assert_eq!(local[0].count,1);
    }
}
