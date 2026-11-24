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
    pub dark:bool,pub dpi:u32,
    pub sources:[Option<WindowIdentity>;5],
}
impl Target{
    fn body(self,layer:usize)->Box2{self.layout.rear(self.index,layer as i32)}
    fn card(self,layer:usize,area:Box2)->Card{
        Card::new(area,self.layout.right,28.0+layer as f32*0.45)
    }
    fn zoom(self,mask:u8,strength:u8)->f32{
        // A partial stack shares space with static compatibility layers.
        let full=(1u8<<self.sources.iter().flatten().count())-1;
        if self.reduced||mask!=full||strength==0{return 1.0;}
        let (x,y)=self.layout.card(self.index).center();
        let (x,y)=(x as f32,y as f32);
        let mut limit=1.055f32;
        for layer in 0..self.sources.iter().flatten().count(){
            let r=self.body(layer);
            let halo=(r.h as f32*0.067).clamp(3.0,10.0)+1.0;
            for (available,used) in [
                (x,x-r.x as f32+halo),
                (self.layout.width as f32-x,(r.x+r.w) as f32-x+halo),
                (y,y-r.y as f32+halo),
                (self.layout.height as f32-y,(r.y+r.h) as f32-y+halo),
            ]{
                if used>0.0{limit=limit.min((available/used).max(1.0));}
            }
        }
        1.0+(limit-1.0)*f32::from(strength)/255.0
    }
    pub fn contains(self,layer:usize,x:i32,y:i32,mask:u8,strength:u8)->bool{
        let (cx,cy)=self.layout.card(self.index).center();
        let scale=self.zoom(mask,strength);
        let unscale=|a:i32,c:i32|c as f32+(a as f32+0.5-c as f32)/scale;
        self.sources.get(layer).is_some_and(Option::is_some)&&self.card(layer,self.body(layer))
            .contains(unscale(x,cx),unscale(y,cy))
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
        // Leave room in the cached tile for the scaled halo as well as the
        // body. A fixed eleven-pixel halo clips when the tile itself grows.
        let pad=11+if self.reduced{0}else{
            let (x,y)=self.layout.card(self.index).center();
            let reach=(x-b.x).max(right-x).max(y-b.y).max(bottom-y)+11;
            (reach as f32*0.055).ceil() as i32+1
        };
        b.x=(b.x-pad).max(0);b.y=(b.y-pad).max(0);
        b.w=(right+pad).min(self.layout.width)-b.x;
        b.h=(bottom+pad).min(self.layout.height)-b.y;
        b
    }
}
#[derive(Clone)]
pub(super) struct Preview{
    pub target:Target, pub mask:u8, pub settings_placeholder:bool, bounds:Box2,idle:Arc<[u8]>,
}
impl Preview{
    fn project(target:Target,frames:&[Option<&Frame>;5])->Self{
        let settings_placeholder=frames[0].is_none()&&target.sources[0].is_some_and(|id|id.pid==std::process::id());
        let placeholder=settings_placeholder.then(settings_tile);
        let mut frames=*frames;
        if let Some(tile)=placeholder.as_ref(){frames[0]=Some(tile);}
        let mask=frames.iter().enumerate().fold(0,|m,(i,f)|if f.is_some(){m|(1<<i)}else{m});
        let b=target.bounds();
        let idle={
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
                let slab=target.card(layer,Box2{x:base.x-b.x,y:base.y-b.y,..base});
                slab.shadow(&mut out,b.w,b.h,layer);
                let active=layer==0&&target.layout.active==Some(target.index);
                let edge=if active{super::surface::Palette::new(target.dark).accent}
                    else if target.dark{[88,81,75]}else{[248,243,235]};
                let scale=target.dpi as f32/96.0;
                let (stroke,strength)=if active{(2.0*scale,0.74)}else{(0.9*scale,if target.dark{0.36}else{0.22})};
                slab.paint_face(&mut out,b.w,b.h,data,*w,*h,layer,true,Some((edge,stroke,strength)));
            }
            Arc::<[u8]>::from(out)
        };
        Self{target,mask,settings_placeholder,bounds:b,idle}
    }
    #[cfg(test)]
    pub(super) fn blend(&self,data:&mut[u8],w:i32,h:i32,strength:u8){
        self.blend_with_ink(data,w,h,strength,255);
    }
    pub(super) fn blend_with_ink(&self,data:&mut[u8],w:i32,h:i32,strength:u8,ink:u8){
        let b=self.bounds;
        if b.x<0||b.y<0||b.w<=0||b.h<=0||b.x+b.w>w||b.y+b.h>h{return;}
        let composite=|s:&[u8],d:&mut[u8]|{
            let a=u16::from(s[3]);
            if a!=0{
                for k in 0..4{
                    let src=u16::from(s[k]);
                    let src=if ink!=255&&k<3{(src*u16::from(ink)+127)/255}else{src};
                    d[k]=(src+(u16::from(d[k])*(255-a)+127)/255).min(255) as u8;
                }
            }
        };
        let scale=self.target.zoom(self.mask,strength);
        if scale!=1.0{
            let (cx,cy)=self.target.layout.card(self.target.index).center();
            let cx=(cx-b.x) as f32;let cy=(cy-b.y) as f32;
            // Bilinear sampling of the existing premultiplied tile. Only the
            // hovered card is resampled; we never capture at animation rate.
            let axis=|n:i32,c:f32,max:i32|{
                let a=(c+(n as f32+0.5-c)/scale-0.5).clamp(0.0,(max-1) as f32);
                let i=a as i32;(i,(i+1).min(max-1),((a-i as f32)*256.0) as u32)
            };
            for y in 0..b.h{
                let (y0,y1,fy)=axis(y,cy,b.h);
                for x in 0..b.w{
                    let (x0,x1,fx)=axis(x,cx,b.w);
                    let pixel=|x,y|{let at=((y*b.w+x)*4) as usize;&self.idle[at..at+4]};
                    let [p,q,r,s]=[pixel(x0,y0),pixel(x1,y0),pixel(x0,y1),pixel(x1,y1)];
                    if p[3]|q[3]|r[3]|s[3]==0{continue;}
                    let ink:[u8;4]=std::array::from_fn(|k|{
                        let top=u32::from(p[k])*(256-fx)+u32::from(q[k])*fx;
                        let bottom=u32::from(r[k])*(256-fx)+u32::from(s[k])*fx;
                        ((top*(256-fy)+bottom*fy+32768)>>16) as u8
                    });
                    let at=(((b.y+y)*w+b.x+x)*4) as usize;
                    composite(&ink,&mut data[at..at+4]);
                }
            }
            return;
        }
        for (i,line) in self.idle.chunks_exact(b.w as usize*4).enumerate(){
            let at=((b.y as usize+i)*w as usize+b.x as usize)*4;
            for (s,d) in line.chunks_exact(4).zip(data[at..at+line.len()].chunks_exact_mut(4)){
                composite(s,d);
            }
        }
    }
}
type Frame=(Vec<u8>,i32,i32);
fn settings_tile()->Frame{
    const W:i32=96;
    const H:i32=60;
    let mut pixels=Vec::with_capacity((W*H*4) as usize);
    for y in 0..H{
        for x in 0..W{
            let top=y<9;
            let menu=x<23;
            let chrome=top||menu;
            let line=!chrome&&x>31&&x<84&&matches!(y,20..=23|31..=33|43..=45);
            pixels.extend(if chrome{[72,65,58,255]}else if line{[106,95,84,255]}else{[53,48,43,255]});
        }
    }
    (pixels,W,H)
}
const MAX_ENTRIES:usize=80;
const MAX_FRAME_BYTES:usize=12*1024*1024;
const MAX_PROJECTION_BYTES:usize=12*1024*1024;
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
                    match rx.recv_timeout(Duration::from_secs(3)){
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
    if source.pid==std::process::id(){
        source.matches_stage()&&super::previews::permitted_settings_pid(hwnd)==Ok(source.pid)
    }else{
        source.matches(hwnd)&&super::previews::permitted_pid(hwnd)==Ok(source.pid)
    }
}
unsafe fn stamp(source:WindowIdentity)->Option<Stamp>{
    use windows_sys::Win32::{Foundation::RECT,UI::WindowsAndMessaging::*};
    let hwnd=source.hwnd as HWND;
    if !retainable(source)||IsIconic(hwnd)!=0||IsWindowVisible(hwnd)==0{return None;}
    let mut r:RECT=std::mem::zeroed();
    if GetWindowRect(hwnd,&mut r)==0||!budget(r.right-r.left,r.bottom-r.top){return None;}
    // Never synchronously ask the Settings UI thread for its title.
    let title=if source.pid==std::process::id(){super::internal_app::LABEL.into()}else{
        let mut text=[0u16;256];
        let len=GetWindowTextW(hwnd,text.as_mut_ptr(),text.len() as i32).max(0) as usize;
        String::from_utf16_lossy(&text[..len])
    };
    Some(Stamp{i:r.right-r.left,j:r.bottom-r.top,title})
}
impl State{
    fn evict(&mut self,now:Instant){
        self.sources.retain(|_,s|s.last_used.is_some_and(|t|now.duration_since(t)<=MAX_AGE));
        for entry in self.sources.values_mut(){
            if entry.captured_at.is_some_and(|t|now.duration_since(t)>MAX_AGE){
                entry.frame=None;entry.captured_at=None;
            }
        }
        let frame_bytes=|s:&Source|s.frame.as_ref().map_or(0,|f|f.0.len());
        let mut len=self.sources.len();
        let mut bytes:usize=self.sources.values().map(frame_bytes).sum();
        if len<=MAX_ENTRIES&&bytes<=MAX_FRAME_BYTES{return;}
        let mut keys=self.sources.iter().map(|(&id,s)|(id,s.last_used,frame_bytes(s))).collect::<Vec<_>>();
        keys.sort_unstable_by_key(|(_,t,_)|*t);
        for (id,_,size) in keys{
            if len<=MAX_ENTRIES&&bytes<=MAX_FRAME_BYTES{break;}
            self.sources.remove(&id);
            len-=1;bytes-=size;
        }
    }
}

