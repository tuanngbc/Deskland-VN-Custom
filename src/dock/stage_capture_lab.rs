//! Explicit isolated WGC diagnostic. Only the diagnostic's own painted
//! windows are eligible, and only this content can be exported.
use super::{stage_surface::Box2,stage_projection::Card,glass_probe::Layer};
use crate::desktop::wide;
use std::{path::Path,ptr::{self,null_mut},sync::mpsc,time::{Duration,Instant}};
use windows_sys::Win32::{Foundation::*, Graphics::{Dwm::{DwmFlush,DwmGetWindowAttribute,DWMWA_CLOAKED}, Gdi::*},
    System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*};

const CLASS: &str = "DesklandOwnedStageWgcFixture";
const WIDTH: i32 = 432;
const HEIGHT: i32 = 246;
#[cfg(test)]
static WINDOW_VARIANT:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);

struct Fixture(HWND);
impl Drop for Fixture {
    fn drop(&mut self) { unsafe { DestroyWindow(self.0); } }
}

unsafe extern "system" fn fixture_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    #[cfg(test)]
    if msg==WM_APP+171&&std::env::var("GITHUB_ACTIONS").as_deref()==Ok("true"){
        SetPropW(hwnd,wide("TLauncher.Fixture.Solid").as_ptr(),(w+1) as HANDLE);InvalidateRect(hwnd,std::ptr::null(),0);return 0;
    }
    #[cfg(test)]
    if msg==WM_PRINTCLIENT{
        let key=wide("TLauncher.Fixture.PrintCount");let count=GetPropW(hwnd,key.as_ptr()) as usize;SetPropW(hwnd,key.as_ptr(),(count+1) as HANDLE);
    }
    if msg != WM_PAINT && msg!=WM_PRINTCLIENT { return DefWindowProcW(hwnd,msg,w,l); }
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let dc = if msg==WM_PAINT{BeginPaint(hwnd,&mut ps)}else{w as HDC};
    let mut client:RECT=std::mem::zeroed();GetClientRect(hwnd,&mut client);
    #[cfg(test)]
    {
        let solid=GetPropW(hwnd,wide("TLauncher.Fixture.Solid").as_ptr()) as usize;
        if solid!=0{
            let brush=CreateSolidBrush((solid-1) as u32);FillRect(dc,&client,brush);DeleteObject(brush);
            if !GetPropW(hwnd,wide("TLauncher.Fixture.HighContrast").as_ptr()).is_null(){
                for x in (0..client.right).step_by(4){FillRect(dc,&RECT{left:x,top:0,right:(x+2).min(client.right),bottom:client.bottom},GetStockObject(BLACK_BRUSH) as HBRUSH);}
            }
            if !GetPropW(hwnd,wide("TLauncher.Fixture.BlackFrame").as_ptr()).is_null(){FrameRect(dc,&client,GetStockObject(BLACK_BRUSH) as HBRUSH);}
            if msg==WM_PAINT{EndPaint(hwnd,&ps);}return 0;
        }
    }
    let background=CreateSolidBrush(0x00ede6d1);FillRect(dc,&client,background);DeleteObject(background);
    for (left,top,right,bottom,color) in [
        (0,0,WIDTH,HEIGHT,0x00ede6d1),
        (0,0,WIDTH,34,0x00453928),
        (0,34,103,HEIGHT,0x0071892f),
        (125,61,390,69,0x0085c2dc),
        (125,88,381,94,0x009e8973),
        (125,110,368,116,0x009e8973),
        (125,132,388,138,0x009e8973),
        (125,154,302,160,0x009e8973),
    ] {
        #[cfg(test)]
        let color=if color==0x0071892f&&WINDOW_VARIANT.load(std::sync::atomic::Ordering::Relaxed){0x00305cda}else{color};
        let brush = CreateSolidBrush(color);
        FillRect(dc,&RECT{left,top,right,bottom},brush);
        DeleteObject(brush);
    }
    SetBkMode(dc,TRANSPARENT as i32);
    SetTextColor(dc,0x00ffffff);
    let title=wide("Deskland • Window Capture Fixture");
    TextOutW(dc,12,8,title.as_ptr(),title.len() as i32-1);
    if msg==WM_PAINT{EndPaint(hwnd,&ps);}
    0
}

unsafe fn window(normal:bool) -> Result<Fixture,String> {
    #[cfg(all(test,target_env="msvc"))]
    {
        extern "C"{fn tlauncher_fixture_native_diagnostics();}
        static ONCE:std::sync::Once=std::sync::Once::new();
        ONCE.call_once(||tlauncher_fixture_native_diagnostics());
    }
    let name = wide(CLASS);
    let instance = GetModuleHandleW(std::ptr::null());
    let class = WNDCLASSW { lpfnWndProc: Some(fixture_proc), hInstance: instance,
        lpszClassName: name.as_ptr(), ..std::mem::zeroed() };
    if RegisterClassW(&class)==0 && GetLastError()!=ERROR_CLASS_ALREADY_EXISTS{return Err("RegisterClassW failed".into());}
    let hwnd = CreateWindowExW(if normal{0}else{WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE}, name.as_ptr(), wide(CLASS).as_ptr(),
        if normal{WS_OVERLAPPEDWINDOW}else{WS_POPUP}, 24,24,WIDTH,HEIGHT,null_mut(),null_mut(),instance,null_mut());
    if hwnd.is_null(){return Err(format!("CreateWindowExW: {}",GetLastError()));}
    ShowWindow(hwnd,SW_SHOWNOACTIVATE);
    UpdateWindow(hwnd);
    let _ = DwmFlush();
    Ok(Fixture(hwnd))
}

unsafe fn allowed(hwnd: HWND) -> bool {
    let mut pid = 0;
    let mut class = [0u16;64];
    let tid = GetWindowThreadProcessId(hwnd,&mut pid);
    let n = GetClassNameW(hwnd,class.as_mut_ptr(),class.len() as i32);
    let mut affinity = u32::MAX;
    tid != 0 && pid == std::process::id() && n > 0
        && String::from_utf16_lossy(&class[..n as usize]) == CLASS
        && IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) == 0
        && GetWindowDisplayAffinity(hwnd,&mut affinity) != 0 && affinity == WDA_NONE
}


