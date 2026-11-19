//! tuanpc-desk: bản Rust native (1 tiến trình, OpenGL 3.3) của tuanpc-desk — 16 chủ đề tương tác thích ứng.
//! Chạy:   tuanpc-desk.exe [--theme <id>] [--fps 20] [--settings]
//! Thoát:  tuanpc-desk.exe --quit   (hoặc menu khay hệ thống)

#![windows_subsystem = "windows"]

mod desktop;
mod engine;
mod palm_layout;
mod solar;
mod lock;
mod settings;
mod launcher;
mod mini_tools;
mod web;
mod shortcuts;
mod themes {
    pub mod aquarium;
    pub mod beach;
    pub mod boxing_ring;
    pub mod cat_hill;
    pub mod cats;
    pub mod commute;
    pub mod lake;
    pub mod maldives;
    pub mod meadow;
    pub mod mocchau;
    pub mod ocean;
    pub mod rainforest;
    pub mod sky;
    pub mod stadium;
    pub mod summer_porch;
    pub mod tet_sakura;
}
mod wgl;
mod widget;
mod system_monitor;
mod presentation;
mod gpu_present;

use desktop::wide;
use engine::{IconBox, Prog, Shared, Theme};
use glow::HasContext;
use std::cell::{Cell,RefCell};
use std::fs::File;
use std::io::Write;
use std::sync::{Arc, Mutex, Mutex as StdMutex};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Graphics::OpenGL::SwapBuffers;
use windows_sys::Win32::Media::{timeBeginPeriod,timeEndPeriod};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::ProcessStatus::*;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::Accessibility::*;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// ---------------------------------------------------------------- Nhật ký
static LOG: Mutex<Option<File>> = Mutex::new(None);

pub fn log_line(s: &str) {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    if let Ok(mut g) = LOG.lock() {
        if let Some(f) = g.as_mut() {
            let _ = writeln!(f, "[{t:.3}] {s}");
            let _ = f.flush();
        }
    }
}

#[macro_export]
macro_rules! log {
    ($($t:tt)*) => { $crate::log_line(&format!($($t)*)) };
}

// ---------------------------------------------------------------- Thông điệp cửa sổ
const MSG_CLASS: &str = "TuanpcDeskMsg";
const WALL_CLASS: &str = "TuanpcDeskWall";
const SAVER_CLASS: &str = "TuanpcDeskSaver";
const WM_TRAY: u32 = WM_APP + 1;
const CMD_LOCK: usize = 120;
const CMD_SAVER: usize = 121;
const CMD_SYNC: usize = 122;
const CMD_WINL: usize = 123;
const CMD_SAVER_NOW: usize = 124;
const CMD_SETTINGS: usize = 125;
const CMD_INTERACTION: usize = 126;
const CMD_WIDGET: usize = 127;
const CMD_WIDGET_LOCK: usize = 128;
const CMD_WIDGET_RESET: usize = 129;
const CMD_LAUNCHER: usize = 130;
const CMD_CAPTURE: usize = 131;
const CMD_RECORD: usize = 132;
const CMD_SYSTEM_MONITOR: usize = 133;
const WM_OPEN_SETTINGS: u32 = WM_APP + 2;
const WM_OPEN_LAUNCHER: u32 = WM_APP + 3;
const CMD_EXIT: usize = 199;
const HOTKEY_SAVER: i32 = 1;
const HOTKEY_LAUNCHER: i32 = 2;
const WM_WTSSESSION_CHANGE_: u32 = 0x02B1;
const WTS_SESSION_LOCK_: usize = 0x7;
const WTS_SESSION_UNLOCK_: usize = 0x8;

pub const THEMES: &[(&str, &str, usize)] = &[
    ("meadow", "Thảo nguyên", 101),
    ("beach", "Bãi biển", 102),
    ("beach-turquoise", "Đảo Maldives", 103),
    ("lake", "Hồ nước", 104),
    ("sky", "Bầu trời", 105),
    ("ocean", "Đại dương", 106),
    ("rainforest", "Rừng mưa nhiệt đới", 107),
    ("mocchau", "Hoa mận", 108),
    ("tet-sakura", "Hoa anh đào Tết", 109),
    ("cat-hill", "Đồi mèo nắng ấm", 110),
    ("stadium", "Sân bóng sôi động", 111),
    ("aquarium", "Bể cá vàng", 113),
    ("commute", "Chuyến xe chiều", 114),
    ("summer-porch", "Hiên nhà ngày hạ", 115),
    ("cats", "Mèo con trên đồi", 116),
    ("boxing-ring", "Võ đài quyền anh", 117),
];

pub fn canonical_theme(id: &str) -> Option<&'static str> {
    match id {
        "maldives" => Some("beach-turquoise"),
        "plum-blossoms" | "plum_blossoms" | "plum" => Some("mocchau"),
        "tet_sakura" => Some("tet-sakura"),
        "cat_hill" => Some("cat-hill"),
        "football" | "soccer" | "pitch" => Some("stadium"),
        "fish" | "fishing" | "goldfish" | "fishbowl" => Some("aquarium"),
        "bus" | "evening-bus" | "commute-bus" => Some("commute"),
        "porch" | "corgi" | "corgi-porch" | "veranda" | "summer_porch" => Some("summer-porch"),
        "kittens" | "hillside-kittens" => Some("cats"),
        "boxing" | "ring" | "arcade-boxing" | "punch" | "boxing_ring" => Some("boxing-ring"),
        other => THEMES.iter().find(|&&(tid, _, _)| tid == other).map(|&(tid, _, _)| tid),
    }
}

pub fn theme_cmd(id: &str) -> usize {
    let cid = canonical_theme(id).unwrap_or("beach");
    THEMES.iter().find(|&&(tid, _, _)| tid == cid).map(|&(_, _, cmd)| cmd).unwrap_or(102)
}

pub fn cmd_theme(cmd: usize) -> Option<&'static str> {
    THEMES.iter().find(|&&(_, _, c)| c == cmd).map(|&(tid, _, _)| tid)
}

/// Nạp biểu tượng ứng dụng nhúng trong exe (resource ID = 1) theo kích thước DPI mong muốn.
pub fn app_icon(cx: i32, cy: i32) -> HICON {
    unsafe {
        let h = LoadImageW(
            GetModuleHandleW(std::ptr::null()),
            1usize as *const u16,
            IMAGE_ICON,
            cx,
            cy,
            LR_DEFAULTCOLOR | LR_SHARED,
        ) as HICON;
        if !h.is_null() {
            h
        } else {
            LoadIconW(std::ptr::null_mut(), IDI_APPLICATION)
        }
    }
}

thread_local! {
    static PENDING_CMD: Cell<usize> = const { Cell::new(0) };
    static PENDING_CLICK: Cell<Option<(i32, i32)>> = const { Cell::new(None) };
    static MOUSE_DOWN: Cell<Option<(i32, i32, Instant)>> = const { Cell::new(None) };
    static ZORDER_DIRTY: Cell<bool> = const { Cell::new(false) };
    static DISPLAY_CHANGED: Cell<bool> = const { Cell::new(false) };
    static REPAINT: Cell<bool> = const { Cell::new(false) };
    static LOCK_SYNC: Cell<bool> = const { Cell::new(true) };
    static INTERACTIONS_ENABLED: Cell<bool> = const { Cell::new(true) };
    static PENDING_THEME_SYNC: Cell<bool> = const { Cell::new(false) };
    static SAVER_START: Cell<Option<Instant>> = const { Cell::new(None) };
    static SAVER_MOUSE: Cell<Option<(i32, i32)>> = const { Cell::new(None) };
    static CURRENT_THEME: Cell<usize> = const { Cell::new(102) };
    /// Phiên Windows đang khóa (WM_WTSSESSION_CHANGE): không ai thấy desktop -> ngừng vẽ.
    static SESSION_LOCKED: Cell<bool> = const { Cell::new(false) };
    static SAVER_RECONCILE_PENDING: Cell<bool> = const { Cell::new(false) };
    static SHORTCUT_RECORDING: Cell<bool> = const { Cell::new(false) };
    static HOTKEY_OK: Cell<bool> = const { Cell::new(false) };
    static LAUNCHER_HOTKEY_OK: Cell<bool> = const { Cell::new(false) };
    static TOOL_HOTKEYS: Cell<[bool;2]> = const { Cell::new([false;2]) };
    static WIDGET_ENABLED: Cell<bool> = const { Cell::new(true) };
    static WIDGET_LOCKED: Cell<bool> = const { Cell::new(true) };
}

fn normalize_hotkey(mods: u32, vk: u32) -> Option<u32> { shortcuts::normalize(mods,vk) }

unsafe fn register_hotkey_logged(hwnd: HWND, id: i32, mods: u32, vk: u32, name: &str) -> bool {
    if normalize_hotkey(mods, vk).is_none() {
        log!("Phím tắt {name}: tổ hợp không hợp lệ (mods={mods}, vk={vk})");
        return false;
    }
    let ok = RegisterHotKey(hwnd, id, mods, vk) != 0;
    if ok {
        log!("Phím tắt {name}: đã đăng ký {}", mini_tools::hotkey_text(mods, vk));
    } else {
        log!("Phím tắt {name}: đăng ký thất bại {} (GetLastError={})", mini_tools::hotkey_text(mods, vk), GetLastError());
    }
    ok
}

unsafe extern "system" fn msg_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match m {
        WM_HOTKEY if SHORTCUT_RECORDING.with(|c| c.get()) => 0,
        WM_INPUT => {
            let mut raw: RAWINPUT = std::mem::zeroed();
            let mut size = std::mem::size_of::<RAWINPUT>() as u32;
            GetRawInputData(l as HRAWINPUT, RID_INPUT, &mut raw as *mut _ as *mut _, &mut size, std::mem::size_of::<RAWINPUTHEADER>() as u32);
            if raw.header.dwType == RIM_TYPEMOUSE {
                let flags = raw.data.mouse.Anonymous.Anonymous.usButtonFlags;
                if flags & (RI_MOUSE_LEFT_BUTTON_DOWN as u16) != 0 {
                    let mut pt = POINT { x: 0, y: 0 };
                    GetCursorPos(&mut pt);
                    MOUSE_DOWN.with(|c| c.set(Some((pt.x, pt.y, Instant::now()))));
                }
                if flags & (RI_MOUSE_LEFT_BUTTON_UP as u16) != 0 {
                    let mut pt = POINT { x: 0, y: 0 };
                    GetCursorPos(&mut pt);
                    if let Some((x0, y0, t0)) = MOUSE_DOWN.with(|c| c.take()) {
                        let d2 = (pt.x - x0).saturating_mul(pt.x - x0).saturating_add((pt.y - y0).saturating_mul(pt.y - y0));
                        if d2 <= 36 && t0.elapsed().as_millis() <= 450 {
                            PENDING_CLICK.with(|c| c.set(Some((pt.x, pt.y))));
                        }
                    }
                }
            }
            DefWindowProcW(h, m, w, l)
        }
        WM_CLOSE => {
            PostQuitMessage(0);
            0
        }
        WM_DISPLAYCHANGE => {
            DISPLAY_CHANGED.with(|c| c.set(true));
            0
        }
        WM_WTSSESSION_CHANGE_ => {
            match w {
                WTS_SESSION_LOCK_ => {
                    SESSION_LOCKED.with(|c| c.set(true));
                    log!("Windows thông báo phiên đã khóa (WTS_SESSION_LOCK)");
                    lock::log_saver_state("phiên khóa");
                }
                WTS_SESSION_UNLOCK_ => {
                    SESSION_LOCKED.with(|c| c.set(false));
                    SAVER_RECONCILE_PENDING.with(|c| c.set(true));
                    log!("Windows thông báo phiên đã mở khóa (WTS_SESSION_UNLOCK)");
                }
                _ => {}
            }
            0
        }
        WM_OPEN_SETTINGS => {
            PENDING_CMD.with(|c| c.set(CMD_SETTINGS));
            0
        }
        WM_COMMAND if w & 0xffff == CMD_SYSTEM_MONITOR => {
            PENDING_CMD.with(|c| c.set(CMD_SYSTEM_MONITOR));
            0
        }
        WM_HOTKEY if w as i32 == HOTKEY_SAVER => {
            // Nhận WM_HOTKEY cho phép tiến trình đưa cửa sổ lên trước (cần cho SC_SCREENSAVE)
            SetForegroundWindow(h);
            PENDING_CMD.with(|c| c.set(CMD_SAVER_NOW));
            0
        }
        WM_HOTKEY if w as i32 == HOTKEY_LAUNCHER => {
            PENDING_CMD.with(|c| c.set(CMD_LAUNCHER));
            0
        }
        WM_OPEN_LAUNCHER => {
            PENDING_CMD.with(|c| c.set(CMD_LAUNCHER));
            0
        }
        WM_HOTKEY if w as i32 == mini_tools::CAPTURE_ID => {
            // Lấy ảnh trước khi context menu nhận ra phím tắt và tự đóng.
            let _ = mini_tools::stage_hotkey_capture();
            PENDING_CMD.with(|c| c.set(CMD_CAPTURE));
            0
        }
        WM_HOTKEY if w as i32 == mini_tools::RECORD_ID => { PENDING_CMD.with(|c| c.set(CMD_RECORD)); 0 }
        WM_TRAY => {
            let ev = (l as u32) & 0xFFFF;
            if ev == WM_LBUTTONUP {
                PENDING_CMD.with(|c| c.set(CMD_SETTINGS));
            } else if ev == WM_RBUTTONUP || ev == WM_CONTEXTMENU {
                let menu = CreatePopupMenu();
                AppendMenuW(menu, MF_STRING, CMD_SETTINGS, wide("Cài đặt…").as_ptr());
                let menu_cfg = load_config(None);
                let launcher_label = format!("Mở Launcher\t{}", launcher::hotkey_text(menu_cfg.launcher_mods, menu_cfg.launcher_vk));
                AppendMenuW(menu, MF_STRING, CMD_LAUNCHER, wide(&launcher_label).as_ptr());
                AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
                let capture_label = format!("Chụp vùng và chỉnh sửa…\t{}", mini_tools::hotkey_text(menu_cfg.tools.capture_mods, menu_cfg.tools.capture_key));
                let record_label = format!("Quay vùng / dừng và lưu…\t{}", mini_tools::hotkey_text(menu_cfg.tools.record_mods, menu_cfg.tools.record_key));
                AppendMenuW(menu, MF_STRING, CMD_CAPTURE, wide(&capture_label).as_ptr());
                AppendMenuW(menu, MF_STRING, CMD_RECORD, wide(&record_label).as_ptr());
                AppendMenuW(menu, MF_STRING | if menu_cfg.system_monitor.enabled { MF_CHECKED } else { 0 }, CMD_SYSTEM_MONITOR, wide("Thống kê CPU / RAM / GPU / mạng trên taskbar").as_ptr());
                SetMenuDefaultItem(menu, CMD_SETTINGS as u32, 0);
                AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
                let cur = CURRENT_THEME.with(|c| c.get());
                let cur_label = THEMES
                    .iter()
                    .find(|&&(_, _, cmd_id)| cmd_id == cur)
                    .map(|&(_, label, _)| label)
                    .unwrap_or("Bãi biển");
                let cur_text = format!("Chủ đề: {cur_label}");
                AppendMenuW(menu, MF_STRING | MF_CHECKED, CMD_SETTINGS, wide(&cur_text).as_ptr());
                AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
                // Màn hình khóa: chỉ dùng khóa thật của Windows
                AppendMenuW(menu, MF_STRING, CMD_LOCK, wide("Khóa máy ngay (khóa Windows)").as_ptr());
                let label = format!("Khóa với cảnh động\t{}",shortcuts::label(menu_cfg.saver_mods,menu_cfg.saver_vk));
                AppendMenuW(menu, MF_STRING, CMD_SAVER_NOW, wide(&label).as_ptr());
                let chk = |on: bool| if on { MF_STRING | MF_CHECKED } else { MF_STRING };
                AppendMenuW(menu, chk(lock::saver_active()), CMD_SAVER, wide("Cảnh động khi rảnh (trình bảo vệ màn hình, khóa khi quay lại)").as_ptr());
                AppendMenuW(menu, chk(LOCK_SYNC.with(|c| c.get())), CMD_SYNC, wide("Dùng cảnh làm ảnh màn khóa Windows").as_ptr());
                let chk_int = if INTERACTIONS_ENABLED.with(|c| c.get()) { MF_STRING | MF_CHECKED } else { MF_STRING };
                AppendMenuW(menu, chk_int, CMD_INTERACTION, wide("Tương tác chuột trên màn hình nền").as_ptr());
                let chk_widget = if WIDGET_ENABLED.with(|c| c.get()) { MF_STRING | MF_CHECKED } else { MF_STRING };
                AppendMenuW(menu, chk_widget, CMD_WIDGET, wide("Hiện đồng hồ native").as_ptr());
                let chk_widget_lock = if WIDGET_LOCKED.with(|c| c.get()) { MF_STRING | MF_CHECKED } else { MF_STRING };
                AppendMenuW(menu, chk_widget_lock, CMD_WIDGET_LOCK, wide("Khóa vị trí đồng hồ").as_ptr());
                AppendMenuW(menu, MF_STRING, CMD_WIDGET_RESET, wide("Đặt lại vị trí đồng hồ").as_ptr());
                if lock::winl_disabled() {
                    AppendMenuW(menu, MF_STRING, CMD_WINL, wide("⚠ Bật lại Win+L của Windows (bản C# cũ đã tắt)").as_ptr());
                }
                AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
                AppendMenuW(menu, MF_STRING, CMD_EXIT, wide("Thoát tuanpc-desk").as_ptr());
                let mut pt = POINT { x: 0, y: 0 };
                GetCursorPos(&mut pt);
                SetForegroundWindow(h);
                let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, h, std::ptr::null());
                DestroyMenu(menu);
                if cmd > 0 {
                    PENDING_CMD.with(|c| c.set(cmd as usize));
                }
            }
            0
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}

unsafe extern "system" fn wall_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match m {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            ValidateRect(h, std::ptr::null());
            REPAINT.with(|c| c.set(true));
            0
        }
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        _ => DefWindowProcW(h, m, w, l),
    }
}

/// Cửa sổ trình bảo vệ màn hình: có bất kỳ thao tác nào thì thoát. Việc khóa / mở khóa do Windows
/// làm (ScreenSaverIsSecure), tuanpc-desk không kiểm mật khẩu và không chặn phím nào.
unsafe extern "system" fn saver_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let warm = SAVER_START.with(|c| c.get()).is_some_and(|t| t.elapsed().as_millis() > 800);
    match m {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            ValidateRect(h, std::ptr::null());
            REPAINT.with(|c| c.set(true));
            0
        }
        WM_SETCURSOR => {
            SetCursor(std::ptr::null_mut());
            1
        }
        WM_KEYDOWN | WM_SYSKEYDOWN | WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN | WM_MOUSEWHEEL => {
            if warm {
                PostQuitMessage(0);
            }
            0
        }
        WM_MOUSEMOVE => {
            let (x, y) = ((l & 0xFFFF) as i16 as i32, ((l >> 16) & 0xFFFF) as i16 as i32);
            match SAVER_MOUSE.with(|c| c.get()) {
                None => SAVER_MOUSE.with(|c| c.set(Some((x, y)))),
                Some((x0, y0)) if warm && ((x - x0).abs() > 6 || (y - y0).abs() > 6) => PostQuitMessage(0),
                _ => {}
            }
            0
        }
        WM_ACTIVATEAPP if w == 0 && warm => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}

unsafe extern "system" fn win_event(_: HWINEVENTHOOK, _ev: u32, _h: HWND, id_object: i32, _: i32, _: u32, _: u32) {
    if id_object == 0 {
        ZORDER_DIRTY.with(|c| c.set(true));
    }
}

