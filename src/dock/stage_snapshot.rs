//! Bounded WGC source cache for Stage. Frames stay in this process and never
//! touch the filesystem. Capture is scoped to a permitted top-level window.
use std::{sync::mpsc,time::{Duration,Instant}};
use std::{cell::RefCell,collections::HashMap,sync::{Arc,atomic::{AtomicBool,Ordering}},thread::JoinHandle};
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

fn step<T>(s:&str,r:windows::core::Result<T>)->Result<T,String>{
    r.map_err(|e|format!("{s}: {e}"))
}
struct Apartment;
impl Apartment {
    unsafe fn enter()->Result<Self,String>{
        step("RoInitialize",RoInitialize(RO_INIT_MULTITHREADED)).map(|_|Self)
    }
}
impl Drop for Apartment { fn drop(&mut self){unsafe{RoUninitialize();}} }

struct CaptureDevice {
    device:ID3D11Device, context:ID3D11DeviceContext, winrt:IDirect3DDevice,
}
impl Drop for CaptureDevice{
    fn drop(&mut self){
        unsafe{
            // Release deferred D3D11 references before dropping the immediate
            // context and its DXGI device (also on cancellation).
            self.context.ClearState();
            self.context.Flush();
        }
    }
}
impl CaptureDevice {
    unsafe fn new()->Result<Self,String>{
        if !step("WGC support",GraphicsCaptureSession::IsSupported())?{
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
        Ok(Self{device,context,winrt})
    }
    unsafe fn frame(&self,hwnd:HWND,valid:impl Fn()->bool)->Result<Frame,String>{
        if !valid(){return Err("window identity, visibility or affinity denied".into());}
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetAncestor,GA_ROOT};
        if GetAncestor(hwnd,GA_ROOT)!=hwnd{return Err("WGC requires a top-level HWND".into());}
        let Self{device,context,winrt}=self;
        let interop=step("WGC item factory",factory::<GraphicsCaptureItem,IGraphicsCaptureItemInterop>())?;
        let item:GraphicsCaptureItem=step("CreateForWindow",interop.CreateForWindow(OwnedWindow(hwnd)))?;
        let size=step("WGC item size",item.Size())?;
        if size.Width<1||size.Height<1||!budget(size.Width,size.Height){return Err("window exceeds capture budget".into());}
        let pool=step("frame pool",Direct3D11CaptureFramePool::CreateFreeThreaded(
            winrt,DirectXPixelFormat::B8G8R8A8UIntNormalized,1,size))?;
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
                        let ptr=mapped.pData as *const u8;
                        if ptr.is_null()||mapped.RowPitch<(w*4){
                            context.Unmap(&staging,0);
                            return Err("invalid staging mapping".into());
                        }
                        let stride=mapped.RowPitch as usize;
                        let bytes=super::stage_projection::downsample_rows(w as usize,h as usize,dw,dh,|y|{
                            // Each mapped row has w initialized texels; do not
                            // form a slice over the driver's row padding.
                            std::slice::from_raw_parts(ptr.add(y*stride),w as usize*4)
                        });
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
}

pub(super) unsafe fn frame(hwnd:HWND,valid:impl Fn()->bool)->Result<Frame,String>{
    if !valid(){return Err("window identity, visibility or affinity denied".into());}
    let _apartment=Apartment::enter()?;
    CaptureDevice::new()?.frame(hwnd,valid)
}

pub(super) const READY:u32=WM_APP+58;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Target{
    pub owner:usize, pub layout:Layout, pub index:usize, pub reduced:bool,
    pub sources:[Option<WindowIdentity>;5],
}
impl Target{
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
    pub target:Target, pub mask:u8, bounds:Box2,idle:Arc<[u8]>,hover:Arc<[u8]>,
}
impl Preview{
    fn project(target:Target,frames:&[Option<&Frame>;5])->Self{
        let mask=frames.iter().enumerate().fold(0,|m,(i,f)|if f.is_some(){m|(1<<i)}else{m});
        let b=target.bounds();
        let fit=|hover|{
            let mut out=vec![0;b.w as usize*b.h as usize*4];
            for (layer,frame) in frames.iter().enumerate().rev(){
                let base=target.body(layer);
                let Some((data,w,h))=frame else{
                    if target.sources[layer].is_some(){
                        for y in base.y.max(b.y)..(base.y+base.h).min(b.y+b.h){
                            for x in base.x.max(b.x)..(base.x+base.w).min(b.x+b.w){
                                if target.layout.layer_hit(target.index,layer,x,y){
                                    let at=(((y-b.y)*b.w+x-b.x)*4) as usize;
                                    out[at..at+4].fill(0);
                                }
                            }
                        }
                    }
                    continue;
                };
                let slab=target.card(layer,Box2{x:base.x-b.x,y:base.y-b.y,..base},hover);
                slab.shadow(&mut out,b.w,b.h,layer);
                slab.paint_depth(&mut out,b.w,b.h,data,*w,*h,layer);
            }
            Arc::<[u8]>::from(out)
        };
        let idle=fit(false);
        let hover=if target.reduced{idle.clone()}else{fit(true)};
        Self{target,mask,bounds:b,idle,hover}
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
type Frame=(Vec<u8>,i32,i32);
const MAX_ENTRIES:usize=80;
const MAX_FRAME_BYTES:usize=40*1024*1024;
const MAX_PROJECTION_BYTES:usize=28*1024*1024;
const MAX_AGE:Duration=Duration::from_secs(30*60);

#[derive(Clone,PartialEq,Eq)]
struct Stamp{i:i32,j:i32,title:String}
#[derive(Default)]
struct Source{
    frame:Option<Frame>, revision:u64, last_used:Option<Instant>,
    stamp:Option<Stamp>, dirty:bool, retry:Option<Instant>, captured_at:Option<Instant>,
}
impl Source{
    fn observe(&mut self,stamp:Option<Stamp>,now:Instant){
        if self.stamp!=stamp{self.dirty=true;self.retry=None;self.stamp=stamp;}
        self.last_used=Some(now);
    }
    fn wants_frame(&self,now:Instant)->bool{
        self.stamp.is_some()&&(self.dirty||self.frame.is_none())&&self.retry.is_none_or(|after|now>=after)
    }
}
struct Job{
    source:WindowIdentity, stamp:Stamp, live:Arc<AtomicBool>,
    receiver:mpsc::Receiver<Result<Frame,String>>,
}
struct Request{
    source:WindowIdentity, stamp:Stamp, live:Arc<AtomicBool>,
    reply:mpsc::SyncSender<Result<Frame,String>>,
}
struct Worker{sender:mpsc::SyncSender<Request>,thread:JoinHandle<()>}
impl Worker{
    fn new(controller:usize)->std::io::Result<Self>{
        let (sender,rx)=mpsc::sync_channel::<Request>(1);
        let thread=std::thread::Builder::new().name("Deskland Stage capture".into()).spawn(move||unsafe{
            let apartment=Apartment::enter();
            let mut device=None;
            loop{
                let req=if device.is_some(){
                    match rx.recv_timeout(Duration::from_secs(15)){
                        Ok(req)=>req,
                        Err(mpsc::RecvTimeoutError::Timeout)=>{device=None;continue;},
                        Err(mpsc::RecvTimeoutError::Disconnected)=>break,
                    }
                }else if let Ok(req)=rx.recv(){req}else{break;};
                let valid=||req.live.load(Ordering::Relaxed)&&stamp(req.source).as_ref()==Some(&req.stamp);
                let result=(||{
                    if let Err(reason)=&apartment{return Err(reason.clone());}
                    if !valid(){return Err("capture cancelled".into());}
                    if device.is_none(){device=Some(CaptureDevice::new()?);}
                    device.as_ref().unwrap().frame(req.source.hwnd as HWND,valid)
                })();
                if result.is_err(){device=None;}
                let _=req.reply.send(result);
                PostMessageW(controller as HWND,READY,0,0);
            }
        })?;
        Ok(Self{sender,thread})
    }
    fn join(self){
        drop(self.sender);
        let _=self.thread.join();
    }
}
struct Raster{revisions:[u64;5],preview:Preview}
#[derive(Default)]
struct State{
    job:Option<Job>, worker:Option<Worker>, sources:HashMap<WindowIdentity,Source>, views:Vec<Raster>,
    generation:u64, next_capture:Option<Instant>, foreground:usize, visible:bool,
}
thread_local!{static CURRENT:RefCell<State>=RefCell::new(State::default());}

unsafe fn retainable(source:WindowIdentity)->bool{
    let hwnd=source.hwnd as HWND;
    source.matches(hwnd)&&super::previews::permitted_pid(hwnd)==Ok(source.pid)
}
unsafe fn stamp(source:WindowIdentity)->Option<Stamp>{
    use windows_sys::Win32::{Foundation::RECT,UI::WindowsAndMessaging::*};
    let hwnd=source.hwnd as HWND;
    if !retainable(source)||IsIconic(hwnd)!=0||IsWindowVisible(hwnd)==0{return None;}
    let mut r:RECT=std::mem::zeroed();
    if GetWindowRect(hwnd,&mut r)==0||!budget(r.right-r.left,r.bottom-r.top){return None;}
    let mut text=[0u16;256];
    let len=GetWindowTextW(hwnd,text.as_mut_ptr(),text.len() as i32).max(0) as usize;
    Some(Stamp{i:r.right-r.left,j:r.bottom-r.top,title:String::from_utf16_lossy(&text[..len])})
}
impl State{
    fn evict(&mut self,now:Instant){
        self.sources.retain(|_,s|s.last_used.is_some_and(|t|now.duration_since(t)<=MAX_AGE));
        for entry in self.sources.values_mut(){
            if entry.captured_at.is_some_and(|t|now.duration_since(t)>MAX_AGE){
                entry.frame=None;entry.captured_at=None;
            }
        }
        let mut keys=self.sources.iter().map(|(&id,s)|(id,s.last_used,s.frame.as_ref().map_or(0,|f|f.0.len()))).collect::<Vec<_>>();
        keys.sort_unstable_by_key(|(_,t,_)|*t);
        let mut len=keys.len();
        let mut bytes:usize=keys.iter().map(|k|k.2).sum();
        for (id,_,size) in keys{
            if len<=MAX_ENTRIES&&bytes<=MAX_FRAME_BYTES{break;}
            self.sources.remove(&id);
            len-=1;bytes-=size;
        }
    }
}

pub(super) fn sync(targets:&[Target],controller:HWND,enabled:bool,foreground:usize)->bool{
    CURRENT.with(|cell|{
        let mut state=cell.borrow_mut();
        let now=Instant::now();
        use windows_sys::Win32::UI::WindowsAndMessaging::{KillTimer,SetTimer};
        if !enabled{
            unsafe{KillTimer(controller,super::STAGE_CAPTURE_TIMER);}
            state.foreground=0;state.visible=false;state.next_capture=None;
            if let Some(j)=&state.job{j.live.store(false,Ordering::Relaxed);}
            state.sources.clear();
            let changed=!state.views.is_empty();
            state.views.clear();
            return changed;
        }
        // Don't initialize Windows Graphics Capture during brief Stage
        // reveals/reconfiguration. DWM supplies the first paint in the meantime.
        if !targets.is_empty()&&!state.visible{
            state.next_capture=Some(now+Duration::from_millis(350));
        }
        state.visible=!targets.is_empty();
        state.sources.retain(|&id,_|unsafe{retainable(id)});
        state.evict(now);
        if state.foreground!=foreground&&!targets.is_empty(){
            let previous=std::mem::replace(&mut state.foreground,foreground);
            if let Some((_,entry))=state.sources.iter_mut().find(|(id,_)|id.hwnd==previous){
                entry.dirty=true;
            }
        }
        // Interleave monitor/card layers so a deep stack cannot starve a
        // different monitor or the front of the next visible card.
        let mut visited=std::collections::HashSet::new();
        let mut wanted=Vec::new();
        for layer in 0..5{
            for t in targets{
                let Some(id)=t.sources[layer] else{continue;};
                if !visited.insert(id)||!unsafe{retainable(id)}{continue;}
                let entry=state.sources.entry(id).or_default();
                entry.observe(unsafe{stamp(id)},now);
                if entry.wants_frame(now){wanted.push(id);}
            }
        }
        state.evict(now);
        if let Some(job)=&state.job{
            if !visited.contains(&job.source)||state.sources.get(&job.source).and_then(|s|s.stamp.as_ref())!=Some(&job.stamp){
                job.live.store(false,Ordering::Relaxed);
            }
        }
        if state.job.is_none()&&!wanted.is_empty(){
            if let Some(delay)=state.next_capture.and_then(|t|t.checked_duration_since(now)){
                unsafe{SetTimer(controller,super::STAGE_CAPTURE_TIMER,delay.as_millis().clamp(10,350) as u32,None);}
            }
        }
        if state.job.is_none()&&state.next_capture.is_none_or(|time|now>=time){
            if let Some(id)=wanted.into_iter().find(|id|state.sources.contains_key(id)){
                let stamp=state.sources[&id].stamp.clone().unwrap();
                let live=Arc::new(AtomicBool::new(true));
                let (reply,receiver)=mpsc::sync_channel(1);
                if state.worker.is_none(){state.worker=Worker::new(controller as usize).ok();}
                let req=Request{source:id,stamp:stamp.clone(),live:live.clone(),reply};
                if state.worker.as_ref().is_some_and(|w|w.sender.try_send(req).is_ok()){
                    state.job=Some(Job{source:id,stamp,live,receiver});
                }
                state.next_capture=Some(now+Duration::from_millis(180));
            }
        }

        let old=std::mem::take(&mut state.views);
        let mut next=Vec::new();
        let mut bytes=0;
        let mut changed=false;
        for &target in targets{
            let b=target.bounds();
            if b.w<=0||b.h<=0||i64::from(b.w)*i64::from(b.h)>300_000{continue;}
            let frames=target.sources.map(|id|id.and_then(|id|state.sources.get(&id)).filter(|s|s.frame.is_some()));
            let revisions=frames.map(|v|v.map_or(0,|s|s.revision));
            if revisions==[0;5]{continue;}
            bytes+=b.w as usize*b.h as usize*4*if target.reduced{1}else{2};
            if bytes>MAX_PROJECTION_BYTES{break;}
            if let Some(previous)=old.iter().find(|p|p.preview.target==target&&p.revisions==revisions){
                next.push(Raster{revisions,preview:previous.preview.clone()});
            }else{
                changed=true;
                next.push(Raster{revisions,preview:Preview::project(target,&frames.map(|f|f.and_then(|s|s.frame.as_ref())))});
            }
        }
        changed|=old.len()!=next.len();
        state.views=next;
        changed
    })
}

pub(super) fn complete(){
    CURRENT.with(|cell|{
        let mut state=cell.borrow_mut();
        let Some(job)=state.job.take() else{return;};
        let result=job.receiver.try_recv();
        if matches!(result,Err(mpsc::TryRecvError::Empty)){state.job=Some(job);return;}
        if !job.live.load(Ordering::Relaxed)||unsafe{stamp(job.source)}.as_ref()!=Some(&job.stamp){return;}
        state.generation=state.generation.wrapping_add(1).max(1);
        let generation=state.generation;
        let Some(entry)=state.sources.get_mut(&job.source)else{return;};
        match result{
            Ok(Ok(f))=>{entry.frame=Some(f);entry.captured_at=Some(Instant::now());entry.revision=generation;entry.dirty=false;entry.retry=None;}
            _=>entry.retry=Some(Instant::now()+Duration::from_secs(30)),
        }
    });
}
pub(super) fn views(owner:usize)->Vec<Preview>{
    CURRENT.with(|c|c.borrow().views.iter().filter(|v|v.preview.target.owner==owner).map(|v|v.preview.clone()).collect())
}
pub(super) fn target(owner:usize,index:usize)->Option<(Target,u8)>{
    CURRENT.with(|c|c.borrow().views.iter().find(|v|v.preview.target.owner==owner&&v.preview.target.index==index)
        .map(|v|(v.preview.target,v.preview.mask)))
}
pub(super) fn forget(hwnd:HWND){
    CURRENT.with(|c|{
        let Ok(mut s)=c.try_borrow_mut()else{return;};
        s.sources.retain(|id,_|id.hwnd!=hwnd as usize);
        if let Some(job)=s.job.as_ref().filter(|j|j.source.hwnd==hwnd as usize){job.live.store(false,Ordering::Relaxed);}
        s.views.retain(|v|!v.preview.target.sources.iter().flatten().any(|id|id.hwnd==hwnd as usize));
    });
}
pub(super) fn hide(owner:usize){
    CURRENT.with(|c|c.borrow_mut().views.retain(|v|v.preview.target.owner!=owner));
}
pub(super) fn shutdown(){
    let worker=CURRENT.with(|c|{
        let mut s=c.borrow_mut();
        s.views.clear();s.sources.clear();
        if let Some(job)=s.job.take(){job.live.store(false,Ordering::Relaxed);}
        s.worker.take()
    });
    if let Some(worker)=worker{worker.join();}
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn minimize_has_no_capture_request_and_restore_keeps_last_frame(){
        let now=Instant::now();
        let stamp=Stamp{i:800,j:500,title:"Owned fixture".into()};
        let mut s=Source{frame:Some((vec![1,2,3,255],1,1)),stamp:Some(stamp.clone()),..Default::default()};
        s.observe(None,now);
        assert!(s.frame.is_some());
        assert!(!s.wants_frame(now));
        s.observe(Some(stamp),now+Duration::from_secs(1));
        assert!(s.wants_frame(now+Duration::from_secs(1)));
        assert_eq!(s.frame.as_ref().unwrap().0,[1,2,3,255]);
    }

    #[test]
    fn cache_evicts_old_entries_and_expires_frame_even_if_card_is_visible(){
        let now=Instant::now();
        let mut state=State::default();
        for i in 0..=MAX_ENTRIES{
            state.sources.insert(WindowIdentity{hwnd:i,pid:1,tid:2},Source{
                last_used:Some(now-Duration::from_millis(MAX_ENTRIES as u64-i as u64)),
                captured_at:Some(now-MAX_AGE-Duration::from_secs(1)),frame:Some((vec![1;4],1,1)),
                ..Default::default()
            });
        }
        state.evict(now);
        assert_eq!(state.sources.len(),MAX_ENTRIES);
        assert!(!state.sources.contains_key(&WindowIdentity{hwnd:0,pid:1,tid:2}));
        assert!(state.sources.values().all(|s|s.frame.is_none()));
    }

    #[test]
    fn a_missing_source_only_falls_back_in_its_own_layer(){
        let mut layout=Layout::landscape(262,300,96,16,5,1,false,true);
        layout.count=1;layout.layers[0]=3;
        let id=|hwnd|Some(WindowIdentity{hwnd,pid:11,tid:13});
        let target=Target{owner:9,layout,index:0,reduced:true,sources:[id(1),id(2),id(3),None,None]};
        let frame=(vec![198;160*90*4],160,90);
        let view=Preview::project(target,&[None,Some(&frame),Some(&frame),None,None]);
        assert_eq!(view.mask,0b110);
        let (x,y)=layout.card(0).center();
        let at=((y-view.bounds.y)*view.bounds.w+x-view.bounds.x) as usize*4;
        assert_eq!(view.idle[at+3],0,"rear snapshots cannot paint over the flat front");
        assert!(view.idle.chunks_exact(4).any(|p|p[3]>0));
    }

    #[test]
    fn projected_group_raster_retains_rear_hits_and_mirrors_without_losing_identity(){
        for right in [false,true]{
            let mut layout=Layout::landscape(262,300,96,16,4,1,right,true);
            layout.count=1;layout.layers[0]=3;
            let id=|hwnd|Some(WindowIdentity{hwnd,pid:11,tid:13});
            let tgt=Target{owner:9,layout,index:0,reduced:false,sources:[id(1),id(2),id(3),None,None]};
            let textures=[(vec![198;160*90*4],160,90),(vec![215;75*125*4],75,125),(vec![232;92*92*4],92,92)];
            let raster=Preview::project(tgt,&[Some(&textures[0]),Some(&textures[1]),Some(&textures[2]),None,None]);
            assert_ne!(raster.idle,raster.hover);
            for layer in 0..3{
                let b=tgt.body(layer);
                let x=if right{b.x+2}else{b.x+b.w-3};
                let y=b.y+b.h/2;
                assert_eq!((0..3).find(|&i|tgt.contains(i,x,y,false)),Some(layer));
                assert!(!tgt.contains(layer,x,b.y-2,false));
            }
            let mut pixels=vec![0;layout.width as usize*layout.height as usize*4];
            raster.blend(&mut pixels,layout.width,layout.height,false);
            assert!(pixels.chunks_exact(4).any(|p|p[3]==255));
            assert!(pixels.chunks_exact(4).any(|p|p[3]==0));
        }
    }
}
