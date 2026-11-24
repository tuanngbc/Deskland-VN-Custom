//! Native fullscreen library. The only HWNDs created here are the overlay and
//! its child editor; the main process owns the profile and the app catalog.
use super::{
    catalog,
    library::{Library, Preferences, TileKey},
    *,
};
use std::{
    collections::{HashMap, HashSet},
    ptr,
    sync::mpsc,
};
use windows_sys::Win32::Graphics::Dwm::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{Dialogs::*, MARGINS};

const CLASS: &str = "TLauncherSpotlightLibrary";
const EDIT: usize = 501;
const EDIT_KEY: u32 = WM_APP + 150;
const POLL: usize = 1;
const SEARCH: usize = 2;
const SHELL_REFRESH: usize = 3;
const CATALOG_CHANGE: u32 = WM_APP + 151;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Category {
    All,
    Apps,
    Files,
    System,
}
impl Category {
    const ALL: [Self; 4] = [Self::All, Self::Apps, Self::Files, Self::System];
    const LABELS: [&str; 4] = ["Tất cả", "Ứng dụng", "Tệp tin", "Hệ thống"];
    fn accepts(self, e: &Entry) -> bool {
        match self {
            Self::All => true,
            Self::Apps => app_entry(e),
            Self::Files => matches!(e.kind, "File" | "Web shortcut"),
            Self::System => e.kind == "System",
        }
    }
}

#[derive(Clone)]
struct Tile {
    key: TileKey,
    entry: Entry,
    preview: Vec<PathBuf>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    Compact,
    Close,
    Edit,
    Add,
    Create,
    Hidden,
    Refresh,
    Back,
    Tab(Category),
    Prev,
    Next,
    Page(usize),
    Tile(usize),
}

#[derive(Clone, Copy)]
struct Grid {
    area: RECT,
    columns: usize,
    rows: usize,
    dx: i32,
    dy: i32,
    icon: i32,
}
impl Grid {
    fn new(w: i32, h: i32, dpi: u32, cfg: &Preferences) -> Self {
        let p = |n| px(n, dpi);
        let edge = p(28).min(w / 10);
        let mut area = rect(
            edge,
            p(if w < p(740) { 220 } else { 190 }).min(h / 2),
            (w - edge).max(edge + 1),
            (h - p(78)).max(p(if w < p(740) { 220 } else { 190 }).min(h / 2) + 1),
        );
        let icon = p(cfg.icon_size)
            .min((area.bottom - area.top - p(28)).max(12))
            .min(w / 4)
            .max(8);
        let want_x = (icon + p(cfg.spacing + 52)).max(p(108));
        let want_y = (icon + p(cfg.spacing + 32)).max(p(82));
        let aw = (area.right - area.left).max(1);
        let ah = (area.bottom - area.top).max(1);
        let columns = (aw / want_x).clamp(1, 10) as usize;
        let rows = (ah / want_y).clamp(1, 7) as usize;
        let dx = (aw / columns as i32).min(p(174)).max(1);
        let dy = (ah / rows as i32).min(p(150)).max(1);
        let left = area.left + (aw - dx * columns as i32) / 2;
        area.left = left;
        area.right = left + dx * columns as i32;
        Self {
            area,
            columns,
            rows,
            dx,
            dy,
            icon,
        }
    }
    fn capacity(self) -> usize {
        self.columns * self.rows
    }
    fn tiles(self, i: usize) -> RECT {
        let x = self.area.left + (i % self.columns) as i32 * self.dx;
        let y = self.area.top + (i / self.columns) as i32 * self.dy;
        rect(x + 2, y + 2, x + self.dx - 2, y + self.dy - 2)
    }
}
fn px(n: i32, dpi: u32) -> i32 {
    (n * dpi as i32 + 48) / 96
}
fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
    RECT {
        left,
        top,
        right,
        bottom,
    }
}
fn hit(r: RECT, p: POINT) -> bool {
    p.x >= r.left && p.y >= r.top && p.x < r.right && p.y < r.bottom
}
fn coords(l: LPARAM) -> POINT {
    POINT {
        x: l as i16 as i32,
        y: (l >> 16) as i16 as i32,
    }
}

struct Panel {
    hwnd: HWND,
    edit: HWND,
    prior: HWND,
    point: POINT,
    width: i32,
    height: i32,
    dpi: u32,
    acrylic: bool,
    dark: bool,
    font: HFONT,
    apps: Vec<Entry>,
    catalog: Option<mpsc::Receiver<Vec<Entry>>>,
    watch: Option<catalog::ShellWatch>,
    input: String,
    category: Category,
    folder: u64,
    hidden: bool,
    generation: u64,
    search_rx: Option<mpsc::Receiver<SearchReply>>,
    accept_pending: bool,
    search_settled: bool,
    editing: bool,
    page: usize,
    selected: usize,
    tiles: Vec<Tile>,
    error: String,
    hover: Option<Hit>,
    pressed: Option<(Hit, POINT)>,
    icon_tx: IconRequests,
    icon_rx: mpsc::Receiver<IconResult>,
    icons: HashMap<IconKey, Option<IconPixels>>,
    pending: HashSet<IconKey>,
    icon_rev: u64,
}
thread_local! {
    static PANEL: RefCell<Option<Panel>> = const { RefCell::new(None) };
    static OPTIONS: RefCell<Preferences> = RefCell::new(Preferences::default());
    static LIBRARY: RefCell<Library> = RefCell::new(Library::default());
    static EDIT_PROC: Cell<WNDPROC> = const { Cell::new(None) };
    static MODAL: Cell<bool> = const { Cell::new(false) };
}
pub(super) fn configure(prefs: Preferences) {
    let changed = OPTIONS.with(|o| {
        if *o.borrow() == prefs {
            return false;
        }
        o.replace(prefs);
        true
    });
    if changed && enabled() {
        if let Some(hwnd) = window() {
            unsafe {
                SendMessageW(hwnd, WM_THEMECHANGED, 0, 0);
            }
            rebuild(hwnd, false);
        }
    }
}
pub(super) fn initialize(library: Library) {
    LIBRARY.with(|s| s.replace(library));
}
pub(super) fn merge_manual(rows: &mut Vec<Entry>) {
    LIBRARY.with(|s| s.borrow().merge_manual(rows));
}
pub(super) fn window() -> Option<HWND> {
    PANEL.with(|p| p.borrow().as_ref().map(|s| s.hwnd))
}
pub(super) fn enabled() -> bool {
    OPTIONS.with(|s| s.borrow().enabled)
}
pub(super) fn dock_fullscreen() -> bool {
    OPTIONS.with(|s| s.borrow().dock_fullscreen)
}

fn dark() -> bool {
    OPTIONS.with(|p| match p.borrow().theme.as_str() {
        "dark" => true,
        "light" => false,
        _ => !crate::web::light_theme(),
    })
}
fn grid(p: &Panel) -> Grid {
    OPTIONS.with(|o| Grid::new(p.width, p.height, p.dpi, &o.borrow()))
}
fn count_pages(p: &Panel) -> usize {
    p.tiles.len().div_ceil(grid(p).capacity()).max(1)
}
fn refresh(hwnd: HWND) {
    let _ = unsafe { InvalidateRect(hwnd, ptr::null(), 0) };
}

fn system_items() -> Vec<Entry> {
    [
        ("Cài đặt Windows", "ms-settings:"),
        ("Task Manager", "Taskmgr.exe"),
        ("Bảng điều khiển · Control Panel", "control.exe"),
        ("File Explorer", "explorer.exe"),
    ]
    .into_iter()
    .map(|(name, path)| Entry {
        name: name.into(),
        path: path.into(),
        kind: "System",
        detail: "Windows".into(),
    })
    .collect()
}

