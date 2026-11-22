//! Cửa sổ Cài đặt giống hệt bản gốc: mở chính trang `prototype/settings.html` trong WebView2.
//! WebView2 CHỈ chạy khi cửa sổ đang mở; đóng cửa sổ là đóng controller -> các tiến trình
//! msedgewebview2 thoát, RAM về lại như cũ. Thông điệp trang gửi lên được xếp hàng (`take_inbox`),
//! vòng lặp chính xử lý rồi trả lời bằng `post`.

use crate::desktop::wide;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{CoTaskMemPWSTR, CreateCoreWebView2ControllerCompletedHandler, CreateCoreWebView2EnvironmentCompletedHandler, WebMessageReceivedEventHandler};
use windows::core::{Interface, HSTRING, PCWSTR, PWSTR};
use windows_sys::Win32::Foundation as F;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CLASS: &str = "TuanpcDeskWebSettings";

/// `--webview-debug` (chỉ để phát triển / kiểm thử): mở cổng DevTools 9222 cho trang cài đặt.
pub static DEBUG_PORT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct Web {
    hwnd: F::HWND,
    controller: ICoreWebView2Controller,
    webview: ICoreWebView2,
}

struct Opening { ticket: u64, hwnd: F::HWND, started: Instant, error: Option<String> }
const OPEN_TIMEOUT: Duration = Duration::from_secs(8);

thread_local! {
    static WEB: RefCell<Option<Web>> = const { RefCell::new(None) };
    static INBOX: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static COM_READY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static APPEARANCE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    static OPENING: RefCell<Option<Opening>> = const { RefCell::new(None) };
    static TICKET: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

pub fn set_appearance(value: &str) {
    APPEARANCE.with(|c|c.set(match value {"light"=>Some(true),"dark"=>Some(false),_=>None}));
    let hwnd=WEB.with(|w|w.try_borrow().ok().and_then(|w|w.as_ref().map(|w|w.hwnd)));
    if let Some(hwnd)=hwnd { theme_title_bar(hwnd); }
}

/// Tìm thư mục `prototype` (có `settings.html` + `themes.json`): cạnh exe, hoặc đi ngược lên từ exe
/// (chạy từ `target/release` trong repo).
pub fn find_prototype() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.parent();
    while let Some(d) = dir {
        for cand in [d.join("prototype"), d.to_path_buf()] {
            if cand.join("settings.html").is_file() && cand.join("themes.json").is_file() {
                return Some(cand);
            }
        }
        dir = d.parent();
    }
    None
}

pub fn is_open() -> bool {
    WEB.with(|w| w.try_borrow().map(|w| w.is_some()).unwrap_or(true))
}

pub fn take_inbox() -> Vec<String> {
    INBOX.with(|i| std::mem::take(&mut *i.borrow_mut()))
}

pub fn post(json: &str) {
    WEB.with(|w| {
        if let Ok(w) = w.try_borrow() {
            if let Some(w) = w.as_ref() {
                let s = HSTRING::from(json);
                let _ = unsafe { w.webview.PostWebMessageAsString(PCWSTR(s.as_ptr())) };
            }
        }
    });
}

fn light_theme() -> bool {
    use windows_sys::Win32::System::Registry::HKEY_CURRENT_USER;
    let k = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let n = wide("AppsUseLightTheme");
    let mut v = 1u32;
    let mut sz = 4u32;
    unsafe {
        windows_sys::Win32::System::Registry::RegGetValueW(
            HKEY_CURRENT_USER,
            k.as_ptr(),
            n.as_ptr(),
            windows_sys::Win32::System::Registry::RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            &mut v as *mut u32 as *mut _,
            &mut sz,
        );
    }
    v != 0
}

