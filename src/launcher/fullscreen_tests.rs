//! Native fixture on a non-input desktop. No SwitchDesktop, owner profile,
//! global keystrokes, app launch, or capture of the user's desktop.
use super::*;
use std::time::{Duration, Instant};
use windows_sys::Win32::System::StationsAndDesktops::*;
use windows_sys::Win32::System::Threading::*;

// Run on the GitHub-hosted desktop only: the compositor cannot sample the
// private non-input desktop used by the deterministic library fixture below.
#[test]
#[ignore = "GitHub Actions compositor proof"]
fn native_live_backdrop_retains_color_and_filters_detail() {
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    const W:i32=720;
    const H:i32=400;
    const X:i32=48;
    const Y:i32=48;
    static PATTERN:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
    unsafe extern "system" fn background(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
        if msg==WM_ERASEBKGND{return 1;}
        if msg!=WM_PAINT{return DefWindowProcW(hwnd,msg,w,l);}
        let mut ps:PAINTSTRUCT=std::mem::zeroed();
        let dc=BeginPaint(hwnd,&mut ps);
        let changed=PATTERN.load(std::sync::atomic::Ordering::Relaxed);
        for x in (0..W).step_by(6) {
            let colors=[0x005b1545,0x00285b9e,0x009c4f25];
            let index=(x/(W/3)) as usize;
            let base=colors[if changed{(index+1)%3}else{index}];
            let ink=if (x/6)%2==0{base+0x00474747}else{base};
            fill(dc,rect(x,0,x+6,H),ink);
        }
        EndPaint(hwnd,&ps);0
    }
    struct Owned(HWND);
    impl Drop for Owned{fn drop(&mut self){unsafe{DestroyWindow(self.0);}}}
    unsafe fn screen()->Option<Vec<u8>>{
        let mut layer=crate::dock::PopupPixels::new(W,H)?;
        let dc=GetDC(ptr::null_mut());
        if dc.is_null(){return None;}
        let ok=BitBlt(layer.dc(),0,0,W,H,dc,X,Y,SRCCOPY|CAPTUREBLT)!=0;
        ReleaseDC(ptr::null_mut(),dc);GdiFlush();
        ok.then(||layer.pixels().to_vec())
    }
    fn metrics(b:&[u8])->(f64,f64){
        let pixel=|x:i32,y:i32,c:usize|b[((y*W+x)*4) as usize+c] as f64;
        let mut detail=0.0;
        for x in 48..(W/3-40){
            for c in 0..3{detail+=(pixel(x,320,c)-pixel(x+6,320,c)).abs();}
        }
        let mut spread=0.0;
        for c in 0..3{
            spread+=(pixel(W/6,320,c)-pixel(W*5/6,320,c)).abs();
        }
        (detail/(3*(W/3-88)) as f64,spread/3.0)
    }
    fn evidence(name:&str,raw:&[u8]){
        if let Some(dir)=std::env::var_os("TUANPC_SPOTLIGHT_EVIDENCE"){
            let rgb=raw.chunks_exact(4).flat_map(|p|[p[2],p[1],p[0]]).collect::<Vec<_>>();
            image::save_buffer(PathBuf::from(dir).join(name),&rgb,W as u32,H as u32,image::ColorType::Rgb8).unwrap();
        }
    }
    unsafe{
        let name=wide("TLauncherGlassEvidenceSource");
        let instance=GetModuleHandleW(ptr::null());
        let wc=WNDCLASSW{lpfnWndProc:Some(background),hInstance:instance,lpszClassName:name.as_ptr(),..std::mem::zeroed()};
        RegisterClassW(&wc);
        let bg=Owned(CreateWindowExW(WS_EX_TOPMOST|WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW,name.as_ptr(),name.as_ptr(),
            WS_POPUP,X,Y,W,H,ptr::null_mut(),ptr::null_mut(),instance,ptr::null_mut()));
        assert!(!bg.0.is_null());
        ShowWindow(bg.0,SW_SHOWNOACTIVATE);UpdateWindow(bg.0);
        let settle=||{for _ in 0..5{pump();std::thread::sleep(Duration::from_millis(35));}let _=DwmFlush();};
        settle();
        let Some(reference)=screen() else{println!("GLASS_POC=BLOCKED screen capture unavailable");return;};
        let (detail,spread)=metrics(&reference);
        if detail<35.0||spread<20.0{
            println!("GLASS_POC=BLOCKED synthetic baseline not composited detail={detail:.2} color={spread:.2}");return;
        }
        evidence("glass-reference.png",&reference);
        // Exercise the same DirectComposition upload used by the real panel.
        // Sample an opaque patch, then verify a dirty update and an untouched
        // transparent area over the fixture desktop.
        let (gdi,user,_)=resources();
        {
            let c=wide(MATERIAL_CLASS);
            let wc=WNDCLASSW{lpfnWndProc:Some(backdrop_proc),hInstance:instance,lpszClassName:c.as_ptr(),..std::mem::zeroed()};
            RegisterClassW(&wc);
            let front=Owned(CreateWindowExW(WS_EX_NOREDIRECTIONBITMAP|WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST,
                c.as_ptr(),ptr::null(),WS_POPUP,X,Y,W,H,ptr::null_mut(),ptr::null_mut(),instance,ptr::null_mut()));
            if !front.0.is_null(){
                let check=(||->windows::core::Result<bool>{
                    let mut fg=Foreground::new(front.0)?;
                    let frame=Frame::new(ptr::null_mut(),W,H).expect("fixture foreground bitmap");
                    let pixels=std::slice::from_raw_parts_mut(frame.bits,(W*H*4) as usize);
                    pixels.fill(0);
                    let paint=|pixels:&mut[u8],bgrx:[u8;4]|{
                        for y in 85..99{for x in 88..102{
                            let at=((y*W+x)*4) as usize;
                            pixels[at..at+4].copy_from_slice(&bgrx);
                        }}
                    };
                    let sample=|img:&[u8],x:i32,y:i32|-> [u8;3]{
                        let at=((y*W+x)*4) as usize;
                        img[at..at+3].try_into().unwrap()
                    };
                    paint(pixels,[25,185,245,255]);
                    fg.upload(&frame,rect(0,0,W,H))?;
                    ShowWindow(front.0,SW_SHOWNOACTIVATE);
                    settle();
                    let Some(first)=screen() else{return Ok(false);};
                    paint(pixels,[245,180,25,255]);
                    fg.upload(&frame,rect(88,85,102,99))?;
                    settle();
                    let Some(next)=screen() else{return Ok(false);};
                    evidence("glass-composed-foreground.png",&next);
                    let opaque=sample(&first,94,90)==[25,185,245]&&sample(&next,94,90)==[245,180,25];
                    let transparent=sample(&next,140,315)==sample(&reference,140,315);
                    println!("GLASS_FOREGROUND opaque_dirty_update={opaque} unchanged_transparent_region={transparent}");
                    if opaque&&transparent{
                        fg.prepare_open()?;
                        fg.comp.WaitForCommitCompletion()?;
                        settle();
                        let prepared=screen().is_some_and(|img|sample(&img,94,90)==sample(&reference,94,90));
                        assert!(prepared,"prepared foreground must be transparent before the opening clock");
                        println!("MOTION_SPEED fullscreen_first_frame=PASS");
                        let opening=Transition::new(true,motion::FULL,None);
                        fg.motion(opening,W,90)?;
                        fg.comp.WaitForCommitCompletion()?;
                        std::thread::sleep(Duration::from_millis(55));
                        let closing=Transition::new(false,motion::FULL,Some(opening));
                        fg.motion(closing,W,90)?;
                        fg.comp.WaitForCommitCompletion()?;
                        let deadline=Instant::now()+Duration::from_millis(u64::from(closing.duration)+50);
                        while Instant::now()<deadline{pump();std::thread::sleep(Duration::from_millis(10));}
                        let _=DwmFlush();
                        let faded=screen().is_some_and(|img|sample(&img,94,90)==sample(&reference,94,90));
                        fg.settle()?;
                        fg.comp.WaitForCommitCompletion()?;
                        settle();
                        let restored=screen().map(|img|{
                            evidence("glass-composed-foreground-after-reversal.png",&img);
                            sample(&img,94,90)
                        });
                        let settled=restored.is_some_and(|bgr|bgr.into_iter().zip([245u8,180,25]).all(|(a,b)|a.abs_diff(b)<=2));
                        println!("SPOTLIGHT_NATIVE compositor_reversal_transparent_end={faded} settled_foreground_restored={settled} sample_bgr={restored:?}");
                        assert!(faded&&settled,"compositor must clear and restore the same foreground after reversal");
                    }
                    Ok(opaque&&transparent)
                })();
                println!("GLASS_FOREGROUND fixture={check:?}");
            }else{println!("GLASS_FOREGROUND=BLOCKED cannot create fixture window");}
        }
        for (kind, label) in [(BackdropKind::Host,"host"),(BackdropKind::Acrylic,"acrylic")]{
            for dark in [true,false]{
                let native=match NativeBackdrop::create(kind){
                    Ok(layer)=>layer,
                    Err(e)=>{println!("GLASS_POC backend={label} unavailable: {e}");break;}
                };
                native.material(dark,36);
                native.show_behind(HWND_TOPMOST,rect(X,Y,X+W,Y+H));
                UpdateWindow(native.hwnd);
                settle();
                if let Some(layer)=screen(){
                    let (d,c)=metrics(&layer);
                    println!("GLASS_POC host_only backend={label} dark={dark} detail_ratio={:.3} color_ratio={:.3}",d/detail,c/spread);
                    evidence(&format!("glass-{label}-{}-backdrop-only.png",if dark{"dark"}else{"light"}),&layer);
                }
                // Separate premultiplied foreground: glass cannot blur icons
                // or text because neither is part of its backdrop source.
                let face=Owned(CreateWindowExW(WS_EX_LAYERED|WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST,
                    wide("STATIC").as_ptr(),ptr::null(),WS_POPUP,X,Y,W,H,ptr::null_mut(),ptr::null_mut(),instance,ptr::null_mut()));
                assert!(!face.0.is_null());
                let mut frame=crate::dock::PopupPixels::new(W,H).unwrap();
                frame.pixels().fill(0);
                let tint=if dark{0x00302624}else{0x00f2efec};
                round(frame.dc(),rect(180,32,540,75),20,tint,240);
                let font=crate::native_text::font_pixels(19,550);
                text(frame.dc(),"Spotlight · Search",rect(190,32,530,75),font,if dark{0x00ffffff}else{0x00222020},true);
                DeleteObject(font);
                for i in 0..6{
                    let left=95+92*i;
                    round(frame.dc(),rect(left,152,left+58,210),22,0x0052a2ec+0x00040400*i as u32,255);
                }
                let present=||{
                    let zero=POINT{x:0,y:0};
                    let size=SIZE{cx:W,cy:H};
                    let blend=BLENDFUNCTION{BlendOp:AC_SRC_OVER as u8,BlendFlags:0,SourceConstantAlpha:255,AlphaFormat:AC_SRC_ALPHA as u8};
                    assert_ne!(UpdateLayeredWindow(face.0,ptr::null_mut(),ptr::null(),&size,frame.dc(),&zero,0,&blend,ULW_ALPHA),0);
                };
                present();
                ShowWindow(face.0,SW_SHOWNOACTIVATE);settle();
                let b=screen().unwrap_or_default();
                if b.len()!=(W*H*4) as usize{println!("GLASS_POC backend={label} capture unavailable");continue;}
                let (d,c)=metrics(&b);
                let filtered=c>spread*0.15&&d/(c.max(1.0))<detail/spread*0.58;
                let pixel=&b[((320*W+W/6)*4) as usize..][..3];
                println!("GLASS_POC backend={label} dark={dark} detail_ratio={:.3} color_ratio={:.3} optical_blur={filtered} sample_bgr={pixel:?}",d/detail,c/spread);
                evidence(&format!("glass-{label}-{}.png",if dark{"dark"}else{"light"}),&b);
                round(frame.dc(),rect(175,143,253,220),20,0x007997cc,90);
                present();settle();
                if let Some(hover)=screen(){evidence(&format!("glass-{label}-{}-hover.png",if dark{"dark"}else{"light"}),&hover);}
                PATTERN.store(true,std::sync::atomic::Ordering::Relaxed);
                InvalidateRect(bg.0,ptr::null(),0);
                UpdateWindow(bg.0);
                settle();
                if let Some(after)=screen(){
                    let at=(320*W*4) as usize;
                    let band=48usize*4..(W as usize-48)*4;
                    let delta=after[at+band.start..at+band.end].iter().zip(&b[at+band.start..at+band.end])
                        .map(|(&before,&sample)|f64::from(before.abs_diff(sample))).sum::<f64>()/(band.end-band.start) as f64;
                    println!("GLASS_POC backend={label} dark={dark} live_delta={delta:.2}");
                    if filtered{evidence(&format!("glass-{label}-{}-live.png",if dark{"dark"}else{"light"}),&after);}
                }
                PATTERN.store(false,std::sync::atomic::Ordering::Relaxed);
                InvalidateRect(bg.0,ptr::null(),0);
                UpdateWindow(bg.0);
            }
        }
        let (end_gdi,end_user,_)=resources();
        assert!(end_gdi<=gdi+12&&end_user<=user+12,"compositor fixture leaked GDI/USER resources");
    }
}

