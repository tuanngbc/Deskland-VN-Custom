//! Native fixture on a non-input desktop. No SwitchDesktop, owner profile,
//! global keystrokes, app launch, or capture of the user's desktop.
use super::*;
use std::time::{Duration, Instant};
use windows_sys::Win32::System::StationsAndDesktops::*;
use windows_sys::Win32::System::Threading::*;

pub(in crate::launcher) struct Desktop {
    previous: HDESK,
    isolated: HDESK,
}
impl Desktop {
    pub(in crate::launcher) unsafe fn isolated() -> Self {
        let previous = GetThreadDesktop(GetCurrentThreadId());
        let name = wide(&format!(
            "TLauncherSpotlightTest-{}-{}",
            std::process::id(),
            GetCurrentThreadId()
        ));
        let isolated = CreateDesktopW(
            name.as_ptr(),
            ptr::null(),
            ptr::null(),
            0,
            0x10000000,
            ptr::null(),
        );
        assert!(
            !isolated.is_null(),
            "CreateDesktopW: {:?}",
            std::io::Error::last_os_error()
        );
        if SetThreadDesktop(isolated) == 0 {
            CloseDesktop(isolated);
            panic!("SetThreadDesktop: {:?}", std::io::Error::last_os_error());
        }
        assert_ne!(isolated, previous);
        Self { previous, isolated }
    }
}
impl Drop for Desktop {
    fn drop(&mut self) {
        dismiss(false);
        // Compact intentionally keeps its one hidden HWND in production.
        let old = STATE.with(|p| p.borrow_mut().take());
        unsafe {
            if let Some(p) = old {
                DestroyWindow(p.hwnd);
            }
            SetThreadDesktop(self.previous);
            CloseDesktop(self.isolated);
        }
    }
}
unsafe fn resources() -> (u32, u32, u32) {
    let p = GetCurrentProcess();
    let mut handles = 0;
    GetProcessHandleCount(p, &mut handles);
    (GetGuiResources(p, 0), GetGuiResources(p, 1), handles)
}
unsafe fn pump() {
    let mut msg: MSG = std::mem::zeroed();
    while PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}
fn wait_for(mut done: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(3);
    while !done() {
        assert!(Instant::now() < end, "timed out waiting for native update");
        unsafe {
            pump();
        }
        std::thread::sleep(Duration::from_millis(4));
    }
}
fn fixture(name: &str, file: &str, kind: &'static str) -> Entry {
    let win = std::env::var("WINDIR").map_or_else(|_| PathBuf::from(r"C:\Windows"), PathBuf::from);
    Entry {
        name: name.into(),
        path: win.join("System32").join(file),
        kind,
        detail: "Isolated fixture".into(),
    }
}
fn use_fixture_entries(hwnd: HWND) {
    let mut apps = [
        ("Ghi chú", "notepad.exe"),
        ("Máy tính", "calc.exe"),
        ("Trình dựng video", "mspaint.exe"),
        ("Màn hình", "magnify.exe"),
        ("Windows Tools", "taskmgr.exe"),
        ("PowerShell", "WindowsPowerShell\\v1.0\\powershell.exe"),
        ("Character Map", "charmap.exe"),
        ("Narrator", "narrator.exe"),
        ("WordPad", "write.exe"),
        ("Bàn phím", "osk.exe"),
        ("Trình theo dõi", "perfmon.exe"),
        ("Trình cắt", "SnippingTool.exe"),
    ]
    .into_iter()
    .map(|(name, file)| fixture(name, file, "Application"))
    .collect::<Vec<_>>();
    apps.push(fixture("Tài liệu demo", "fixture-not-opened.txt", "File"));
    PANEL.with(|p| {
        if let Some(p) = p.borrow_mut().as_mut() {
            p.catalog = None;
            p.apps = apps;
        }
    });
    rebuild(hwnd, true);
}
unsafe fn screenshot(hwnd: HWND, name: &str) {
    let Some(dir) = std::env::var_os("TUANPC_SPOTLIGHT_EVIDENCE") else {
        return;
    };
    let dir = PathBuf::from(dir);
    assert!(dir.is_dir());
    let Some((w, h, edit, dpi)) = PANEL.with(|p| {
        p.borrow()
            .as_ref()
            .map(|p| (p.width, p.height, p.edit, p.dpi))
    }) else {
        return;
    };
    let dc = CreateCompatibleDC(ptr::null_mut());
    assert!(!dc.is_null());
    let mut info: BITMAPINFO = std::mem::zeroed();
    info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = w;
    info.bmiHeader.biHeight = -h;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    let mut bits = ptr::null_mut();
    let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0);
    assert!(!bitmap.is_null() && !bits.is_null());
    let previous = SelectObject(dc, bitmap);
    SendMessageW(hwnd, WM_PRINTCLIENT, dc as WPARAM, 0);
    let mut r: RECT = std::mem::zeroed();
    GetWindowRect(edit, &mut r);
    let mut at = POINT {
        x: r.left,
        y: r.top,
    };
    ScreenToClient(hwnd, &mut at);
    let font = crate::native_text::font_pixels(px(15, dpi), 450);
    let query = read_edit(edit, 256);
    let (label, ink) = if query.is_empty() {
        ("Tìm ứng dụng, tệp, công cụ…", 0x00635b56)
    } else {
        (query.as_str(), 0x004044bc)
    };
    text(
        dc,
        label,
        rect(at.x, at.y, at.x + r.right - r.left, at.y + r.bottom - r.top),
        font,
        ink,
        false,
    );
    DeleteObject(font);
    GdiFlush();
    let mut rgba =
        std::slice::from_raw_parts(bits.cast::<u8>(), (w as usize) * (h as usize) * 4).to_vec();
    SelectObject(dc, previous);
    DeleteObject(bitmap);
    DeleteDC(dc);
    for p in rgba.chunks_exact_mut(4) {
        p.swap(0, 2);
        p[3] = 255;
    }
    let dest = dir.join(name);
    assert!(!dest.exists());
    image::save_buffer(dest, &rgba, w as u32, h as u32, image::ColorType::Rgba8).unwrap();
}

