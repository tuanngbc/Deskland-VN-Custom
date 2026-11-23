//! Cửa sổ Cài đặt giống hệt bản gốc: mở chính trang `prototype/settings.html` trong WebView2.
//! WebView2 CHỈ chạy khi cửa sổ đang mở; đóng cửa sổ là đóng controller -> các tiến trình
//! msedgewebview2 thoát, RAM về lại như cũ. Thông điệp trang gửi lên được xếp hàng (`take_inbox`),
//! vòng lặp chính xử lý rồi trả lời bằng `post`.

use crate::desktop::wide;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::{CallDevToolsProtocolMethodCompletedHandler, CoTaskMemPWSTR, CreateCoreWebView2ControllerCompletedHandler, CreateCoreWebView2EnvironmentCompletedHandler, ProcessFailedEventHandler, WebMessageReceivedEventHandler};
use windows::core::{Interface, HSTRING, PCWSTR, PWSTR};
use windows_sys::Win32::Foundation as F;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CLASS: &str = "TuanpcDeskWebSettings";

/// `--webview-debug` (chỉ để phát triển / kiểm thử): mở cổng DevTools 9222 cho trang cài đặt.
pub static DEBUG_PORT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct Web {
    ticket: u64,
    hwnd: F::HWND,
    controller: ICoreWebView2Controller,
    webview: ICoreWebView2,
}

struct Opening { ticket: u64, hwnd: F::HWND, started: Instant, error: Option<String> }
const OPEN_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TrialFault { Controller, NoReady, Renderer }

thread_local! {
    static WEB: RefCell<Option<Web>> = const { RefCell::new(None) };
    static INBOX: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static COM_READY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static APPEARANCE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    static OPENING: RefCell<Option<Opening>> = const { RefCell::new(None) };
    static TICKET: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static RUNTIME_ERROR: RefCell<Option<(u64, String)>> = const { RefCell::new(None) };
    static TRIAL_FAULT: std::cell::Cell<Option<TrialFault>> = const { std::cell::Cell::new(None) };
    static TRIAL_CRASH: std::cell::Cell<Option<(u64, Option<Instant>)>> = const { std::cell::Cell::new(None) };
}

pub fn set_trial_fault(fault: TrialFault) {
    assert!(crate::trial::active());
    TRIAL_FAULT.with(|f| f.set(Some(fault)));
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

pub(crate) fn host()->Option<F::HWND>{
    OPENING.with(|p|p.borrow().as_ref().map(|p|p.hwnd))
        .or_else(||WEB.with(|w|w.borrow().as_ref().map(|w|w.hwnd)))
        .filter(|&hwnd|unsafe{IsWindow(hwnd)!=0})
}
pub(crate) fn waiting()->bool{OPENING.with(|p|p.borrow().is_some())}

pub fn is_open() -> bool {
    WEB.with(|w| w.try_borrow().map(|w| w.is_some()).unwrap_or(true))
}

pub fn take_inbox() -> Vec<String> {
    INBOX.with(|i| std::mem::take(&mut *i.borrow_mut()))
}

pub fn post(json: &str) {
    let view=WEB.with(|w|w.borrow().as_ref().map(|w|w.webview.clone()));
    if let Some(view)=view {
        let s=HSTRING::from(json);
        let _=unsafe{view.PostWebMessageAsString(PCWSTR(s.as_ptr()))};
    }
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
    let controller=WEB.with(|w|w.borrow().as_ref().filter(|w|w.hwnd==hwnd).map(|w|w.controller.clone()));
    if let Some(controller)=controller {
        let mut r: F::RECT = unsafe { std::mem::zeroed() };
        unsafe { GetClientRect(hwnd, &mut r) };
        let rr = windows::Win32::Foundation::RECT { left: 0, top: 0, right: r.right, bottom: r.bottom };
        let _ = unsafe { controller.SetBounds(rr) };
    }
}

unsafe extern "system" fn proc_(h: F::HWND, m: u32, wp: F::WPARAM, lp: F::LPARAM) -> F::LRESULT {
    if matches!(m,WM_WINDOWPOSCHANGED|WM_ACTIVATE|WM_SHOWWINDOW){crate::dock::internal_app::changed(h);}
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
            DestroyWindow(h);
            0
        }
        WM_DESTROY => {
            crate::dock::internal_app::unregister(h);
            // Invalidate this request before calling COM: Close may dispatch a
            // final callback. Never close anything belonging to a newer host.
            let pending = OPENING.with(|p| {
                let mut p = p.borrow_mut();
                if p.as_ref().is_some_and(|p| p.hwnd == h) { p.take() } else { None }
            });
            let web = WEB.with(|w| {
                let mut w = w.borrow_mut();
                if w.as_ref().is_some_and(|w| w.hwnd == h) { w.take() } else { None }
            });
            if pending.is_some() || web.is_some() {
                INBOX.with(|i| i.borrow_mut().clear());
                RUNTIME_ERROR.with(|e| e.borrow_mut().take());
                TRIAL_CRASH.with(|f|f.set(None));
            }
            if let Some(w) = web {
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
        if unsafe{crate::desktop::show_settings_window(h)}{
            return Ok(());
        }
        close();
        return Err("Không hiện được cửa sổ WebView2".into());
    }
    unsafe {
        if !COM_READY.with(|c| c.get()) {
            windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED)
                .ok().map_err(|e| e.to_string())?;
            COM_READY.with(|c| c.set(true));
        }
        let hwnd = create_host()?;
        let ticket=TICKET.with(|n|{let next=n.get().wrapping_add(1);n.set(next);next});
        OPENING.with(|p| p.replace(Some(Opening{ticket,hwnd,started:Instant::now(),error:None})));
        let fault = TRIAL_FAULT.with(|f| f.take());
        if let Err(error) = start(ticket, proto.to_owned(), fault) {
            close();
            return Err(error);
        }
        Ok(())
    }
}

