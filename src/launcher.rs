//! Native Spotlight panel. No child edit/list controls and no RefCell borrow
//! crosses a Win32 call: synchronous window messages may re-enter this module.
use crate::desktop::wide;
use std::cell::{Cell,RefCell};
use std::path::{Path, PathBuf};
use std::sync::{Arc,LazyLock,Mutex};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub(crate) mod start;
mod catalog;
mod fullscreen;
pub(crate) mod library;
mod calculator;
mod index;
mod app_menu;
mod wave;

/// Start and Spotlight share this UI thread. Carry the actual input method
/// across the focus handoff, rather than reusing a layout left on our thread
/// by an earlier popup. Never change the source app or the user's preferences.
#[derive(Clone, Copy)]
struct InputHandoff(HKL);
impl InputHandoff {
    unsafe fn capture() -> Self {
        let hwnd=GetForegroundWindow();
        let thread=if hwnd.is_null(){0}else{GetWindowThreadProcessId(hwnd,std::ptr::null_mut())};
        Self(if thread==0{std::ptr::null_mut()}else{GetKeyboardLayout(thread)})
    }

    unsafe fn finish(self, popup:HWND, name:&str) {
        if self.0.is_null() || GetForegroundWindow()!=popup{return;}
        let current=GetKeyboardLayout(0);
        if current!=self.0 {
            // One transfer at entry, with no layout loading, process-wide
            // flags, or override of a later user-initiated language switch.
            let applied=!ActivateKeyboardLayout(self.0,0).is_null();
            if crate::trial::active(){
                crate::log!("POPUP_INPUT {name}: source={:04x} popup={:04x} carried={applied}",
                    self.0 as usize&0xffff,current as usize&0xffff);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Appearance { pub theme: String, pub transparency: f32, pub blur: f32 }
impl Default for Appearance {
    fn default() -> Self { Self { theme: "light".into(), transparency: 0.55, blur: 20.0 } }
}
impl Appearance {
    pub fn from_json(v: &serde_json::Value) -> Self {
        let d = Self::default();
        Self {
            theme: v.get("theme").and_then(|v| v.as_str()).filter(|v| matches!(*v, "light" | "dark")).unwrap_or(&d.theme).to_owned(),
            transparency: v.get("transparency").and_then(|v| v.as_f64()).map(|v| v as f32).filter(|v| v.is_finite()).unwrap_or(d.transparency).clamp(0.0, 0.85),
            blur: v.get("blur").and_then(|v| v.as_f64()).map(|v| v as f32).filter(|v| v.is_finite()).unwrap_or(d.blur).clamp(4.0, 40.0),
        }
    }
    pub fn json(&self) -> serde_json::Value { serde_json::json!({"theme": self.theme, "transparency": self.transparency, "blur": self.blur}) }
}

pub fn configure(appearance: Appearance) {
    if PREFS.with(|c| c.borrow().eq(&appearance)) {return;}
    PREFS.with(|c| *c.borrow_mut() = appearance.clone());
    let hwnd = STATE.with(|c| {
        let mut state=c.borrow_mut();
        let s=state.as_mut()?;
        s.glass=frost(&s.backdrop,s.width,s.height,s.scale,&appearance);
        Some(s.hwnd)
    });
    if let Some(hwnd)=hwnd{unsafe{InvalidateRect(hwnd,std::ptr::null(),0);}}
}
pub(crate) fn corners_changed(){
    let hwnd=STATE.with(|s|s.borrow().as_ref().map(|s|s.hwnd));
    if let Some(hwnd)=hwnd{unsafe{InvalidateRect(hwnd,std::ptr::null(),0);}}
    start::corners_changed();
}

const CLASS: &str = "TuanpcDeskLauncherGlass";
const ROW: i32 = 54;
const TOP: i32 = 78;
const PAGE: usize = 6;
const REFRESH: u32 = WM_APP + 101;
const OPEN_SELECTED: u32 = WM_APP + 102;
const HISTORY_LIMIT: usize = 30;

#[derive(Default, Debug, PartialEq)]
struct History { paths: Vec<PathBuf> }
impl History {
    fn from_json(value: &serde_json::Value) -> Self {
        let mut paths: Vec<PathBuf> = Vec::new();
        if let Some(items) = value.get("items").and_then(|v| v.as_array()) {
            for item in items {
                if let Some(path) = item.as_str().filter(|p| !p.is_empty() && p.len() <= 32768) {
                    let path = PathBuf::from(path);
                    if !paths.iter().any(|p| same_path(p, &path)) { paths.push(path); }
                    if paths.len() == HISTORY_LIMIT { break; }
                }
            }
        }
        Self { paths }
    }
    fn json(&self) -> serde_json::Value { serde_json::json!({"version":1,"items":self.paths}) }
    fn remember(&mut self, path: &Path) -> bool {
        if self.paths.first().is_some_and(|p| same_path(p, path)) { return false; }
        self.paths.retain(|p| !same_path(p, path));
        self.paths.insert(0, path.to_path_buf());
        self.paths.truncate(HISTORY_LIMIT);
        true
    }
}
fn same_path(a: &Path, b: &Path) -> bool { a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase() }
#[cfg(not(test))]
fn load_history() -> History {
    std::fs::read(crate::lock::data_dir().join("launcher-history.json")).ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok()).map(|v| History::from_json(&v)).unwrap_or_default()
}
#[cfg(test)]
fn load_history() -> History { History::default() }
#[cfg(not(test))]
fn save_history(history: &History) {
    if let Ok(bytes) = serde_json::to_vec(&history.json()) {
        if let Err(error) = std::fs::write(crate::lock::data_dir().join("launcher-history.json"), bytes) {
            crate::log!("Launcher: không lưu được lịch sử: {error}");
        }
    }
}
#[cfg(test)]
fn save_history(_history: &History) {}

#[derive(Clone, Debug)]
struct Entry { name: String, path: PathBuf, kind: &'static str, detail: String }
struct State {
    hwnd: HWND, all: Vec<Entry>, matches: Vec<usize>, query: String,
    selected: usize, first: usize, scale: f32, width: i32, height: i32,
    glass: Vec<u8>, backdrop: Vec<u8>, error: String, select_all: bool,
    icons: std::collections::HashMap<(PathBuf, i32), Option<Arc<IconPixels>>>,
    icon_requests: IconRequests,
    icon_results: std::sync::mpsc::Receiver<IconResult>,
    pending_icons: usize,
    apps_rx:Option<std::sync::mpsc::Receiver<Vec<Entry>>>,
    index_query: String, index_rows:Vec<Entry>, index_rx:Option<std::sync::mpsc::Receiver<Vec<Entry>>>, index_due:Option<std::time::Instant>,
    history: History, launching: bool, dock_start:bool, dock_anchor:Option<DockAnchor>,
    point: POINT, prior: HWND, icon_rev: u64,
}
#[derive(Clone)]
pub(crate) struct IconPixels { pub(crate) width: i32, pub(crate) height: i32, pub(crate) pixels: Vec<u8> }
thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    static DOCK_PINS:RefCell<Vec<String>>=const{RefCell::new(Vec::new())};
    static PIN_EDIT:RefCell<Option<crate::dock::Edit>>=const{RefCell::new(None)};
    static IN_ACTION_MENU:Cell<bool>=const{Cell::new(false)};
    static PREFS: RefCell<Appearance> = RefCell::new(Appearance::default());
}
pub fn dock_pins(paths:&[String]){
    DOCK_PINS.with(|v|v.replace(paths.to_vec()));
    start::pins_changed();
    fullscreen::update_pins();
    if let Some((hwnd,_,_,_))=handles(){
        STATE.with(|c|if let Some(s)=c.borrow_mut().as_mut(){sync_pin_entries(&mut s.all);});
        unsafe{if IsWindowVisible(hwnd)!=0{refresh_results(false);}}
    }
}
pub fn take_dock_edit()->Option<crate::dock::Edit>{PIN_EDIT.with(|edit|edit.borrow_mut().take())}
fn existing_pin(path:&Path)->Option<String>{
    let id=catalog::identity(path);
    DOCK_PINS.with(|pins|pins.borrow().iter().find(|p|same_path(Path::new(p),path)||(!id.is_empty()&&id==catalog::identity(Path::new(p)))).cloned())
}
fn pinned(path:&Path)->bool{existing_pin(path).is_some()}
pub fn cached_application_name(path:&Path)->Option<String>{catalog::label(path)}
pub fn shell_catalog_changed(){catalog::invalidate();}
fn app_entry(e:&Entry)->bool{matches!(e.kind,"Application"|"Pinned")}
fn sync_pin_entries(all:&mut Vec<Entry>){
    all.retain(|entry|entry.kind!="Pinned"&&entry.kind!=calculator::KIND&&!index::is_indexed(entry));
    let mut identities=all.iter().map(|e|catalog::identity(&e.path)).collect::<std::collections::HashSet<_>>();
    DOCK_PINS.with(|prefs|for value in prefs.borrow().iter(){
        if value=="deskland:settings"{continue;}
        let path=PathBuf::from(value);
        let id=catalog::identity(&path);
        if !identities.insert(id){continue;}
        let command=crate::dock::command_pin::decode(value);
        let name=command.as_ref().map(|c|c.label.clone()).unwrap_or_else(||{
            path.file_stem().and_then(|name|name.to_str()).unwrap_or(value).to_owned()
        });
        all.push(Entry{name,path,kind:"Pinned",detail:"Đã ghim vào Dock".into()});
    });
}
fn pin_change(entry:&Entry)->Option<crate::dock::Edit>{
    app_entry(entry).then(||{
        let path=entry.path.to_string_lossy().into_owned();
        if let Some(pin)=existing_pin(&entry.path){crate::dock::Edit::Unpin(pin)}else{crate::dock::Edit::Pin(path)}
    })
}
unsafe extern "system" { fn SetFocus(hwnd: HWND) -> HWND; }

fn enum_dir(root: &Path, depth: u8, files: bool, out: &mut Vec<Entry>) {
    if depth == 0 || out.len() >= 5000 { return; }
    let Ok(rd) = std::fs::read_dir(root) else { return };
    for item in rd.flatten() {
        if out.len() >= 5000 { break; }
        let p = item.path();
        // Do not follow junctions/symlinks into unrelated folders.
        use std::os::windows::fs::MetadataExt;
        let Ok(md) = item.metadata() else { continue };
        if md.file_attributes() & 0x400 != 0 { continue; }
        let ft = md.file_type();
        if ft.is_dir() { enum_dir(&p, depth - 1, files, out); continue; }
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or_default().to_ascii_lowercase();
        if !files && !matches!(ext.as_str(), "lnk" | "exe" | "url" | "appref-ms") { continue; }
        let name = p.file_stem().and_then(|x| x.to_str()).unwrap_or_default().replace('_', " ");
        if name.is_empty() || name.starts_with('.') || ext == "tmp" { continue; }
        let kind = match ext.as_str() { "lnk" | "exe" | "appref-ms" => "Application", "url" => "Web shortcut", _ => "File" };
        let size = md.len();
        let detail = if kind == "File" { format!("{} · {} KB · {}", ext.to_uppercase(), size.div_ceil(1024), p.display()) } else { p.display().to_string() };
        out.push(Entry { name, path: p, kind, detail });
    }
}

fn builtin_apps() -> Vec<Entry> {
    let mut out = Vec::new();
    // Prefer explicit system commands over duplicate shortcuts.
    for (name, command) in [("Paint", "mspaint.exe"), ("Notepad", "notepad.exe"), ("Calculator", "calc.exe"), ("File Explorer", "explorer.exe"), ("Settings", "ms-settings:"), ("Task Manager","Taskmgr.exe")] {
        let root = PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into()));
        let system = if command == "explorer.exe" { root.join(command) } else { root.join("System32").join(command) };
        let path = if system.is_file() { system } else { command.into() };
        out.push(Entry { name: name.into(), path, kind: "Application", detail: format!("Windows · {command}") });
    }
    out
}

fn app_entries() -> Vec<Entry> {
    let mut out = builtin_apps();
    if cfg!(test) { return out; }
    for key in ["APPDATA", "PROGRAMDATA"] {
        if let Ok(base) = std::env::var(key) { enum_dir(&PathBuf::from(base).join("Microsoft\\Windows\\Start Menu\\Programs"), 5, false, &mut out); }
    }
    out
}

unsafe fn installed_apps()->Vec<Entry>{
    use windows::{core::PCWSTR,Win32::{System::Com::CoTaskMemFree,UI::Shell as shell}};
    fn canonical_id(raw:&str)->Option<&str>{
        let id=raw.rsplit('\\').next()?;
        let (package,app)=id.split_once('!')?;
        let valid=|s:&str|!s.is_empty()&&s.bytes().all(|b|b.is_ascii_alphanumeric()||b"_.-".contains(&b));
        (id.len()<=350&&valid(package)&&valid(app)).then_some(id)
    }
    let Ok(folder)=shell::SHCreateItemInKnownFolder::<_,shell::IShellItem>(&shell::FOLDERID_AppsFolder,shell::KF_FLAG_DEFAULT,PCWSTR::null())else{return Vec::new();};
    let Ok(items)=folder.BindToHandler::<_,shell::IEnumShellItems>(None,&shell::BHID_EnumItems)else{return Vec::new();};
    let display=|item:&shell::IShellItem,kind|{
        let p=item.GetDisplayName(kind).ok()?;
        let value=p.to_string().ok();
        CoTaskMemFree(Some(p.0.cast()));
        value
    };
    let mut out=Vec::new();
    for _ in 0..1024{
        let mut row=[None];let mut count=0;
        if items.Next(&mut row,Some(&mut count)).is_err()||count==0{break;}
        let [Some(item)]=row else{break;};
        let Some(id)=[shell::SIGDN_PARENTRELATIVEPARSING,shell::SIGDN_DESKTOPABSOLUTEPARSING].into_iter()
            .find_map(|kind|display(&item,kind).and_then(|v|canonical_id(&v).map(str::to_owned)))else{continue;};
        let Some(name)=display(&item,shell::SIGDN_NORMALDISPLAY).filter(|s|!s.trim().is_empty())else{continue;};
        out.push(Entry{name,path:format!("shell:AppsFolder\\{id}").into(),kind:"Application",detail:"Ứng dụng Windows".into()});
    }
    out
}

fn filter(all: &[Entry], query: &str, history: &History) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        let indexed: std::collections::HashMap<_, _> = all.iter().enumerate().map(|(i, e)| (e.path.to_string_lossy().to_lowercase(), i)).collect();
        return history.paths.iter().filter_map(|path| indexed.get(&path.to_string_lossy().to_lowercase()).copied()).collect();
    }
    let mut found: Vec<(u8, usize)> = all.iter().enumerate().filter_map(|(i, e)| {
        let rank = catalog::search_rank(&e.name,&q).or_else(||index::is_indexed(e).then_some(3))?;
        Some((rank, i))
    }).collect();
    found.sort_by_key(|&(rank, i)| (rank, all[i].kind != "Application"));
    found.into_iter().map(|(_, i)| i).take(100).collect()
}

