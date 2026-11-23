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
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetWindowDpiAwarenessContext, SetThreadDpiAwarenessContext};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CLASS: &str = "TuanpcDeskSystemMonitor";
const WIDTH: i32 = 176;
const HEIGHT: i32 = 28;

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
        if width < px(WIDTH) { return None; }
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
                                if rect.right > rect.left && rect.left > bar.left && rect.right <= bar.right
                                    && rect.top < bar.bottom && rect.bottom > bar.top {
                                    tray_left = Some(tray_left.map_or(rect.left, |native| native.min(rect.left)));
                                }
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
                                                    let class=element.CurrentClassName().map(|s|s.to_string()).unwrap_or_default();
                                                    let id=element.CurrentAutomationId().map(|s|s.to_string()).unwrap_or_default();
                                                    if class.contains("SystemTray") || id.contains("Overflow") || id.contains("SystemTray") {continue;}
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
    if GetWindowLongW(foreground,GWL_EXSTYLE) as u32 & (WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE) != 0 {return false;}
    let mut class = [0u16; 64];
    let count = GetClassNameW(foreground, class.as_mut_ptr(), 64);
    if shell_popup(&String::from_utf16_lossy(&class[..count.max(0) as usize])) { return false; }
    let mut rect: RECT = std::mem::zeroed();
    GetWindowRect(foreground, &mut rect) != 0 && rect.left <= monitor[0] && rect.top <= monitor[1]
        && rect.right >= monitor[2] && rect.bottom >= monitor[3]
}