fn register_class(name: &str, proc_: WNDPROC, own_dc: bool) {
    unsafe {
        let cls = wide(name);
        let mut wc: WNDCLASSEXW = std::mem::zeroed();
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.style = if own_dc { CS_OWNDC } else { 0 };
        wc.lpfnWndProc = proc_;
        wc.hInstance = GetModuleHandleW(std::ptr::null());
        wc.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
        wc.lpszClassName = cls.as_ptr();
        RegisterClassExW(&wc);
    }
}

// ---------------------------------------------------------------- Màn hình
struct Monitor {
    rect: RECT,
    dpr: f32,
    primary: bool,
}

unsafe extern "system" fn enum_monitor(hm: HMONITOR, _: HDC, _: *mut RECT, l: LPARAM) -> BOOL {
    let out = &mut *(l as *mut Vec<Monitor>);
    let mut mi: MONITORINFO = std::mem::zeroed();
    mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(hm, &mut mi) != 0 {
        let (mut dx, mut dy) = (96u32, 96u32);
        GetDpiForMonitor(hm, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        out.push(Monitor { rect: mi.rcMonitor, dpr: dx as f32 / 96.0, primary: mi.dwFlags & MONITORINFOF_PRIMARY != 0 });
    }
    1
}

fn monitors() -> Vec<Monitor> {
    let mut v: Vec<Monitor> = Vec::new();
    unsafe { EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(enum_monitor), &mut v as *mut Vec<Monitor> as LPARAM) };
    v.sort_by_key(|m| !m.primary);
    v
}

// ---------------------------------------------------------------- Cài đặt (đọc chung với bản C#)
#[derive(Clone)]
struct Environment {
    time_enabled: bool,
    weather_enabled: bool,
    preset: String,
    source: String,
    location: Option<serde_json::Value>,
}

#[derive(Clone, Copy, Default)]
struct LiveWeather {
    cloud: f32,
    rain: f32,
    snow: f32,
    storm: f32,
    wind: f32,
}

fn live_weather_amount(env: &Environment, live: Option<LiveWeather>) -> Option<f32> {
    if env.source != "live" || env.location.is_none() || !env.weather_enabled { return None; }
    let w = live?;
    Some(w.storm.max(w.snow).max(w.rain).max(w.cloud * 0.42).clamp(0.0, 1.0))
}

fn fetch_live_weather(env: &Environment, target: Arc<StdMutex<Option<LiveWeather>>>) {
    let Some(loc) = env.location.as_ref() else { return };
    let Some(lat) = loc.get("latitude").and_then(|v| v.as_f64()) else { return };
    let Some(lon) = loc.get("longitude").and_then(|v| v.as_f64()) else { return };
    std::thread::spawn(move || {
        let url = format!("https://api.open-meteo.com/v1/forecast?latitude={lat:.5}&longitude={lon:.5}&timezone=auto&current=precipitation,rain,showers,snowfall,weather_code,cloud_cover,wind_speed_10m");
        let Ok(resp) = reqwest::blocking::get(url) else { log!("Thời tiết live: không kết nối được Open-Meteo"); return };
        let Ok(root) = resp.json::<serde_json::Value>() else { log!("Thời tiết live: dữ liệu JSON không hợp lệ"); return };
        let Some(c) = root.get("current") else { log!("Thời tiết live: thiếu current"); return };
        let n = |k: &str| c.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let code = c.get("weather_code").and_then(|v| v.as_i64()).unwrap_or(0);
        let sample = LiveWeather {
            cloud: (n("cloud_cover") / 100.0).clamp(0.0, 1.0),
            rain: ((n("precipitation") + n("rain") + n("showers")) / 4.0).clamp(0.0, 1.0),
            snow: (n("snowfall") / 3.0).clamp(0.0, 1.0),
            storm: if code >= 95 { 1.0 } else { 0.0 },
            wind: (n("wind_speed_10m") / 70.0).clamp(0.0, 1.0),
        };
        if let Ok(mut slot) = target.lock() { *slot = Some(sample); }
        log!("Thời tiết live: đã cập nhật lat={lat:.4}, lon={lon:.4}, code={code}");
    });
}

#[repr(C)]
struct LocalSystemTime {
    year: u16, month: u16, _dow: u16, day: u16,
    hour: u16, minute: u16, second: u16, millis: u16,
}

unsafe extern "system" {
    fn GetLocalTime(lp_system_time: *mut LocalSystemTime);
}

fn local_day_phase() -> f32 {
    let mut st = LocalSystemTime { year: 0, month: 0, _dow: 0, day: 0, hour: 12, minute: 0, second: 0, millis: 0 };
    unsafe { GetLocalTime(&mut st) };
    ((st.hour as f32 * 3600.0 + st.minute as f32 * 60.0 + st.second as f32 + st.millis as f32 / 1000.0) / 86400.0).fract()
}

impl Environment {
    fn sun(&self) -> solar::Sun {
        if !self.time_enabled { return solar::clock(0.42); }
        if let Some(loc) = self.location.as_ref() {
            if let (Some(lat), Some(lon)) = (
                loc.get("latitude").and_then(|v| v.as_f64()),
                loc.get("longitude").and_then(|v| v.as_f64())) {
                if lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0 {
                    let utc = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default().as_secs_f64();
                    return solar::position(utc, lat, lon);
                }
            }
        }
        solar::clock(local_day_phase())
    }

    fn day_phase(&self) -> f32 {
        if self.time_enabled { local_day_phase() } else { 0.5 }
    }

    fn weather_amount(&self, now: f64) -> f32 {
        if !self.weather_enabled { return 0.0; }
        match self.preset.as_str() {
            "clear" => 0.0,
            "cloudy" => 0.28,
            "rain" => 0.58,
            "storm" => 0.86,
            "snow" => 1.0,
            // Mô phỏng cục bộ, không gọi mạng khi chạy native. Chu kỳ dài để
            // màu trời và cường độ mưa thay đổi chậm, không gây tải CPU.
            _ => (0.18 + ((now / 1000.0) * 0.0021).sin().abs() * 0.54) as f32,
        }
    }
}

#[derive(Clone)]
struct Config {
    theme: String,
    params: serde_json::Value,
    mode: String,
    environment: Environment,
    /// Đặt ảnh cảnh hiện tại làm ảnh màn khóa Windows (`lockSyncWin`, mặc định bật như bản C#)
    lock_sync: bool,
    /// Số phút rảnh trước khi trình bảo vệ màn hình chạy (`lockIdleMins`)
    lock_idle_mins: u32,
    /// Ngôn ngữ giao diện trang cài đặt: "auto" | "vi" | "en"
    lang: String,
    /// Bật/tắt tương tác chuột trên màn hình nền
    interactions_enabled: bool,
    widget_enabled: bool,
    widget_locked: bool,
    widget_x: i32,
    widget_y: i32,
    widget_scale: f32,
    widget_opacity: f32,
    widget_lines: [widget::Line; 3],
    saver_mods: u32,
    saver_vk: u32,
    launcher_mods: u32,
    launcher_vk: u32,
    launcher_appearance: launcher::Appearance,
    tools: mini_tools::Options,
    system_monitor: system_monitor::Options,
}

fn rust_settings_path() -> std::path::PathBuf {
    lock::data_dir().join("settings.json")
}

/// Đọc `~/.deskland-rs/settings.json` (cài đặt riêng của bản Rust); chưa có thì đọc của bản C#
/// (`%LOCALAPPDATA%/DesklandPreview/settings.json`) để giữ chủ đề / thông số đang dùng.
fn load_config(cli_theme: Option<String>) -> Config {
    let read = |p: &std::path::Path| std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(s.trim_start_matches('\u{feff}')).ok());
    let cs = std::env::var("LOCALAPPDATA").map(|d| std::path::PathBuf::from(d).join("DesklandPreview").join("settings.json")).unwrap_or_default();
    let csv = read(&cs);
    let root = read(&rust_settings_path()).or_else(|| csv.clone()).unwrap_or(serde_json::Value::Null);
    // Ngôn ngữ: bản Rust chưa lưu thì theo lựa chọn ở bản C#
    let lang = root.get("lang").or_else(|| csv.as_ref().and_then(|c| c.get("lang"))).and_then(|v| v.as_str()).unwrap_or("auto").to_string();
    let saved = root.get("theme").and_then(|v| v.as_str()).unwrap_or("beach").to_string();
    let interactions_enabled = root.get("interactionsEnabled").and_then(|v| v.as_bool()).unwrap_or(true);
    let widget = root.get("widget");
    let env = root.get("environment");
    let legacy = csv.as_ref();
    let env_bool = |key: &str| env.and_then(|v| v.get(key)).and_then(|v| v.as_bool());
    let legacy_bool = |key: &str| root.get(key).and_then(|v| v.as_bool()).or_else(|| legacy.and_then(|v| v.get(key)).and_then(|v| v.as_bool()));
    let env_str = |key: &str| env.and_then(|v| v.get(key)).and_then(|v| v.as_str());
    let legacy_str = |key: &str| root.get(key).and_then(|v| v.as_str()).or_else(|| legacy.and_then(|v| v.get(key)).and_then(|v| v.as_str()));
    let env_location = env.and_then(|v| v.get("location")).cloned().filter(|v| !v.is_null());
    let legacy_location = root.get("weatherLocation").cloned().filter(|v| !v.is_null()).or_else(|| legacy.and_then(|v| v.get("weatherLocation")).cloned().filter(|v| !v.is_null()));
    let environment = Environment {
        time_enabled: env_bool("time").or_else(|| legacy_bool("dynamicTime")).unwrap_or(true),
        weather_enabled: env_bool("weather").or_else(|| legacy_bool("dynamicWeather")).unwrap_or(true),
        preset: env_str("preset").or_else(|| legacy_str("weatherPreset")).unwrap_or("auto").to_string(),
        source: env_str("source").or_else(|| legacy_str("weatherSource")).unwrap_or("local").to_string(),
        location: env_location.or(legacy_location),
    };
    let raw_theme = cli_theme.unwrap_or(saved);
    let theme = match canonical_theme(&raw_theme) {
        Some(t) => t.to_string(),
        None => {
            log!("Chủ đề '{raw_theme}' không hợp lệ, dùng 'beach'");
            "beach".into()
        }
    };
    Config {
        theme,
        params: root.get("params").cloned().unwrap_or(serde_json::Value::Null),
        mode: root.get("mode").and_then(|v| v.as_str()).map(|v| v.to_ascii_lowercase()).filter(|v| matches!(v.as_str(), "mirror" | "independent" | "panorama")).unwrap_or_else(|| "mirror".to_string()),
        environment,
        lock_sync: root.get("lockSyncWin").and_then(|v| v.as_bool()).unwrap_or(true),
        lang,
        interactions_enabled,
        widget_enabled: widget.and_then(|v| v.get("enabled")).and_then(|v| v.as_bool()).unwrap_or(true),
        widget_locked: widget.and_then(|v| v.get("locked")).and_then(|v| v.as_bool()).unwrap_or(true),
        widget_x: widget.and_then(|v| v.get("x")).and_then(|v| v.as_i64()).map(|v| v as i32).unwrap_or(-1),
        widget_y: widget.and_then(|v| v.get("y")).and_then(|v| v.as_i64()).map(|v| v as i32).unwrap_or(-1),
        widget_scale: widget.and_then(|v| v.get("scale")).and_then(|v| v.as_f64()).map(|v| v as f32).filter(|v| v.is_finite()).unwrap_or(1.0).clamp(0.7, 1.6),
        widget_opacity: widget.and_then(|v| v.get("opacity")).and_then(|v| v.as_f64()).map(|v| v as f32).filter(|v| v.is_finite()).unwrap_or(0.72).clamp(0.05, 1.0),
        widget_lines: ["time", "weekday", "date"].map(|k| widget::Line {
            x: widget.and_then(|v| v.get(k)).and_then(|v| v.get("x")).and_then(|v| v.as_i64()).unwrap_or(120) as i32,
            y: widget.and_then(|v| v.get(k)).and_then(|v| v.get("y")).and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            opacity: widget.and_then(|v| v.get(k)).and_then(|v| v.get("opacity")).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(1.0).clamp(0.0, 1.0),
            size: widget.and_then(|v| v.get(k)).and_then(|v| v.get("size")).and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(1.0).clamp(0.5, 2.0),
        }),
        saver_mods: shortcuts::normalize(root["saverHotkey"]["mods"].as_u64().unwrap_or(16387) as u32, root["saverHotkey"]["vk"].as_u64().unwrap_or(76) as u32).unwrap_or(16387),
        saver_vk: { let mods=root["saverHotkey"]["mods"].as_u64().unwrap_or(16387) as u32; let vk=root["saverHotkey"]["vk"].as_u64().unwrap_or(76) as u32; if shortcuts::normalize(mods,vk).is_some() {vk} else {76} },
        launcher_mods: {
            let mods = root.get("launcherHotkey").and_then(|v| v.get("mods")).and_then(|v| v.as_u64()).map(|v| v as u32).unwrap_or(MOD_ALT | MOD_NOREPEAT);
            let vk = root.get("launcherHotkey").and_then(|v| v.get("vk")).and_then(|v| v.as_u64()).map(|v| v as u32).unwrap_or(VK_SPACE as u32);
            normalize_hotkey(mods, vk).unwrap_or(MOD_ALT | MOD_NOREPEAT)
        },
        launcher_vk: {
            let vk = root.get("launcherHotkey").and_then(|v| v.get("vk")).and_then(|v| v.as_u64()).map(|v| v as u32).unwrap_or(VK_SPACE as u32);
            if normalize_hotkey(root.get("launcherHotkey").and_then(|v| v.get("mods")).and_then(|v| v.as_u64()).map(|v| v as u32).unwrap_or(MOD_ALT | MOD_NOREPEAT), vk).is_some() { vk } else { VK_SPACE as u32 }
        },
        launcher_appearance: launcher::Appearance::from_json(&root["launcherAppearance"]),
        tools: mini_tools::Options::from_json(&root["miniTools"]),
        system_monitor: system_monitor::Options::from_json(&root["systemMonitor"]),
        lock_idle_mins: root.get("lockIdleMins").and_then(|v| v.as_u64()).map(|v| v as u32).filter(|&v| v > 0).unwrap_or(5),
    }
}

fn configured_shortcuts(cfg: &Config) -> [(&'static str,i32,u32,u32,&'static str);4] {
    [("launcher",HOTKEY_LAUNCHER,cfg.launcher_mods,cfg.launcher_vk,"Spotlight"),
     ("saver",HOTKEY_SAVER,cfg.saver_mods,cfg.saver_vk,"Khóa với cảnh động"),
     ("capture",mini_tools::CAPTURE_ID,cfg.tools.capture_mods,cfg.tools.capture_key,"Chụp màn hình"),
     ("record",mini_tools::RECORD_ID,cfg.tools.record_mods,cfg.tools.record_key,"Quay màn hình")]
}
fn unregister_shortcuts(hwnd: HWND) {
    unsafe { for id in [HOTKEY_LAUNCHER,HOTKEY_SAVER,mini_tools::CAPTURE_ID,mini_tools::RECORD_ID] { UnregisterHotKey(hwnd,id); } }
}
fn register_configured_shortcuts(hwnd: HWND,cfg: &Config) {
    unregister_shortcuts(hwnd);
    unsafe {
        HOTKEY_OK.with(|c|c.set(register_hotkey_logged(hwnd,HOTKEY_SAVER,cfg.saver_mods,cfg.saver_vk,"Khóa với cảnh động")));
        LAUNCHER_HOTKEY_OK.with(|c|c.set(register_hotkey_logged(hwnd,HOTKEY_LAUNCHER,cfg.launcher_mods,cfg.launcher_vk,"Spotlight")));
    }
    TOOL_HOTKEYS.with(|c|c.set(mini_tools::register(hwnd,&cfg.tools)));
}

/// Lưu cài đặt của bản Rust (không đụng file của bản C#). Không lưu mật khẩu / PIN nào.
fn save_config(cfg: &Config) {
    let v = serde_json::json!({
        "version": 1,
        "theme": cfg.theme,
        "mode": cfg.mode,
        "environment": {
            "time": cfg.environment.time_enabled,
            "weather": cfg.environment.weather_enabled,
            "preset": cfg.environment.preset,
            "source": cfg.environment.source,
            "location": cfg.environment.location,
        },
        "lockSyncWin": LOCK_SYNC.with(|c| c.get()),
        "lockIdleMins": cfg.lock_idle_mins,
        "lang": cfg.lang,
        "interactionsEnabled": INTERACTIONS_ENABLED.with(|c| c.get()),
        "widget": {
            "enabled": WIDGET_ENABLED.with(|c| c.get()),
            "locked": WIDGET_LOCKED.with(|c| c.get()),
            "x": cfg.widget_x,
            "y": cfg.widget_y,
            "scale": cfg.widget_scale,
            "opacity": cfg.widget_opacity,
            "time": { "x": cfg.widget_lines[0].x, "y": cfg.widget_lines[0].y, "opacity": cfg.widget_lines[0].opacity, "size": cfg.widget_lines[0].size },
            "weekday": { "x": cfg.widget_lines[1].x, "y": cfg.widget_lines[1].y, "opacity": cfg.widget_lines[1].opacity, "size": cfg.widget_lines[1].size },
            "date": { "x": cfg.widget_lines[2].x, "y": cfg.widget_lines[2].y, "opacity": cfg.widget_lines[2].opacity, "size": cfg.widget_lines[2].size },
        },
        "params": cfg.params,
        "saverHotkey": { "mods": cfg.saver_mods, "vk": cfg.saver_vk },
        "launcherHotkey": { "mods": cfg.launcher_mods, "vk": cfg.launcher_vk },
        "launcherAppearance": cfg.launcher_appearance.json(),
        "miniTools": cfg.tools.json(),
        "systemMonitor": cfg.system_monitor.json(),
    });
    let p = rust_settings_path();
    if let Err(e) = std::fs::write(&p, serde_json::to_string_pretty(&v).unwrap_or_default()) {
        log!("Lưu cài đặt {} lỗi: {e}", p.display());
    }
}

/// Ngôn ngữ thực dùng + ngôn ngữ hệ thống cho trang cài đặt.
fn lang_eff(setting: &str) -> (String, String) {
    let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    let sys = if id & 0x3FF == 0x2A { "vi" } else { "en" }.to_string();
    let eff = if setting == "vi" || setting == "en" { setting.to_string() } else { sys.clone() };
    (eff, sys)
}

/// Màn hình chính trong danh sách cửa sổ hình nền.
fn primary_wall(app: &App) -> Option<&Wallpaper> {
    app.walls.iter().find(|w| w.screen.left == 0 && w.screen.top == 0).or(app.walls.first())
}