fn pump_capture(fixture:&Fixture)->Result<(Vec<u8>,i32,i32),String>{
    let handle=fixture.0 as usize;
    std::thread::scope(|scope|{
        let (tx,rx)=mpsc::sync_channel(1);
        scope.spawn(move||unsafe{
            let hwnd=handle as HWND;
            let _=tx.send(super::stage_snapshot::frame(hwnd,||allowed(hwnd)));
        });
        // WGC's target has its own functioning window message loop. In
        // particular, the HWND owner is not blocked waiting on CreateForWindow.
        unsafe{
            let mut msg=std::mem::zeroed();
            loop{
                for _ in 0..32{
                    if PeekMessageW(&mut msg,null_mut(),0,0,PM_REMOVE)==0{break;}
                    TranslateMessage(&msg);DispatchMessageW(&msg);
                }
                match rx.recv_timeout(Duration::from_millis(12)){
                    Ok(result)=>return result,
                    Err(mpsc::RecvTimeoutError::Disconnected)=>return Err("capture worker ended".into()),
                    Err(mpsc::RecvTimeoutError::Timeout)=>{},
                }
            }
        }
    })
}
pub(super) fn inspect(dir:&Path,visible_trial:bool)->Result<(),String>{
    std::fs::create_dir_all(dir).map_err(|e|e.to_string())?;
    let mut statuses=Vec::new();
    for (normal,description) in [(true,"overlapped"),(false,"toolwindow")]{
        let started=Instant::now();
        let outcome=unsafe{
            let fixture=window(normal)?;
            let hwnd=fixture.0;
            let mut affinity=u32::MAX;
            let af=GetWindowDisplayAffinity(hwnd,&mut affinity);
            let affinity_error=if af!=0{0}else{GetLastError()};
            let mut cloak=u32::MAX;
            let cloak_result=DwmGetWindowAttribute(hwnd,DWMWA_CLOAKED as u32,(&mut cloak as *mut u32).cast(),4);
            let summary=format!("window={description} own_window=true root={} visible={} minimized={} cloak=0x{cloak_result:08x}/{cloak} affinity={af}/{affinity}/{affinity_error} pump=separate_mta",
                GetAncestor(hwnd,GA_ROOT)==hwnd,IsWindowVisible(hwnd),IsIconic(hwnd));
            if allowed(GetDesktopWindow()){return Err("Desktop must never be eligible".into());}
            let captured=pump_capture(&fixture);
            if SetWindowDisplayAffinity(hwnd,WDA_EXCLUDEFROMCAPTURE)!=0&&allowed(hwnd){
                return Err("Protected window remained eligible".into());
            }
            let message=match captured{
                Err(reason)=>format!("WGC_OWNED_HWND=BLOCKED {summary}: {reason}"),
                Ok((bytes,w,h))=>{
                    let distinct=bytes.chunks_exact(4).filter(|p|p[1]>p[2].saturating_add(45)&&p[1]>105).count();
                    if distinct<=100{return Err("Expected green marker in owned window pixels".into());}
                    let mut dest=Layer::new(223,126).ok_or("Could not allocate result DIB")?;
                    let pixels=dest.pixels();pixels.fill(0);
                    let slab=Card::new(Box2{x:9,y:9,w:198,h:108},false,28.0);
                    slab.paint(pixels,223,126,&bytes,w,h);
                    if !pixels.chunks_exact(4).any(|p|p[1]>p[2].saturating_add(30)&&p[1]>95){
                        return Err("Owned pixels missing after projection".into());
                    }
                    let rgba=pixels.chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],p[3]]).collect::<Vec<_>>();
                    image::save_buffer(dir.join(format!("stage-wgc-owned-{description}.png")),&rgba,223,126,image::ColorType::Rgba8)
                        .map_err(|e|e.to_string())?;
                    format!("WGC_OWNED_HWND=PASS {summary} source={w}x{h} elapsed_ms={}",started.elapsed().as_millis())
                }
            };
            message
        };
        println!("{outcome}");
        let passed=outcome.starts_with("WGC_OWNED_HWND=PASS");
        statuses.push(outcome);
        if passed || visible_trial{break;}
    }
    std::fs::write(dir.join("stage-wgc-result.txt"),statuses.join("\n")).map_err(|e|e.to_string())
}

#[cfg(test)]
#[test]
#[ignore = "requires an isolated Windows graphics session"]
pub(super) fn native_wgc_projects_one_owned_hwnd_or_reports_blocked() {
    let Some(dir)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE").map(std::path::PathBuf::from)else{return;};
    inspect(&dir,false).unwrap();
}