fn shell_popup(class:&str)->bool {
    matches!(class,"Progman"|"WorkerW"|"Shell_TrayWnd"|"Shell_SecondaryTrayWnd"|"NotifyIconOverflowWindow"
        |"TopLevelWindowForOverflowXamlIsland"|"XamlExplorerHostIslandWindow"|"ControlCenterWindow")
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
            AppendMenuW(menu, MF_STRING, 2, wide("Cài đặt TLauncher…").as_ptr());
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
    last: String,
    bitmap: Option<Bitmap>,
    rect: Option<[i32;4]>,
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
            if RegisterClassExW(&class)==0 && GetLastError()!=ERROR_CLASS_ALREADY_EXISTS {
                return Err("Không đăng ký được thống kê taskbar".into());
            }
            // Create the actual child only once Explorer has a valid taskbar.
            // A top-level owned popup participates in Explorer's flyout Z order
            // and can be hidden/covered when the overflow tray opens or closes.
            Ok(Self { hwnd:std::ptr::null_mut(), _state:Box::new(WindowState { owner }),
                last:String::new(),bitmap:None,rect:None })
        }
    }
    unsafe fn attach(&mut self, taskbar: HWND) -> bool {
        if IsWindow(self.hwnd)!=0 && GetParent(self.hwnd)==taskbar { return true; }
        if IsWindow(self.hwnd)!=0 { DestroyWindow(self.hwnd); }
        self.hwnd=std::ptr::null_mut();self.last.clear();self.rect=None;
        // Create as a child directly, instead of reparenting a layered popup.
        // Matching Explorer's DPI context avoids resetting our process awareness.
        let previous=SetThreadDpiAwarenessContext(GetWindowDpiAwarenessContext(taskbar));
        self.hwnd=CreateWindowExW(WS_EX_LAYERED|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,
            wide(CLASS).as_ptr(),wide(CLASS).as_ptr(),WS_CHILD|WS_CLIPSIBLINGS,
            0,0,WIDTH,HEIGHT,taskbar,std::ptr::null_mut(),GetModuleHandleW(std::ptr::null()),std::ptr::null());
        if !previous.is_null() { SetThreadDpiAwarenessContext(previous); }
        if self.hwnd.is_null() { return false; }
        SetWindowLongPtrW(self.hwnd,GWLP_USERDATA,(&mut *self._state) as *mut WindowState as isize);
        true
    }
    pub fn update(&mut self, snapshot: &Snapshot, dock: Option<&Dock>, suspended: bool) {
        unsafe {
            let placement = dock.filter(|dock| !suspended && IsWindow(dock.taskbar as HWND)!=0
                    && IsWindowVisible(dock.taskbar as HWND) != 0 && !fullscreen(dock.monitor, dock.taskbar as HWND))
                .and_then(|dock| {
                    let mut bar: RECT = std::mem::zeroed();
                    if GetWindowRect(dock.taskbar as HWND, &mut bar) == 0 { None } else { dock.placement(array(bar)).map(|rect| (dock, rect)) }
                });
            let Some((dock, rect)) = placement else {
                if IsWindow(self.hwnd)!=0 && GetWindowLongW(self.hwnd,GWL_STYLE) as u32 & WS_VISIBLE != 0 {
                    ShowWindow(self.hwnd,SW_HIDE);
                }
                return;
            };
            if !self.attach(dock.taskbar as HWND) { return; }
            // Inspect the HWND, not a cached bool: shell changes, display changes
            // and a restored taskbar can invalidate our previous visibility.
            let visible=IsWindowVisible(self.hwnd)!=0;
            let mut actual:RECT=std::mem::zeroed();
            let moved=GetWindowRect(self.hwnd,&mut actual)==0 || array(actual)!=rect;
            let key = visual_key(snapshot,rect[2]-rect[0],rect[3]-rect[1],dock.dpi,dock.light);
            let mut changed=false;
            if key != self.last {
                if let Ok(bitmap) = render(snapshot, rect[2]-rect[0], rect[3]-rect[1], dock.dpi, dock.light) {
                    self.bitmap=Some(bitmap);changed=true;
                }
            }
            if changed || self.rect!=Some(rect) || moved || !visible {
                if let Some(bitmap)=self.bitmap.as_ref() {
                    // A layered child is positioned in its parent's client
                    // coordinates; keep Dock/diagnostics in physical screen space.
                    let mut destination = POINT { x:rect[0], y:rect[1] };
                    ScreenToClient(dock.taskbar as HWND,&mut destination);
                    let size = SIZE { cx:bitmap.width, cy:bitmap.height };
                    let source = POINT { x:0, y:0 };
                    let blend = BLENDFUNCTION { BlendOp:AC_SRC_OVER as u8, BlendFlags:0, SourceConstantAlpha:255, AlphaFormat:AC_SRC_ALPHA as u8 };
                    if UpdateLayeredWindow(self.hwnd, std::ptr::null_mut(), &destination, &size, bitmap.dc, &source, 0, &blend, ULW_ALPHA) != 0 {
                        self.last = key;self.rect=Some(rect);
                    }
                }
            }
            if self.rect==Some(rect) {
                // Only repair sibling order when Explorer really changed it.
                // This cannot lift the panel over taskbar flyouts or other apps.
                if !GetWindow(self.hwnd,GW_HWNDPREV).is_null() {
                    SetWindowPos(self.hwnd,HWND_TOP,0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE|SWP_NOOWNERZORDER);
                }
                if !visible { ShowWindow(self.hwnd,SW_SHOWNOACTIVATE); }
            }
        }
    }
}
impl Drop for Window { fn drop(&mut self) { unsafe { if IsWindow(self.hwnd)!=0 { DestroyWindow(self.hwnd); } } } }

fn percent(value: Option<f64>) -> String { value.filter(|n| n.is_finite()).map(|n| format!("{:.0}%", n.clamp(0.0,100.0))).unwrap_or_else(|| "--".into()) }
fn visual_key(snapshot:&Snapshot,width:i32,height:i32,dpi:u32,light:bool)->String {
    format!("{width}:{height}:{dpi}:{light}:{}:{}:{}:{}:{}",percent(snapshot.cpu),percent(snapshot.ram),percent(snapshot.gpu),speed(snapshot.down),speed(snapshot.up))
}
fn speed(value: Option<f64>) -> String {
    let Some(value) = value.filter(|n| n.is_finite() && *n >= 0.0) else { return "--".into(); };
    let (number,unit)=if value >= 1024.0*1024.0*1024.0 {(value/(1024.0*1024.0*1024.0),"GB/s")}
        else if value >= 1024.0*1024.0 {(value/(1024.0*1024.0),"MB/s")}
        else {(value/1024.0,"KB/s")};
    if number >= 100.0 || unit=="KB/s" {format!("{number:.0} {unit}")}else{format!("{number:.1} {unit}")}
}

