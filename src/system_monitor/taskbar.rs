use super::Snapshot;
use crate::desktop::wide;
use std::ffi::c_void;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::*;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::*;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CLASS: &str = "TuanpcDeskSystemMonitor";
const WIDTH: i32 = 216;
const HEIGHT: i32 = 38;

#[derive(Clone, Debug)]
pub(super) struct Dock {
    taskbar: isize,
    bar: [i32; 4],
    monitor: [i32; 4],
    tray_left: i32,
    buttons_right: i32,
    dpi: u32,
    light: bool,
}
impl Dock {
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"handle":self.taskbar,"bar":self.bar,"monitor":self.monitor,"trayLeft":self.tray_left,
            "buttonsRight":self.buttons_right,"dpi":self.dpi,"light":self.light,"widgetRect":self.placement(self.bar)})
    }
    fn placement(&self, bar: [i32; 4]) -> Option<[i32; 4]> {
        let scale = self.dpi.max(96) as f32 / 96.0;
        let px = |value: i32| (value as f32 * scale).round() as i32;
        let height = bar[3].min(self.monitor[3]) - bar[1].max(self.monitor[1]);
        // Auto-hide animation or a vertical taskbar: don't leave a floating
        // overlay over application content when the taskbar is off screen.
        if height < px(28) || bar[2] - bar[0] <= height { return None; }
        let offset = bar[0] - self.bar[0];
        let right = self.tray_left + offset - px(6);
        let available = right - (self.buttons_right + offset + px(8)).max(bar[0]);
        let width = px(WIDTH).min(available);
        if width < px(180) { return None; }
        let h = px(HEIGHT).min(height - px(4));
        let top = bar[1].max(self.monitor[1]) + (height - h) / 2;
        Some([right - width, top, right, top + h])
    }
}

