//! Native Spotlight panel. No child edit/list controls and no RefCell borrow
//! crosses a Win32 call: synchronous window messages may re-enter this module.
use crate::desktop::wide;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CLASS: &str = "DesklandRsLauncherGlass";
const ROW: i32 = 54;
const TOP: i32 = 78;
const PAGE: usize = 6;
const REFRESH: u32 = WM_APP + 101;
const OPEN_SELECTED: u32 = WM_APP + 102;

#[derive(Clone, Debug)]
struct Entry { name: String, path: PathBuf, kind: &'static str, detail: String }
struct State {
    hwnd: HWND, all: Vec<Entry>, matches: Vec<usize>, query: String,
    selected: usize, first: usize, scale: f32, width: i32, height: i32,
    glass: Vec<u8>, error: String, select_all: bool,
}
thread_local! { static STATE: RefCell<Option<State>> = const { RefCell::new(None) }; }
unsafe extern "system" { fn SetFocus(hwnd: HWND) -> HWND; }

fn enum_dir(root: &Path, depth: u8, files: bool, out: &mut Vec<Entry>) {
    if depth == 0 || out.len() >= 5000 { return; }
    let Ok(rd) = std::fs::read_dir(root) else { return };
    for item in rd.flatten() {
        if out.len() >= 5000 { break; }
        let p = item.path();
        // Do not follow junctions/symlinks into unrelated folders.
        let Ok(ft) = item.file_type() else { continue };
        if ft.is_symlink() { continue; }
        if ft.is_dir() { enum_dir(&p, depth - 1, files, out); continue; }
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or_default().to_ascii_lowercase();
        if !files && !matches!(ext.as_str(), "lnk" | "exe" | "url" | "appref-ms") { continue; }
        let name = p.file_stem().and_then(|x| x.to_str()).unwrap_or_default().replace('_', " ");
        if name.is_empty() || name.starts_with('.') || ext == "tmp" { continue; }
        let kind = match ext.as_str() { "lnk" | "exe" | "appref-ms" => "Application", "url" => "Web shortcut", _ => "File" };
        let size = item.metadata().ok().map(|m| m.len()).unwrap_or(0);
        let detail = if kind == "File" { format!("{} · {} KB · {}", ext.to_uppercase(), size.div_ceil(1024), p.display()) } else { p.display().to_string() };
        out.push(Entry { name, path: p, kind, detail });
    }
}

fn entries() -> Vec<Entry> {
    let mut out = Vec::new();
    // Prefer explicit system commands over duplicate shortcuts.
    for (name, command) in [("Paint", "mspaint.exe"), ("Notepad", "notepad.exe"), ("Calculator", "calc.exe"), ("File Explorer", "explorer.exe"), ("Settings", "ms-settings:")] {
        out.push(Entry { name: name.into(), path: command.into(), kind: "Application", detail: format!("Windows · {command}") });
    }
    for key in ["APPDATA", "PROGRAMDATA"] {
        if let Ok(base) = std::env::var(key) { enum_dir(&PathBuf::from(base).join("Microsoft\\Windows\\Start Menu\\Programs"), 5, false, &mut out); }
    }
    if let Ok(base) = std::env::var("USERPROFILE") {
        for folder in ["Desktop", "Documents", "Downloads", "OneDrive\\Desktop", "OneDrive\\Documents"] {
            enum_dir(&PathBuf::from(&base).join(folder), 2, true, &mut out);
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|e| seen.insert(if e.kind == "Application" { e.name.to_lowercase() } else { e.path.to_string_lossy().to_lowercase() }));
    out.sort_by_key(|e| (e.kind != "Application", e.name.to_lowercase()));
    out
}

fn filter(all: &[Entry], query: &str) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    let mut found: Vec<(u8, usize)> = all.iter().enumerate().filter_map(|(i, e)| {
        let name = e.name.to_lowercase();
        let rank = if q.is_empty() || name == q { 0 } else if name.starts_with(&q) { 1 } else if q.split_whitespace().all(|part| name.contains(part)) { 2 } else { return None; };
        Some((rank, i))
    }).collect();
    found.sort_by_key(|&(rank, i)| (rank, all[i].kind != "Application"));
    found.into_iter().map(|(_, i)| i).take(100).collect()
}

// Copy data before calling Win32. In particular ShellExecute, ShowWindow,
// SetFocus and painting must never run while STATE is borrowed.
fn handles() -> Option<(HWND, i32, i32, f32)> {
    STATE.with(|c| c.borrow().as_ref().map(|s| (s.hwnd, s.width, s.height, s.scale)))
}
fn refresh() {
    let hwnd = STATE.with(|c| {
        let mut guard = c.borrow_mut(); let s = guard.as_mut()?;
        s.matches = filter(&s.all, &s.query); s.selected = 0; s.first = 0; s.error.clear(); Some(s.hwnd)
    });
    if let Some(hwnd) = hwnd { unsafe { InvalidateRect(hwnd, std::ptr::null(), 0); } }
}

fn move_selection(delta: i32) {
    let hwnd = STATE.with(|c| {
        let mut guard = c.borrow_mut(); let s = guard.as_mut()?;
        s.selected = (s.selected as i32 + delta).clamp(0, s.matches.len().saturating_sub(1) as i32) as usize;
        if s.selected < s.first { s.first = s.selected; }
        if s.selected >= s.first + PAGE { s.first = s.selected + 1 - PAGE; }
        Some(s.hwnd)
    });
    if let Some(hwnd) = hwnd { unsafe { InvalidateRect(hwnd, std::ptr::null(), 0); } }
}

fn launch_selected() {
    let target = STATE.with(|c| {
        let guard = c.borrow(); let s = guard.as_ref()?;
        let idx = *s.matches.get(s.selected)?;
        Some((s.hwnd, s.all.get(idx)?.clone()))
    });
    let Some((hwnd, entry)) = target else { return; };
    unsafe {
        // Hide before ShellExecute activates another process (WM_ACTIVATE is synchronous).
        ShowWindow(hwnd, SW_HIDE);
        let result = execute(hwnd, &entry.path);
        if result <= 32 {
            crate::log!("Launcher: không mở được {} (ShellExecute {})", entry.path.display(), result);
            STATE.with(|c| { if let Some(s) = c.borrow_mut().as_mut() { s.error = format!("Could not open {} (Windows error {result})", entry.name); } });
            ShowWindow(hwnd, SW_SHOW); InvalidateRect(hwnd, std::ptr::null(), 0);
        }
    }
}

#[cfg(not(test))]
unsafe fn execute(hwnd: HWND, path: &Path) -> isize {
    ShellExecuteW(hwnd, wide("open").as_ptr(), wide(&path.to_string_lossy()).as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) as isize
}
#[cfg(test)]
unsafe fn execute(_hwnd: HWND, _path: &Path) -> isize { 33 }

// Frost the pixels behind the panel once per opening. Text remains fully opaque.
// Unlike whole-window alpha this never reveals sharp desktop icons through text,
// and costs no continuous blur/redraw work while the panel is idle.
unsafe fn capture_glass(x: i32, y: i32, width: i32, height: i32) -> Vec<u8> {
    let (bw, bh) = ((width / 4).max(1), (height / 4).max(1));
    let screen = GetDC(std::ptr::null_mut()); let dc = CreateCompatibleDC(screen);
    let mut info: BITMAPINFO = std::mem::zeroed();
    info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = bw; info.bmiHeader.biHeight = -bh; info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32; info.bmiHeader.biCompression = BI_RGB;
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let bitmap = CreateDIBSection(screen, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
    if bitmap.is_null() || bits.is_null() || dc.is_null() {
        if !bitmap.is_null() { DeleteObject(bitmap); } if !dc.is_null() { DeleteDC(dc); } ReleaseDC(std::ptr::null_mut(), screen); return Vec::new();
    }
    let old = SelectObject(dc, bitmap);
    SetStretchBltMode(dc, HALFTONE);
    let ok = StretchBlt(dc, 0, 0, bw, bh, screen, x, y, width, height, SRCCOPY);
    GdiFlush();
    let raw = std::slice::from_raw_parts(bits as *const u8, (bw * bh * 4) as usize).to_vec();
    SelectObject(dc, old); DeleteObject(bitmap); DeleteDC(dc); ReleaseDC(std::ptr::null_mut(), screen);
    if ok == 0 { return Vec::new(); }
    let Some(img) = image::RgbaImage::from_raw(bw as u32, bh as u32, raw) else { return Vec::new() };
    let mut pixels = image::imageops::blur(&img, 7.0).into_raw();
    for p in pixels.chunks_exact_mut(4) { for v in &mut p[..3] { *v = (*v as f32 * 0.30 + 242.0 * 0.70) as u8; } p[3] = 255; }
    pixels
}

unsafe fn font(size: i32, weight: u32) -> HFONT {
    CreateFontW(-size, 0, 0, 0, weight as i32, 0, 0, 0, DEFAULT_CHARSET as u32, 0, 0, CLEARTYPE_QUALITY as u32, DEFAULT_PITCH as u32, wide("Segoe UI Variable").as_ptr())
}
unsafe fn label(dc: HDC, text: &str, mut rect: RECT, font: HFONT, color: u32) {
    let old = SelectObject(dc, font); SetTextColor(dc, color); SetBkMode(dc, TRANSPARENT as i32);
    let text = wide(text); DrawTextW(dc, text.as_ptr(), -1, &mut rect, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX);
    SelectObject(dc, old);
}

unsafe fn paint(hwnd: HWND) {
    // Clone the visible model; no nested borrow even when BeginPaint triggers messages.
    let model = STATE.with(|c| c.borrow().as_ref().map(|s| (
        s.width, s.height, s.scale, s.glass.clone(), s.query.clone(),
        s.matches.iter().skip(s.first).take(PAGE).filter_map(|&i| s.all.get(i).cloned()).collect::<Vec<_>>(),
        s.selected.saturating_sub(s.first), s.matches.len(), s.error.clone(), s.select_all,
    )));
    let mut ps: PAINTSTRUCT = std::mem::zeroed(); let target = BeginPaint(hwnd, &mut ps);
    if let Some((w, h, scale, glass, query, rows, selected, count, error, select_all)) = model {
        let px = |v: i32| (v as f32 * scale).round() as i32;
        let dc = CreateCompatibleDC(target); let bitmap = CreateCompatibleBitmap(target, w, h); let old = SelectObject(dc, bitmap);
        let bg = CreateSolidBrush(0x00F0EFED); let full = RECT { left: 0, top: 0, right: w, bottom: h }; FillRect(dc, &full, bg); DeleteObject(bg);
        if !glass.is_empty() {
            let mut info: BITMAPINFO = std::mem::zeroed(); info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = (w / 4).max(1); info.bmiHeader.biHeight = -(h / 4).max(1); info.bmiHeader.biPlanes = 1; info.bmiHeader.biBitCount = 32;
            SetStretchBltMode(dc, HALFTONE);
            StretchDIBits(dc, 0, 0, w, h, 0, 0, (w / 4).max(1), (h / 4).max(1), glass.as_ptr() as *const _, &info, DIB_RGB_COLORS, SRCCOPY);
        }
        let body = font(px(16), 400); let small = font(px(12), 400); let search = font(px(25), 400);
        let border = CreatePen(PS_SOLID, px(1).max(1), 0x00FAFAFA); let previous_pen = SelectObject(dc, border); let previous_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        RoundRect(dc, 0, 0, w, h, px(28), px(28)); SelectObject(dc, previous_pen); SelectObject(dc, previous_brush); DeleteObject(border);
        // Search magnifier.
        let pen = CreatePen(PS_SOLID, px(2), 0x00847C76); let oldpen = SelectObject(dc, pen); let oldbrush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        Ellipse(dc, px(22), px(25), px(40), px(43)); MoveToEx(dc, px(37), px(40), std::ptr::null_mut()); LineTo(dc, px(45), px(48));
        SelectObject(dc, oldpen); SelectObject(dc, oldbrush); DeleteObject(pen);
        let text = wide(&query); let mut extent: SIZE = std::mem::zeroed(); let old_font = SelectObject(dc, search);
        GetTextExtentPoint32W(dc, text.as_ptr(), text.len().saturating_sub(1) as i32, &mut extent); SelectObject(dc, old_font);
        if select_all && !query.is_empty() { let brush = CreateSolidBrush(0x00ECD5BC); FillRect(dc, &RECT { left: px(59), top: px(23), right: (px(62) + extent.cx).min(w - px(65)), bottom: px(53) }, brush); DeleteObject(brush); }
        label(dc, if query.is_empty() { "Spotlight Search" } else { &query }, RECT { left: px(60), top: px(14), right: w - px(64), bottom: px(61) }, search, if query.is_empty() { 0x008E8883 } else { 0x002B2724 });
        SetCaretPos((px(60) + extent.cx).min(w - px(65)), px(25));
        label(dc, "esc", RECT { left: w - px(43), top: px(24), right: w - px(14), bottom: px(48) }, small, 0x00918882);
        let line = CreatePen(PS_SOLID, 1, 0x00CFCAC5); let oldpen = SelectObject(dc, line); MoveToEx(dc, px(18), px(68), std::ptr::null_mut()); LineTo(dc, w - px(18), px(68)); SelectObject(dc, oldpen); DeleteObject(line);
        for (i, entry) in rows.iter().enumerate() {
            let y = px(TOP + i as i32 * ROW);
            if i == selected {
                let brush = CreateSolidBrush(0x00CEC9C2); let pen = CreatePen(PS_SOLID, 1, 0x00CEC9C2);
                let ob = SelectObject(dc, brush); let op = SelectObject(dc, pen); RoundRect(dc, px(9), y, w - px(9), y + px(ROW - 3), px(16), px(16)); SelectObject(dc, ob); SelectObject(dc, op); DeleteObject(brush); DeleteObject(pen);
            }
            let mut fi: SHFILEINFOW = std::mem::zeroed();
            let path = wide(&entry.path.to_string_lossy());
            let got = SHGetFileInfoW(path.as_ptr(), 0, &mut fi, std::mem::size_of::<SHFILEINFOW>() as u32, SHGFI_ICON | SHGFI_SMALLICON);
            if got != 0 && !fi.hIcon.is_null() { DrawIconEx(dc, px(21), y + px(11), fi.hIcon, px(28), px(28), 0, std::ptr::null_mut(), DI_NORMAL); DestroyIcon(fi.hIcon); }
            else { label(dc, if entry.kind == "Application" { "▣" } else { "▤" }, RECT { left: px(23), top: y, right: px(52), bottom: y + px(50) }, search, 0x00907859); }
            label(dc, &entry.name, RECT { left: px(64), top: y + px(3), right: w - px(77), bottom: y + px(27) }, body, 0x002B2724);
            label(dc, &format!("{} · {}", entry.kind, entry.detail), RECT { left: px(64), top: y + px(27), right: w - px(24), bottom: y + px(46) }, small, 0x00827970);
            if i == selected { label(dc, "↵", RECT { left: w - px(49), top: y + px(3), right: w - px(22), bottom: y + px(28) }, body, 0x00776D64); }
        }
        if rows.is_empty() { label(dc, "No results", RECT { left: px(28), top: px(105), right: w - px(24), bottom: px(147) }, body, 0x00827970); }
        label(dc, if error.is_empty() { "↑ ↓  Select       ↵  Open       Esc  Close" } else { &error }, RECT { left: px(22), top: h - px(36), right: w - px(80), bottom: h - px(10) }, small, 0x00827970);
        label(dc, &format!("{count}"), RECT { left: w - px(58), top: h - px(36), right: w - px(15), bottom: h - px(10) }, small, 0x00827970);
        BitBlt(target, 0, 0, w, h, dc, 0, 0, SRCCOPY);
        SelectObject(dc, old); DeleteObject(bitmap); DeleteDC(dc); DeleteObject(body); DeleteObject(small); DeleteObject(search);
    }
    EndPaint(hwnd, &ps);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => { paint(hwnd); 0 }
        WM_SETFOCUS => { CreateCaret(hwnd, std::ptr::null_mut(), 1, 25); SetCaretPos(60, 25); ShowCaret(hwnd); 0 }
        WM_KILLFOCUS => { HideCaret(hwnd); DestroyCaret(); 0 }
        WM_ACTIVATE if w & 0xffff == WA_INACTIVE as usize => { ShowWindow(hwnd, SW_HIDE); 0 }
        WM_CLOSE => { ShowWindow(hwnd, SW_HIDE); 0 }
        REFRESH => { refresh(); 0 }
        OPEN_SELECTED => { launch_selected(); 0 }
        WM_KEYDOWN => {
            if GetKeyState(VK_CONTROL as i32) < 0 {
                match w as u32 {
                    0x41 => { STATE.with(|c| if let Some(s) = c.borrow_mut().as_mut() { s.select_all = true; }); InvalidateRect(hwnd, std::ptr::null(), 0); }
                    0x56 => {
                        if OpenClipboard(hwnd) != 0 {
                            let data = GetClipboardData(13); // CF_UNICODETEXT
                            if !data.is_null() {
                                let ptr = GlobalLock(data) as *const u16;
                                if !ptr.is_null() {
                                    let cap = (GlobalSize(data) / 2).min(512); let slice = std::slice::from_raw_parts(ptr, cap);
                                    let n = slice.iter().position(|&ch| ch == 0).unwrap_or(cap);
                                    let text: String = String::from_utf16_lossy(&slice[..n]).chars().filter(|ch| !ch.is_control()).collect();
                                    STATE.with(|c| if let Some(s) = c.borrow_mut().as_mut() { if s.select_all { s.query.clear(); } s.select_all = false; s.query.extend(text.chars().take(128)); });
                                    GlobalUnlock(data);
                                }
                            }
                            CloseClipboard(); PostMessageW(hwnd, REFRESH, 0, 0);
                        }
                    }
                    _ => {}
                }
                return 0;
            }
            match w as u16 {
                VK_ESCAPE => { ShowWindow(hwnd, SW_HIDE); }
                VK_RETURN => { PostMessageW(hwnd, OPEN_SELECTED, 0, 0); }
                VK_DOWN => move_selection(1), VK_UP => move_selection(-1),
                VK_NEXT => move_selection(PAGE as i32), VK_PRIOR => move_selection(-(PAGE as i32)),
                VK_BACK => {
                    STATE.with(|c| if let Some(s) = c.borrow_mut().as_mut() { if s.select_all { s.query.clear(); } else { s.query.pop(); } s.select_all = false; });
                    PostMessageW(hwnd, REFRESH, 0, 0);
                }
                _ => {}
            } 0
        }
        WM_CHAR => {
            if let Some(ch) = char::from_u32(w as u32).filter(|c| !c.is_control()) {
                STATE.with(|c| if let Some(s) = c.borrow_mut().as_mut() { if s.select_all { s.query.clear(); } s.select_all = false; if s.query.len() < 512 { s.query.push(ch); } });
                PostMessageW(hwnd, REFRESH, 0, 0);
            } 0
        }
        WM_MOUSEWHEEL => { move_selection(if (w >> 16) as i16 > 0 { -1 } else { 1 }); 0 }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            let y = (l >> 16) as i16 as i32;
            let clicked = STATE.with(|c| {
                let mut guard = c.borrow_mut(); let Some(s) = guard.as_mut() else { return false; };
                let row = ((y as f32 / s.scale) as i32 - TOP) / ROW;
                if y >= (TOP as f32 * s.scale) as i32 && row >= 0 && row < PAGE as i32 && s.first + (row as usize) < s.matches.len() {
                    s.selected = s.first + row as usize; true
                } else { false }
            });
            if clicked { InvalidateRect(hwnd, std::ptr::null(), 0); if msg == WM_LBUTTONDBLCLK { PostMessageW(hwnd, OPEN_SELECTED, 0, 0); } }
            0
        }
        WM_SETCURSOR => { SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_ARROW)); 1 }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