/// Thông điệp `init` cho trang cài đặt (cùng định dạng bản C#), kèm `caps` nói rõ bản Rust làm được gì.
fn web_init(app: &App) -> serde_json::Value {
    let (w, h) = primary_wall(app).map(|p| (p.screen.right - p.screen.left, p.screen.bottom - p.screen.top)).unwrap_or((1920, 1080));
    let (eff, sys) = lang_eff(&app.cfg.lang);
    let mut monitors = Vec::new();
    let mut world = serde_json::json!({ "x": 0, "y": 0, "w": w, "h": h });
    if let (Some(first), Some(last)) = (app.walls.iter().map(|p| p.screen.left).min(), app.walls.iter().map(|p| p.screen.top).min()) {
        let right = app.walls.iter().map(|p| p.screen.right).max().unwrap_or(w);
        let bottom = app.walls.iter().map(|p| p.screen.bottom).max().unwrap_or(h);
        world = serde_json::json!({ "x": first, "y": last, "w": right - first, "h": bottom - last });
    }
    for (i, p) in app.walls.iter().enumerate() {
        monitors.push(serde_json::json!({
            "id": format!("screen-{i}"), "name": format!("Màn hình {}", i + 1),
            "x": p.screen.left, "y": p.screen.top,
            "w": p.screen.right - p.screen.left, "h": p.screen.bottom - p.screen.top,
            "dpi": (p.dpr * 96.0).round(), "scale": p.dpr,
            "primary": p.screen.left == 0 && p.screen.top == 0,
            "theme": app.cfg.theme, "status": if p.paused { "paused" } else { "running" },
        }));
    }
    let registry = web::find_prototype()
        .and_then(|p| std::fs::read_to_string(p.join("themes.json")).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .unwrap_or(serde_json::Value::Null);
    serde_json::json!({
        "t": "init",
        "registry": registry,
        "theme": app.cfg.theme,
        "params": app.cfg.params,
        "mode": app.cfg.mode,
        "monitors": monitors,
        "world": world,
        "environment": {
            "time": app.cfg.environment.time_enabled,
            "weather": app.cfg.environment.weather_enabled,
            "preset": app.cfg.environment.preset,
            "source": app.cfg.environment.source,
            "location": app.cfg.environment.location,
        },
        "screen": { "w": w, "h": h },
        "autostart": settings::autostart_enabled(),
        "saverHotkey": { "mods": app.cfg.saver_mods, "vk": app.cfg.saver_vk, "label": shortcuts::label(app.cfg.saver_mods,app.cfg.saver_vk) },
        "launcherHotkey": { "mods": app.cfg.launcher_mods, "vk": app.cfg.launcher_vk, "label": launcher::hotkey_text(app.cfg.launcher_mods, app.cfg.launcher_vk) },
        "launcherAppearance": app.cfg.launcher_appearance.json(),
        "miniTools": app.cfg.tools.json(),
        "systemMonitor": app.cfg.system_monitor.json(),
        "miniToolsHotkeys": TOOL_HOTKEYS.with(|c|c.get()),
        "autostartWarn": false,
        "interactions": app.interactions_enabled,
        "widget": {
            "enabled": app.cfg.widget_enabled,
            "locked": app.cfg.widget_locked,
            "x": app.cfg.widget_x,
            "y": app.cfg.widget_y,
            "scale": app.cfg.widget_scale,
            "opacity": app.cfg.widget_opacity,
            "time": { "x": app.cfg.widget_lines[0].x, "y": app.cfg.widget_lines[0].y, "opacity": app.cfg.widget_lines[0].opacity, "size": app.cfg.widget_lines[0].size },
            "weekday": { "x": app.cfg.widget_lines[1].x, "y": app.cfg.widget_lines[1].y, "opacity": app.cfg.widget_lines[1].opacity, "size": app.cfg.widget_lines[1].size },
            "date": { "x": app.cfg.widget_lines[2].x, "y": app.cfg.widget_lines[2].y, "opacity": app.cfg.widget_lines[2].opacity, "size": app.cfg.widget_lines[2].size },
        },
        "lang": app.cfg.lang, "langEff": eff, "langSys": sys,
        "lockCfg": {
            "idleMins": if lock::saver_active() { app.cfg.lock_idle_mins } else { 0 },
            "syncWin": LOCK_SYNC.with(|c| c.get()),
        },
        "caps": {
            // Chỉ dùng khóa thật của Windows: trang hiện nhóm tùy chọn khóa riêng, không có PIN / chặn Win+L
            "realLock": true,
            "interactions": true,
            "widget": true,
            "hotkey": HOTKEY_OK.with(|c| c.get()),
            "share": false,
            "themes": [
                "meadow",
                "beach",
                "beach-turquoise",
                "lake",
                "sky",
                "ocean",
                "rainforest",
                "mocchau",
                "tet-sakura",
                "cat-hill",
                "stadium",
                "aquarium",
                "commute",
                "summer-porch",
                "cats",
                "boxing-ring"
            ],
            // Cây dừa dùng atlas PNG gốc được nhúng trong binary Rust.
            "hide": ["showBoy", "showCat"],
            "force": { "showBoy": 0, "showCat": 0 },
        },
    })
}

fn away_state(app: &App, status: &str) -> serde_json::Value {
    let enabled = lock::saver_active();
    serde_json::json!({
        "t": "awayState",
        "enabled": enabled,
        "minutes": if enabled { app.cfg.lock_idle_mins } else { app.cfg.lock_idle_mins.max(1) },
        "busy": false,
        "status": status,
    })
}

/// Vị trí biểu tượng trên màn hình chính (toạ độ trong màn hình, điểm ảnh vật lý) như bản C#.
fn icons_json(app: &App) -> String {
    let Some(p) = primary_wall(app) else { return String::new() };
    let s = p.screen;
    let r: Vec<[i32; 4]> = p.last_icons.as_deref().unwrap_or(&[]).iter().map(|r| [r.left - s.left, r.top - s.top, r.right - s.left, r.bottom - s.top]).collect();
    serde_json::json!({ "t": "icons", "vis": !r.is_empty(), "r": r }).to_string()
}

// ---------------------------------------------------------------- Một cửa sổ hình nền / màn hình
struct Wallpaper {
    hwnd: HWND,
    hdc: HDC,
    /// DC dùng cho wglMakeCurrent: chính cửa sổ (chế độ gl) hoặc cửa sổ ẩn (chế độ gdi)
    gl_dc: HDC,
    /// Chế độ gdi: FBO độ phân giải cảnh + PBO đọc về không chặn.
    /// Khung N được đọc vào PBO, đầu khung N+1 (sau ~50 ms, GPU đã xong) mới map và vẽ ra cửa sổ.
    out: Option<(glow::Framebuffer, glow::Texture, glow::Buffer)>,
    pbo_ready: bool,
    gpu: Option<gpu_present::Output>,
    benchmark_target: Option<presentation::MemoryTarget>,
    screen: RECT,
    dpr: f32,
    /// None ở chế độ trình bảo vệ màn hình (cửa sổ toàn màn hình trên cùng, không gắn vào desktop)
    attach: Option<desktop::Attach>,
    // Panorama owns one simulation on output 0; other outputs only present it.
    theme: Option<Box<dyn Theme>>,
    fbo: glow::Framebuffer,
    fbo_tex: glow::Texture,
    buf_w: i32,
    buf_h: i32,
    phys_w: i32,
    phys_h: i32,
    last_icons: Option<Vec<RECT>>,
    paused: bool,
    paused_since: Option<f64>,
    needs_settle: bool,
    last_frame: f64,
}

/// Canvas ảo dùng cho chế độ toàn cảnh. Một theme duy nhất được vẽ vào
/// texture này rồi cắt ra từng màn hình; nhờ vậy cảnh, sóng và vật thể không
/// bị khởi tạo lại thành các đảo độc lập ở mỗi monitor.
struct PanoramaSurface {
    fbo: glow::Framebuffer,
    tex: glow::Texture,
    stencil: glow::Renderbuffer,
    buf_w: i32,
    buf_h: i32,
    scale: f32,
    world: RECT,
}

fn output_surface(gl:&glow::Context,width:i32,height:i32)->Result<(glow::Framebuffer,glow::Texture,glow::Buffer),String> {unsafe {
    let texture=gl.create_texture()?;gl.bind_texture(glow::TEXTURE_2D,Some(texture));
    gl.tex_image_2d(glow::TEXTURE_2D,0,glow::RGBA8 as i32,width,height,0,glow::RGBA,glow::UNSIGNED_BYTE,None);
    let framebuffer=gl.create_framebuffer()?;gl.bind_framebuffer(glow::FRAMEBUFFER,Some(framebuffer));
    gl.framebuffer_texture_2d(glow::FRAMEBUFFER,glow::COLOR_ATTACHMENT0,glow::TEXTURE_2D,Some(texture),0);
    gl.bind_framebuffer(glow::FRAMEBUFFER,None);
    let buffer=gl.create_buffer()?;gl.bind_buffer(glow::PIXEL_PACK_BUFFER,Some(buffer));
    gl.buffer_data_size(glow::PIXEL_PACK_BUFFER,width*height*4,glow::STREAM_READ);gl.bind_buffer(glow::PIXEL_PACK_BUFFER,None);
    Ok((framebuffer,texture,buffer))
}}

struct App {
    ctx: Option<wgl::GlContext>,
    shared: Option<Shared>,
    vao: Option<glow::VertexArray>,
    walls: Vec<Wallpaper>,
    panorama: Option<PanoramaSurface>,
    reader: desktop::IconReader,
    cfg: Config,
    environment: Environment,
    start: Instant,
    frames: u64,
    frame_cpu_ms: f64,
    /// true: OpenGL chỉ vẽ vào FBO, đọc về rồi StretchDIBits lên cửa sổ hình nền (chắc chắn hiện
    /// trên cửa sổ con phân lớp). false: blit + SwapBuffers trực tiếp (nhanh hơn nhưng nhiều driver
    /// không hiển thị OpenGL trên cửa sổ WS_EX_LAYERED).
    gdi: bool,
    dummy: Option<(HWND, HDC)>,
    /// Chạy như trình bảo vệ màn hình Windows (`/s`)
    saver: bool,
    headless: bool,
    interactions_enabled: bool,
    boost_until: f64,
    live_weather: Arc<StdMutex<Option<LiveWeather>>>,
    weather_fetch_at: f64,
    environment_update_at: f64,
    widget: Option<widget::Widget>,
    system_monitor: Option<system_monitor::Monitor>,
    // One cached CPU buffer shared by all outputs, never scaled while GPU-mapped.
    staging: RefCell<Vec<u8>>,
    benchmark_gdi: bool,
    benchmark_direct_readback: bool,
    prefer_gpu: bool,
    gpu_device: Option<std::rc::Rc<gpu_present::Device>>,
    gpu_attempted: bool,
}

impl App {
    fn configure_system_monitor(&mut self, owner: HWND) {
        if self.saver || self.headless || !self.cfg.system_monitor.enabled {
            self.system_monitor = None;
        } else if let Some(monitor) = self.system_monitor.as_ref() {
            monitor.set_interval(self.cfg.system_monitor.interval);
        } else {
            match system_monitor::Monitor::new(owner, &self.cfg.system_monitor) {
                Ok(monitor) => { self.system_monitor = Some(monitor); }
                Err(error) => log!("Thống kê taskbar: {error}"),
            }
        }
    }

    fn init_widget(&mut self) {
        if self.saver || self.headless || !self.cfg.widget_enabled || self.walls.is_empty() { return; }
        let Some(screen) = primary_wall(self).map(|w| w.screen) else { return };
        let lang_vi = self.cfg.lang == "vi" || (self.cfg.lang == "auto" && lang_eff("auto").0 == "vi");
        match widget::Widget::new(screen, self.cfg.widget_x, self.cfg.widget_y, self.cfg.widget_locked, self.cfg.widget_scale, self.cfg.widget_lines, self.cfg.widget_opacity) {
            Ok(w) => { self.widget = Some(w); }
            Err(e) => log!("Không tạo được đồng hồ native: {e}"),
        }
    }

    fn set_widget_enabled(&mut self, on: bool) {
        self.cfg.widget_enabled = on;
        WIDGET_ENABLED.with(|c| c.set(on));
        if on {
            if self.widget.is_none() { self.init_widget(); }
            if let Some(w) = self.widget.as_mut() { w.set_visible(true); }
        } else if let Some(w) = self.widget.as_mut() { w.set_visible(false); }
    }
}

impl App {
    fn ensure_dummy(&mut self) -> Result<HDC, String> {
        if let Some((_, dc)) = self.dummy {
            return Ok(dc);
        }
        let cls = wide(WALL_CLASS);
        let hwnd = unsafe {
            CreateWindowExW(WS_EX_TOOLWINDOW, cls.as_ptr(), cls.as_ptr(), WS_POPUP, 0, 0, 16, 16, std::ptr::null_mut(), std::ptr::null_mut(), GetModuleHandleW(std::ptr::null()), std::ptr::null())
        };
        if hwnd.is_null() {
            return Err("Không tạo được cửa sổ ẩn cho OpenGL".into());
        }
        let dc = unsafe { GetDC(hwnd) };
        if !wgl::set_pixel_format(dc) {
            return Err("SetPixelFormat (cửa sổ ẩn) thất bại".into());
        }
        self.dummy = Some((hwnd, dc));
        Ok(dc)
    }

    /// Transfer at the existing scene resolution; GDI scales directly to the
    /// physical window, avoiding full-resolution GPU readback every frame.
    /// Ngữ cảnh GL phải đang current.
    fn present_gdi(gl: &glow::Context, wp: &Wallpaper, staging:&RefCell<Vec<u8>>, direct:bool) {
        let Some((_, _, pbo)) = wp.out else { return };
        if !wp.pbo_ready {
            return;
        }
        let n = wp.buf_w * wp.buf_h * 4;
        unsafe {
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(pbo));
            let draw=|ptr:*const u8| {
                let mut bmi: BITMAPINFO = std::mem::zeroed();
                bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                bmi.bmiHeader.biWidth = wp.buf_w;
                bmi.bmiHeader.biHeight = wp.buf_h; // dương = hàng dưới lên trên, trùng thứ tự OpenGL
                bmi.bmiHeader.biPlanes = 1;
                bmi.bmiHeader.biBitCount = 32;
                bmi.bmiHeader.biCompression = BI_RGB;
                let dc=wp.benchmark_target.as_ref().map_or(wp.hdc,|target|target.dc);
                StretchDIBits(dc,0,0,wp.phys_w,wp.phys_h,0,0,wp.buf_w,wp.buf_h,ptr as *const _,&bmi,DIB_RGB_COLORS,SRCCOPY);
            };
            if direct {
                // Diagnostic comparison only: reproduce the old mapped-memory
                // path on an offscreen memory target, never on the live desktop.
                let ptr=gl.map_buffer_range(glow::PIXEL_PACK_BUFFER,0,n,glow::MAP_READ_BIT);
                if !ptr.is_null() {draw(ptr);gl.unmap_buffer(glow::PIXEL_PACK_BUFFER);}
            } else {
                let mut pixels=staging.borrow_mut();
                if pixels.len()<n as usize {pixels.resize(n as usize,0);}
                // Driver-managed readback copies sequentially into cacheable RAM.
                // HALFTONE otherwise repeatedly reads mapped GPU/staging memory
                // while resampling, keeping the copy engine and CPU busy.
                gl.get_buffer_sub_data(glow::PIXEL_PACK_BUFFER,0,&mut pixels[..n as usize]);
                gl.bind_buffer(glow::PIXEL_PACK_BUFFER,None);
                draw(pixels.as_ptr());
            }
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
        }
    }

    fn repaint_all(&self) {
        if !self.gdi {
            return;
        }
        let Some(ctx) = self.ctx.as_ref() else { return };
        for wp in &self.walls {
            if wp.gpu.is_some() { continue; } // DComp retains the last frame.
            ctx.make_current(wp.gl_dc);
            Self::present_gdi(&ctx.gl, wp,&self.staging,self.benchmark_direct_readback);
        }
    }

    fn release_hidden_outputs(&mut self,now:f64,hidden:bool) {
        let Some(ctx)=self.ctx.as_ref() else {return};
        for wp in &mut self.walls {
            if !hidden && !wp.paused {wp.paused_since=None;continue;}
            let since=*wp.paused_since.get_or_insert(now);
            if now-since<10_000.0 || wp.out.is_none() {continue;}
            ctx.make_current(wp.gl_dc);
            // Drain the last pending readback once, retain the scene and the
            // window's displayed pixels, then release full-size output buffers.
            Self::present_gdi(&ctx.gl,wp,&self.staging,self.benchmark_direct_readback);
            if let Some((fbo,texture,buffer))=wp.out.take() {unsafe {
                ctx.gl.delete_buffer(buffer);ctx.gl.delete_framebuffer(fbo);ctx.gl.delete_texture(texture);ctx.gl.flush();
            }}
            wp.pbo_ready=false;
            log!("Giải phóng bộ đệm màn hình đang nghỉ [{},{}]",wp.screen.left,wp.screen.top);
        }
        if self.walls.iter().all(|w|w.out.is_none()) && !self.staging.borrow().is_empty() {
            *self.staging.borrow_mut()=Vec::new();
        }
    }

    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    fn maybe_fetch_live_weather(&mut self, now: f64) {
        if self.environment.source != "live" || self.environment.location.is_none() || now < self.weather_fetch_at { return; }
        self.weather_fetch_at = now + 900_000.0;
        fetch_live_weather(&self.environment, self.live_weather.clone());
    }

    fn seed(&self) -> i32 {
        self.cfg
            .params
            .get(&self.cfg.theme)
            .and_then(|p| p.get("seed"))
            .and_then(|s| s.as_i64())
            .map(|s| s as i32)
            .unwrap_or_else(|| (self.start.elapsed().as_nanos() % 1_000_000) as i32)
    }

    fn make_theme(&self, gl: &glow::Context, w: f32, h: f32, buf_w: i32, buf_h: i32, scale: f32, idx: usize) -> Box<dyn Theme> {
        let seed = self.seed();
        let p = self.cfg.params.get(&self.cfg.theme);
        let now = self.now();
        let rs = 0x5EED_0000 + idx as u64 * 7919 + self.start.elapsed().as_nanos() as u64;
        match self.cfg.theme.as_str() {
            "meadow" => Box::new(themes::meadow::Meadow::new(gl, w, h, buf_w, buf_h, scale, seed, themes::meadow::Params::from_json(p), rs, now)),
            "beach-turquoise" => Box::new(themes::maldives::Maldives::new(gl, w, h, buf_w, buf_h, scale, seed, themes::maldives::Params::from_json(p), rs, now)),
            "lake" => Box::new(themes::lake::Lake::new(gl, w, h, buf_w, buf_h, scale, seed, themes::lake::Params::from_json(p), rs, now)),
            "sky" => Box::new(themes::sky::Sky::new(gl, w, h, buf_w, buf_h, scale, seed, themes::sky::Params::from_json(p), rs, now)),
            "ocean" => Box::new(themes::ocean::Ocean::new(gl, w, h, buf_w, buf_h, scale, seed, themes::ocean::Params::from_json(p), rs, now)),
            "rainforest" => Box::new(themes::rainforest::Rainforest::new(gl, w, h, buf_w, buf_h, scale, seed, themes::rainforest::Params::from_json(p), rs, now)),
            "mocchau" => Box::new(themes::mocchau::PlumBlossoms::new(gl, w, h, buf_w, buf_h, scale, seed, themes::mocchau::Params::from_json(p), rs, now)),
            "tet-sakura" => Box::new(themes::tet_sakura::TetSakura::new(gl, w, h, buf_w, buf_h, scale, seed, themes::tet_sakura::Params::from_json(p), rs, now)),
            "cat-hill" => Box::new(themes::cat_hill::CatHill::new(gl, w, h, buf_w, buf_h, scale, seed, themes::cat_hill::Params::from_json(p), rs, now)),
            "stadium" => Box::new(themes::stadium::Stadium::new(gl, w, h, buf_w, buf_h, scale, seed, themes::stadium::Params::from_json(p), rs, now)),
            "commute" => Box::new(themes::commute::Commute::new(gl, w, h, buf_w, buf_h, scale, seed, themes::commute::Params::from_json(p), rs, now)),
            "aquarium" => Box::new(themes::aquarium::Aquarium::new(gl, w, h, buf_w, buf_h, scale, seed, themes::aquarium::Params::from_json(p), rs, now)),
            "summer-porch" => Box::new(themes::summer_porch::SummerPorch::new(gl, w, h, buf_w, buf_h, scale, seed, themes::summer_porch::Params::from_json(p), rs, now)),
            "cats" => Box::new(themes::cats::Cats::new(gl, w, h, buf_w, buf_h, scale, seed, themes::cats::Params::from_json(p), rs, now)),
            "boxing-ring" => Box::new(themes::boxing_ring::BoxingRing::new(gl, w, h, buf_w, buf_h, scale, seed, themes::boxing_ring::Params::from_json(p), rs, now)),
            _ => Box::new(themes::beach::Beach::new(gl, w, h, buf_w, buf_h, scale, seed, themes::beach::Params::from_json(p), rs, now)),
        }
    }

    fn build_shared(&mut self) -> Result<(), String> {
        let ctx = self.ctx.as_ref().unwrap();
        let gl = &ctx.gl;
        let seed = self.seed();
        let noise = engine::Noise { seed };
        let frag = match self.cfg.theme.as_str() {
            "meadow" => themes::meadow::FRAG,
            "beach-turquoise" => themes::maldives::FRAG,
            "lake" => themes::lake::FRAG,
            "sky" => themes::sky::FRAG,
            "ocean" => themes::ocean::FRAG,
            "rainforest" => themes::rainforest::FRAG,
            "mocchau" => themes::mocchau::FRAG,
            "tet-sakura" => themes::tet_sakura::FRAG,
            "cat-hill" => themes::cat_hill::FRAG,
            "stadium" => themes::stadium::FRAG,
            "aquarium" => themes::aquarium::FRAG,
            "commute" => themes::commute::FRAG,
            "summer-porch" => themes::summer_porch::FRAG,
            "cats" => themes::cats::FRAG,
            "boxing-ring" => themes::boxing_ring::FRAG,
            _ => themes::beach::FRAG,
        };
        let t0 = Instant::now();
        let prog = Prog::new(gl, frag)?;
        log!("Biên dịch shader '{}' mất {:.0} ms", self.cfg.theme, t0.elapsed().as_secs_f64() * 1000.0);
        let noise_tex = engine::build_noise_tex(gl, &noise);
        let post = engine::Prog::new(gl, engine::POST_FRAG)?;
        let crop = engine::Prog::new(gl, engine::CROP_FRAG)?;
        let palm = engine::Prog::from_parts(gl, engine::PALM_VERT, engine::PALM_FRAG)?;
        let palm_atlas = engine::build_palm_atlas(gl)?;
        self.shared = Some(Shared { noise_tex, seed, prog, post, crop, palm, palm_atlas, sun: self.environment.sun(), day_phase: self.environment.day_phase(), weather: self.environment.weather_amount(self.now()) });
        Ok(())
    }

    fn drop_shared(&mut self) {
        if let (Some(ctx), Some(s)) = (self.ctx.as_ref(), self.shared.take()) {
            unsafe {
                ctx.gl.delete_program(s.prog.prog);
                ctx.gl.delete_program(s.post.prog);
                ctx.gl.delete_program(s.crop.prog);
                ctx.gl.delete_program(s.palm.prog);
                ctx.gl.delete_texture(s.noise_tex);
                ctx.gl.delete_texture(s.palm_atlas);
            }
        }
    }

    fn create_walls(&mut self) -> Result<(), String> {
        self.ensure_seed();
        let mons = monitors();
        log!("Phát hiện {} màn hình", mons.len());
        let panorama_world = if self.cfg.mode == "panorama" && !mons.is_empty() {
            let left = mons.iter().map(|m| m.rect.left).min().unwrap_or(0);
            let top = mons.iter().map(|m| m.rect.top).min().unwrap_or(0);
            let right = mons.iter().map(|m| m.rect.right).max().unwrap_or(1920);
            let bottom = mons.iter().map(|m| m.rect.bottom).max().unwrap_or(1080);
            Some(RECT { left, top, right, bottom })
        } else { None };
        let panorama_scale = mons.iter().map(|m| m.dpr).fold(0.72f32, f32::min).clamp(0.5, 0.72);
        let cls = wide(if self.saver { SAVER_CLASS } else { WALL_CLASS });
        let title = wide("tuanpc-deskWindow");
        let ex_style = if self.saver { WS_EX_TOPMOST | WS_EX_TOOLWINDOW } else { WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE };
        for (idx, m) in mons.iter().enumerate() {
            let (pw, ph) = (m.rect.right - m.rect.left, m.rect.bottom - m.rect.top);
            let hwnd = unsafe {
                CreateWindowExW(
                    ex_style,
                    cls.as_ptr(),
                    title.as_ptr(),
                    WS_POPUP,
                    m.rect.left,
                    m.rect.top,
                    pw,
                    ph,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    GetModuleHandleW(std::ptr::null()),
                    std::ptr::null(),
                )
            };
            if hwnd.is_null() {
                return Err("CreateWindowExW thất bại".into());
            }
            let attach = if self.headless {
                None
            } else if self.saver {
                unsafe {
                    SetWindowPos(hwnd, HWND_TOPMOST, m.rect.left, m.rect.top, pw, ph, SWP_SHOWWINDOW);
                }
                None
            } else {
                Some(desktop::attach(hwnd, m.rect).ok_or("Không gắn được vào lớp hình nền")?)
            };
            let hdc = unsafe { GetDC(hwnd) };
            unsafe {SetStretchBltMode(hdc,HALFTONE);SetBrushOrgEx(hdc,0,0,std::ptr::null_mut());}
            let gl_dc = if self.gdi {
                self.ensure_dummy()?
            } else {
                if !wgl::set_pixel_format(hdc) {
                    return Err("SetPixelFormat thất bại".into());
                }
                hdc
            };
            if self.ctx.is_none() {
                let ctx = wgl::GlContext::new(gl_dc)?;
                unsafe {
                    log!(
                        "OpenGL: {} | {} | GLSL {}",
                        ctx.gl.get_parameter_string(glow::RENDERER),
                        ctx.gl.get_parameter_string(glow::VERSION),
                        ctx.gl.get_parameter_string(glow::SHADING_LANGUAGE_VERSION)
                    );
                    let vao = ctx.gl.create_vertex_array()?;
                    ctx.gl.bind_vertex_array(Some(vao));
                    self.vao = Some(vao);
                }
                self.ctx = Some(ctx);
                self.build_shared()?;
            }
            let ctx = self.ctx.as_ref().unwrap();
            ctx.make_current(gl_dc);
            if self.prefer_gpu && !self.gpu_attempted {
                self.gpu_attempted=true;
                match gpu_present::Device::new() {
                    Ok(device)=>self.gpu_device=Some(device),
                    Err(e)=>log!("GPU trực tiếp không khả dụng, dùng GDI dự phòng: {e}"),
                }
            }
            // Giống bản JS: canvas = kích thước CSS * min(dpr, 0.72), trình duyệt phóng lên toàn màn hình
            let (w, h) = (pw as f32 / m.dpr, ph as f32 / m.dpr);
            let scale = m.dpr.min(0.72);
            let (buf_w, buf_h) = ((w * scale).round() as i32, (h * scale).round() as i32);
            let gl = &ctx.gl;
            let (fbo, fbo_tex) = unsafe {
                let t = gl.create_texture()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(t));
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, buf_w, buf_h, 0, glow::RGBA, glow::UNSIGNED_BYTE, None);
                engine::set_tex_params(gl, glow::LINEAR, glow::CLAMP_TO_EDGE);
                let f = gl.create_framebuffer()?;
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(f));
                gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(t), 0);
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                (f, t)
            };
            let gpu=if let Some(device)=&self.gpu_device {
                match gpu_present::Output::new(device.clone(),gl,hwnd,buf_w,buf_h,pw,ph) {
                    Ok(output)=>Some(output),Err(e)=>{log!("Màn hình {idx} dùng GDI dự phòng: {e}");None}
                }
            } else {None};
            let out = if self.gdi && gpu.is_none() {Some(output_surface(gl,buf_w,buf_h)?)}else{None};
            // Ở panorama, mọi theme nhận cùng kích thước canvas ảo. Chỉ theme
            // của màn hình đầu tiên được vẽ; các cửa sổ còn lại nhận viewport
            // cắt từ canvas đó ở render().
            let (theme_w, theme_h, theme_bw, theme_bh, theme_scale) = if let Some(world) = panorama_world {
                let ww = (world.right - world.left).max(1) as f32;
                let wh = (world.bottom - world.top).max(1) as f32;
                (ww, wh, (ww * panorama_scale).round() as i32, (wh * panorama_scale).round() as i32, panorama_scale)
            } else {
                (w, h, buf_w, buf_h, scale)
            };
            let mut theme = if panorama_world.is_none() || idx == 0 {
                Some(self.make_theme(gl, theme_w, theme_h, theme_bw, theme_bh, theme_scale, idx))
            } else { None };
            if let (Some(world),Some(theme)) = (panorama_world,theme.as_mut()) {
                theme.set_viewports(mons.iter().map(|m| [
                    (m.rect.left-world.left) as f32, (m.rect.top-world.top) as f32,
                    (m.rect.right-m.rect.left) as f32, (m.rect.bottom-m.rect.top) as f32,
                ]).collect());
            }
            log!("Màn hình {idx}: {pw}x{ph}, dpr {:.2}, bộ đệm vẽ {buf_w}x{buf_h}", m.dpr);
            self.walls.push(Wallpaper {
                hwnd,
                hdc,
                gl_dc,
                out,
                pbo_ready: false,
                gpu,
                benchmark_target:if self.benchmark_gdi {Some(presentation::MemoryTarget::new(pw,ph)?)}else{None},
                screen: m.rect,
                dpr: m.dpr,
                attach,
                theme,
                fbo,
                fbo_tex,
                buf_w,
                buf_h,
                phys_w: pw,
                phys_h: ph,
                last_icons: None,
                paused: false,
                paused_since: None,
                needs_settle: false,
                last_frame: 0.0,
            });
        }
        if let Some(world) = panorama_world {
            let ww = (world.right - world.left).max(1);
            let wh = (world.bottom - world.top).max(1);
            let bw = (ww as f32 * panorama_scale).round() as i32;
            let bh = (wh as f32 * panorama_scale).round() as i32;
            let ctx = self.ctx.as_ref().unwrap();
            ctx.make_current(self.walls[0].gl_dc);
            let gl = &ctx.gl;
            let tex = unsafe {
                let t = gl.create_texture()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(t));
                gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, bw, bh, 0, glow::RGBA, glow::UNSIGNED_BYTE, None);
                engine::set_tex_params(gl, glow::LINEAR, glow::CLAMP_TO_EDGE);
                t
            };
            let (fbo,stencil) = unsafe {
                let f = gl.create_framebuffer()?;
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(f));
                gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
                // Panorama contains holes when monitors have different sizes
                // or heights. Mask those once so the expensive scene shader
                // never shades pixels that cannot appear on any monitor.
                let stencil=gl.create_renderbuffer()?;
                gl.bind_renderbuffer(glow::RENDERBUFFER,Some(stencil));
                gl.renderbuffer_storage(glow::RENDERBUFFER,glow::STENCIL_INDEX8,bw,bh);
                gl.framebuffer_renderbuffer(glow::FRAMEBUFFER,glow::STENCIL_ATTACHMENT,glow::RENDERBUFFER,Some(stencil));
                if gl.check_framebuffer_status(glow::FRAMEBUFFER)!=glow::FRAMEBUFFER_COMPLETE{return Err("Không tạo được stencil panorama".into());}
                gl.stencil_mask(0xff);gl.clear_stencil(0);gl.clear(glow::STENCIL_BUFFER_BIT);
                gl.enable(glow::SCISSOR_TEST);gl.clear_stencil(1);
                for monitor in &mons {
                    let r=panorama_clip(monitor.rect,world,panorama_scale,bw,bh);
                    // One guard texel for linear filtering at monitor edges.
                    gl.scissor((r[0]-1).max(0),(r[1]-1).max(0),(r[0]+r[2]+1).min(bw)-(r[0]-1).max(0),(r[1]+r[3]+1).min(bh)-(r[1]-1).max(0));gl.clear(glow::STENCIL_BUFFER_BIT);
                }
                gl.disable(glow::SCISSOR_TEST);gl.stencil_mask(0);
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                (f,stencil)
            };
            self.panorama = Some(PanoramaSurface { fbo, tex, stencil, buf_w: bw, buf_h: bh, scale: panorama_scale, world });
            log!("Panorama canvas: {}x{} -> bộ đệm {}x{}", ww, wh, bw, bh);
        }
        Ok(())
    }

    fn destroy_walls(&mut self) {
        if let Some(p) = self.panorama.take() {
            if let Some(ctx) = self.ctx.as_ref() {
                ctx.make_current(self.walls.first().map(|w| w.gl_dc).unwrap_or(std::ptr::null_mut()));
                unsafe { ctx.gl.delete_framebuffer(p.fbo); ctx.gl.delete_texture(p.tex);ctx.gl.delete_renderbuffer(p.stencil); }
            }
        }
        let walls = std::mem::take(&mut self.walls);
        for mut wp in walls {
            if let Some(ctx) = self.ctx.as_ref() {
                ctx.make_current(wp.gl_dc);
                if let Some(mut gpu)=wp.gpu.take(){gpu.destroy(&ctx.gl);}
                if let Some(theme)=wp.theme.as_mut() { theme.destroy(&ctx.gl); }
                unsafe {
                    ctx.gl.delete_framebuffer(wp.fbo);
                    ctx.gl.delete_texture(wp.fbo_tex);
                    if let Some((f, t, b)) = wp.out.take() {
                        ctx.gl.delete_framebuffer(f);
                        ctx.gl.delete_texture(t);
                        ctx.gl.delete_buffer(b);
                    }
                }
            }
            if let Some(a) = &wp.attach {
                desktop::detach(wp.hwnd, a);
            }
            unsafe {
                ReleaseDC(wp.hwnd, wp.hdc);
                DestroyWindow(wp.hwnd);
            }
        }
    }

    fn switch_theme(&mut self, theme: &str) {
        if self.cfg.theme == theme {
            return;
        }
        log!("Đổi chủ đề -> {theme}");
        self.cfg.theme = theme.into();
        self.rebuild_theme();
    }

    /// Đổi layout đa màn hình bằng một chu kỳ tháo/gắn hoàn chỉnh. Trước đây
    /// chỉ đổi chuỗi `mode`, khiến renderer cũ vẫn giữ viewport cũ và treo khi
    /// chuyển sang panorama.
    fn switch_mode(&mut self, mode: &str) -> Result<(), String> {
        if self.cfg.mode == mode { return Ok(()); }
        let old = self.cfg.mode.clone();
        self.cfg.mode = mode.to_string();
        self.destroy_walls();
        match self.create_walls() {
            Ok(()) => {
                self.fix_z_order();
                Ok(())
            }
            Err(e) => {
                log!("Dựng layout {mode} thất bại: {e}; khôi phục {old}");
                self.destroy_walls();
                self.cfg.mode = old;
                self.create_walls().map_err(|restore| format!("{e}; khôi phục cũng lỗi: {restore}"))
            }
        }
    }

    /// Thông số của chủ đề hiện tại (tạo object nếu chưa có).
    fn theme_params_mut(&mut self) -> &mut serde_json::Map<String, serde_json::Value> {
        if !self.cfg.params.is_object() {
            self.cfg.params = serde_json::json!({});
        }
        let theme = self.cfg.theme.clone();
        let root = self.cfg.params.as_object_mut().unwrap();
        let e = root.entry(theme).or_insert_with(|| serde_json::json!({}));
        if !e.is_object() {
            *e = serde_json::json!({});
        }
        e.as_object_mut().unwrap()
    }

    /// Giữ cố định bố cục khi chỉnh thông số: chưa có seed thì tạo và lưu lại.
    fn ensure_seed(&mut self) {
        let fresh = (self.start.elapsed().as_nanos() % 1_000_000) as i64;
        self.theme_params_mut().entry("seed").or_insert(serde_json::json!(fresh));
    }

    fn reseed(&mut self) {
        let fresh = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0) % 1_000_000) as i64;
        self.theme_params_mut().insert("seed".into(), serde_json::json!(fresh));
    }

    fn settings_state(&self) -> settings::State {
        settings::State {
            theme: self.cfg.theme.clone(),
            params: self.cfg.params.get(&self.cfg.theme).cloned().unwrap_or(serde_json::Value::Null),
            saver: lock::saver_active(),
            idle: self.cfg.lock_idle_mins,
            sync: LOCK_SYNC.with(|c| c.get()),
            autostart: settings::autostart_enabled(),
            hotkey: HOTKEY_OK.with(|c| c.get()),
            interaction: self.interactions_enabled,
        }
    }

    /// Dựng lại cảnh của chủ đề hiện tại (đổi chủ đề / thông số / seed).
    fn rebuild_theme(&mut self) {
        let now = self.now();
        self.drop_shared();
        if let Err(e) = self.build_shared() {
            log!("Lỗi dựng chủ đề: {e}");
            return;
        }
        let ctx = self.ctx.take().unwrap();
        let mut walls = std::mem::take(&mut self.walls);
        let viewports = self.panorama.as_ref().map(|pan| walls.iter().map(|wp| [
            (wp.screen.left-pan.world.left) as f32, (wp.screen.top-pan.world.top) as f32,
            wp.phys_w as f32, wp.phys_h as f32,
        ]).collect::<Vec<_>>());
        for (i, wp) in walls.iter_mut().enumerate() {
            ctx.make_current(wp.gl_dc);
            if let Some(theme)=wp.theme.take().as_mut() { theme.destroy(&ctx.gl); }
            if self.panorama.is_some() && i > 0 { continue; }
            let (w, h, bw, bh, scale) = if let Some(pan) = self.panorama.as_ref() {
                ( (pan.world.right - pan.world.left) as f32,
                  (pan.world.bottom - pan.world.top) as f32,
                  pan.buf_w, pan.buf_h, pan.scale )
            } else {
                (wp.phys_w as f32 / wp.dpr, wp.phys_h as f32 / wp.dpr, wp.buf_w, wp.buf_h, wp.dpr.min(0.72))
            };
            let mut theme = self.make_theme(&ctx.gl, w, h, bw, bh, scale, i);
            if let Some(ref v) = viewports { theme.set_viewports(v.clone()); }
            wp.theme = Some(theme);
            wp.last_icons = None;
            wp.last_frame = now;
        }
        self.walls = walls;
        self.ctx = Some(ctx);
        PENDING_THEME_SYNC.with(|c| c.set(true));
    }

    fn poll_icons(&mut self) {
        let Some((rects, visible)) = self.reader.read() else { return };
        let now = self.now();
        let ctx = self.ctx.as_ref().unwrap();
        if let Some(pan) = self.panorama.as_ref() {
            let world = pan.world;
            let desired_rects = if visible { rects.clone() } else { Vec::new() };
            // Panorama dùng một theme chung; không được commit lại trường và
            // bố trí lại toàn bộ cây mỗi chu kỳ đọc icon nếu dữ liệu không đổi.
            if self.walls.first().and_then(|w| w.last_icons.as_ref()).is_some_and(|old| rects_eq(old, &desired_rects)) {
                return;
            }
            let boxes: Vec<IconBox> = desired_rects.iter().map(|r| IconBox {
                x: (r.left - world.left) as f32,
                y: (r.top - world.top) as f32,
                w: (r.right - r.left) as f32,
                h: (r.bottom - r.top) as f32,
            }).collect();
            let first = self.walls.first().and_then(|w| w.last_icons.as_ref()).is_none();
            if let Some(wp0) = self.walls.first_mut() {
                ctx.make_current(wp0.gl_dc);
                if let Some(theme)=wp0.theme.as_mut() { theme.set_icons(&ctx.gl, boxes, first, now); }
                wp0.last_icons = Some(desired_rects);
                if wp0.paused { wp0.needs_settle = true; }
            }
            return;
        }
        for wp in &mut self.walls {
            let s = wp.screen;
            let mine: Vec<RECT> = rects
                .iter()
                .filter(|r| {
                    let (cx, cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
                    cx >= s.left && cx < s.right && cy >= s.top && cy < s.bottom
                })
                .copied()
                .collect();
            let mine = if visible { mine } else { Vec::new() };
            if wp.last_icons.as_ref().is_some_and(|l| rects_eq(l, &mine)) {
                continue;
            }
            let first = wp.last_icons.is_none();
            let k = wp.dpr;
            let boxes: Vec<IconBox> = mine
                .iter()
                .map(|r| IconBox {
                    x: (r.left - s.left) as f32 / k,
                    y: (r.top - s.top) as f32 / k,
                    w: (r.right - r.left) as f32 / k,
                    h: (r.bottom - r.top) as f32 / k,
                })
                .collect();
            ctx.make_current(wp.gl_dc);
            let Some(theme)=wp.theme.as_mut() else {continue};
            theme.set_icons(&ctx.gl, boxes, first, now);
            log!("Biểu tượng thay đổi (màn hình [{},{}]): {} mục, trường {:.1} ms", s.left, s.top, mine.len(), theme.field_ms());
            wp.last_icons = Some(mine);
            if wp.paused {
                wp.needs_settle = true;
            }
        }
    }

    fn poll_foreground(&mut self) {
        let own: Vec<HWND> = self.walls.iter().map(|w| w.hwnd).collect();
        let screens:Vec<RECT>=self.walls.iter().map(|w|w.screen).collect();
        let covered=desktop::covered_monitors(&screens,&own);
        for (wp,p) in self.walls.iter_mut().zip(covered) {
            if p != wp.paused {
                wp.paused = p;
                log!("Màn hình [{},{}] {}", wp.screen.left, wp.screen.top, if p { "tạm dừng (bị che)" } else { "tiếp tục phát" });
            }
        }
        if let Some(w) = self.widget.as_ref() {
            w.sync_z_order();
        }
    }

    fn post_process_texture(gl: &glow::Context, shared: &mut Shared, scene: glow::Texture, target: Option<glow::Framebuffer>, width: i32, height: i32, now: f64, flip_y: bool) {
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, target);
            gl.viewport(0, 0, width, height);
            gl.disable(glow::BLEND);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(scene));
            gl.use_program(Some(shared.post.prog));
        }
        shared.post.i1(gl, "uScene", 0);
        shared.post.f2(gl, "uRes", width as f32, height as f32);
        shared.post.f1(gl, "uTime", (now / 1000.0) as f32);
        shared.post.f1(gl, "uDayPhase", shared.day_phase);
        shared.post.f1(gl, "uWeather", shared.weather);
        shared.post.i1(gl, "uFlipY", flip_y as i32);
        unsafe {
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.use_program(None);
        }
    }

    fn post_process(gl: &glow::Context, shared: &mut Shared, wp: &Wallpaper, target: Option<glow::Framebuffer>, width: i32, height: i32, now: f64) {
        Self::post_process_texture(gl, shared, wp.fbo_tex, target, width, height, now, false);
    }

    fn ensure_output(gl:&glow::Context,wp:&mut Wallpaper,gdi:bool)->bool {
        if gdi && wp.gpu.is_none() && wp.out.is_none() {
            match output_surface(gl,wp.buf_w,wp.buf_h) {
                Ok(surface)=>wp.out=Some(surface),Err(e)=>{log!("Khôi phục bộ đệm lỗi: {e}");return false;}
            }
        }true
    }

    fn present_output(gl:&glow::Context,shared:&mut Shared,wp:&mut Wallpaper,now:f64,gdi:bool) {
        if let Some(gpu)=wp.gpu.as_mut() {
            let result=(||->Result<(),String>{
                let target=gpu.begin()?;
                Self::post_process_texture(gl,shared,wp.fbo_tex,Some(target),wp.buf_w,wp.buf_h,now,true);
                gpu.present(gl)
            })();
            if result.is_ok(){return;}
            log!("Lỗi GPU trực tiếp, trở về GDI: {}",result.unwrap_err());
            if let Some(mut gpu)=wp.gpu.take(){gpu.destroy(gl);}
            if !Self::ensure_output(gl,wp,gdi){return;}
        }
        if let Some((out_fbo,_,pbo))=wp.out {
            Self::post_process(gl,shared,wp,Some(out_fbo),wp.buf_w,wp.buf_h,now);
            unsafe {
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER,Some(out_fbo));
                gl.bind_buffer(glow::PIXEL_PACK_BUFFER,Some(pbo));gl.pixel_store_i32(glow::PACK_ALIGNMENT,4);
                gl.read_pixels(0,0,wp.buf_w,wp.buf_h,glow::BGRA,glow::UNSIGNED_BYTE,glow::PixelPackData::BufferOffset(0));
                gl.bind_buffer(glow::PIXEL_PACK_BUFFER,None);gl.bind_framebuffer(glow::FRAMEBUFFER,None);gl.flush();
            }
            wp.pbo_ready=true;
        } else {
            Self::post_process(gl,shared,wp,None,wp.phys_w,wp.phys_h,now);
            unsafe{SwapBuffers(wp.hdc);}
        }
    }

    fn crop_panorama(gl: &glow::Context, shared: &mut Shared, scene: glow::Texture, target: glow::Framebuffer, dst_w: i32, dst_h: i32, src_x: f32, src_y: f32, src_w: f32, src_h: f32, src_total_w: f32, src_total_h: f32) {
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target));
            gl.viewport(0, 0, dst_w, dst_h);
            gl.disable(glow::BLEND);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(scene));
            gl.use_program(Some(shared.crop.prog));
        }
        shared.crop.i1(gl, "uScene", 0);
        shared.crop.f4v(gl, "uSrcRect", &[src_x, src_y, src_w, src_h]);
        shared.crop.f2(gl, "uSrcSize", src_total_w, src_total_h);
        shared.crop.f2(gl, "uDstSize", dst_w as f32, dst_h as f32);
        unsafe { gl.draw_arrays(glow::TRIANGLES, 0, 3); gl.use_program(None); }
    }

    fn render(&mut self) {
        // Return before weather/solar calculations, simulation, GL or readback.
        let Some(visible)=visible_bounds(self.walls.iter().map(|w|(w.screen,w.paused))) else {
            for wp in &mut self.walls {wp.last_frame=0.0;}
            return;
        };
        let now = self.now();
        self.maybe_fetch_live_weather(now);
        let t0 = Instant::now();
        // Solar angles and weather change slowly; don't recalculate 30 times/s.
        if now >= self.environment_update_at {
            let shared=self.shared.as_mut().unwrap();
            shared.day_phase=self.environment.day_phase();
            shared.sun=self.environment.sun();
            let live=self.live_weather.lock().ok().and_then(|g|*g);
            shared.weather=live_weather_amount(&self.environment,live).unwrap_or_else(||self.environment.weather_amount(now));
            self.environment_update_at=now+1000.0;
        }
        let ctx = self.ctx.as_ref().unwrap();
        let shared = self.shared.as_mut().unwrap();
        let gl = &ctx.gl;
        let mut drew = false;
        if let Some(pan) = self.panorama.as_ref() {
            // Consume last frame before submitting the new scene. Mapping or
            // copying the PBO after drawing can wait on unrelated new GPU work.
            for wp in &self.walls {
                if !wp.paused {ctx.make_current(wp.gl_dc);Self::present_gdi(gl,wp,&self.staging,self.benchmark_direct_readback);}
            }
            // Vẽ một lần vào canvas ảo, sau đó cắt viewport cho từng màn hình.
            ctx.make_current(self.walls[0].gl_dc);
            unsafe {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(pan.fbo)); gl.viewport(0, 0, pan.buf_w, pan.buf_h);
                let clip=panorama_clip(visible,pan.world,pan.scale,pan.buf_w,pan.buf_h);
                gl.enable(glow::SCISSOR_TEST);gl.scissor(clip[0],clip[1],clip[2],clip[3]);
                gl.enable(glow::STENCIL_TEST);gl.stencil_func(glow::EQUAL,1,0xff);gl.stencil_op(glow::KEEP,glow::KEEP,glow::KEEP);
            }
            {
                let wp0 = &mut self.walls[0];
                let dt = if wp0.last_frame > 0.0 { ((now - wp0.last_frame) / 1000.0).min(0.1) as f32 } else { 0.03 };
                wp0.last_frame = now;
                let theme=wp0.theme.as_mut().expect("panorama scene owner");
                theme.frame(gl, shared, dt, now);
                theme.draw_overlay(gl, shared, now);
                wp0.needs_settle=false;
            }
            unsafe {gl.disable(glow::SCISSOR_TEST);gl.disable(glow::STENCIL_TEST);}
            let (world, total_w, total_h, ptex) = (pan.world, (pan.world.right - pan.world.left) as f32 * pan.scale, (pan.world.bottom - pan.world.top) as f32 * pan.scale, pan.tex);
            for wp in &mut self.walls {
                if wp.paused {wp.pbo_ready=false;continue;}
                ctx.make_current(wp.gl_dc);
                if !Self::ensure_output(gl,wp,self.gdi) {continue;}
                let ox = (wp.screen.left - world.left) as f32 * pan.scale;
                let oy = (world.bottom - wp.screen.bottom) as f32 * pan.scale;
                let sw = (wp.screen.right - wp.screen.left) as f32 * pan.scale;
                let sh = (wp.screen.bottom - wp.screen.top) as f32 * pan.scale;
                Self::crop_panorama(gl, shared, ptex, wp.fbo, wp.buf_w, wp.buf_h, ox, oy, sw, sh, total_w, total_h);
                Self::present_output(gl,shared,wp,now,self.gdi);
                drew = true;
            }
            if drew { self.frames += 1; self.frame_cpu_ms += t0.elapsed().as_secs_f64() * 1000.0; }
            return;
        }
        for wp in &mut self.walls {
            if wp.paused {
                wp.last_frame = 0.0;
                wp.pbo_ready=false;
                continue;
            }
            if wp.theme.is_none() {continue;}
            let settle=wp.needs_settle;
            ctx.make_current(wp.gl_dc);
            if !Self::ensure_output(gl,wp,self.gdi) {continue;}
            // Chế độ gdi: hiện khung đã đọc về ở lượt trước (GPU xong từ lâu, map không phải chờ)
            Self::present_gdi(gl, wp,&self.staging,self.benchmark_direct_readback);
            let theme=wp.theme.as_mut().unwrap();
            unsafe {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(wp.fbo));
                gl.viewport(0, 0, wp.buf_w, wp.buf_h);
            }
            if settle {
                theme.settle(gl, shared, now);
                wp.needs_settle = false;
            } else {
                let dt = if wp.last_frame > 0.0 { ((now - wp.last_frame) / 1000.0).min(0.1) as f32 } else { 0.03 };
                wp.last_frame = now;
                theme.frame(gl, shared, dt, now);
            }
            theme.draw_overlay(gl, shared, now);
            Self::present_output(gl,shared,wp,now,self.gdi);
            drew = true;
        }
        if drew {
            self.frames += 1;
            self.frame_cpu_ms += t0.elapsed().as_secs_f64() * 1000.0;
        }
    }

    /// Đọc lại bộ đệm vẽ của từng màn hình từ GPU và lưu BMP (kiểm tra shader không cần chụp màn hình).
    fn snapshot(&self, dir: &str) {
        if let Err(e) = std::fs::create_dir_all(dir) {
            log!("Không tạo được thư mục kiểm tra: {e}");
            return;
        }
        let outputs: Vec<_> = self.walls.iter().map(|w| serde_json::json!({
            "screen": [w.screen.left, w.screen.top, w.screen.right, w.screen.bottom],
            "dpiScale": w.dpr,
            "icons": w.last_icons.as_ref().map(|rs| rs.iter().map(|r|
                [r.left, r.top, r.right, r.bottom]).collect::<Vec<_>>()),
        })).collect();
        let _ = std::fs::write(format!("{dir}\\layout.json"), serde_json::json!({
            "mode": self.cfg.mode, "theme": self.cfg.theme, "outputs": outputs,
        }).to_string());
        for i in 0..self.walls.len() {
            let path = format!("{dir}\\{}-screen{i}.bmp", self.cfg.theme);
            match std::fs::write(&path, self.read_bmp(i)) {
                Ok(_) => log!("Đã lưu ảnh kiểm tra {path}"),
                Err(e) => log!("Lưu ảnh kiểm tra lỗi: {e}"),
            }
        }
    }

    /// Đọc khung hình của màn hình `i` từ GPU thành file BMP 32-bit
    /// (độ phân giải thật nếu có FBO phóng to, không thì độ phân giải bộ đệm vẽ).
    fn read_bmp(&self, i: usize) -> Vec<u8> {
        let ctx = self.ctx.as_ref().unwrap();
        let gl = &ctx.gl;
        let wp = &self.walls[i];
        ctx.make_current(wp.gl_dc);
        let (fbo, w, h) = match wp.out {
            Some((f, _, _)) => (f, wp.buf_w as usize, wp.buf_h as usize),
            None => (wp.fbo, wp.buf_w as usize, wp.buf_h as usize),
        };
        let gpu_pixels=wp.gpu.as_ref().and_then(|gpu|gpu.read_bgra().map_err(|e|log!("Đọc ảnh GPU lỗi: {e}")).ok());
        let mut px = vec![0u8; w * h * 4];
        if let Some(top_down)=gpu_pixels {
            let stride=w*4;
            for(y,row)in px.chunks_exact_mut(stride).enumerate(){row.copy_from_slice(&top_down[(h-1-y)*stride..(h-y)*stride]);}
        } else { unsafe {
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(fbo));
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            gl.read_pixels(0, 0, w as i32, h as i32, glow::BGRA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(&mut px));
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }}
        // BMP 32-bit, hàng dưới lên trên (trùng thứ tự của OpenGL); kênh alpha đặt 255
        for p in px.chunks_exact_mut(4) {
            p[3] = 255;
        }
        let mut f = Vec::with_capacity(54 + px.len());
        let size = (54 + px.len()) as u32;
        f.extend_from_slice(b"BM");
        f.extend_from_slice(&size.to_le_bytes());
        f.extend_from_slice(&[0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0]);
        f.extend_from_slice(&(w as i32).to_le_bytes());
        f.extend_from_slice(&(h as i32).to_le_bytes());
        f.extend_from_slice(&[1, 0, 32, 0, 0, 0, 0, 0]);
        f.extend_from_slice(&(px.len() as u32).to_le_bytes());
        f.extend_from_slice(&[0u8; 16]);
        f.extend_from_slice(&px);
        f
    }

    /// Chụp cảnh màn hình chính rồi đặt làm ảnh màn khóa Windows (chạy nền).
    fn sync_lock_image(&self) {
        if self.walls.is_empty() {
            return;
        }
        let path = lock::data_dir().join("lockscreen.bmp");
        match std::fs::write(&path, self.read_bmp(0)) {
            Ok(_) => lock::sync_lock_image(&path),
            Err(e) => log!("Lưu ảnh màn khóa lỗi: {e}"),
        }
    }

    /// Chụp cảnh màn hình chính làm hình nền tĩnh của Windows, để lúc mở máy (trước khi app chạy)
    /// desktop hiện đúng cảnh này thay vì màn đen.
    fn sync_static_wallpaper(&self) {
        if self.walls.is_empty() {
            return;
        }
        let path = lock::data_dir().join("wallpaper.bmp");
        match std::fs::write(&path, self.read_bmp(0)) {
            Ok(_) => lock::set_static_wallpaper(path),
            Err(e) => log!("Lưu ảnh hình nền tĩnh lỗi: {e}"),
        }
    }

    fn fix_z_order(&self) {
        for wp in &self.walls {
            if let Some(a) = &wp.attach {
                desktop::ensure_z_order(wp.hwnd, a);
            }
        }
        if let Some(w) = self.widget.as_ref() {
            w.sync_z_order();
        }
    }

    fn any_interacting(&self, now: f64) -> bool {
        now < self.boost_until || self.walls.iter().any(|w| w.theme.as_ref().is_some_and(|theme| (!w.paused || self.panorama.is_some()) && theme.is_interacting(now)))
    }

    fn handle_click(&mut self, x: i32, y: i32) -> bool {
        let pt = POINT { x, y };
        let hwnd = unsafe { WindowFromPoint(pt) };
        if hwnd.is_null() {
            return false;
        }
        // Đảm bảo cửa sổ gốc tại điểm nhấn thực sự là lớp Desktop (Progman / WorkerW),
        // tránh bắt nhầm cửa sổ File Explorer (CabinetWClass) cũng có SHELLDLL_DefView.
        let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
        if root.is_null() {
            return false;
        }
        let mut class = [0u16; 64];
        let n = unsafe { GetClassNameW(root, class.as_mut_ptr(), 64) };
        let root_name = String::from_utf16_lossy(&class[..n.max(0) as usize]);
        if !matches!(root_name.as_str(), "Progman" | "WorkerW") {
            return false;
        }

        let now = self.now();
        let Some(ctx) = self.ctx.as_ref() else { return false };
        if let Some(pan) = self.panorama.as_ref() {
            if !self.walls.iter().any(|w| pt.x >= w.screen.left && pt.x < w.screen.right && pt.y >= w.screen.top && pt.y < w.screen.bottom) {
                return false;
            }
            let Some(wp0) = self.walls.first_mut() else { return false };
            ctx.make_current(wp0.gl_dc);
            if let Some(theme)=wp0.theme.as_mut() { theme.handle_click(&ctx.gl, (pt.x - pan.world.left) as f32, (pt.y - pan.world.top) as f32, now); }
            self.boost_until = now + 3_500.0;
            return true;
        }
        for wp in &mut self.walls {
            if pt.x >= wp.screen.left && pt.x < wp.screen.right && pt.y >= wp.screen.top && pt.y < wp.screen.bottom {
                // Không phản hồi khi người dùng nhấn trực tiếp vào biểu tượng trên màn hình
                if let Some(icons) = &wp.last_icons {
                    if icons.iter().any(|r| pt.x >= r.left && pt.x < r.right && pt.y >= r.top && pt.y < r.bottom) {
                        return false;
                    }
                }
                ctx.make_current(wp.gl_dc);
                let (cx, cy) = ((pt.x - wp.screen.left) as f32 / wp.dpr, (pt.y - wp.screen.top) as f32 / wp.dpr);
                if let Some(theme)=wp.theme.as_mut() { theme.handle_click(&ctx.gl, cx, cy, now); }
                self.boost_until = now + 3_500.0;
                return true;
            }
        }
        false
    }

    /// Mô phỏng click kiểm thử cho `--test-click` (không cần chuột thật).
    fn simulate_test_clicks(&mut self) {
        let now = self.now();
        let Some(ctx) = self.ctx.as_ref() else { return };
        for wp in &mut self.walls {
            ctx.make_current(wp.gl_dc);
            let w = (wp.screen.right - wp.screen.left) as f32 / wp.dpr;
            let h = (wp.screen.bottom - wp.screen.top) as f32 / wp.dpr;
            if let Some(theme)=wp.theme.as_mut() {
                theme.handle_click(&ctx.gl, w * 0.56, h * 0.48, now);
                theme.handle_click(&ctx.gl, w * 0.72, h * 0.64, now + 60.0);
            }
        }
        self.boost_until = now + 3_500.0;
    }
}