#[cfg(test)]
#[test]
#[ignore="actual WGC source pixels and Scale proxy lifecycle on Actions"]
fn native_scale_proxy_owned_source_100_cycles(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    unsafe{
        let source=window(true).unwrap();
        // Repaint the actual owned HWND before the one-shot WGC request.
        // Never create a second separate capture device on the fixture.
        WINDOW_VARIANT.store(true,std::sync::atomic::Ordering::Relaxed);
        InvalidateRect(source.0,std::ptr::null(),0);UpdateWindow(source.0);let _=DwmFlush();
        WINDOW_VARIANT.store(false,std::sync::atomic::Ordering::Relaxed);
        InvalidateRect(source.0,std::ptr::null(),0);UpdateWindow(source.0);let _=DwmFlush();
        println!("WINDOW_SCALE_NATIVE requesting_current_owned_frame");
        let frame=match pump_capture(&source){
            Ok(frame)=>frame,
            Err(reason)=>{println!("WINDOW_SCALE_NATIVE=BLOCKED {reason}");return;}
        };
        let green=frame.0.chunks_exact(4).filter(|p|p[1]>p[2].saturating_add(45)&&p[1]>105).count();
        assert!(green>100,"one-shot capture must contain the currently repainted source");
        println!("WINDOW_SCALE_SOURCE_FRESHNESS current_repaint_marker=PASS green={green} repeated_capture=NOT_VERIFIED");
        super::window_effect::probe_owned_window(source.0,&frame);
    }
}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="actual WGC and upstream Genie compositor probe on Actions"]
fn native_fluidwin_genie_presented_pixels(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    unsafe{
        let source=window(true).unwrap();
        let frame=match pump_capture(&source){
            Ok(frame)=>frame,
            Err(reason)=>{println!("FLUIDWIN_GENIE_SCREEN_PIXELS=NOT_VERIFIED {reason}");return;}
        };
        super::window_effect::probe_mesh_pixels(source.0,&frame);
    }
}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="full-resolution GPU capture and actual rendered perimeter on hosted Windows"]
fn native_fluidwin_gpu_capture_full_resolution(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    unsafe{
        let source=window(true).unwrap();
        SetWindowPos(source.0,null_mut(),16,16,2110,980,SWP_NOACTIVATE|SWP_NOZORDER);
        UpdateWindow(source.0);
        let mut area:RECT=std::mem::zeroed();
        assert!(windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute(source.0,
            windows_sys::Win32::Graphics::Dwm::DWMWA_EXTENDED_FRAME_BOUNDS as u32,(&mut area as *mut RECT).cast(),std::mem::size_of::<RECT>() as u32)>=0);
        let handle=source.0 as usize;
        let result=std::thread::scope(|scope|{
            let (tx,rx)=mpsc::sync_channel(1);
            scope.spawn(move||{let _=tx.send(super::stage_snapshot::gpu_frame(handle as HWND,||allowed(handle as HWND)));});
            let mut msg=std::mem::zeroed();
            loop{
                while PeekMessageW(&mut msg,null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}
                match rx.recv_timeout(Duration::from_millis(12)){
                    Ok(frame)=>break frame,
                    Err(mpsc::RecvTimeoutError::Disconnected)=>panic!("GPU fixture worker ended"),Err(mpsc::RecvTimeoutError::Timeout)=>{}
                }
            }
        });
        let super::stage_snapshot::MotionFrame::Gpu(frame)=result.expect("native resolution WGC GPU capture")else{panic!("motion source returned a CPU thumbnail")};
        assert_eq!((frame.width,frame.height),(area.right-area.left,area.bottom-area.top));
        assert!(frame.width>1024,"full-window source must exceed the old thumbnail cap");
        let icon=RECT{left:400,top:1040,right:448,bottom:1088};
        let pipeline=super::fluidwin_mesh::Pipeline::new(&frame.gpu.context()).unwrap();
        for effect in [super::motion::WindowEffect::Genie,super::motion::WindowEffect::Drop,super::motion::WindowEffect::Scale]{
            let layer={let _batch=frame.gpu.context();super::fluidwin_mesh::Layer::from_gpu(pipeline.clone(),&frame,area,icon,super::DockSide::Bottom,effect,96).unwrap()};
            assert!({let _batch=frame.gpu.context();layer.enable_evidence()},"pre-Present rendered evidence target");
            for (index,progress) in [0.0,0.05,0.25,0.5,0.85,0.97,1.0,0.85,0.5,0.0].into_iter().enumerate(){
                let start=Instant::now();
                loop{
                    let status={let _batch=frame.gpu.context();layer.paint(progress,255)};
                    if status==1{break;}
                    assert!(status==2&&start.elapsed()<Duration::from_secs(1),"GPU draw failed {status:#x}");
                    std::thread::sleep(Duration::from_millis(8));
                }
                let (pixels,w,h)={let _batch=frame.gpu.context();layer.rendered_pixels().expect("complete GPU output readback")};
                if progress<0.85{
                    assert!(pixels.chunks_exact(4).filter(|p|p[3]>16).count()>256,"rendered evidence must contain window pixels, including first pose");
                }
                assert!(pixels.chunks_exact(4).all(|p|p[0]<=p[3]&&p[1]<=p[3]&&p[2]<=p[3]),"all rendered pixels, including full perimeter, must be premultiplied");
                if let Some(dir)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){
                    let rgba=pixels.chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],p[3]]).collect::<Vec<_>>();
                    image::save_buffer(std::path::PathBuf::from(dir).join(format!("motion-gpu-{}-{index}.png",effect.name())),&rgba,w,h,image::ColorType::Rgba8).unwrap();
                }
            }
        }
        println!("FLUIDWIN_GPU_CAPTURE source={}x{} cpu_readback_in_runtime=false rendered_perimeter=CHECKED owner_artifact_acceptance=FAIL",frame.width,frame.height);
    }
}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="separate source texture and live screen perimeter evidence on hosted Windows; never infers an owner border fix"]
fn native_live_capture_texture_and_screen_perimeters(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use std::sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}};
    use super::stage_snapshot::{self,MotionFrame,CaptureTiming};
    unsafe{
        let source=window(true).unwrap();let h=source.0;
        SetWindowLongPtrW(h,GWL_STYLE,(WS_POPUP|WS_VISIBLE) as isize);
        SetWindowPos(h,HWND_TOPMOST,80,80,480,320,SWP_NOACTIVATE|SWP_FRAMECHANGED);
        SetPropW(h,wide("TLauncher.Fixture.Solid").as_ptr(),0x01000000usize as HANDLE);InvalidateRect(h,ptr::null(),0);UpdateWindow(h);
        let area=RECT{left:80,top:80,right:560,bottom:400};let expanded=RECT{left:64,top:64,right:576,bottom:416};
        let pump=||{let mut message:MSG=std::mem::zeroed();for _ in 0..64{if PeekMessageW(&mut message,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&message);DispatchMessageW(&message);}DwmFlush();};
        let save=|name:&str,bgra:&[u8],width:i32,height:i32|{
            if let Some(dir)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){
                let rgba=bgra.chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],p[3]]).collect::<Vec<_>>();
                image::save_buffer(std::path::PathBuf::from(dir).join(format!("capture-{name}.png")),&rgba,width as u32,height as u32,image::ColorType::Rgba8).unwrap();
            }
        };
        let screen=|name:&str|{pump();let width=expanded.right-expanded.left;let height=expanded.bottom-expanded.top;let mut layer=Layer::new(width,height).unwrap();
            let dc=GetDC(ptr::null_mut());assert_ne!(BitBlt(layer.dc(),0,0,width,height,dc,expanded.left,expanded.top,SRCCOPY|CAPTUREBLT),0);ReleaseDC(ptr::null_mut(),dc);GdiFlush();
            let mut pixels=layer.pixels().to_vec();for p in pixels.chunks_exact_mut(4){p[3]=255;}save(name,&pixels,width,height);pixels};
        pump();let before=screen("source-before");let timing=Arc::new(Mutex::new(CaptureTiming::default()));
        std::thread::scope(|scope|{
            let live=Arc::new(AtomicBool::new(true));struct Stop(Arc<AtomicBool>);impl Drop for Stop{fn drop(&mut self){self.0.store(false,Ordering::Release);}}
            let stop=Stop(live.clone());let (send,receive)=mpsc::sync_channel(1);let handle=h as usize;let worker_timing=timing.clone();
            let worker=scope.spawn(move||stage_snapshot::fixture_live_gpu(handle as HWND,||live.load(Ordering::Acquire)&&allowed(handle as HWND),&send,&worker_timing));
            let started=Instant::now();let frame=loop{
                pump();match receive.recv_timeout(Duration::from_millis(12)){
                    Ok(Ok(MotionFrame::Gpu(frame)))=>break frame,Ok(Ok(_))=>panic!("live screen fixture got a CPU thumbnail"),Ok(Err(e))=>panic!("live screen fixture: {e}"),
                    Err(mpsc::RecvTimeoutError::Disconnected)=>panic!("live screen fixture ended before first frame"),Err(mpsc::RecvTimeoutError::Timeout)=>assert!(started.elapsed()<Duration::from_secs(5)),
                }
            };
            assert_eq!((frame.width,frame.height),(480,320));
            let white=frame.fixture_pixels().unwrap();let center=(160*480+240)*4;assert!(white[center..center+3].iter().all(|v|*v>=245));
            save("texture-white",&white,frame.width,frame.height);
            let start=Instant::now();while start.elapsed()<Duration::from_millis(150){pump();std::thread::sleep(Duration::from_millis(10));}
            let during=screen("source-active-white");
            let (mut ring,mut changed,mut chromatic)=(0usize,0usize,0usize);let width=(expanded.right-expanded.left) as usize;
            for (i,(b,d)) in before.chunks_exact(4).zip(during.chunks_exact(4)).enumerate(){
                let x=expanded.left+(i%width) as i32;let y=expanded.top+(i/width) as i32;
                if x>=area.left+3&&x<area.right-3&&y>=area.top+3&&y<area.bottom-3{continue;}
                ring+=1;changed+=usize::from(b[..3]!=d[..3]);let high=*d[..3].iter().max().unwrap();let low=*d[..3].iter().min().unwrap();chromatic+=usize::from(high.saturating_sub(low)>32);
            }
            SetPropW(h,wide("TLauncher.Fixture.Solid").as_ptr(),0x00ff0001usize as HANDLE);InvalidateRect(h,ptr::null(),0);UpdateWindow(h);
            let repaint=Instant::now();let blue=loop{
                pump();let pixels=frame.fixture_pixels().unwrap();if pixels[center]>245&&pixels[center+1]<10&&pixels[center+2]<10{break pixels;}
                assert!(repaint.elapsed()<Duration::from_secs(4),"same live texture did not update from white to blue");std::thread::sleep(Duration::from_millis(12));
            };
            save("texture-blue",&blue,frame.width,frame.height);screen("source-active-blue");
            println!("TLAUNCHER_CAPTURE_SCREEN ring_pixels={ring} changed={changed} chromatic={chromatic} source_texture=EXPORTED border_conclusion=NOT_INFERRED");
            drop(stop);let result=worker.join().expect("capture perimeter worker panicked");assert!(result.is_err(),"live stream should end through explicit cancellation");
        });
        screen("source-stopped");println!("TLAUNCHER_CAPTURE_TIMING {}",serde_json::to_string(&*timing.lock().unwrap()).unwrap());
        println!("TLAUNCHER_CAPTURE_BASELINE GPU_to_GPU=PASS live_white_to_blue=PASS screen_perimeter=EXPORTED consent_prompt=NOT_REQUESTED owner_border=UNRESOLVED");
    }
}

#[cfg(all(test,target_env="msvc"))]
unsafe fn pump(){
    let mut msg:MSG=std::mem::zeroed();
    while PeekMessageW(&mut msg,null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}
    DwmFlush();
}