unsafe fn create_host() -> Result<F::HWND,String> {
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
    Ok(hwnd)
}

fn opening_window(ticket: u64) -> Option<F::HWND> {
    OPENING.with(|p| p.borrow().as_ref().filter(|p| p.ticket==ticket).map(|p|p.hwnd))
}
fn failed(ticket: u64, error: impl ToString) {
    OPENING.with(|p| {
        if let Some(p)=p.borrow_mut().as_mut().filter(|p|p.ticket==ticket){p.error=Some(error.to_string());}
    });
}

fn installed(ticket: u64) -> bool {
    WEB.with(|w| w.try_borrow().is_ok_and(|w| w.as_ref().is_some_and(|w| w.ticket == ticket)))
}

fn process_failed(ticket: u64, kind: COREWEBVIEW2_PROCESS_FAILED_KIND) {
    if kind != COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED
        && kind != COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
        && kind != COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE {
        return;
    }
    if opening_window(ticket).is_none() && !installed(ticket) { return; }
    // No destruction from inside a WebView2 event callback. The owner consumes
    // this after dispatch, dropping any commands queued by the failed view.
    RUNTIME_ERROR.with(|e| e.replace(Some((ticket, format!("Tiến trình WebView2 gặp lỗi ({})", kind.0)))));
}

fn queue_page_message(ticket: u64, message: String) {
    if (opening_window(ticket).is_some() || installed(ticket))
        && !RUNTIME_ERROR.with(|e| e.borrow().as_ref().is_some_and(|(t,_)| *t==ticket)) {
        INBOX.with(|i| i.borrow_mut().push(message));
    }
}
pub fn take_open_error() -> Option<String> {
    let runtime = RUNTIME_ERROR.with(|e| e.borrow_mut().take())
        .filter(|(ticket,_)| opening_window(*ticket).is_some() || installed(*ticket))
        .map(|(_,message)| message);
    let error=runtime.or_else(|| OPENING.with(|p| {
        let p=p.borrow();
        let p=p.as_ref()?;
        p.error.clone().or_else(||(p.started.elapsed()>=OPEN_TIMEOUT).then(||"WebView2 không gửi tín hiệu sẵn sàng trong 8 giây".to_owned()))
    }));
    if error.is_some(){close();}
    error
}
pub fn ready() -> Result<(),String> {
    if let Some((ticket,hwnd,started))=OPENING.with(|p|p.borrow().as_ref().map(|p|(p.ticket,p.hwnd,p.started))){
        if !installed(ticket) {return Ok(());}
        if !unsafe{crate::desktop::show_settings_window(hwnd)}{
            close();
            return Err("WebView2 đã sẵn sàng nhưng không hiện được cửa sổ".into());
        }
        crate::dock::internal_app::register(hwnd);
        OPENING.with(|p| p.borrow_mut().take());
        TRIAL_CRASH.with(|f|if f.get()==Some((ticket,None)) {f.set(Some((ticket,Some(Instant::now()+Duration::from_secs(3)))));});
        crate::log!("Mở cửa sổ cài đặt WebView2 sau {:.0} ms",started.elapsed().as_secs_f64()*1000.0);
    }
    Ok(())
}
pub fn close() -> bool {
    let pending=OPENING.with(|p|p.borrow().as_ref().map(|p|p.hwnd));
    let hwnd=pending.or_else(||WEB.with(|w|w.borrow().as_ref().map(|w|w.hwnd)));
    if let Some(hwnd)=hwnd{unsafe{if IsWindow(hwnd)!=0&&DestroyWindow(hwnd)==0{return false;}}}
    OPENING.with(|p|p.borrow_mut().take());
    RUNTIME_ERROR.with(|e|e.borrow_mut().take());
    INBOX.with(|i|i.borrow_mut().clear());
    true
}

pub fn poll_trial_fault() {
    let due=TRIAL_CRASH.with(|f|{
        match f.get(){
            Some((ticket,Some(at))) if Instant::now()>=at=>{f.set(None);Some(ticket)}
            _=>None,
        }
    });
    let Some(ticket)=due else{return;};
    let view=WEB.with(|w|w.borrow().as_ref().filter(|w|w.ticket==ticket).map(|w|w.webview.clone()));
    if let Some(view)=view {
        let method=HSTRING::from("Page.crash");
        let args=HSTRING::from("{}");
        let handler=CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_,_|Ok(())));
        match unsafe{view.CallDevToolsProtocolMethod(PCWSTR(method.as_ptr()),PCWSTR(args.as_ptr()),&handler)} {
            Ok(())=>crate::log!("Trial Settings: requested crash for isolated renderer"),
            Err(error)=>crate::log!("Trial Settings: renderer crash request failed: {error}"),
        }
    }
}

