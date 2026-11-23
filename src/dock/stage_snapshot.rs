//! Bounded one-shot WGC for explicitly selected Stage windows. Frames never
//! leave this process or touch the filesystem. No desktop capture or retries.
use std::{sync::mpsc,time::{Duration,Instant}};
use std::{cell::RefCell,sync::{Arc,atomic::{AtomicBool,Ordering}},thread::JoinHandle};
use super::{stage_surface::{Box2,Layout},stage_projection::Card,WindowIdentity};
use windows_sys::Win32::{Foundation::HWND,UI::WindowsAndMessaging::{WM_APP,PostMessageW}};
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

fn budget(w:i32,h:i32)->bool{
    w>0&&h>0&&w<=4096&&h<=4096&&i64::from(w)*i64::from(h)<=8_388_608
}
fn thumb_size(w:usize,h:usize)->(usize,usize){
    let side=w.max(h).max(360);
    ((w*360/side).max(1),(h*360/side).max(1))
}

pub(super) unsafe fn frame(hwnd: windows_sys::Win32::Foundation::HWND, valid:impl Fn()->bool) -> Result<(Vec<u8>,i32,i32),String> {
    if !valid() { return Err("window identity, visibility or affinity denied".into()); }
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetAncestor,GA_ROOT};
    if GetAncestor(hwnd,GA_ROOT)!=hwnd{return Err("WGC requires a top-level HWND".into());}
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
    let item:GraphicsCaptureItem=step("CreateForWindow",interop.CreateForWindow(OwnedWindow(hwnd)))?;
    let size=step("WGC item size",item.Size())?;
    if size.Width<1||size.Height<1||!budget(size.Width,size.Height){return Err("window exceeds capture budget".into());}
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
                let mut arrived=false;
                for _ in 0..20{
                    if !valid(){return Err("capture cancelled".into());}
                    match rx.recv_timeout(Duration::from_millis(100)){
                        Ok(())=>{arrived=true;break;}
                        Err(mpsc::RecvTimeoutError::Disconnected)=>break,
                        Err(mpsc::RecvTimeoutError::Timeout)=>{},
                    }
                }
                if !arrived{return Err("no WGC frame within 2 s".into());}
                if !valid(){return Err("window closed, changed or no longer visible".into());}
                let frame=step("read first frame",pool.TryGetNextFrame())?;
                let result=(||{
                    let size=step("frame content size",frame.ContentSize())?;
                    if size.Width<1||size.Height<1||!budget(size.Width,size.Height){
                        return Err("window resized beyond capture limit".into());
                    }
                    let surface=step("frame surface",frame.Surface())?;
                    let interop=step("surface DXGI access",surface.cast::<IDirect3DDxgiInterfaceAccess>())?;
                    let texture:ID3D11Texture2D=step("frame texture",interop.GetInterface())?;
                    let mut desc=D3D11_TEXTURE2D_DESC::default();
                    texture.GetDesc(&mut desc);
                    let (w,h)=(size.Width as u32,size.Height as u32);
                    if desc.Format!=DXGI_FORMAT_B8G8R8A8_UNORM||desc.Width<w||desc.Height<h||
                        !budget(desc.Width as i32,desc.Height as i32)||desc.SampleDesc.Count!=1{
                        return Err("unexpected WGC texture format or size".into());
                    }
                    desc.Width=w;desc.Height=h;desc.Usage=D3D11_USAGE_STAGING;desc.BindFlags=0;
                    desc.CPUAccessFlags=D3D11_CPU_ACCESS_READ.0 as _;desc.MiscFlags=0;
                    let mut staging=None;
                    step("WGC staging texture",device.CreateTexture2D(&desc,None,Some(&mut staging)))?;
                    let staging=staging.ok_or("missing staging texture")?;
                    let region=D3D11_BOX{left:0,top:0,front:0,right:w,bottom:h,back:1};
                    context.CopySubresourceRegion(&staging,0,0,0,0,&texture,0,Some(&region));
                    let mut mapped=D3D11_MAPPED_SUBRESOURCE::default();
                    step("WGC readback",context.Map(&staging,0,D3D11_MAP_READ,0,Some(&mut mapped)))?;
                    let (dw,dh)=thumb_size(w as usize,h as usize);
                    let mut bytes=vec![0;dw*dh*4];
                    let ptr=mapped.pData as *const u8;
                    if ptr.is_null()||mapped.RowPitch<(w*4){
                        context.Unmap(&staging,0);
                        return Err("invalid staging mapping".into());
                    }
                    for y in 0..dh{
                        let sy=(y*h as usize/dh)*mapped.RowPitch as usize;
                        for x in 0..dw{
                            let sx=x*w as usize/dw*4;
                            std::ptr::copy_nonoverlapping(ptr.add(sy+sx),bytes.as_mut_ptr().add((y*dw+x)*4),4);
                        }
                    }
                    context.Unmap(&staging,0);
                    if !valid(){return Err("window changed during capture".into());}
                    if !bytes.chunks_exact(4).any(|p|p[0]|p[1]|p[2]!=0){return Err("capture was blank".into());}
                    Ok((bytes,dw as i32,dh as i32))
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

pub(super) const READY:u32=WM_APP+58;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Target{
    pub owner:usize, pub layout:Layout, pub index:usize, pub reduced:bool,
    pub sources:[Option<WindowIdentity>;5],
}
impl Target{
    pub fn matches_members(self,group:Option<&Vec<usize>>)->bool{
        group.is_some_and(|members| {
            let n=self.sources.iter().flatten().count();
            n!=0&&n==members.len().min(5)&&self.sources.iter().flatten().map(|s|s.hwnd).eq(members.iter().copied().take(n))
        })
    }
    fn body(self,layer:usize)->Box2{self.layout.rear(self.index,layer as i32)}
    fn card(self,layer:usize,area:Box2,hover:bool)->Card{
        let yaw=if hover&&!self.reduced{19.0}else{28.0};
        Card::new(area,self.layout.right,yaw+layer as f32*0.45)
    }
    pub fn contains(self,layer:usize,x:i32,y:i32,hover:bool)->bool{
        self.sources.get(layer).is_some_and(Option::is_some)&&self.card(layer,self.body(layer),hover)
            .contains(x as f32+0.5,y as f32+0.5)
    }
    fn bounds(self)->Box2{
        let mut b=self.layout.card(self.index);
        let mut right=b.x+b.w;
        let mut bottom=b.y+b.h;
        for i in 1..self.sources.iter().flatten().count(){
            let r=self.body(i);
            b.x=b.x.min(r.x);b.y=b.y.min(r.y);
            right=right.max(r.x+r.w);bottom=bottom.max(r.y+r.h);
        }
        b.x=(b.x-11).max(0);b.y=(b.y-11).max(0);
        b.w=(right+11).min(self.layout.width)-b.x;
        b.h=(bottom+11).min(self.layout.height)-b.y;
        b
    }
}
#[derive(Clone)]
pub(super) struct Preview{
    pub target:Target, bounds:Box2,idle:Arc<[u8]>,hover:Arc<[u8]>,
}
impl Preview{
    fn project(target:Target,frames:&[(Vec<u8>,i32,i32)])->Self{
        let b=target.bounds();
        let fit=|hover|{
            let mut out=vec![0;b.w as usize*b.h as usize*4];
            for (layer,(data,w,h)) in frames.iter().enumerate().rev(){
                let base=target.body(layer);
                let slab=target.card(layer,Box2{x:base.x-b.x,y:base.y-b.y,..base},hover);
                slab.shadow(&mut out,b.w,b.h,layer);
                slab.paint_depth(&mut out,b.w,b.h,data,*w,*h,layer);
            }
            Arc::<[u8]>::from(out)
        };
        let idle=fit(false);
        let hover=if target.reduced{idle.clone()}else{fit(true)};
        Self{target,bounds:b,idle,hover}
    }
    pub(super) fn blend(&self,data:&mut[u8],w:i32,h:i32,hover:bool){
        let b=self.bounds;
        if b.x<0||b.y<0||b.w<=0||b.h<=0||b.x+b.w>w||b.y+b.h>h{return;}
        let ink=if hover{&self.hover}else{&self.idle};
        for (i,line) in ink.chunks_exact(b.w as usize*4).enumerate(){
            let at=((b.y as usize+i)*w as usize+b.x as usize)*4;
            for (s,d) in line.chunks_exact(4).zip(data[at..at+line.len()].chunks_exact_mut(4)){
                let a=u16::from(s[3]);
                if a!=0{for k in 0..4{d[k]=(u16::from(s[k])+(u16::from(d[k])*(255-a)+127)/255).min(255) as u8;}}
            }
        }
    }
}
struct Job{
    owner:usize,epoch:Arc<AtomicBool>,receiver:mpsc::Receiver<Result<Preview,String>>,thread:JoinHandle<()>,
}
#[derive(Default)]
struct State{job:Option<Job>,view:Option<Preview>}
thread_local!{static CURRENT:RefCell<State>=RefCell::new(State::default());}

unsafe fn eligible(source:WindowIdentity)->bool{
    let hwnd=source.hwnd as HWND;
    source.matches(hwnd)&&super::previews::preview_pid(hwnd)==Ok(source.pid)
}
pub(super) fn start(target:Target,controller:HWND)->Result<(),String>{
    if target.sources[0].is_none()||!target.sources.iter().flatten().all(|&source|unsafe{eligible(source)}){
        return Err("Cửa sổ đã ẩn hoặc Windows không cho phép chụp. Khôi phục các cửa sổ rồi thử lại.".into());
    }
    let b=target.bounds();
    if b.w<=0||b.h<=0||i64::from(b.w)*i64::from(b.h)>300_000{return Err("Thẻ vượt giới hạn kích cỡ.".into());}
    let control=controller as usize;
    CURRENT.with(|cell|{
        let mut state=cell.borrow_mut();
        if state.job.is_some(){return Err("Đang xử lý một ảnh phối cảnh.".into());}
        let (sender,receiver)=mpsc::sync_channel(1);
        let epoch=Arc::new(AtomicBool::new(true));let live=epoch.clone();
        let thread=std::thread::Builder::new().name("Deskland Stage snapshot".into()).spawn(move||unsafe{
            let started=Instant::now();
            let result=(||{
                let valid=||live.load(Ordering::Relaxed)&&started.elapsed()<Duration::from_secs(5)
                    &&target.sources.iter().flatten().all(|&source|eligible(source));
                let mut frames=Vec::with_capacity(5);
                for &source in target.sources.iter().flatten(){
                    frames.push(frame(source.hwnd as HWND,valid)?);
                }
                if !valid(){return Err("Ảnh đã hủy hoặc vượt thời gian cho phép.".into());}
                Ok(Preview::project(target,&frames))
            })();
            let _=sender.send(result);
            PostMessageW(control as HWND,READY,0,0);
        }).map_err(|e|e.to_string())?;
        state.job=Some(Job{owner:target.owner,epoch,receiver,thread});
        Ok(())
    })
}
pub(super) fn complete()->Option<Result<Preview,String>>{
    CURRENT.with(|cell|{
        let mut state=cell.borrow_mut();let job=state.job.take()?;
        let valid=job.epoch.load(Ordering::Relaxed);
        let result=job.receiver.try_recv();
        if matches!(result,Err(mpsc::TryRecvError::Empty)){state.job=Some(job);return None;}
        let _=job.thread.join();
        valid.then(||result.unwrap_or_else(|_|Err("Luồng chụp đã dừng.".into())))
    })
}
pub(super) fn install(view:Preview){CURRENT.with(|c|c.borrow_mut().view=Some(view));}
pub(super) fn view(owner:usize)->Option<Preview>{
    CURRENT.with(|c|c.borrow().view.as_ref().filter(|v|v.target.owner==owner).cloned())
}
pub(super) fn hide(owner:usize)->bool{
    CURRENT.with(|c|{
        let mut s=c.borrow_mut();
        if let Some(job)=s.job.as_ref().filter(|j|j.owner==owner){job.epoch.store(false,Ordering::Relaxed);}
        if s.view.as_ref().is_some_and(|v|v.target.owner==owner){s.view.take();true}else{false}
    })
}
pub(super) fn still_valid(view:&Preview)->bool{view.target.sources.iter().flatten().all(|&s|unsafe{eligible(s)})}
pub(super) fn shutdown(){
    let job=CURRENT.with(|c|{let mut s=c.borrow_mut();s.view.take();s.job.take()});
    if let Some(job)=job{job.epoch.store(false,Ordering::Relaxed);let _=job.thread.join();}
}