#[cfg(all(test,target_env="msvc"))]
fn pump_static_fixture(fixture:&Fixture,allow_print:bool)->Result<Option<(super::stage_snapshot::GpuFrame,super::stage_snapshot::CaptureTiming)>,String>{
    let handle=fixture.0 as usize;
    std::thread::scope(|scope|{
        let (tx,rx)=mpsc::sync_channel(1);
        scope.spawn(move||unsafe{
            let timing=std::sync::Arc::new(std::sync::Mutex::new(super::stage_snapshot::CaptureTiming::default()));
            let result=super::stage_snapshot::static_motion_frame(handle as HWND,||allowed(handle as HWND),Some(&timing),allow_print)
                .and_then(|f|match f{Some(super::stage_snapshot::MotionFrame::Gpu(frame))=>Ok(Some((frame,timing.lock().unwrap().clone()))),None=>Ok(None),_=>Err("static photo became a thumbnail".into())});
            let _=tx.send(result);
        });
        unsafe{let mut msg:MSG=std::mem::zeroed();loop{
            while PeekMessageW(&mut msg,null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}
            match rx.recv_timeout(Duration::from_millis(10)){
                Ok(result)=>return result,Err(mpsc::RecvTimeoutError::Timeout)=>{},Err(_)=>return Err("static fixture worker disconnected".into()),
            }
        }}
    })
}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="native original static photo paths, actual full texture/perimeter and source-policy split on hosted Windows"]
fn native_static_photos_keep_original_pixels_and_reject_unsafe_sources(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    unsafe{
        let source=window(false).unwrap();SetWindowPos(source.0,HWND_TOPMOST,32,32,640,420,SWP_NOACTIVATE);
        let save=|name:&str,pixels:&[u8],w:i32,h:i32|{
            if let Some(dir)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){
                let mut rgba=pixels.to_vec();for p in rgba.chunks_exact_mut(4){p.swap(0,2);}
                image::save_buffer(std::path::PathBuf::from(dir).join(format!("static-{name}.png")),&rgba,w as u32,h as u32,image::ColorType::Rgba8).unwrap();
            }
        };
        for (name,color,expected) in [("white",0x00ffffff,[255,255,255,255]),("black",0,[0,0,0,255]),("red",0x000000ff,[0,0,255,255]),("green",0x0000ff00,[0,255,0,255]),("blue",0x00ff0000,[255,0,0,255])]{
            SetPropW(source.0,wide("TLauncher.Fixture.Solid").as_ptr(),(color+1) as HANDLE);InvalidateRect(source.0,ptr::null(),0);UpdateWindow(source.0);pump();
            let (frame,timing)=pump_static_fixture(&source,false).unwrap().expect("rectangular source must use static photo");
            assert_eq!((frame.width,frame.height),(640,420));assert_eq!(timing.capture_backend,Some("gdi-uncovered-screen"));assert!(!timing.gpu_to_gpu);assert_eq!(timing.live_frames,0);
            let pixels=frame.fixture_pixels().unwrap();assert!(pixels.chunks_exact(4).all(|p|p==expected),"{name} complete texture including all perimeter pixels");save(name,&pixels,frame.width,frame.height);
            let mut screen=Layer::new(656,436).unwrap();let dc=GetDC(ptr::null_mut());assert_ne!(BitBlt(screen.dc(),0,0,656,436,dc,24,24,SRCCOPY|CAPTUREBLT),0);ReleaseDC(ptr::null_mut(),dc);
            let mut screen=screen.pixels().to_vec();for p in screen.chunks_exact_mut(4){p[3]=255;}save(&format!("source-{name}"),&screen,656,436);
            let area=RECT{left:32,top:32,right:672,bottom:452};let icon=RECT{left:330,top:650,right:378,bottom:698};
            let pipeline=super::fluidwin_mesh::Pipeline::new(&frame.gpu.context()).unwrap();
            for effect in [super::motion::WindowEffect::Genie,super::motion::WindowEffect::Drop,super::motion::WindowEffect::Scale]{
                let layer={let _batch=frame.gpu.context();super::fluidwin_mesh::Layer::from_gpu(pipeline.clone(),&frame,area,icon,super::DockSide::Bottom,effect,96).unwrap()};
                assert!({let _batch=frame.gpu.context();layer.enable_evidence()});
                for (index,progress) in [0.0,0.05,0.25,0.5,0.85,0.97,1.0,0.85,0.5,0.0].into_iter().enumerate(){
                    let deadline=Instant::now()+Duration::from_secs(1);
                    loop{let status={let _batch=frame.gpu.context();layer.paint(progress,255)};if status==1{break;}assert!(status==2&&Instant::now()<deadline);std::thread::sleep(Duration::from_millis(8));}
                    let (pixels,w,h)={let _batch=frame.gpu.context();layer.rendered_pixels().unwrap()};
                    if progress<0.85{assert!(pixels.chunks_exact(4).filter(|p|p[3]>16).count()>256);}
                    for p in pixels.chunks_exact(4){
                        assert!(p[..3].iter().all(|&c|c<=p[3]),"static premultiplication on full rendered canvas");
                        let neutral=p[..3].iter().max().unwrap()-p[..3].iter().min().unwrap()<=12;
                        let source_color=match name{"red"=>p[0].abs_diff(p[1])<=3&&p[2]>=p[0].min(p[1]),"green"=>p[0].abs_diff(p[2])<=3&&p[1]>=p[0].min(p[2]),"blue"=>p[1].abs_diff(p[2])<=3&&p[0]>=p[1].min(p[2]),_=>false};
                        assert!(neutral||source_color,"static unexpected chroma {name} {} pose {index}: {p:?}",effect.name());
                    }
                    save(&format!("{name}-{}-{index}",effect.name()),&pixels,w as i32,h as i32);
                }
            }
        }
        // Preserve a legitimate application's black one-pixel frame. Original
        // carica's darkening/recolor repair must never leak into this adapter.
        SetPropW(source.0,wide("TLauncher.Fixture.Solid").as_ptr(),0x1000000 as HANDLE);SetPropW(source.0,wide("TLauncher.Fixture.BlackFrame").as_ptr(),1usize as HANDLE);
        InvalidateRect(source.0,ptr::null(),0);UpdateWindow(source.0);pump();
        let (frame,_)=pump_static_fixture(&source,false).unwrap().unwrap();let pixels=frame.fixture_pixels().unwrap();save("legitimate-black-frame",&pixels,frame.width,frame.height);
        for y in 0..frame.height{for x in 0..frame.width{let edge=x==0||y==0||x==frame.width-1||y==frame.height-1;let i=(y as usize*frame.width as usize+x as usize)*4;assert_eq!(&pixels[i..i+4],if edge{&[0,0,0,255]}else{&[255,255,255,255]},"legitimate frame pixel {x},{y}");}}
        RemovePropW(source.0,wide("TLauncher.Fixture.BlackFrame").as_ptr());RemovePropW(source.0,wide("TLauncher.Fixture.Solid").as_ptr());InvalidateRect(source.0,ptr::null(),0);UpdateWindow(source.0);pump();
        let (frame,_)=pump_static_fixture(&source,false).unwrap().unwrap();let pixels=frame.fixture_pixels().unwrap();save("mixed-fine-text",&pixels,frame.width,frame.height);
        let mut expected=Layer::new(frame.width,frame.height).unwrap();let dc=GetDC(ptr::null_mut());assert_ne!(BitBlt(expected.dc(),0,0,frame.width,frame.height,dc,32,32,SRCCOPY),0);ReleaseDC(ptr::null_mut(),dc);
        for p in expected.pixels().chunks_exact_mut(4){p[3]=255;}assert_eq!(pixels.as_slice(),&*expected.pixels(),"full native mixed/fine-text pixels and all perimeter rows");
        // A real occluder forces the original PrintWindow route. Its blue
        // pixels must never appear in the source's red texture.
        SetPropW(source.0,wide("TLauncher.Fixture.Solid").as_ptr(),0x100 as HANDLE);InvalidateRect(source.0,ptr::null(),0);UpdateWindow(source.0);pump();
        let cover=window(false).unwrap();SetPropW(cover.0,wide("TLauncher.Fixture.Solid").as_ptr(),0xff0001 as HANDLE);
        SetWindowPos(cover.0,HWND_TOPMOST,50,50,100,80,SWP_NOACTIVATE);InvalidateRect(cover.0,ptr::null(),0);UpdateWindow(cover.0);pump();
        assert!(pump_static_fixture(&source,false).is_err(),"occluded pixels cannot use screen capture");
        let (frame,timing)=pump_static_fixture(&source,true).unwrap().unwrap();assert_eq!(timing.capture_backend,Some("printwindow-static"));
        let pixels=frame.fixture_pixels().unwrap();assert!(pixels.chunks_exact(4).all(|p|p==[0,0,255,255]));save("print-occluded-red",&pixels,frame.width,frame.height);drop(cover);
        // A real custom region / layered source is left for alpha-aware WGC.
        assert_ne!(SetWindowRgn(source.0,CreateRoundRectRgn(0,0,640,420,50,50),1),0);
        assert!(pump_static_fixture(&source,true).unwrap().is_none());SetWindowRgn(source.0,ptr::null_mut(),1);
        let style=GetWindowLongPtrW(source.0,GWL_EXSTYLE);SetWindowLongPtrW(source.0,GWL_EXSTYLE,style|WS_EX_LAYERED as isize);
        SetLayeredWindowAttributes(source.0,0,128,LWA_ALPHA);assert!(pump_static_fixture(&source,true).unwrap().is_none());SetLayeredWindowAttributes(source.0,0,255,LWA_ALPHA);SetWindowLongPtrW(source.0,GWL_EXSTYLE,style);
        SetWindowLongPtrW(source.0,GWL_EXSTYLE,style|WS_EX_NOREDIRECTIONBITMAP as isize);assert!(pump_static_fixture(&source,true).unwrap().is_none(),"composition alpha must remain on WGC");SetWindowLongPtrW(source.0,GWL_EXSTYLE,style);
        assert_ne!(SetWindowDisplayAffinity(source.0,WDA_MONITOR),0);assert!(pump_static_fixture(&source,true).is_err());assert_ne!(SetWindowDisplayAffinity(source.0,WDA_NONE),0);
        assert!(super::stage_snapshot::static_motion_frame(source.0,||false,None,true).is_err());
        SetWindowPos(source.0,HWND_TOPMOST,20,20,2110,980,SWP_NOACTIVATE);InvalidateRect(source.0,ptr::null(),0);UpdateWindow(source.0);pump();
        // Original full-size off-display photo on this runner includes black
        // padding and even its taskbar. Refuse that route, without shrinking or
        // cropping source content; the worker retains its existing WGC path.
        let mut monitor:MONITORINFO=std::mem::zeroed();monitor.cbSize=std::mem::size_of::<MONITORINFO>() as u32;
        assert_ne!(GetMonitorInfoW(MonitorFromWindow(source.0,MONITOR_DEFAULTTONEAREST),&mut monitor),0);
        let off_display=2110+20>monitor.rcMonitor.right||980+20>monitor.rcMonitor.bottom;
        if off_display{assert!(pump_static_fixture(&source,true).unwrap().is_none(),"post-layered off-display source must retain WGC");}
        let fresh=window(false).unwrap();SetPropW(fresh.0,wide("TLauncher.Fixture.Solid").as_ptr(),0x100 as HANDLE);
        SetWindowPos(fresh.0,HWND_TOPMOST,20,20,2110,980,SWP_NOACTIVATE);InvalidateRect(fresh.0,ptr::null(),0);UpdateWindow(fresh.0);pump();
        if off_display{assert!(pump_static_fixture(&fresh,true).unwrap().is_none(),"fresh off-display source must retain WGC");}
        else{let (frame,_)=pump_static_fixture(&fresh,true).unwrap().unwrap();assert_eq!((frame.width,frame.height),(2110,980));let pixels=frame.fixture_pixels().unwrap();save("large-native-red",&pixels,frame.width,frame.height);assert!(pixels.chunks_exact(4).all(|p|p==[0,0,255,255]));}
        // Restore a fully covered size after the real native state changes;
        // this remains a full-pixel regression, not a discarded failing case.
        drop(fresh);SetWindowPos(source.0,HWND_TOPMOST,32,32,640,420,SWP_NOACTIVATE);InvalidateRect(source.0,ptr::null(),0);UpdateWindow(source.0);pump();
        let (frame,_)=pump_static_fixture(&source,true).unwrap().unwrap();let pixels=frame.fixture_pixels().unwrap();save("post-layered-affinity-red",&pixels,frame.width,frame.height);assert!(pixels.chunks_exact(4).all(|p|p==[0,0,255,255]));
        println!("FLUIDWIN_STATIC_PHOTO screen_colors=VERIFIED occluded_print=VERIFIED post_layered_affinity=VERIFIED off_display_deferred_to_wgc={off_display} alpha_shape=DEFERRED_TO_WGC protected_cancel=REFUSED gpu_to_gpu=false edge_recolor=NONE owner_border=UNRESOLVED");
    }
}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="held native print callback, globally bounded admission and late cleanup on hosted Windows"]
fn native_static_print_timeout_keeps_one_job_until_native_completion(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    unsafe{
        use windows_sys::Win32::System::Threading::{CreateEventW,SetEvent,WaitForSingleObject};
        use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
        extern "C"{fn tlauncher_static_fixture_pause(target:HWND,entered:HANDLE,resume:HANDLE)->bool;}
        let hold=CreateEventW(ptr::null(),1,0,ptr::null());let entered=CreateEventW(ptr::null(),1,0,ptr::null());assert!(!hold.is_null()&&!entered.is_null());
        struct Events(HANDLE,HANDLE);impl Drop for Events{fn drop(&mut self){unsafe{SetEvent(self.0);tlauncher_static_fixture_pause(ptr::null_mut(),ptr::null_mut(),ptr::null_mut());CloseHandle(self.0);CloseHandle(self.1);}}}let _events=Events(hold,entered);
        let stop=Arc::new(AtomicBool::new(false));let done=stop.clone();let cancelled=Arc::new(AtomicBool::new(false));let (tx,rx)=mpsc::sync_channel(1);
        let owner=std::thread::spawn(move||{
            let source=window(false).unwrap();SetPropW(source.0,wide("TLauncher.Fixture.Solid").as_ptr(),0x100 as HANDLE);
            UpdateWindow(source.0);tx.send(source.0 as usize).unwrap();while !done.load(Ordering::Relaxed){pump();std::thread::sleep(Duration::from_millis(5));}
        });
        struct Owner{stop:Arc<AtomicBool>,hold:usize,thread:Option<std::thread::JoinHandle<()>>}
        impl Drop for Owner{fn drop(&mut self){unsafe{SetEvent(self.hold as HANDLE);}self.stop.store(true,Ordering::Relaxed);if let Some(thread)=self.thread.take(){let _=thread.join();}}}
        let owner=Owner{stop,hold:hold as usize,thread:Some(owner)};
        let hwnd=rx.recv_timeout(Duration::from_secs(3)).unwrap() as HWND;
        let second=window(false).unwrap();let cover=window(false).unwrap();SetWindowPos(cover.0,HWND_TOPMOST,0,0,700,500,SWP_NOACTIVATE);pump();
        assert!(tlauncher_static_fixture_pause(hwnd,entered,hold));let target=hwnd as usize;let token=cancelled.clone();
        let started=Instant::now();let reader=std::thread::spawn(move||super::stage_snapshot::static_motion_frame(target as HWND,||!token.load(Ordering::Acquire)&&allowed(target as HWND),None,true));
        assert_eq!(WaitForSingleObject(entered,1500),0);cancelled.store(true,Ordering::Release);let first=reader.join().unwrap();
        assert!(first.as_ref().err().unwrap().contains("cancelled before handoff"));
        assert!(first.is_err());assert!(started.elapsed()<Duration::from_millis(900));assert_eq!(WaitForSingleObject(entered,0),0);
        for _ in 0..4{let error=super::stage_snapshot::static_motion_frame(second.0,||allowed(second.0),None,true).err().unwrap();assert!(error.contains("(4)"),"global admission: {error}");}
        SetEvent(hold);assert!(tlauncher_static_fixture_pause(ptr::null_mut(),ptr::null_mut(),ptr::null_mut()));let deadline=Instant::now()+Duration::from_secs(3);
        loop{match super::stage_snapshot::static_motion_frame(hwnd,||allowed(hwnd),None,true){
            Ok(Some(_))=>break,Err(e) if e.contains("(4)")&&Instant::now()<deadline=>std::thread::sleep(Duration::from_millis(10)),other=>panic!("late print cleanup: {}",other.err().unwrap_or_default()),
        }}
        drop(owner);
        println!("FLUIDWIN_STATIC_PRINT deadline=BOUNDED global_jobs=ONE late_native_completion=RELEASED repeated_admission=VERIFIED fixture=HELD_CALLBACK actual_hung_PrintWindow=NOT_VERIFIED");
    }
}