#[test]
#[ignore = "optional Windows native fixture on a private non-input desktop"]
fn native_isolated_library_search_navigation_and_100_toggles() {
    unsafe {
        let _desktop = Desktop::isolated();
        let p = POINT { x: 0, y: 0 };
        configure(Preferences {
            theme: "light".into(),
            ..Preferences::default()
        });
        let cold = Instant::now();
        let (bounds, dpi) = estimate(p).expect("native display geometry");
        println!(
            "SPOTLIGHT_NATIVE display={},{},{},{} dpi={dpi}",
            bounds.left, bounds.top, bounds.right, bounds.bottom
        );
        open(p, String::new(), ptr::null_mut(), false);
        let hwnd = window().expect("created native overlay");
        println!(
            "SPOTLIGHT_NATIVE cold_hwnd_open_ms={:.3}",
            cold.elapsed().as_secs_f64() * 1000.0
        );
        pump();
        let edit = PANEL.with(|p| p.borrow().as_ref().unwrap().edit);
        assert_eq!(GetParent(edit), hwnd);
        let mut actual: RECT = std::mem::zeroed();
        GetWindowRect(hwnd, &mut actual);
        assert_eq!(
            (actual.left, actual.top, actual.right, actual.bottom),
            (bounds.left, bounds.top, bounds.right, bounds.bottom)
        );
        assert_ne!(
            GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW,
            0
        );
        use_fixture_entries(hwnd);
        PANEL.with(|p| {
            let p=p.borrow();let p=p.as_ref().unwrap();
            assert!(!p.glass.is_empty());
            assert!(p.glass.chunks_exact(4).all(|v|v[3]==255));
            assert!(p.glass.chunks_exact(4).any(|v|v[..3]!=p.glass[..3]));
        });
        println!("SPOTLIGHT_NATIVE backdrop=static_snapshot_composite fixture=synthetic_gradient opaque=true");
        // Let the real icon service finish on the fixture paths before evidence.
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until {
            UpdateWindow(hwnd);
            pump();
            if PANEL.with(|p| {
                p.borrow()
                    .as_ref()
                    .is_some_and(|p| p.icons.values().any(Option::is_some) && p.pending.is_empty())
            }) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        screenshot(hwnd, "library-fixture-light.png");
        // Stable hit testing, bounded repaint and no repaint for movement
        // within the same tile, using the actual native window procedure.
        let tile=PANEL.with(|p|grid(p.borrow().as_ref().unwrap()).tiles(0));
        let x=tile.left+5; let y=tile.top+5;
        let mouse=|x:i32,y:i32| (x as u16 as usize | ((y as u16 as usize)<<16)) as LPARAM;
        PANEL.with(|p|{let mut p=p.borrow_mut();let p=p.as_mut().unwrap();p.hover=None;p.hover_from=None;p.hover_started=None;});
        ValidateRect(hwnd,ptr::null());
        SendMessageW(hwnd,WM_MOUSEMOVE,0,mouse(x,y));
        let mut dirty:RECT=std::mem::zeroed();
        assert_ne!(GetUpdateRect(hwnd,&mut dirty,0),0);
        assert!(dirty.left>=tile.left && dirty.top>=tile.top && dirty.right<=tile.right && dirty.bottom<=tile.bottom);
        UpdateWindow(hwnd);
        SendMessageW(hwnd,WM_MOUSEMOVE,0,mouse(x+1,y+1));
        assert_eq!(GetUpdateRect(hwnd,&mut dirty,0),0);
        std::thread::sleep(Duration::from_millis(130));
        pump();
        assert!(PANEL.with(|p|p.borrow().as_ref().unwrap().hover_started.is_none()));
        screenshot(hwnd,"hover-fixture-light.png");
        SendMessageW(hwnd,WM_MOUSELEAVE,0,0);
        UpdateWindow(hwnd);
        println!("SPOTLIGHT_NATIVE hover_dirty_region=PASS stable_same_tile=PASS");
        action(hwnd, Hit::Edit);
        screenshot(hwnd, "edit-fixture-light.png");
        action(hwnd, Hit::Edit);
        SetWindowTextW(edit, wide("tài liệu").as_ptr());
        SendMessageW(hwnd, WM_TIMER, SEARCH, 0);
        let search = Instant::now();
        SetWindowTextW(edit, wide("dựng VIDEO").as_ptr());
        SendMessageW(hwnd, WM_TIMER, SEARCH, 0);
        wait_for(|| {
            PANEL.with(|p| {
                p.borrow().as_ref().is_some_and(|p| {
                    p.tiles.len() == 1 && p.tiles[0].entry.name == "Trình dựng video"
                })
            })
        });
        println!(
            "SPOTLIGHT_NATIVE isolated_fixture_search_roundtrip_ms={:.3}",
            search.elapsed().as_secs_f64() * 1000.0
        );
        screenshot(hwnd, "search-fixture.png");
        action(hwnd, Hit::Tab(Category::Files));
        wait_for(|| PANEL.with(|p| p.borrow().as_ref().is_some_and(|p| p.search_rx.is_none())));
        assert!(PANEL.with(|p| p.borrow().as_ref().unwrap().tiles.is_empty()));
        SetWindowTextW(edit, wide("").as_ptr());
        SendMessageW(hwnd, WM_TIMER, SEARCH, 0);
        action(hwnd, Hit::Tab(Category::Apps));
        change(hwnd, |lib| {
            let folder = lib.create_folder("Làm việc").unwrap();
            for app in [
                fixture("Ghi chú", "notepad.exe", "Application"),
                fixture("Máy tính", "calc.exe", "Application"),
            ] {
                lib.move_to(&catalog::identity(&app.path), folder);
            }
            Ok(())
        });
        let (folder, first) = PANEL.with(|p| {
            let p = p.borrow();
            let p = p.as_ref().unwrap();
            let (first, folder) = p
                .tiles
                .iter()
                .enumerate()
                .find_map(|(i, t)| {
                    if let TileKey::Folder(id) = t.key {
                        Some((i, id))
                    } else {
                        None
                    }
                })
                .unwrap();
            (folder, first)
        });
        action(hwnd, Hit::Tile(first));
        assert!(PANEL.with(|p| {
            p.borrow()
                .as_ref()
                .is_some_and(|p| p.folder == folder && p.tiles.len() == 2)
        }));
        let mut cfg = Preferences::default();
        cfg.theme = "dark".into();
        configure(cfg);
        SendMessageW(hwnd, WM_THEMECHANGED, 0, 0);
        screenshot(hwnd, "folder-fixture-dark.png");
        dismiss(false);
        pump();
        let before = resources();
        let mut elapsed = Vec::new();
        // Focus/IME activation on a non-input desktop is a different OS path;
        // stress the real native creation, show and destroy without activating it.
        for _ in 0..100 {
            let start = Instant::now();
            open_native(p, String::new(), ptr::null_mut(), false, false);
            let new = window().expect("one overlay");
            elapsed.push(start.elapsed().as_secs_f64() * 1000.0);
            dismiss(false);
            assert!(window().is_none());
            assert_eq!(IsWindow(new), 0);
        }
        pump();
        let after = resources();
        elapsed.sort_by(f64::total_cmp);
        println!(
            "SPOTLIGHT_NATIVE toggles=100 gdi_user_handles_before={before:?} after={after:?} warm_nonactivating_open_median_ms={:.3} p95_ms={:.3}",
            elapsed[50], elapsed[94]
        );
        assert!(after.0 <= before.0 + 2 && after.1 <= before.1 + 2 && after.2 <= before.2 + 8);
    }
}