// Copy data before calling Win32. In particular ShellExecute, ShowWindow,
// SetFocus and painting must never run while STATE is borrowed.
fn handles() -> Option<(HWND, i32, i32, f32)> {
    STATE.with(|c| c.borrow().as_ref().map(|s| (s.hwnd, s.width, s.height, s.scale)))
}
fn start_rows(all:&[Entry],history:&History)->Vec<usize>{
    let pins=DOCK_PINS.with(|prefs|prefs.borrow().iter().filter_map(|path|
        all.iter().position(|e|app_entry(e)&&same_path(&e.path,Path::new(path)))).collect::<Vec<_>>());
    let mut seen=std::collections::HashSet::new();
    pins.into_iter().chain(filter(all,"",history).into_iter().filter(|&i|app_entry(&all[i])))
        .chain(all.iter().enumerate().filter_map(|(i,e)|app_entry(e).then_some(i)))
        .filter(|i|seen.insert(*i)).take(100).collect()
}
fn refresh() {refresh_results(false);}
fn refresh_results(keep_selection:bool) {
    let pending = STATE.with(|c| {
        let mut guard = c.borrow_mut(); let s = guard.as_mut()?;
        // The ephemeral calculation is always appended, so ordinary catalog
        // indices remain stable while results are being recomputed.
        while s.all.last().is_some_and(|e|e.kind==calculator::KIND||index::is_indexed(e)){s.all.pop();}
        let query=s.query.trim().to_lowercase();
        if s.index_query!=query{
            s.index_query=query;
            s.index_rows.clear();
            s.index_rx=None;
            s.index_due=(!s.dock_start&&index::eligible(&s.index_query)).then(||
                std::time::Instant::now()+std::time::Duration::from_millis(150));
        }
        if !s.dock_start&&!s.index_rows.is_empty(){
            let mut seen:std::collections::HashSet<_>=s.all.iter()
                .map(|e|e.path.to_string_lossy().to_lowercase()).collect();
            s.all.extend(s.index_rows.iter().filter(|e|seen.insert(e.path.to_string_lossy().to_lowercase())).cloned());
        }
        let keep=keep_selection.then(||s.matches.get(s.selected).copied()).flatten();
        s.matches = if s.dock_start&&s.query.trim().is_empty(){start_rows(&s.all,&s.history)}
            else{filter(&s.all,&s.query,&s.history)};
        if !s.dock_start{
            if let Some(result)=calculator::entry(&s.query){
                s.matches.insert(0,s.all.len());s.matches.truncate(100);s.all.push(result);
            }
        }
        s.selected=keep.and_then(|idx|s.matches.iter().position(|&i|i==idx)).unwrap_or(0);
        s.first=s.first.min(s.selected).max((s.selected+1).saturating_sub(PAGE));if !keep_selection{s.error.clear();}
        Some((s.hwnd,s.index_due.is_some()||s.index_rx.is_some()))
    });
    if let Some((hwnd,index_pending)) = pending { unsafe {
        if index_pending{SetTimer(hwnd,7,60,None);}
        InvalidateRect(hwnd, std::ptr::null(), 0);
    } }
}

fn remember_entry(path: &Path) {
    STATE.with(|c| { if let Some(s) = c.borrow_mut().as_mut() {
        if s.history.remember(path) { save_history(&s.history); }
    } });
}

fn dismiss(hwnd: HWND) {
    wave::close(hwnd);
    let searched = STATE.with(|c| {
        let g = c.borrow(); let s = g.as_ref()?;
        if s.launching || s.query.trim().is_empty() { return None; }
        s.matches.get(s.selected).and_then(|&i| s.all.get(i)).filter(|e|e.kind!=calculator::KIND).map(|e| e.path.clone())
    });
    if let Some(path) = searched { remember_entry(&path); }
    STATE.with(|c|if let Some(s)=c.borrow_mut().as_mut(){
        s.index_due=None;s.index_rx=None;s.index_rows.clear();s.index_query.clear();
        while s.all.last().is_some_and(|e|e.kind==calculator::KIND||index::is_indexed(e)){s.all.pop();}
    });
    stop_compact_motion(hwnd);
    unsafe { KillTimer(hwnd,7); ShowWindow(hwnd, SW_HIDE); }
}

fn dismiss_compact_with_focus(hwnd: HWND) {
    let prior = STATE.with(|s| s.borrow().as_ref().map(|p| p.prior));
    dismiss(hwnd);
    if let Some(prior) = prior {
        unsafe { if !prior.is_null() && IsWindow(prior) != 0 { SetForegroundWindow(prior); } }
    }
}
const COMPACT_TWEEN:usize=8;
use crate::dock::motion::{self,Tween};
struct CompactFrame {
    source:crate::dock::PopupPixels,
    target:crate::dock::PopupPixels,
    width:i32,
    height:i32,
}
thread_local! {
    static COMPACT_MOTION:Cell<Option<Tween>>=const{Cell::new(None)};
    static COMPACT_FRAME:RefCell<Option<CompactFrame>>=const{RefCell::new(None)};
}
fn stop_compact_motion(hwnd:HWND){
    COMPACT_MOTION.set(None);
    COMPACT_FRAME.with(|f|f.borrow_mut().take());
    unsafe{KillTimer(hwnd,COMPACT_TWEEN);}
}
fn start_compact_motion(hwnd:HWND,open:bool)->bool{
    if !fullscreen::motion_allowed(){return false;}
    let t=Tween::new(open,motion::COMPACT,COMPACT_MOTION.get());
    let t=if open&&unsafe{IsWindowVisible(hwnd)}==0{t.prepared()}else{t};
    if unsafe{SetTimer(hwnd,COMPACT_TWEEN,15,None)}==0{return false;}
    COMPACT_MOTION.set(Some(t));
    if open{wave::resume(hwnd);}else{wave::close(hwnd);}
    true
}
fn close_compact(hwnd:HWND,toggle:bool){
    if COMPACT_MOTION.get().is_some_and(|t|t.to==0.0){
        if toggle&&!start_compact_motion(hwnd,true){dismiss_compact_with_focus(hwnd);}
    }else if start_compact_motion(hwnd,false){
        unsafe{paint(hwnd);}
    }else{dismiss_compact_with_focus(hwnd);}
}
unsafe fn present_compact_transition(hwnd:HWND,frame:&mut CompactFrame,t:Tween){
    let k=t.openness();
    let s=motion::zoom(motion::COMPACT.scale,k);
    let (w,h)=(frame.width,frame.height);
    let (sw,sh)=((w as f32*s).round() as i32,(h as f32*s).round() as i32);
    GdiFlush();
    frame.target.pixels().fill(0);
    let alpha=BLENDFUNCTION{BlendOp:AC_SRC_OVER as u8,BlendFlags:0,SourceConstantAlpha:255,AlphaFormat:AC_SRC_ALPHA as u8};
    AlphaBlend(frame.target.dc(),(w-sw)/2,(h-sh)/2,sw,sh,frame.source.dc(),0,0,w,h,alpha);
    let opacity=BLENDFUNCTION{SourceConstantAlpha:(255.0*k).round() as u8,..alpha};
    let size=SIZE{cx:w,cy:h};
    let origin=POINT{x:0,y:0};
    UpdateLayeredWindow(hwnd,std::ptr::null_mut(),std::ptr::null(),&size,frame.target.dc(),&origin,0,&opacity,ULW_ALPHA);
}
unsafe fn compact_transition_frame(hwnd:HWND,w:i32,h:i32,pixels:&[u8])->bool{
    let Some(t)=COMPACT_MOTION.get()else{return false;};
    COMPACT_FRAME.with(|cache|{
        let mut cache=cache.borrow_mut();
        if !cache.as_ref().is_some_and(|f|f.width==w&&f.height==h){
            *cache=(||Some(CompactFrame{source:crate::dock::PopupPixels::new(w,h)?,target:crate::dock::PopupPixels::new(w,h)?,width:w,height:h}))();
        }
        let Some(frame)=cache.as_mut()else{return false;};
        frame.source.pixels().copy_from_slice(pixels);
        // Prepare the source raster first; opening must not consume the
        // transition while the initial icons and glass are being painted.
        present_compact_transition(hwnd,frame,t);
        true
    })
}
fn compact_hit(l:LPARAM)->(i32,i32){
    let (x,y)=(l as i16 as i32,(l>>16) as i16 as i32);
    let Some(t)=COMPACT_MOTION.get()else{return(x,y);};
    let k=motion::zoom(motion::COMPACT.scale,t.openness());
    let center=COMPACT_FRAME.with(|p|p.borrow().as_ref().map(|f|(f.width as f32/2.0,f.height as f32/2.0)));
    center.map_or((x,y),|(cx,cy)|((cx+(x as f32-cx)/k).round() as i32,(cy+(y as f32-cy)/k).round() as i32))
}
fn move_selection(delta: i32) {
    let hwnd = STATE.with(|c| {
        let mut guard = c.borrow_mut(); let s = guard.as_mut()?;
        s.selected = (s.selected as i32 + delta).clamp(0, s.matches.len().saturating_sub(1) as i32) as usize;
        if s.selected < s.first { s.first = s.selected; }
        if s.selected >= s.first + PAGE { s.first = s.selected + 1 - PAGE; }
        Some(s.hwnd)
    });
    if let Some(hwnd) = hwnd { unsafe { InvalidateRect(hwnd, std::ptr::null(), 0); } }
}

