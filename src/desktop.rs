//! Tích hợp màn hình nền Windows - port từ host/DesktopHost.cs:
//! gắn cửa sổ dưới lớp biểu tượng (Progman/WorkerW), giữ thứ tự lớp, đọc khung biểu tượng
//! của SysListView32, và phát hiện cửa sổ phóng to / toàn màn hình để tạm dừng.

use std::ffi::c_void;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::Diagnostics::Debug::*;
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// An explicit Settings request must work even for a process whose launch
/// STARTUPINFO specified SW_HIDE. The first ShowWindow can honor that flag
/// instead of the requested show state.
pub(crate) unsafe fn show_settings_window(hwnd: HWND) -> bool {
    if IsIconic(hwnd)!=0 { ShowWindow(hwnd,SW_RESTORE); }
    let shown=SetWindowPos(hwnd,std::ptr::null_mut(),0,0,0,0,
        SWP_NOMOVE|SWP_NOSIZE|SWP_NOZORDER|SWP_SHOWWINDOW)!=0 && IsWindowVisible(hwnd)!=0;
    if shown {SetForegroundWindow(hwnd);}
    shown
}

fn find_window(class: &str) -> HWND {
    let c = wide(class);
    unsafe { FindWindowW(c.as_ptr(), std::ptr::null()) }
}

fn find_child(parent: HWND, after: HWND, class: &str) -> HWND {
    let c = wide(class);
    unsafe { FindWindowExW(parent, after, c.as_ptr(), std::ptr::null()) }
}

fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), 64) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

unsafe extern "system" fn enum_find_defview_host(h: HWND, l: LPARAM) -> BOOL {
    let out = &mut *(l as *mut HWND);
    if !find_child(h, std::ptr::null_mut(), "SHELLDLL_DefView").is_null() {
        *out = h;
        return 0;
    }
    1
}

fn find_classic_worker() -> HWND {
    let mut host: HWND = std::ptr::null_mut();
    unsafe { EnumWindows(Some(enum_find_defview_host), &mut host as *mut HWND as LPARAM) };
    if host.is_null() {
        return std::ptr::null_mut();
    }
    find_child(std::ptr::null_mut(), host, "WorkerW")
}

