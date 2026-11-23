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

struct Fixture(HWND);
impl Drop for Fixture {
    fn drop(&mut self) { unsafe { DestroyWindow(self.0); } }
}

unsafe extern "system" fn fixture_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg != WM_PAINT { return DefWindowProcW(hwnd,msg,w,l); }
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let dc = BeginPaint(hwnd,&mut ps);
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