#[cfg(all(test,target_env="msvc"))]
fn pump_gpu_fixture(fixture:&Fixture)->Result<super::stage_snapshot::GpuFrame,String>{
    let handle=fixture.0 as usize;
    std::thread::scope(|scope|{
        let (tx,rx)=mpsc::sync_channel(1);
        scope.spawn(move||unsafe{let _=tx.send(super::stage_snapshot::gpu_frame(handle as HWND,||allowed(handle as HWND)));});
        unsafe{
            let mut msg:MSG=std::mem::zeroed();
            loop{
                while PeekMessageW(&mut msg,null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}
                match rx.recv_timeout(Duration::from_millis(10)){
                    Ok(Ok(super::stage_snapshot::MotionFrame::Gpu(frame)))=>return Ok(frame),
                    Ok(Ok(_))=>return Err("unexpected CPU fixture capture".into()),Ok(Err(e))=>return Err(e),
                    Err(mpsc::RecvTimeoutError::Timeout)=>{},Err(_)=>return Err("fixture worker disconnected".into()),
                }
            }
        }
    })
}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="actual WGC colors and complete rendered output across all motion poses on hosted Windows"]
fn native_gpu_color_perimeters_across_first_reverse_and_cancel_poses(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    unsafe{
        let source=window(true).unwrap();
        SetWindowLongPtrW(source.0,GWL_STYLE,(WS_POPUP|WS_VISIBLE) as isize);
        SetWindowPos(source.0,null_mut(),32,32,640,420,SWP_NOACTIVATE|SWP_NOZORDER|SWP_FRAMECHANGED);
        for repeat in 0..3{
        SetWindowRgn(source.0,ptr::null_mut(),1);RemovePropW(source.0,wide("TLauncher.Fixture.HighContrast").as_ptr());
        SetWindowLongPtrW(source.0,GWL_EXSTYLE,GetWindowLongPtrW(source.0,GWL_EXSTYLE)&!(WS_EX_LAYERED as isize));
        let poses=[0.0,0.05,0.25,0.5,0.85,0.97,1.0,0.85,0.5,0.0];
        for (name,color,axis,contrast) in [("white",0x00ffffff,None,false),("black",0,None,false),
            ("red",0x000000ff,Some(2),false),("green",0x0000ff00,Some(1),false),("blue",0x00ff0000,Some(0),false),
            ("high-contrast-rounded",0x00ffffff,None,true),("transparent-white",0x00ffffff,None,false)]{
            eprintln!("WGC_COLOR repeat={repeat} begin={name}");
            SetPropW(source.0,wide("TLauncher.Fixture.Solid").as_ptr(),(color as usize+1) as HANDLE);
            if contrast{
                SetPropW(source.0,wide("TLauncher.Fixture.HighContrast").as_ptr(),1usize as HANDLE);
                let region=CreateRoundRectRgn(0,0,640,420,24,24);assert!(!region.is_null());assert_ne!(SetWindowRgn(source.0,region,1),0);
            }
            InvalidateRect(source.0,std::ptr::null(),0);UpdateWindow(source.0);DwmFlush();
            if name=="transparent-white"{
                // Real premultiplied layered HWND, with a broad checker of
                // transparent/partly transparent pixels rather than merely a
                // renderer tint. This does not enable foreign layered leases.
                SetWindowRgn(source.0,ptr::null_mut(),1);
                SetWindowLongPtrW(source.0,GWL_EXSTYLE,GetWindowLongPtrW(source.0,GWL_EXSTYLE)|WS_EX_LAYERED as isize);
                let screen=GetDC(ptr::null_mut());let dc=CreateCompatibleDC(screen);
                assert!(!dc.is_null());
                let mut info:BITMAPINFO=std::mem::zeroed();info.bmiHeader.biSize=std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                info.bmiHeader.biWidth=640;info.bmiHeader.biHeight=-420;info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;
                let mut data=ptr::null_mut();let bitmap=CreateDIBSection(screen,&info,DIB_RGB_COLORS,&mut data,ptr::null_mut(),0);
                assert!(!bitmap.is_null()&&!data.is_null());let old=SelectObject(dc,bitmap);
                for y in 0..420{for x in 0..640{
                    let alpha:u32=if x<12||x>=628||y<12||y>=408{0}else if (x/32+y/32)%2==0{64}else{192};
                    data.cast::<u32>().add(y*640+x).write((alpha<<24)|alpha*0x00010101);
                }}
                let size=SIZE{cx:640,cy:420};let origin=POINT{x:0,y:0};let position=POINT{x:32,y:32};
                let blend=BLENDFUNCTION{BlendOp:AC_SRC_OVER as u8,BlendFlags:0,SourceConstantAlpha:255,AlphaFormat:AC_SRC_ALPHA as u8};
                let updated=UpdateLayeredWindow(source.0,screen,&position,&size,dc,&origin,0,&blend,ULW_ALPHA);
                SelectObject(dc,old);DeleteObject(bitmap);DeleteDC(dc);ReleaseDC(ptr::null_mut(),screen);
                assert_ne!(updated,0);DwmFlush();
            }
            let frame=pump_gpu_fixture(&source).expect("actual synthetic WGC color capture");
            eprintln!("WGC_COLOR captured={name}");
            let mut area:RECT=std::mem::zeroed();
            assert!(windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute(source.0,windows_sys::Win32::Graphics::Dwm::DWMWA_EXTENDED_FRAME_BOUNDS as u32,
                (&mut area as *mut RECT).cast(),std::mem::size_of::<RECT>() as u32)>=0);
            let icon=RECT{left:330,top:650,right:378,bottom:698};
            let pipeline=super::fluidwin_mesh::Pipeline::new(&frame.gpu.context()).unwrap();
            for effect in [super::motion::WindowEffect::Genie,super::motion::WindowEffect::Drop,super::motion::WindowEffect::Scale]{
                let layer={let _batch=frame.gpu.context();super::fluidwin_mesh::Layer::from_gpu(pipeline.clone(),&frame,area,icon,super::DockSide::Bottom,effect,96).unwrap()};
                eprintln!("WGC_COLOR layer={name}/{}",effect.name());
                assert!({let _batch=frame.gpu.context();layer.enable_evidence()});
                for (index,progress) in poses.into_iter().enumerate(){
                    let start=Instant::now();
                    loop{
                        let status={let _batch=frame.gpu.context();layer.paint(progress,255)};
                        if status==1{break;}assert!(status==2&&start.elapsed()<Duration::from_secs(1));std::thread::sleep(Duration::from_millis(8));
                    }
                    let (pixels,w,h)={let _batch=frame.gpu.context();layer.rendered_pixels().unwrap()};
                    if name=="transparent-white"&&index==0{
                        let partial=pixels.chunks_exact(4).filter(|p|p[3]>16&&p[3]<240).count();
                        assert!(partial>640*420/4,"actual WGC layered transparency must survive the first pose: {partial}");
                    }
                    if progress<0.85{assert!(pixels.chunks_exact(4).filter(|p|p[3]>16).count()>256,"blank {name} {} pose {index}",effect.name());}
                    for pixel in pixels.chunks_exact(4){
                        assert!(pixel[..3].iter().all(|&c|c<=pixel[3]),"premultiplied alpha across entire canvas");
                        let lo=*pixel[..3].iter().min().unwrap();let hi=*pixel[..3].iter().max().unwrap();
                        let neutral=hi.saturating_sub(lo)<=12; // upstream neutral Drop chip/shadow
                        let source_color=axis.is_some_and(|axis|{
                            let (a,b)=match axis{0=>(1,2),1=>(0,2),_=>(0,1)};
                            pixel[a].abs_diff(pixel[b])<=3&&pixel[axis]>=pixel[a].min(pixel[b])
                        });
                        assert!(neutral||source_color,"unexpected chroma {name} {} pose {index}: {pixel:?}",effect.name());
                    }
                    if let Some(dir)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){
                        let rgba=pixels.chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],p[3]]).collect::<Vec<_>>();
                        if repeat==0{image::save_buffer(std::path::PathBuf::from(dir).join(format!("motion-colors-{name}-{}-{index}.png",effect.name())),&rgba,w,h,image::ColorType::Rgba8).unwrap();}
                    }
                }
            }
        }
        }
    }
    println!("FLUIDWIN_COLOR_PERIMETERS actual_wgc=YES full_canvas=CHECKED white_black_rgb_high_contrast_rounded=EXERCISED source_transparency=EXERCISED owner_border_acceptance=FAIL");
}