fn request_worker_w(progman: HWND, ready: impl Fn() -> bool) {
    unsafe {
        let mut res = 0usize;
        SendMessageTimeoutW(progman, 0x052C, 0xD, 1, SMTO_NORMAL, 1000, &mut res);
    }
    for _ in 0..20 {
        if ready() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

pub struct Attach {
    pub parent: HWND,
    pub z_anchor: HWND,
    pub raised: bool,
}

/// Gắn `hwnd` vào lớp hình nền, phủ `screen` (toạ độ màn hình ảo, điểm ảnh vật lý).
pub fn attach(hwnd: HWND, screen: RECT) -> Option<Attach> {
    unsafe {
        let progman = find_window("Progman");
        if progman.is_null() {
            crate::log!("Không tìm thấy Progman");
            return None;
        }
        let def_view = find_child(progman, std::ptr::null_mut(), "SHELLDLL_DefView");
        let raised = !def_view.is_null();
        let (parent, insert_after, z_anchor, mode);
        if raised {
            if find_child(progman, std::ptr::null_mut(), "WorkerW").is_null() {
                request_worker_w(progman, || !find_child(progman, std::ptr::null_mut(), "WorkerW").is_null());
            }
            parent = progman;
            insert_after = def_view;
            z_anchor = def_view;
            mode = "24H2+: cửa sổ con phân lớp của Progman, nằm dưới lớp biểu tượng";
        } else {
            let mut worker = find_classic_worker();
            if worker.is_null() {
                request_worker_w(progman, || !find_classic_worker().is_null());
                worker = find_classic_worker();
            }
            if worker.is_null() {
                crate::log!("Không tìm thấy WorkerW / SHELLDLL_DefView");
                return None;
            }
            parent = worker;
            insert_after = std::ptr::null_mut();
            z_anchor = std::ptr::null_mut();
            mode = "Classic: cửa sổ con WorkerW cấp cao nhất";
        }
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let style = (style & !(WS_POPUP | WS_CAPTION | WS_THICKFRAME)) | WS_CHILD | WS_VISIBLE;
        SetWindowLongW(hwnd, GWL_STYLE, style as i32);
        SetParent(hwnd, parent);
        if raised {
            // Bật WS_EX_LAYERED SAU khi đã là cửa sổ con. Nếu bật lúc còn là cửa sổ cấp cao nhất rồi
            // mới SetParent, Windows vẫn giữ nội dung (PrintWindow thấy hình) nhưng DWM không đưa lên
            // màn hình -> desktop đen (đã kiểm trên 26300). Tắt rồi bật lại để chắc chắn.
            let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) & !(WS_EX_LAYERED as i32);
            SetWindowLongW(hwnd, GWL_EXSTYLE, ex);
            SetWindowLongW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED as i32);
            SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
        }
        let mut pt = POINT { x: screen.left, y: screen.top };
        ScreenToClient(parent, &mut pt);
        SetWindowPos(
            hwnd,
            insert_after,
            pt.x,
            pt.y,
            screen.right - screen.left,
            screen.bottom - screen.top,
            SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED,
        );
        let a = Attach { parent, z_anchor, raised };
        ensure_z_order(hwnd, &a);
        crate::log!(
            "Đã gắn ({mode}), màn hình [{},{} {}x{}]",
            screen.left,
            screen.top,
            screen.right - screen.left,
            screen.bottom - screen.top
        );
        Some(a)
    }
}