struct SearchWork {
    source: Vec<Entry>,
    lib: Library,
    query: String,
    category: Category,
    folder: u64,
    hidden: bool,
    favorites: bool,
}
impl SearchWork {
    fn from_panel(p: &Panel) -> Self {
        let mut source = p.apps.clone();
        sync_pin_entries(&mut source);
        let lib = LIBRARY.with(|s| s.borrow().clone());
        lib.merge_manual(&mut source);
        Self {
            source,
            lib,
            query: p.input.trim().to_lowercase(),
            category: p.category,
            folder: p.folder,
            hidden: p.hidden,
            favorites: OPTIONS.with(|o| o.borrow().favorites_first),
        }
    }
    fn rows(self) -> Vec<Tile> {
        let Self {
            mut source,
            lib,
            query,
            category,
            folder,
            hidden,
            favorites,
        } = self;
        let browsing = query.is_empty();
        if !browsing || category == Category::System {
            source.extend(system_items());
        }
        let mut previews: HashMap<u64, Vec<PathBuf>> = HashMap::new();
        if browsing && folder == 0 && !hidden {
            for app in source.iter().filter(|app| app_entry(app)) {
                let id = catalog::identity(&app.path);
                let group = lib.folder(&id);
                if group == 0 || lib.hidden(&id) {
                    continue;
                }
                let icons = previews.entry(group).or_default();
                if icons.len() < 4 {
                    icons.push(app.path.clone());
                }
            }
        }
        let mut found: Vec<_> = source
            .into_iter()
            .filter_map(|mut e| {
                if !category.accepts(&e)
                    || (browsing && category == Category::All && !app_entry(&e))
                {
                    return None;
                }
                let id = catalog::identity(&e.path);
                if app_entry(&e) {
                    if lib.hidden(&id) != hidden
                        || (browsing && !hidden && lib.folder(&id) != folder)
                    {
                        return None;
                    }
                    e.name = lib.name(&id, &e.name).to_owned();
                } else if hidden || folder != 0 {
                    return None;
                }
                let rank = if browsing {
                    0
                } else {
                    catalog::search_rank(&e.name, &query)?
                };
                Some((rank, (TileKey::App(id), e)))
            })
            .collect();
        if browsing && folder == 0 && !hidden && matches!(category, Category::All | Category::Apps)
        {
            found.extend(lib.folders.iter().map(|f| {
                (
                    0,
                    (
                        TileKey::Folder(f.id),
                        Entry {
                            name: f.name.clone(),
                            path: PathBuf::new(),
                            kind: "Folder",
                            detail: String::new(),
                        },
                    ),
                )
            }));
        }
        let sorted = if browsing {
            let mut items = found.into_iter().map(|(_, t)| t).collect::<Vec<_>>();
            lib.ordered(&mut items, favorites);
            items
        } else {
            found.sort_by_key(|(rank, (_, e))| (*rank, !app_entry(e)));
            found.into_iter().take(100).map(|(_, t)| t).collect()
        };
        sorted
            .into_iter()
            .map(|(key, entry)| {
                let preview = if let TileKey::Folder(id) = key {
                    previews.remove(&id).unwrap_or_default()
                } else {
                    Vec::new()
                };
                Tile {
                    key,
                    entry,
                    preview,
                }
            })
            .collect()
    }
}

type SearchReply = (u64, Vec<Tile>);
fn search_async(gen: u64, job: SearchWork) -> Option<mpsc::Receiver<SearchReply>> {
    type Task = (u64, SearchWork, mpsc::Sender<SearchReply>);
    static QUEUE: std::sync::OnceLock<Option<mpsc::SyncSender<Task>>> = std::sync::OnceLock::new();
    let queue = QUEUE
        .get_or_init(|| {
            let (tx, rx) = mpsc::sync_channel::<Task>(1);
            std::thread::Builder::new()
                .name("tlauncher-spotlight-search".into())
                .spawn(move || {
                    while let Ok((generation, job, reply)) = rx.recv() {
                        let _ = reply.send((generation, job.rows()));
                    }
                })
                .ok()?;
            Some(tx)
        })
        .as_ref()?;
    let (tx, rx) = mpsc::channel();
    queue.try_send((gen, job, tx)).ok()?;
    Some(rx)
}

fn apply_rows(hwnd: HWND, generation: u64, tiles: Vec<Tile>) {
    let launch = PANEL.with(|state| {
        let mut state = state.borrow_mut();
        let p = state.as_mut()?;
        if generation != p.generation {
            return None;
        }
        p.search_settled = true;
        p.tiles = tiles;
        p.page = p.page.min(count_pages(p) - 1);
        p.selected = p.selected.min(p.tiles.len().saturating_sub(1));
        (std::mem::take(&mut p.accept_pending) && !p.tiles.is_empty()).then_some(p.selected)
    });
    if let Some(index) = launch {
        unsafe {
            action(hwnd, Hit::Tile(index));
        }
    }
    refresh(hwnd);
}
fn rebuild(hwnd: HWND, reset_page: bool) {
    let Some((gen, job)) = PANEL.with(|state| {
        let mut state = state.borrow_mut();
        let p = state.as_mut()?;
        p.generation = p.generation.wrapping_add(1);
        p.search_rx = None;
        p.search_settled = false;
        if reset_page {
            p.page = 0;
            p.selected = 0;
        }
        Some((p.generation, SearchWork::from_panel(p)))
    }) else {
        return;
    };
    if job.query.is_empty() && job.source.len() < 1000 {
        apply_rows(hwnd, gen, job.rows());
        return;
    }
    let reply = search_async(gen, job);
    unsafe {
        if reply.is_none() {
            SetTimer(hwnd, SEARCH, 45, None);
            return;
        }
        SetTimer(hwnd, POLL, 25, None);
    }
    PANEL.with(|s| {
        if let Some(p) = s.borrow_mut().as_mut() {
            p.search_rx = reply;
        }
    });
    refresh(hwnd);
}

fn controls(p: &Panel) -> Vec<(Hit, RECT, &'static str)> {
    let u = |n| px(n, p.dpi);
    let w = p.width;
    let mid = w / 2;
    let narrow = w < u(740);
    let side = u(if narrow { 12 } else { 26 });
    let button = |x, y, w| rect(x, y, x + w, y + u(32));
    let mut result = vec![
        (Hit::Close, button(w - side - u(44), u(22), u(44)), "×"),
        (
            Hit::Compact,
            button(
                w - side - u(if narrow { 116 } else { 152 }),
                u(22),
                u(if narrow { 66 } else { 102 }),
            ),
            "Thu gọn",
        ),
        (
            Hit::Edit,
            button(
                w - side - u(if narrow { 196 } else { 270 }),
                u(22),
                u(if narrow { 74 } else { 110 }),
            ),
            if p.editing { "Xong" } else { "Chỉnh sửa" },
        ),
        (
            Hit::Add,
            button(
                w - side - u(if narrow { 126 } else { 182 }),
                u(150),
                u(if narrow { 126 } else { 182 }),
            ),
            "+ Thêm ứng dụng",
        ),
        (
            Hit::Refresh,
            button(
                w - side - u(if narrow { 204 } else { 284 }),
                u(150),
                u(if narrow { 72 } else { 94 }),
            ),
            "Làm mới",
        ),
    ];
    if p.folder != 0 || p.hidden {
        result.push((Hit::Back, button(side, u(150), u(88)), "‹ Thư viện"));
    } else if p.editing {
        result.push((
            Hit::Create,
            button(side, u(150), u(if narrow { 88 } else { 104 })),
            "+ Thư mục",
        ));
        result.push((
            Hit::Hidden,
            button(
                side + u(if narrow { 92 } else { 110 }),
                u(150),
                u(if narrow { 78 } else { 92 }),
            ),
            "Mục đã ẩn",
        ));
    }
    let tab_width = u(106).min(w / 5).max(1);
    for (i, cat) in Category::ALL.into_iter().enumerate() {
        result.push((
            Hit::Tab(cat),
            button(
                mid - 2 * tab_width + i as i32 * tab_width,
                u(109),
                tab_width - 2,
            ),
            Category::LABELS[i],
        ));
    }
    let y = p.height - u(61);
    let pages = count_pages(p);
    if pages > 1 {
        result.push((Hit::Prev, button(mid - u(151), y, u(46)), "‹"));
        result.push((Hit::Next, button(mid + u(105), y, u(46)), "›"));
        if pages <= 9 {
            for i in 0..pages {
                result.push((
                    Hit::Page(i),
                    button(
                        mid - (pages as i32 * u(21)) / 2 + i as i32 * u(21),
                        y,
                        u(21),
                    ),
                    if i == p.page { "●" } else { "○" },
                ));
            }
        }
    }
    result
}

