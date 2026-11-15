//! Đồng hồ native kiểu La Compagnie des Ombres.
//!
//! Đây là widget trong suốt, click-through khi khóa và không cần Rainmeter.
//! Font hệ thống được dùng để tránh đóng gói lại font bên thứ ba từ skin .rmskin.

use crate::desktop::wide;
use std::mem::size_of;
use std::sync::Once;
use std::ffi::c_void;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CLASS: &str = "DesklandRsClockWidget";
const WIDTH: i32 = 320;
const HEIGHT: i32 = 230;
static LOAD_FONTS: Once = Once::new();

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
    opacity: u8,
    last_clock: String,
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
    match msg {
        WM_NCHITTEST if !state.is_null() && (*state).locked => HTTRANSPARENT as LRESULT,
        WM_NCHITTEST => HTCAPTION as LRESULT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        WM_PAINT => { ValidateRect(hwnd, std::ptr::null()); 0 }
        WM_LBUTTONDOWN if !state.is_null() && !(*state).locked => {
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
        height, 0, 0, 0, weight, italic as u32, 0, 0, DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, CLEARTYPE_QUALITY as u32, (DEFAULT_PITCH | FF_DONTCARE) as u32,
        f.as_ptr(),
    )
}

fn ensure_fonts() {
    LOAD_FONTS.call_once(|| unsafe {
        let Some(root) = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())) else { return };
        for name in ["Gravity-Light.ttf", "la Compagnie des Ombres.ttf", "steelfish rg.ttf", "steelfish rg it.ttf"] {
            let path = root.join("assets").join("widget").join(name);
            if path.is_file() {
                let p = wide(&path.to_string_lossy());
                AddFontResourceExW(p.as_ptr(), FR_PRIVATE, std::ptr::null_mut());
            }
        }
    });
}