// Explicit proof of the DWM thumbnail API on owned synthetic windows only.
// This is not the product transition backend: source freshness and interaction
// with native Windows animations still require owner-hardware acceptance.
#[cfg(test)]
#[test]
#[ignore="owned-window DWM transition feasibility on Actions desktop"]
fn native_dwm_scale_proxy_feasibility(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use windows_sys::Win32::Graphics::Dwm::*;
    unsafe{
        let source=window(true).unwrap();
        let dst=Fixture(CreateWindowExW(WS_EX_TOPMOST|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,
            wide("STATIC").as_ptr(),std::ptr::null(),WS_POPUP,480,60,450,300,
            null_mut(),null_mut(),GetModuleHandleW(std::ptr::null()),null_mut()));
        if dst.0.is_null(){println!("WINDOW_PROXY_POC=BLOCKED destination HWND unavailable");return;}
        ShowWindow(dst.0,SW_SHOWNOACTIVATE);
        let mut link=0isize;
        let hr=DwmRegisterThumbnail(dst.0,source.0,&mut link);
        if hr<0||link==0{println!("WINDOW_PROXY_POC=BLOCKED DWM registration {hr:#x}");return;}
        struct Thumbnail(isize);
        impl Drop for Thumbnail{fn drop(&mut self){unsafe{DwmUnregisterThumbnail(self.0);}}}
        let thumb=Thumbnail(link);
        let mut before:WINDOWPLACEMENT=std::mem::zeroed();before.length=std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        assert_ne!(GetWindowPlacement(source.0,&mut before),0);
        let update=|k:f32|{
            let left=(12.0+310.0*k).round() as i32;let top=(12.0+192.0*k).round() as i32;
            let size=(400.0-352.0*k).round() as i32;
            let props=DWM_THUMBNAIL_PROPERTIES{dwFlags:DWM_TNP_RECTDESTINATION|DWM_TNP_OPACITY|DWM_TNP_VISIBLE,
                rcDestination:RECT{left,top,right:left+size,bottom:top+(size as f32*0.58).round() as i32},
                opacity:(255.0-128.0*k).round() as u8,fVisible:1,..std::mem::zeroed()};
            DwmUpdateThumbnailProperties(thumb.0,&props)>=0
        };
        let mut size:SIZE=std::mem::zeroed();
        if DwmQueryThumbnailSourceSize(thumb.0,&mut size)<0||size.cx<=0||size.cy<=0||!update(0.0){
            println!("WINDOW_PROXY_POC=BLOCKED invalid DWM source or destination");return;
        }
        let mut submitted=0;
        for speed in [25,70,100,200]{
            let duration=30000.0/speed as f32;
            let start=Instant::now();
            loop{
                let t=(start.elapsed().as_secs_f32()*1000.0/duration).min(1.0);
                assert!(update(super::motion::ease(t)));
                submitted+=1;
                if t>=1.0{break;}
                std::thread::sleep(Duration::from_millis(16));
            }
            assert!(update(0.0));
        }
        ShowWindow(source.0,SW_SHOWMINNOACTIVE);
        let minimized=IsIconic(source.0)!=0;
        let valid_after_minimize=update(0.7);
        ShowWindow(source.0,SW_RESTORE);
        let mut after:WINDOWPLACEMENT=std::mem::zeroed();after.length=before.length;
        assert_ne!(GetWindowPlacement(source.0,&mut after),0);
        let placement=super::same_rect(before.rcNormalPosition,after.rcNormalPosition);
        println!("WINDOW_PROXY_POC api_frames={submitted} minimized={minimized} update_after_minimize={valid_after_minimize} placement_preserved={placement} pixel_freshness=NOT_VERIFIED native_overlap=NOT_VERIFIED");
        if !placement||IsIconic(source.0)!=0||!minimized{println!("WINDOW_PROXY_POC=BLOCKED native placement/visibility transition");}
    }
}