struct Bitmap { dc:HDC, bitmap:HBITMAP, old:HGDIOBJ, bits:*mut u8, width:i32, height:i32 }
impl Bitmap {
    fn new(width:i32,height:i32)->Result<Self,String> { unsafe {
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
        std::ptr::write_bytes(bits,0,(width*height*4) as usize);
        Ok(Self { dc,bitmap,old,bits:bits as _,width,height })
    } }
}
impl Drop for Bitmap { fn drop(&mut self) { unsafe { SelectObject(self.dc,self.old); DeleteObject(self.bitmap as _); DeleteDC(self.dc); } } }
fn render(snapshot:&Snapshot, width:i32, height:i32, dpi:u32, light:bool) -> Result<Bitmap,String> {
    unsafe {
        // Render at twice the physical size, then downsample the coverage mask.
        // Small labels and hand-drawn arrows stay smooth at 100% and fractional DPI.
        const AA:i32=2;
        let mask=Bitmap::new(width*AA,height*AA)?;
        let dc=mask.dc;
        let scale=(dpi.max(96) as f32/96.0).min(width as f32/WIDTH as f32).min(height as f32/HEIGHT as f32)*AA as f32;
        let px=|value:f32| (value*scale).round() as i32;
        let font=|size:f32,weight:i32| CreateFontW(-px(size),0,0,0,weight,0,0,0,DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,CLIP_DEFAULT_PRECIS as u32,ANTIALIASED_QUALITY as u32,DEFAULT_PITCH as u32,wide("Segoe UI").as_ptr());
        let label_font=font(9.0,400);
        let value_font=font(11.0,600);
        let network_font=font(10.0,400);
        let old_font=SelectObject(dc,label_font as _);
        SetBkMode(dc,TRANSPARENT as i32);
        let draw=|value:&str,left:f32,top:f32,right:f32,bottom:f32,alignment:u32| {
            let mut rect=RECT {left:px(left),top:px(top),right:px(right),bottom:px(bottom)};let mut text=wide(value);
            DrawTextW(dc,text.as_mut_ptr(),-1,&mut rect,alignment|DT_VCENTER|DT_SINGLELINE|DT_NOPREFIX);
        };
        for (index,(label,value)) in [("CPU",snapshot.cpu),("RAM",snapshot.ram),("GPU",snapshot.gpu)].iter().enumerate() {
            let left=2.0+index as f32*33.0;
            SelectObject(dc,label_font as _);SetTextColor(dc,0x999999);
            draw(label,left,0.0,left+31.0,12.0,DT_CENTER);
            SelectObject(dc,value_font as _);SetTextColor(dc,0xffffff);
            draw(&percent(*value),left,12.0,left+31.0,28.0,DT_CENTER);
        }
        let divider=RECT {left:px(106.0),top:px(5.0),right:px(106.0)+AA,bottom:px(23.0)};
        SetDCBrushColor(dc,0x303030);FillRect(dc,&divider,GetStockObject(DC_BRUSH) as _);
        SelectObject(dc,network_font as _);SetTextColor(dc,0xeeeeee);
        draw(&speed(snapshot.down),125.0,0.0,175.0,14.0,DT_RIGHT);
        draw(&speed(snapshot.up),125.0,14.0,175.0,28.0,DT_RIGHT);
        // Vector arrows avoid font-dependent glyphs and keep their strokes crisp.
        let pen=CreatePen(PS_SOLID,px(1.0).max(1),0xffffff);
        let old_pen=SelectObject(dc,pen as _);
        for (top,down) in [(0.0,true),(14.0,false)] {
            let tip=if down {top+10.0}else{top+4.0};
            let end=if down {top+3.0}else{top+11.0};
            let shoulder=if down {tip-3.0}else{tip+3.0};
            MoveToEx(dc,px(117.5),px(end),std::ptr::null_mut());LineTo(dc,px(117.5),px(tip));
            let points=[POINT{x:px(114.5),y:px(shoulder)},POINT{x:px(117.5),y:px(tip)},POINT{x:px(120.5),y:px(shoulder)}];
            Polyline(dc,points.as_ptr(),points.len() as i32);
        }
        SelectObject(dc,old_pen);DeleteObject(pen as _);
        SelectObject(dc,old_font);
        for font in [label_font,value_font,network_font] { DeleteObject(font as _); }
        GdiFlush();
        let bitmap=Bitmap::new(width,height)?;
        let source=std::slice::from_raw_parts(mask.bits,(mask.width*mask.height*4) as usize);
        let pixels=std::slice::from_raw_parts_mut(bitmap.bits,(width*height*4) as usize);
        let primary=if light {[57u8,50,46]}else{[248u8,244,240]};
        let download=if light {[194u8,116,36]}else{[245u8,190,111]};
        let upload=if light {[117u8,137,35]}else{[183u8,213,116]};
        for (index,pixel) in pixels.chunks_exact_mut(4).enumerate() {
            let x=index as i32%width;let y=index as i32/width;
            let mut coverage=0u16;
            for dy in 0..AA { for dx in 0..AA {
                let offset=(((y*AA+dy)*mask.width+x*AA+dx)*4) as usize;
                coverage+=source[offset].max(source[offset+1]).max(source[offset+2]) as u16;
            } }
            let alpha=((coverage+2)/(AA*AA) as u16) as u8;
            let color=if x*AA>=px(112.0)&&x*AA<px(123.0) {
                if y*AA<px(14.0) {download}else{upload}
            }else{primary};
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
        let taskbar=FindWindowW(wide("Shell_TrayWnd").as_ptr(),std::ptr::null());
        let child=FindWindowExW(taskbar,std::ptr::null_mut(),wide(CLASS).as_ptr(),std::ptr::null());
        let hwnd=if child.is_null() {FindWindowW(wide(CLASS).as_ptr(),std::ptr::null())}else{child};
        if hwnd.is_null(){return serde_json::Value::Null;}
        let mut rect:RECT=std::mem::zeroed();GetWindowRect(hwnd,&mut rect);
        let point=WindowFromPoint(POINT{x:(rect.left+rect.right)/2,y:(rect.top+rect.bottom)/2});
        let mut class=[0u16;128];let length=GetClassNameW(point,class.as_mut_ptr(),class.len() as i32).max(0) as usize;
        serde_json::json!({"handle":hwnd as usize,"visible":IsWindowVisible(hwnd)!=0,"rect":array(rect),
            "parent":GetParent(hwnd) as usize,"owner":GetWindow(hwnd,GW_OWNER) as usize,
            "style":GetWindowLongW(hwnd,GWL_STYLE) as u32,"exStyle":GetWindowLongW(hwnd,GWL_EXSTYLE) as u32,
            "siblingAbove":GetWindow(hwnd,GW_HWNDPREV) as usize,
            "windowAtCenter":{"handle":point as usize,"class":String::from_utf16_lossy(&class[..length])}})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dock()->Dock {Dock{taskbar:0,bar:[0,1032,2560,1080],monitor:[0,0,2560,1080],tray_left:2300,buttons_right:1800,dpi:96,light:true}}
    #[test]
    fn placement_is_left_of_tray_and_never_over_icons() {
        let mut dock=dock();
        assert_eq!(dock.placement(dock.bar),Some([2118,1042,2294,1070]));
        dock.buttons_right=2200;
        assert!(dock.placement(dock.bar).is_none());
    }
    #[test]
    fn placement_handles_negative_monitors_dpi_and_auto_hide() {
        let mut dock=dock();
        dock.bar=[-2560,1032,0,1080];dock.monitor=[-2560,0,0,1080];dock.tray_left=-260;dock.buttons_right=-760;
        assert_eq!(dock.placement(dock.bar),Some([-442,1042,-266,1070]));
        assert!(dock.placement([-2560,1078,0,1126]).is_none());
        dock.dpi=144;dock.bar=[-2560,1008,0,1080];
        let rect=dock.placement(dock.bar).unwrap();assert_eq!(rect[2]-rect[0],264);
    }
    #[test]
    fn rates_and_unavailable_counters_have_readable_units() {
        assert_eq!(speed(Some(0.0)),"0 KB/s");assert_eq!(speed(Some(1048576.0)),"1.0 MB/s");
        assert_eq!(speed(Some(1023.9*1048576.0)),"1024 MB/s");
        assert_eq!(speed(None),"--");assert_eq!(percent(Some(500.0)),"100%");
    }
    #[test]
    fn tray_popups_are_not_fullscreen_and_equal_labels_reuse_the_surface() {
        assert!(shell_popup("TopLevelWindowForOverflowXamlIsland"));
        assert!(shell_popup("XamlExplorerHostIslandWindow"));
        assert!(!shell_popup("SDL_app"));
        let a=Snapshot{cpu:Some(20.1),ram:Some(70.1),sequence:1,..Snapshot::default()};
        let b=Snapshot{cpu:Some(20.2),ram:Some(70.2),sequence:2,..Snapshot::default()};
        assert_eq!(visual_key(&a,WIDTH,HEIGHT,96,true),visual_key(&b,WIDTH,HEIGHT,96,true));
        assert_ne!(visual_key(&a,WIDTH,HEIGHT,96,true),visual_key(&a,WIDTH,HEIGHT,96,false));
    }
    #[test]
    #[ignore = "Isolated native taskbar visibility test; runs explicitly in CI"]
    fn native_panel_recovers_visibility_geometry_and_shell_recreation() {unsafe {
        let owner=CreateWindowExW(WS_EX_TOOLWINDOW,wide("STATIC").as_ptr(),wide("Monitor test owner").as_ptr(),
            WS_POPUP|WS_VISIBLE,10000,10000,1000,48,std::ptr::null_mut(),std::ptr::null_mut(),GetModuleHandleW(std::ptr::null()),std::ptr::null());
        assert!(!owner.is_null());
        let dock=Dock{taskbar:owner as isize,bar:[10000,10000,11000,10048],monitor:[10000,9500,11000,10048],
            tray_left:10900,buttons_right:10500,dpi:96,light:true};
        let mut window=Window::new(std::ptr::null_mut()).unwrap();
        let mut snapshot=Snapshot{cpu:Some(20.0),sequence:1,..Snapshot::default()};
        window.update(&snapshot,Some(&dock),false);
        let dc=window.bitmap.as_ref().unwrap().dc;
        assert_ne!(IsWindowVisible(window.hwnd),0);
        assert_eq!(GetParent(window.hwnd),owner);
        assert_ne!(GetWindowLongW(window.hwnd,GWL_STYLE) as u32 & WS_CHILD,0);
        assert_eq!(GetWindowLongW(window.hwnd,GWL_STYLE) as u32 & WS_POPUP,0);
        let assert_rect=|hwnd:HWND| {let mut rect:RECT=std::mem::zeroed();assert_ne!(GetWindowRect(hwnd,&mut rect),0);assert_eq!(array(rect),dock.placement(dock.bar).unwrap());};
        assert_rect(window.hwnd);
        let popup=CreateWindowExW(WS_EX_TOOLWINDOW,wide("STATIC").as_ptr(),wide("Monitor test popup").as_ptr(),
            WS_POPUP,10000,9700,200,300,owner,std::ptr::null_mut(),GetModuleHandleW(std::ptr::null()),std::ptr::null());
        let sibling=CreateWindowExW(0,wide("STATIC").as_ptr(),wide("Monitor test backdrop").as_ptr(),
            WS_CHILD|WS_VISIBLE,0,0,1000,48,owner,std::ptr::null_mut(),GetModuleHandleW(std::ptr::null()),std::ptr::null());
        let foreground=GetForegroundWindow();
        for index in 0..10 {
            ShowWindow(popup,if index%2==0 {SW_SHOWNOACTIVATE}else{SW_HIDE});snapshot.sequence+=1;
            // Simulate an overlapping taskbar backdrop being reordered by WinUI.
            SetWindowPos(sibling,HWND_TOP,0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE);
            window.update(&snapshot,Some(&dock),false);
            assert_ne!(IsWindowVisible(window.hwnd),0);assert_eq!(window.bitmap.as_ref().unwrap().dc,dc);
            assert_eq!(GetParent(window.hwnd),owner);assert!(GetWindow(window.hwnd,GW_HWNDPREV).is_null());
            assert_rect(window.hwnd);
        }
        assert_eq!(GetForegroundWindow(),foreground);
        ShowWindow(window.hwnd,SW_HIDE);
        assert_eq!(IsWindowVisible(window.hwnd),0);
        window.update(&snapshot,Some(&dock),false);assert_ne!(IsWindowVisible(window.hwnd),0);
        SetWindowPos(window.hwnd,std::ptr::null_mut(),0,0,WIDTH,HEIGHT,SWP_NOZORDER|SWP_NOACTIVATE);
        window.update(&snapshot,Some(&dock),false);assert_rect(window.hwnd);
        window.update(&snapshot,Some(&dock),true);assert_eq!(IsWindowVisible(window.hwnd),0);
        window.update(&snapshot,Some(&dock),false);assert_ne!(IsWindowVisible(window.hwnd),0);
        assert_eq!(window.bitmap.as_ref().unwrap().dc,dc);
        DestroyWindow(window.hwnd);
        window.update(&snapshot,Some(&dock),false);
        assert_ne!(IsWindowVisible(window.hwnd),0);assert_eq!(GetParent(window.hwnd),owner);
        ShowWindow(owner,SW_HIDE);
        window.update(&snapshot,Some(&dock),false);assert_eq!(IsWindowVisible(window.hwnd),0);
        ShowWindow(owner,SW_SHOWNOACTIVATE);
        window.update(&snapshot,Some(&dock),false);assert_ne!(IsWindowVisible(window.hwnd),0);
        assert_rect(window.hwnd);
        DestroyWindow(popup);DestroyWindow(sibling);DestroyWindow(owner);
        assert_eq!(IsWindow(window.hwnd),0);
        window.update(&snapshot,Some(&dock),false); // stale Explorer handle
        let replacement=CreateWindowExW(WS_EX_TOOLWINDOW,wide("STATIC").as_ptr(),wide("Replacement taskbar").as_ptr(),
            WS_POPUP|WS_VISIBLE,10000,10000,1000,48,std::ptr::null_mut(),std::ptr::null_mut(),GetModuleHandleW(std::ptr::null()),std::ptr::null());
        let dock=Dock{taskbar:replacement as isize,..dock};
        window.update(&snapshot,Some(&dock),false);
        assert_ne!(IsWindowVisible(window.hwnd),0);assert_eq!(GetParent(window.hwnd),replacement);
        drop(window);DestroyWindow(replacement);
    }}
    #[test]
    fn native_text_has_alpha_in_both_taskbar_themes() {
        let snapshot=Snapshot{cpu:Some(100.0),ram:Some(99.0),gpu:Some(88.0),down:Some(1e8),up:Some(1e7),..Snapshot::default()};
        for light in [true,false] {
            let bitmap=render(&snapshot,WIDTH,HEIGHT,96,light).unwrap();
            let pixels=unsafe {std::slice::from_raw_parts(bitmap.bits,(WIDTH*HEIGHT*4) as usize)};
            assert!(pixels.chunks_exact(4).filter(|p|p[3]>16).count()>500);
            assert!(pixels.chunks_exact(4).all(|p|p[..3].iter().all(|color|*color<=p[3])));
        }
    }
}