fn start(ticket: u64, proto: PathBuf, fault: Option<TrialFault>) -> Result<(), String> {
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
            if fault == Some(TrialFault::Controller) {
                if let Some(controller)=controller {let _=unsafe{controller.Close()};}
                failed(ticket,"trial: controller initialization failed");
                return Ok(());
            }
            if let Err(err)=err{
                if let Some(controller)=controller {let _=unsafe{controller.Close()};}
                failed(ticket,err);return Ok(());
            }
            let Some(controller)=controller else{failed(ticket,"WebView2 không trả về controller");return Ok(());};
            match attach(ticket,hwnd,&proto,&controller,fault) {
                Ok(web) if opening_window(ticket)==Some(hwnd)=>{WEB.with(|w|w.replace(Some(web)));resize(hwnd);}
                Ok(_)=>{let _=unsafe{controller.Close()};}
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

fn attach(ticket:u64, hwnd:F::HWND, proto:&Path, controller:&ICoreWebView2Controller, fault:Option<TrialFault>)->Result<Web,String>{
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
                &WebMessageReceivedEventHandler::create(Box::new(move |_wv, args| {
                    if let Some(args) = args {
                        let mut p = PWSTR(std::ptr::null_mut());
                        if args.TryGetWebMessageAsString(&mut p).is_ok() {
                            let s = CoTaskMemPWSTR::from(p).to_string();
                            if fault != Some(TrialFault::NoReady) { queue_page_message(ticket, s); }
                        }
                    }
                    Ok(())
                })),
                &mut token,
            )
            .map_err(|e| e.to_string())?;
        webview.add_ProcessFailed(&ProcessFailedEventHandler::create(Box::new(move |_, args| {
            if let Some(args)=args {
                let mut kind=COREWEBVIEW2_PROCESS_FAILED_KIND(0);
                if args.ProcessFailedKind(&mut kind).is_ok(){ process_failed(ticket,kind); }
            }
            Ok(())
        })),&mut token).map_err(|e|e.to_string())?;
        let url = HSTRING::from(format!("https://app.local/settings.html?v={}", env!("CARGO_PKG_VERSION")));
        webview.Navigate(PCWSTR(url.as_ptr())).map_err(|e| e.to_string())?;
        if fault == Some(TrialFault::Renderer) {
            // Use only the dedicated trial profile and the Settings renderer.
            // Schedule after the page is visible, from the owner loop below.
            TRIAL_CRASH.with(|p|p.set(Some((ticket,None))));
        }
        Ok(Web { ticket, hwnd, controller:controller.clone(), webview })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_close_reopen_rejects_stale_callbacks_and_timeout_destroys_hidden_host(){
        let first=unsafe{create_host()}.expect("first Settings HWND");
        OPENING.with(|p|p.replace(Some(Opening{ticket:7,hwnd:first,started:Instant::now(),error:None})));
        queue_page_message(7,"first request queued".into());
        // A second request while creating must coalesce, even with no runtime.
        open(std::path::Path::new("unused")).unwrap();
        assert_eq!(opening_window(7),Some(first));
        assert_eq!(unsafe{IsWindowVisible(first)},0);
        unsafe{SendMessageW(first,WM_CLOSE,0,0);}
        assert_eq!(unsafe{IsWindow(first)},0);
        assert!(opening_window(7).is_none());
        assert!(take_inbox().is_empty());

        let second=unsafe{create_host()}.expect("second Settings HWND");
        OPENING.with(|p|p.replace(Some(Opening{ticket:8,hwnd:second,started:Instant::now(),error:None})));
        failed(7,"stale completion");
        queue_page_message(7,"late message from canceled request".into());
        process_failed(7,COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED);
        assert!(take_inbox().is_empty());
        assert!(take_open_error().is_none());
        assert_eq!(opening_window(8),Some(second));
        OPENING.with(|p|p.borrow_mut().as_mut().unwrap().started=Instant::now()-OPEN_TIMEOUT);
        assert!(take_open_error().is_some());
        assert_eq!(unsafe{IsWindow(second)},0);
        assert!(opening_window(8).is_none());
    }

    #[test]
    fn terminal_failure_clears_host_and_rejects_page_messages_but_gpu_exit_does_not(){
        let hwnd=unsafe{create_host()}.expect("Settings HWND");
        OPENING.with(|p|p.replace(Some(Opening{ticket:5,hwnd,started:Instant::now(),error:None})));
        process_failed(5,COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED);
        assert!(take_open_error().is_none());
        queue_page_message(5,"ready".into());
        process_failed(5,COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED);
        assert!(take_open_error().is_some());
        assert!(take_inbox().is_empty());
        assert_eq!(unsafe{IsWindow(hwnd)},0);
    }
}
