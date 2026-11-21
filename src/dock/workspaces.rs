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
struct Hidden { prior: WINDOWPLACEMENT, observed: bool, restoring: Option<Instant> }
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
impl Member{
    unsafe fn observe(&mut self) {
        let Some(hidden) = self.hidden.as_mut() else { return; };
        let iconic = IsIconic(self.key.handle()) != 0;
        if let Some(since) = hidden.restoring {
            if iconic { hidden.observed = true; }
            else if hidden.observed || since.elapsed() >= Duration::from_millis(250) {
                self.hidden = None;
            }
        } else if iconic {
            hidden.observed = true;
        } else if hidden.observed {
            // Once a user or the app has reopened it we no longer own the state.
            self.hidden = None;
        }
    }
    unsafe fn reveal(&mut self, leaving: bool) -> bool {
        if !self.key.live() { return false; }
        let Some(hidden) = self.hidden.as_mut() else { return false; };
        if hidden.restoring.is_some() { return true; }
        if hidden.observed && IsIconic(self.key.handle()) == 0 {
            self.hidden = None;
            return false;
        }
        let command = if hidden.prior.showCmd == SW_SHOWMAXIMIZED as u32 {
            SW_SHOWMAXIMIZED
        } else {
            SW_SHOWNOACTIVATE
        };
        // Post through the same placement path even when the minimize has not
        // been processed. Testing IsIconic alone loses rapid switch/exit races.
        if !request(self.key.handle(), hidden.prior, command) { return false; }
        hidden.restoring = Some(Instant::now());
        if leaving { self.hidden = None; }
        true
    }
    unsafe fn hide(&mut self) {
        let hwnd = self.key.handle();
        if !self.key.live() || IsWindowVisible(hwnd) == 0 { return; }
        let prior = match self.hidden.as_ref() {
            Some(hidden) if hidden.restoring.is_some() => Some(hidden.prior),
            Some(_) => return,
            None if IsIconic(hwnd) != 0 => return,
            None => placement(hwnd),
        };
        if let Some(prior) = prior {
            if request(hwnd, prior, SW_SHOWMINNOACTIVE) {
                self.hidden = Some(Hidden { prior, observed: false, restoring: None });
            }
        }
    }
}
struct Group{id:u64,screen:String,members:Vec<Member>}
#[derive(Default)]
pub struct Workspaces{groups:Vec<Group>,active:HashMap<String,u64>,sequence:u64}
impl Workspaces{
    pub unsafe fn prune(&mut self){
        self.groups.retain_mut(|g|{
            g.members.retain_mut(|m| {
                if !m.key.live() { return false; }
                m.observe();
                true
            });
            !g.members.is_empty()
        });
        self.active.retain(|screen,id|self.groups.iter().any(|g|g.id==*id&&g.screen==*screen));
    }
    pub fn active(&self,screen:&str)->Option<u64>{self.active.get(screen).copied()}
    pub fn title(id:u64)->String{format!("Nhóm {id}")}
    pub fn can_create(&self,screen:&str)->bool {
        self.groups.len()<20 && self.groups.iter().filter(|g|g.screen==screen).count()<4
    }
    pub unsafe fn create(&mut self,screen:&str,hwnd:HWND)->Option<u64>{
        if !self.can_create(screen){return None;}
        let key=Key::read(hwnd)?;
        self.detach(key);
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
        self.detach(key);
        if let Some(g)=self.groups.iter_mut().find(|g|g.id==id){
            g.members.push(Member{key,hidden:None});
        }
    }
    unsafe fn detach(&mut self,key:Key){
        for g in &mut self.groups{
            if let Some(i)=g.members.iter().position(|m|m.key==key){
                g.members.remove(i).reveal(true);
            }
        }
        self.prune();
    }
    pub unsafe fn remove(&mut self,hwnd:HWND){
        if let Some(key)=Key::read(hwnd){self.detach(key);}
    }
    pub unsafe fn dissolve(&mut self,id:u64){
        self.groups.retain_mut(|g|{
            if g.id!=id{return true;}
            for member in &mut g.members{member.reveal(true);}
            false
        });
        self.active.retain(|_,active|*active!=id);
    }
    pub unsafe fn switch(&mut self,screen:&str,id:u64)->Option<HWND>{
        if !self.groups.iter().any(|g|g.screen==screen&&g.id==id){return None;}
        self.active.insert(screen.into(),id);
        let mut focus=None;
        for g in self.groups.iter_mut().filter(|g|g.screen==screen){
            for member in &mut g.members{
                if g.id==id{
                    let will_activate=member.reveal(false);
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
                for member in &mut group.members{member.reveal(true);}
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
        items.truncate(5);
        ids.truncate(5);
        (items,ids)
    }
}
impl Drop for Workspaces{
    fn drop(&mut self){
        unsafe{self.restore_offscreen(&HashSet::new());}
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
            assert_eq!(IsIconic(normal), 0, "quick Stage OFF leaked a minimize");
            assert_ne!(IsZoomed(maximized), 0);
            assert_ne!(IsIconic(already_minimized), 0);

            model.switch("primary", b);
            until(|| IsIconic(normal) != 0 && IsIconic(maximized) != 0);
            assert_eq!(IsIconic(other_display), 0);
            model.switch("primary", a);
            until(|| IsIconic(normal) == 0 && IsZoomed(maximized) != 0 && IsIconic(second_group) != 0);
            assert!(super::super::same_rect(placement(normal).unwrap().rcNormalPosition, original));
            assert!(super::super::same_rect(placement(maximized).unwrap().rcNormalPosition, max_original));
            assert_eq!(super::super::MonitorFromWindow(normal, super::super::MONITOR_DEFAULTTONEAREST), monitor);

            // Never re-hide a member that the user has reopened on their own.
            model.prune();
            ShowWindowAsync(second_group, SW_SHOWNOACTIVATE);
            until(|| IsIconic(second_group) == 0);
            model.prune();
            drop(model);
            assert_eq!(IsIconic(second_group), 0);
            assert_ne!(IsIconic(already_minimized), 0);
        }
    }
}