fn rects_eq(a: &[RECT], b: &[RECT]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.left == y.left && x.top == y.top && x.right == y.right && x.bottom == y.bottom)
}

fn visible_bounds(outputs:impl Iterator<Item=(RECT,bool)>)->Option<RECT> {
    outputs.filter(|(_,paused)|!*paused).map(|(rect,_)|rect).reduce(|a,b|RECT {
        left:a.left.min(b.left),top:a.top.min(b.top),right:a.right.max(b.right),bottom:a.bottom.max(b.bottom),
    })
}

fn panorama_clip(visible:RECT,world:RECT,scale:f32,width:i32,height:i32)->[i32;4] {
    let x=((visible.left-world.left) as f32*scale).floor().clamp(0.0,width as f32) as i32;
    let y=((world.bottom-visible.bottom) as f32*scale).floor().clamp(0.0,height as f32) as i32;
    let right=((visible.right-world.left) as f32*scale).ceil().clamp(x as f32,width as f32) as i32;
    let top=((world.bottom-visible.top) as f32*scale).ceil().clamp(y as f32,height as f32) as i32;
    [x,y,right-x,top-y]
}

fn private_mb() -> (f64, f64) {
    unsafe {
        let mut c: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        GetProcessMemoryInfo(GetCurrentProcess(), &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, c.cb);
        (c.PrivateUsage as f64 / 1048576.0, c.WorkingSetSize as f64 / 1048576.0)
    }
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(path) = arg_value(&args, "--system-monitor-diagnostics") {
        let _ = system_monitor::diagnostics(std::path::Path::new(&path));
        return;
    }
    if let Some(path) = arg_value(&args, "--clock-diagnostics") {
        let _ = widget::write_diagnostics(std::path::Path::new(&path));
        return;
    }
    if mini_tools::dispatch(&args) { return; }
    // `--settings`: mở cửa sổ cài đặt của bản đang chạy (nếu có), không thì khởi động rồi mở
    let force_win32 = args.iter().any(|a| a == "--settings-win32");
    web::DEBUG_PORT.store(args.iter().any(|a| a == "--webview-debug"), std::sync::atomic::Ordering::Relaxed);
    let open_settings = force_win32 || args.iter().any(|a| a == "--settings");
    let open_launcher = args.iter().any(|a| a == "--launcher");
    if open_settings || open_launcher {
        let cls = wide(MSG_CLASS);
        unsafe {
            let h = FindWindowW(cls.as_ptr(), std::ptr::null());
            if !h.is_null() {
                let mut pid = 0;
                GetWindowThreadProcessId(h, &mut pid);
                AllowSetForegroundWindow(pid);
                PostMessageW(h, if open_launcher { WM_OPEN_LAUNCHER } else { WM_OPEN_SETTINGS }, 0, 0);
                return;
            }
        }
    }
    if args.iter().any(|a| a == "--quit") {
        let cls = wide(MSG_CLASS);
        unsafe {
            let h = FindWindowW(cls.as_ptr(), std::ptr::null());
            if !h.is_null() {
                PostMessageW(h, WM_CLOSE, 0, 0);
            }
        }
        return;
    }

    // Giao thức trình bảo vệ màn hình Windows: "/s" chạy, "/p <hwnd>" xem trước, "/c" hoặc không tham số
    // (khi tên file là .scr) = cấu hình. `--saver` để chạy thử chế độ /s từ file .exe.
    let is_scr = std::env::current_exe().ok().and_then(|p| p.extension().map(|e| e.eq_ignore_ascii_case("scr"))).unwrap_or(false);
    let first = args.get(1).map(|a| a.to_ascii_lowercase()).unwrap_or_default();
    let saver = first.starts_with("/s") || first.starts_with("-s") || first == "--saver";
    if is_scr && !saver {
        if first.starts_with("/c") || first.starts_with("-c") || first.is_empty() {
            unsafe {
                MessageBoxW(
                    std::ptr::null_mut(),
                    wide("Bật / tắt trình bảo vệ màn hình tuanpc-desk trong menu khay của tuanpc-desk.\n\nKhi quay lại, Windows sẽ hiện màn hình đăng nhập thật (khóa bằng mật khẩu / PIN / Windows Hello của Windows).").as_ptr(),
                    wide("tuanpc-desk").as_ptr(),
                    MB_OK | MB_ICONINFORMATION,
                );
            }
        }
        return; // "/p": không vẽ khung xem trước nhỏ
    }

    unsafe {
        let name = wide(if saver { "TuanpcDesk_Saver" } else { "TuanpcDesk_Singleton" });
        CreateMutexW(std::ptr::null(), 1, name.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        timeBeginPeriod(1);
    }

    let log_name = if saver { "tuanpc-desk-saver.log" } else { "tuanpc-desk.log" };
    let log_dir = if is_scr { Some(lock::data_dir()) } else { std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) };
    let log_path = log_dir.map(|d| d.join(log_name)).unwrap_or_else(|| log_name.into());
    *LOG.lock().unwrap() = File::create(&log_path).ok();
    std::panic::set_hook(Box::new(|info| log!("PANIC: {info}")));


    // 30 FPS là nhịp mục tiêu của bản native. Cảnh vẫn giảm tải khi bị che,
    // còn lúc mở Settings/đang tương tác có thể tăng tạm thời để không giật.
    let fps: f64 = arg_value(&args, "--fps").and_then(|s| s.parse().ok()).unwrap_or(30.0);
    let interval = 1000.0 / fps.max(1.0);
    let mut cfg = load_config(arg_value(&args, "--theme"));
    if let Some(mode) = arg_value(&args, "--mode") {
        if matches!(mode.as_str(), "mirror" | "independent" | "panorama") {
            cfg.mode = mode;
        }
    }
    log!(
        "tuanpc-desk {} khởi động{}: chủ đề {}, {fps} khung/giây",
        env!("CARGO_PKG_VERSION"),
        if saver { " (trình bảo vệ màn hình)" } else { "" },
        cfg.theme
    );
    CURRENT_THEME.with(|c| c.set(theme_cmd(&cfg.theme)));
    let lock_sync = cfg.lock_sync;
    LOCK_SYNC.with(|c| c.set(lock_sync));
    INTERACTIONS_ENABLED.with(|c| c.set(cfg.interactions_enabled));
    WIDGET_ENABLED.with(|c| c.set(cfg.widget_enabled));
    WIDGET_LOCKED.with(|c| c.set(cfg.widget_locked));

    register_class(MSG_CLASS, Some(msg_proc), false);
    register_class(WALL_CLASS, Some(wall_proc), true);
    register_class(SAVER_CLASS, Some(saver_proc), true);
    let msg_hwnd = unsafe {
        let cls = wide(MSG_CLASS);
        let h = CreateWindowExW(0, cls.as_ptr(), cls.as_ptr(), 0, 0, 0, 0, 0, std::ptr::null_mut(), std::ptr::null_mut(), GetModuleHandleW(std::ptr::null()), std::ptr::null());
        let mut rid: RAWINPUTDEVICE = std::mem::zeroed();
        rid.usUsagePage = 0x01;
        rid.usUsage = 0x02;
        rid.dwFlags = RIDEV_INPUTSINK;
        rid.hwndTarget = h;
        RegisterRawInputDevices(&rid, 1, std::mem::size_of::<RAWINPUTDEVICE>() as u32);
        h
    };

    // Khay hệ thống (không có ở chế độ trình bảo vệ màn hình)
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = msg_hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = unsafe { app_icon(GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON)) };
    for (i, c) in "tuanpc-desk".encode_utf16().enumerate() {
        nid.szTip[i] = c;
    }
    let headless = args.iter().any(|a| a == "--headless");
    if !saver && !headless {
        settings::migrate_autostart();
        lock::log_saver_state("khởi động");
        if let Err(e) = lock::reconcile_disabled_saver() {
            log!("Chưa đồng bộ được trạng thái saver lúc khởi động: {e}; sẽ thử lại khi mở khóa");
        }
        unsafe {
            Shell_NotifyIconW(NIM_ADD, &nid);
            // Báo khi khóa / mở khóa phiên để ngừng vẽ lúc không ai thấy desktop
            windows_sys::Win32::System::RemoteDesktop::WTSRegisterSessionNotification(msg_hwnd, 0);
            // Phím tắt riêng (RegisterHotKey, không hook bàn phím; Win+L để nguyên cho Windows)
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
            register_configured_shortcuts(msg_hwnd, &cfg);
        }
        if lock::winl_disabled() {
            // Bản C# cũ để lại DisableLockWorkstation=1: Win+L của Windows đang bị tắt. Chỉ cảnh báo,
            // việc bật lại do người dùng bấm trong menu khay.
            log!("CẢNH BÁO: DisableLockWorkstation=1 - Win+L của Windows đang bị tắt (do bản C# cũ)");
            let mut b = nid;
            b.uFlags = NIF_INFO;
            b.dwInfoFlags = NIIF_WARNING;
            let title = "Win+L của Windows đang bị tắt";
            let text = "Bản C# cũ đã tắt khóa Windows. Chuột phải icon tuanpc-desk > \"Bật lại Win+L của Windows\".";
            for (i, c) in title.encode_utf16().take(63).enumerate() {
                b.szInfoTitle[i] = c;
            }
            for (i, c) in text.encode_utf16().take(255).enumerate() {
                b.szInfo[i] = c;
            }
            unsafe { Shell_NotifyIconW(NIM_MODIFY, &b) };
        }
    }

    let t_start = Instant::now();
    let interactions_enabled = cfg.interactions_enabled;
    let environment = cfg.environment.clone();
    let mut app = App {
        ctx: None,
        shared: None,
        vao: None,
        walls: Vec::new(),
        panorama: None,
        reader: desktop::IconReader::new(),
        cfg,
        environment,
        start: Instant::now(),
        frames: 0,
        frame_cpu_ms: 0.0,
        // GDI là đường mặc định để DWM luôn nhận đủ khung trên monitor phụ;
        // `--present gl` vẫn có thể dùng trên máy đã xác nhận driver ổn định.
        gdi: arg_value(&args, "--present").as_deref() != Some("gl"),
        dummy: None,
        saver,
        headless,
        interactions_enabled,
        boost_until: 0.0,
        live_weather: Arc::new(StdMutex::new(None)),
        weather_fetch_at: 0.0,
        environment_update_at: 0.0,
        widget: None,
        system_monitor: None,
        staging:RefCell::new(Vec::new()),
        benchmark_gdi:headless && args.iter().any(|a|a=="--benchmark-gdi"),
        benchmark_direct_readback:headless && args.iter().any(|a|a=="--benchmark-direct-readback"),
        prefer_gpu: !matches!(arg_value(&args,"--present").as_deref(),Some("gdi"|"gl")) && !args.iter().any(|a|a=="--benchmark-gdi"),
        gpu_device:None,
        gpu_attempted:false,
    };
    log!("Cách hiển thị: {}", if app.prefer_gpu {"GPU trực tiếp (GDI dự phòng)"} else if app.gdi { "GDI" } else { "OpenGL SwapBuffers" });
    // Khi chạy từ HKCU\...\Run, Windows có thể khởi động tiến trình trước khi
    // Explorer dựng xong Progman/WorkerW. Chờ và thử lại để tuanpc-desk tự chạy
    // ổn định sau đăng nhập thay vì thoát ngay ở lần gắn hình nền đầu tiên.
    let mut walls_ready = false;
    for attempt in 0..12 {
        match app.create_walls() {
            Ok(()) => {
                walls_ready = true;
                break;
            }
            Err(e) => {
                log!("Khởi động/gắn hình nền lần {} thất bại: {e}", attempt + 1);
                app.destroy_walls();
                app.drop_shared();
                app.gpu_device=None;app.gpu_attempted=false;
                app.ctx = None;
                app.vao = None;
                if attempt < 11 {
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
    }
    if !walls_ready {
        log!("Không thể gắn hình nền sau nhiều lần thử");
        unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
        return;
    }
    app.init_widget();
    app.configure_system_monitor(msg_hwnd);
    if headless && !args.iter().any(|a| a == "--real-icons") {
        let now = app.now();
        if let Some(ctx) = app.ctx.as_ref() {
            for wp in &mut app.walls {
                ctx.make_current(wp.gl_dc);
                let sample_icons = vec![
                    IconBox { x: 48.0, y: 48.0, w: 74.0, h: 86.0 },
                    IconBox { x: 48.0, y: 152.0, w: 74.0, h: 86.0 },
                    IconBox { x: 148.0, y: 48.0, w: 74.0, h: 86.0 },
                ];
                if let Some(theme)=wp.theme.as_mut() {
                    theme.set_icons(&ctx.gl, sample_icons, true, now);
                    theme.settle(&ctx.gl, app.shared.as_mut().unwrap(), now);
                }
            }
        }
    } else {
        app.poll_icons();
    }
    app.render();
    if headless && args.iter().any(|a|a=="--test-gpu-output") {
        let now=app.now();
        let ctx=app.ctx.as_ref().unwrap();let gl=&ctx.gl;let shared=app.shared.as_mut().unwrap();
        for wp in &mut app.walls {
            ctx.make_current(wp.gl_dc);
            let gpu=wp.gpu.as_mut().expect("GPU presentation required for this diagnostic; GDI fallback is not a pass");
            let reference=output_surface(gl,wp.buf_w,wp.buf_h).unwrap();
            App::post_process_texture(gl,shared,wp.fbo_tex,Some(reference.0),wp.buf_w,wp.buf_h,now,false);
            let mut expected=vec![0u8;(wp.buf_w*wp.buf_h*4) as usize];
            unsafe{
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER,Some(reference.0));
                gl.read_pixels(0,0,wp.buf_w,wp.buf_h,glow::BGRA,glow::UNSIGNED_BYTE,glow::PixelPackData::Slice(&mut expected));
            }
            let target=gpu.begin().unwrap();
            App::post_process_texture(gl,shared,wp.fbo_tex,Some(target),wp.buf_w,wp.buf_h,now,true);
            gpu.present(gl).unwrap();
            let actual=gpu.read_bgra().unwrap();let stride=wp.buf_w as usize*4;
            let mut error=0u64;let mut max_error=0u8;
            for(y,row)in expected.chunks_exact(stride).rev().enumerate(){
                for(a,b)in row.iter().zip(&actual[y*stride..(y+1)*stride]) {let d=a.abs_diff(*b);error+=d as u64;max_error=max_error.max(d);}
            }
            let mean=error as f64/actual.len() as f64;
            assert!(actual.chunks_exact(4).any(|p|p[..3]!=[0,0,0]),"GPU output stayed black");
            assert!(mean<0.05 && max_error<=8,"GPU output differs: mean={mean}, max={max_error}");
            unsafe{gl.delete_framebuffer(reference.0);gl.delete_texture(reference.1);gl.delete_buffer(reference.2);assert_eq!(gl.get_error(),glow::NO_ERROR);}
            log!("PASS: GPU output [{},{}] BGRA/chiều dọc đúng, sai số trung bình {mean:.5}, lớn nhất {max_error}",wp.screen.left,wp.screen.top);
        }
    }
    if app.benchmark_gdi && app.gdi && args.iter().any(|a|a=="--test-readback") {
        let ctx=app.ctx.as_ref().unwrap();
        for wp in &app.walls {
            ctx.make_current(wp.gl_dc);
            App::present_gdi(&ctx.gl,wp,&app.staging,true);
            let direct=wp.benchmark_target.as_ref().unwrap().pixels();
            App::present_gdi(&ctx.gl,wp,&app.staging,false);
            let staged=wp.benchmark_target.as_ref().unwrap().pixels();
            assert!(direct.chunks_exact(4).any(|p|p[..3]!=[0,0,0]),"GDI target stayed black");
            assert_eq!(direct,staged,"Staging changed the scaled output pixels");
            assert_eq!(unsafe{ctx.gl.get_error()},glow::NO_ERROR,"Readback GL error");
        }
        log!("PASS: ảnh GDI đầy đủ hai màn giống từng pixel giữa đọc trực tiếp và RAM đệm");
    }
    if headless && args.iter().any(|a|a=="--test-output-resume") {
        let now=app.now();
        for wp in &mut app.walls {wp.paused=true;wp.paused_since=Some(now-11_000.0);}
        app.release_hidden_outputs(now,false);
        assert!(app.walls.iter().all(|w|w.out.is_none()));
        let presents:Vec<_>=app.walls.iter().map(|w|w.gpu.as_ref().map_or(0,|g|g.presents)).collect();
        let frames=app.frames;app.render();assert_eq!(frames,app.frames);
        assert_eq!(presents,app.walls.iter().map(|w|w.gpu.as_ref().map_or(0,|g|g.presents)).collect::<Vec<_>>(),"Covered outputs submitted GPU presents");
        let index=app.walls.len().saturating_sub(1);app.walls[index].paused=false;
        app.render();assert!(app.walls[index].gpu.is_some() || app.walls[index].out.is_some() || !app.gdi);
        for(i,w)in app.walls.iter().enumerate(){if let Some(gpu)=&w.gpu{assert_eq!(gpu.presents,presents[i]+u64::from(i==index));}}
        for wp in &mut app.walls {wp.paused=false;}
        app.render();assert!(app.walls.iter().all(|w|w.gpu.is_some() || w.out.is_some() || !app.gdi));
        log!("PASS: dừng hoàn toàn, giải phóng bộ đệm, tiếp tục màn phụ rồi toàn bộ màn hình");
    }
    if open_settings && !saver {
        PENDING_CMD.with(|c| c.set(CMD_SETTINGS));
    }
    if open_launcher && !saver { PENDING_CMD.with(|c| c.set(CMD_LAUNCHER)); }
    log!("Khung hình đầu tiên sau {:.0} ms kể từ khi khởi động", t_start.elapsed().as_secs_f64() * 1000.0);
    if saver {
        unsafe {
            ShowCursor(0);
            if let Some(w) = app.walls.first() {
                SetForegroundWindow(w.hwnd);
            }
        }
        SAVER_START.with(|c| c.set(Some(Instant::now())));
    }
    // Đồng bộ ảnh màn khóa Windows vài giây sau khi cảnh đã ổn định
    // và hình nền tĩnh của Windows (luôn bật: tránh màn đen lúc mở máy trước khi app kịp chạy)
    let mut lock_sync_at = (!saver && !headless).then(|| app.now() + 6_000.0);
    let mut saver_check_at: Option<f64> = None;
    let mut rebuild_at: Option<f64> = None;
    let mut web_changed = false;
    let mut shortcut_capture_started: Option<Instant> = None;
    let mut last_icons_json = String::new();
    // Ngừng vẽ khi không ai thấy desktop: phiên khóa / màn Winlogon / trình bảo vệ màn hình đang chạy
    let mut hidden = false;

    let hook = if headless {
        std::ptr::null_mut()
    } else {
        unsafe {
            let progman = FindWindowW(wide("Progman").as_ptr(), std::ptr::null());
            let mut pid = 0u32;
            GetWindowThreadProcessId(progman, &mut pid);
            SetWinEventHook(0x8000, 0x8004, std::ptr::null_mut(), Some(win_event), pid, 0, 0)
        }
    };

    let mut next_frame = app.now() + interval;
    let mut next_icons = app.now() + 400.0;
    let mut next_fg = app.now();
    let mut next_stat = app.now() + 10_000.0;
    let mut precise_timer=true;
    // Vị trí widget thay đổi khi kéo cửa sổ, nên lưu trễ một nhịp để không
    // ghi settings.json liên tục trong lúc rê chuột nhưng vẫn giữ vị trí sau
    // khi Windows khởi động lại.
    let mut widget_save_at: Option<f64> = None;
    let snapshot_dir = arg_value(&args, "--snapshot");
    let exit_after_snap = args.iter().any(|a| a == "--exit-after-snapshot");
    // Integration check: reuse the same process/context across mode changes.
    let cycle_modes = headless && args.iter().any(|a| a == "--test-mode-cycle");
    let mode_steps = ["panorama", "independent", "panorama", "mirror", "panorama"];
    let mut mode_step = 0usize;
    let test_click = args.iter().any(|a| a == "--test-click");
    let mut test_click_at = test_click.then(|| app.now() + 900.0);
    let mut snapshot_at = snapshot_dir.as_ref().map(|_| app.now() + if test_click { 1_500.0 } else { 4_000.0 });
    let benchmark_until=if headless {arg_value(&args,"--run-seconds").and_then(|s|s.parse::<f64>().ok())
        .filter(|s|s.is_finite()).map(|s|app.now()+s.clamp(1.0,120.0)*1000.0)}else{None};
    'outer: loop {
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_QUIT {
                    break 'outer;
                }
                // Phím Tab / Enter / mũi tên trong cửa sổ cài đặt
                if let Some(sh) = settings::hwnd() {
                    if IsDialogMessageW(sh, &msg) != 0 {
                        continue;
                    }
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let cmd = PENDING_CMD.with(|c| c.replace(0));
        if benchmark_until.is_some_and(|t|app.now()>=t) {break 'outer;}
        let click = PENDING_CLICK.with(|c| c.take());
        if let Some((x, y)) = click {
            if !saver && !headless && INTERACTIONS_ENABLED.with(|c| c.get()) && app.handle_click(x, y) {
                next_frame = app.now();
            }
        }
        if test_click_at.is_some_and(|t| app.now() >= t) {
            test_click_at = None;
            app.simulate_test_clicks();
            next_frame = app.now();
        }
        let actions = settings::take_actions();
        let changed = cmd != 0 || !actions.is_empty();
        for a in actions {
            use settings::Action;
            match a {
                Action::Theme(t) => {
                    app.switch_theme(t);
                    CURRENT_THEME.with(|c| c.set(theme_cmd(t)));
                }
                Action::Param(k, v) => {
                    app.ensure_seed();
                    app.theme_params_mut().insert(k.into(), serde_json::json!(v));
                    log!("Thông số {}.{k} = {v}", app.cfg.theme);
                    app.rebuild_theme();
                }
                Action::Reseed => {
                    app.reseed();
                    log!("Đổi cảnh (seed mới)");
                    app.rebuild_theme();
                }
                Action::Reset => {
                    app.ensure_seed();
                    let m = app.theme_params_mut();
                    let seed = m.get("seed").cloned();
                    m.clear();
                    if let Some(sd) = seed {
                        m.insert("seed".into(), sd);
                    }
                    log!("Đặt lại thông số mặc định cho {}", app.cfg.theme);
                    app.rebuild_theme();
                }
                Action::Lock => lock::lock_now(),
                Action::SaverNow => PENDING_CMD.with(|c| c.set(CMD_SAVER_NOW)),
                Action::Saver(on) => {
                    if !on {
                        saver_check_at = None;
                        if let Err(e) = lock::uninstall_saver() {
                            log!("Tắt màn hình chờ lỗi: {e}");
                            unsafe { MessageBoxW(settings::hwnd().unwrap_or(msg_hwnd), wide(&e).as_ptr(), wide("tuanpc-desk").as_ptr(), MB_OK | MB_ICONWARNING) };
                        }
                    } else if let Err(e) = lock::install_saver(app.cfg.lock_idle_mins) {
                        log!("Đặt trình bảo vệ màn hình lỗi: {e}");
                        unsafe { MessageBoxW(settings::hwnd().unwrap_or(msg_hwnd), wide(&e).as_ptr(), wide("tuanpc-desk").as_ptr(), MB_OK | MB_ICONWARNING) };
                    }
                }
                Action::Idle(m) => {
                    app.cfg.lock_idle_mins = m;
                    if lock::saver_installed() {
                        if let Err(e) = lock::install_saver(m) {
                            log!("Đổi thời gian trình bảo vệ màn hình lỗi: {e}");
                        }
                    }
                }
                Action::Sync(on) => {
                    LOCK_SYNC.with(|c| c.set(on));
                    log!("Đồng bộ ảnh màn khóa Windows: {}", if on { "bật" } else { "tắt" });
                    if on {
                        app.sync_lock_image();
                    }
                }
                Action::Autostart(on) => settings::set_autostart(on),
                Action::Interaction(on) => {
                    INTERACTIONS_ENABLED.with(|c| c.set(on));
                    app.interactions_enabled = on;
                    app.cfg.interactions_enabled = on;
                    log!("Tương tác chuột: {}", if on { "bật" } else { "tắt" });
                }
            }
        }
        if cmd == CMD_SETTINGS {
            // Giống bản gốc: trang settings.html trong WebView2. Không được thì dùng cửa sổ Win32 dự phòng.
            let r = if force_win32 { Err("--settings-win32".to_string()) } else { web::find_prototype().ok_or_else(|| "không thấy thư mục prototype (settings.html)".to_string()).and_then(|p| web::open(&p)) };
            if let Err(e) = r {
                log!("Không mở được cài đặt WebView2 ({e}) -> dùng cửa sổ Win32");
                settings::open(&app.settings_state());
            }
        }
        // Thông điệp từ trang cài đặt WebView2
        if shortcut_capture_started.is_some_and(|started| started.elapsed() > Duration::from_secs(120) || !web::is_open()) {
            SHORTCUT_RECORDING.with(|c| c.set(false));
            register_configured_shortcuts(msg_hwnd, &app.cfg);
            shortcut_capture_started = None;
            web::post(&serde_json::json!({"t":"shortcutCaptureState","active":false}).to_string());
        }
        for raw in web::take_inbox() {
            let Ok(m) = serde_json::from_str::<serde_json::Value>(&raw) else { continue };
            let s_ = |k: &str| m.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
            match s_("t").as_str() {
                "hello" => {
                    web::post(&web_init(&app).to_string());
                    web::post(&away_state(&app, "").to_string());
                    last_icons_json.clear();
                }
                "set" => {
                    let (theme, key) = (s_("theme"), s_("key"));
                    let val = m.get("value").cloned().unwrap_or(serde_json::Value::Null);
                    if theme.is_empty() || key.is_empty() {
                        continue;
                    }
                    if !app.cfg.params.is_object() {
                        app.cfg.params = serde_json::json!({});
                    }
                    let root = app.cfg.params.as_object_mut().unwrap();
                    let e = root.entry(theme.clone()).or_insert_with(|| serde_json::json!({}));
                    if !e.is_object() {
                        *e = serde_json::json!({});
                    }
                    e.as_object_mut().unwrap().insert(key, val);
                    if theme == app.cfg.theme {
                        // Trang gửi liên tục khi kéo thanh trượt: gom lại, dựng cảnh sau khi ngừng kéo 250 ms
                        rebuild_at = Some(app.now() + 250.0);
                    }
                    web_changed = true;
                }
                "reset" => {
                    let theme = s_("theme");
                    if let Some(root) = app.cfg.params.as_object_mut() {
                        let seed = root.get(&theme).and_then(|p| p.get("seed")).cloned();
                        root.insert(theme.clone(), seed.map(|sd| serde_json::json!({ "seed": sd })).unwrap_or(serde_json::json!({})));
                    }
                    if theme == app.cfg.theme {
                        rebuild_at = Some(app.now());
                    }
                    log!("Đặt lại thông số mặc định cho {theme}");
                    web_changed = true;
                }
                "theme" => {
                    let id = s_("id");
                    if let Some(tid) = canonical_theme(&id) {
                        app.switch_theme(tid);
                        CURRENT_THEME.with(|c| c.set(theme_cmd(tid)));
                    } else {
                        log!("Chủ đề '{id}' chưa hỗ trợ");
                        web::post(&serde_json::json!({ "t": "themeChanged", "id": app.cfg.theme }).to_string());
                    }
                    web_changed = true;
                }
                "mode" => {
                    let mode = s_("v");
                    if matches!(mode.as_str(), "mirror" | "independent" | "panorama") {
                        match app.switch_mode(&mode) {
                            Ok(()) => {
                                log!("Chế độ đa màn hình: {mode}");
                                web::post(&serde_json::json!({ "t": "modeChanged", "mode": mode }).to_string());
                                web::post(&web_init(&app).to_string());
                                web_changed = true;
                            }
                            Err(e) => {
                                log!("Không đổi được chế độ đa màn hình: {e}");
                                web::post(&serde_json::json!({ "t": "modeError", "mode": app.cfg.mode, "error": e }).to_string());
                            }
                        }
                    }
                }
                "environment" => {
                    if let Some(v) = m.get("time").and_then(|v| v.as_bool()) { app.cfg.environment.time_enabled = v; }
                    if let Some(v) = m.get("weather").and_then(|v| v.as_bool()) { app.cfg.environment.weather_enabled = v; }
                    if let Some(v) = m.get("preset").and_then(|v| v.as_str()) { app.cfg.environment.preset = v.to_string(); }
                    if let Some(v) = m.get("source").and_then(|v| v.as_str()) { app.cfg.environment.source = v.to_string(); }
                    if m.get("location").is_some() { app.cfg.environment.location = m.get("location").cloned().filter(|v| !v.is_null()); }
                    app.environment = app.cfg.environment.clone();
                    app.weather_fetch_at = 0.0;
                    app.environment_update_at = 0.0;
                    if let Ok(mut live) = app.live_weather.lock() { *live = None; }
                    log!("Môi trường: time={}, weather={}, preset={}, source={}", app.environment.time_enabled, app.environment.weather_enabled, app.environment.preset, app.environment.source);
                    web::post(&web_init(&app).to_string());
                    web_changed = true;
                }
                "geocode" => {
                    let query = s_("query");
                    if query.trim().len() < 2 { web::post(&serde_json::json!({"t":"geocodeResults","results":[]}).to_string()); continue; }
                    let language = s_("language");
                    let url = format!("https://geocoding-api.open-meteo.com/v1/search?name={}&count=6&language={}&format=json", urlencoding::encode(query.trim()), if language == "vi" || language == "zh" { language } else { "en".to_string() });
                    match reqwest::blocking::get(url).and_then(|r| r.json::<serde_json::Value>().map_err(Into::into)) {
                        Ok(root) => {
                            let results = root.get("results").cloned().unwrap_or_else(|| serde_json::json!([]));
                            web::post(&serde_json::json!({"t":"geocodeResults","results":results}).to_string());
                        }
                        Err(e) => {
                            log!("Tìm vị trí thời tiết lỗi: {e}");
                            web::post(&serde_json::json!({"t":"geocodeResults","error":e.to_string()}).to_string());
                        }
                    }
                }
                "monitorTheme" => {
                    // Engine native dùng cùng cảnh trên toàn bộ màn hình; vẫn
                    // phản hồi để Settings giữ được trạng thái đa màn hình.
                    if let Some(id) = m.get("theme").and_then(|v| v.as_str()).and_then(canonical_theme) {
                        app.switch_theme(id);
                        web::post(&web_init(&app).to_string());
                        web_changed = true;
                    }
                }
                "lockNow" => lock::lock_now(),
                "saverNow" => PENDING_CMD.with(|c| c.set(CMD_SAVER_NOW)),
                "lockCfg" => {
                    let mut saver_status = String::new();
                    if let Some(on) = m.get("syncWin").and_then(|v| v.as_bool()) {
                        if on != LOCK_SYNC.with(|c| c.get()) {
                            LOCK_SYNC.with(|c| c.set(on));
                            log!("Đồng bộ ảnh màn khóa Windows: {}", if on { "bật" } else { "tắt" });
                            if on {
                                app.sync_lock_image();
                            }
                        }
                    }
                    if let Some(mins) = m.get("idleMins").and_then(|v| v.as_u64()).map(|v| v as u32) {
                        if mins == 0 {
                            saver_check_at = None;
                            if let Err(e) = lock::uninstall_saver() {
                                log!("Tắt màn hình chờ lỗi: {e}");
                                saver_status = format!("Không tắt được màn hình chờ: {e}");
                            }
                        } else if mins > 0 && (!lock::saver_active() || mins != app.cfg.lock_idle_mins) {
                            app.cfg.lock_idle_mins = mins;
                            if let Err(e) = lock::install_saver(mins) {
                                log!("Đặt trình bảo vệ màn hình lỗi: {e}");
                                unsafe { MessageBoxW(msg_hwnd, wide(&e).as_ptr(), wide("tuanpc-desk").as_ptr(), MB_OK | MB_ICONWARNING) };
                                web::post(&web_init(&app).to_string());
                            }
                        }
                    }
                    web::post(&away_state(&app, &saver_status).to_string());
                    web_changed = true;
                }
                "awayEnable" => {
                    let mins = m.get("minutes").and_then(|v| v.as_u64()).map(|v| v as u32).unwrap_or(app.cfg.lock_idle_mins).max(1);
                    app.cfg.lock_idle_mins = mins;
                    let status = match lock::install_saver(mins) {
                        Ok(()) => "Đã bật màn hình chờ tuanpc-desk".to_string(),
                        Err(e) => {
                            log!("Bật màn hình chờ lỗi: {e}");
                            format!("Không bật được màn hình chờ: {e}")
                        }
                    };
                    web::post(&away_state(&app, &status).to_string());
                    web_changed = true;
                }
                "awayDisable" => {
                    saver_check_at = None;
                    let status = match lock::uninstall_saver() {
                        Ok(()) => "Đã tắt màn hình chờ tuanpc-desk; Windows đã xác nhận trạng thái".to_string(),
                        Err(e) => { log!("Tắt màn hình chờ lỗi: {e}"); format!("Không tắt được màn hình chờ: {e}") }
                    };
                    web::post(&away_state(&app, &status).to_string());
                    web_changed = true;
                }
                "awayRefresh" => {
                    app.sync_static_wallpaper();
                    web::post(&away_state(&app, "Đã cập nhật cảnh lưu cho màn hình chờ").to_string());
                }
                "awayLockImage" => {
                    app.sync_lock_image();
                    web::post(&away_state(&app, "Đã cập nhật ảnh màn khóa Windows").to_string());
                }
                "awayPreview" => {
                    // Xem thử là tiến trình vẽ thông thường, không được cài
                    // saver tự động hoặc gọi SC_SCREENSAVE có khóa.
                    if let Err(e) = lock::preview_saver() {
                        web::post(&away_state(&app, &format!("Không xem thử được: {e}")).to_string());
                    }
                }
                "autostart" => {
                    settings::set_autostart(m.get("v").and_then(|v| v.as_bool()).unwrap_or(false));
                    web::post(&serde_json::json!({ "t": "autostart", "v": settings::autostart_enabled() }).to_string());
                }
                "settingsAppearance" => { web::set_appearance(m["value"].as_str().unwrap_or("system")); }
                "shortcutCapture" => {
                    let active = m["active"].as_bool().unwrap_or(false) && web::is_open();
                    if active {
                        unregister_shortcuts(msg_hwnd);
                        SHORTCUT_RECORDING.with(|c| c.set(true));
                        shortcut_capture_started = Some(Instant::now());
                        log!("Tạm ngưng phím tắt để ghi tổ hợp mới");
                    } else if SHORTCUT_RECORDING.with(|c| c.replace(false)) {
                        register_configured_shortcuts(msg_hwnd, &app.cfg);
                        shortcut_capture_started = None;
                    }
                    web::post(&serde_json::json!({"t":"shortcutCaptureState","active":active}).to_string());
                }
                "shortcut" => {
                    let kind = m["kind"].as_str().unwrap_or("");
                    let requested = (m["mods"].as_u64().unwrap_or(0) as u32,m["vk"].as_u64().unwrap_or(0) as u32);
                    let current = configured_shortcuts(&app.cfg);
                    let index = current.iter().position(|item| item.0 == kind);
                    let normalized = shortcuts::normalize(requested.0,requested.1);
                    let mut error = None;
                    if index.is_none() || normalized.is_none() {
                        error=Some("Tổ hợp không hợp lệ. Dùng Ctrl, Alt hoặc Shift cùng một phím; F12 dành riêng cho Windows.".to_string());
                    } else if let Some(other) = current.iter().find(|item| item.0 != kind && item.2 & 7 == requested.0 & 7 && item.3 == requested.1) {
                        error=Some(format!("Tổ hợp này đã dùng cho {}. Hãy chọn tổ hợp khác.",other.4));
                    } else {
                        let (_,id,old_mods,old_vk,_) = current[index.unwrap()];
                        if SHORTCUT_RECORDING.with(|c| c.replace(false)) { register_configured_shortcuts(msg_hwnd,&app.cfg); }
                        shortcut_capture_started=None;
                        match shortcuts::replace(msg_hwnd,id,(old_mods,old_vk),(normalized.unwrap(),requested.1)) {
                            Ok(()) => {
                                let mods=normalized.unwrap(); let vk=requested.1;
                                match kind {
                                    "launcher" => { app.cfg.launcher_mods=mods;app.cfg.launcher_vk=vk;LAUNCHER_HOTKEY_OK.with(|c|c.set(true)); },
                                    "saver" => {app.cfg.saver_mods=mods;app.cfg.saver_vk=vk;HOTKEY_OK.with(|c|c.set(true));},
                                    "capture" => {app.cfg.tools.capture_mods=mods;app.cfg.tools.capture_key=vk;},
                                    "record" => {app.cfg.tools.record_mods=mods;app.cfg.tools.record_key=vk;},
                                    _=>{},
                                }
                                if kind == "capture" || kind == "record" {
                                    let index=if kind == "capture" {0} else {1};
                                    if !app.cfg.tools.enabled { unsafe { UnregisterHotKey(msg_hwnd,id); } }
                                    TOOL_HOTKEYS.with(|c|{let mut flags=c.get();flags[index]=app.cfg.tools.enabled;c.set(flags);});
                                }
                                log!("Đã áp dụng phím tắt {kind}: {}",shortcuts::label(mods,vk));
                                web_changed=true;
                            }
                            Err(e) => {
                                error=Some(e);
                                unregister_shortcuts(msg_hwnd);
                                SHORTCUT_RECORDING.with(|c|c.set(true));
                                shortcut_capture_started=Some(Instant::now());
                            }
                        }
                    }
                    let ok=error.is_none();
                    web::post(&web_init(&app).to_string());
                    web::post(&serde_json::json!({"t":"hotkeyStatus","kind":kind,"ok":ok,"message":error.unwrap_or_else(||"Đã lưu và áp dụng phím tắt".into())}).to_string());
                }
                "launcherHotkey" => {
                    let requested_mods = m.get("mods").and_then(|v| v.as_u64()).map(|v| v as u32).unwrap_or(MOD_ALT | MOD_NOREPEAT);
                    let requested_vk = m.get("vk").and_then(|v| v.as_u64()).map(|v| v as u32).unwrap_or(VK_SPACE as u32);
                    let mods = normalize_hotkey(requested_mods, requested_vk).unwrap_or(MOD_ALT | MOD_NOREPEAT);
                    let vk = requested_vk;
                    let old_mods = app.cfg.launcher_mods;
                    let old_vk = app.cfg.launcher_vk;
                    unsafe { UnregisterHotKey(msg_hwnd, HOTKEY_LAUNCHER); }
                    let ok = unsafe { register_hotkey_logged(msg_hwnd, HOTKEY_LAUNCHER, mods, vk, "Launcher") };
                    if ok {
                        app.cfg.launcher_mods = mods;
                        app.cfg.launcher_vk = vk;
                    } else {
                        // Xung đột với ứng dụng khác không được làm mất phím đang chạy.
                        let restored = unsafe { register_hotkey_logged(msg_hwnd, HOTKEY_LAUNCHER, old_mods, old_vk, "Launcher (khôi phục)") };
                        log!("Launcher: giữ tổ hợp cũ sau khi đăng ký mới thất bại (khôi phục={restored})");
                    }
                    LAUNCHER_HOTKEY_OK.with(|c| c.set(ok));
                    web::post(&web_init(&app).to_string());
                    web::post(&serde_json::json!({"t":"hotkeyStatus","kind":"launcher","ok":ok,"message":if ok { "Đã lưu phím tắt" } else { "Phím tắt đang được ứng dụng khác sử dụng" }}).to_string());
                    web_changed = true;
                }
                "launcherAppearance" => {
                    app.cfg.launcher_appearance = launcher::Appearance::from_json(&m);
                    launcher::configure(app.cfg.launcher_appearance.clone());
                    web_changed = true;
                }
                "systemMonitor" => {
                    app.cfg.system_monitor = system_monitor::Options::from_json(&m);
                    app.configure_system_monitor(msg_hwnd);
                    web::post(&serde_json::json!({"t":"systemMonitorState","systemMonitor":app.cfg.system_monitor.json()}).to_string());
                    web_changed = true;
                }
                "miniTools" => {
                    let old_tools = app.cfg.tools.clone();
                    let requested_tools = mini_tools::Options::from_json(&m);
                    let statuses = mini_tools::register(msg_hwnd, &requested_tools);
                    let accepted = !requested_tools.enabled || statuses.iter().all(|v| *v);
                    let final_statuses;
                    if accepted {
                        app.cfg.tools = requested_tools;
                        final_statuses = statuses;
                    } else {
                        // Một tiện ích xung đột thì hoàn nguyên cả cặp để không làm mất
                        // tổ hợp đang hoạt động của tiện ích còn lại.
                        let restored = mini_tools::register(msg_hwnd, &old_tools);
                        final_statuses = restored;
                        log!("Phím tắt tiện ích bị xung đột, giữ cấu hình cũ (khôi phục={restored:?})");
                    }
                    TOOL_HOTKEYS.with(|c| c.set(final_statuses));
                    web::post(&serde_json::json!({"t":"miniToolsState","miniTools":app.cfg.tools.json(),"miniToolsHotkeys":TOOL_HOTKEYS.with(|c|c.get())}).to_string());
                    web::post(&serde_json::json!({"t":"hotkeyStatus","kind":"miniTools","ok":accepted,"message":if accepted { "Đã lưu phím tắt" } else { "Một phím tắt đang được ứng dụng khác sử dụng" }}).to_string());
                    web_changed=true;
                }
                "miniToolOpen" => {
                    match m.get("mode").and_then(|v|v.as_str()) {Some("capture")=>mini_tools::spawn("capture",&app.cfg.tools),Some("record")=>mini_tools::spawn("record",&app.cfg.tools),Some("preview")=>mini_tools::spawn("preview",&app.cfg.tools),_=>{}}
                }
                "interactions" => {
                    let on = m.get("v").and_then(|v| v.as_bool()).unwrap_or(true);
                    INTERACTIONS_ENABLED.with(|c| c.set(on));
                    app.interactions_enabled = on;
                    app.cfg.interactions_enabled = on;
                    log!("Tương tác chuột: {}", if on { "bật" } else { "tắt" });
                    web::post(&serde_json::json!({ "t": "interactions", "v": on }).to_string());
                    web_changed = true;
                }
                "widget" => {
                    if let Some(v) = m.get("enabled").and_then(|v| v.as_bool()) { app.set_widget_enabled(v); }
                    if let Some(v) = m.get("locked").and_then(|v| v.as_bool()) {
                        WIDGET_LOCKED.with(|c| c.set(v));
                        app.cfg.widget_locked = v;
                        if let Some(w) = app.widget.as_mut() { w.set_locked(v); }
                    }
                    if let Some(v) = m.get("scale").and_then(|v| v.as_f64()).map(|v| v as f32).filter(|v| v.is_finite()) {
                        app.cfg.widget_scale = v.clamp(0.5, 2.0);
                        if let Some(w) = app.widget.as_mut() { w.set_scale(app.cfg.widget_scale); }
                    }
                    if let Some(v) = m.get("opacity").and_then(|v| v.as_f64()).map(|v| v as f32).filter(|v| v.is_finite()) {
                        app.cfg.widget_opacity = v.clamp(0.05, 1.0);
                        if let Some(w) = app.widget.as_mut() { w.set_opacity(app.cfg.widget_opacity); }
                    }
                    if let Some(v) = m.get("x").and_then(|v| v.as_i64()) { app.cfg.widget_x = v as i32; }
                    if let Some(v) = m.get("y").and_then(|v| v.as_i64()) { app.cfg.widget_y = v as i32; }
                    for (i, key) in ["time", "weekday", "date"].iter().enumerate() {
                        if let Some(line) = m.get(key) {
                            if let Some(v) = line.get("x").and_then(|v| v.as_i64()) { app.cfg.widget_lines[i].x = v as i32; }
                            if let Some(v) = line.get("y").and_then(|v| v.as_i64()) { app.cfg.widget_lines[i].y = v as i32; }
                            if let Some(v) = line.get("opacity").and_then(|v| v.as_f64()) { app.cfg.widget_lines[i].opacity = (v as f32).clamp(0.0, 1.0); }
                            if let Some(v) = line.get("size").and_then(|v| v.as_f64()) { app.cfg.widget_lines[i].size = (v as f32).clamp(0.5, 2.0); }
                        }
                    }
                    if let Some(w) = app.widget.as_mut() { w.set_lines(app.cfg.widget_lines); }
                    if m.get("reset").and_then(|v| v.as_bool()).unwrap_or(false) {
                        app.cfg.widget_x = -1;
                        app.cfg.widget_y = -1;
                        if let Some(screen) = primary_wall(&app).map(|w| w.screen) {
                            if let Some(w) = app.widget.as_mut() { w.reset(screen); }
                        }
                    }
                    web::post(&web_init(&app).to_string());
                    web_changed = true;
                }
                "lang" => {
                    app.cfg.lang = s_("v");
                    let (eff, sys) = lang_eff(&app.cfg.lang);
                    web::post(&serde_json::json!({ "t": "lang", "v": app.cfg.lang, "eff": eff, "sys": sys }).to_string());
                    web_changed = true;
                }
                other => log!("Bỏ qua thông điệp cài đặt '{other}' (bản Rust chưa hỗ trợ)"),
            }
        }
        if rebuild_at.is_some_and(|t| app.now() >= t) {
            rebuild_at = None;
            app.ensure_seed();
            app.rebuild_theme();
        }
        if web_changed {
            web_changed = false;
            save_config(&app.cfg);
            settings::sync(&app.settings_state());
        }
        if let Some(tid) = cmd_theme(cmd) {
            app.switch_theme(tid);
            CURRENT_THEME.with(|c| c.set(cmd));
        } else {
            match cmd {
                CMD_EXIT => break,
                CMD_LAUNCHER => {
                    launcher::configure(app.cfg.launcher_appearance.clone());
                    launcher::toggle(msg_hwnd);
                }
                CMD_CAPTURE => mini_tools::spawn_capture_staged(&app.cfg.tools),
                CMD_RECORD => mini_tools::spawn("record",&app.cfg.tools),
                CMD_LOCK => lock::lock_now(),
                CMD_SAVER => {
                    saver_check_at = None;
                    if lock::saver_installed() {
                        if let Err(e) = lock::uninstall_saver() {
                            log!("Tắt màn hình chờ lỗi: {e}");
                            unsafe { MessageBoxW(msg_hwnd, wide(&e).as_ptr(), wide("tuanpc-desk").as_ptr(), MB_OK | MB_ICONWARNING) };
                        }
                    } else if let Err(e) = lock::install_saver(app.cfg.lock_idle_mins) {
                        log!("Đặt trình bảo vệ màn hình lỗi: {e}");
                        unsafe { MessageBoxW(msg_hwnd, wide(&e).as_ptr(), wide("tuanpc-desk").as_ptr(), MB_OK | MB_ICONWARNING) };
                    }
                }
                CMD_SYNC => {
                    let on = !LOCK_SYNC.with(|c| c.get());
                    LOCK_SYNC.with(|c| c.set(on));
                    log!("Đồng bộ ảnh màn khóa Windows: {}", if on { "bật" } else { "tắt" });
                    if on {
                        app.sync_lock_image();
                    }
                }
                CMD_WINL => {
                    lock::restore_winl();
                }
                CMD_INTERACTION => {
                    let on = !INTERACTIONS_ENABLED.with(|c| c.get());
                    INTERACTIONS_ENABLED.with(|c| c.set(on));
                    app.interactions_enabled = on;
                    app.cfg.interactions_enabled = on;
                    log!("Tương tác chuột: {}", if on { "bật" } else { "tắt" });
                }
                CMD_WIDGET => {
                    app.set_widget_enabled(!WIDGET_ENABLED.with(|c| c.get()));
                    log!("Đồng hồ native: {}", if app.cfg.widget_enabled { "bật" } else { "tắt" });
                }
                CMD_SYSTEM_MONITOR => {
                    app.cfg.system_monitor.enabled = !app.cfg.system_monitor.enabled;
                    app.configure_system_monitor(msg_hwnd);
                    web::post(&serde_json::json!({"t":"systemMonitorState","systemMonitor":app.cfg.system_monitor.json()}).to_string());
                }
                CMD_WIDGET_LOCK => {
                    let on = !WIDGET_LOCKED.with(|c| c.get());
                    WIDGET_LOCKED.with(|c| c.set(on));
                    app.cfg.widget_locked = on;
                    if let Some(w) = app.widget.as_mut() { w.set_locked(on); }
                    log!("Khóa vị trí đồng hồ: {}", if on { "bật" } else { "tắt" });
                }
                CMD_WIDGET_RESET => {
                    app.cfg.widget_x = -1;
                    app.cfg.widget_y = -1;
                    if let Some(screen) = primary_wall(&app).map(|w| w.screen) {
                        if let Some(w) = app.widget.as_mut() { w.reset(screen); }
                    }
                    log!("Đã đặt lại vị trí đồng hồ");
                }
                CMD_SAVER_NOW => {
                    let mut ready = lock::saver_active() && lock::saver_secure();
                    if !ready {
                        let q = "\"Khóa với cảnh động\" dùng trình bảo vệ màn hình của Windows, có bật \"khi quay lại, hiện màn hình đăng nhập\".\n\nĐặt tuanpc-desk làm trình bảo vệ màn hình (có khóa) ngay bây giờ?";
                        let r = unsafe { MessageBoxW(msg_hwnd, wide(q).as_ptr(), wide("tuanpc-desk").as_ptr(), MB_YESNO | MB_ICONQUESTION) };
                        if r == IDYES {
                            match lock::install_saver(app.cfg.lock_idle_mins) {
                                Ok(()) => ready = lock::saver_active() && lock::saver_secure(),
                                Err(e) => {
                                    log!("Đặt trình bảo vệ màn hình lỗi: {e}");
                                    unsafe { MessageBoxW(msg_hwnd, wide(&e).as_ptr(), wide("tuanpc-desk").as_ptr(), MB_OK | MB_ICONWARNING) };
                                }
                            }
                        }
                    }
                    if ready {
                        lock::start_secure_saver(msg_hwnd);
                        saver_check_at = Some(app.now() + 2_500.0);
                    }
                }
                _ => {}
            }
        }
        if changed && cmd != CMD_EXIT {
            save_config(&app.cfg);
            settings::sync(&app.settings_state());
            if web::is_open() && cmd != CMD_SETTINGS {
                // Đổi từ menu khay / cửa sổ Win32: đồng bộ sang trang cài đặt WebView2
                if cmd != 0 {
                    web::post(&serde_json::json!({ "t": "themeChanged", "id": app.cfg.theme }).to_string());
                }
                web::post(&serde_json::json!({ "t": "interactions", "v": app.interactions_enabled }).to_string());
            }
        }
        // Chỉ chẩn đoán lỗi khởi chạy saver. Không được tự gọi LockWorkStation:
        // người dùng có thể đã chọn Never hoặc đã tắt màn hình chờ trong tuanpc-desk.
        if saver_check_at.is_some_and(|t| app.now() >= t) {
            saver_check_at = None;
            if lock::saver_active() && lock::saver_secure() && !lock::saver_running() && !SESSION_LOCKED.with(|c| c.get()) {
                log!("Windows chưa bật trình bảo vệ màn hình sau 2,5 giây; bỏ qua, không tự khóa phiên");
            }
        }
        if matches!(PENDING_THEME_SYNC.with(|c| c.replace(false)), true) && !saver && !headless {
            lock_sync_at = Some(app.now() + 3_000.0);
        }
        if lock_sync_at.is_some_and(|t| app.now() >= t) {
            lock_sync_at = None;
            app.sync_static_wallpaper();
            if LOCK_SYNC.with(|c| c.get()) {
                app.sync_lock_image();
            }
        }
        if DISPLAY_CHANGED.with(|c| c.replace(false)) {
            log!("Cấu hình màn hình thay đổi, dựng lại cửa sổ hình nền");
            if let Some(mut w) = app.widget.take() { w.destroy(); }
            app.destroy_walls();
            if let Err(e) = app.create_walls() {
                log!("Dựng lại thất bại: {e}");
                break;
            }
            app.init_widget();
        }
        if REPAINT.with(|c| c.replace(false)) {
            app.repaint_all();
        }
        if ZORDER_DIRTY.with(|c| c.replace(false)) {
            app.fix_z_order();
        }
        let now = app.now();
        let scene_visible=app.walls.iter().any(|w|!w.paused);
        if now >= next_icons && !hidden && scene_visible {
            if !headless || args.iter().any(|a| a == "--real-icons") {
                app.fix_z_order();
                app.poll_icons();
            }
            if web::is_open() {
                let j = icons_json(&app);
                if j != last_icons_json {
                    web::post(&j);
                    last_icons_json = j;
                }
            }
            next_icons = now + 400.0;
        }
        if now >= next_fg {
            if !saver && !headless && snapshot_dir.is_none() {
                if SAVER_RECONCILE_PENDING.with(|c| c.replace(false)) {
                    if let Err(e) = lock::reconcile_disabled_saver() { log!("Đồng bộ saver sau mở khóa lỗi: {e}"); }
                    lock::log_saver_state("sau mở khóa");
                }
                app.poll_foreground();
                let locked = SESSION_LOCKED.with(|c| c.get());
                let saver_running = lock::saver_running();
                let default_desktop = lock::input_desktop_is_default();
                let h = locked || saver_running || !default_desktop;
                if h != hidden {
                    hidden = h;
                    log!("Desktop {}: sessionLocked={locked}, saverRunning={saver_running}, inputDefault={default_desktop}", if h { "ẩn -> ngừng vẽ" } else { "hiển thị -> vẽ tiếp" });
                    lock::log_saver_state("desktop đổi trạng thái");
                    if !h {
                        REPAINT.with(|c| c.set(true));
                    }
                }
            }
            next_fg = now + 500.0;
            app.release_hidden_outputs(now,hidden);
            if let Some(monitor) = app.system_monitor.as_mut() { monitor.tick(hidden); }
            if let Some(w) = app.widget.as_mut() {
                w.set_visible(!hidden && app.cfg.widget_enabled);
                if !hidden && app.cfg.widget_enabled {
                    w.tick();
                    let r=w.rect();
                    if app.cfg.widget_x!=r.left || app.cfg.widget_y!=r.top {
                        app.cfg.widget_x=r.left;app.cfg.widget_y=r.top;widget_save_at=Some(now+350.0);
                    }
                }
            }
        }
        let scene_visible=app.walls.iter().any(|w|!w.paused);
        let animate=!hidden && scene_visible;
        if animate!=precise_timer {
            unsafe {if animate {timeBeginPeriod(1);}else{timeEndPeriod(1);}}
            precise_timer=animate;
        }
        if hidden || !scene_visible {
            // Chỉ thức dậy theo nhịp kiểm tra 500 ms, không vẽ, không đọc biểu tượng
            next_frame = next_fg;
            next_icons = next_fg;
            for wp in &mut app.walls {wp.last_frame=0.0;}
        } else if now >= next_frame {
            app.render();
            let cur_interval = if !saver && app.any_interacting(now) {
                interval.min(20.0) // Adaptive FPS: 50 khung/giây mượt mà khi đang tương tác
            } else {
                interval
            };
            next_frame += cur_interval;
            if next_frame < now {
                next_frame = now + cur_interval;
            }
        }
        if snapshot_at.is_some_and(|t| now >= t) {
            let root = snapshot_dir.as_deref().unwrap();
            let dir = if cycle_modes { format!("{root}/{mode_step}-{}", app.cfg.mode) } else { root.to_string() };
            app.snapshot(&dir);
            snapshot_at = None;
            if cycle_modes && mode_step < mode_steps.len() {
                let mode = mode_steps[mode_step];
                if let Err(e) = app.switch_mode(mode) {
                    log!("Kiểm tra chuyển chế độ thất bại: {e}");
                    break;
                }
                app.poll_icons();
                log!("Kiểm tra chuyển chế độ -> {mode}: đã dựng lại cảnh");
                mode_step += 1;
                snapshot_at = Some(app.now() + 4_000.0);
            } else if exit_after_snap {
                break;
            }
        }
        if now >= next_stat {
            let (pm, ws) = private_mb();
            let n = app.frames.max(1) as f64;
            log!("RAM riêng {pm:.1} MB, working set {ws:.1} MB | {:.1} khung/giây, CPU vẽ {:.2} ms/khung", app.frames as f64 / 10.0, app.frame_cpu_ms / n);
            app.frames = 0;
            app.frame_cpu_ms = 0.0;
            next_stat = now + 10_000.0;
        }
        if widget_save_at.is_some_and(|t| now >= t) {
            save_config(&app.cfg);
            widget_save_at = None;
        }
        // Rounding down left a sub-millisecond busy loop before every frame.
        let wait = (next_frame.min(next_icons).min(next_fg) - app.now()).ceil().clamp(1.0,500.0) as u32;
        unsafe { MsgWaitForMultipleObjects(0, std::ptr::null(), 0, wait, QS_ALLINPUT) };
    }

    if precise_timer {unsafe {timeEndPeriod(1);}}
    log!("Đang thoát");
    if !headless {save_config(&app.cfg);}
    if !hook.is_null() {
        unsafe { UnhookWinEvent(hook) };
    }
    if let Some(mut w) = app.widget.take() { w.destroy(); }
    app.system_monitor = None;
    app.destroy_walls();
    app.drop_shared();
    app.gpu_device=None;
    app.ctx = None;
    unsafe {
        if let Some((h, dc)) = app.dummy.take() {
            ReleaseDC(h, dc);
            DestroyWindow(h);
        }
        if !saver && !headless {
            Shell_NotifyIconW(NIM_DELETE, &nid);
        }
        DestroyWindow(msg_hwnd);
    }
}

#[cfg(test)]
mod visibility_tests {
    use super::*;
    fn rect(v:[i32;4])->RECT {RECT{left:v[0],top:v[1],right:v[2],bottom:v[3]}}
    #[test]
    fn panorama_pauses_all_outputs_and_resumes_the_secondary_without_moving_the_scene() {
        let world=rect([-1080,-720,2560,1200]);
        let primary=rect([0,0,2560,1080]);let secondary=rect([-1080,-720,0,1200]);
        assert!(visible_bounds([(primary,true),(secondary,true)].into_iter()).is_none());
        let visible=visible_bounds([(primary,true),(secondary,false)].into_iter()).unwrap();
        assert_eq!(panorama_clip(visible,world,0.5,1820,960),[0,0,540,960]);
        let visible=visible_bounds([(primary,false),(secondary,true)].into_iter()).unwrap();
        assert_eq!(panorama_clip(visible,world,0.5,1820,960),[540,60,1280,540]);
        let visible=visible_bounds([(primary,false),(secondary,false)].into_iter()).unwrap();
        assert_eq!(panorama_clip(visible,world,0.5,1820,960),[0,0,1820,960]);
    }
    #[test]
    fn fractional_panorama_clip_keeps_edge_pixels_inside_the_framebuffer() {
        let clip=panorama_clip(rect([-1,0,101,99]),rect([-1,-1,101,101]),0.72,73,73);
        assert_eq!(clip,[0,1,73,72]);
    }
}