fn hit_at(p: &Panel, point: POINT) -> Option<Hit> {
    if let Some((action, ..)) = controls(p).into_iter().find(|(_, r, _)| hit(*r, point)) {
        return Some(action);
    }
    let g = grid(p);
    if !hit(g.area, point) {
        return None;
    }
    let x = ((point.x - g.area.left) / g.dx) as usize;
    let y = ((point.y - g.area.top) / g.dy) as usize;
    if y >= g.rows || x >= g.columns {
        return None;
    }
    let i = p.page * g.capacity() + y * g.columns + x;
    (i < p.tiles.len()).then_some(Hit::Tile(i))
}

unsafe fn fill(dc: HDC, r: RECT, rgb: u32) {
    SetDCBrushColor(dc, rgb);
    FillRect(dc, &r, GetStockObject(DC_BRUSH) as HBRUSH);
}
unsafe fn round(dc: HDC, r: RECT, rad: i32, rgb: u32) {
    SetDCBrushColor(dc, rgb);
    let b = SelectObject(dc, GetStockObject(DC_BRUSH));
    let p = SelectObject(dc, GetStockObject(NULL_PEN));
    RoundRect(dc, r.left, r.top, r.right, r.bottom, rad, rad);
    SelectObject(dc, p);
    SelectObject(dc, b);
}
unsafe fn text(dc: HDC, s: &str, mut r: RECT, f: HFONT, ink: u32, center: bool) {
    let old = SelectObject(dc, f);
    SetTextColor(dc, ink);
    SetBkMode(dc, TRANSPARENT as i32);
    DrawTextW(
        dc,
        wide(s).as_ptr(),
        -1,
        &mut r,
        DT_SINGLELINE
            | DT_VCENTER
            | DT_END_ELLIPSIS
            | DT_NOPREFIX
            | if center { DT_CENTER } else { DT_LEFT },
    );
    SelectObject(dc, old);
}
unsafe fn draw_icon(dc: HDC, icon: &IconPixels, x: i32, y: i32) {
    let mut info: BITMAPINFO = std::mem::zeroed();
    info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = icon.width;
    info.bmiHeader.biHeight = -icon.height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    let mut bits = ptr::null_mut();
    let bm = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0);
    if bm.is_null() {
        return;
    }
    let src = CreateCompatibleDC(dc);
    if !src.is_null() && !bits.is_null() {
        ptr::copy_nonoverlapping(icon.pixels.as_ptr(), bits.cast(), icon.pixels.len());
        let old = SelectObject(src, bm);
        AlphaBlend(
            dc,
            x,
            y,
            icon.width,
            icon.height,
            src,
            0,
            0,
            icon.width,
            icon.height,
            BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            },
        );
        SelectObject(src, old);
    }
    if !src.is_null() {
        DeleteDC(src);
    }
    DeleteObject(bm);
}

type PaintTile = (
    Tile,
    RECT,
    Option<IconPixels>,
    Vec<Option<IconPixels>>,
    bool,
);
fn icon_for_tile(p: &mut Panel, path: PathBuf, size: i32) -> Option<IconPixels> {
    let key = (path, size.clamp(8, 192));
    if !key.0.as_os_str().is_empty()
        && !p.icons.contains_key(&key)
        && p.pending.len() < 32
        && p.pending.insert(key.clone())
        && p.icon_tx.send(key.clone()).is_err()
    {
        p.pending.remove(&key);
    }
    p.icons.get(&key).cloned().flatten()
}
unsafe fn paint_full(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let dc = BeginPaint(hwnd, &mut ps);
    render_full(hwnd, dc, false);
    EndPaint(hwnd, &ps);
}
unsafe fn render_full(hwnd: HWND, dc: HDC, force_fallback: bool) {
    let view = PANEL.with(|s| {
        let mut s = s.borrow_mut();
        let p = s.as_mut()?;
        if p.icon_rev != crate::app_icons::revision() {
            p.icons.clear();
            p.pending.clear();
            (p.icon_tx, p.icon_rx) = icon_worker();
            p.icon_rev = crate::app_icons::revision();
        }
        let grid = grid(p);
        let start = p.page * grid.capacity();
        let visible = p
            .tiles
            .iter()
            .skip(start)
            .take(grid.capacity())
            .cloned()
            .collect::<Vec<_>>();
        let tiles = visible
            .into_iter()
            .enumerate()
            .map(|(i, t)| {
                let icon = icon_for_tile(p, t.entry.path.clone(), grid.icon);
                let preview = t
                    .preview
                    .iter()
                    .map(|path| icon_for_tile(p, path.clone(), grid.icon / 3))
                    .collect();
                (t, grid.tiles(i), icon, preview, p.selected == i + start)
            })
            .collect::<Vec<PaintTile>>();
        let heading = LIBRARY.with(|l| {
            l.borrow()
                .folders
                .iter()
                .find(|f| f.id == p.folder)
                .map(|f| f.name.clone())
        });
        Some((
            p.width,
            p.height,
            p.dpi,
            p.dark,
            p.acrylic,
            p.font,
            grid.icon,
            start,
            tiles,
            controls(p),
            p.hover,
            p.category,
            p.page,
            count_pages(p),
            !p.pending.is_empty(),
            p.error.clone(),
            heading,
            p.hidden,
            p.catalog.is_some(),
            p.input.is_empty(),
        ))
    });
    if let Some((
        w,
        h,
        dpi,
        dark,
        acrylic,
        font,
        size,
        start,
        tiles,
        buttons,
        hover,
        category,
        page,
        pages,
        busy,
        error,
        heading,
        hidden,
        loading,
        empty_query,
    )) = view
    {
        let u = |n| px(n, dpi);
        let face = if dark { 0x00373230 } else { 0x00f9f6f4 };
        let ink = if dark { 0x00f8f5f2 } else { 0x0024211d };
        let secondary = if dark { 0x00d8d0cb } else { 0x005a5048 };
        let active = if dark { 0x00664c35 } else { 0x00f5e5d5 };
        let fallback = if dark { 0x002c2624 } else { 0x00e9e3dc };
        fill(
            dc,
            rect(0, 0, w, h),
            if acrylic && !force_fallback {
                0
            } else {
                fallback
            },
        );
        let header = crate::native_text::font_pixels(u(21), 600);
        text(
            dc,
            "TLauncher Spotlight",
            rect(
                u(26),
                u(16),
                w.min(w - u(if w < u(740) { 210 } else { 300 })).max(u(60)),
                u(57),
            ),
            header,
            ink,
            false,
        );
        let search_w = u(600).min(w - u(50)).max(1);
        round(
            dc,
            rect((w - search_w) / 2, u(64), (w + search_w) / 2, u(102)),
            u(16),
            face,
        );
        for (action, r, s) in buttons {
            let selected = matches!(action,Hit::Tab(c) if c==category);
            round(
                dc,
                r,
                u(12),
                if selected || hover == Some(action) {
                    active
                } else {
                    face
                },
            );
            text(dc, s, r, font, ink, true);
        }
        let label = if hidden {
            "Ứng dụng đã ẩn"
        } else {
            heading.as_deref().unwrap_or(if empty_query {
                "Thư viện ứng dụng"
            } else {
                "Kết quả tìm kiếm"
            })
        };
        text(
            dc,
            label,
            if w < u(740) {
                rect(u(12), u(182), w - u(12), u(216))
            } else {
                rect(w / 3, u(147), w * 2 / 3, u(184))
            },
            header,
            ink,
            true,
        );
        let names = OPTIONS.with(|o| o.borrow().show_names);
        for (i, (t, r, icon, preview, sel)) in tiles.iter().enumerate() {
            let hovered = hover == Some(Hit::Tile(start + i));
            if *sel || hovered {
                round(dc, *r, u(18), active);
            }
            let ix = (r.left + r.right) / 2;
            let iy = r.top + ((r.bottom - r.top - size - u(16)) / 2).max(0);
            if let Some(img) = icon {
                draw_icon(dc, img, ix - img.width / 2, iy + (size - img.height) / 2);
            } else {
                let badge = rect(ix - size / 2, iy, ix + (size + 1) / 2, iy + size);
                round(
                    dc,
                    badge,
                    u(18),
                    if t.entry.kind == "Folder" {
                        active
                    } else {
                        face
                    },
                );
                let letter = if t.entry.kind == "Folder" {
                    "▦".into()
                } else {
                    t.entry.name.chars().take(1).collect::<String>()
                };
                if preview.iter().any(Option::is_some) {
                    for (i, icon) in preview.iter().enumerate() {
                        if let Some(icon) = icon {
                            let quarter = size / 4;
                            let x =
                                badge.left + quarter + (i % 2) as i32 * (size / 2) - icon.width / 2;
                            let y =
                                badge.top + quarter + (i / 2) as i32 * (size / 2) - icon.height / 2;
                            draw_icon(dc, icon, x, y);
                        }
                    }
                } else {
                    text(dc, &letter, badge, header, ink, true);
                }
            }
            if names || !empty_query {
                let last = (iy + size + u(24)).min(r.bottom);
                text(
                    dc,
                    &t.entry.name,
                    rect(r.left + u(5), last - u(23), r.right - u(5), last),
                    font,
                    ink,
                    true,
                );
            }
        }
        if tiles.is_empty() {
            let message = if loading {
                "Đang tải ứng dụng…"
            } else {
                "Không có mục phù hợp"
            };
            text(
                dc,
                message,
                rect(0, h / 2, w, h / 2 + u(38)),
                header,
                secondary,
                true,
            );
        }
        if pages > 9 {
            text(
                dc,
                &format!("{} / {}", page + 1, pages),
                rect(w / 3, h - u(61), w * 2 / 3, h - u(29)),
                font,
                ink,
                true,
            );
        }
        let footer = if error.is_empty() {
            "Gõ để tìm · Cuộn hoặc PgUp/PgDn để chuyển trang · Esc để đóng"
        } else {
            &error
        };
        text(
            dc,
            footer,
            rect(u(12), h - u(27), w - u(12), h - u(3)),
            font,
            secondary,
            true,
        );
        if !header.is_null() {
            DeleteObject(header);
        }
        if busy {
            SetTimer(hwnd, POLL, 45, None);
        }
    }
}

