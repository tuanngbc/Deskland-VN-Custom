//! Launcher nhanh kiểu Spotlight: một cửa sổ native nhỏ, không chạy tiến trình
//! nền riêng. Danh sách lấy từ Start Menu/Desktop và mở bằng ShellExecuteW.

use crate::desktop::wide;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CLASS: &str = "DesklandRsLauncher";
const IDC_EDIT: usize = 1;
const IDC_LIST: usize = 2;

struct Entry { name: String, path: PathBuf }
struct State { hwnd: HWND, edit: HWND, list: HWND, all: Vec<Entry>, visible: Vec<usize>, font: HFONT }
thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    // Tách brush khỏi STATE để WM_CTLCOLOR không mượn RefCell đang được giữ
    // trong WM_COMMAND khi người dùng gõ tìm kiếm.
    static BG_BRUSH: Cell<HBRUSH> = const { Cell::new(std::ptr::null_mut()) };
}

unsafe extern "system" { fn SetFocus(hwnd: HWND) -> HWND; }

fn enum_dir(root: &Path, depth: u8, out: &mut Vec<Entry>) {
    if depth == 0 { return; }
    let Ok(rd) = std::fs::read_dir(root) else { return };
    for item in rd.flatten() {
        let p = item.path();
        if p.is_dir() { enum_dir(&p, depth - 1, out); continue; }
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or_default().to_ascii_lowercase();
        if !matches!(ext.as_str(), "lnk" | "exe" | "url") { continue; }
        let mut name = p.file_stem().and_then(|x| x.to_str()).unwrap_or_default().to_string();
        if name.is_empty() { continue; }
        name = name.replace('_', " ");
        out.push(Entry { name, path: p });
    }
}

fn entries() -> Vec<Entry> {
    let mut out = Vec::new();
    for key in ["APPDATA", "PROGRAMDATA", "USERPROFILE"] {
        if let Ok(base) = std::env::var(key) {
            let p = PathBuf::from(base);
            let root = if key == "USERPROFILE" { p.join("Desktop") } else { p.join("Microsoft\\Windows\\Start Menu\\Programs") };
            enum_dir(&root, 4, &mut out);
        }
    }
    for (name, command) in [("Paint", "mspaint.exe"), ("Notepad", "notepad.exe"), ("Calculator", "calc.exe"), ("File Explorer", "explorer.exe"), ("Settings", "ms-settings:")] {
        out.push(Entry { name: name.to_string(), path: PathBuf::from(command) });
    }
    out.sort_by_key(|e| e.name.to_ascii_lowercase());
    out.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name) || a.path.to_string_lossy().eq_ignore_ascii_case(&b.path.to_string_lossy()));
    out
}

unsafe fn set_font(hwnd: HWND, font: HFONT) { SendMessageW(hwnd, WM_SETFONT, font as WPARAM, 1); }
unsafe fn refresh(s: &mut State) {
    let mut buf = vec![0u16; 512];
    let n = GetWindowTextW(s.edit, buf.as_mut_ptr(), buf.len() as i32);
    let q = String::from_utf16_lossy(&buf[..n.max(0) as usize]).to_ascii_lowercase();
    SendMessageW(s.list, LB_RESETCONTENT, 0, 0);
    s.visible.clear();
    for (i, e) in s.all.iter().enumerate() {
        if q.is_empty() || e.name.to_ascii_lowercase().contains(&q) {
            s.visible.push(i);
            let text = wide(&e.name);
            SendMessageW(s.list, LB_ADDSTRING, 0, text.as_ptr() as LPARAM);
            if s.visible.len() >= 30 { break; }
        }
    }
    if !s.visible.is_empty() { SendMessageW(s.list, LB_SETCURSEL, 0, 0); }
}

unsafe fn launch_selected(s: &mut State) {
    let pos = SendMessageW(s.list, LB_GETCURSEL, 0, 0) as usize;
    let Some(&idx) = s.visible.get(pos) else { return; };
    let path = wide(&s.all[idx].path.to_string_lossy());
    let verb = wide("open");
    ShellExecuteW(s.hwnd, verb.as_ptr(), path.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL);
    ShowWindow(s.hwnd, SW_HIDE);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            let dc = w as HDC;
            SetTextColor(dc, 0x00F5F5F7); SetBkColor(dc, 0x001C1C20);
            BG_BRUSH.with(|brush| {
                let brush = brush.get();
                if brush.is_null() { GetSysColorBrush(COLOR_WINDOW) as LRESULT } else { brush as LRESULT }
            })
        }
        WM_ACTIVATE if (w & 0xffff) as u32 == WA_INACTIVE => { ShowWindow(hwnd, SW_HIDE); 0 }
        WM_COMMAND => {
            STATE.with(|cell| if let Some(s) = cell.borrow_mut().as_mut() {
                let id = (w & 0xffff) as usize; let code = ((w >> 16) & 0xffff) as u16;
                if id == IDC_EDIT && code == EN_CHANGE as u16 { refresh(s); }
                if id == IDC_LIST && code == LBN_DBLCLK as u16 { launch_selected(s); }
            }); 0
        }
        WM_KEYDOWN if w as u32 == VK_ESCAPE as u32 => { ShowWindow(hwnd, SW_HIDE); 0 }
        WM_KEYDOWN if w as u32 == VK_RETURN as u32 => { STATE.with(|c| if let Some(s)=c.borrow_mut().as_mut(){ launch_selected(s); }); 0 }
        WM_CLOSE => { ShowWindow(hwnd, SW_HIDE); 0 }
        WM_DESTROY => 0,
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

