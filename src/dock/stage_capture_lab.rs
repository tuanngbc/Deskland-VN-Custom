//! Test-only WGC probe. The only capture target is a synthetic, owned HWND.
//! No desktop/display capture, inactive app capture, or production integration.
use super::{Box2, Card, Layer};
use crate::desktop::wide;
use std::{ptr::null_mut, time::Instant};
use windows_sys::Win32::{Foundation::*, Graphics::{Dwm::DwmFlush, Gdi::*},
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

unsafe fn window() -> Fixture {
    let name = wide(CLASS);
    let instance = GetModuleHandleW(std::ptr::null());
    let class = WNDCLASSW { lpfnWndProc: Some(fixture_proc), hInstance: instance,
        lpszClassName: name.as_ptr(), ..std::mem::zeroed() };
    assert_ne!(RegisterClassW(&class),0);
    let hwnd = CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE, name.as_ptr(), wide(CLASS).as_ptr(),
        WS_POPUP, 24,24,WIDTH,HEIGHT,null_mut(),null_mut(),instance,null_mut());
    assert!(!hwnd.is_null());
    ShowWindow(hwnd,SW_SHOWNOACTIVATE);
    UpdateWindow(hwnd);
    let _ = DwmFlush();
    Fixture(hwnd)
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


#[test]
#[ignore = "requires an isolated interactive Windows graphics session"]
fn native_wgc_projects_one_owned_hwnd_or_reports_blocked() {
    let Some(dir)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE").map(std::path::PathBuf::from)else{return;};
    std::fs::create_dir_all(&dir).unwrap();
    let started=Instant::now();
    let status=unsafe{
        let fixture=window();
        assert!(!allowed(GetDesktopWindow()),"the desktop is never an eligible source");
        let captured=crate::dock::stage_snapshot::frame(fixture.0,||allowed(fixture.0));
        if SetWindowDisplayAffinity(fixture.0,WDA_EXCLUDEFROMCAPTURE)!=0{
            assert!(!allowed(fixture.0),"protected fixture must be rejected");
        }
        match captured{
            Err(reason)=>format!("WGC_OWNED_HWND=BLOCKED: {reason}"),
            Ok((bytes,w,h))=>{
                let distinct=bytes.chunks_exact(4).filter(|p|p[1]>p[2].saturating_add(45)&&p[1]>105).count();
                assert!(distinct>100,"the green fixture marker must be present in the captured content");
                let mut dest=Layer::new(223,126).unwrap();
                let pixels=dest.pixels();
                pixels.fill(0);
                let slab=Card::new(Box2{x:9,y:9,w:198,h:108},false,28.0);
                slab.paint(pixels,223,126,&bytes,w,h);
                assert!(pixels.chunks_exact(4).any(|p|p[1]>p[2].saturating_add(45)&&p[1]>105));
                let rgba=pixels.chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],p[3]]).collect::<Vec<_>>();
                image::save_buffer(dir.join("stage-wgc-owned.png"),&rgba,223,126,image::ColorType::Rgba8).unwrap();
                format!("WGC_OWNED_HWND=PASS source={}x{} elapsed_ms={} preview=one_frame polygons={:?}",w,h,
                    started.elapsed().as_millis(),slab.polygon())
            }
        }
    };
    println!("{status}");
    std::fs::write(dir.join("stage-wgc-result.txt"),status).unwrap();
}