fn show_error(hwnd: HWND, message: &str) {
    PANEL.with(|p| {
        if let Some(p) = p.borrow_mut().as_mut() {
            p.error = message.into();
        }
    });
    refresh(hwnd);
}
fn change(hwnd: HWND, apply: impl FnOnce(&mut Library) -> Result<(), &'static str>) {
    let mut next = LIBRARY.with(|l| l.borrow().clone());
    if let Err(message) = apply(&mut next) {
        show_error(hwnd, message);
        return;
    }
    if !cfg!(test) {
        if let Err(error) = crate::config_store::save(
            &crate::rust_settings_path(),
            serde_json::json!({"spotlightLibrary":&next}),
        ) {
            show_error(hwnd, "Không lưu được thay đổi vào hồ sơ TLauncher.");
            crate::log!(
                "Spotlight library: settings write failed ({:?})",
                error.kind()
            );
            return;
        }
    }
    initialize(next);
    rebuild(hwnd, false);
}
pub(super) fn reset(hwnd: HWND) {
    unsafe {
        MODAL.set(true);
        let result = MessageBoxW(
            hwnd,
            wide(
                "Đặt lại thứ tự và thư mục? Ứng dụng tự thêm, tên, icon và mục đã ẩn vẫn được giữ.",
            )
            .as_ptr(),
            wide("TLauncher Spotlight").as_ptr(),
            MB_OKCANCEL | MB_ICONQUESTION,
        );
        MODAL.set(false);
        if result == IDOK {
            change(hwnd, |l| {
                l.reset_layout();
                Ok(())
            });
        }
    }
}

pub(super) fn refresh_catalog(hwnd: HWND) {
    PANEL.with(|s| {
        if let Some(p) = s.borrow_mut().as_mut() {
            p.catalog = catalog::request(true, true);
        }
    });
    unsafe {
        SetTimer(hwnd, POLL, 40, None);
    }
}
pub(super) fn update_pins() {
    if let Some(hwnd) = window() {
        rebuild(hwnd, false);
    }
}

unsafe fn acrylic(hwnd: HWND, dark: bool) -> bool {
    let attr = |id: DWMWINDOWATTRIBUTE, value: i32| {
        DwmSetWindowAttribute(hwnd, id as u32, (&value as *const i32).cast(), 4)
    };
    let _ = attr(DWMWA_USE_IMMERSIVE_DARK_MODE, i32::from(dark));
    let _ = attr(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND);
    let frame = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    let result = attr(DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_TRANSIENTWINDOW);
    if result >= 0 && DwmExtendFrameIntoClientArea(hwnd, &frame) >= 0 {
        return true;
    }
    let _ = attr(DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_NONE);
    false
}

fn estimate(point: POINT) -> Option<(RECT, u32)> {
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    unsafe {
        let mon = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut mi) == 0 {
            return None;
        }
        let mut x = 96;
        let mut y = 96;
        let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut x, &mut y);
        Some((mi.rcMonitor, x.clamp(72, 288)))
    }
}
unsafe fn place(hwnd: HWND, point: POINT) {
    let Some((r, dpi)) = estimate(point) else {
        return;
    };
    let (width, height) = (r.right - r.left, r.bottom - r.top);
    if width <= 0 || height <= 0 {
        return;
    }
    let Some((edit, old_font)) = PANEL.with(|p| {
        let mut p = p.borrow_mut();
        let p = p.as_mut()?;
        let old = p.font;
        p.point = point;
        p.dpi = dpi;
        p.width = width;
        p.height = height;
        p.font = crate::native_text::font_pixels(px(15, dpi), 450);
        Some((p.edit, old))
    }) else {
        return;
    };
    let fw = px(600, dpi).min(width - px(50, dpi)).max(1);
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        r.left,
        r.top,
        width,
        height,
        SWP_NOACTIVATE,
    );
    SetWindowPos(
        edit,
        ptr::null_mut(),
        (width - fw) / 2 + px(14, dpi),
        px(73, dpi),
        (fw - px(28, dpi)).max(1),
        px(24, dpi),
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
    if let Some(font) = PANEL.with(|p| p.borrow().as_ref().map(|s| s.font)) {
        SendMessageW(edit, WM_SETFONT, font as WPARAM, 0);
    }
    if !old_font.is_null() {
        DeleteObject(old_font);
    }
    rebuild(hwnd, false);
}

pub(super) fn dismiss(restore: bool) {
    let Some(mut p) = PANEL.with(|s| s.borrow_mut().take()) else {
        return;
    };
    unsafe {
        if GetCapture() == p.hwnd {
            ReleaseCapture();
        }
        KillTimer(p.hwnd, POLL);
        KillTimer(p.hwnd, SEARCH);
        KillTimer(p.hwnd, SHELL_REFRESH);
        drop(p.watch.take());
        DestroyWindow(p.hwnd);
        if !p.font.is_null() {
            DeleteObject(p.font);
        }
        if restore && !p.prior.is_null() && IsWindow(p.prior) != 0 {
            SetForegroundWindow(p.prior);
        }
    }
}

fn compact_query() -> Option<(POINT, String, HWND)> {
    PANEL.with(|p| {
        p.borrow()
            .as_ref()
            .map(|p| (p.point, p.input.clone(), p.prior))
    })
}

fn safe_launch(entry: &Entry) -> bool {
    if entry.kind == "Folder" {
        return false;
    }
    let value = entry.path.to_string_lossy();
    if crate::dock::command_pin::decode(&value).is_some() {
        return true;
    }
    if apps_folder_parsing_name(&value).is_some() {
        return app_entry(entry);
    }
    entry.path.is_absolute()
        || matches!(
            value.as_ref(),
            "mspaint.exe"
                | "notepad.exe"
                | "calc.exe"
                | "explorer.exe"
                | "ms-settings:"
                | "Taskmgr.exe"
                | "control.exe"
        )
}

