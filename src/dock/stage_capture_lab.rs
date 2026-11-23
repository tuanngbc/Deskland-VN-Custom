//! Test-only WGC probe. The only capture target is a synthetic, owned HWND.
//! No desktop/display capture, inactive app capture, or production integration.
use super::{Box2, Card, Layer};
use crate::desktop::wide;
use std::{ptr::null_mut, sync::mpsc, time::{Duration, Instant}};
use windows::{
    core::{factory, Interface},
    Foundation::TypedEventHandler,
    Graphics::{Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession},
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat}},
    Win32::{Foundation::{HMODULE, HWND as OwnedWindow}, Graphics::{
        Direct3D::D3D_DRIVER_TYPE_HARDWARE, Direct3D11::*,
        Dxgi::{Common::DXGI_FORMAT_B8G8R8A8_UNORM, IDXGIDevice, IDXGIAdapter}},
        System::WinRT::{
            Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
            Graphics::Capture::IGraphicsCaptureItemInterop,
            RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED,
        }},
};
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

unsafe fn snapshot(fixture: &Fixture) -> Result<(Vec<u8>,i32,i32),String> {
    if !allowed(fixture.0) { return Err("fixture identity or affinity denied".into()); }
    fn step<T>(s:&str,r:windows::core::Result<T>)->Result<T,String>{r.map_err(|e|format!("{s}: {e}"))}
    step("RoInitialize",RoInitialize(RO_INIT_MULTITHREADED))?;
    struct Apartment;
    impl Drop for Apartment { fn drop(&mut self){unsafe{RoUninitialize();}} }
    let _apartment=Apartment;
    if !step("WGC support",GraphicsCaptureSession::IsSupported())? {
        return Err("WGC not supported on this session".into());
    }
    let mut device=None;
    let mut context=None;
    step("D3D11 hardware device",D3D11CreateDevice(None::<&IDXGIAdapter>,D3D_DRIVER_TYPE_HARDWARE,
        HMODULE::default(),D3D11_CREATE_DEVICE_BGRA_SUPPORT,None,D3D11_SDK_VERSION,
        Some(&mut device),None,Some(&mut context)))?;
    let device=device.ok_or("missing D3D11 device")?;
    let context=context.ok_or("missing D3D11 context")?;
    let dxgi=step("DXGI device",device.cast::<IDXGIDevice>())?;
    let inspectable=step("WinRT D3D11 device",CreateDirect3D11DeviceFromDXGIDevice(&dxgi))?;
    let winrt=step("IDirect3DDevice",inspectable.cast::<IDirect3DDevice>())?;
    let interop=step("WGC item factory",factory::<GraphicsCaptureItem,IGraphicsCaptureItemInterop>())?;
    let item:GraphicsCaptureItem=step("CreateForWindow",interop.CreateForWindow(OwnedWindow(fixture.0)))?;
    let size=step("WGC item size",item.Size())?;
    if size.Width<1||size.Height<1||size.Width>512||size.Height>320{return Err("fixture exceeds capture budget".into());}
    let pool=step("frame pool",Direct3D11CaptureFramePool::CreateFreeThreaded(
        &winrt,DirectXPixelFormat::B8G8R8A8UIntNormalized,1,size))?;
    let result=(||{
        let session=step("capture session",pool.CreateCaptureSession(&item))?;
        let result=(||{
            step("omit cursor",session.SetIsCursorCaptureEnabled(false))?;
            let (tx,rx)=mpsc::sync_channel(1);
            let notify=TypedEventHandler::new(move|_,_|{let _=tx.try_send(());Ok(())});
            let token=step("frame event",pool.FrameArrived(&notify))?;
            let result=(||{
                step("StartCapture",session.StartCapture())?;
                rx.recv_timeout(Duration::from_secs(2)).map_err(|_|"no WGC frame within 2 s")?;
                let frame=step("read first frame",pool.TryGetNextFrame())?;
                let result=(||{
                    let size=step("frame content size",frame.ContentSize())?;
                    if size.Width<1||size.Height<1||size.Width>512||size.Height>320{
                        return Err("frame resized beyond the fixture limit".into());
                    }
                    let surface=step("frame surface",frame.Surface())?;
                    let interop=step("surface DXGI access",surface.cast::<IDirect3DDxgiInterfaceAccess>())?;
                    let texture:ID3D11Texture2D=step("frame texture",interop.GetInterface())?;
                    let mut desc=D3D11_TEXTURE2D_DESC::default();
                    texture.GetDesc(&mut desc);
                    let (w,h)=(size.Width as u32,size.Height as u32);
                    if desc.Format!=DXGI_FORMAT_B8G8R8A8_UNORM||desc.Width<w||desc.Height<h||
                        desc.Width>512||desc.Height>320||desc.SampleDesc.Count!=1{
                        return Err("unexpected WGC texture format or size".into());
                    }
                    desc.Width=w;desc.Height=h;desc.Usage=D3D11_USAGE_STAGING;desc.BindFlags=0;
                    desc.CPUAccessFlags=D3D11_CPU_ACCESS_READ.0 as _;desc.MiscFlags=0;
                    let mut staging=None;
                    step("fixture staging texture",device.CreateTexture2D(&desc,None,Some(&mut staging)))?;
                    let staging=staging.ok_or("missing staging texture")?;
                    let region=D3D11_BOX{left:0,top:0,front:0,right:w,bottom:h,back:1};
                    context.CopySubresourceRegion(&staging,0,0,0,0,&texture,0,Some(&region));
                    let mut mapped=D3D11_MAPPED_SUBRESOURCE::default();
                    step("read back fixture texture",context.Map(&staging,0,D3D11_MAP_READ,0,Some(&mut mapped)))?;
                    let pitch=w as usize*4;
                    let mut bytes=vec![0;pitch*h as usize];
                    for (y,row) in bytes.chunks_exact_mut(pitch).enumerate(){
                        std::ptr::copy_nonoverlapping((mapped.pData as *const u8).add(y*mapped.RowPitch as usize),row.as_mut_ptr(),pitch);
                    }
                    context.Unmap(&staging,0);
                    Ok((bytes,w as i32,h as i32))
                })();
                let _=frame.Close();
                result
            })();
            let _=pool.RemoveFrameArrived(token);
            result
        })();
        let _=session.Close();
        result
    })();
    let _=pool.Close();
    result
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
        let captured=snapshot(&fixture);
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
