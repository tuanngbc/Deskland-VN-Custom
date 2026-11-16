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

#[derive(Clone, Debug, PartialEq)]
pub struct Appearance { pub theme: String, pub transparency: f32, pub blur: f32 }
impl Default for Appearance {
    fn default() -> Self { Self { theme: "light".into(), transparency: 0.55, blur: 20.0 } }
}
impl Appearance {
    pub fn from_json(v: &serde_json::Value) -> Self {
        let d = Self::default();
        Self {
            theme: v.get("theme").and_then(|v| v.as_str()).filter(|v| matches!(*v, "light" | "dark")).unwrap_or(&d.theme).to_owned(),
            transparency: v.get("transparency").and_then(|v| v.as_f64()).map(|v| v as f32).filter(|v| v.is_finite()).unwrap_or(d.transparency).clamp(0.0, 0.85),
            blur: v.get("blur").and_then(|v| v.as_f64()).map(|v| v as f32).filter(|v| v.is_finite()).unwrap_or(d.blur).clamp(4.0, 40.0),
        }
    }
    pub fn json(&self) -> serde_json::Value { serde_json::json!({"theme": self.theme, "transparency": self.transparency, "blur": self.blur}) }
}

pub fn configure(appearance: Appearance) {
    PREFS.with(|c| *c.borrow_mut() = appearance.clone());
    let backdrop = STATE.with(|c| c.borrow().as_ref().map(|s| (s.hwnd, s.width, s.height, s.scale, s.backdrop.clone())));
    if let Some((hwnd, w, h, scale, backdrop)) = backdrop {
        let glass = frost(&backdrop, w, h, scale, &appearance);
        STATE.with(|c| if let Some(s) = c.borrow_mut().as_mut() { s.glass = glass; });
        unsafe { InvalidateRect(hwnd, std::ptr::null(), 0); }
    }
}

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
    glass: Vec<u8>, backdrop: Vec<u8>, error: String, select_all: bool,
    icons: std::collections::HashMap<(PathBuf, i32), Option<IconPixels>>,
    icon_requests: std::sync::mpsc::Sender<(PathBuf, i32)>,
    icon_results: std::sync::mpsc::Receiver<((PathBuf, i32), Option<IconPixels>)>,
    pending_icons: usize,
}
#[derive(Clone)]
struct IconPixels { width: i32, height: i32, pixels: Vec<u8> }
thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    static PREFS: RefCell<Appearance> = RefCell::new(Appearance::default());
}
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
        let root = PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into()));
        let system = if command == "explorer.exe" { root.join(command) } else { root.join("System32").join(command) };
        let path = if system.is_file() { system } else { command.into() };
        out.push(Entry { name: name.into(), path, kind: "Application", detail: format!("Windows · {command}") });
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

// Extract large shell icons on one worker; shell extensions never block typing.
fn icon_worker() -> (std::sync::mpsc::Sender<(PathBuf, i32)>, std::sync::mpsc::Receiver<((PathBuf, i32), Option<IconPixels>)>) {
    let (tx, work) = std::sync::mpsc::channel::<(PathBuf, i32)>();
    let (done, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || unsafe {
        use windows::Win32::System::Com::*;
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        while let Ok(key) = work.recv() {
            let icon = extract_icon(&key.0, key.1);
            if done.send((key, icon)).is_err() { break; }
        }
        if com.is_ok() { CoUninitialize(); }
    });
    (tx, rx)
}

