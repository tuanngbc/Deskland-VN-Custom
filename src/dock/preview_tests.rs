use super::*;
use crate::desktop::wide;
use std::{
    env, ptr,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::System::{
    LibraryLoader::GetModuleHandleW,
    ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX},
    Threading::{GetCurrentProcess, GetProcessHandleCount},
};

const TEST: &str = "dock::previews::native_tests::native_preview_relationships_survive_switches_and_release";
const CLASS: &str = "TuanpcDeskPreviewTestOwned";
const CHILD: &str = "TUANPC_DESK_PREVIEW_FIXTURE";
const RESIZE: u32 = WM_APP + 118;
const COUNT: usize = 9;

struct Fixture(Child);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Window(HWND);
impl Drop for Window {
    fn drop(&mut self) {
        unsafe { DestroyWindow(self.0); }
    }
}

unsafe extern "system" fn fixture_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == RESIZE {
        SetWindowPos(hwnd, ptr::null_mut(), 0, 0, 110, 360, SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOMOVE);
        return 0;
    }
    DefWindowProcW(hwnd, msg, w, l)
}

unsafe fn register() {
    let mut class: WNDCLASSW = mem::zeroed();
    let name = wide(CLASS);
    class.lpszClassName = name.as_ptr();
    class.lpfnWndProc = Some(fixture_proc);
    class.hInstance = GetModuleHandleW(ptr::null());
    class.hbrBackground = GetSysColorBrush(COLOR_WINDOW);
    assert_ne!(RegisterClassW(&class), 0);
}

unsafe fn window(title: &str, layered: bool) -> Window {
    let ex = WS_EX_NOACTIVATE | if layered { WS_EX_LAYERED } else { WS_EX_TOOLWINDOW };
    let hwnd = CreateWindowExW(ex, wide(CLASS).as_ptr(), wide(title).as_ptr(), WS_POPUP,
        38, 80, 360, 180, ptr::null_mut(), ptr::null_mut(), GetModuleHandleW(ptr::null()), ptr::null());
    assert!(!hwnd.is_null());
    if layered {
        assert_ne!(SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA), 0);
    }
    Window(hwnd)
}

fn until(label: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(6);
    while !ready() {
        assert!(Instant::now() < deadline, "{label}");
        thread::sleep(Duration::from_millis(15));
    }
}