unsafe fn open_entry(hwnd: HWND, entry: &Entry, elevated: bool) {
    if !safe_launch(entry) {
        show_error(hwnd, "Mục này không có đường dẫn mở hợp lệ.");
        return;
    }
    MODAL.set(true);
    ShowWindow(hwnd, SW_HIDE);
    let result = if elevated {
        #[cfg(test)]
        {
            33
        }
        #[cfg(not(test))]
        {
            ShellExecuteW(
                hwnd,
                wide("runas").as_ptr(),
                wide(&entry.path.to_string_lossy()).as_ptr(),
                ptr::null(),
                ptr::null(),
                SW_SHOWNORMAL,
            ) as isize
        }
    } else {
        execute(hwnd, &entry.path)
    };
    MODAL.set(false);
    if result > 32 {
        dismiss(false);
    } else {
        ShowWindow(hwnd, SW_SHOW);
        if let Some(edit) = PANEL.with(|p| p.borrow().as_ref().map(|p| p.edit)) {
            SetFocus(edit);
        }
        show_error(hwnd, &format!("Windows không mở được mục này ({result})."));
    }
}

unsafe fn menu(hwnd: HWND, point: POINT, labels: &[&str]) -> usize {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return 0;
    }
    for (i, label) in labels.iter().enumerate() {
        AppendMenuW(menu, MF_STRING, i + 1, wide(label).as_ptr());
    }
    let mut at = point;
    ClientToScreen(hwnd, &mut at);
    MODAL.set(true);
    let result = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
        at.x,
        at.y,
        0,
        hwnd,
        ptr::null(),
    ) as usize;
    MODAL.set(false);
    DestroyMenu(menu);
    result
}

unsafe fn pick(hwnd: HWND, filter: &str, title: &str) -> Option<PathBuf> {
    let mut path = [0u16; 32768];
    let filter = wide(filter);
    let title = wide(title);
    let mut dialog: OPENFILENAMEW = std::mem::zeroed();
    dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner = hwnd;
    dialog.lpstrFilter = filter.as_ptr();
    dialog.lpstrTitle = title.as_ptr();
    dialog.lpstrFile = path.as_mut_ptr();
    dialog.nMaxFile = path.len() as u32;
    dialog.Flags =
        OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_HIDEREADONLY | OFN_EXPLORER;
    MODAL.set(true);
    let ok = GetOpenFileNameW(&mut dialog) != 0;
    MODAL.set(false);
    ok.then(|| {
        PathBuf::from(String::from_utf16_lossy(
            &path[..path.iter().position(|&c| c == 0).unwrap_or(path.len())],
        ))
    })
}

struct Prompt<'a> {
    title: &'a str,
    fields: &'a [(&'a str, &'a str)],
    values: Vec<String>,
}
unsafe extern "system" fn prompt_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> isize {
    const FIRST: i32 = 610;
    match msg {
        WM_INITDIALOG => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, l);
            let data = &*(l as *const Prompt<'_>);
            SetWindowTextW(hwnd, wide(data.title).as_ptr());
            let mut r: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut r);
            let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd).clamp(72, 288);
            let n = |n| px(n, dpi);
            let h = GetModuleHandleW(ptr::null());
            for (i, (name, value)) in data.fields.iter().enumerate() {
                let y = n(14 + i as i32 * 58);
                let label = CreateWindowExW(
                    0,
                    wide("STATIC").as_ptr(),
                    wide(name).as_ptr(),
                    WS_CHILD | WS_VISIBLE,
                    n(14),
                    y,
                    r.right - n(28),
                    n(18),
                    hwnd,
                    ptr::null_mut(),
                    h,
                    ptr::null_mut(),
                );
                let edit = CreateWindowExW(
                    WS_EX_CLIENTEDGE,
                    wide("EDIT").as_ptr(),
                    wide(value).as_ptr(),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32,
                    n(14),
                    y + n(19),
                    r.right - n(28),
                    n(26),
                    hwnd,
                    (FIRST + i as i32) as HMENU,
                    h,
                    ptr::null_mut(),
                );
                for child in [label, edit] {
                    SendMessageW(
                        child,
                        WM_SETFONT,
                        GetStockObject(DEFAULT_GUI_FONT) as WPARAM,
                        1,
                    );
                }
                SendMessageW(edit, 0x00c5, 4096, 0);
            }
            let y = n(14 + data.fields.len() as i32 * 58);
            for (id, name, x, style) in [
                (IDOK, "Lưu", r.right - n(194), BS_DEFPUSHBUTTON),
                (IDCANCEL, "Hủy", r.right - n(100), BS_PUSHBUTTON),
            ] {
                let button = CreateWindowExW(
                    0,
                    wide("BUTTON").as_ptr(),
                    wide(name).as_ptr(),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | style as u32,
                    x,
                    y,
                    n(86),
                    n(28),
                    hwnd,
                    id as HMENU,
                    h,
                    ptr::null_mut(),
                );
                SendMessageW(
                    button,
                    WM_SETFONT,
                    GetStockObject(DEFAULT_GUI_FONT) as WPARAM,
                    1,
                );
            }
            let edit = GetDlgItem(hwnd, FIRST);
            SetFocus(edit);
            SendMessageW(edit, 0x00b1, 0, -1);
            0
        }
        WM_COMMAND if w & 0xffff == IDOK as usize => {
            let value = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            if value != 0 {
                let data = &mut *(value as *mut Prompt<'_>);
                data.values = (0..data.fields.len())
                    .map(|i| read_edit(GetDlgItem(hwnd, FIRST + i as i32), 4096))
                    .collect();
            }
            EndDialog(hwnd, 1);
            1
        }
        WM_CLOSE | WM_COMMAND if msg == WM_CLOSE || w & 0xffff == IDCANCEL as usize => {
            EndDialog(hwnd, 0);
            1
        }
        _ => 0,
    }
}
unsafe fn prompt(hwnd: HWND, title: &str, fields: &[(&str, &str)]) -> Option<Vec<String>> {
    // Standard empty dialog template; children use native Edit controls and
    // Windows tab/IME handling. Keep the dialog owned by the library window.
    #[repr(align(4))]
    struct Template([u16; 12]);
    let style = WS_POPUP | WS_CAPTION | WS_SYSMENU | DS_MODALFRAME as u32 | DS_CENTER as u32;
    let h = 42 + fields.len() as u16 * 37;
    let template = Template([
        style as u16,
        (style >> 16) as u16,
        0,
        0,
        0,
        0,
        0,
        300,
        h,
        0,
        0,
        0,
    ]);
    let mut ctx = Prompt {
        title,
        fields,
        values: Vec::new(),
    };
    MODAL.set(true);
    let result = DialogBoxIndirectParamW(
        GetModuleHandleW(ptr::null()),
        template.0.as_ptr().cast(),
        hwnd,
        Some(prompt_proc),
        (&mut ctx as *mut Prompt<'_>) as LPARAM,
    );
    MODAL.set(false);
    (result == 1).then_some(ctx.values)
}
unsafe fn read_edit(hwnd: HWND, limit: usize) -> String {
    let n = (GetWindowTextLengthW(hwnd).max(0) as usize).min(limit);
    let mut value = vec![0u16; n + 1];
    let got = GetWindowTextW(hwnd, value.as_mut_ptr(), value.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&value[..got])
}
unsafe fn add_app(hwnd: HWND) {
    let Some(path) = pick(
        hwnd,
        "Ứng dụng hoặc shortcut\0*.exe;*.lnk\0\0",
        "Thêm ứng dụng vào Spotlight",
    ) else {
        return;
    };
    let name = path.file_stem().and_then(|v| v.to_str()).unwrap_or("");
    let Some(values) = prompt(
        hwnd,
        "Thêm ứng dụng",
        &[
            ("Tên hiển thị", name),
            ("Đối số EXE (tùy chọn)", ""),
            ("Thư mục làm việc đầy đủ (tùy chọn)", ""),
        ],
    ) else {
        return;
    };
    change(hwnd, |lib| {
        lib.add(&path, &values[0], &values[1], &values[2])
    });
}

fn rewrite_state(hwnd: HWND, f: impl FnOnce(&mut Panel)) {
    PANEL.with(|s| {
        if let Some(p) = s.borrow_mut().as_mut() {
            f(p);
            p.error.clear();
        }
    });
    rebuild(hwnd, true);
}
fn internal_view(hwnd: HWND, folder: u64, hidden: bool) {
    rewrite_state(hwnd, |p| {
        p.folder = folder;
        p.hidden = hidden;
        p.category = Category::All;
    });
}
unsafe fn choose_folder(hwnd: HWND, id: &str, at: POINT) {
    let folders = LIBRARY.with(|l| l.borrow().folders.clone());
    let names = std::iter::once("Thư viện chính")
        .chain(folders.iter().map(|f| f.name.as_str()))
        .collect::<Vec<_>>();
    let i = menu(hwnd, at, &names);
    if i > 0 {
        change(hwnd, |l| {
            l.move_to(id, if i == 1 { 0 } else { folders[i - 2].id });
            Ok(())
        });
    }
}

unsafe fn tile_menu(hwnd: HWND, i: usize, point: POINT) {
    let Some((tile, hidden)) = PANEL.with(|s| {
        let s = s.borrow();
        let p = s.as_ref()?;
        Some((p.tiles.get(i)?.clone(), p.hidden))
    }) else {
        return;
    };
    if let TileKey::Folder(folder) = tile.key {
        match menu(
            hwnd,
            point,
            &[
                "Mở thư mục",
                "Đổi tên thư mục",
                "Xóa thư mục (giữ ứng dụng)",
            ],
        ) {
            1 => internal_view(hwnd, folder, false),
            2 => {
                if let Some(v) = prompt(
                    hwnd,
                    "Đổi tên thư mục",
                    &[("Tên thư mục", &tile.entry.name)],
                ) {
                    change(hwnd, |l| {
                        l.rename_folder(folder, &v[0]);
                        Ok(())
                    });
                }
            }
            3 => change(hwnd, |l| {
                l.delete_folder(folder);
                Ok(())
            }),
            _ => {}
        }
        return;
    }
    let TileKey::App(id) = tile.key else {
        return;
    };
    let entry = tile.entry;
    if !app_entry(&entry) {
        if menu(hwnd, point, &["Mở"]) == 1 {
            open_entry(hwnd, &entry, false);
        }
        return;
    }
    let (manual, favorite) = LIBRARY.with(|lib| {
        let l = lib.borrow();
        (l.is_manual(&id), l.favorite(&id))
    });
    let mut options = vec![(1, "Mở ứng dụng")];
    let at_file = entry.path.is_absolute();
    if at_file
        && entry
            .path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("exe"))
    {
        options.push((2, "Chạy với quyền quản trị…"));
    }
    let pin = pin_change(&entry);
    options.push((
        3,
        if matches!(&pin, Some(crate::dock::Edit::Unpin(_))) {
            "Bỏ ghim khỏi Dock"
        } else {
            "Ghim vào Dock"
        },
    ));
    options.push((
        4,
        if favorite {
            "Bỏ yêu thích"
        } else {
            "Thêm vào yêu thích"
        },
    ));
    options.push((5, "Chuyển vào thư mục…"));
    options.push((6, "Đổi tên hiển thị…"));
    options.push((7, "Đổi biểu tượng…"));
    if at_file || crate::dock::command_pin::decode(&entry.path.to_string_lossy()).is_some() {
        options.push((8, "Mở vị trí tệp"));
    }
    options.push((
        9,
        if hidden {
            "Khôi phục vào Spotlight"
        } else {
            "Ẩn khỏi Spotlight"
        },
    ));
    if manual {
        options.push((10, "Bỏ mục tự thêm (giữ tệp gốc)"));
    }
    let labels = options.iter().map(|(_, l)| *l).collect::<Vec<_>>();
    let action = menu(hwnd, point, &labels)
        .checked_sub(1)
        .and_then(|n| options.get(n))
        .map_or(0, |o| o.0);
    match action {
        1 => open_entry(hwnd, &entry, false),
        2 => open_entry(hwnd, &entry, true),
        3 => {
            if let Some(pin) = pin {
                PIN_EDIT.with(|c| c.replace(Some(pin)));
            }
        }
        4 => change(hwnd, |l| {
            l.toggle_favorite(&id);
            Ok(())
        }),
        5 => choose_folder(hwnd, &id, point),
        6 => {
            if let Some(v) = prompt(hwnd, "Đổi tên hiển thị", &[("Tên ứng dụng", &entry.name)])
            {
                change(hwnd, |l| {
                    l.rename(&id, &v[0]);
                    Ok(())
                });
            }
        }
        7 => choose_icon(hwnd, &entry.path),
        8 => {
            let target = crate::dock::command_pin::decode(&entry.path.to_string_lossy())
                .map_or_else(|| entry.path.clone(), |c| PathBuf::from(c.exe));
            if let Some(dir) = target.parent() {
                if dir.is_absolute() {
                    let _ = execute(hwnd, dir);
                }
            }
        }
        9 => change(hwnd, |l| {
            l.hide(&id, !hidden);
            Ok(())
        }),
        10 => change(hwnd, |l| {
            l.remove_manual(&id);
            Ok(())
        }),
        _ => {}
    }
}
unsafe fn choose_icon(hwnd: HWND, path: &Path) {
    let Some(key) = crate::app_icons::key(&path.to_string_lossy()) else {
        return;
    };
    let Some(file) = pick(
        hwnd,
        "Ảnh\0*.png;*.jpg;*.jpeg;*.ico;*.webp;*.bmp\0\0",
        "Đổi biểu tượng ứng dụng",
    ) else {
        return;
    };
    match crate::app_icons::import(&file) {
        Ok(file) => {
            PIN_EDIT.with(|e| {
                e.replace(Some(crate::dock::Edit::AppIcon {
                    key,
                    file: Some(file),
                }))
            });
        }
        Err(e) => show_error(hwnd, &e),
    }
}