fn launch_selected() {
    let target = STATE.with(|c| {
        let guard = c.borrow(); let s = guard.as_ref()?;
        let idx = *s.matches.get(s.selected)?;
        Some((s.hwnd, s.all.get(idx)?.clone()))
    });
    let Some((hwnd, entry)) = target else { return; };
    if entry.kind==calculator::KIND{
        unsafe{
            if calculator::copy(hwnd,&entry){dismiss_compact_with_focus(hwnd);}
            else{
                STATE.with(|c|if let Some(s)=c.borrow_mut().as_mut(){s.error=calculator::copy_error().into();});
                InvalidateRect(hwnd,std::ptr::null(),0);
            }
        }
        return;
    }
    STATE.with(|c| if let Some(s) = c.borrow_mut().as_mut() { s.launching = true; });
    stop_compact_motion(hwnd);
    unsafe {
        // Hide before ShellExecute activates another process (WM_ACTIVATE is synchronous).
        ShowWindow(hwnd, SW_HIDE);
        let result = execute(hwnd, &entry.path);
        STATE.with(|c| if let Some(s) = c.borrow_mut().as_mut() { s.launching = false; });
        if result <= 32 {
            if !index::is_indexed(&entry){
                crate::log!("Launcher: không mở được {} (ShellExecute {})", entry.path.display(), result);
            }
            STATE.with(|c| { if let Some(s) = c.borrow_mut().as_mut() { s.error = format!("Could not open {} (Windows error {result})", entry.name); } });
            ShowWindow(hwnd, SW_SHOW); InvalidateRect(hwnd, std::ptr::null(), 0);
        } else {
            remember_entry(&entry.path);
        }
    }
}

#[cfg(not(test))]
unsafe fn execute(hwnd: HWND, path: &Path) -> isize {
    if let Some(value)=path.to_str().filter(|v|crate::dock::command_pin::decode(v).is_some()){
        return crate::dock::command_pin::open(value);
    }
    ShellExecuteW(hwnd, wide("open").as_ptr(), wide(&path.to_string_lossy()).as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) as isize
}

// One bounded icon queue and cache. Popups own only their reply endpoints.
type IconKey = (PathBuf, i32);
type IconResult = (IconKey, Option<Arc<IconPixels>>);
type IconJob = (IconKey, Option<std::sync::mpsc::Sender<IconResult>>);
type RasterRows=std::collections::VecDeque<(IconKey,Option<Arc<IconPixels>>,u64,std::time::Instant,Option<String>)>;
static RASTERS:LazyLock<Mutex<RasterRows>>=LazyLock::new(||Mutex::new(RasterRows::new()));
const RASTER_BUDGET:usize=8*1024*1024;
static PREFETCH_SIZE:std::sync::atomic::AtomicI32=std::sync::atomic::AtomicI32::new(0);
static PREFETCH_COMPACT:std::sync::atomic::AtomicI32=std::sync::atomic::AtomicI32::new(36);
pub(crate) fn prewarm_spotlight(icon_size:i32){
    use std::sync::atomic::Ordering::Relaxed;
    if cfg!(test){return;}
    let dpi=unsafe{windows_sys::Win32::UI::HiDpi::GetDpiForSystem()}.clamp(72,288);
    PREFETCH_COMPACT.store((36*dpi as i32+48)/96,Relaxed);
    PREFETCH_SIZE.store(((icon_size*dpi as i32+48)/96).clamp(8,192),Relaxed);
    // A Spotlight or Start open during boot may already have populated the
    // catalog. Consume an immediately available result; otherwise the shared
    // catalog worker prefetches when it publishes the rows.
    if let Some(rows)=catalog::request(false,false).and_then(|rx|rx.try_recv().ok()){
        prefetch_catalog(&rows);
    }
}
fn prefetch_catalog(rows:&[Entry]){
    use std::sync::atomic::Ordering::Relaxed;
    let size=PREFETCH_SIZE.swap(0,Relaxed);
    if size==0{return;}
    let Some(work)=icon_queue()else{return;};
    let mut apps=rows.iter().filter(|app|app_entry(app)).collect::<Vec<_>>();
    apps.sort_by_cached_key(|app|app.name.to_lowercase());
    let compact=PREFETCH_COMPACT.load(Relaxed);
    for app in apps.into_iter().take(32) {
        for edge in [size,compact] {
            let key=(app.path.clone(),edge);
            if warm_raster(&key).is_none()&&work.enqueue(key,None).is_err(){return;}
        }
    }
}
fn lookup_raster(key:&IconKey,allow_stale:bool)->Option<(Option<Arc<IconPixels>>,bool)>{
    let mut rows=RASTERS.try_lock().ok()?;
    let revision=crate::app_icons::revision();
    let at=rows.iter().position(|row|row.0==*key)?;
    let mut pair=rows.remove(at)?;
    if pair.2!=revision{
        let token=crate::app_icons::override_token(&key.0,&catalog::identity(&key.0));
        if pair.4!=token{return None;}
        pair.2=revision;
    }
    let elapsed=pair.3.elapsed().as_secs();
    if !(allow_stale&&pair.1.is_some())&&elapsed>=if pair.1.is_some(){300}else{10}{return None;}
    let result=(pair.1.clone(),elapsed<300);
    rows.push_back(pair);
    Some(result)
}
fn warm_raster(key:&IconKey)->Option<Option<Arc<IconPixels>>>{lookup_raster(key,false).map(|p|p.0)}
fn retain_raster(key:IconKey,icon:Option<Arc<IconPixels>>,revision:u64){
    let token=crate::app_icons::override_token(&key.0,&catalog::identity(&key.0));
    let Ok(mut rows)=RASTERS.lock() else{return;};
    if crate::app_icons::revision()!=revision{return;}
    rows.retain(|r|r.0!=key);
    rows.push_back((key,icon,revision,std::time::Instant::now(),token));
    let mut bytes=rows.iter().filter_map(|r|r.1.as_ref()).map(|icon|icon.pixels.len()).sum::<usize>();
    while rows.len()>512||bytes>RASTER_BUDGET{
        let Some((_,icon,_,_,_))=rows.pop_front() else{break;};
        bytes=bytes.saturating_sub(icon.as_ref().map_or(0,|v|v.pixels.len()));
    }
}
struct IconRequests(std::sync::mpsc::Sender<IconResult>);
#[derive(Default)]
struct IconWork {
    jobs:Mutex<(std::collections::VecDeque<IconJob>,std::collections::VecDeque<IconKey>)>,
    ready:std::sync::Condvar,
}
impl IconWork {
    fn enqueue(&self,key:IconKey,reply:Option<std::sync::mpsc::Sender<IconResult>>)->Result<(),()>{
        let mut jobs=self.jobs.lock().map_err(|_|())?;
        if let Some(reply)=reply {
            if jobs.0.len()>=64{return Err(());}
            jobs.0.push_back((key,Some(reply)));
        }else{
            if jobs.1.len()>=64{return Err(());}
            if !jobs.1.contains(&key){jobs.1.push_back(key);}
        }
        self.ready.notify_one();
        Ok(())
    }
    fn next(&self)->Option<IconJob>{
        let mut jobs=self.jobs.lock().ok()?;
        loop{
            if let Some(job)=jobs.0.pop_front(){return Some(job);}
            if let Some(key)=jobs.1.pop_front(){return Some((key,None));}
            jobs=self.ready.wait(jobs).ok()?;
        }
    }
}
fn icon_queue()->Option<&'static Arc<IconWork>>{
    static WORK:std::sync::OnceLock<Option<Arc<IconWork>>>=std::sync::OnceLock::new();
    WORK.get_or_init(||{
        let work=Arc::new(IconWork::default());
        let worker=work.clone();
        std::thread::Builder::new().name("tlauncher-shell-icons".into()).spawn(move||unsafe{
            use windows::Win32::System::Com::*;
            let com=CoInitializeEx(None,COINIT_APARTMENTTHREADED);
            while let Some((key,reply))=worker.next(){
                let revision=crate::app_icons::revision();
                let visible=reply.is_some();
                let icon=warm_raster(&key).unwrap_or_else(||{
                    let loaded=extract_icon(&key.0,key.1.clamp(8,192),visible);
                    // A speculative miss cannot suppress a real request later.
                    if visible||loaded.is_some(){retain_raster(key.clone(),loaded.clone(),revision);}
                    loaded
                });
                if let Some(reply)=reply{let _=reply.send((key,icon));}
            }
            if com.is_ok(){CoUninitialize();}
        }).ok()?;
        Some(work)
    }).as_ref()
}
impl IconRequests {
    fn send(&self, key: IconKey) -> Result<(), ()> {
        icon_queue().ok_or(())?.enqueue(key,Some(self.0.clone()))
    }
}
fn icon_worker() -> (IconRequests, std::sync::mpsc::Receiver<IconResult>) {
    let (tx, rx) = std::sync::mpsc::channel();
    (IconRequests(tx), rx)
}
unsafe fn extract_icon(path: &Path, size: i32, allow_extract:bool) -> Option<Arc<IconPixels>> {
    let command=path.to_str().and_then(crate::dock::command_pin::decode);
    let id=catalog::identity(path);
    let source=command.as_ref().map_or(path,|c|Path::new(&c.exe));
    let load=if allow_extract{crate::app_icons::shared_source}else{crate::app_icons::cached_source};
    let icon=load(path,source,Path::new(&id),crate::app_icons::source_tier((size*3+1)/2))?;
    // Do not manufacture detail by enlarging small shell icons. Keep their
    // native raster centred in a larger tile when no large source exists.
    let scale = (size as f32 / icon.width.max(icon.height) as f32).min(1.0);
    let width = (icon.width as f32 * scale).round().max(1.0) as i32;
    let height = (icon.height as f32 * scale).round().max(1.0) as i32;
    if width==icon.width&&height==icon.height{return Some(icon);}
    let image = image::RgbaImage::from_raw(icon.width as u32, icon.height as u32, icon.pixels.clone())?;
    let mut pixels = image::imageops::resize(&image, width as u32, height as u32, image::imageops::FilterType::Lanczos3).into_raw();
    for p in pixels.chunks_exact_mut(4) { for c in 0..3 {p[c] = p[c].min(p[3]);} }
    Some(Arc::new(IconPixels { width, height, pixels }))
}

