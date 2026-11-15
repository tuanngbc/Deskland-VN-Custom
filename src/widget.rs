//! Đồng hồ native kiểu La Compagnie des Ombres.
//!
//! Đây là widget trong suốt, click-through khi khóa và không cần Rainmeter.
//! Font hệ thống được dùng để tránh đóng gói lại font bên thứ ba từ skin .rmskin.

use crate::desktop::wide;
use std::mem::size_of;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CLASS: &str = "DesklandRsClockWidget";
const WIDTH: i32 = 420;
const HEIGHT: i32 = 250;

struct State {
    locked: bool,
    lang_vi: bool,
    scale: f32,
}

pub struct Widget {
    hwnd: HWND,
    state: Box<State>,
    rect: RECT,
    visible: bool,
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
    match msg {
        WM_NCHITTEST if !state.is_null() && (*state).locked => HTTRANSPARENT as LRESULT,
        WM_NCHITTEST => HTCAPTION as LRESULT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut r: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut r);
            FillRect(dc, &r, GetStockObject(BLACK_BRUSH as i32));
            if !state.is_null() {
                paint(dc, r, &*state);
            }
            EndPaint(hwnd, &ps);
            0
        }
        WM_LBUTTONDOWN if !state.is_null() && !(*state).locked => {
            ReleaseCapture();
            SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION as WPARAM, 0);
            0
        }
        WM_DESTROY => 0,
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

unsafe fn font(face: &str, height: i32, italic: bool, weight: i32) -> HFONT {
    let f = wide(face);
    CreateFontW(
        height, 0, 0, 0, weight, italic as i32, 0, 0, DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, DEFAULT_PITCH | FF_DONTCARE,
        f.as_ptr(),
    )
}

unsafe fn text(dc: HDC, rect: RECT, value: &str, face: &str, size: i32, color: COLORREF, italic: bool, weight: i32) {
    let h = font(face, size, italic, weight);
    let old = SelectObject(dc, h as _);
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, color);
    let mut s = wide(value);
    DrawTextW(dc, s.as_mut_ptr(), -1, &rect as *const RECT as *mut RECT, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    SelectObject(dc, old);
    DeleteObject(h as _);
}

unsafe fn paint(dc: HDC, r: RECT, state: &State) {
    let scale = state.scale.max(0.7).min(1.6);
    let hour = local_time();
    let time = format!("{:02}:{:02}", hour.0, hour.1);
    let (weekday, date) = if state.lang_vi {
        (weekday_vi(hour.6), format!("{:02} THÁNG {} {}", hour.3, hour.2, hour.5))
    } else {
        (weekday_en(hour.6), format!("{} {} {}", ordinal(hour.3), month_en(hour.2), hour.5))
    };
    let alpha = RGB(185, 231, 245);
    let bright = RGB(220, 247, 255);
    let h1 = (100.0 * scale) as i32;
    let h2 = (68.0 * scale) as i32;
    let h3 = (29.0 * scale) as i32;
    let center = (r.left + r.right) / 2;
    let box_rect = |top: i32, height: i32| RECT { left: center - r.right / 2, top, right: center + r.right / 2, bottom: top + height };
    text(dc, box_rect(8, h1 + 8), &time, "Segoe UI Light", h1, alpha, false, 300);
    text(dc, box_rect((108.0 * scale) as i32, h2 + 8), &weekday, "Segoe Script", h2, bright, true, 400);
    text(dc, box_rect((178.0 * scale) as i32, h3 + 8), &date.to_uppercase(), "Segoe UI Light", h3, alpha, false, 300);
}

// (hour, minute, second, day, month, year, weekday; weekday: 0=Sunday)
fn local_time() -> (u16, u16, u16, u16, u16, u16, u16) {
    #[repr(C)] struct T { year: u16, month: u16, dow: u16, day: u16, hour: u16, minute: u16, second: u16, ms: u16 }
    unsafe extern "system" { fn GetLocalTime(t: *mut T); }
    let mut t = T { year: 0, month: 1, dow: 0, day: 1, hour: 0, minute: 0, second: 0, ms: 0 };
    unsafe { GetLocalTime(&mut t); }
    (t.hour, t.minute, t.second, t.day, t.month, t.year, t.dow)
}