pub(super) struct Probe { automation: Option<IUIAutomation> }
impl Probe {
    pub fn new() -> Self { Self { automation: unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok() } } }
    pub fn read(&mut self) -> Option<Dock> {
        unsafe {
            let taskbar = FindWindowW(wide("Shell_TrayWnd").as_ptr(), std::ptr::null());
            if taskbar.is_null() { return None; }
            let mut bar: RECT = std::mem::zeroed();
            if GetWindowRect(taskbar, &mut bar) == 0 { return None; }
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(MonitorFromWindow(taskbar, MONITOR_DEFAULTTONEAREST), &mut info) == 0 { return None; }
            let mut tray_left = None;
            let native_tray = FindWindowExW(taskbar, std::ptr::null_mut(), wide("TrayNotifyWnd").as_ptr(), std::ptr::null());
            if !native_tray.is_null() {
                let mut tray: RECT = std::mem::zeroed();
                if GetWindowRect(native_tray, &mut tray) != 0 && tray.right > tray.left && tray.left > bar.left { tray_left = Some(tray.left); }
            }
            let mut buttons_right = bar.left;
            // Windows 11's WinUI tray may not have a separate native HWND.
            // UI Automation is read on this worker only, at most once per 5s.
            if let Some(automation) = self.automation.as_ref() {
                if let Ok(root) = automation.ElementFromHandle(windows::Win32::Foundation::HWND(taskbar)) {
                    if let Ok(condition) = automation.CreatePropertyCondition(UIA_ClassNamePropertyId, &VARIANT::from("SystemTray.SystemTrayFrame")) {
                        if let Ok(tray) = root.FindFirst(TreeScope_Descendants, &condition) {
                            if let Ok(rect) = tray.CurrentBoundingRectangle() {
                                if rect.right > rect.left && rect.left > bar.left { tray_left = Some(rect.left); }
                            }
                        }
                    }
                    if let Some(tray_left) = tray_left {
                        let buttons = automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_ButtonControlTypeId.0));
                        let items = automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_ListItemControlTypeId.0));
                        if let (Ok(buttons), Ok(items)) = (buttons, items) {
                            if let Ok(condition) = automation.CreateOrCondition(&buttons, &items) {
                                if let Ok(elements) = root.FindAll(TreeScope_Descendants, &condition) {
                                    for index in 0..elements.Length().unwrap_or(0).min(256) {
                                        if let Ok(element) = elements.GetElement(index) {
                                            if let Ok(rect) = element.CurrentBoundingRectangle() {
                                                if rect.right > rect.left && rect.top < bar.bottom && rect.bottom > bar.top && rect.right <= tray_left {
                                                    buttons_right = buttons_right.max(rect.right);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Some(Dock { taskbar: taskbar as isize, bar: array(bar), monitor: array(info.rcMonitor), tray_left: tray_left?, buttons_right,
                dpi: GetDpiForWindow(taskbar).max(96), light: light_taskbar() })
        }
    }
}

fn array(rect: RECT) -> [i32; 4] { [rect.left, rect.top, rect.right, rect.bottom] }
fn light_taskbar() -> bool {
    unsafe {
        let mut value = 0u32;
        let mut bytes = 4u32;
        RegGetValueW(HKEY_CURRENT_USER, wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize").as_ptr(),
            wide("SystemUsesLightTheme").as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), &mut value as *mut _ as _, &mut bytes);
        value != 0
    }
}
unsafe fn fullscreen(monitor: [i32; 4], taskbar: HWND) -> bool {
    let foreground = GetForegroundWindow();
    if foreground.is_null() || foreground == taskbar { return false; }
    let mut class = [0u16; 64];
    let count = GetClassNameW(foreground, class.as_mut_ptr(), 64);
    if matches!(String::from_utf16_lossy(&class[..count.max(0) as usize]).as_str(), "Progman" | "WorkerW") { return false; }
    let mut rect: RECT = std::mem::zeroed();
    GetWindowRect(foreground, &mut rect) != 0 && rect.left <= monitor[0] && rect.top <= monitor[1]
        && rect.right >= monitor[2] && rect.bottom >= monitor[3]
}

struct WindowState { owner: HWND }
unsafe extern "system" fn procedure(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState;
    match message {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
        WM_ERASEBKGND => 1,
        WM_PAINT => { ValidateRect(hwnd, std::ptr::null()); 0 }
        WM_LBUTTONDBLCLK => {
            ShellExecuteW(hwnd, wide("open").as_ptr(), wide("taskmgr.exe").as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL);
            0
        }
        WM_RBUTTONUP if !state.is_null() => {
            let owner = (*state).owner;
            let menu = CreatePopupMenu();
            AppendMenuW(menu, MF_STRING, 1, wide("Mở Trình quản lý tác vụ").as_ptr());
            AppendMenuW(menu, MF_STRING, 2, wide("Cài đặt tuanpc-desk…").as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(menu, MF_STRING, 3, wide("Ẩn thống kê trên taskbar").as_ptr());
            let mut point = POINT { x:0, y:0 };
            GetCursorPos(&mut point);
            SetForegroundWindow(hwnd);
            let result = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, point.x, point.y, 0, hwnd, std::ptr::null());
            DestroyMenu(menu);
            match result {
                1 => { ShellExecuteW(hwnd, wide("open").as_ptr(), wide("taskmgr.exe").as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL); }
                2 => { PostMessageW(owner, crate::WM_OPEN_SETTINGS, 0, 0); }
                3 => { PostMessageW(owner, WM_COMMAND, crate::CMD_SYSTEM_MONITOR, 0); }
                _ => {}
            }
            PostMessageW(hwnd, WM_NULL, 0, 0);
            0
        }
        _ => DefWindowProcW(hwnd, message, w, l),
    }
}

pub(super) struct Window {
    hwnd: HWND,
    _state: Box<WindowState>,
    visible: bool,
    last: String,
}
impl Window {
    pub fn new(owner: HWND) -> Result<Self, String> {
        unsafe {
            let mut class: WNDCLASSEXW = std::mem::zeroed();
            class.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
            class.lpfnWndProc = Some(procedure);
            class.style = CS_DBLCLKS;
            class.hInstance = GetModuleHandleW(std::ptr::null());
            class.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
            let name = wide(CLASS);
            class.lpszClassName = name.as_ptr();
            RegisterClassExW(&class);
            let hwnd = CreateWindowExW(WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, name.as_ptr(), name.as_ptr(), WS_POPUP,
                0, 0, WIDTH, HEIGHT, std::ptr::null_mut(), std::ptr::null_mut(), class.hInstance, std::ptr::null());
            if hwnd.is_null() { return Err("Không tạo được thống kê taskbar".into()); }
            let mut state = Box::new(WindowState { owner });
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&mut *state) as *mut WindowState as isize);
            Ok(Self { hwnd, _state: state, visible: false, last: String::new() })
        }
    }
    pub fn update(&mut self, snapshot: &Snapshot, dock: Option<&Dock>, suspended: bool) {
        unsafe {
            let placement = dock.filter(|dock| !suspended && IsWindowVisible(dock.taskbar as HWND) != 0 && !fullscreen(dock.monitor, dock.taskbar as HWND))
                .and_then(|dock| {
                    let mut bar: RECT = std::mem::zeroed();
                    if GetWindowRect(dock.taskbar as HWND, &mut bar) == 0 { None } else { dock.placement(array(bar)).map(|rect| (dock, rect)) }
                });
            let Some((dock, rect)) = placement else {
                if self.visible { ShowWindow(self.hwnd, SW_HIDE); self.visible = false; }
                self.last.clear();
                return;
            };
            let key = format!("{}:{rect:?}:{}", snapshot.sequence, dock.light);
            if key != self.last {
                if let Ok(bitmap) = render(snapshot, rect[2]-rect[0], rect[3]-rect[1], dock.dpi, dock.light) {
                    let destination = POINT { x:rect[0], y:rect[1] };
                    let size = SIZE { cx:bitmap.width, cy:bitmap.height };
                    let source = POINT { x:0, y:0 };
                    let blend = BLENDFUNCTION { BlendOp:AC_SRC_OVER as u8, BlendFlags:0, SourceConstantAlpha:255, AlphaFormat:AC_SRC_ALPHA as u8 };
                    if UpdateLayeredWindow(self.hwnd, std::ptr::null_mut(), &destination, &size, bitmap.dc, &source, 0, &blend, ULW_ALPHA) != 0 { self.last = key; }
                }
            }
            if !self.visible { ShowWindow(self.hwnd, SW_SHOWNOACTIVATE); self.visible = true; }
            let previous = GetWindow(dock.taskbar as HWND, GW_HWNDPREV);
            if previous != self.hwnd {
                // Above the taskbar, below menus/Start/other topmost windows;
                // never reorder or resize any Explorer-owned window.
                SetWindowPos(self.hwnd, if previous.is_null() { HWND_TOP } else { previous }, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER);
            }
        }
    }
}
impl Drop for Window { fn drop(&mut self) { unsafe { DestroyWindow(self.hwnd); } } }

fn percent(value: Option<f64>) -> String { value.filter(|n| n.is_finite()).map(|n| format!("{:.0}%", n.clamp(0.0,100.0))).unwrap_or_else(|| "--".into()) }
fn speed(value: Option<f64>) -> String {
    let Some(value) = value.filter(|n| n.is_finite() && *n >= 0.0) else { return "--".into(); };
    if value >= 1024.0*1024.0*1024.0 { format!("{:.1} GB/s",value/(1024.0*1024.0*1024.0)) }
    else if value >= 1024.0*1024.0 { format!("{:.1} MB/s",value/(1024.0*1024.0)) }
    else { format!("{:.0} KB/s",value/1024.0) }
}

struct Bitmap { dc:HDC, bitmap:HBITMAP, old:HGDIOBJ, bits:*mut u8, width:i32, height:i32 }
impl Drop for Bitmap { fn drop(&mut self) { unsafe { SelectObject(self.dc,self.old); DeleteObject(self.bitmap as _); DeleteDC(self.dc); } } }
fn render(snapshot:&Snapshot, width:i32, height:i32, dpi:u32, light:bool) -> Result<Bitmap,String> {
    unsafe {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        if dc.is_null() { return Err("Không tạo được DC".into()); }
        let mut info:BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize=std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth=width; info.bmiHeader.biHeight=-height;
        info.bmiHeader.biPlanes=1; info.bmiHeader.biBitCount=32;
        let mut bits:*mut c_void=std::ptr::null_mut();
        let bitmap=CreateDIBSection(dc,&info,DIB_RGB_COLORS,&mut bits,std::ptr::null_mut(),0);
        if bitmap.is_null() || bits.is_null() { DeleteDC(dc); return Err("Không tạo được bitmap".into()); }
        let old=SelectObject(dc,bitmap as _);
        let bitmap=Bitmap { dc,bitmap,old,bits:bits as _,width,height };
        let pixels=std::slice::from_raw_parts_mut(bitmap.bits,(width*height*4) as usize);
        pixels.fill(0);
        let scale=(dpi.max(96) as f32/96.0).min(width as f32/WIDTH as f32);
        let font=CreateFontW(-(11.5*scale).round() as i32,0,0,0,400,0,0,0,DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,CLIP_DEFAULT_PRECIS as u32,ANTIALIASED_QUALITY as u32,DEFAULT_PITCH as u32,wide("Segoe UI").as_ptr());
        let old_font=SelectObject(dc,font as _);
        SetBkMode(dc,TRANSPARENT as i32); SetTextColor(dc,0xffffff);
        let draw=|value:&str,left:i32,top:i32,right:i32,bottom:i32| {
            let mut rect=RECT {left,top,right,bottom};let mut text=wide(value);
            DrawTextW(dc,text.as_mut_ptr(),-1,&mut rect,DT_CENTER|DT_VCENTER|DT_SINGLELINE|DT_NOPREFIX);
        };
        let row=height/2;
        for (index,value) in [format!("CPU {}",percent(snapshot.cpu)),format!("RAM {}",percent(snapshot.ram)),format!("GPU {}",percent(snapshot.gpu))].iter().enumerate() {
            draw(value,index as i32*width/3,0,(index as i32+1)*width/3,row);
        }
        draw(&format!("↓ {}",speed(snapshot.down)),0,row,width/2,height);
        draw(&format!("↑ {}",speed(snapshot.up)),width/2,row,width,height);
        SelectObject(dc,old_font);DeleteObject(font as _);GdiFlush();
        let color=if light {[35u8,28,23]}else{[248u8,244,240]};
        for pixel in pixels.chunks_exact_mut(4) {
            let alpha=pixel[0].max(pixel[1]).max(pixel[2]);
            for channel in 0..3 { pixel[channel]=(color[channel] as u16*alpha as u16/255) as u8; }
            // Alpha 1 keeps the empty space clickable for the context menu
            // without adding a visible rectangle over the taskbar.
            pixel[3]=alpha.max(1);
        }
        Ok(bitmap)
    }
}

pub(super) fn save_preview(snapshot:&Snapshot,dock:&Dock,path:&std::path::Path)->Result<(),String> {
    let rect=dock.placement(dock.bar).ok_or("Taskbar không đủ chỗ")?;
    let bitmap=render(snapshot,rect[2]-rect[0],rect[3]-rect[1],dock.dpi,dock.light)?;
    let pixels=unsafe {std::slice::from_raw_parts(bitmap.bits,(bitmap.width*bitmap.height*4) as usize)};
    let background=if dock.light {239u16}else{28u16};
    let mut rgba=Vec::with_capacity(pixels.len());
    for pixel in pixels.chunks_exact(4) {
        let blend=background*(255-pixel[3] as u16)/255;
        rgba.extend_from_slice(&[(pixel[2] as u16+blend).min(255) as u8,(pixel[1] as u16+blend).min(255) as u8,(pixel[0] as u16+blend).min(255) as u8,255]);
    }
    image::RgbaImage::from_raw(bitmap.width as u32,bitmap.height as u32,rgba).ok_or("Bitmap không hợp lệ")?.save(path).map_err(|error|error.to_string())
}

pub(super) fn window_diagnostics()->serde_json::Value {
    unsafe {
        let hwnd=FindWindowW(wide(CLASS).as_ptr(),std::ptr::null());
        if hwnd.is_null(){return serde_json::Value::Null;}
        let mut rect:RECT=std::mem::zeroed();GetWindowRect(hwnd,&mut rect);
        serde_json::json!({"handle":hwnd as usize,"visible":IsWindowVisible(hwnd)!=0,"rect":array(rect)})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dock()->Dock {Dock{taskbar:0,bar:[0,1032,2560,1080],monitor:[0,0,2560,1080],tray_left:2300,buttons_right:1800,dpi:96,light:true}}
    #[test]
    fn placement_is_left_of_tray_and_never_over_icons() {
        let mut dock=dock();
        assert_eq!(dock.placement(dock.bar),Some([2078,1037,2294,1075]));
        dock.buttons_right=2200;
        assert!(dock.placement(dock.bar).is_none());
    }
    #[test]
    fn placement_handles_negative_monitors_dpi_and_auto_hide() {
        let mut dock=dock();
        dock.bar=[-2560,1032,0,1080];dock.monitor=[-2560,0,0,1080];dock.tray_left=-260;dock.buttons_right=-760;
        assert_eq!(dock.placement(dock.bar),Some([-482,1037,-266,1075]));
        assert!(dock.placement([-2560,1078,0,1126]).is_none());
        dock.dpi=144;dock.bar=[-2560,1008,0,1080];
        let rect=dock.placement(dock.bar).unwrap();assert_eq!(rect[2]-rect[0],324);
    }
    #[test]
    fn rates_and_unavailable_counters_have_readable_units() {
        assert_eq!(speed(Some(0.0)),"0 KB/s");assert_eq!(speed(Some(1048576.0)),"1.0 MB/s");
        assert_eq!(speed(None),"--");assert_eq!(percent(Some(500.0)),"100%");
    }
    #[test]
    fn native_text_has_alpha_in_both_taskbar_themes() {
        let snapshot=Snapshot{cpu:Some(100.0),ram:Some(99.0),gpu:Some(88.0),down:Some(1e8),up:Some(1e7),..Snapshot::default()};
        for light in [true,false] {
            let bitmap=render(&snapshot,216,38,96,light).unwrap();
            let pixels=unsafe {std::slice::from_raw_parts(bitmap.bits,(216*38*4) as usize)};
            assert!(pixels.chunks_exact(4).filter(|p|p[3]>16).count()>500);
            assert!(pixels.chunks_exact(4).all(|p|p[..3].iter().all(|color|*color<=p[3])));
        }
    }
}