/// Preserve the shell bitmap dimensions. IconPixels are premultiplied BGRA
/// (the order used by both Win32 renderers); resize only at the destination.
pub(crate) unsafe fn shell_icon(path: &Path, source_size: i32) -> Option<IconPixels> {
    use windows::Win32::UI::Shell::{SHCreateItemFromParsingName, IShellItemImageFactory, SIIGBF_ICONONLY, SIIGBF_BIGGERSIZEOK};
    let name = path.to_string_lossy();
    let factory_for = |name: &str| {
        let path = wide(name);
        { let item: windows::core::Result<IShellItemImageFactory> = SHCreateItemFromParsingName(windows::core::PCWSTR(path.as_ptr()), None); item }
    };
    let factory = factory_for(&name).or_else(|error| {
        if let Some(alternate)=apps_folder_parsing_name(&name){factory_for(&alternate)}else{Err(error)}
    }).ok()?;
    let bitmap = factory.GetImage(windows::Win32::Foundation::SIZE { cx: source_size, cy: source_size }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
    let bitmap = bitmap.0;
    let mut object: BITMAP = std::mem::zeroed();
    if GetObjectW(bitmap, std::mem::size_of::<BITMAP>() as i32, &mut object as *mut _ as *mut _) == 0 { DeleteObject(bitmap); return None; }
    let (w, h) = (object.bmWidth, object.bmHeight.abs());
    if w <= 0 || h <= 0 || w > 512 || h > 512 { DeleteObject(bitmap); return None; }
    let dc = CreateCompatibleDC(std::ptr::null_mut());
    if dc.is_null(){DeleteObject(bitmap);return None;}
    let mut info: BITMAPINFO = std::mem::zeroed(); info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = w; info.bmiHeader.biHeight = -h; info.bmiHeader.biPlanes = 1; info.bmiHeader.biBitCount = 32;
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    let ok = GetDIBits(dc, bitmap, 0, h as u32, pixels.as_mut_ptr() as *mut _, &mut info, DIB_RGB_COLORS);
    DeleteDC(dc); DeleteObject(bitmap);
    if ok != h || !pixels.iter().any(|&v|v!=0) { return None; }
    normalize_shell_alpha(&mut pixels);
    Some(IconPixels { width: w, height: h, pixels })
}

fn apps_folder_parsing_name(path:&str)->Option<String>{
    const PREFIX:&str="shell:AppsFolder\\";
    if !path.get(..PREFIX.len())?.eq_ignore_ascii_case(PREFIX){return None;}
    let id=path.get(PREFIX.len()..)?;
    if id.is_empty()||id.contains(['\\','/']){return None;}
    Some(format!(r"::{{4234d49b-0245-4df3-b780-3893943456e1}}\{id}"))
}

#[cfg(test)]
#[test]
fn apps_folder_icon_fallback_uses_one_separator_and_preserves_aumid(){
    let id="Microsoft.WindowsNotepad_8wekyb3d8bbwe!App";
    let path=format!(r"Shell:AppsFolder\{id}");
    assert_eq!(apps_folder_parsing_name(&path).unwrap(),format!(r"::{{4234d49b-0245-4df3-b780-3893943456e1}}\{id}"));
    assert!(apps_folder_parsing_name(r"shell:AppsFolder\\Invalid").is_none());
    assert!(apps_folder_parsing_name(r"C:\Windows\explorer.exe").is_none());
}

fn normalize_shell_alpha(pixels: &mut [u8]) {
    let alpha_missing = pixels.chunks_exact(4).all(|p| p[3] == 0);
    let straight = !alpha_missing && pixels.chunks_exact(4).any(|p| p[3] > 0 && p[..3].iter().any(|&c| c > p[3]));
    for p in pixels.chunks_exact_mut(4) {
        if alpha_missing { p[3] = 255; }
        else if p[3] == 0 { p[..3].fill(0); }
        else if straight {
            let a = p[3] as u32;
            for c in &mut p[..3] { *c = ((*c as u32 * a + 127) / 255) as u8; }
        }
    }
}

#[cfg(test)]
#[test]
fn shell_pixels_keep_premultiplied_bgra_without_colored_transparent_edges() {
    let mut straight=[30,60,240,128,200,200,200,0,3,220,120,255];
    normalize_shell_alpha(&mut straight);
    assert_eq!(straight,[15,30,120,128,0,0,0,0,3,220,120,255]);
    let mut premultiplied=[15,30,120,128,0,0,0,0,3,220,120,255];
    normalize_shell_alpha(&mut premultiplied);
    assert_eq!(premultiplied,straight);
}
fn icon_for(hwnd: HWND, path: &Path, size: i32) -> Option<Arc<IconPixels>> {
    let (icon, pending) = STATE.with(|c| {
        let mut g = c.borrow_mut(); let s = g.as_mut()?;
        if s.icon_rev!=crate::app_icons::revision(){s.icons.clear();s.pending_icons=0;(s.icon_requests,s.icon_results)=icon_worker();s.icon_rev=crate::app_icons::revision();}
        let key = (path.to_path_buf(), size);
        if !s.icons.contains_key(&key){
            if let Some((icon,fresh))=lookup_raster(&key,true){
                if !fresh&&s.pending_icons<24&&s.icon_requests.send(key.clone()).is_ok(){
                    s.icons.insert(key,icon.clone());
                    s.pending_icons+=1;
                    return Some((icon,true));
                }
                return Some((icon,false));
            }
        }
        if !s.icons.contains_key(&key) && s.pending_icons < 24 {
            if s.icons.len() >= 192 { s.icons.retain(|_,icon|icon.is_none()); if s.icons.len()>=192{s.icons.clear();} }
            if s.icon_requests.send(key.clone()).is_ok(){
                s.icons.insert(key, None);
                s.pending_icons += 1;
                return Some((None, true));
            }
        }
        Some((s.icons.get(&key).cloned().flatten(), false))
    }).unwrap_or((None, false));
    if pending { unsafe { SetTimer(hwnd, 7, 60, None); } }
    icon
}
#[cfg(test)]
unsafe fn execute(_hwnd: HWND, _path: &Path) -> isize { 33 }

// Frost the pixels behind the panel once per opening. Text remains fully opaque.
// Unlike whole-window alpha this never reveals sharp desktop icons through text,
// and costs no continuous blur/redraw work while the panel is idle.
pub(crate) unsafe fn capture_glass(x: i32, y: i32, width: i32, height: i32) -> Vec<u8> {
    let (bw, bh) = ((width / 4).max(1), (height / 4).max(1));
    if cfg!(test) { return vec![96; (bw * bh * 4) as usize]; }
    let screen = GetDC(std::ptr::null_mut()); let dc = CreateCompatibleDC(screen);
    let mut info: BITMAPINFO = std::mem::zeroed();
    info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = bw; info.bmiHeader.biHeight = -bh; info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32; info.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let bitmap = CreateDIBSection(screen, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
    if bitmap.is_null() || bits.is_null() || dc.is_null() {
        if !bitmap.is_null() { DeleteObject(bitmap); } if !dc.is_null() { DeleteDC(dc); } ReleaseDC(std::ptr::null_mut(), screen); return Vec::new();
    }
    let old = SelectObject(dc, bitmap);
    SetStretchBltMode(dc, HALFTONE);
    let ok = StretchBlt(dc, 0, 0, bw, bh, screen, x, y, width, height, SRCCOPY);
    GdiFlush();
    let raw = if ok != 0 { std::slice::from_raw_parts(bits as *const u8, (bw * bh * 4) as usize).to_vec() } else { Vec::new() };
    SelectObject(dc, old); DeleteObject(bitmap); DeleteDC(dc); ReleaseDC(std::ptr::null_mut(), screen);
    raw
}

pub(crate) fn frost(raw: &[u8], width: i32, height: i32, scale: f32, appearance: &Appearance) -> Vec<u8> {
    let bw = (width / 4).max(1) as u32; let bh = (height / 4).max(1) as u32;
    let Some(img) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(bw, bh, raw) else { return Vec::new() };
    let mut pixels = image::imageops::blur(&img, appearance.blur * scale / 4.0).into_raw();
    let dark = appearance.theme == "dark";
    let base = if dark { 24.0 } else { 247.0 };
    let tint = base * (1.0 - appearance.transparency);
    for (y, row) in pixels.chunks_exact_mut(bw as usize * 4).enumerate() {
        let reflection = (1.0 - (y as f32 + 0.5) / bh as f32).powi(5) * if dark { 15.0 } else { 8.0 };
        for p in row.chunks_exact_mut(4) {
            for v in &mut p[..3] { *v = (*v as f32 * appearance.transparency + tint + reflection).clamp(0.0, 255.0) as u8; }
            p[3] = 255;
        }
    }
    pixels
}

unsafe fn font(size: i32, weight: u32) -> HFONT {
    CreateFontW(-size, 0, 0, 0, weight as i32, 0, 0, 0, DEFAULT_CHARSET as u32, 0, 0, ANTIALIASED_QUALITY as u32, DEFAULT_PITCH as u32, wide("Segoe UI Variable").as_ptr())
}
unsafe fn label(dc: HDC, text: &str, mut rect: RECT, font: HFONT, color: u32) {
    let old = SelectObject(dc, font); SetTextColor(dc, color); SetBkMode(dc, TRANSPARENT as i32);
    let text = wide(text); DrawTextW(dc, text.as_ptr(), -1, &mut rect, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX);
    SelectObject(dc, old);
}

// Premultiplied pixel composition keeps text opaque and gives rounded edges
// fractional alpha. No region clipping or global Windows theme changes.
fn panel_coverage(x: f32, y: f32, w: f32, h: f32, radius: f32) -> (f32, f32) {
    let qx = (x - w / 2.0).abs() - (w / 2.0 - radius);
    let qy = (y - h / 2.0).abs() - (h / 2.0 - radius);
    let distance = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
    ((0.5 - distance).clamp(0.0, 1.0), distance)
}
fn round_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let mut p = tiny_skia::PathBuilder::new(); let k = 0.5522848 * r;
    p.move_to(x+r,y); p.line_to(x+w-r,y); p.cubic_to(x+w-r+k,y,x+w,y+r-k,x+w,y+r);
    p.line_to(x+w,y+h-r); p.cubic_to(x+w,y+h-r+k,x+w-r+k,y+h,x+w-r,y+h);
    p.line_to(x+r,y+h); p.cubic_to(x+r-k,y+h,x,y+h-r+k,x,y+h-r);
    p.line_to(x,y+r); p.cubic_to(x,y+r-k,x+r-k,y,x+r,y); p.close(); p.finish().unwrap()
}
fn blend_pixels(dst: &mut [u8], stride: i32, x: i32, y: i32, src: &[u8], w: i32, h: i32, rgba: bool) {
    let rows = dst.len() / (stride as usize * 4);
    for sy in 0..h { for sx in 0..w {
        let (dx, dy) = (x+sx, y+sy);
        if dx < 0 || dx >= stride || dy < 0 || dy as usize >= rows { continue; }
        let si = ((sy*w+sx)*4) as usize; let di = ((dy*stride+dx)*4) as usize;
        let a = src[si+3] as u32;
        for c in 0..3 { let sc = if rgba { 2-c } else { c }; dst[di+c] = (src[si+sc] as u32 + (dst[di+c] as u32 * (255-a) + 127) / 255).min(255) as u8; }
    } }
}

unsafe fn paint(hwnd: HWND) {
    let model = STATE.with(|c| c.borrow().as_ref().map(|s| (
        s.width, s.height, s.scale, s.glass.clone(), s.query.clone(),
        s.matches.iter().skip(s.first).take(PAGE).filter_map(|&i| s.all.get(i).cloned()).collect::<Vec<_>>(),
        s.selected.saturating_sub(s.first), s.matches.len(), s.error.clone(), s.select_all,s.dock_start,
    )));
    let appearance = PREFS.with(|c| c.borrow().clone());
    let dark = appearance.theme == "dark";
    let high=fullscreen::high_contrast();
    let mut ps: PAINTSTRUCT = std::mem::zeroed(); let target = BeginPaint(hwnd, &mut ps);
    if let Some((w, h, scale, glass, query, rows, selected, count, error, select_all,dock_start)) = model {
        let corner_radius=crate::dock::corners::radius(20.0*scale);
        let px = |v: i32| (v as f32 * scale).round() as i32;
        let pad = px(12); let ww = w+2*pad; let hh = h+2*pad;
        let mut info: BITMAPINFO = std::mem::zeroed(); info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = ww; info.bmiHeader.biHeight = -hh; info.bmiHeader.biPlanes = 1; info.bmiHeader.biBitCount = 32;
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(target, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        let dc = CreateCompatibleDC(target);
        if bitmap.is_null() || dc.is_null() || bits.is_null() {
            if !bitmap.is_null() { DeleteObject(bitmap); } if !dc.is_null() { DeleteDC(dc); } EndPaint(hwnd, &ps); return;
        }
        std::ptr::write_bytes(bits as *mut u8, 0, (ww*hh*4) as usize);
        let old = SelectObject(dc, bitmap); SetViewportOrgEx(dc, pad, pad, std::ptr::null_mut());
        let bg = CreateSolidBrush(if dark { 0x00201D1B } else { 0x00F8F7F5 }); FillRect(dc, &RECT {left:0,top:0,right:w,bottom:h}, bg); DeleteObject(bg);
        if !glass.is_empty() {
            let mut background = info; background.bmiHeader.biWidth = (w/4).max(1); background.bmiHeader.biHeight = -(h/4).max(1);
            SetStretchBltMode(dc, HALFTONE);
            StretchDIBits(dc, 0, 0, w, h, 0, 0, (w/4).max(1), (h/4).max(1), glass.as_ptr() as *const _, &background, DIB_RGB_COLORS, SRCCOPY);
        }
        GdiFlush();
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, (ww*hh*4) as usize);
        if let Some(mut overlay) = tiny_skia::Pixmap::new(w as u32, h as u32) {
            let mut color = tiny_skia::Paint::default(); color.anti_alias = true;
            color.set_color_rgba8(255,255,255,if dark {65} else {155});
            overlay.stroke_path(&round_path(0.75,0.75,w as f32-1.5,h as f32-1.5,corner_radius), &color, &tiny_skia::Stroke {width:scale.max(1.0),..Default::default()}, tiny_skia::Transform::identity(), None);
            if !rows.is_empty() {
                color.set_color_rgba8(if dark {255} else {30}, if dark {255} else {45}, if dark {255} else {60}, if dark {28} else {24});
                overlay.fill_path(&round_path(px(9) as f32,px(TOP+selected as i32*ROW) as f32,(w-px(18)) as f32,px(ROW-3) as f32,9.0*scale), &color, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
            }
            // Compact and fullscreen use the same 49 DIP, 16 DIP-radius
            // glass search treatment. The contrast tint is limited to this
            // field; the existing glass material remains unchanged.
            if !dock_start{
                color.set_color_rgba8(12,17,24,35);
                overlay.fill_path(&round_path(px(13) as f32,px(11) as f32,(w-px(26)) as f32,px(51) as f32,17.0*scale),&color,tiny_skia::FillRule::Winding,tiny_skia::Transform::identity(),None);
                if high{
                    let c=GetSysColor(COLOR_WINDOW);
                    color.set_color_rgba8(c as u8,(c>>8) as u8,(c>>16) as u8,255);
                }else{color.set_color_rgba8(if dark{52}else{98},if dark{61}else{108},if dark{73}else{119},if dark{150}else{210});}
                let field=round_path(px(14) as f32,px(10) as f32,(w-px(28)) as f32,px(49) as f32,16.0*scale);
                overlay.fill_path(&field,&color,tiny_skia::FillRule::Winding,tiny_skia::Transform::identity(),None);
                if high{
                    let c=GetSysColor(COLOR_WINDOWTEXT);
                    color.set_color_rgba8(c as u8,(c>>8) as u8,(c>>16) as u8,255);
                }else{color.set_color_rgba8(245,248,252,if GetForegroundWindow()==hwnd{100}else{62});}
                overlay.stroke_path(&field,&color,&tiny_skia::Stroke{width:scale.max(1.0),..Default::default()},tiny_skia::Transform::identity(),None);
            }
            if high{
                let c=GetSysColor(COLOR_WINDOWTEXT);
                color.set_color_rgba8(c as u8,(c>>8) as u8,(c>>16) as u8,255);
            }else{color.set_color_rgba8(if dock_start&&!dark{65}else{245},if dock_start&&!dark{70}else{248},if dock_start&&!dark{78}else{252},220);}
            let mut p = tiny_skia::PathBuilder::new(); p.push_circle(31.0*scale,34.0*scale,8.5*scale); p.move_to(37.2*scale,40.2*scale); p.line_to(44.0*scale,47.0*scale);
            if let Some(path) = p.finish() { overlay.stroke_path(&path, &color, &tiny_skia::Stroke {width:1.8*scale,line_cap:tiny_skia::LineCap::Round,..Default::default()}, tiny_skia::Transform::identity(), None); }
            color.set_color_rgba8(if dark {255} else {25},if dark {255} else {30},if dark {255} else {40},if dark {40} else {30});
            if let Some(rect) = tiny_skia::Rect::from_xywh(px(18) as f32,px(68) as f32,(w-px(36)) as f32,scale.max(1.0)) { overlay.fill_rect(rect,&color,tiny_skia::Transform::identity(),None); }
            blend_pixels(pixels,ww,pad,pad,overlay.data(),w,h,true);
        }
        let body = font(px(16),400); let small = font(px(12),400); let search = font(px(if dock_start{25}else{16}),500);
        let primary = if dark {0x00FAF8F6} else {0x002B2724}; let secondary = if dark {0x00C8C2BC} else {0x00665D54};
        let search_ink=if high{GetSysColor(COLOR_WINDOWTEXT)}else if dock_start{primary}else{0x00fcf8f5};
        let search_placeholder=if high{search_ink}else if dock_start{secondary}else{0x00d8d4cf};
        let text = wide(&query); let mut extent: SIZE = std::mem::zeroed(); let oldfont = SelectObject(dc,search);
        GetTextExtentPoint32W(dc,text.as_ptr(),text.len().saturating_sub(1) as i32,&mut extent); SelectObject(dc,oldfont);
        if select_all && !query.is_empty() { let brush=CreateSolidBrush(if high{GetSysColor(COLOR_HIGHLIGHT)}else if dock_start&&!dark{0x00ECD5BC}else{0x009c7048}); FillRect(dc,&RECT {left:px(59),top:px(23),right:(px(62)+extent.cx).min(w-px(99)),bottom:px(47)},brush); DeleteObject(brush); }
        label(dc,if query.is_empty(){if dock_start{"Tìm ứng dụng, tệp…"}else{"Spotlight Search"}}else{&query},RECT {left:px(60),top:px(10),right:w-px(99),bottom:px(59)},search,if query.is_empty(){search_placeholder}else if high&&select_all{GetSysColor(COLOR_HIGHLIGHTTEXT)}else{search_ink});
        if !dock_start { label(dc,"▦",RECT{left:w-px(80),top:px(10),right:w-px(48),bottom:px(59)},search,search_ink); }
        if GetForegroundWindow() == hwnd&&!select_all { let caret=CreateSolidBrush(search_ink); let x=(px(60)+extent.cx).min(w-px(99)); FillRect(dc,&RECT{left:x,top:px(24),right:x+px(1).max(1),bottom:px(46)},caret); DeleteObject(caret); }
        label(dc,"esc",RECT {left:w-px(43),top:px(24),right:w-px(14),bottom:px(48)},small,search_placeholder);
        let mut icons = Vec::new();
        for (i,entry) in rows.iter().enumerate() {
            let y=px(TOP+i as i32*ROW);
            if let Some(icon) = (entry.kind!=calculator::KIND).then(||icon_for(hwnd,&entry.path,px(36))).flatten() { icons.push((icon,px(18),y+px(7))); }
            else { label(dc,match entry.kind{"Application"=>"▣",calculator::KIND=>"=" ,_=>"▤"},RECT{left:px(23),top:y,right:px(52),bottom:y+px(50)},search,secondary); }
            label(dc,&entry.name,RECT {left:px(66),top:y+px(3),right:w-px(77),bottom:y+px(27)},body,primary);
            let detail=if entry.kind==calculator::KIND{calculator::hint().into()}else{format!("{}{} · {}",if dock_start&&pinned(&entry.path){"Đã ghim · "}else{""},entry.kind,entry.detail)};
            label(dc,&detail,RECT {left:px(66),top:y+px(27),right:w-px(24),bottom:y+px(46)},small,secondary);
            if i==selected { label(dc,"↵",RECT {left:w-px(49),top:y+px(3),right:w-px(22),bottom:y+px(28)},body,secondary); }
        }
        if rows.is_empty(){label(dc,if query.trim().is_empty(){"Type to find apps and files. Recent items will appear here."}else{"No results"},RECT{left:px(28),top:px(105),right:w-px(24),bottom:px(147)},body,secondary);}
        label(dc,if !error.is_empty(){&error}else if dock_start{"↑ ↓  Chọn     ↵  Mở     Chuột phải: ghim vào Dock"}else if query.trim().is_empty(){"Recent · newest first     ↑ ↓  Select     ↵  Open     Esc  Close"}else{"↑ ↓  Select       ↵  Open       Esc  Close"},RECT{left:px(22),top:h-px(36),right:w-px(80),bottom:h-px(10)},small,secondary);
        label(dc,&format!("{count}"),RECT{left:w-px(58),top:h-px(36),right:w-px(15),bottom:h-px(10)},small,secondary);
        GdiFlush();
        for (icon,x,y) in icons { blend_pixels(pixels,ww,pad+x,pad+y,&icon.pixels,icon.width,icon.height,false); }
        for y in 0..hh { for x in 0..ww {
            let (coverage,distance)=panel_coverage((x-pad) as f32+0.5,(y-pad) as f32+0.5,w as f32,h as f32,corner_radius);
            let i=((y*ww+x)*4) as usize;
            let shadow=(-distance.max(0.0)/(3.5*scale)).exp()*0.22;
            for c in 0..3 { pixels[i+c]=(pixels[i+c] as f32*coverage) as u8; }
            pixels[i+3]=((coverage+(1.0-coverage)*shadow)*255.0) as u8;
        } }
        SetViewportOrgEx(dc,0,0,std::ptr::null_mut());
        let size=SIZE {cx:ww,cy:hh}; let origin=POINT{x:0,y:0};
        let blend=BLENDFUNCTION {BlendOp:AC_SRC_OVER as u8,BlendFlags:0,SourceConstantAlpha:255,AlphaFormat:AC_SRC_ALPHA as u8};
        if !compact_transition_frame(hwnd,ww,hh,pixels)&&UpdateLayeredWindow(hwnd,std::ptr::null_mut(),std::ptr::null(),&size,dc,&origin,0,&blend,ULW_ALPHA)==0 { crate::log!("Launcher: UpdateLayeredWindow failed {}",GetLastError()); }
        SelectObject(dc,old); DeleteObject(bitmap); DeleteDC(dc); DeleteObject(body); DeleteObject(small); DeleteObject(search);
    }
    EndPaint(hwnd,&ps);
}

unsafe fn result_menu(hwnd:HWND,mouse:Option<(i32,i32)>){
    let target=STATE.with(|c|{
        let mut g=c.borrow_mut();let s=g.as_mut()?;
        if let Some((_,y))=mouse{
            let logical=(y as f32/s.scale).floor() as i32-12;
            let row=logical.checked_sub(TOP).filter(|v|*v>=0)?/ROW;
            if row>=PAGE as i32||s.first+row as usize>=s.matches.len(){return None;}
            s.selected=s.first+row as usize;
        }
        Some((s.all.get(*s.matches.get(s.selected)?)?.clone(),s.scale,s.selected.saturating_sub(s.first)))
    });
    let Some((entry,scale,row))=target else{return;};
    let Some(edit)=pin_change(&entry)else{return;};
    InvalidateRect(hwnd,std::ptr::null(),0);
    let menu=CreatePopupMenu();
    if menu.is_null(){return;}
    let label=if matches!(&edit,crate::dock::Edit::Unpin(_)){"Bỏ ghim khỏi Dock"}else{"Ghim vào Dock"};
    AppendMenuW(menu,MF_STRING,1,wide("Mở ứng dụng").as_ptr());
    AppendMenuW(menu,MF_STRING,2,wide(label).as_ptr());
    if app_menu::available(&entry){AppendMenuW(menu,MF_STRING,3,wide(localized("Lệnh ứng dụng Windows…","Windows app commands…","Windows 应用命令…")).as_ptr());}
    let (x,y)=mouse.unwrap_or(((80.0*scale) as i32,((TOP+(row as i32+1)*ROW+12) as f32*scale) as i32));
    let mut point=POINT{x,y};ClientToScreen(hwnd,&mut point);
    IN_ACTION_MENU.with(|flag|flag.set(true));
    let cmd=TrackPopupMenu(menu,TPM_RETURNCMD|TPM_RIGHTBUTTON|TPM_NONOTIFY,point.x,point.y,0,hwnd,std::ptr::null());
    IN_ACTION_MENU.with(|flag|flag.set(false));
    DestroyMenu(menu);
    match cmd{
        1=>{PostMessageW(hwnd,OPEN_SELECTED,0,0);},
        2=>{PIN_EDIT.with(|pending|pending.replace(Some(edit)));},
        3=>{IN_ACTION_MENU.with(|flag|flag.set(true));app_menu::show(hwnd,point,&entry);IN_ACTION_MENU.with(|flag|flag.set(false));},
        _=>{}
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => { paint(hwnd); 0 }
        WM_TIMER if w==COMPACT_TWEEN=>{
            if let Some(t)=COMPACT_MOTION.get(){
                if t.progress()>=1.0||!motion::allowed(){
                    stop_compact_motion(hwnd);
                    if t.to==0.0{dismiss_compact_with_focus(hwnd);}else{paint(hwnd);}
                }else{
                    COMPACT_FRAME.with(|f|if let Some(frame)=f.borrow_mut().as_mut(){present_compact_transition(hwnd,frame,t);});
                }
            }else{stop_compact_motion(hwnd);}
            0
        }
        WM_SETFOCUS | WM_KILLFOCUS => { InvalidateRect(hwnd, std::ptr::null(), 0); 0 }
        WM_TIMER if w == 7 => {
            let (changed, pending, apps_changed) = STATE.with(|c| {
                let mut g=c.borrow_mut(); let Some(s)=g.as_mut() else {return (false,false,false);}; let mut changed=false;
                while let Ok((key,icon))=s.icon_results.try_recv() { s.icons.insert(key,icon); s.pending_icons=s.pending_icons.saturating_sub(1); changed=true; }
                let mut apps_changed=false;
                if let Some(rx)=&s.apps_rx{
                    match rx.try_recv(){
                        Ok(apps)=>{
                            s.apps_rx=None;
                            s.all=apps;
                            fullscreen::merge_manual(&mut s.all);
                            sync_pin_entries(&mut s.all);
                            apps_changed=true;
                        },
                        Err(std::sync::mpsc::TryRecvError::Disconnected)=>s.apps_rx=None,
                        Err(std::sync::mpsc::TryRecvError::Empty)=>{}
                    }
                }
                if s.index_due.is_some_and(|deadline|std::time::Instant::now()>=deadline){
                    s.index_rx=fullscreen::search_index_async(s.index_query.clone());
                    if s.index_rx.is_some(){s.index_due=None;}
                }
                if let Some(rx)=&s.index_rx{
                    match rx.try_recv(){
                        Ok(rows)=>{s.index_rows=rows;s.index_rx=None;apps_changed=true;}
                        Err(std::sync::mpsc::TryRecvError::Disconnected)=>s.index_rx=None,
                        Err(std::sync::mpsc::TryRecvError::Empty)=>{}
                    }
                }
                (changed,s.pending_icons>0||s.apps_rx.is_some()||s.index_rx.is_some()||s.index_due.is_some(),apps_changed)
            });
            if apps_changed{refresh_results(true);}
            if changed { InvalidateRect(hwnd,std::ptr::null(),0); }
            if !pending || IsWindowVisible(hwnd)==0 { KillTimer(hwnd,7); } 0
        }
        WM_ACTIVATE if w & 0xffff == WA_INACTIVE as usize => { if !IN_ACTION_MENU.with(Cell::get){dismiss(hwnd);} 0 }
        WM_CLOSE => { dismiss(hwnd); 0 }
        REFRESH => { refresh(); 0 }
        OPEN_SELECTED => { launch_selected(); 0 }
        WM_KEYDOWN => {
            if GetKeyState(VK_CONTROL as i32) < 0 {
                match w as u32 {
                    0x41 => { STATE.with(|c| if let Some(s) = c.borrow_mut().as_mut() { s.select_all = true; }); InvalidateRect(hwnd, std::ptr::null(), 0); }
                    0x56 => {
                        if OpenClipboard(hwnd) != 0 {
                            let data = GetClipboardData(13); // CF_UNICODETEXT
                            if !data.is_null() {
                                let ptr = GlobalLock(data) as *const u16;
                                if !ptr.is_null() {
                                    let cap = (GlobalSize(data) / 2).min(512); let slice = std::slice::from_raw_parts(ptr, cap);
                                    let n = slice.iter().position(|&ch| ch == 0).unwrap_or(cap);
                                    let text: String = String::from_utf16_lossy(&slice[..n]).chars().filter(|ch| !ch.is_control()).collect();
                                    STATE.with(|c| if let Some(s) = c.borrow_mut().as_mut() { if s.select_all { s.query.clear(); } s.select_all = false; s.query.extend(text.chars().take(128)); });
                                    GlobalUnlock(data);
                                }
                            }
                            CloseClipboard(); PostMessageW(hwnd, REFRESH, 0, 0);
                        }
                    }
                    _ => {}
                }
                return 0;
            }
            match w as u16 {
                VK_F6 => expand_compact(),
                VK_APPS => result_menu(hwnd,None),
                VK_F10 if GetKeyState(VK_SHIFT as i32)<0=>result_menu(hwnd,None),
                VK_ESCAPE => { close_compact(hwnd,false); }
                VK_RETURN => { PostMessageW(hwnd, OPEN_SELECTED, 0, 0); }
                VK_DOWN => move_selection(1), VK_UP => move_selection(-1),
                VK_NEXT => move_selection(PAGE as i32), VK_PRIOR => move_selection(-(PAGE as i32)),
                VK_BACK => {
                    STATE.with(|c| if let Some(s) = c.borrow_mut().as_mut() { if s.select_all { s.query.clear(); } else { s.query.pop(); } s.select_all = false; });
                    PostMessageW(hwnd, REFRESH, 0, 0);
                }
                _ => {}
            } 0
        }
        WM_CHAR => {
            if let Some(ch) = char::from_u32(w as u32).filter(|c| !c.is_control()) {
                STATE.with(|c| if let Some(s) = c.borrow_mut().as_mut() { if s.select_all { s.query.clear(); } s.select_all = false; if s.query.len() < 512 { s.query.push(ch); } });
                PostMessageW(hwnd, REFRESH, 0, 0);
            } 0
        }
        WM_MOUSEWHEEL => { move_selection(if (w >> 16) as i16 > 0 { -1 } else { 1 }); 0 }
        WM_RBUTTONUP=>{result_menu(hwnd,Some(compact_hit(l)));0},
        WM_LBUTTONDOWN | WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
            let (x,y)=compact_hit(l);
            if msg==WM_LBUTTONUP && STATE.with(|s|s.borrow().as_ref().is_some_and(|p|{
                let x=(x as f32/p.scale) as i32-12;
                let y=(y as f32/p.scale) as i32-12;
                !p.dock_start && x>=580 && x<=612 && (10..=59).contains(&y)
            })){expand_compact();return 0;}
            let clicked = STATE.with(|c| {
                let mut guard = c.borrow_mut(); let Some(s) = guard.as_mut() else { return (false,false); };
                let logical_y = (y as f32 / s.scale) as i32 - 12;
                let row = (logical_y - TOP) / ROW;
                if logical_y >= TOP && row >= 0 && row < PAGE as i32 && s.first + (row as usize) < s.matches.len() {
                    s.selected = s.first + row as usize;
                    (true,if s.dock_start{msg==WM_LBUTTONUP}else{msg==WM_LBUTTONDBLCLK})
                } else { (false,false) }
            });
            if clicked.0 { InvalidateRect(hwnd, std::ptr::null(), 0); if clicked.1 { PostMessageW(hwnd, OPEN_SELECTED, 0, 0); } }
            0
        }
        WM_SETCURSOR => { SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_ARROW)); 1 }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

pub(crate) fn spotlight_locale(value:&str){fullscreen::locale(value);}
pub(crate) fn localized<'a>(vi:&'a str,en:&'a str,zh:&'a str)->&'a str{fullscreen::words(vi,en,zh)}