pub fn toggle(owner: HWND) {
    STATE.with(|cell| unsafe {
        if let Some(s) = cell.borrow_mut().as_mut() {
            let visible = IsWindowVisible(s.hwnd) != 0;
            ShowWindow(s.hwnd, if visible { SW_HIDE } else { SW_SHOW });
            if !visible {
                SetWindowPos(s.hwnd, HWND_TOP, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
                SetForegroundWindow(s.hwnd); SetFocus(s.edit); refresh(s);
            }
            return;
        }
        let cls = wide(CLASS); let mut wc: WNDCLASSEXW = std::mem::zeroed();
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32; wc.lpfnWndProc = Some(proc);
        wc.hInstance = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
        wc.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW); wc.lpszClassName = cls.as_ptr(); wc.hbrBackground = CreateSolidBrush(0x001C1C20);
        RegisterClassExW(&wc);
        let sw = GetSystemMetrics(SM_CXSCREEN); let sh = GetSystemMetrics(SM_CYSCREEN); let w = 720; let h = 430;
        let hwnd = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_LAYERED, cls.as_ptr(), wide("Deskland Launcher").as_ptr(), WS_POPUP, (sw-w)/2, (sh-h)/3, w, h, owner, std::ptr::null_mut(), wc.hInstance, std::ptr::null_mut());
        if hwnd.is_null() { return; }
        SetWindowRgn(hwnd, CreateRoundRectRgn(0, 0, w+1, h+1, 24, 24), 1);
        SetLayeredWindowAttributes(hwnd, 0, 228, LWA_ALPHA);
        let backdrop: u32 = 3;
        windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(hwnd, 38, &backdrop as *const u32 as *const _, 4);
        let font = CreateFontW(-20, 0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET as u32, 0, 0, CLEARTYPE_QUALITY as u32, DEFAULT_PITCH as u32, wide("Segoe UI Variable").as_ptr());
        let edit = CreateWindowExW(WS_EX_CLIENTEDGE, wide("EDIT").as_ptr(), std::ptr::null(), WS_CHILD|WS_VISIBLE|(ES_AUTOHSCROLL as u32), 28, 24, w-56, 48, hwnd, IDC_EDIT as _, wc.hInstance, std::ptr::null_mut());
        let list = CreateWindowExW(0, wide("LISTBOX").as_ptr(), std::ptr::null(), WS_CHILD|WS_VISIBLE|WS_VSCROLL|(LBS_NOTIFY as u32)|(LBS_NOINTEGRALHEIGHT as u32), 28, 88, w-56, h-112, hwnd, IDC_LIST as _, wc.hInstance, std::ptr::null_mut());
        SendMessageW(edit, 0x1501, 1, wide("Search apps").as_ptr() as LPARAM);
        SendMessageW(list, LB_SETITEMHEIGHT, 0, 42);
        set_font(edit, font); set_font(list, font);
        let brush = CreateSolidBrush(0x001C1C20);
        BG_BRUSH.with(|cell| cell.set(brush));
        let mut s = State { hwnd, edit, list, all: entries(), visible: Vec::new(), font };
        refresh(&mut s); *cell.borrow_mut() = Some(s); ShowWindow(hwnd, SW_SHOW); SetWindowPos(hwnd, HWND_TOP, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW); SetForegroundWindow(hwnd); SetFocus(edit);
    });
}

pub fn hotkey_text(mods: u32, vk: u32) -> String {
    let m = if mods & MOD_CONTROL != 0 { "Ctrl+" } else if mods & MOD_SHIFT != 0 { "Shift+" } else { "Alt+" };
    let k = if vk == VK_SPACE as u32 { "Space".to_string() } else if (b'A' as u32..=b'Z' as u32).contains(&vk) { char::from_u32(vk).unwrap().to_string() } else { format!("F{}", vk.saturating_sub(0x6F)) };
    format!("{m}{k}")
}
