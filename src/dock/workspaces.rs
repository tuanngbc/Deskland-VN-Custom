//! Session-local workspaces. Only explicitly grouped windows can be hidden.
//! Minimize only windows Stage hid itself; Windows retains their native monitor
//! and normal geometry in WINDOWPLACEMENT. Never capture or move app data.
use std::{collections::{HashMap,HashSet},mem,time::{Duration,Instant}};
use windows_sys::Win32::{Foundation::*,UI::WindowsAndMessaging::*};
use super::{AppWindow,Item,rect_intersects};

#[derive(Clone,Copy,PartialEq,Eq,Hash)]
struct Key{hwnd:usize,pid:u32,tid:u32}
impl Key{
    unsafe fn read(hwnd:HWND)->Option<Self>{
        let mut pid=0;
        let tid=GetWindowThreadProcessId(hwnd,&mut pid);
        if tid==0||pid==0||pid==std::process::id(){None}else{Some(Self{hwnd:hwnd as usize,pid,tid})}
    }
    fn handle(self)->HWND{self.hwnd as HWND}
    unsafe fn live(self)->bool{Self::read(self.handle())==Some(self)&&IsWindow(self.handle())!=0}
}
const TRANSITION_LIMIT: Duration = Duration::from_millis(1200);

#[derive(Clone, Copy)]
enum Phase { Minimizing(Instant), Hidden, Restoring(Instant) }
struct Hidden { prior: WINDOWPLACEMENT, phase: Phase, want_hidden: bool }
struct Member { key: Key, hidden: Option<Hidden> }

unsafe fn placement(hwnd: HWND) -> Option<WINDOWPLACEMENT> {
    let mut value: WINDOWPLACEMENT = mem::zeroed();
    value.length = mem::size_of::<WINDOWPLACEMENT>() as u32;
    (GetWindowPlacement(hwnd, &mut value) != 0).then_some(value)
}
unsafe fn request(hwnd: HWND, mut value: WINDOWPLACEMENT, command: i32) -> bool {
    value.flags = WPF_ASYNCWINDOWPLACEMENT;
    value.showCmd = command as u32;
    SetWindowPlacement(hwnd, &value) != 0
}
fn restore_command(prior: &WINDOWPLACEMENT) -> i32 {
    if prior.showCmd == SW_SHOWMAXIMIZED as u32 { SW_SHOWMAXIMIZED } else { SW_SHOWNOACTIVATE }
}
impl Member {
    /// Only arm the short transition timer while a request is outstanding.
    /// A restore is issued *after* the target window acknowledges minimize.
    /// Posting both up front can leave the final state dependent on delivery.
    unsafe fn advance(&mut self) -> bool {
        let hwnd = self.key.handle();
        if !self.key.live() { self.hidden = None; return false; }
        let Some(state) = self.hidden.as_mut() else { return false; };
        let iconic = IsIconic(hwnd) != 0;
        match state.phase {
            Phase::Minimizing(since) if !iconic => {
                if since.elapsed() < TRANSITION_LIMIT { return true; }
                if !state.want_hidden {
                    // Last chance for an unresponsive owner; ordinary paths
                    // restore only after IsIconic confirms the minimize.
                    request(hwnd, state.prior, restore_command(&state.prior));
                }
                self.hidden = None;
                return false;
            }
            Phase::Minimizing(_) => state.phase = Phase::Hidden,
            Phase::Restoring(since) => {
                if iconic && since.elapsed() < TRANSITION_LIMIT { return true; }
                let rehide = state.want_hidden && !iconic;
                if iconic {
                    // Keep ownership so the next explicit switch/Exit can
                    // retry, but do not keep a fast timer on a hung HWND.
                    state.phase = Phase::Hidden;
                    return false;
                }
                self.hidden = None;
                if rehide { self.hide(); }
                return rehide && self.hidden.is_some();
            }
            Phase::Hidden if !iconic => {
                // A manual reopen releases our ownership. Do not re-minimize
                // the window on a passive desktop refresh.
                self.hidden = None;
                return false;
            }
            Phase::Hidden => {}
        }
        if state.want_hidden { return false; }
        if request(hwnd, state.prior, restore_command(&state.prior)) {
            state.phase = Phase::Restoring(Instant::now());
            true
        } else {
            false
        }
    }