pub fn configure_spotlight(options: library::Preferences) {
    wave::configure(options.edge_wave&&options.enabled&&options.animations);
    fullscreen::configure(options);
    if !fullscreen::enabled() { close_spotlight(); }
    if !fullscreen::motion_allowed(){
        if let (Some(t),Some((hwnd,_,_,_)))=(COMPACT_MOTION.get(),handles()){
            stop_compact_motion(hwnd);
            if t.to==0.0{dismiss_compact_with_focus(hwnd);}
            else{unsafe{InvalidateRect(hwnd,std::ptr::null(),0);}}
        }
    }
}
pub fn initialize_spotlight(value: library::Library) { fullscreen::initialize(value); }
pub fn close_spotlight(){
    wave::stop();
    fullscreen::dismiss(false);
    fullscreen::reset_graphics_factories();
    if let Some((hwnd,_,_,_))=handles(){
        unsafe{if IsWindowVisible(hwnd)!=0{dismiss(hwnd);}}
    }
}
pub(crate) fn spotlight_frame_clock()->HANDLE{wave::frame_clock()}
pub(crate) fn spotlight_frame_wake(){wave::frame_wake();}
pub(crate) fn spotlight_frame_failed(){wave::stop();}
fn compact_handoff()->Option<(POINT,String,HWND)> {
    let (hwnd,_,_,_)=handles()?;
    if unsafe{IsWindowVisible(hwnd)}==0{return None;}
    let handoff=STATE.with(|p|p.borrow().as_ref().map(|p|(p.point,p.query.clone(),p.prior)));
    stop_compact_motion(hwnd);
    IN_ACTION_MENU.with(|cell|cell.set(true));
    unsafe { KillTimer(hwnd,7); ShowWindow(hwnd,SW_HIDE); }
    IN_ACTION_MENU.with(|cell|cell.set(false));
    handoff
}
fn expand_compact(){
    if let Some((point,query,prior))=compact_handoff(){
        fullscreen::open(point,query,prior,false);
    }
}
pub fn open_spotlight_dock(point: POINT) {
    if !fullscreen::enabled(){return;}
    if fullscreen::fullscreen_for(true){
        let prior=unsafe{GetForegroundWindow()};
        let old=compact_handoff();
        fullscreen::open(point,old.as_ref().map(|(_,s,_)|s.clone()).unwrap_or_default(),
            old.map_or(prior,|(_,_,p)|p),false);
    }else if let Some((_,query,prior))=fullscreen::compact_query(){
        fullscreen::dismiss(false);
        toggle_at(Some(point),Some(query),Some(prior));
    }else{open_compact(Some(point),None);}
}
pub fn open_spotlight_hidden(){
    if !fullscreen::enabled(){return;}
    let mut at=POINT{x:0,y:0};
    let prior=unsafe{GetCursorPos(&mut at);GetForegroundWindow()};
    let old=compact_handoff();
    fullscreen::open(at,String::new(),old.map_or(prior,|(_,_,p)|p),true);
}
pub fn reset_spotlight_layout(owner:HWND) { fullscreen::reset(owner); }
pub fn refresh_spotlight_library(){
    if let Some(hwnd)=fullscreen::window(){fullscreen::refresh_catalog(hwnd);}
    else{let _=catalog::request(true,true);}
}
pub fn toggle(_owner: HWND) {
    if !fullscreen::enabled(){return;}
    if fullscreen::toggle_visible(){return;}
    if let Some((hwnd,_,_,_))=handles(){
        if unsafe{IsWindowVisible(hwnd)}!=0{close_compact(hwnd,true);return;}
    }
    if fullscreen::fullscreen_for(false){
        let mut point=POINT{x:0,y:0};
        let prior=unsafe{GetCursorPos(&mut point);GetForegroundWindow()};
        fullscreen::open(point,String::new(),prior,false);
    }else{open_compact(None,None);}
}
pub(super) fn open_compact(point:Option<POINT>,query:Option<String>){toggle_at(point,query,None);}
#[derive(Clone,Copy)]
pub(crate) struct DockAnchor {pub point:POINT,pub side:crate::dock::DockSide}
impl DockAnchor {
    pub(crate) fn same_position(self,other:Self)->bool{
        self.side==other.side&&self.point.x==other.point.x&&self.point.y==other.point.y
    }
    pub(crate) fn encode(self)->(WPARAM,LPARAM){
        let side=match self.side{crate::dock::DockSide::Bottom=>0,crate::dock::DockSide::Top=>3,crate::dock::DockSide::Left=>1,crate::dock::DockSide::Right=>2};
        (((self.point.x as u32 as u64)|((side as u64)<<32)) as WPARAM,self.point.y as LPARAM)
    }
    pub(crate) fn decode(w:WPARAM,l:LPARAM)->Self{
        let side=match (w as u64)>>32{1=>crate::dock::DockSide::Left,2=>crate::dock::DockSide::Right,3=>crate::dock::DockSide::Top,_=>Default::default()};
        Self{point:POINT{x:w as i32,y:l as i32},side}
    }
    fn position(self,work:RECT,w:i32,h:i32,pad:i32)->(i32,i32){
        use crate::dock::DockSide;
        let (x,y)=match self.side{
            DockSide::Bottom=>(self.point.x,self.point.y-h-pad),
            DockSide::Top=>(self.point.x,self.point.y+pad),
            DockSide::Left=>(self.point.x+pad,self.point.y-h/2),
            DockSide::Right=>(self.point.x-w-pad,self.point.y-h/2),
        };
        (x.clamp(work.left,(work.right-w).max(work.left)),y.clamp(work.top,(work.bottom-h).max(work.top)))
    }
}
fn spotlight_origin(screen:RECT,w:i32,h:i32)->(i32,i32){
    (screen.left+(screen.right-screen.left-w)/2,screen.top+(screen.bottom-screen.top-h)/2)
}
fn toggle_at(at:Option<POINT>, query:Option<String>, prior:Option<HWND>) {
    unsafe {
        if let Some((hwnd, _, _, _)) = handles() {
            if IsWindowVisible(hwnd) != 0 {
                let same=STATE.with(|s|s.borrow().as_ref().is_some_and(|s| at.is_none_or(|p|MonitorFromPoint(p,MONITOR_DEFAULTTONEAREST)==MonitorFromPoint(s.point,MONITOR_DEFAULTTONEAREST))));
                if same{close_compact(hwnd,true);return;}
                dismiss(hwnd);
            }
        } else {
            let instance = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
            let cls = wide(CLASS); let mut wc: WNDCLASSEXW = std::mem::zeroed();
            wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32; wc.lpfnWndProc = Some(proc); wc.hInstance = instance;
            wc.style = CS_DBLCLKS; wc.lpszClassName = cls.as_ptr(); wc.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
            RegisterClassExW(&wc);
            // An independent top-level window receives focus and keyboard input
            // without inheriting the hidden message window's visibility/owner chain.
            let hwnd = CreateWindowExW(WS_EX_LAYERED|WS_EX_TOOLWINDOW, cls.as_ptr(), wide("TLauncher Spotlight").as_ptr(), WS_POPUP, 0, 0, 684, 468, std::ptr::null_mut(), std::ptr::null_mut(), instance, std::ptr::null_mut());
            if hwnd.is_null() { crate::log!("Launcher: CreateWindowEx failed {}", GetLastError()); return; }
            let mut all = catalog::snapshot(true);
            fullscreen::merge_manual(&mut all);
            sync_pin_entries(&mut all);
            let (icon_requests,icon_results)=icon_worker();
            STATE.with(|c| *c.borrow_mut() = Some(State { hwnd, all, matches: Vec::new(), query: String::new(), selected: 0, first: 0, scale: 1.0, width: 660, height: 444, glass: Vec::new(), backdrop: Vec::new(), error: String::new(), select_all: false, icons:std::collections::HashMap::new(),icon_requests,icon_results,pending_icons:0,apps_rx:None,index_query:String::new(),index_rows:Vec::new(),index_rx:None,index_due:None,history:load_history(),launching:false,dock_start:false,dock_anchor:None,point:POINT{x:0,y:0},prior:std::ptr::null_mut(),icon_rev:crate::app_icons::revision() }));
        }
        let Some((hwnd, _, _, _)) = handles() else { return; };
        let mouse=at.unwrap_or_else(||{let mut p=POINT{x:0,y:0};GetCursorPos(&mut p);p});
        let monitor = MonitorFromPoint(mouse, MONITOR_DEFAULTTONEAREST); let mut mi: MONITORINFO = std::mem::zeroed(); mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32; GetMonitorInfoW(monitor, &mut mi);
        if mi.rcWork.right<=mi.rcWork.left||mi.rcWork.bottom<=mi.rcWork.top{return;}
        // Query DPI after moving the still-hidden HWND to the target output.
        SetWindowPos(hwnd,std::ptr::null_mut(),mi.rcWork.left,mi.rcWork.top,1,1,SWP_NOACTIVATE|SWP_NOREDRAW|SWP_NOZORDER);
        let fit=mi.rcMonitor;
        let scale = (windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd).clamp(72,288) as f32 / 96.0)
            .min((fit.right-fit.left) as f32/684.0).min((fit.bottom-fit.top) as f32/468.0);
        let width = (660.0 * scale) as i32; let height = (444.0 * scale) as i32;
        let pad=(12.0*scale).round() as i32;
        let (x,y)=spotlight_origin(mi.rcMonitor,width+2*pad,height+2*pad);
        let backdrop = capture_glass(x+pad, y+pad, width, height);
        let appearance=PREFS.with(|c| c.borrow().clone());
        let glass=frost(&backdrop,width,height,scale,&appearance);
        let mut base=catalog::snapshot(true);
        fullscreen::merge_manual(&mut base);
        sync_pin_entries(&mut base);
        STATE.with(|c| if let Some(s) = c.borrow_mut().as_mut() { s.all=base; s.scale = scale; s.width = width; s.height = height; s.glass = glass; s.backdrop=backdrop; s.query=query.unwrap_or_default(); s.select_all = false; s.dock_start=false;s.dock_anchor=None;s.point=mouse;s.prior=prior.unwrap_or_else(||GetForegroundWindow());
            if s.apps_rx.is_none(){s.apps_rx=catalog::request(true,false);}
        });
        refresh();
        SetWindowPos(hwnd, HWND_TOP, x, y, width+2*pad, height+2*pad, SWP_NOACTIVATE);
        start_compact_motion(hwnd,true);
        paint(hwnd);
        SetTimer(hwnd,7,60,None);
        let input=InputHandoff::capture();
        wave::open(hwnd,mouse,POINT{x:x+width/2,y:y+(42.0*scale) as i32});
        ShowWindow(hwnd, SW_SHOW); SetForegroundWindow(hwnd); SetFocus(hwnd);
        if let Some(t)=COMPACT_MOTION.get(){COMPACT_MOTION.set(Some(t.restart_clock()));}
        input.finish(hwnd,"Spotlight");
    }
}

