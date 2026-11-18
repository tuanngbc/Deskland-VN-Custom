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

const CLASS: &str = "TuanpcDeskClockWidget";
const WIDTH: i32 = 320;
const HEIGHT: i32 = 230;
static LOAD_FONTS: Once = Once::new();

struct State {
    locked: bool,
    scale: f32,
    time: Line,
    weekday: Line,
    date: Line,
}

#[derive(Clone, Copy)]
pub struct Line {
    pub x: i32,
    pub y: i32,
    pub opacity: f32,
    pub size: f32,
}

pub struct Widget {
    hwnd: HWND,
    state: Box<State>,
    rect: RECT,
    // Vùng vẽ có thể tràn sang trái/trên mốc vị trí đã lưu. rect vẫn là
    // khung logic cũ để thêm khoảng đệm không làm đồng hồ đổi vị trí.
    surface: RECT,
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
        OUT_DEFAULT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, ANTIALIASED_QUALITY as u32, (DEFAULT_PITCH | FF_DONTCARE) as u32,
        f.as_ptr(),
    )
}

fn ensure_fonts() {
    LOAD_FONTS.call_once(|| unsafe {
        #[cfg(not(test))]
        let Some(root) = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())) else { return };
        #[cfg(test)]
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        for name in ["Gravity-Light.ttf", "la Compagnie des Ombres.ttf", "steelfish rg.ttf", "steelfish rg it.ttf"] {
            let path = root.join("assets").join("widget").join(name);
            if path.is_file() {
                let p = wide(&path.to_string_lossy());
                AddFontResourceExW(p.as_ptr(), FR_PRIVATE, std::ptr::null_mut());
            }
        }
    });
}

struct TextRun {
    value: String,
    face: &'static str,
    size: i32,
    color: COLORREF,
    opacity: f32,
    rect: RECT,
}

unsafe fn text(dc: HDC, run: &TextRun, origin: POINT) {
    let h = font(run.face, run.size, false, 400);
    let old = SelectObject(dc, h as _);
    SetBkMode(dc, TRANSPARENT as i32);
    let q = run.opacity.clamp(0.0, 1.0);
    let color = run.color;
    SetTextColor(dc, rgb(((color & 0xff) as f32 * q) as u8, (((color >> 8) & 0xff) as f32 * q) as u8, (((color >> 16) & 0xff) as f32 * q) as u8));
    let mut s = wide(&run.value);
    let mut rect = RECT { left: run.rect.left - origin.x, top: run.rect.top - origin.y, right: run.rect.right - origin.x, bottom: run.rect.bottom - origin.y };
    DrawTextW(dc, s.as_mut_ptr(), -1, &mut rect, DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOCLIP | DT_NOPREFIX);
    SelectObject(dc, old);
    DeleteObject(h as _);
}

unsafe fn layout(dc: HDC, state: &State, hour: (u16, u16, u16, u16, u16, u16, u16)) -> (Vec<TextRun>, RECT) {
    let scale = state.scale.clamp(0.7, 1.6);
    let time = format!("{:02}:{:02}", hour.0, hour.1);
    let weekday = weekday_en(hour.6);
    // local_time() trả về (giờ, phút, giây, ngày, tháng, năm, thứ).
    // Dùng chỉ số tháng (4), không dùng giây (2), để tên tháng không đổi mỗi giây.
    let date = format!("{} {} {}", ordinal(hour.3), month_en(hour.4), hour.5);
    let alpha = rgb(185, 231, 245);
    let bright = rgb(220, 247, 255);
    let h1 = (100.0 * scale * state.time.size.clamp(0.5, 2.0)) as i32;
    let h2 = (85.0 * scale * state.weekday.size.clamp(0.5, 2.0)) as i32;
    let h3 = (35.0 * scale * state.date.size.clamp(0.5, 2.0)) as i32;
    let mut bounds = RECT { left: 0, top: 0, right: (WIDTH as f32 * scale) as i32, bottom: (HEIGHT as f32 * scale) as i32 };
    let mut runs = Vec::with_capacity(3);
    // Giữ nguyên tâm, mốc dọc và thứ tự chồng chữ của skin đã duyệt.
    // Đo bằng đúng font đang vẽ, không giới hạn chữ trong khung 320px cũ.
    for (value, face, size, color, line, base_y) in [
        (time, "Steelfish Rg", h1, alpha, state.time, 0),
        (date.to_uppercase(), "Steelfish Rg", h3, alpha, state.date, 150),
        (weekday, "la Compagnie des Ombres", h2, bright, state.weekday, 70),
    ] {
        let h = font(face, size, false, 400);
        let old = SelectObject(dc, h as _);
        let s = wide(&value);
        let mut extent: SIZE = std::mem::zeroed();
        GetTextExtentPoint32W(dc, s.as_ptr(), (s.len() - 1) as i32, &mut extent);
        SelectObject(dc, old);
        DeleteObject(h as _);
        let half_width = (extent.cx.max(1) + 1) / 2;
        let top = (base_y as f32 * scale) as i32 + line.y;
        let rect = RECT { left: line.x - half_width, top, right: line.x + half_width, bottom: top + size + 8 };
        // Font viết tay có nét vươn ra ngoài advance width. Chừa khoảng đệm
        // cho nét đó và bỏ clipping của DrawText thay vì cắt mất đầu/cuối chữ.
        let pad = size.max(8);
        bounds.left = bounds.left.min(rect.left - pad);
        bounds.top = bounds.top.min(rect.top - pad);
        bounds.right = bounds.right.max(rect.right + pad);
        bounds.bottom = bounds.bottom.max(rect.bottom + pad);
        runs.push(TextRun { value, face, size, color, opacity: line.opacity, rect });
    }
    (runs, bounds)
}