pub fn toggle(_owner: HWND) {
    unsafe {
        if let Some((hwnd, _, _, _)) = handles() {
            if IsWindowVisible(hwnd) != 0 { ShowWindow(hwnd, SW_HIDE); return; }
        } else {
            let instance = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
            let cls = wide(CLASS); let mut wc: WNDCLASSEXW = std::mem::zeroed();
            wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32; wc.lpfnWndProc = Some(proc); wc.hInstance = instance;
            wc.style = CS_DBLCLKS | CS_DROPSHADOW; wc.lpszClassName = cls.as_ptr(); wc.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
            RegisterClassExW(&wc);
            let hwnd = CreateWindowExW(WS_EX_TOOLWINDOW, cls.as_ptr(), wide("Deskland Spotlight").as_ptr(), WS_POPUP, 0, 0, 660, 444, std::ptr::null_mut(), std::ptr::null_mut(), instance, std::ptr::null_mut());
            if hwnd.is_null() { crate::log!("Launcher: CreateWindowEx failed {}", GetLastError()); return; }
            let all = entries();
            STATE.with(|c| *c.borrow_mut() = Some(State { hwnd, all, matches: Vec::new(), query: String::new(), selected: 0, first: 0, scale: 1.0, width: 660, height: 444, glass: Vec::new(), error: String::new(), select_all: false }));
        }
        let Some((hwnd, _, _, _)) = handles() else { return; };
        let mut mouse: POINT = std::mem::zeroed(); GetCursorPos(&mut mouse);
        let monitor = MonitorFromPoint(mouse, MONITOR_DEFAULTTONEAREST); let mut mi: MONITORINFO = std::mem::zeroed(); mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32; GetMonitorInfoW(monitor, &mut mi);
        let scale = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
        let width = (660.0 * scale) as i32; let height = (444.0 * scale) as i32;
        let x = mi.rcWork.left + (mi.rcWork.right - mi.rcWork.left - width) / 2;
        let y = mi.rcWork.top + ((mi.rcWork.bottom - mi.rcWork.top - height) / 3).max(0);
        let glass = capture_glass(x, y, width, height);
        STATE.with(|c| if let Some(s) = c.borrow_mut().as_mut() { s.scale = scale; s.width = width; s.height = height; s.glass = glass; s.query.clear(); s.select_all = false; });
        refresh();
        SetWindowPos(hwnd, HWND_TOP, x, y, width, height, SWP_NOACTIVATE);
        SetWindowRgn(hwnd, CreateRoundRectRgn(0, 0, width + 1, height + 1, (28.0 * scale) as i32, (28.0 * scale) as i32), 1);
        ShowWindow(hwnd, SW_SHOW); SetForegroundWindow(hwnd); SetFocus(hwnd);
    }
}