unsafe fn action(hwnd: HWND, which: Hit) {
    match which {
        Hit::Close => dismiss(true),
        Hit::Compact => {
            if let Some((point, query, prior)) = compact_query() {
                dismiss(false);
                super::toggle_at(Some(point), Some(query), Some(prior));
            }
        }
        Hit::Add => add_app(hwnd),
        Hit::Refresh => refresh_catalog(hwnd),
        Hit::Edit => rewrite_state(hwnd, |p| p.editing = !p.editing),
        Hit::Hidden => show_hidden(hwnd),
        Hit::Back => internal_view(hwnd, 0, false),
        Hit::Create => {
            if let Some(v) = prompt(hwnd, "Thư mục mới", &[("Tên thư mục", "")]) {
                change(hwnd, |lib| {
                    lib.create_folder(&v[0])
                        .map(|_| ())
                        .ok_or("Nhập tên thư mục hợp lệ.")
                });
            }
        }
        Hit::Tab(category) => rewrite_state(hwnd, |p| {
            p.category = category;
            p.folder = 0;
            p.hidden = false;
        }),
        Hit::Prev => page(hwnd, -1),
        Hit::Next => page(hwnd, 1),
        Hit::Page(page) => set_page(hwnd, page),
        Hit::Tile(i) => {
            let Some((tile, editing)) = PANEL.with(|s| {
                let s = s.borrow();
                let p = s.as_ref()?;
                Some((p.tiles.get(i)?.clone(), p.editing))
            }) else {
                return;
            };
            match tile.key {
                TileKey::Folder(id) => internal_view(hwnd, id, false),
                TileKey::App(_) if !editing => open_entry(hwnd, &tile.entry, false),
                _ => {}
            }
        }
    }
}
fn set_page(hwnd: HWND, page: usize) {
    PANEL.with(|s| {
        if let Some(p) = s.borrow_mut().as_mut() {
            p.page = page.min(count_pages(p) - 1);
            p.selected = p.page * grid(p).capacity();
            p.hover = None;
        }
    });
    refresh(hwnd);
}
fn page(hwnd: HWND, delta: isize) {
    if let Some(value) = PANEL.with(|s| {
        s.borrow()
            .as_ref()
            .map(|p| p.page.saturating_add_signed(delta))
    }) {
        set_page(hwnd, value);
    }
}
fn step(hwnd: HWND, offset: isize) {
    PANEL.with(|s| {
        if let Some(p) = s.borrow_mut().as_mut() {
            p.selected = p
                .selected
                .saturating_add_signed(offset)
                .min(p.tiles.len().saturating_sub(1));
            p.page = p.selected / grid(p).capacity();
        }
    });
    refresh(hwnd);
}
pub(super) fn show_hidden(hwnd: HWND) {
    let edit = PANEL.with(|p| p.borrow().as_ref().map(|p| p.edit));
    if let Some(edit) = edit {
        unsafe {
            SetWindowTextW(edit, wide("").as_ptr());
        }
    }
    internal_view(hwnd, 0, true);
}