// Separate fixture process exercises the same external-HWND eligibility and
// persistent worker used by Stage, without touching a runner or owner app.
#[cfg(test)]
#[test]
#[ignore = "spawned only by the isolated native Stage worker test"]
fn native_worker_fixture_host(){
    let Some(path)=std::env::var_os("TUANPC_DESK_STAGE_FIXTURE_READY")else{return;};
    unsafe{
        let first=window(true).unwrap();
        let second=window(true).unwrap();
        SetWindowTextW(first.0,wide("Stage fixture one").as_ptr());
        SetWindowTextW(second.0,wide("Stage fixture two").as_ptr());
        SetWindowPos(second.0,null_mut(),12,296,WIDTH,HEIGHT,SWP_NOACTIVATE|SWP_NOZORDER);
        let ready=std::path::PathBuf::from(path);
        let tmp=ready.with_extension("tmp");
        std::fs::write(&tmp,format!("{} {}",first.0 as usize,second.0 as usize)).unwrap();
        std::fs::rename(tmp,ready).unwrap();
        let mut msg=std::mem::zeroed();
        while GetMessageW(&mut msg,null_mut(),0,0)>0{
            TranslateMessage(&msg);DispatchMessageW(&msg);
        }
    }
}

#[cfg(test)]
#[test]
#[ignore = "requires isolated Windows graphics session and a child fixture"]
fn native_stage_worker_keeps_independent_frames_through_minimize(){
    use super::{stage_snapshot as cache,stage_surface::Layout,WindowIdentity};
    use std::process::{Child,Command,Stdio};
    struct FixtureProcess(Child);
    impl Drop for FixtureProcess{
        fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}
    }
    struct Reset;
    impl Drop for Reset{fn drop(&mut self){cache::shutdown();}}
    let Some(dir)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE").map(std::path::PathBuf::from)else{return;};
    let path=dir.join(format!("stage-worker-ready-{}.txt",std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let child=FixtureProcess(Command::new(std::env::current_exe().unwrap())
        .args(["--ignored","--exact","dock::stage_capture_lab::native_worker_fixture_host"])
        .env("TUANPC_DESK_STAGE_FIXTURE_READY",&path).stdout(Stdio::null()).spawn().unwrap());
    let _reset=Reset;
    let started=Instant::now();
    let handles=loop{
        if let Ok(text)=std::fs::read_to_string(&path){
            let _=std::fs::remove_file(&path);
            break text.split_whitespace().map(|h|h.parse::<usize>().unwrap()).collect::<Vec<_>>();
        }
        assert!(started.elapsed()<Duration::from_secs(8),"child fixture did not start");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(handles.len(),2);
    let ids=handles.iter().map(|&h|unsafe{WindowIdentity::read(h as HWND).unwrap()}).collect::<Vec<_>>();
    assert!(ids.iter().all(|id|id.pid==child.0.id()));
    let mut layout=Layout::landscape(240,320,96,21,5,2,false,true);
    layout.count=2;
    let target=|i|cache::Target{owner:1,layout,index:i,reduced:true,dark:true,dpi:96,sources:[Some(ids[i]),None,None,None,None]};
    let both=[target(0),target(1)];
    let sync=||{cache::complete();cache::sync(&both,null_mut(),true,0);};
    let spin=|condition:&dyn Fn()->bool|{
        let start=Instant::now();
        loop{
            sync();
            if condition(){return true;}
            if start.elapsed()>=Duration::from_secs(7){return false;}
            std::thread::sleep(Duration::from_millis(25));
        }
    };
    let result=if !spin(&||cache::views(1).len()==2){
        let statuses=handles.iter().map(|&h|unsafe{super::previews::preview_pid(h as HWND)}).collect::<Vec<_>>();
        format!("STAGE_EXTERNAL_FIXTURE=BLOCKED eligibility={statuses:?} projected={}",cache::views(1).len())
    }else{
        let hwnd=handles[0] as HWND;
        assert_ne!(unsafe{ShowWindowAsync(hwnd,SW_MINIMIZE)},0);
        assert!(spin(&||unsafe{IsIconic(hwnd)!=0}));
        assert_eq!(cache::views(1).len(),2,"minimize removed a valid source frame or the other card");
        assert_eq!(cache::target(1,0).map(|v|v.1),Some(1));
        assert_ne!(unsafe{ShowWindowAsync(hwnd,SW_RESTORE)},0);
        assert!(spin(&||unsafe{IsIconic(hwnd)==0}));
        assert_eq!(cache::views(1).len(),2,"restore lost the last valid frame");
        assert_ne!(unsafe{PostMessageW(hwnd,WM_CLOSE,0,0)},0);
        assert!(spin(&||unsafe{IsWindow(hwnd)==0}));
        assert!(cache::target(1,0).is_none());
        assert_eq!(cache::target(1,1).map(|v|v.1),Some(1));
        "STAGE_EXTERNAL_FIXTURE=PASS two owned child HWNDs, independent previews, minimize/restore/close".to_owned()
    };
    println!("{result}");
    std::fs::write(dir.join("stage-external-fixture-result.txt"),result).unwrap();
}

#[cfg(test)]
#[test]
#[ignore="actual Dock activation, capture worker, compositor and restore on Actions"]
fn native_scale_production_owned_external_windows(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use super::WindowIdentity;
    use windows_sys::Win32::{Security::*,System::Threading::*};
    struct Handle(HANDLE);
    impl Drop for Handle{fn drop(&mut self){unsafe{if !self.0.is_null(){CloseHandle(self.0);}}}}
    struct FixtureProcess{process:Handle,pid:u32}
    impl Drop for FixtureProcess{fn drop(&mut self){unsafe{
        // The handle comes only from creating our known synthetic child.
        TerminateProcess(self.process.0,0);WaitForSingleObject(self.process.0,3000);
    }}}
    let dir=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE").map(std::path::PathBuf::from).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let path=dir.join(format!("scale-worker-ready-{}.txt",std::process::id()));
    let child=unsafe{
        let mut raw=null_mut();
        assert_ne!(OpenProcessToken(GetCurrentProcess(),TOKEN_DUPLICATE|TOKEN_QUERY|TOKEN_ASSIGN_PRIMARY,&mut raw),0);
        let original=Handle(raw);
        let mut restricted=null_mut();
        // This is a real Windows least-privilege token for the owned CI
        // fixture, not an override of the production elevation check.
        assert_ne!(CreateRestrictedToken(original.0,LUA_TOKEN|DISABLE_MAX_PRIVILEGE,0,null_mut(),0,null_mut(),0,null_mut(),&mut restricted),0,
            "restricted CI fixture token: {}",GetLastError());
        let restricted=Handle(restricted);
        let mut e:TOKEN_ELEVATION=std::mem::zeroed();let mut returned=0;
        assert_ne!(GetTokenInformation(restricted.0,TokenElevation,(&mut e as *mut TOKEN_ELEVATION).cast(),std::mem::size_of::<TOKEN_ELEVATION>() as u32,&mut returned),0);
        assert_eq!(e.TokenIsElevated,0,"runner did not grant a non-elevated fixture token");
        let exe=std::env::current_exe().unwrap();
        let mut command=wide(&format!("\"{}\" --ignored --exact dock::stage_capture_lab::native_worker_fixture_host",exe.display()));
        let app=wide(&exe.to_string_lossy());
        let mut start:STARTUPINFOW=std::mem::zeroed();start.cb=std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut info:PROCESS_INFORMATION=std::mem::zeroed();
        // Single ignored test process; inherit its environment with one
        // fixture-specific ready path, restoring it immediately after spawn.
        let key="TUANPC_DESK_STAGE_FIXTURE_READY";
        let old=std::env::var_os(key);std::env::set_var(key,&path);
        let ok=CreateProcessAsUserW(restricted.0,app.as_ptr(),command.as_mut_ptr(),null_mut(),null_mut(),0,
            0,null_mut(),null_mut(),&start,&mut info);
        let error=GetLastError();
        if let Some(old)=old{std::env::set_var(key,old);}else{std::env::remove_var(key);}
        assert_ne!(ok,0,"create non-elevated CI fixture: {error}");
        CloseHandle(info.hThread);
        FixtureProcess{process:Handle(info.hProcess),pid:info.dwProcessId}
    };
    let started=Instant::now();
    let handles=loop{
        if let Ok(text)=std::fs::read_to_string(&path){
            let _=std::fs::remove_file(&path);
            break text.split_whitespace().map(|h|h.parse::<usize>().unwrap()).collect::<Vec<_>>();
        }
        assert!(started.elapsed()<Duration::from_secs(8),"non-elevated child fixture did not start");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(handles.len(),2);
    let ids=handles.iter().map(|&h|unsafe{WindowIdentity::read(h as HWND).unwrap()}).collect::<Vec<_>>();
    assert!(ids.iter().all(|id|id.pid==child.pid));
    unsafe{super::window_effect::probe_production_path(&ids);}
}

#[cfg(test)]
#[test]
#[ignore="real hosted native-animation policy with detached Guardian and foreign motion handoff; restores runner policy afterwards"]
fn native_guarded_motion_with_windows_animations_enabled(){
    guarded_motion_fixture(false,None);
}
#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="same detached Guardian in compatibility mode, native Windows animation enabled; no taskbar mutation"]
fn native_guarded_motion_in_compatibility_mode(){
    guarded_motion_fixture(true,None);
}
#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="independent hosted Show Desktop cascade through the real foreign-window controller"]
fn native_guarded_desktop_cascade(){guarded_motion_fixture(false,Some("desktop"));}
#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="independent hosted prepared maximize/restore/Snap through the real foreign-window controller"]
fn native_guarded_geometry_commands(){guarded_motion_fixture(true,Some("geometry"));}
#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="native maximized drag handoff into live Jelly; hosted foreign HWND and existing Guardian"]
fn native_guarded_jelly_native_restore(){guarded_motion_fixture(true,Some("jelly"));}
#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="original Snap picker keyboard selection and guarded/native geometry on hosted foreign HWND"]
fn native_original_snap_layout(){guarded_motion_fixture(true,Some("snap-layout"));}
#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="hosted Always on Top through real command/hotkey handlers and detached Guardian"]
fn native_guarded_topmost_commands(){guarded_motion_fixture(true,Some("topmost"));}
#[cfg(all(test,target_env="msvc"))]
fn guarded_motion_fixture(compatibility:bool,scope:Option<&str>){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use std::ffi::OsString;
    struct Restore{animation:ANIMATIONINFO,env:Vec<(&'static str,Option<OsString>)>,compatibility:bool}
    impl Drop for Restore{fn drop(&mut self){unsafe{
        super::window_effect::reset();
        let child=if self.compatibility{super::motion_guard::take_fixture()}else{super::TASKBAR_WATCH.with(|w|w.borrow_mut().take().and_then(|w|w.stop()))};
        if let Some(mut child)=child{
            let start=Instant::now();let mut msg:MSG=std::mem::zeroed();
            while child.try_wait().ok().flatten().is_none()&&start.elapsed()<Duration::from_secs(5){
                while PeekMessageW(&mut msg,null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        SystemParametersInfoW(SPI_SETANIMATION,self.animation.cbSize,(&mut self.animation as *mut ANIMATIONINFO).cast(),0);
        for (key,value) in &self.env{if let Some(value)=value{std::env::set_var(key,value);}else{std::env::remove_var(key);}}
    }}}
    unsafe{
        let mut original:ANIMATIONINFO=std::mem::zeroed();original.cbSize=std::mem::size_of::<ANIMATIONINFO>() as u32;
        assert_ne!(SystemParametersInfoW(SPI_GETANIMATION,original.cbSize,(&mut original as *mut ANIMATIONINFO).cast(),0),0);
        let before=original.iMinAnimate;
        let restore=Restore{animation:original,compatibility,env:["TUANPC_GUARD_FIXTURE","TUANPC_GUARDED_MOTION_FIXTURE","TUANPC_MOTION_FIXTURE_SCOPE"].into_iter().map(|k|(k,std::env::var_os(k))).collect()};
        if let Some(scope)=scope{std::env::set_var("TUANPC_MOTION_FIXTURE_SCOPE",scope);}else{std::env::remove_var("TUANPC_MOTION_FIXTURE_SCOPE");}
        let mut enabled=ANIMATIONINFO{cbSize:std::mem::size_of::<ANIMATIONINFO>() as u32,iMinAnimate:1};
        assert_ne!(SystemParametersInfoW(SPI_SETANIMATION,enabled.cbSize,(&mut enabled as *mut ANIMATIONINFO).cast(),0),0);
        let mut observed:ANIMATIONINFO=std::mem::zeroed();observed.cbSize=enabled.cbSize;
        assert_ne!(SystemParametersInfoW(SPI_GETANIMATION,observed.cbSize,(&mut observed as *mut ANIMATIONINFO).cast(),0),0);assert_eq!(observed.iMinAnimate,1);
        std::env::set_var("TUANPC_GUARD_FIXTURE","{}");std::env::set_var("TUANPC_GUARDED_MOTION_FIXTURE","1");
        let mut guard=super::taskbar_guard::Watch::start_motion_fixture().unwrap();let start=Instant::now();
        while !guard.poll().unwrap()||guard.motion_session().is_none(){assert!(start.elapsed()<Duration::from_secs(8));std::thread::sleep(Duration::from_millis(20));}
        if compatibility{
            super::motion_guard::install_fixture(guard);assert!(!super::taskbar_active());
        }else{super::TASKBAR_WATCH.with(|w|w.replace(Some(guard)));}
        native_scale_production_owned_external_windows();
        drop(restore);
        assert_ne!(SystemParametersInfoW(SPI_GETANIMATION,observed.cbSize,(&mut observed as *mut ANIMATIONINFO).cast(),0),0);assert_eq!(observed.iMinAnimate,before);
    }
    println!("FLUIDWIN_GUARDED_MOTION mode={} scope={} windows_native_animation=ENABLED runner_policy=RESTORED owner_quality=NOT_VERIFIED",if compatibility{"compatibility"}else{"taskbar"},scope.unwrap_or("all"));
}