/// Thanh tiêu đề theo chế độ sáng / tối của Windows (như bản C#).
fn theme_title_bar(hwnd: F::HWND) {
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
    let light = APPEARANCE.with(|c|c.get()).unwrap_or_else(light_theme);
    let dark: i32 = if light { 0 } else { 1 };
    let color: u32 = if light { 0x00FB_F9F7 } else { 0x0023_1B11 };
    unsafe {
        DwmSetWindowAttribute(hwnd, 20, &dark as *const i32 as *const _, 4);
        DwmSetWindowAttribute(hwnd, 35, &color as *const u32 as *const _, 4);
    }
}

fn resize(hwnd: F::HWND) {
    WEB.with(|w| {
        if let Ok(w) = w.try_borrow() {
            if let Some(w) = w.as_ref() {
                let mut r: F::RECT = unsafe { std::mem::zeroed() };
                unsafe { GetClientRect(hwnd, &mut r) };
                let rr = windows::Win32::Foundation::RECT { left: 0, top: 0, right: r.right, bottom: r.bottom };
                let _ = unsafe { w.controller.SetBounds(rr) };
            }
        }
    });
}

unsafe extern "system" fn proc_(h: F::HWND, m: u32, wp: F::WPARAM, lp: F::LPARAM) -> F::LRESULT {
    match m {
        WM_SIZE => {
            resize(h);
            0
        }
        WM_GETMINMAXINFO => {
            let s = GetDpiForWindow(h).max(96) as f32 / 96.0;
            let mi = &mut *(lp as *mut MINMAXINFO);
            mi.ptMinTrackSize.x = (900.0 * s) as i32;
            mi.ptMinTrackSize.y = (620.0 * s) as i32;
            0
        }
        WM_SETTINGCHANGE => {
            theme_title_bar(h);
            DefWindowProcW(h, m, wp, lp)
        }
        WM_ACTIVATE if wp as u32 & 0xffff == WA_INACTIVE => {
            INBOX.with(|i| i.borrow_mut().push(r#"{"t":"shortcutCapture","active":false}"#.into()));
            DefWindowProcW(h,m,wp,lp)
        }
        WM_CLOSE => {
            INBOX.with(|i| i.borrow_mut().push(r#"{"t":"shortcutCapture","active":false}"#.into()));
            DestroyWindow(h);
            0
        }
        WM_DESTROY => {
            if let Some(w) = WEB.with(|x| x.try_borrow_mut().ok().and_then(|mut g| {
                if g.as_ref().is_some_and(|w| w.hwnd == h) {g.take()} else {None}
            })) {
                // Close() dừng WebView2 -> các tiến trình msedgewebview2 của cửa sổ này thoát
                let _ = w.controller.Close();
            }
            crate::log!("Đóng cửa sổ cài đặt (WebView2 đã dừng)");
            0
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

/// Mở (hoặc đưa lên trước) cửa sổ cài đặt WebView2. Lỗi (không có WebView2 Runtime, không thấy
/// thư mục prototype...) trả về Err để bên gọi dùng cửa sổ Win32 dự phòng.
pub fn open(proto: &Path) -> Result<(), String> {
    if OPENING.with(|p| p.borrow().is_some()) {return Ok(());}
    if let Some(h) = WEB.with(|w| w.borrow().as_ref().map(|w| w.hwnd)) {
        unsafe {
            if IsIconic(h) != 0 || IsWindowVisible(h) == 0 {
                ShowWindow(h, SW_RESTORE);
            }
            SetForegroundWindow(h);
        }
        return Ok(());
    }
    unsafe {
        if !COM_READY.with(|c| c.get()) {
            windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED)
                .ok().map_err(|e| e.to_string())?;
            COM_READY.with(|c| c.set(true));
        }
        let cls = wide(CLASS);
        let mut wc: WNDCLASSEXW = std::mem::zeroed();
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(proc_);
        wc.hInstance = GetModuleHandleW(std::ptr::null());
        wc.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
        wc.hIcon = crate::app_icon(GetSystemMetrics(SM_CXICON), GetSystemMetrics(SM_CYICON));
        wc.hIconSm = crate::app_icon(GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON));
        wc.lpszClassName = cls.as_ptr();
        RegisterClassExW(&wc);

        let s = windows_sys::Win32::UI::HiDpi::GetDpiForSystem().max(96) as f32 / 96.0;
        // Cửa sổ rộng hơn để lưới theme, preview và nhóm điều khiển không bị
        // ép chật; vẫn tự co theo màn hình nhỏ.
        let (w, h) = ((1360.0 * s) as i32, (860.0 * s) as i32);
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let hwnd = CreateWindowExW(
            0,
            cls.as_ptr(),
            wide("tuanpc-desk").as_ptr(),
            WS_OVERLAPPEDWINDOW,
            ((sw - w) / 2).max(0),
            ((sh - h) / 2).max(0),
            w.min(sw),
            h.min(sh),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        if hwnd.is_null() {
            return Err(format!("CreateWindowEx lỗi {}", F::GetLastError()));
        }
        theme_title_bar(hwnd);
        let ticket=TICKET.with(|n|{let next=n.get().wrapping_add(1);n.set(next);next});
        OPENING.with(|p| p.replace(Some(Opening{ticket,hwnd,started:Instant::now(),error:None})));
        if let Err(error) = start(ticket, proto.to_owned()) {
            close();
            return Err(error);
        }
        Ok(())
    }
}

fn opening_window(ticket: u64) -> Option<F::HWND> {
    OPENING.with(|p| p.borrow().as_ref().filter(|p| p.ticket==ticket).map(|p|p.hwnd))
}
fn failed(ticket: u64, error: impl ToString) {
    OPENING.with(|p| {
        if let Some(p)=p.borrow_mut().as_mut().filter(|p|p.ticket==ticket){p.error=Some(error.to_string());}
    });
}
pub fn take_open_error() -> Option<String> {
    let error=OPENING.with(|p| {
        let p=p.borrow();
        let p=p.as_ref()?;
        p.error.clone().or_else(||(p.started.elapsed()>=OPEN_TIMEOUT).then(||"WebView2 không gửi tín hiệu sẵn sàng trong 8 giây".to_owned()))
    });
    if error.is_some(){close();}
    error
}
pub fn ready() {
    if let Some(p)=OPENING.with(|p|p.borrow_mut().take()){
        unsafe{ShowWindow(p.hwnd,SW_SHOWNORMAL);SetForegroundWindow(p.hwnd);}
        crate::log!("Mở cửa sổ cài đặt WebView2 sau {:.0} ms",p.started.elapsed().as_secs_f64()*1000.0);
    }
}
pub fn close() {
    let pending=OPENING.with(|p|p.borrow_mut().take()).map(|p|p.hwnd);
    let hwnd=pending.or_else(||WEB.with(|w|w.borrow().as_ref().map(|w|w.hwnd)));
    if let Some(hwnd)=hwnd{unsafe{DestroyWindow(hwnd);}}
    INBOX.with(|i|i.borrow_mut().clear());
}

fn start(ticket: u64, proto: PathBuf) -> Result<(), String> {
    let data = HSTRING::from(crate::lock::data_dir().join("WebView2").to_string_lossy().as_ref());
    let opts = webview2_com::CoreWebView2EnvironmentOptions::default();
    if DEBUG_PORT.load(std::sync::atomic::Ordering::Relaxed) {
        unsafe { opts.set_additional_browser_arguments("--remote-debugging-port=9222".into()) };
    }
    let opts: ICoreWebView2EnvironmentOptions = opts.into();
    let handler=CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(move |err, env| {
        let Some(hwnd)=opening_window(ticket) else{return Ok(());};
        if let Err(err)=err {failed(ticket,err);return Ok(());}
        let Some(env)=env else{failed(ticket,"WebView2 không trả về môi trường");return Ok(());};
        let handler=CreateCoreWebView2ControllerCompletedHandler::create(Box::new(move |err, controller| {
            let Some(hwnd)=opening_window(ticket) else{
                if let Some(controller)=controller{let _=unsafe{controller.Close()};}
                return Ok(());
            };
            if let Err(err)=err{failed(ticket,err);return Ok(());}
            let Some(controller)=controller else{failed(ticket,"WebView2 không trả về controller");return Ok(());};
            match attach(hwnd,&proto,&controller) {
                Ok(web)=>{WEB.with(|w|w.replace(Some(web)));resize(hwnd);}
                Err(error)=>{let _=unsafe{controller.Close()};failed(ticket,error);}
            }
            Ok(())
        }));
        if let Err(error)=unsafe{env.CreateCoreWebView2Controller(windows::Win32::Foundation::HWND(hwnd),&handler)}{
            failed(ticket,error);
        }
        Ok(())
    }));
    unsafe{CreateCoreWebView2EnvironmentWithOptions(PCWSTR::null(),PCWSTR(data.as_ptr()),&opts,&handler)}
        .map_err(|e|format!("Không khởi tạo được WebView2: {e}"))
}

fn attach(hwnd:F::HWND, proto:&Path, controller:&ICoreWebView2Controller)->Result<Web,String>{
    unsafe {
        if let Ok(c2) = controller.cast::<ICoreWebView2Controller2>() {
            let bg = if light_theme() { COREWEBVIEW2_COLOR { A: 255, R: 255, G: 255, B: 255 } } else { COREWEBVIEW2_COLOR { A: 255, R: 23, G: 23, B: 23 } };
            let _ = c2.SetDefaultBackgroundColor(bg);
        }
        let _ = controller.SetIsVisible(true);
        let webview = controller.CoreWebView2().map_err(|e| e.to_string())?;
        let st = webview.Settings().map_err(|e| e.to_string())?;
        let _ = st.SetAreDefaultContextMenusEnabled(false);
        let _ = st.SetIsStatusBarEnabled(false);
        let _ = st.SetIsZoomControlEnabled(false);
        if let Ok(s3) = st.cast::<ICoreWebView2Settings3>() {
            let _ = s3.SetAreBrowserAcceleratorKeysEnabled(false);
        }
        let _ = st.SetAreDevToolsEnabled(cfg!(debug_assertions));
        let w3 = webview.cast::<ICoreWebView2_3>().map_err(|e| format!("WebView2 Runtime quá cũ: {e}"))?;
        let host = HSTRING::from("app.local");
        let dir = HSTRING::from(proto.to_string_lossy().as_ref());
        w3.SetVirtualHostNameToFolderMapping(PCWSTR(host.as_ptr()), PCWSTR(dir.as_ptr()), COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_ALLOW).map_err(|e| e.to_string())?;
        let mut token = 0i64;
        webview
            .add_WebMessageReceived(
                &WebMessageReceivedEventHandler::create(Box::new(|_wv, args| {
                    if let Some(args) = args {
                        let mut p = PWSTR(std::ptr::null_mut());
                        if args.TryGetWebMessageAsString(&mut p).is_ok() {
                            let s = CoTaskMemPWSTR::from(p).to_string();
                            INBOX.with(|i| i.borrow_mut().push(s));
                        }
                    }
                    Ok(())
                })),
                &mut token,
            )
            .map_err(|e| e.to_string())?;
        let url = HSTRING::from(format!("https://app.local/settings.html?v={}", env!("CARGO_PKG_VERSION")));
        webview.Navigate(PCWSTR(url.as_ptr())).map_err(|e| e.to_string())?;
        Ok(Web { hwnd, controller:controller.clone(), webview })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_completion_cannot_fail_a_new_open_and_timeout_releases_the_ticket(){
        let ticket=7;
        OPENING.with(|p|p.replace(Some(Opening{
            ticket,hwnd:std::ptr::null_mut(),started:Instant::now(),error:None,
        })));
        assert!(opening_window(ticket-1).is_none());
        failed(ticket-1,"stale completion");
        assert!(take_open_error().is_none());
        OPENING.with(|p|p.borrow_mut().as_mut().unwrap().started=Instant::now()-OPEN_TIMEOUT);
        assert!(take_open_error().is_some());
        assert!(opening_window(ticket).is_none());
    }
}
