//! Explicit isolated WGC diagnostic. Only the diagnostic's own painted
//! windows are eligible, and only this content can be exported.
use super::{stage_surface::Box2,stage_projection::Card,glass_probe::Layer};
use crate::desktop::wide;
use std::{path::Path,ptr::null_mut,sync::mpsc,time::{Duration,Instant}};
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
    if msg != WM_PAINT { return DefWindowProcW(hwnd,msg,w,l); }
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let dc = BeginPaint(hwnd,&mut ps);
    let mut client:RECT=std::mem::zeroed();GetClientRect(hwnd,&mut client);
    #[cfg(test)]
    {
        let solid=GetPropW(hwnd,wide("TLauncher.Fixture.Solid").as_ptr()) as usize;
        if solid!=0{
            let brush=CreateSolidBrush((solid-1) as u32);FillRect(dc,&client,brush);DeleteObject(brush);
            if !GetPropW(hwnd,wide("TLauncher.Fixture.HighContrast").as_ptr()).is_null(){
                for x in (0..client.right).step_by(4){FillRect(dc,&RECT{left:x,top:0,right:(x+2).min(client.right),bottom:client.bottom},GetStockObject(BLACK_BRUSH) as HBRUSH);}
            }
            EndPaint(hwnd,&ps);return 0;
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
    EndPaint(hwnd,&ps);
    0
}

unsafe fn window(normal:bool) -> Result<Fixture,String> {
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
        let poses=[0.0,0.05,0.25,0.5,0.85,0.97,1.0,0.85,0.5,0.0];
        for (name,color,axis,contrast) in [("white",0x00ffffff,None,false),("black",0,None,false),
            ("red",0x000000ff,Some(2),false),("green",0x0000ff00,Some(1),false),("blue",0x00ff0000,Some(0),false),
            ("high-contrast-rounded",0x00ffffff,None,true)]{
            SetPropW(source.0,wide("TLauncher.Fixture.Solid").as_ptr(),(color as usize+1) as HANDLE);
            if contrast{
                SetPropW(source.0,wide("TLauncher.Fixture.HighContrast").as_ptr(),1usize as HANDLE);
                let region=CreateRoundRectRgn(0,0,640,420,24,24);assert!(!region.is_null());assert_ne!(SetWindowRgn(source.0,region,1),0);
            }
            InvalidateRect(source.0,std::ptr::null(),0);UpdateWindow(source.0);DwmFlush();
            let frame=pump_gpu_fixture(&source).expect("actual synthetic WGC color capture");
            let mut area:RECT=std::mem::zeroed();
            assert!(windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute(source.0,windows_sys::Win32::Graphics::Dwm::DWMWA_EXTENDED_FRAME_BOUNDS as u32,
                (&mut area as *mut RECT).cast(),std::mem::size_of::<RECT>() as u32)>=0);
            let icon=RECT{left:330,top:650,right:378,bottom:698};
            let pipeline=super::fluidwin_mesh::Pipeline::new(&frame.gpu.context()).unwrap();
            for effect in [super::motion::WindowEffect::Genie,super::motion::WindowEffect::Drop,super::motion::WindowEffect::Scale]{
                let layer={let _batch=frame.gpu.context();super::fluidwin_mesh::Layer::from_gpu(pipeline.clone(),&frame,area,icon,super::DockSide::Bottom,effect,96).unwrap()};
                assert!({let _batch=frame.gpu.context();layer.enable_evidence()});
                for (index,progress) in poses.into_iter().enumerate(){
                    let start=Instant::now();
                    loop{
                        let status={let _batch=frame.gpu.context();layer.paint(progress,255)};
                        if status==1{break;}assert!(status==2&&start.elapsed()<Duration::from_secs(1));std::thread::sleep(Duration::from_millis(8));
                    }
                    let (pixels,w,h)={let _batch=frame.gpu.context();layer.rendered_pixels().unwrap()};
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
                        image::save_buffer(std::path::PathBuf::from(dir).join(format!("motion-colors-{name}-{}-{index}.png",effect.name())),&rgba,w,h,image::ColorType::Rgba8).unwrap();
                    }
                }
            }
        }
    }
    println!("FLUIDWIN_COLOR_PERIMETERS actual_wgc=YES full_canvas=CHECKED white_black_rgb_high_contrast_rounded=EXERCISED source_transparency=NOT_COVERED owner_border_acceptance=FAIL");
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
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use std::ffi::OsString;
    struct Restore{animation:ANIMATIONINFO,env:Vec<(&'static str,Option<OsString>)>}
    impl Drop for Restore{fn drop(&mut self){unsafe{
        super::window_effect::reset();
        let child=super::TASKBAR_WATCH.with(|w|w.borrow_mut().take().and_then(|w|w.stop()));
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
        let restore=Restore{animation:original,env:["TUANPC_GUARD_FIXTURE","TUANPC_GUARDED_MOTION_FIXTURE"].into_iter().map(|k|(k,std::env::var_os(k))).collect()};
        let mut enabled=ANIMATIONINFO{cbSize:std::mem::size_of::<ANIMATIONINFO>() as u32,iMinAnimate:1};
        assert_ne!(SystemParametersInfoW(SPI_SETANIMATION,enabled.cbSize,(&mut enabled as *mut ANIMATIONINFO).cast(),0),0);
        let mut observed:ANIMATIONINFO=std::mem::zeroed();observed.cbSize=enabled.cbSize;
        assert_ne!(SystemParametersInfoW(SPI_GETANIMATION,observed.cbSize,(&mut observed as *mut ANIMATIONINFO).cast(),0),0);assert_eq!(observed.iMinAnimate,1);
        std::env::set_var("TUANPC_GUARD_FIXTURE","{}");std::env::set_var("TUANPC_GUARDED_MOTION_FIXTURE","1");
        let mut guard=super::taskbar_guard::Watch::start_motion_fixture().unwrap();let start=Instant::now();
        while !guard.poll().unwrap()||guard.motion_session().is_none(){assert!(start.elapsed()<Duration::from_secs(8));std::thread::sleep(Duration::from_millis(20));}
        super::TASKBAR_WATCH.with(|w|w.replace(Some(guard)));
        native_scale_production_owned_external_windows();
        drop(restore);
        assert_ne!(SystemParametersInfoW(SPI_GETANIMATION,observed.cbSize,(&mut observed as *mut ANIMATIONINFO).cast(),0),0);assert_eq!(observed.iMinAnimate,before);
    }
    println!("FLUIDWIN_GUARDED_MOTION windows_native_animation=ENABLED scale_genie_drop=EXERCISED reversal=EXERCISED runner_policy=RESTORED owner_quality=NOT_VERIFIED");
}