pub fn hotkey_text(mods: u32, vk: u32) -> String {
    let m = if mods & MOD_CONTROL != 0 { "Ctrl+" } else if mods & MOD_SHIFT != 0 { "Shift+" } else { "Alt+" };
    let k = if vk == VK_SPACE as u32 { "Space".to_string() } else if (b'A' as u32..=b'Z' as u32).contains(&vk) { char::from_u32(vk).unwrap().to_string() } else { format!("F{}", vk.saturating_sub(0x6F)) };
    format!("{m}{k}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_search_enter_and_reopen_do_not_reenter_state() {
        unsafe {
            toggle(std::ptr::null_mut());
            let (hwnd, _, _, _) = handles().expect("native launcher window");
            for _ in 0..8 {
                for ch in "paint".chars() { SendMessageW(hwnd, WM_CHAR, ch as usize, 0); SendMessageW(hwnd, REFRESH, 0, 0); UpdateWindow(hwnd); }
                STATE.with(|c| { let g = c.borrow(); let s = g.as_ref().unwrap(); assert_eq!(s.all[s.matches[0]].name, "Paint"); assert_eq!(s.selected, 0); });
                SendMessageW(hwnd, WM_KEYDOWN, VK_RETURN as usize, 0);
                let mut msg: MSG = std::mem::zeroed();
                while PeekMessageW(&mut msg, hwnd, 0, 0, PM_REMOVE) != 0 { DispatchMessageW(&msg); }
                assert_eq!(IsWindowVisible(hwnd), 0);
                toggle(std::ptr::null_mut()); assert_ne!(IsWindowVisible(hwnd), 0);
                SendMessageW(hwnd, WM_KEYDOWN, VK_DOWN as usize, 0); SendMessageW(hwnd, WM_PAINT, 0, 0);
                SendMessageW(hwnd, WM_KEYDOWN, VK_ESCAPE as usize, 0); toggle(std::ptr::null_mut());
            }
            DestroyWindow(hwnd); STATE.with(|c| *c.borrow_mut() = None);
        }
    }
}