    unsafe fn reveal(&mut self) -> bool {
        let Some(state) = self.hidden.as_mut() else { return false; };
        state.want_hidden = false;
        self.advance();
        self.hidden.is_some()
    }
    unsafe fn hide(&mut self) {
        let hwnd = self.key.handle();
        if !self.key.live() || IsWindowVisible(hwnd) == 0 { return; }
        if let Some(state) = self.hidden.as_mut() {
            state.want_hidden = true;
            self.advance();
            return;
        }
        if IsIconic(hwnd) != 0 { return; }
        if let Some(prior) = placement(hwnd) {
            if request(hwnd, prior, SW_SHOWMINNOACTIVE) {
                self.hidden = Some(Hidden { prior, phase: Phase::Minimizing(Instant::now()), want_hidden: true });
            }
        }
    }
    fn pending(&self) -> bool {
        self.hidden.as_ref().is_some_and(|state| !matches!(state.phase, Phase::Hidden))
    }
    unsafe fn request_final_restore(&self) {
        if let Some(state) = &self.hidden {
            if self.key.live() {
                request(self.key.handle(), state.prior, restore_command(&state.prior));
            }
        }
    }
}
struct Group{id:u64,screen:String,members:Vec<Member>}
#[derive(Default)]
pub struct Workspaces{groups:Vec<Group>,active:HashMap<String,u64>,sequence:u64, detaching:Vec<Member>}
impl Workspaces{
    pub unsafe fn prune(&mut self){
        self.advance();
        self.groups.retain_mut(|g|{
            g.members.retain(|m|m.key.live());
            !g.members.is_empty()
        });
        self.active.retain(|screen,id|self.groups.iter().any(|g|g.id==*id&&g.screen==*screen));
    }
    pub fn pending(&self) -> bool {
        self.groups.iter().flat_map(|group| &group.members).chain(&self.detaching).any(Member::pending)
    }
    pub unsafe fn advance(&mut self) -> bool {
        for group in &mut self.groups {
            for member in &mut group.members {
                if member.pending() || member.hidden.as_ref().is_some_and(|state|state.want_hidden) {
                    member.advance();
                }
            }
        }
        self.detaching.retain_mut(|member| {
            member.advance();
            member.pending()
        });
        self.pending()
    }
    pub unsafe fn recover_for_exit(&mut self) {
        self.restore_offscreen(&HashSet::new());
        let start = Instant::now();
        while self.advance() && start.elapsed() < Duration::from_millis(400) {
            // Only on final shutdown, and only while a foreign placement
            // request is in flight. No UI-thread wait on the target process.
            std::thread::sleep(Duration::from_millis(10));
        }
        for member in self.groups.iter().flat_map(|group|&group.members).chain(&self.detaching) {
            member.request_final_restore();
        }
        self.groups.clear();
        self.detaching.clear();
    }
    pub fn active(&self,screen:&str)->Option<u64>{self.active.get(screen).copied()}
    pub fn title(id:u64)->String{format!("Nhóm {id}")}
    pub fn can_create(&self,screen:&str)->bool {
        self.groups.len()<20 && self.groups.iter().filter(|g|g.screen==screen).count()<4
    }
    pub unsafe fn create(&mut self,screen:&str,hwnd:HWND)->Option<u64>{
        if !self.can_create(screen){return None;}
        let key=Key::read(hwnd)?;
        if self.detaching.iter().any(|m| m.key==key)
            || self.groups.iter().any(|g| g.members.iter().any(|m| m.key==key)) {return None;}
        self.sequence=self.sequence.wrapping_add(1).max(1);
        let id=self.sequence;
        self.groups.push(Group{id,screen:screen.into(),members:vec![Member{key,hidden:None}]});
        self.switch(screen,id);
        Some(id)
    }
    pub unsafe fn add(&mut self,screen:&str,hwnd:HWND){
        let Some(id)=self.active(screen)else{return;};
        let Some(key)=Key::read(hwnd)else{return;};
        if self.groups.iter().any(|g|g.id==id&&g.members.iter().any(|m|m.key==key)){return;}
        if !self.groups.iter().any(|g|g.id==id&&g.members.len()<12){return;}
        if self.detaching.iter().any(|m| m.key==key)
            || self.groups.iter().any(|g| g.members.iter().any(|m| m.key==key)) {return;}
        if let Some(g)=self.groups.iter_mut().find(|g|g.id==id){
            g.members.push(Member{key,hidden:None});
        }
    }
    unsafe fn detach(&mut self,key:Key){
        for g in &mut self.groups{
            if let Some(i)=g.members.iter().position(|m|m.key==key){
                let mut member=g.members.remove(i);
                member.reveal();
                if member.pending(){self.detaching.push(member);}
            }
        }
        self.prune();
    }
    pub unsafe fn remove(&mut self,hwnd:HWND){
        if let Some(key)=Key::read(hwnd){self.detach(key);}
    }
    pub unsafe fn dissolve(&mut self,id:u64){
        if let Some(i)=self.groups.iter().position(|g|g.id==id){
            for mut member in self.groups.remove(i).members {
                member.reveal();
                if member.pending(){self.detaching.push(member);}
            }
        }
        self.active.retain(|_,active|*active!=id);
    }
    pub unsafe fn switch(&mut self,screen:&str,id:u64)->Option<HWND>{
        if !self.groups.iter().any(|g|g.screen==screen&&g.id==id){return None;}
        self.active.insert(screen.into(),id);
        let mut focus=None;
        for g in self.groups.iter_mut().filter(|g|g.screen==screen){
            for member in &mut g.members{
                if g.id==id{
                    let will_activate=member.reveal();
                    if focus.is_none()&&member.key.live()&&(will_activate||IsIconic(member.key.handle())==0){
                        focus=Some(member.key.handle());
                    }
                }else{member.hide();}
            }
        }
        focus
    }
    pub unsafe fn restore_offscreen(&mut self,enabled:&HashSet<String>){
        for group in &mut self.groups{
            if !enabled.contains(&group.screen){
                for member in &mut group.members{member.reveal();}
            }
        }
        self.active.retain(|screen,_|enabled.contains(screen));
    }
    pub fn cards(&self,screen:&str,bounds:RECT,windows:&[AppWindow],global:bool)->(Vec<Item>,Vec<Option<u64>>){
        let mut items=Vec::new();
        let mut ids=Vec::new();
        let known:HashSet<_>=self.groups.iter().flat_map(|g|g.members.iter().map(|m|m.key.hwnd)).collect();
        let active=self.active(screen);
        let mut groups:Vec<_>=self.groups.iter().filter(|g|g.screen==screen).collect();
        groups.sort_by_key(|g|(Some(g.id)!=active,g.id));
        for g in groups{
            // Use the first available window in current native Z-order.
            if let Some(window)=windows.iter().find(|w|g.members.iter().any(|m|m.key.hwnd==w.hwnd as usize)){
                items.push(Item{path:window.path.clone(),title:Self::title(g.id),hwnd:window.hwnd,count:g.members.len(),pinned:false});
                ids.push(Some(g.id));
            }
        }
        for w in windows.iter().filter(|w|!known.contains(&(w.hwnd as usize))&&(global||rect_intersects(w.rect,bounds))){
            items.push(Item{path:w.path.clone(),title:w.title.clone(),hwnd:w.hwnd,count:1,pinned:false});
            ids.push(None);
        }
        (items,ids)
    }
}
impl Drop for Workspaces{
    fn drop(&mut self){
        unsafe{self.recover_for_exit();}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::wide;
    use std::{env, path::PathBuf, process::{Child, Command, Stdio}, ptr, thread};

    const HELPER: &str = "dock::workspaces::tests::foreign_window_fixture";
    const ACK: u32 = WM_APP + 90;
    const ACK_PROP: &str = "tuanpc-desk-ci-placement-ack";

    unsafe extern "system" fn fixture_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        if msg == ACK {
            SetPropW(hwnd, wide(ACK_PROP).as_ptr(), w as HANDLE);
            return 0;
        }
        DefWindowProcW(hwnd, msg, w, l)
    }