unsafe fn text(dc: HDC, rect: RECT, value: &str, face: &str, size: i32, color: COLORREF, italic: bool, weight: i32) {
    let h = font(face, size, italic, weight);
    let old = SelectObject(dc, h as _);
    SetBkMode(dc, TRANSPARENT as i32);
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
    let alpha = rgb(185, 231, 245);
    let bright = rgb(220, 247, 255);
    let h1 = (100.0 * scale) as i32;
    let h2 = (85.0 * scale) as i32;
    let h3 = (35.0 * scale) as i32;
    let center = (r.left + r.right) / 2;
    let box_rect = |top: i32, height: i32| RECT { left: 0, top, right: r.right, bottom: top + height };
    text(dc, box_rect(0, h1 + 8), &time, "Steelfish Rg", h1, alpha, false, 400);
    text(dc, box_rect((105.0 * scale) as i32, h2 + 8), &weekday, "la Compagnie des Ombres", h2, bright, false, 400);
    text(dc, box_rect((165.0 * scale) as i32, h3 + 8), &date.to_uppercase(), "Steelfish Rg", h3, alpha, false, 400);
}

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF { r as u32 | ((g as u32) << 8) | ((b as u32) << 16) }

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
    pub fn new(screen: RECT, x: i32, y: i32, locked: bool, lang_vi: bool, scale: f32, opacity: f32) -> Result<Self, String> {
        ensure_fonts();
        register();
        let mut state = Box::new(State { locked, lang_vi, scale });
        let ptr = (&mut *state) as *mut State;
        let w = (WIDTH as f32 * scale.max(0.7).min(1.6)) as i32;
        let h = (HEIGHT as f32 * scale.max(0.7).min(1.6)) as i32;
        let left = if x < 0 { screen.left + ((screen.right - screen.left) - w) / 2 } else { x };
        let top = if y < 0 { screen.top + 70 } else { y };
        let ex = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | if locked { WS_EX_TRANSPARENT } else { 0 };
        let cls = wide(CLASS);
        let hwnd = unsafe { CreateWindowExW(ex, cls.as_ptr(), cls.as_ptr(), WS_POPUP, left, top, w, h, std::ptr::null_mut(), std::ptr::null_mut(), GetModuleHandleW(std::ptr::null()), std::ptr::null_mut()) };
        if hwnd.is_null() { return Err("Không tạo được widget đồng hồ".into()); }
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        let mut out = Self { hwnd, state, rect: RECT { left, top, right: left + w, bottom: top + h }, visible: true, opacity: (opacity.clamp(0.05, 1.0) * 255.0) as u8, last_clock: String::new() };
        out.redraw(true);
        Ok(out)
    }

    pub fn tick(&mut self) {
        unsafe { GetWindowRect(self.hwnd, &mut self.rect); }
        let t = local_time();
        let key = format!("{:02}:{:02}", t.0, t.1);
        if key != self.last_clock { self.redraw(false); }
    }
    pub fn set_visible(&mut self, on: bool) { if self.visible != on { self.visible = on; unsafe { ShowWindow(self.hwnd, if on { SW_SHOWNOACTIVATE } else { SW_HIDE }); } } }
    pub fn set_locked(&mut self, locked: bool) { self.state.locked = locked; unsafe { let mut ex = GetWindowLongW(self.hwnd, GWL_EXSTYLE); if locked { ex |= WS_EX_TRANSPARENT as i32; } else { ex &= !(WS_EX_TRANSPARENT as i32); } SetWindowLongW(self.hwnd, GWL_EXSTYLE, ex); } }
    pub fn set_scale(&mut self, scale: f32) { let scale = scale.clamp(0.5, 2.0); self.state.scale = scale; let w = (WIDTH as f32 * scale) as i32; let h = (HEIGHT as f32 * scale) as i32; unsafe { SetWindowPos(self.hwnd, HWND_TOP, self.rect.left, self.rect.top, w, h, SWP_NOACTIVATE | SWP_NOZORDER); GetWindowRect(self.hwnd, &mut self.rect); } self.redraw(true); }
    pub fn set_opacity(&mut self, opacity: f32) { self.opacity = (opacity.clamp(0.05, 1.0) * 255.0) as u8; self.redraw(true); }
    pub fn locked(&self) -> bool { self.state.locked }
    pub fn rect(&self) -> RECT { self.rect }
    pub fn reset(&mut self, screen: RECT) { let w = self.rect.right - self.rect.left; let x = screen.left + ((screen.right - screen.left) - w) / 2; let y = screen.top + 70; unsafe { SetWindowPos(self.hwnd, HWND_TOP, x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW); GetWindowRect(self.hwnd, &mut self.rect); } }
    pub fn destroy(&mut self) { unsafe { DestroyWindow(self.hwnd); } }

    fn redraw(&mut self, force: bool) {
        let t = local_time();
        let key = format!("{:02}:{:02}", t.0, t.1);
        if !force && key == self.last_clock { return; }
        self.last_clock = key;
        let w = (self.rect.right - self.rect.left).max(1);
        let h = (self.rect.bottom - self.rect.top).max(1);
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            let mem = CreateCompatibleDC(screen);
            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = w;
            bi.bmiHeader.biHeight = -h;
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            bi.bmiHeader.biCompression = BI_RGB;
            let mut bits: *mut c_void = std::ptr::null_mut();
            let bmp = CreateDIBSection(screen, &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            let old = SelectObject(mem, bmp as _);
            std::ptr::write_bytes(bits, 0, (w * h * 4) as usize);
            paint(mem, RECT { left: 0, top: 0, right: w, bottom: h }, &self.state);
            let px = std::slice::from_raw_parts_mut(bits as *mut u8, (w * h * 4) as usize);
            for p in px.chunks_exact_mut(4) {
                let a = p[0].max(p[1]).max(p[2]);
                p[3] = a;
            }
            let dst = POINT { x: self.rect.left, y: self.rect.top };
            let size = SIZE { cx: w, cy: h };
            let src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: self.opacity, AlphaFormat: AC_SRC_ALPHA as u8 };
            UpdateLayeredWindow(self.hwnd, screen, &dst, &size, mem, &src, 0, &blend, ULW_ALPHA);
            SelectObject(mem, old);
            DeleteObject(bmp as _);
            DeleteDC(mem);
            ReleaseDC(std::ptr::null_mut(), screen);
        }
    }
}