unsafe fn resources() -> (u32, usize) {
    let mut count = 0;
    assert_ne!(GetProcessHandleCount(GetCurrentProcess(), &mut count), 0);
    let mut memory: PROCESS_MEMORY_COUNTERS_EX = mem::zeroed();
    memory.cb = mem::size_of_val(&memory) as u32;
    assert_ne!(GetProcessMemoryInfo(GetCurrentProcess(), (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast(), memory.cb), 0);
    (count, memory.PrivateUsage)
}

unsafe fn require_released(ids: &[isize]) {
    for &id in ids {
        let mut size: SIZE = mem::zeroed();
        assert!(DwmQueryThumbnailSourceSize(id, &mut size) < 0, "unregistered thumbnail still valid");
    }
}

#[test]
#[ignore = "live Windows compositor; explicitly run in CI"]
fn native_preview_relationships_survive_switches_and_release() {
    unsafe {
        register();
        if let Ok(prefix) = env::var(CHILD) {
            let windows = (0..COUNT).map(|i| window(&format!("{prefix}-{i}"), true)).collect::<Vec<_>>();
            assert_ne!(SetWindowDisplayAffinity(windows[7].0, WDA_EXCLUDEFROMCAPTURE), 0);
            for (i, w) in windows.iter().enumerate() {
                ShowWindow(w.0, if i == 8 { SW_SHOWMINNOACTIVE } else { SW_SHOWNOACTIVATE });
            }
            let until = Instant::now() + Duration::from_secs(38);
            let mut msg: MSG = mem::zeroed();
            while Instant::now() < until {
                while PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    DispatchMessageW(&msg);
                }
                thread::sleep(Duration::from_millis(5));
            }
            return;
        }
        let prefix = format!("tuanpc-stage-preview-{}", std::process::id());
        let fixture = Fixture(Command::new(env::current_exe().unwrap())
            .args(["--ignored", "--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD, &prefix).stdin(Stdio::null()).spawn().unwrap());
        let find = |i| FindWindowW(wide(CLASS).as_ptr(), wide(&format!("{prefix}-{i}")).as_ptr());
        until("foreign thumbnails are visible and protected/minimized fixtures ready", || {
            IsWindowVisible(find(7)) != 0 && IsIconic(find(8)) != 0
        });
        let items = (0..COUNT).map(|i| {
            let hwnd = find(i);
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            assert_eq!(pid, fixture.0.id());
            Item { path: String::new(), title: format!("test card {i}"), hwnd, count: 1, pinned: false }
        }).collect::<Vec<_>>();
        until("Windows permits preview for each public fixture", ||
            items[..7].iter().all(|item| preview_pid(item.hwnd) == Some(fixture.0.id())));
        let mut affinity = 0;
        assert_ne!(GetWindowDisplayAffinity(items[7].hwnd, &mut affinity), 0);
        assert_eq!(affinity, WDA_EXCLUDEFROMCAPTURE);
        assert!(preview_pid(items[7].hwnd).is_none() && preview_pid(items[8].hwnd).is_none());

        let dst = window("test compositor destination", false);
        let replacement = window("test replacement destination", false);
        let layout = Layout::new(180, 880, 96, 6, 8, COUNT, false);
        let mut state = Previews::default();
        assert!(state.update(dst.0, &items, layout));
        assert_eq!(state.links.len(), 7);
        assert!(!state.includes(items[7].hwnd) && !state.includes(items[8].hwnd));
        assert_eq!(state.benchmark_counts(fixture.0.id(), 0), (7, 0));
        assert_eq!(state.benchmark_counts(fixture.0.id(), items[0].hwnd as usize), (6, 1));
        assert_eq!(state.benchmark_counts(std::process::id(), 0), (0, 7));
        let key = items[0].hwnd as usize;
        let first = state.links[&key].area;
        let stable = state.links[&key].raw;
        assert_ne!(PostMessageW(items[0].hwnd, RESIZE, 0, 0), 0);
        until("DWM source resize updates the letterboxed rectangle", || {
            state.update(dst.0, &items, layout);
            !same_rect(first, state.links[&key].area)
        });
        assert_eq!(stable, state.links[&key].raw, "resize should not re-register a live link");
        let ids = state.links.values().map(|l| l.raw).collect::<Vec<_>>();
        assert!(state.update(replacement.0, &items, layout));
        let not_reused = ids.into_iter().filter(|id| !state.links.values().any(|link| link.raw == *id)).collect::<Vec<_>>();
        require_released(&not_reused);
        assert_eq!(state.destination, replacement.0 as usize);
        assert_eq!(state.links.len(), 7);
        state.clear();

        // Same 240 card switches on the same DWM fixtures. "Cold" mimics
        // the former throw-away insertion map; count map buffers that must
        // grow and report private bytes without an unstable memory threshold.
        let group_a = &items[..5];
        let group_b = &items[2..7];
        let run = |reuse: bool| -> (usize, u32, isize) {
            let mut state = Previews::default();
            state.update(dst.0, group_a, layout);
            state.update(dst.0, group_b, layout);
            let start = resources();
            let mut grows = 0;
            for i in 0..240 {
                if !reuse { state.next = HashMap::new(); }
                let capacity = state.next.capacity();
                let group = if i % 2 == 0 { group_a } else { group_b };
                let stale = state.links.iter().filter(|(key, _)| !group.iter().any(|item| item.hwnd as usize == **key))
                    .map(|(_, link)| link.raw).collect::<Vec<_>>();
                assert!(state.update(dst.0, group, layout));
                assert_eq!(state.links.len(), 5);
                assert!(group.iter().all(|item| state.includes(item.hwnd)));
                assert!(state.next.is_empty());
                grows += usize::from(state.links.capacity() > capacity);
                require_released(&stale);
            }
            let final_ids = state.links.values().map(|l| l.raw).collect::<Vec<_>>();
            if reuse { drop(state); } else { assert!(state.clear()); assert!(!state.clear()); }
            require_released(&final_ids);
            let end = resources();
            (grows, end.0.saturating_sub(start.0), end.1 as isize - start.1 as isize)
        };
        let cold = run(false);
        let reused = run(true);
        assert_eq!(cold.0, 240);
        assert_eq!(reused.0, 0);
        assert!(reused.1 <= 8, "steady state handles grew: {reused:?}");
        eprintln!("DWM live stress: 240 switches per mode; (map growths, handle delta, private byte delta) cold={cold:?} reused={reused:?}");
    }
}