    #[test]
    #[ignore = "only the foreign-window integration test starts this fixture"]
    fn foreign_window_fixture() {
        let Some(prefix) = env::var_os("TUANPC_DOCK_TEST_FIXTURE") else { return; };
        let gate = PathBuf::from(env::var_os("TUANPC_DOCK_TEST_GATE").unwrap());
        let prefix = prefix.to_string_lossy();
        unsafe {
            let class = wide("TuanpcDockStageCiWindow");
            let instance = super::super::GetModuleHandleW(ptr::null());
            let mut wc: WNDCLASSW = mem::zeroed();
            wc.lpszClassName = class.as_ptr();
            wc.hInstance = instance;
            wc.lpfnWndProc = Some(fixture_proc);
            assert_ne!(RegisterClassW(&wc), 0);
            for (i, state) in [SW_SHOWNOACTIVATE, SW_SHOWMAXIMIZED, SW_SHOWMINNOACTIVE,
                SW_SHOWNOACTIVATE, SW_SHOWNOACTIVATE].into_iter().enumerate() {
                let h = CreateWindowExW(0, class.as_ptr(),wide(&format!("{prefix}-{i}")).as_ptr(),
                    WS_OVERLAPPEDWINDOW, 60+i as i32*20, 80, 360, 260, ptr::null_mut(), ptr::null_mut(), instance, ptr::null());
                assert!(!h.is_null());
                ShowWindow(h, state);
            }
            // Force the parent to queue minimize and restore before delivery.
            until(|| gate.exists());
            let end = Instant::now() + Duration::from_secs(15);
            let mut msg: MSG = mem::zeroed();
            while Instant::now() < end {
                while PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    DispatchMessageW(&msg);
                }
                thread::sleep(Duration::from_millis(5));
            }
        }
    }

    fn until(mut ready: impl FnMut() -> bool) {
        let start = Instant::now();
        while !ready() {
            assert!(start.elapsed() < Duration::from_secs(5), "fixture operation timed out");
            thread::sleep(Duration::from_millis(10));
        }
    }

    struct Fixture(Child, PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
            let _ = std::fs::remove_file(&self.1);
        }
    }

    #[test]
    #[ignore = "requires a Windows desktop with foreign HWNDs"]
    fn native_stage_switch_restores_placement_and_ignores_unmanaged_windows() {
        let unique = format!("tuanpc-ci-stage-{}", std::process::id());
        let gate = env::temp_dir().join(format!("{unique}.release"));
        let _ = std::fs::remove_file(&gate);
        let fixture = Fixture(Command::new(env::current_exe().unwrap())
            .args(["--ignored", "--exact", HELPER, "--nocapture"])
            .env("TUANPC_DOCK_TEST_FIXTURE", &unique).env("TUANPC_DOCK_TEST_GATE", &gate)
            .stdin(Stdio::null()).spawn().unwrap(), gate);
        unsafe {
            let window = |i| FindWindowW(ptr::null(), wide(&format!("{unique}-{i}")).as_ptr());
            until(|| !window(4).is_null() && IsWindowVisible(window(4)) != 0);
            let [normal, maximized, already_minimized, second_group, other_display] =
                [0, 1, 2, 3, 4].map(window);
            for h in [normal, maximized, already_minimized, second_group, other_display] {
                assert_eq!(Key::read(h).unwrap().pid, fixture.0.id());
            }
            let original = placement(normal).unwrap().rcNormalPosition;
            let max_original = placement(maximized).unwrap().rcNormalPosition;
            let monitor = super::super::MonitorFromWindow(normal, super::super::MONITOR_DEFAULTTONEAREST);
            assert_ne!(IsZoomed(maximized), 0);
            assert_ne!(IsIconic(already_minimized), 0);
            let mut model = Workspaces::default();
            let a = model.create("primary", normal).unwrap();
            model.add("primary", maximized);
            model.add("primary", already_minimized);
            model.create("portrait", other_display).unwrap();
            let b = model.create("primary", second_group).unwrap();
            assert_eq!(IsIconic(normal), 0, "fixture must not have processed the minimize");
            model.restore_offscreen(&HashSet::from(["portrait".to_owned()]));
            assert_ne!(PostMessageW(normal, ACK, 1, 0), 0);
            std::fs::write(&fixture.1, b"resume").unwrap();
            until(|| GetPropW(normal, wide(ACK_PROP).as_ptr()) as usize == 1);
            until(|| !model.advance() && IsIconic(normal) == 0 && IsZoomed(maximized) != 0);
            assert_ne!(IsZoomed(maximized), 0);
            assert_ne!(IsIconic(already_minimized), 0);

            model.switch("primary", b);
            until(|| {model.advance(); IsIconic(normal) != 0 && IsIconic(maximized) != 0});
            assert_eq!(IsIconic(other_display), 0);
            model.switch("primary", a);
            until(|| {model.advance(); IsIconic(normal) == 0 && IsZoomed(maximized) != 0 && IsIconic(second_group) != 0});
            assert!(super::super::same_rect(placement(normal).unwrap().rcNormalPosition, original));
            assert!(super::super::same_rect(placement(maximized).unwrap().rcNormalPosition, max_original));
            assert_eq!(super::super::MonitorFromWindow(normal, super::super::MONITOR_DEFAULTTONEAREST), monitor);

            // Never re-hide a member that the user has reopened on their own.
            model.prune();
            ShowWindowAsync(second_group, SW_SHOWNOACTIVATE);
            until(|| IsIconic(second_group) == 0);
            model.prune();
            assert_eq!(IsIconic(second_group), 0);
            model.switch("primary", b);
            until(|| { model.advance(); IsIconic(normal) != 0 && IsIconic(maximized) != 0 });
            drop(model);
            until(|| IsIconic(normal) == 0 && IsZoomed(maximized) != 0);
            assert_eq!(IsIconic(second_group), 0);
            assert_ne!(IsIconic(already_minimized), 0);
        }
    }
}