pub fn ensure_z_order(hwnd: HWND, a: &Attach) {
    unsafe {
        if !a.raised || a.z_anchor.is_null() || IsWindow(a.z_anchor) == 0 {
            return;
        }
        let mut below = false;
        let mut cur = GetWindow(a.z_anchor, GW_HWNDNEXT);
        while !cur.is_null() {
            if cur == hwnd {
                below = true;
                break;
            }
            cur = GetWindow(cur, GW_HWNDNEXT);
        }
        if !below {
            SetWindowPos(hwnd, a.z_anchor, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        let mut w = find_child(a.parent, std::ptr::null_mut(), "WorkerW");
        while !w.is_null() {
            if !GetWindow(w, GW_HWNDNEXT).is_null() {
                SetWindowPos(w, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
            w = find_child(a.parent, w, "WorkerW");
        }
    }
}

pub fn detach(hwnd: HWND, a: &Attach) {
    unsafe {
        SetParent(hwnd, std::ptr::null_mut());
        ShowWindow(hwnd, SW_HIDE);
        if IsWindow(a.parent) != 0 {
            RedrawWindow(a.parent, std::ptr::null(), std::ptr::null_mut(), RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW);
        }
    }
}

// ---------------------------------------------------------------- Đọc biểu tượng màn hình nền
pub struct IconReader {
    list_view: HWND,
    proc: HANDLE,
    mem: *mut c_void,
}

const LVM_GETITEMCOUNT: u32 = 0x1004;
const LVM_GETITEMRECT: u32 = 0x100E;

impl IconReader {
    pub fn new() -> Self {
        IconReader { list_view: std::ptr::null_mut(), proc: std::ptr::null_mut(), mem: std::ptr::null_mut() }
    }

    fn find_list_view() -> HWND {
        let progman = find_window("Progman");
        let mut def_view = if progman.is_null() { std::ptr::null_mut() } else { find_child(progman, std::ptr::null_mut(), "SHELLDLL_DefView") };
        if def_view.is_null() {
            let mut host: HWND = std::ptr::null_mut();
            unsafe { EnumWindows(Some(enum_find_defview_host), &mut host as *mut HWND as LPARAM) };
            if !host.is_null() {
                def_view = find_child(host, std::ptr::null_mut(), "SHELLDLL_DefView");
            }
        }
        if def_view.is_null() {
            return std::ptr::null_mut();
        }
        find_child(def_view, std::ptr::null_mut(), "SysListView32")
    }

    fn open(&mut self) -> bool {
        self.close();
        self.list_view = Self::find_list_view();
        if self.list_view.is_null() {
            return false;
        }
        unsafe {
            let mut pid = 0u32;
            GetWindowThreadProcessId(self.list_view, &mut pid);
            self.proc = OpenProcess(PROCESS_VM_OPERATION | PROCESS_VM_READ | PROCESS_VM_WRITE, 0, pid);
            if self.proc.is_null() {
                return false;
            }
            self.mem = VirtualAllocEx(self.proc, std::ptr::null(), 64, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
            !self.mem.is_null()
        }
    }

    /// Trả về (khung biểu tượng theo toạ độ màn hình ảo, ListView có hiển thị không)
    pub fn read(&mut self) -> Option<(Vec<RECT>, bool)> {
        unsafe {
            if (self.list_view.is_null() || self.mem.is_null() || IsWindow(self.list_view) == 0) && !self.open() {
                return None;
            }
            let visible = IsWindowVisible(self.list_view) != 0;
            let mut out = Vec::new();
            if !visible {
                return Some((out, false));
            }
            let count = SendMessageW(self.list_view, LVM_GETITEMCOUNT, 0, 0) as usize;
            let mut origin = POINT { x: 0, y: 0 };
            ClientToScreen(self.list_view, &mut origin);
            for i in 0..count {
                let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 }; // left = LVIR_BOUNDS
                let mut n = 0usize;
                if WriteProcessMemory(self.proc, self.mem, &r as *const RECT as *const c_void, 16, &mut n) == 0 {
                    self.close();
                    return None;
                }
                SendMessageW(self.list_view, LVM_GETITEMRECT, i, self.mem as isize);
                if ReadProcessMemory(self.proc, self.mem, &mut r as *mut RECT as *mut c_void, 16, &mut n) == 0 {
                    self.close();
                    return None;
                }
                r.left += origin.x;
                r.right += origin.x;
                r.top += origin.y;
                r.bottom += origin.y;
                out.push(r);
            }
            Some((out, true))
        }
    }

    fn close(&mut self) {
        unsafe {
            if !self.mem.is_null() && !self.proc.is_null() {
                VirtualFreeEx(self.proc, self.mem, 0, MEM_RELEASE);
            }
            if !self.proc.is_null() {
                CloseHandle(self.proc);
            }
        }
        self.mem = std::ptr::null_mut();
        self.proc = std::ptr::null_mut();
    }
}

impl Drop for IconReader {
    fn drop(&mut self) {
        self.close();
    }
}

// ---------------------------------------------------------------- Tạm dừng khi bị che
struct FgScan<'a> {
    screens: &'a [RECT],
    own: &'a [HWND],
    paused: Vec<bool>,
    probe: Option<CoverageProbe>,
}

struct CoverageProbe {
    wallpaper_pid: Option<u32>,
    reasons: Vec<Option<&'static str>>,
}

fn coverage_reason(screen: &RECT, window: &RECT, zoomed: bool, ex: u32) -> Option<&'static str> {
    let (cx, cy) = ((window.left + window.right) / 2, (window.top + window.bottom) / 2);
    if !(cx >= screen.left && cx < screen.right && cy >= screen.top && cy < screen.bottom) { return None; }
    if zoomed { return Some("maximized"); }
    (ex & WS_EX_TOOLWINDOW == 0
        && window.left <= screen.left + 2 && window.top <= screen.top + 2
        && window.right >= screen.right - 2 && window.bottom >= screen.bottom - 56).then_some("bounds")
}

unsafe extern "system" fn enum_fg(w: HWND, l: LPARAM) -> BOOL {
    let s = &mut *(l as *mut FgScan);
    if s.own.contains(&w) || IsWindowVisible(w) == 0 || IsIconic(w) != 0 {
        return 1;
    }
    let mut cloaked = 0i32;
    if DwmGetWindowAttribute(w, DWMWA_CLOAKED as u32, &mut cloaked as *mut i32 as *mut c_void, 4) == 0 && cloaked != 0 {
        return 1;
    }
    let ex = GetWindowLongW(w, GWL_EXSTYLE) as u32;
    if ex & WS_EX_TRANSPARENT != 0 {
        return 1;
    }
    let c = class_name(w);
    if matches!(c.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "DummyDWMListenerWindow") {
        return 1;
    }
    if c == crate::WALL_CLASS {
        if let Some(pid) = s.probe.as_ref().and_then(|probe| probe.wallpaper_pid) {
            let mut owner = 0;
            GetWindowThreadProcessId(w, &mut owner);
            if owner == pid { return 1; }
        }
    }
    let mut wr = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    if GetWindowRect(w, &mut wr) == 0 {
        return 1;
    }
    if wr.right - wr.left <= 0 || wr.bottom - wr.top <= 0 {
        return 1;
    }
    let zoomed = IsZoomed(w) != 0;
    for (index,sc) in s.screens.iter().enumerate() {
        if s.paused[index] { continue; }
        if let Some(reason) = coverage_reason(sc, &wr, zoomed, ex) {
            s.paused[index]=true;
            if let Some(probe) = s.probe.as_mut() {
                probe.reasons[index] = Some(reason);
            }
        }
    }
    if s.paused.iter().all(|paused|*paused) {0}else{1}
}

pub fn covered_monitors(screens: &[RECT], own: &[HWND]) -> Vec<bool> {
    let mut s = FgScan { screens, own, paused: vec![false;screens.len()], probe: None };
    unsafe { EnumWindows(Some(enum_fg), &mut s as *mut FgScan as LPARAM) };
    s.paused
}

/// Read the same policy before pausing the installed app for a benchmark. The
/// optional PID excludes only its wallpaper HWNDs; its Settings still count.
/// The probe never returns titles, paths, HWNDs or PIDs.
pub(crate) fn probe_coverage(screens: &[RECT], wallpaper_pid: Option<u32>) -> Vec<Option<&'static str>> {
    let mut s = FgScan {
        screens, own: &[], paused: vec![false;screens.len()],
        probe: Some(CoverageProbe { wallpaper_pid, reasons: vec![None;screens.len()] }),
    };
    unsafe { EnumWindows(Some(enum_fg), &mut s as *mut FgScan as LPARAM) };
    s.probe.unwrap().reasons
}

#[cfg(test)]
mod coverage_tests {
    use super::*;

    #[test]
    fn small_foreground_fixture_does_not_hide_even_one_output() {
        let landscape = RECT { left: 0, top: 0, right: 2560, bottom: 1080 };
        let portrait = RECT { left: -1080, top: -731, right: 0, bottom: 1189 };
        let fixture = RECT { left: 475, top: 100, right: 875, bottom: 330 };
        let maximized = RECT { left: -8, top: -8, right: 2568, bottom: 1040 };
        assert_eq!(coverage_reason(&landscape, &fixture, false, 0), None);
        assert_eq!(coverage_reason(&portrait, &fixture, false, 0), None);
        assert_eq!(coverage_reason(&landscape, &maximized, true, 0), Some("maximized"));
        assert_eq!(coverage_reason(&portrait, &maximized, true, 0), None);
        assert_eq!(coverage_reason(&portrait, &portrait, false, 0), Some("bounds"));
        assert_eq!(coverage_reason(&portrait, &portrait, false, WS_EX_TOOLWINDOW), None);
    }
}