unsafe fn drop_tile(hwnd: HWND, from: usize, to: usize) {
    let Some((keys, src, dst, folder)) = PANEL.with(|s| {
        let s = s.borrow();
        let p = s.as_ref()?;
        let src = p.tiles.get(from)?.key.clone();
        let dst = p.tiles.get(to)?.key.clone();
        Some((
            p.tiles.iter().map(|t| t.key.clone()).collect::<Vec<_>>(),
            src,
            dst,
            p.folder,
        ))
    }) else {
        return;
    };
    change(hwnd, |lib| {
        if let (TileKey::App(id), TileKey::Folder(target)) = (&src, &dst) {
            lib.move_to(id, *target);
        } else if from != to {
            lib.reorder(&keys, &src, &dst);
        }
        if folder != 0 && matches!(&src, TileKey::Folder(_)) {
            return Err("Không hỗ trợ thư mục lồng nhau.");
        }
        Ok(())
    });
}

unsafe extern "system" fn edit_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_KEYDOWN {
        let empty = PANEL.with(|s| s.borrow().as_ref().is_some_and(|p| p.input.is_empty()));
        if matches!(
            w as u16,
            VK_UP | VK_DOWN | VK_RETURN | VK_ESCAPE | VK_PRIOR | VK_NEXT | VK_APPS | VK_F6
        ) || (empty && matches!(w as u16, VK_LEFT | VK_RIGHT))
        {
            PostMessageW(GetParent(hwnd), EDIT_KEY, w, 0);
            return 0;
        }
    }
    if msg == WM_CHAR && matches!(w, 13 | 27) {
        return 0;
    }
    CallWindowProcW(EDIT_PROC.get(), hwnd, msg, w, l)
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint_full(hwnd);
            0
        }
        #[cfg(test)]
        WM_PRINTCLIENT => {
            render_full(hwnd, w as HDC, true);
            0
        }
        WM_CTLCOLOREDIT => {
            let dark = PANEL.with(|p| p.borrow().as_ref().is_some_and(|s| s.dark));
            let bg = if dark { 0x00373230 } else { 0x00f9f6f4 };
            SetTextColor(w as HDC, if dark { 0x00f8f5f2 } else { 0x0024211d });
            SetBkColor(w as HDC, bg);
            SetDCBrushColor(w as HDC, bg);
            GetStockObject(DC_BRUSH) as LRESULT
        }
        WM_COMMAND if w & 0xffff == EDIT && (w >> 16) & 0xffff == EN_CHANGE as usize => {
            let text = read_edit(l as HWND, 256);
            let dirty = PANEL.with(|s| {
                let mut s = s.borrow_mut();
                let Some(p) = s.as_mut() else {
                    return false;
                };
                if p.input == text {
                    return false;
                }
                p.input = text;
                p.tiles.clear();
                p.page = 0;
                p.selected = 0;
                p.hover = None;
                p.error.clear();
                p.generation = p.generation.wrapping_add(1);
                p.search_rx = None;
                p.accept_pending = false;
                p.search_settled = false;
                true
            });
            if dirty {
                SetTimer(hwnd, SEARCH, 90, None);
                refresh(hwnd);
            }
            0
        }
        CATALOG_CHANGE => {
            // Do not read the shared notification payload; any scoped change
            // invalidates the snapshot. Merge bursts of shortcut writes.
            catalog::invalidate();
            SetTimer(hwnd, SHELL_REFRESH, 450, None);
            0
        }
        WM_TIMER if w == SHELL_REFRESH => {
            KillTimer(hwnd, SHELL_REFRESH);
            if PANEL.with(|p| p.borrow().as_ref().is_some_and(|p| p.catalog.is_some())) {
                SetTimer(hwnd, SHELL_REFRESH, 450, None);
            } else {
                refresh_catalog(hwnd);
            }
            0
        }
        WM_TIMER if w == SEARCH => {
            KillTimer(hwnd, SEARCH);
            rebuild(hwnd, true);
            0
        }
        WM_TIMER if w == POLL => {
            let (icons, apps, result, busy) = PANEL.with(|s| {
                let mut s = s.borrow_mut();
                let Some(p) = s.as_mut() else {
                    return (false, false, None, false);
                };
                let mut icons = false;
                let grid_capacity = grid(p).capacity();
                while let Ok((key, image)) = p.icon_rx.try_recv() {
                    p.pending.remove(&key);
                    if p.icons.len() >= 512 {
                        p.icons.retain(|(path, _), _| {
                            p.tiles
                                .iter()
                                .skip(p.page * grid_capacity)
                                .take(grid_capacity)
                                .any(|tile| &tile.entry.path == path || tile.preview.contains(path))
                        });
                    }
                    p.icons.insert(key, image);
                    icons = true;
                }
                let mut apps = false;
                if let Some(rx) = &p.catalog {
                    match rx.try_recv() {
                        Ok(rows) => {
                            p.apps = rows;
                            p.catalog = None;
                            apps = true;
                        }
                        Err(mpsc::TryRecvError::Disconnected) => p.catalog = None,
                        Err(mpsc::TryRecvError::Empty) => {}
                    }
                }
                let mut result = None;
                if let Some(rx) = &p.search_rx {
                    match rx.try_recv() {
                        Ok(rows) => {
                            result = Some(rows);
                            p.search_rx = None;
                        }
                        Err(mpsc::TryRecvError::Disconnected) => p.search_rx = None,
                        Err(mpsc::TryRecvError::Empty) => {}
                    }
                }
                (
                    icons,
                    apps,
                    result,
                    p.catalog.is_some() || !p.pending.is_empty() || p.search_rx.is_some(),
                )
            });
            if !busy {
                KillTimer(hwnd, POLL);
            }
            if apps {
                rebuild(hwnd, false);
            } else if let Some((gen, tiles)) = result {
                apply_rows(hwnd, gen, tiles);
            } else if icons {
                refresh(hwnd);
            }
            0
        }
        WM_ACTIVATE if w & 0xffff == WA_INACTIVE as usize => {
            if !MODAL.get() {
                dismiss(false);
            }
            0
        }
        WM_CLOSE => {
            dismiss(true);
            0
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED | WM_DWMCOMPOSITIONCHANGED => {
            let dark = dark();
            let glass = acrylic(hwnd, dark);
            PANEL.with(|s| {
                if let Some(p) = s.borrow_mut().as_mut() {
                    p.dark = dark;
                    p.acrylic = glass;
                }
            });
            refresh(hwnd);
            0
        }
        WM_DPICHANGED | WM_DISPLAYCHANGE => {
            let point = PANEL.with(|p| p.borrow().as_ref().map(|p| p.point));
            if let Some(point) = point {
                place(hwnd, point);
            }
            0
        }
        WM_KEYDOWN | EDIT_KEY => {
            let (n, escape, selected) = PANEL.with(|s| {
                s.borrow().as_ref().map_or((1, false, 0), |p| {
                    (
                        grid(p).columns as isize,
                        p.hidden || p.folder != 0,
                        p.selected,
                    )
                })
            });
            match w as u16 {
                VK_ESCAPE if escape => internal_view(hwnd, 0, false),
                VK_ESCAPE => dismiss(true),
                VK_RETURN => {
                    let waiting = PANEL.with(|p| {
                        let mut s = p.borrow_mut();
                        let Some(p) = s.as_mut() else {
                            return false;
                        };
                        let waiting = !p.search_settled;
                        p.accept_pending = waiting;
                        waiting
                    });
                    if waiting {
                        KillTimer(hwnd, SEARCH);
                        rebuild(hwnd, true);
                    } else {
                        action(hwnd, Hit::Tile(selected));
                    }
                }
                VK_UP => step(hwnd, -n),
                VK_DOWN => step(hwnd, n),
                VK_LEFT => step(hwnd, -1),
                VK_RIGHT => step(hwnd, 1),
                VK_PRIOR => page(hwnd, -1),
                VK_NEXT => page(hwnd, 1),
                VK_F6 => action(hwnd, Hit::Compact),
                VK_APPS => tile_menu(hwnd, selected, POINT { x: 28, y: 180 }),
                _ => {}
            }
            0
        }
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            page(hwnd, if (w >> 16) as i16 > 0 { -1 } else { 1 });
            0
        }
        WM_MOUSEMOVE => {
            let changed = PANEL.with(|s| {
                let mut s = s.borrow_mut();
                let Some(p) = s.as_mut() else {
                    return false;
                };
                let next = hit_at(p, coords(l));
                let changed = p.hover != next;
                p.hover = next;
                changed
            });
            if changed {
                refresh(hwnd);
            }
            0
        }
        WM_LBUTTONDOWN => {
            let point = coords(l);
            let pressed = PANEL.with(|s| {
                let mut s = s.borrow_mut();
                let Some(p) = s.as_mut() else {
                    return false;
                };
                p.pressed = hit_at(p, point).map(|h| (h, point));
                p.pressed.is_some()
            });
            if pressed {
                SetCapture(hwnd);
            }
            0
        }
        WM_CAPTURECHANGED => {
            PANEL.with(|s| {
                if let Some(p) = s.borrow_mut().as_mut() {
                    p.pressed = None;
                }
            });
            0
        }
        WM_LBUTTONUP => {
            let point = coords(l);
            let targets = PANEL.with(|s| {
                let mut s = s.borrow_mut();
                let p = s.as_mut()?;
                let (start, origin) = p.pressed.take()?;
                let end = hit_at(p, point)?;
                let far = (point.x - origin.x).abs().max((point.y - origin.y).abs()) > px(7, p.dpi);
                Some((start, end, far, p.editing, p.folder))
            });
            if GetCapture() == hwnd {
                ReleaseCapture();
            }
            if let Some((start, end, far, editing, folder)) = targets {
                match (start, end, far) {
                    (Hit::Tile(a), Hit::Tile(b), true) if editing => drop_tile(hwnd, a, b),
                    (Hit::Tile(a), Hit::Next | Hit::Prev, true) if editing => {
                        let dir = if end == Hit::Next { 1 } else { -1 };
                        if let Some(dest) = PANEL.with(|s| {
                            let s = s.borrow();
                            let p = s.as_ref()?;
                            let page = p.page.saturating_add_signed(dir).min(count_pages(p) - 1);
                            (page != p.page).then_some((page, page * grid(p).capacity()))
                        }) {
                            drop_tile(hwnd, a, dest.1);
                            set_page(hwnd, dest.0);
                        }
                    }
                    (Hit::Tile(a), Hit::Back, true) if editing && folder != 0 => {
                        let tile = PANEL
                            .with(|s| s.borrow().as_ref().and_then(|p| p.tiles.get(a).cloned()));
                        if let Some(Tile {
                            key: TileKey::App(id),
                            ..
                        }) = tile
                        {
                            change(hwnd, |l| {
                                l.move_to(&id, 0);
                                Ok(())
                            });
                        }
                    }
                    (a, b, false) if a == b => action(hwnd, b),
                    _ => {}
                }
            }
            0
        }
        WM_RBUTTONUP => {
            let point = coords(l);
            if let Some(Hit::Tile(i)) =
                PANEL.with(|s| s.borrow().as_ref().and_then(|p| hit_at(p, point)))
            {
                tile_menu(hwnd, i, point);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

pub(super) fn open(point: POINT, query: String, prior: HWND, manage: bool) {
    open_native(point, query, prior, manage, true);
}

fn open_native(point: POINT, query: String, prior: HWND, manage: bool, activate: bool) {
    if !enabled() {
        return;
    }
    unsafe {
        if let Some(hwnd) = window() {
            let same = PANEL.with(|s| {
                s.borrow().as_ref().is_some_and(|p| {
                    MonitorFromPoint(p.point, MONITOR_DEFAULTTONEAREST)
                        == MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST)
                })
            });
            if manage {
                show_hidden(hwnd);
                return;
            }
            if same {
                dismiss(true);
                return;
            }
            place(hwnd, point);
            return;
        }
        let Some((area, _)) = estimate(point) else {
            return;
        };
        let instance = GetModuleHandleW(ptr::null());
        let c = wide(CLASS);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance,
            lpszClassName: c.as_ptr(),
            hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let input = InputHandoff::capture();
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            c.as_ptr(),
            wide("TLauncher Spotlight · Thư viện ứng dụng").as_ptr(),
            WS_POPUP | WS_CLIPCHILDREN,
            area.left,
            area.top,
            1,
            1,
            ptr::null_mut(),
            ptr::null_mut(),
            instance,
            ptr::null_mut(),
        );
        if hwnd.is_null() {
            return;
        }
        let edit = CreateWindowExW(
            0,
            wide("EDIT").as_ptr(),
            wide("").as_ptr(),
            WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL as u32,
            0,
            0,
            1,
            1,
            hwnd,
            EDIT as HMENU,
            instance,
            ptr::null_mut(),
        );
        if edit.is_null() {
            DestroyWindow(hwnd);
            return;
        }
        let old = SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as *const () as isize);
        if old == 0 {
            DestroyWindow(hwnd);
            return;
        }
        EDIT_PROC.set(Some(std::mem::transmute::<
            isize,
            unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
        >(old)));
        SendMessageW(edit, 0x00c5, 256, 0);
        SendMessageW(
            edit,
            0x1501,
            1,
            wide("Tìm ứng dụng, tệp, công cụ…").as_ptr() as LPARAM,
        );
        let d = dark();
        let glass = acrylic(hwnd, d);
        let (icon_tx, icon_rx) = icon_worker();
        let apps = catalog::snapshot(true);
        PANEL.with(|p| {
            p.replace(Some(Panel {
                hwnd,
                edit,
                prior,
                point,
                width: 0,
                height: 0,
                dpi: 96,
                acrylic: glass,
                dark: d,
                font: ptr::null_mut(),
                apps,
                catalog: catalog::request(true, false),
                watch: None,
                input: query.clone(),
                category: Category::All,
                folder: 0,
                hidden: manage,
                generation: 0,
                search_rx: None,
                accept_pending: false,
                search_settled: false,
                editing: manage,
                page: 0,
                selected: 0,
                tiles: Vec::new(),
                error: String::new(),
                hover: None,
                pressed: None,
                icon_tx,
                icon_rx,
                icons: HashMap::new(),
                pending: HashSet::new(),
                icon_rev: crate::app_icons::revision(),
            }))
        });
        SetWindowTextW(edit, wide(&query).as_ptr());
        place(hwnd, point);
        if activate {
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
            SetFocus(edit);
            input.finish(hwnd, "Spotlight library");
        } else {
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        SendMessageW(edit, 0x00b1, usize::MAX, -1);
        SetTimer(hwnd, POLL, 40, None);
        let watch = catalog::ShellWatch::new(hwnd, CATALOG_CHANGE);
        PANEL.with(|p| {
            if let Some(p) = p.borrow_mut().as_mut() {
                p.watch = Some(watch);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_keeps_tiles_inside_landscape_portrait_and_high_dpi() {
        for (w, h) in [
            (2560, 1080),
            (1080, 1920),
            (3840, 2160),
            (3440, 1440),
            (1024, 768),
        ] {
            for dpi in [96, 120, 144, 192] {
                let g = Grid::new(w, h, dpi, &Preferences::default());
                assert!(g.capacity() >= 2);
                let a = g.tiles(0);
                let b = g.tiles(g.capacity() - 1);
                assert!(a.left >= 0 && a.top >= 0 && b.right <= w && b.bottom <= h);
                assert!(a.right - a.left >= g.icon && a.bottom - a.top >= g.icon);
                assert_eq!((g.capacity() + 1).div_ceil(g.capacity()), 2);
            }
        }
    }
    #[test]
    fn categories_filter_launchable_apps_files_and_system_actions() {
        let entry = |kind| Entry {
            name: String::new(),
            path: PathBuf::new(),
            kind,
            detail: String::new(),
        };
        assert!(Category::Apps.accepts(&entry("Pinned")));
        assert!(!Category::Apps.accepts(&entry("File")));
        assert!(Category::Files.accepts(&entry("Web shortcut")));
        assert!(Category::System.accepts(&entry("System")));
        assert!(!Category::System.accepts(&entry("Application")));
    }
}

#[cfg(test)]
#[path = "fullscreen_tests.rs"]
pub(super) mod native_tests;