unsafe fn extract_icon(path: &Path, size: i32) -> Option<IconPixels> {
    use windows::Win32::UI::Shell::{SHCreateItemFromParsingName, IShellItemImageFactory, SIIGBF_ICONONLY, SIIGBF_BIGGERSIZEOK};
    let path = wide(&path.to_string_lossy());
    let factory: IShellItemImageFactory = SHCreateItemFromParsingName(windows::core::PCWSTR(path.as_ptr()), None).ok()?;
    let source_size = (size * 2).clamp(64, 256);
    let bitmap = factory.GetImage(windows::Win32::Foundation::SIZE { cx: source_size, cy: source_size }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
    let bitmap = bitmap.0;
    let mut object: BITMAP = std::mem::zeroed();
    if GetObjectW(bitmap, std::mem::size_of::<BITMAP>() as i32, &mut object as *mut _ as *mut _) == 0 { DeleteObject(bitmap); return None; }
    let (w, h) = (object.bmWidth, object.bmHeight.abs());
    if w <= 0 || h <= 0 || w > 512 || h > 512 { DeleteObject(bitmap); return None; }
    let dc = CreateCompatibleDC(std::ptr::null_mut());
    let mut info: BITMAPINFO = std::mem::zeroed(); info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = w; info.bmiHeader.biHeight = -h; info.bmiHeader.biPlanes = 1; info.bmiHeader.biBitCount = 32;
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    let ok = GetDIBits(dc, bitmap, 0, h as u32, pixels.as_mut_ptr() as *mut _, &mut info, DIB_RGB_COLORS);
    DeleteDC(dc); DeleteObject(bitmap);
    if ok == 0 { return None; }
    if pixels.chunks_exact(4).all(|p| p[3]==0) { for p in pixels.chunks_exact_mut(4) {p[3]=255;} }
    let image = image::RgbaImage::from_raw(w as u32, h as u32, pixels)?;
    let pixels = image::imageops::resize(&image, size as u32, size as u32, image::imageops::FilterType::Lanczos3).into_raw();
    Some(IconPixels { width: size, height: size, pixels })
}

fn icon_for(hwnd: HWND, path: &Path, size: i32) -> Option<IconPixels> {
    let (icon, pending) = STATE.with(|c| {
        let mut g = c.borrow_mut(); let s = g.as_mut()?;
        let key = (path.to_path_buf(), size);
        if !s.icons.contains_key(&key) {
            s.icons.insert(key.clone(), None);
            let _ = s.icon_requests.send(key);
            s.pending_icons += 1;
            return Some((None, true));
        }
        Some((s.icons.get(&key).cloned().flatten(), false))
    }).unwrap_or((None, false));
    if pending { unsafe { SetTimer(hwnd, 7, 60, None); } }
    icon
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
    raw
}

fn frost(raw: &[u8], width: i32, height: i32, scale: f32, appearance: &Appearance) -> Vec<u8> {
    let bw = (width / 4).max(1) as u32; let bh = (height / 4).max(1) as u32;
    let Some(img) = image::RgbaImage::from_raw(bw, bh, raw.to_vec()) else { return Vec::new() };
    let mut pixels = image::imageops::blur(&img, appearance.blur * scale / 4.0).into_raw();
    let dark = appearance.theme == "dark";
    for (i, p) in pixels.chunks_exact_mut(4).enumerate() {
        let y = i as f32 / bw as f32 / bh as f32;
        let base = if dark { 24.0 } else { 247.0 };
        let reflection = (1.0 - y).powf(5.0) * if dark { 15.0 } else { 8.0 };
        for v in &mut p[..3] { *v = (*v as f32 * appearance.transparency + base * (1.0 - appearance.transparency) + reflection).clamp(0.0, 255.0) as u8; }
        p[3] = 255;
    }
    pixels
}

unsafe fn font(size: i32, weight: u32) -> HFONT {
    CreateFontW(-size, 0, 0, 0, weight as i32, 0, 0, 0, DEFAULT_CHARSET as u32, 0, 0, ANTIALIASED_QUALITY as u32, DEFAULT_PITCH as u32, wide("Segoe UI Variable").as_ptr())
}
unsafe fn label(dc: HDC, text: &str, mut rect: RECT, font: HFONT, color: u32) {
    let old = SelectObject(dc, font); SetTextColor(dc, color); SetBkMode(dc, TRANSPARENT as i32);
    let text = wide(text); DrawTextW(dc, text.as_ptr(), -1, &mut rect, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX);
    SelectObject(dc, old);
}

// Premultiplied pixel composition keeps text opaque and gives rounded edges
// fractional alpha. No region clipping or global Windows theme changes.
fn panel_coverage(x: f32, y: f32, w: f32, h: f32, radius: f32) -> (f32, f32) {
    let qx = (x - w / 2.0).abs() - (w / 2.0 - radius);
    let qy = (y - h / 2.0).abs() - (h / 2.0 - radius);
    let distance = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
    ((0.5 - distance).clamp(0.0, 1.0), distance)
}
fn round_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let mut p = tiny_skia::PathBuilder::new(); let k = 0.5522848 * r;
    p.move_to(x+r,y); p.line_to(x+w-r,y); p.cubic_to(x+w-r+k,y,x+w,y+r-k,x+w,y+r);
    p.line_to(x+w,y+h-r); p.cubic_to(x+w,y+h-r+k,x+w-r+k,y+h,x+w-r,y+h);
    p.line_to(x+r,y+h); p.cubic_to(x+r-k,y+h,x,y+h-r+k,x,y+h-r);
    p.line_to(x,y+r); p.cubic_to(x,y+r-k,x+r-k,y,x+r,y); p.close(); p.finish().unwrap()
}
fn blend_pixels(dst: &mut [u8], stride: i32, x: i32, y: i32, src: &[u8], w: i32, h: i32, rgba: bool) {
    let rows = dst.len() / (stride as usize * 4);
    for sy in 0..h { for sx in 0..w {
        let (dx, dy) = (x+sx, y+sy);
        if dx < 0 || dx >= stride || dy < 0 || dy as usize >= rows { continue; }
        let si = ((sy*w+sx)*4) as usize; let di = ((dy*stride+dx)*4) as usize;
        let a = src[si+3] as u32;
        for c in 0..3 { let sc = if rgba { 2-c } else { c }; dst[di+c] = (src[si+sc] as u32 + (dst[di+c] as u32 * (255-a) + 127) / 255).min(255) as u8; }
    } }
}