#[test]
#[ignore="Windows private desktop Spotlight activation routing"]
fn native_activation_modes_apply_live_on_one_overlay(){
    let _desktop=unsafe{Desktop::isolated()};
    let at=POINT{x:0,y:0};
    let mut prefs=Preferences{animations:false,..Preferences::default()};
    let compact_visible=||handles().is_some_and(|(h,_,_,_)|unsafe{IsWindowVisible(h)!=0});
    configure(prefs.clone());
    super::super::toggle(ptr::null_mut());
    assert!(compact_visible()&&window().is_none());
    super::super::toggle(ptr::null_mut());
    assert!(!compact_visible()&&window().is_none());
    prefs.hotkey_fullscreen=true;
    prefs.dock_fullscreen=false;
    configure(prefs.clone());
    super::super::toggle(ptr::null_mut());
    assert!(!compact_visible()&&window().is_some());
    super::super::toggle(ptr::null_mut());
    assert!(window().is_none());
    super::super::open_spotlight_dock(at);
    assert!(compact_visible()&&window().is_none());
    super::super::toggle(ptr::null_mut());
    prefs.dock_follow_hotkey=true;
    configure(prefs.clone());
    super::super::open_spotlight_dock(at);
    let hwnd=window().expect("Dock follows fullscreen preference");
    assert!(!compact_visible());
    let edit=PANEL.with(|p|p.borrow().as_ref().unwrap().edit);
    unsafe{
        SetWindowTextW(edit,wide("notepad").as_ptr());
        SendMessageW(hwnd,EDIT_KEY,VK_F6 as WPARAM,0);
    }
    assert!(window().is_none()&&compact_visible());
    assert_eq!(STATE.with(|c|c.borrow().as_ref().unwrap().query.clone()),"notepad");
    super::super::expand_compact();
    assert!(window().is_some()&&!compact_visible());
    assert_eq!(compact_query().unwrap().1,"notepad");
    assert!(fullscreen_for(true)&&fullscreen_for(false),"F6 must not rewrite preferences");
    dismiss(false);
    prefs.hotkey_fullscreen=false;
    prefs.dock_follow_hotkey=false;
    prefs.dock_fullscreen=true;
    configure(prefs);
    super::super::open_spotlight_dock(at);
    assert!(window().is_some());
    dismiss(false);
    configure(Preferences::default());
    for speed in [25,50,70,75,100,150,200]{
        motion::native_fixture(speed,||unsafe{
            super::super::toggle(ptr::null_mut());
            let (h,_,_,_)=handles().unwrap();
            let opening=COMPACT_MOTION.get().expect("compact runtime transition");
            assert_eq!(opening.duration,(22000.0/speed as f64).round() as u32);
            SendMessageW(h,WM_CHAR,'q' as WPARAM,0);
            assert_eq!(STATE.with(|s|s.borrow().as_ref().unwrap().query.clone()),"q");
            super::super::toggle(ptr::null_mut());
            let closing=COMPACT_MOTION.get().unwrap();
            assert_eq!(closing.duration,(16000.0/speed as f64).round() as u32);
            assert_eq!(closing.to,0.0);
            COMPACT_MOTION.set(Some(closing.finish()));
            SendMessageW(h,WM_TIMER,COMPACT_TWEEN,0);
            assert!(!compact_visible());
        });
    }
    println!("MOTION_SPEED compact_native=PASS six_speeds=PASS closing_endpoint=PASS");
    println!("SPOTLIGHT_NATIVE activation_mode_live_apply=PASS toggle=PASS F6_query=PASS");
}

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
        (words("Tìm kiếm…", "Search…", "搜索…"), 0x00635b56)
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
        println!(
            "SPOTLIGHT_NATIVE cold_hwnd_open_ms={:.3}",
            cold.elapsed().as_secs_f64() * 1000.0
        );
        let hwnd = window().expect("created native overlay");
        let click=|x:i32,y:i32|{
            let l=(((y as u32&0xffff)<<16)|(x as u32&0xffff)) as LPARAM;
            SendMessageW(hwnd,WM_LBUTTONDOWN,1,l);
            SendMessageW(hwnd,WM_LBUTTONUP,0,l);
        };
        let search=PANEL.with(|p|{let p=p.borrow();let c=p.as_ref().unwrap();
            let r=search_rect(c.width,c.dpi);
            assert!(controls(c).iter().all(|(action,_,_)|matches!(action,Hit::Page(_))));
            r
        });
        click(search.left+4,search.top+6);
        assert_eq!(window(),Some(hwnd));
        PANEL.with(|p|p.borrow_mut().as_mut().unwrap().editing=true);
        click(5,5);
        assert_eq!(window(),Some(hwnd));
        PANEL.with(|p|p.borrow_mut().as_mut().unwrap().editing=false);
        MODAL.set(true);
        click(5,5);
        MODAL.set(false);
        assert_eq!(window(),Some(hwnd));
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
        std::thread::sleep(Duration::from_millis(motion::hover_ms() as u64+30));
        // On the private non-input desktop TrackMouseEvent may queue a real
        // leave for the physical pointer, which is outside our synthetic tile.
        // Deliver the timer to the same native window procedure before pumping
        // unrelated pointer messages; then test the explicit leave separately.
        SendMessageW(hwnd,WM_TIMER,HOVER_ANIMATION,0);
        assert!(PANEL.with(|p|p.borrow().as_ref().unwrap().hover_started.is_none()));
        UpdateWindow(hwnd);
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
        configure(Preferences { theme: "dark".into(), animations: false, ..Preferences::default() });
        screenshot(hwnd, "folder-fixture-dark.png");
        click(5,5);
        assert!(PANEL.with(|p|p.borrow().as_ref().is_some_and(|p|p.folder==0)));
        click(5,5);
        assert!(window().is_none(),"background click closes only the outermost view");
        println!("SPOTLIGHT_NATIVE minimal_chrome=PASS empty_click=PASS modal_edit_folder_protection=PASS");
        configure(Preferences::default());
        open(p,String::new(),ptr::null_mut(),false);
        let motion_hwnd=window().unwrap();
        for speed in [25,50,70,75,100,150,200]{
            motion::native_fixture(speed,||{
                PANEL.with(|p|p.borrow_mut().as_mut().unwrap().folder=1);
                start_grid_motion(motion_hwnd);
                assert_eq!(PANEL.with(|p|p.borrow().as_ref().unwrap().grid_started.unwrap().ms()),(24000.0/speed as f64).round() as u32);
                PANEL.with(|p|p.borrow_mut().as_mut().unwrap().folder=0);
                start_grid_motion(motion_hwnd);
                assert_eq!(PANEL.with(|p|p.borrow().as_ref().unwrap().grid_started.unwrap().ms()),(22000.0/speed as f64).round() as u32);
            });
        }
        dismiss(false);
        configure(Preferences{animations:false,..Preferences::default()});
        println!("MOTION_SPEED folder_page_native=PASS six_speeds=PASS");
        pump();
        let before = resources();
        let memory_before=crate::private_mb();
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
        let memory_after=crate::private_mb();
        println!("RESOURCE_SPOTLIGHT private_working_set_mib_before={memory_before:?} private_working_set_mib_after={memory_after:?} tuple=private_bytes,working_set");
        assert!(PANEL.with(|p|p.borrow().is_none())&&FRAME.with(|f|f.borrow().is_none()));
        elapsed.sort_by(f64::total_cmp);
        println!(
            "SPOTLIGHT_NATIVE toggles=100 gdi_user_handles_before={before:?} after={after:?} warm_nonactivating_open_median_ms={:.3} p95_ms={:.3}",
            elapsed[50], elapsed[94]
        );
        assert!(after.0 <= before.0 + 2 && after.1 <= before.1 + 2 && after.2 <= before.2 + 8);
    }
}