pub fn hotkey_text(mods: u32, vk: u32) -> String { crate::shortcuts::label(mods,vk) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn start_tracks_button_left_and_keeps_gap_across_content_height_and_dpi(){
        use crate::dock::DockSide;
        for dpi in [96,120,144]{
            let pad=10*dpi/96;
            let work=RECT{left:-1440,top:-640,right:0,bottom:900};
            let a=DockAnchor{point:POINT{x:-1120,y:822},side:DockSide::Bottom};
            for h in [120,250]{
                let (x,y)=a.position(work,620,h,pad);
                assert_eq!(x,a.point.x);
                assert_eq!(y+h,a.point.y-pad);
            }
            let left=DockAnchor{point:POINT{x:-1370,y:100},side:DockSide::Left};
            assert_eq!(left.position(work,620,200,pad).0,left.point.x+pad);
            let right=DockAnchor{point:POINT{x:-90,y:100},side:DockSide::Right};
            assert_eq!(right.position(work,620,200,pad).0+620,right.point.x-pad);
            let clamp=DockAnchor{point:POINT{x:-55,y:822},..a};
            assert_eq!(clamp.position(work,620,200,pad).0,work.right-620);
        }
    }
    #[test]
    fn dock_anchor_roundtrips_and_stays_on_negative_output(){
        let work=RECT{left:-1080,top:-731,right:0,bottom:1141};
        for side in [crate::dock::DockSide::Bottom,crate::dock::DockSide::Top,crate::dock::DockSide::Left,crate::dock::DockSide::Right]{
            let a=DockAnchor{point:POINT{x:-1035,y:-620},side};
            let (w,l)=a.encode();
            let decoded=DockAnchor::decode(w,l);
            assert_eq!(a.side,decoded.side);
            assert_eq!((decoded.point.x,decoded.point.y),(-1035,-620));
            let (x,y)=decoded.position(work,680,460,10);
            assert!(x>=work.left&&y>=work.top&&x+680<=work.right&&y+460<=work.bottom);
        }
    }
    fn sample_entries() -> Vec<Entry> {
        [("Paint", "C:\\Windows\\System32\\mspaint.exe"), ("Notepad", "C:\\Windows\\System32\\notepad.exe"), ("Budget", "C:\\Documents\\Budget.xlsx")]
            .into_iter().map(|(name,path)| Entry {name:name.into(),path:path.into(),kind:if name=="Budget"{"File"}else{"Application"},detail:String::new()}).collect()
    }
    #[test]
    fn recent_items_are_newest_first_unique_and_survive_reload() {
        let all = sample_entries(); let mut history = History::default();
        assert!(filter(&all,"",&history).is_empty());
        history.remember(&all[0].path); history.remember(&all[2].path); history.remember(&all[1].path);
        assert_eq!(filter(&all,"",&history),vec![1,2,0]);
        history.remember(Path::new("c:\\WINDOWS\\system32\\MSPAINT.exe"));
        assert_eq!(filter(&all,"  ",&History::from_json(&history.json())),vec![0,1,2]);
        assert!(!history.remember(&all[0].path));
        assert_eq!(history.paths.len(),3);
        // A removed/unindexed item must not produce a dangling result index.
        assert_eq!(filter(&all[..2],"",&history),vec![0,1]);
        assert_eq!(filter(&all,"paint",&history),vec![0]);
        assert_eq!(filter(&all,"missing",&history),Vec::<usize>::new());
    }
    #[test]
    fn only_apps_can_be_pinned_and_use_the_shared_dock_list(){
        let entries=sample_entries();
        assert!(pin_change(&entries[2]).is_none());
        assert!(matches!(pin_change(&entries[0]),Some(crate::dock::Edit::Pin(_))));
        dock_pins(&[entries[0].path.to_string_lossy().to_uppercase()]);
        assert!(matches!(pin_change(&entries[0]),Some(crate::dock::Edit::Unpin(_))));
        dock_pins(&[]);
    }
    #[test]
    fn recent_history_is_bounded_and_tolerates_invalid_data() {
        let mut history = History::default();
        for i in 0..45 { history.remember(Path::new(&format!("C:\\item-{i}.lnk"))); }
        assert_eq!(history.paths.len(),HISTORY_LIMIT);
        assert_eq!(history.paths[0],Path::new("C:\\item-44.lnk"));
        assert_eq!(history.paths[29],Path::new("C:\\item-15.lnk"));
        assert_eq!(History::from_json(&history.json()),history);
        assert_eq!(History::from_json(&serde_json::Value::Null),History::default());
        let history=History::from_json(&serde_json::json!({"items":[null,42,"","C:\\x","c:\\X","C:\\y"]}));
        assert_eq!(history.paths,vec![PathBuf::from("C:\\x"),PathBuf::from("C:\\y")]);
    }
    #[test]
    fn appearance_roundtrip_clamps_and_preserves_defaults() {
        let d=Appearance::from_json(&serde_json::Value::Null); assert_eq!(d,Appearance::default());
        let a=Appearance::from_json(&serde_json::json!({"theme":"dark","transparency":0.8,"blur":8}));
        assert_eq!(Appearance::from_json(&a.json()),a);
        let a=Appearance::from_json(&serde_json::json!({"theme":"system-wide","transparency":99,"blur":-1}));
        assert_eq!(a.theme,"light"); assert_eq!(a.transparency,0.85); assert_eq!(a.blur,4.0);
    }
    #[test]
    fn rounded_panel_has_fractional_edge_coverage() {
        assert_eq!(panel_coverage(0.5,0.5,660.0,444.0,20.0).0,0.0);
        assert_eq!(panel_coverage(330.0,222.0,660.0,444.0,20.0).0,1.0);
        assert!((0..40).any(|x| (0..40).any(|y| { let a=panel_coverage(x as f32+0.5,y as f32+0.5,660.0,444.0,20.0).0; a>0.0 && a<1.0 })));
    }
    #[test]
    fn spotlight_is_centered_on_the_physical_negative_monitor(){
        let display=RECT{left:-1200,top:-450,right:0,bottom:1470};
        let (x,y)=spotlight_origin(display,684,468);
        assert_eq!((x,y),(-942,276));
        assert_eq!((x*2+684,y*2+468),(display.left+display.right,display.top+display.bottom));
    }
    #[test]
    fn glass_theme_and_transparency_affect_background_only() {
        let raw=vec![90;16*12*4];
        let mut a=Appearance::default(); a.transparency=0.0;
        let light=frost(&raw,64,48,1.0,&a);
        a.theme="dark".into(); let dark=frost(&raw,64,48,1.0,&a);
        a.transparency=0.8; let clear=frost(&raw,64,48,1.0,&a);
        assert!(light[0]>dark[0]); assert!(clear[0]>dark[0]);
        assert!(light.chunks_exact(4).chain(dark.chunks_exact(4)).chain(clear.chunks_exact(4)).all(|p| p[3]==255));
    }
    #[test]
    fn native_compact_close_reverses_and_settles_without_losing_query() {
        unsafe {
            let _desktop=fullscreen::native_tests::Desktop::isolated();
            configure_spotlight(library::Preferences::default());
            toggle(std::ptr::null_mut());
            let (hwnd,_,_,_)=handles().unwrap();
            assert_eq!(COMPACT_MOTION.get().unwrap().to,1.0);
            SendMessageW(hwnd,WM_CHAR,'p' as WPARAM,0);
            toggle(std::ptr::null_mut());
            assert_eq!(COMPACT_MOTION.get().unwrap().to,0.0);
            toggle(std::ptr::null_mut());
            assert_eq!(COMPACT_MOTION.get().unwrap().to,1.0);
            assert_ne!(IsWindowVisible(hwnd),0);
            assert_eq!(STATE.with(|s|s.borrow().as_ref().unwrap().query.clone()),"p");
            SendMessageW(hwnd,WM_KEYDOWN,VK_ESCAPE as WPARAM,0);
            let mut closing=COMPACT_MOTION.get().unwrap();
            closing.start-=std::time::Duration::from_millis(u64::from(closing.duration)+50);
            COMPACT_MOTION.set(Some(closing));
            SendMessageW(hwnd,WM_TIMER,COMPACT_TWEEN,0);
            assert_eq!(IsWindowVisible(hwnd),0);
            assert!(COMPACT_MOTION.get().is_none()&&COMPACT_FRAME.with(|f|f.borrow().is_none()));
        }
    }
    #[test]
    fn native_search_enter_and_reopen_do_not_reenter_state() {
        unsafe {
            let _desktop = fullscreen::native_tests::Desktop::isolated();
            // Keep this keyboard/history test independent of the transition clock.
            configure_spotlight(library::Preferences{animations:false,..library::Preferences::default()});
            toggle(std::ptr::null_mut());
            let (hwnd, _, _, _) = handles().expect("native launcher window");
            for _ in 0..8 {
                for ch in "paint".chars() { SendMessageW(hwnd, WM_CHAR, ch as usize, 0); SendMessageW(hwnd, REFRESH, 0, 0); UpdateWindow(hwnd); }
                STATE.with(|c| { let g = c.borrow(); let s = g.as_ref().unwrap(); assert_eq!(s.all[s.matches[0]].name, "Paint"); assert_eq!(s.selected, 0); });
                SendMessageW(hwnd, WM_KEYDOWN, VK_RETURN as usize, 0);
                let mut msg: MSG = std::mem::zeroed();
                while PeekMessageW(&mut msg, hwnd, 0, 0, PM_REMOVE) != 0 { DispatchMessageW(&msg); }
                assert_eq!(IsWindowVisible(hwnd), 0);
                toggle(std::ptr::null_mut()); assert_ne!(IsWindowVisible(hwnd), 0);
                STATE.with(|c| { let g=c.borrow(); let s=g.as_ref().unwrap(); assert_eq!(s.matches.len(),1); assert_eq!(s.all[s.matches[0]].name,"Paint"); });
                SendMessageW(hwnd, WM_KEYDOWN, VK_DOWN as usize, 0); SendMessageW(hwnd, WM_PAINT, 0, 0);
                SendMessageW(hwnd, WM_KEYDOWN, VK_ESCAPE as usize, 0); toggle(std::ptr::null_mut());
            }
            for ch in "notepad".chars() { SendMessageW(hwnd, WM_CHAR, ch as usize, 0); SendMessageW(hwnd, REFRESH, 0, 0); }
            SendMessageW(hwnd, WM_KEYDOWN, VK_ESCAPE as usize, 0);
            toggle(std::ptr::null_mut());
            STATE.with(|c| { let g=c.borrow(); let s=g.as_ref().unwrap(); assert_eq!(s.matches.len(),2); assert_eq!(s.all[s.matches[0]].name,"Notepad"); assert_eq!(s.all[s.matches[1]].name,"Paint"); });
            DestroyWindow(hwnd); STATE.with(|c| *c.borrow_mut() = None);
        }
    }
}