unsafe fn paint(hwnd: HWND) {
    let model = STATE.with(|c| c.borrow().as_ref().map(|s| (
        s.width, s.height, s.scale, s.glass.clone(), s.query.clone(),
        s.matches.iter().skip(s.first).take(PAGE).filter_map(|&i| s.all.get(i).cloned()).collect::<Vec<_>>(),
        s.selected.saturating_sub(s.first), s.matches.len(), s.error.clone(), s.select_all,
    )));
    let appearance = PREFS.with(|c| c.borrow().clone());
    let dark = appearance.theme == "dark";
    let mut ps: PAINTSTRUCT = std::mem::zeroed(); let target = BeginPaint(hwnd, &mut ps);
    if let Some((w, h, scale, glass, query, rows, selected, count, error, select_all)) = model {
        let px = |v: i32| (v as f32 * scale).round() as i32;
        let pad = px(12); let ww = w+2*pad; let hh = h+2*pad;
        let mut info: BITMAPINFO = std::mem::zeroed(); info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = ww; info.bmiHeader.biHeight = -hh; info.bmiHeader.biPlanes = 1; info.bmiHeader.biBitCount = 32;
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(target, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        let dc = CreateCompatibleDC(target);
        if bitmap.is_null() || dc.is_null() || bits.is_null() {
            if !bitmap.is_null() { DeleteObject(bitmap); } if !dc.is_null() { DeleteDC(dc); } EndPaint(hwnd, &ps); return;
        }
        std::ptr::write_bytes(bits as *mut u8, 0, (ww*hh*4) as usize);
        let old = SelectObject(dc, bitmap); SetViewportOrgEx(dc, pad, pad, std::ptr::null_mut());
        let bg = CreateSolidBrush(if dark { 0x00201D1B } else { 0x00F8F7F5 }); FillRect(dc, &RECT {left:0,top:0,right:w,bottom:h}, bg); DeleteObject(bg);
        if !glass.is_empty() {
            let mut background = info; background.bmiHeader.biWidth = (w/4).max(1); background.bmiHeader.biHeight = -(h/4).max(1);
            SetStretchBltMode(dc, HALFTONE);
            StretchDIBits(dc, 0, 0, w, h, 0, 0, (w/4).max(1), (h/4).max(1), glass.as_ptr() as *const _, &background, DIB_RGB_COLORS, SRCCOPY);
        }
        GdiFlush();
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, (ww*hh*4) as usize);
        if let Some(mut overlay) = tiny_skia::Pixmap::new(w as u32, h as u32) {
            let mut color = tiny_skia::Paint::default(); color.anti_alias = true;
            color.set_color_rgba8(255,255,255,if dark {65} else {155});
            overlay.stroke_path(&round_path(0.75,0.75,w as f32-1.5,h as f32-1.5,20.0*scale), &color, &tiny_skia::Stroke {width:scale.max(1.0),..Default::default()}, tiny_skia::Transform::identity(), None);
            if !rows.is_empty() {
                color.set_color_rgba8(if dark {255} else {30}, if dark {255} else {45}, if dark {255} else {60}, if dark {28} else {24});
                overlay.fill_path(&round_path(px(9) as f32,px(TOP+selected as i32*ROW) as f32,(w-px(18)) as f32,px(ROW-3) as f32,9.0*scale), &color, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
            }
            color.set_color_rgba8(if dark {235} else {65}, if dark {235} else {70}, if dark {240} else {78}, 220);
            let mut p = tiny_skia::PathBuilder::new(); p.push_circle(31.0*scale,34.0*scale,8.5*scale); p.move_to(37.2*scale,40.2*scale); p.line_to(44.0*scale,47.0*scale);
            if let Some(path) = p.finish() { overlay.stroke_path(&path, &color, &tiny_skia::Stroke {width:1.8*scale,line_cap:tiny_skia::LineCap::Round,..Default::default()}, tiny_skia::Transform::identity(), None); }
            color.set_color_rgba8(if dark {255} else {25},if dark {255} else {30},if dark {255} else {40},if dark {40} else {30});
            if let Some(rect) = tiny_skia::Rect::from_xywh(px(18) as f32,px(68) as f32,(w-px(36)) as f32,scale.max(1.0)) { overlay.fill_rect(rect,&color,tiny_skia::Transform::identity(),None); }
            blend_pixels(pixels,ww,pad,pad,overlay.data(),w,h,true);
        }
        let body = font(px(16),400); let small = font(px(12),400); let search = font(px(25),400);
        let primary = if dark {0x00FAF8F6} else {0x002B2724}; let secondary = if dark {0x00C8C2BC} else {0x00665D54};
        let text = wide(&query); let mut extent: SIZE = std::mem::zeroed(); let oldfont = SelectObject(dc,search);
        GetTextExtentPoint32W(dc,text.as_ptr(),text.len().saturating_sub(1) as i32,&mut extent); SelectObject(dc,oldfont);
        if select_all && !query.is_empty() { let brush=CreateSolidBrush(if dark {0x008B593A} else {0x00ECD5BC}); FillRect(dc,&RECT {left:px(59),top:px(23),right:(px(62)+extent.cx).min(w-px(65)),bottom:px(53)},brush); DeleteObject(brush); }
        label(dc,if query.is_empty(){"Spotlight Search"}else{&query},RECT {left:px(60),top:px(14),right:w-px(64),bottom:px(61)},search,if query.is_empty(){secondary}else{primary});
        if GetForegroundWindow() == hwnd { let caret=CreateSolidBrush(primary); let x=(px(60)+extent.cx).min(w-px(65)); FillRect(dc,&RECT{left:x,top:px(25),right:x+px(1).max(1),bottom:px(51)},caret); DeleteObject(caret); }
        label(dc,"esc",RECT {left:w-px(43),top:px(24),right:w-px(14),bottom:px(48)},small,secondary);
        let mut icons = Vec::new();
        for (i,entry) in rows.iter().enumerate() {
            let y=px(TOP+i as i32*ROW);
            if let Some(icon) = icon_for(hwnd,&entry.path,px(36)) { icons.push((icon,px(18),y+px(7))); }
            else { label(dc,if entry.kind=="Application"{"▣"}else{"▤"},RECT{left:px(23),top:y,right:px(52),bottom:y+px(50)},search,secondary); }
            label(dc,&entry.name,RECT {left:px(66),top:y+px(3),right:w-px(77),bottom:y+px(27)},body,primary);
            label(dc,&format!("{} · {}",entry.kind,entry.detail),RECT {left:px(66),top:y+px(27),right:w-px(24),bottom:y+px(46)},small,secondary);
            if i==selected { label(dc,"↵",RECT {left:w-px(49),top:y+px(3),right:w-px(22),bottom:y+px(28)},body,secondary); }
        }
        if rows.is_empty(){label(dc,"No results",RECT{left:px(28),top:px(105),right:w-px(24),bottom:px(147)},body,secondary);}
        label(dc,if error.is_empty(){"↑ ↓  Select       ↵  Open       Esc  Close"}else{&error},RECT{left:px(22),top:h-px(36),right:w-px(80),bottom:h-px(10)},small,secondary);
        label(dc,&format!("{count}"),RECT{left:w-px(58),top:h-px(36),right:w-px(15),bottom:h-px(10)},small,secondary);
        GdiFlush();
        for (icon,x,y) in icons { blend_pixels(pixels,ww,pad+x,pad+y,&icon.pixels,icon.width,icon.height,false); }
        for y in 0..hh { for x in 0..ww {
            let (coverage,distance)=panel_coverage((x-pad) as f32+0.5,(y-pad) as f32+0.5,w as f32,h as f32,20.0*scale);
            let i=((y*ww+x)*4) as usize;
            let shadow=(-distance.max(0.0)/(3.5*scale)).exp()*0.22;
            for c in 0..3 { pixels[i+c]=(pixels[i+c] as f32*coverage) as u8; }
            pixels[i+3]=((coverage+(1.0-coverage)*shadow)*255.0) as u8;
        } }
        SetViewportOrgEx(dc,0,0,std::ptr::null_mut());
        let size=SIZE {cx:ww,cy:hh}; let origin=POINT{x:0,y:0};
        let blend=BLENDFUNCTION {BlendOp:AC_SRC_OVER as u8,BlendFlags:0,SourceConstantAlpha:255,AlphaFormat:AC_SRC_ALPHA as u8};
        if UpdateLayeredWindow(hwnd,std::ptr::null_mut(),std::ptr::null(),&size,dc,&origin,0,&blend,ULW_ALPHA)==0 { crate::log!("Launcher: UpdateLayeredWindow failed {}",GetLastError()); }
        SelectObject(dc,old); DeleteObject(bitmap); DeleteDC(dc); DeleteObject(body); DeleteObject(small); DeleteObject(search);
    }
    EndPaint(hwnd,&ps);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => { paint(hwnd); 0 }
        WM_SETFOCUS | WM_KILLFOCUS => { InvalidateRect(hwnd, std::ptr::null(), 0); 0 }
        WM_TIMER if w == 7 => {
            let (changed, pending) = STATE.with(|c| {
                let mut g=c.borrow_mut(); let Some(s)=g.as_mut() else {return (false,false);}; let mut changed=false;
                while let Ok((key,icon))=s.icon_results.try_recv() { s.icons.insert(key,icon); s.pending_icons=s.pending_icons.saturating_sub(1); changed=true; }
                (changed,s.pending_icons>0)
            });
            if changed { InvalidateRect(hwnd,std::ptr::null(),0); }
            if !pending || IsWindowVisible(hwnd)==0 { KillTimer(hwnd,7); } 0
        }
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
                let logical_y = (y as f32 / s.scale) as i32 - 12;
                let row = (logical_y - TOP) / ROW;
                if logical_y >= TOP && row >= 0 && row < PAGE as i32 && s.first + (row as usize) < s.matches.len() {
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
            wc.style = CS_DBLCLKS; wc.lpszClassName = cls.as_ptr(); wc.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
            RegisterClassExW(&wc);
            // An independent top-level window receives focus and keyboard input
            // without inheriting the hidden message window's visibility/owner chain.
            let hwnd = CreateWindowExW(WS_EX_LAYERED, cls.as_ptr(), wide("Deskland Spotlight").as_ptr(), WS_POPUP, 0, 0, 684, 468, std::ptr::null_mut(), std::ptr::null_mut(), instance, std::ptr::null_mut());
            if hwnd.is_null() { crate::log!("Launcher: CreateWindowEx failed {}", GetLastError()); return; }
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
            let all = entries();
            let (icon_requests,icon_results)=icon_worker();
            STATE.with(|c| *c.borrow_mut() = Some(State { hwnd, all, matches: Vec::new(), query: String::new(), selected: 0, first: 0, scale: 1.0, width: 660, height: 444, glass: Vec::new(), backdrop: Vec::new(), error: String::new(), select_all: false, icons:std::collections::HashMap::new(),icon_requests,icon_results,pending_icons:0 }));
        }
        let Some((hwnd, _, _, _)) = handles() else { return; };
        let mut mouse: POINT = std::mem::zeroed(); GetCursorPos(&mut mouse);
        let monitor = MonitorFromPoint(mouse, MONITOR_DEFAULTTONEAREST); let mut mi: MONITORINFO = std::mem::zeroed(); mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32; GetMonitorInfoW(monitor, &mut mi);
        let scale = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
        let width = (660.0 * scale) as i32; let height = (444.0 * scale) as i32;
        let pad=(12.0*scale).round() as i32;
        let x = mi.rcWork.left + (mi.rcWork.right - mi.rcWork.left - width-2*pad) / 2;
        let y = mi.rcWork.top + ((mi.rcWork.bottom - mi.rcWork.top - height-2*pad) / 3).max(0);
        let backdrop = capture_glass(x+pad, y+pad, width, height);
        let appearance=PREFS.with(|c| c.borrow().clone());
        let glass=frost(&backdrop,width,height,scale,&appearance);
        STATE.with(|c| if let Some(s) = c.borrow_mut().as_mut() { s.scale = scale; s.width = width; s.height = height; s.glass = glass; s.backdrop=backdrop; s.query.clear(); s.select_all = false; });
        refresh();
        SetWindowPos(hwnd, HWND_TOP, x, y, width+2*pad, height+2*pad, SWP_NOACTIVATE);
        paint(hwnd);
        SetTimer(hwnd,7,60,None);
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
    fn appearance_roundtrip_clamps_and_preserves_defaults() {
        let d=Appearance::from_json(&serde_json::Value::Null); assert_eq!(d,Appearance::default());
        let a=Appearance::from_json(&serde_json::json!({"theme":"dark","transparency":0.8,"blur":8}));
        assert_eq!(Appearance::from_json(&a.json()),a);
        let a=Appearance::from_json(&serde_json::json!({"theme":"system-wide","transparency":99,"blur":-1}));
        assert_eq!(a.theme,"light"); assert_eq!(a.transparency,0.85); assert_eq!(a.blur,4.0);
    }
    #[test]
    fn rounded_panel_has_fractional_edge_coverage() {
        assert_eq!(panel_coverage(0.5,0.5,660.0,444.0,20.0).0,0.0);
        assert_eq!(panel_coverage(330.0,222.0,660.0,444.0,20.0).0,1.0);
        assert!((0..40).any(|x| (0..40).any(|y| { let a=panel_coverage(x as f32+0.5,y as f32+0.5,660.0,444.0,20.0).0; a>0.0 && a<1.0 })));
    }
    #[test]
    fn glass_theme_and_transparency_affect_background_only() {
        let raw=vec![90;16*12*4];
        let mut a=Appearance::default(); a.transparency=0.0;
        let light=frost(&raw,64,48,1.0,&a);
        a.theme="dark".into(); let dark=frost(&raw,64,48,1.0,&a);
        a.transparency=0.8; let clear=frost(&raw,64,48,1.0,&a);
        assert!(light[0]>dark[0]); assert!(clear[0]>dark[0]);
        assert!(light.chunks_exact(4).chain(dark.chunks_exact(4)).chain(clear.chunks_exact(4)).all(|p| p[3]==255));
    }
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