unsafe fn paint(dc: HDC, runs: &[TextRun], origin: POINT) {
    for run in runs { text(dc, run, origin); }
}

unsafe extern "system" fn find_desktop_host(hwnd: HWND, out: LPARAM) -> BOOL {
    let defview = wide("SHELLDLL_DefView");
    if IsWindowVisible(hwnd) != 0 && !FindWindowExW(hwnd, std::ptr::null_mut(), defview.as_ptr(), std::ptr::null()).is_null() {
        *(out as *mut HWND) = hwnd;
        return 0;
    }
    1
}

/// Đặt ngay trên lớp desktop, bên dưới toàn bộ ứng dụng trên mọi màn hình.
/// Anchor vào foreground sẽ đưa đồng hồ lên trên những app không có focus.
unsafe fn order_above_desktop(hwnd: HWND, desktop: HWND) {
    let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
    if GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0 {
        SetWindowPos(hwnd, HWND_NOTOPMOST, 0, 0, 0, 0, flags);
    }
    if desktop.is_null() {
        // Explorer đang khởi động lại: ưu tiên không phủ lên ứng dụng.
        SetWindowPos(hwnd, HWND_BOTTOM, 0, 0, 0, 0, flags);
        return;
    }
    let previous = GetWindow(desktop, GW_HWNDPREV);
    if previous == hwnd { return; }
    let after = if previous.is_null() || GetWindowLongW(previous, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0 { HWND_NOTOPMOST } else { previous };
    SetWindowPos(hwnd, after, 0, 0, 0, 0, flags);
}

/// Chẩn đoán chỉ đọc để kiểm tra lớp cửa sổ trên máy thật, không mở settings
/// hoặc thay đổi cấu hình/focus. Chỉ chạy khi được gọi bằng tham số riêng.
pub fn write_diagnostics(path: &std::path::Path) -> std::io::Result<()> {
    unsafe extern "system" fn collect(hwnd: HWND, out: LPARAM) -> BOOL {
        let windows = &mut *(out as *mut Vec<serde_json::Value>);
        let mut class = [0u16; 128];
        let count = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
        let mut rect: RECT = std::mem::zeroed();
        GetWindowRect(hwnd, &mut rect);
        windows.push(serde_json::json!({
            "handle": hwnd as usize,
            "class": String::from_utf16_lossy(&class[..count.max(0) as usize]),
            "visible": IsWindowVisible(hwnd) != 0,
            "topmost": GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0,
            "rect": [rect.left, rect.top, rect.right, rect.bottom],
        }));
        1
    }
    let mut windows: Vec<serde_json::Value> = Vec::new();
    let mut desktop: HWND = std::ptr::null_mut();
    let foreground = unsafe {
        EnumWindows(Some(find_desktop_host), &mut desktop as *mut HWND as LPARAM);
        EnumWindows(Some(collect), &mut windows as *mut _ as LPARAM);
        GetForegroundWindow() as usize
    };
    let report = serde_json::json!({ "foreground": foreground, "desktop": desktop as usize, "windowsTopToBottom": windows });
    std::fs::write(path, serde_json::to_vec_pretty(&report)?)
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
    pub fn new(screen: RECT, x: i32, y: i32, locked: bool, scale: f32, lines: [Line; 3], opacity: f32) -> Result<Self, String> {
        ensure_fonts();
        register();
        let mut state = Box::new(State { locked, scale, time: lines[0], weekday: lines[1], date: lines[2] });
        let ptr = (&mut *state) as *mut State;
        let w = (WIDTH as f32 * scale.max(0.7).min(1.6)) as i32;
        let h = (HEIGHT as f32 * scale.max(0.7).min(1.6)) as i32;
        // Chỉ -1 là giá trị "chưa đặt". Toạ độ màn hình có thể âm khi màn hình
        // phụ nằm bên trái hoặc phía trên màn hình chính, vì vậy không được coi
        // mọi giá trị âm là vị trí mặc định.
        let left = if x == -1 { screen.left + ((screen.right - screen.left) - w) / 2 } else { x };
        let top = if y == -1 { screen.top + 70 } else { y };
        let ex = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | if locked { WS_EX_TRANSPARENT } else { 0 };
        let cls = wide(CLASS);
        let hwnd = unsafe { CreateWindowExW(ex, cls.as_ptr(), cls.as_ptr(), WS_POPUP, left, top, w, h, std::ptr::null_mut(), std::ptr::null_mut(), GetModuleHandleW(std::ptr::null()), std::ptr::null_mut()) };
        if hwnd.is_null() { return Err("Không tạo được widget đồng hồ".into()); }
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        let mut out = Self { hwnd, state, rect: RECT { left, top, right: left + w, bottom: top + h }, surface: RECT { left: 0, top: 0, right: w, bottom: h }, visible: true, opacity: (opacity.clamp(0.05, 1.0) * 255.0) as u8, last_clock: String::new() };
        out.redraw(true);
        Ok(out)
    }

    pub fn tick(&mut self) {
        self.read_position();
        let t = local_time();
        let key = format!("{:02}:{:02}", t.0, t.1);
        if key != self.last_clock { self.redraw(false); }
    }
    pub fn set_visible(&mut self, on: bool) { if self.visible != on { self.visible = on; unsafe { ShowWindow(self.hwnd, if on { SW_SHOWNOACTIVATE } else { SW_HIDE }); } if on { self.sync_z_order(); } } }
    pub fn set_locked(&mut self, locked: bool) { self.state.locked = locked; unsafe { let mut ex = GetWindowLongW(self.hwnd, GWL_EXSTYLE); if locked { ex |= WS_EX_TRANSPARENT as i32; } else { ex &= !(WS_EX_TRANSPARENT as i32); } SetWindowLongW(self.hwnd, GWL_EXSTYLE, ex); } }
    pub fn set_scale(&mut self, scale: f32) { self.read_position(); self.state.scale = scale.clamp(0.7, 1.6); self.redraw(true); }
    pub fn set_opacity(&mut self, opacity: f32) { self.opacity = (opacity.clamp(0.05, 1.0) * 255.0) as u8; self.redraw(true); }
    pub fn set_lines(&mut self, lines: [Line; 3]) { self.state.time = lines[0]; self.state.weekday = lines[1]; self.state.date = lines[2]; self.redraw(true); }
    pub fn locked(&self) -> bool { self.state.locked }
    pub fn rect(&self) -> RECT { self.rect }
    /// Widget vẫn là top-level để kéo giữa các màn hình, nhưng chỉ nằm ở lớp
    /// desktop. Chuyển focus trên màn chính không được nâng nó qua app màn phụ.
    pub fn sync_z_order(&self) {
        unsafe {
            let mut desktop: HWND = std::ptr::null_mut();
            EnumWindows(Some(find_desktop_host), &mut desktop as *mut HWND as LPARAM);
            order_above_desktop(self.hwnd, desktop);
        }
    }
    pub fn reset(&mut self, screen: RECT) { let w = self.rect.right - self.rect.left; let x = screen.left + ((screen.right - screen.left) - w) / 2; let y = screen.top + 70; unsafe { SetWindowPos(self.hwnd, std::ptr::null_mut(), x + self.surface.left, y + self.surface.top, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER); } self.read_position(); self.sync_z_order(); }
    pub fn destroy(&mut self) { unsafe { DestroyWindow(self.hwnd); } }

    fn read_position(&mut self) {
        let mut actual: RECT = unsafe { std::mem::zeroed() };
        if unsafe { GetWindowRect(self.hwnd, &mut actual) } != 0 {
            let left = actual.left - self.surface.left;
            let top = actual.top - self.surface.top;
            self.rect = RECT { left, top, right: left + (WIDTH as f32 * self.state.scale) as i32, bottom: top + (HEIGHT as f32 * self.state.scale) as i32 };
        }
    }

    fn redraw(&mut self, force: bool) {
        let t = local_time();
        let key = format!("{:02}:{:02}", t.0, t.1);
        if !force && key == self.last_clock { return; }
        self.last_clock = key;
        self.read_position();
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            let mem = CreateCompatibleDC(screen);
            let (runs, surface) = layout(mem, &self.state, t);
            let w = (surface.right - surface.left).max(1);
            let h = (surface.bottom - surface.top).max(1);
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
            paint(mem, &runs, POINT { x: surface.left, y: surface.top });
            GdiFlush();
            let px = std::slice::from_raw_parts_mut(bits as *mut u8, (w * h * 4) as usize);
            for p in px.chunks_exact_mut(4) {
                let a = p[0].max(p[1]).max(p[2]);
                p[3] = a;
            }
            let dst = POINT { x: self.rect.left + surface.left, y: self.rect.top + surface.top };
            let size = SIZE { cx: w, cy: h };
            let src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: self.opacity, AlphaFormat: AC_SRC_ALPHA as u8 };
            if UpdateLayeredWindow(self.hwnd, screen, &dst, &size, mem, &src, 0, &blend, ULW_ALPHA) != 0 {
                self.surface = surface;
            }
            SelectObject(mem, old);
            DeleteObject(bmp as _);
            DeleteDC(mem);
            ReleaseDC(std::ptr::null_mut(), screen);
        }
        self.sync_z_order();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Bitmap {
        dc: HDC,
        bitmap: HBITMAP,
        old: HGDIOBJ,
        bits: *mut u32,
        width: i32,
        height: i32,
    }

    impl Bitmap {
        unsafe fn new(width: i32, height: i32) -> Self {
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = width;
            info.bmiHeader.biHeight = -height;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            assert!(!dc.is_null() && !bitmap.is_null() && !bits.is_null());
            let old = SelectObject(dc, bitmap as _);
            std::ptr::write_bytes(bits, 0, (width * height * 4) as usize);
            Self { dc, bitmap, old, bits: bits as *mut u32, width, height }
        }

        unsafe fn ink_count(&self) -> usize {
            GdiFlush();
            std::slice::from_raw_parts(self.bits, (self.width * self.height) as usize).iter().filter(|p| **p & 0xffffff != 0).count()
        }
    }

    impl Drop for Bitmap {
        fn drop(&mut self) {
            unsafe { SelectObject(self.dc, self.old); DeleteObject(self.bitmap as _); DeleteDC(self.dc); }
        }
    }

    #[test]
    fn all_weekdays_and_sizes_render_without_clipping() {
        ensure_fonts();
        unsafe {
            let measure = Bitmap::new(1, 1);
            // Các giá trị hiện tại của người dùng, gồm x=125 khiến chữ dài
            // tràn qua mép trái khung cố định cũ; thêm cả x=0 và offset âm.
            for scale in [0.7, 1.4, 1.6] {
                for size in [0.5, 1.0, 2.0] {
                    for x in [-120, 0, 125] {
                        let state = State {
                            locked: true, scale,
                            time: Line { x: x + 7, y: -8, size: 1.25, opacity: 0.45 },
                            weekday: Line { x, y: 0, size, opacity: 1.0 },
                            date: Line { x, y: -31, size: 1.2, opacity: 0.5 },
                        };
                        for weekday in 0..7 {
                            let (runs, bounds) = layout(measure.dc, &state, (23, 58, 0, 28, 9, 2026, weekday));
                            let width = bounds.right - bounds.left;
                            let height = bounds.bottom - bounds.top;
                            let actual = Bitmap::new(width, height);
                            paint(actual.dc, &runs, POINT { x: bounds.left, y: bounds.top });
                            // Vẽ lại cùng chữ trên canvas lớn hơn mỗi cạnh
                            // 256px. Nếu còn bị cắt, ảnh tham chiếu có thêm nét.
                            let reference = Bitmap::new(width + 512, height + 512);
                            paint(reference.dc, &runs, POINT { x: bounds.left - 256, y: bounds.top - 256 });
                            assert!(actual.ink_count() > 0);
                            assert_eq!(actual.ink_count(), reference.ink_count(), "clipped {} scale={scale} size={size} x={x}", weekday_en(weekday));
                        }
                    }
                }
            }
        }
    }

    struct TestWindow(HWND);
    impl TestWindow {
        unsafe fn new(x: i32, y: i32) -> Self {
            let class = wide("STATIC");
            let hwnd = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, class.as_ptr(), std::ptr::null(), WS_POPUP, x, y, 80, 60, std::ptr::null_mut(), std::ptr::null_mut(), GetModuleHandleW(std::ptr::null()), std::ptr::null());
            assert!(!hwnd.is_null());
            Self(hwnd)
        }
    }
    impl Drop for TestWindow {
        fn drop(&mut self) { unsafe { DestroyWindow(self.0); } }
    }

    unsafe fn above(first: HWND, second: HWND) -> bool {
        let mut window = GetWindow(first, GW_HWNDNEXT);
        // Bound the scan even if another process rearranges windows mid-test.
        for _ in 0..4096 {
            if window.is_null() { return false; }
            if window == second { return true; }
            window = GetWindow(window, GW_HWNDNEXT);
        }
        false
    }

    #[test]
    #[ignore = "Isolated Windows z-order test; runs explicitly in CI"]
    fn windows_clock_stays_below_apps_across_monitors() {
        unsafe {
            // Cửa sổ ẩn ngoài màn hình: kiểm tra z-order thật của Windows mà
            // không chiếm focus hoặc thay đổi vị trí cửa sổ người dùng.
            let desktop = TestWindow::new(-30000, -30000);
            let primary = TestWindow::new(100, 100);
            let secondary = TestWindow::new(-900, -600);
            let clock = TestWindow::new(-687, -367);
            let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
            SetWindowPos(desktop.0, HWND_BOTTOM, 0, 0, 0, 0, flags);
            SetWindowPos(secondary.0, HWND_TOP, 0, 0, 0, 0, flags);
            SetWindowPos(primary.0, HWND_TOP, 0, 0, 0, 0, flags);
            for app in [primary.0, secondary.0, primary.0] {
                // Mô phỏng chuyển app: app được nâng lên, clock đã ở vị trí
                // sai ngay phía sau app đó như bản lỗi cũ.
                SetWindowPos(app, HWND_TOP, 0, 0, 0, 0, flags);
                SetWindowPos(clock.0, app, 0, 0, 0, 0, flags);
                order_above_desktop(clock.0, desktop.0);
                assert!(above(primary.0, clock.0));
                assert!(above(secondary.0, clock.0));
                assert!(above(clock.0, desktop.0));
                assert_eq!(GetWindow(desktop.0, GW_HWNDPREV), clock.0);
                let mut rect: RECT = std::mem::zeroed();
                GetWindowRect(clock.0, &mut rect);
                assert_eq!((rect.left, rect.top), (-687, -367));
            }
            // Cả khi Explorer đưa desktop lên hoặc clock lỡ nhận topmost,
            // lần đồng bộ tiếp theo không biến clock thành always-on-top.
            SetWindowPos(clock.0, HWND_TOPMOST, 0, 0, 0, 0, flags);
            order_above_desktop(clock.0, desktop.0);
            assert_eq!(GetWindowLongW(clock.0, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST, 0);
            assert!(above(primary.0, clock.0) && above(secondary.0, clock.0));
        }
    }

    #[test]
    #[ignore = "Isolated native layered-window test; runs explicitly in CI"]
    fn windows_clock_padding_preserves_position_when_resized_and_dragged() {
        let line = Line { x: 125, y: 0, size: 1.0, opacity: 1.0 };
        let screen = RECT { left: -1080, top: -731, right: 0, bottom: 1189 };
        let mut clock = Widget::new(screen, -687, -367, true, 1.4, [line; 3], 1.0).unwrap();
        for size in [0.5, 2.0, 1.0] {
            clock.set_lines([Line { size, ..line }; 3]);
            clock.set_scale(size.clamp(0.7, 1.6));
            clock.tick();
            assert_eq!((clock.rect().left, clock.rect().top), (-687, -367));
        }
        unsafe {
            SetWindowPos(clock.hwnd, std::ptr::null_mut(), -800 + clock.surface.left, -500 + clock.surface.top, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER);
        }
        clock.tick();
        assert_eq!((clock.rect().left, clock.rect().top), (-800, -500));
        clock.set_opacity(0.5);
        assert_eq!((clock.rect().left, clock.rect().top), (-800, -500));
        clock.destroy();
    }
}