fn weekday_vi(n: u16) -> String { match n { 0 => "Chủ nhật", 1 => "Thứ hai", 2 => "Thứ ba", 3 => "Thứ tư", 4 => "Thứ năm", 5 => "Thứ sáu", _ => "Thứ bảy" }.into() }
fn weekday_en(n: u16) -> String { match n { 0 => "Sunday", 1 => "Monday", 2 => "Tuesday", 3 => "Wednesday", 4 => "Thursday", 5 => "Friday", _ => "Saturday" }.into() }
fn month_en(n: u16) -> &'static str { ["", "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"].get(n as usize).copied().unwrap_or("") }
fn ordinal(n: u16) -> String { let s = if (11..=13).contains(&(n % 100)) { "th" } else { match n % 10 { 1 => "st", 2 => "nd", 3 => "rd", _ => "th" } }; format!("{}{}", n, s) }

fn register() {
    unsafe {
        let cls = wide(CLASS);
        let mut wc: WNDCLASSEXW = std::mem::zeroed();
        wc.cbSize = size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(proc);
        wc.hInstance = GetModuleHandleW(std::ptr::null());
        wc.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_SIZEALL);
        wc.lpszClassName = cls.as_ptr();
        RegisterClassExW(&wc);
    }
}

impl Widget {
    pub fn new(screen: RECT, x: i32, y: i32, locked: bool, lang_vi: bool, scale: f32) -> Result<Self, String> {
        register();
        let mut state = Box::new(State { locked, lang_vi, scale });
        let ptr = (&mut *state) as *mut State;
        let w = (WIDTH as f32 * scale.max(0.7).min(1.6)) as i32;
        let h = (HEIGHT as f32 * scale.max(0.7).min(1.6)) as i32;
        let left = if x < 0 { screen.left + ((screen.right - screen.left) - w) / 2 } else { x };
        let top = if y < 0 { screen.top + 70 } else { y };
        let ex = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT;
        let cls = wide(CLASS);
        let hwnd = unsafe { CreateWindowExW(ex, cls.as_ptr(), cls.as_ptr(), WS_POPUP, left, top, w, h, std::ptr::null_mut(), std::ptr::null_mut(), GetModuleHandleW(std::ptr::null()), std::ptr::null_mut()) };
        if hwnd.is_null() { return Err("Không tạo được widget đồng hồ".into()); }
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
            SetLayeredWindowAttributes(hwnd, RGB(0, 0, 0), 0, LWA_COLORKEY);
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            UpdateWindow(hwnd);
        }
        Ok(Self { hwnd, state, rect: RECT { left, top, right: left + w, bottom: top + h }, visible: true })
    }

    pub fn tick(&mut self) {
        unsafe { InvalidateRect(self.hwnd, std::ptr::null(), 0); UpdateWindow(self.hwnd); GetWindowRect(self.hwnd, &mut self.rect); }
    }
    pub fn set_visible(&mut self, on: bool) { if self.visible != on { self.visible = on; unsafe { ShowWindow(self.hwnd, if on { SW_SHOWNOACTIVATE } else { SW_HIDE }); } } }
    pub fn set_locked(&mut self, locked: bool) { self.state.locked = locked; unsafe { let mut ex = GetWindowLongW(self.hwnd, GWL_EXSTYLE); if locked { ex |= WS_EX_TRANSPARENT as i32; } else { ex &= !(WS_EX_TRANSPARENT as i32); } SetWindowLongW(self.hwnd, GWL_EXSTYLE, ex); InvalidateRect(self.hwnd, std::ptr::null(), 1); } }
    pub fn locked(&self) -> bool { self.state.locked }
    pub fn rect(&self) -> RECT { self.rect }
    pub fn reset(&mut self, screen: RECT) { let w = self.rect.right - self.rect.left; let x = screen.left + ((screen.right - screen.left) - w) / 2; let y = screen.top + 70; unsafe { SetWindowPos(self.hwnd, HWND_TOP, x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW); } }
    pub fn destroy(&mut self) { unsafe { DestroyWindow(self.hwnd); } }
}