fn mark_changed(changed:&mut Vec<usize>,owner:usize){
    if !changed.contains(&owner){changed.push(owner);}
}

fn check_removed(changed:&mut Vec<usize>,old:&[Raster],next:&[Raster]){
    // The number of tiles can stay the same when a tile disappears on one
    // monitor and a different monitor adds a tile in the same update.
    for previous in old{
        let target=previous.preview.target;
        if !changed.contains(&target.owner)&&!next.iter().any(|v|v.preview.target==target){
            mark_changed(changed,target.owner);
        }
    }
}

pub(super) fn sync(targets:&[Target],controller:HWND,enabled:bool,foreground:usize)->Vec<usize>{
    CURRENT.with(|cell|{
        let mut state=cell.borrow_mut();
        let now=Instant::now();
        use windows_sys::Win32::UI::WindowsAndMessaging::{KillTimer,SetTimer};
        if !enabled{
            unsafe{KillTimer(controller,super::STAGE_CAPTURE_TIMER);}
            state.foreground=0;state.visible=false;state.next_capture=None;
            if let Some(j)=&state.job{j.live.store(false,Ordering::Relaxed);}
            state.sources.clear();
            let mut changed=Vec::new();
            for raster in state.views.drain(..){mark_changed(&mut changed,raster.preview.target.owner);}
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
        // Reserve enough for the worst possible 360px square frame per
        // selected source. Prioritize front cards across displays. Deeper
        // layers can use DWM and never thrash this bounded snapshot cache.
        let max_visible=MAX_FRAME_BYTES/(360*360*4);
        for layer in 0..5{
            for t in targets{
                let Some(id)=t.sources[layer] else{continue;};
                if visited.len()>=max_visible||!visited.insert(id)||!unsafe{retainable(id)}{continue;}
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
        if targets.is_empty(){
            // A hidden Stage has no live projection or active capture. Retain
            // only the bounded source cache for minimized application previews.
            state.next_capture=None;
            unsafe{KillTimer(controller,super::STAGE_CAPTURE_TIMER);}
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
        let mut changed=Vec::new();
        for &target in targets{
            let b=target.bounds();
            if b.w<=0||b.h<=0||i64::from(b.w)*i64::from(b.h)>300_000{continue;}
            let frames=target.sources.map(|id|id.and_then(|id|state.sources.get(&id)).filter(|s|s.frame.is_some()));
            let revisions=frames.map(|v|v.map_or(0,|s|s.revision));
            if revisions==[0;5]&&!target.sources[0].is_some_and(|id|id.pid==std::process::id()){continue;}
            bytes+=b.w as usize*b.h as usize*4;
            if bytes>MAX_PROJECTION_BYTES{break;}
            if let Some(previous)=old.iter().find(|p|p.preview.target==target&&p.revisions==revisions){
                next.push(Raster{revisions,preview:previous.preview.clone()});
            }else{
                mark_changed(&mut changed,target.owner);
                next.push(Raster{revisions,preview:Preview::project(target,&frames.map(|f|f.and_then(|s|s.frame.as_ref())))});
            }
        }
        check_removed(&mut changed,&old,&next);
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
pub(super) fn masks(owner:usize)->[u8;8]{
    CURRENT.with(|c|{
        let mut masks=[0;8];
        for raster in &c.borrow().views{
            let view=&raster.preview;
            if view.target.owner==owner{masks[view.target.index]=view.mask;}
        }
        masks
    })
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
        let target=Target{owner:9,layout,index:0,reduced:true,dark:true,dpi:96,sources:[id(1),id(2),id(3),None,None]};
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
            let tgt=Target{owner:9,layout,index:0,reduced:false,dark:true,dpi:96,sources:[id(1),id(2),id(3),None,None]};
            let textures=[(vec![198;160*90*4],160,90),(vec![215;75*125*4],75,125),(vec![232;92*92*4],92,92)];
            let raster=Preview::project(tgt,&[Some(&textures[0]),Some(&textures[1]),Some(&textures[2]),None,None]);
            assert_eq!(tgt.zoom(7,0),1.0);
            assert!(tgt.zoom(7,128)>1.0&&tgt.zoom(7,128)<tgt.zoom(7,255));
            assert_eq!(tgt.zoom(5,255),1.0,"partial stacks keep the native layers aligned");
            for layer in 0..3{
                let b=tgt.body(layer);
                let x=if right{b.x+2}else{b.x+b.w-3};
                let y=b.y+b.h/2;
                assert_eq!((0..3).find(|&i|tgt.contains(i,x,y,7,0)),Some(layer));
                assert!(!tgt.contains(layer,x,b.y-2,0,0));
            }
            let mut pixels=vec![0;layout.width as usize*layout.height as usize*4];
            raster.blend(&mut pixels,layout.width,layout.height,0);
            assert!(pixels.chunks_exact(4).any(|p|p[3]==255));
            assert!(pixels.chunks_exact(4).any(|p|p[3]==0));
            let mut zoomed=vec![0;pixels.len()];
            raster.blend(&mut zoomed,layout.width,layout.height,180);
            assert_ne!(pixels,zoomed);
            assert!(zoomed.chunks_exact(4).all(|p|p[..3].iter().all(|&v|v<=p[3])));
        }
    }

    #[test]
    fn removing_a_tile_dirties_its_monitor_even_when_another_adds_one(){
        let layout=Layout::landscape(272,420,96,12,5,1,false,true);
        let raster=|owner|{
            let target=Target{owner,layout,index:0,reduced:true,dark:true,dpi:96,sources:[None;5]};
            Raster{revisions:[0;5],preview:Preview{target,mask:0,settings_placeholder:false,
                bounds:Box2{x:0,y:0,w:1,h:1},idle:Arc::from([])}}
        };
        let previous=[raster(1),raster(2)];
        let current=[raster(2),raster(3)];
        let mut changed=vec![3];
        check_removed(&mut changed,&previous,&current);
        assert_eq!(changed,[3,1]);
    }

    #[test]
    fn active_preview_keeps_a_transparent_edge_at_full_hover(){
        for right in [false,true]{
            let mut layout=Layout::landscape(272,420,96,12,5,1,right,true);
            layout.count=1;
            layout.active=Some(0);
            layout.height=layout.content_height();
            let id=WindowIdentity{hwnd:1,pid:11,tid:13};
            let target=Target{owner:9,layout,index:0,reduced:false,dark:true,dpi:96,sources:[Some(id),None,None,None,None]};
            let frame=(vec![210;160*90*4],160,90);
            let preview=Preview::project(target,&[Some(&frame),None,None,None,None]);
            let w=layout.width as usize;
            let mut pixels=vec![0;w*layout.height as usize*4];
            preview.blend(&mut pixels,layout.width,layout.height,255);
            assert!(target.zoom(1,255)>1.0);
            let edge=if right{w-1}else{0};
            assert!(pixels.chunks_exact(w*4).all(|row|row[edge*4+3]==0));
            assert!(pixels.chunks_exact(w*4).last().unwrap().iter().all(|&p|p==0));
        }
    }
}
