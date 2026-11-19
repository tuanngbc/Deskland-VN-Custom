//! Native hardware video playback. One atlas and one decoder share a timeline
//! across every monitor, including monitors above/left of the primary output.
//! The scene renderer supplies only its own pixels; desktop contents are never
//! captured. Cache files and encoder queues have explicit size limits.
use crate::desktop::{self,wide};
use std::{path::{Path,PathBuf},sync::{Arc,atomic::{AtomicBool,AtomicU32,Ordering},mpsc},time::{Duration,Instant}};
use windows::core::{implement,Interface,BSTR};
use windows::Win32::{Media::MediaFoundation::*,System::Com::*,Graphics::DirectComposition::*};
use windows_sys::Win32::{Foundation::*,UI::WindowsAndMessaging::*};

pub const FPS:u32=24;
const SECONDS:u32=12;
const MAX_FILE:u64=32*1024*1024;
const MAX_OVERLAP:usize=48*1024*1024;
const CLASS:&str="TuanpcDeskVideo";

#[derive(Clone,Copy)]
pub struct Layout {pub world:RECT,pub width:usize,pub height:usize,pub scale:f64}
impl Layout {
    pub fn new(rects:impl Iterator<Item=RECT>)->Option<Self>{
        let world=rects.reduce(|a,b|RECT{left:a.left.min(b.left),top:a.top.min(b.top),right:a.right.max(b.right),bottom:a.bottom.max(b.bottom)})?;
        let(w,h)=((world.right-world.left)as f64,(world.bottom-world.top)as f64);
        if w<=0.0||h<=0.0{return None;}
        // H.264 level/dimension and memory limits, even with many 4K monitors.
        let scale=0.72f64.min(3840.0/w).min(2160.0/h).min((8_000_000.0/(w*h)).sqrt());
        let width=((w*scale).round()as usize/2*2).max(2);
        let height=((h*scale).round()as usize/2*2).max(2);
        Some(Self{world,width,height,scale})
    }
    pub fn bytes(&self)->usize{self.width*self.height*4}
    pub fn copy_output(&self,out:&mut[u8],rect:RECT,sw:usize,sh:usize,bottom_up:&[u8]){
        let x0=((rect.left-self.world.left)as f64*self.scale).round()as usize;
        let y0=((rect.top-self.world.top)as f64*self.scale).round()as usize;
        let x1=(((rect.right-self.world.left)as f64*self.scale).round()as usize).min(self.width);
        let y1=(((rect.bottom-self.world.top)as f64*self.scale).round()as usize).min(self.height);
        if x1<=x0||y1<=y0||bottom_up.len()!=sw*sh*4||out.len()!=self.bytes(){return;}
        for y in y0..y1 {
            let sy=sh-1-(y-y0)*sh/(y1-y0);
            let dst=&mut out[(y*self.width+x0)*4..(y*self.width+x1)*4];
            let src=&bottom_up[sy*sw*4..(sy+1)*sw*4];
            if x1-x0==sw{dst.copy_from_slice(src);}else{for(x,p)in dst.chunks_exact_mut(4).enumerate(){let sx=x*sw/(x1-x0);p.copy_from_slice(&src[sx*4..sx*4+4]);}}
        }
    }
}

struct Platform;
impl Platform {unsafe fn new()->Result<Self,String>{
    CoInitializeEx(None,COINIT_APARTMENTTHREADED).ok().map_err(|e|e.to_string())?;
    if let Err(e)=MFStartup(MF_VERSION,MFSTARTUP_FULL){CoUninitialize();return Err(e.to_string());}Ok(Self)
}}
impl Drop for Platform{fn drop(&mut self){unsafe{let _=MFShutdown();CoUninitialize();}}}

#[implement(IMFMediaEngineNotify)]
struct Notify{loaded:Arc<AtomicBool>,error:Arc<AtomicU32>}
impl IMFMediaEngineNotify_Impl for Notify_Impl {
    fn EventNotify(&self,event:u32,_p1:usize,p2:u32)->windows::core::Result<()>{
        if event==MF_MEDIA_ENGINE_EVENT_LOADEDMETADATA.0 as u32{self.loaded.store(true,Ordering::Release);}
        if event==MF_MEDIA_ENGINE_EVENT_ERROR.0 as u32{self.error.store(p2.max(1),Ordering::Release);}
        Ok(())
    }
}

unsafe extern "system" fn video_proc(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{match msg{
    WM_ERASEBKGND=>1,WM_NCHITTEST=>HTTRANSPARENT as _,WM_MOUSEACTIVATE=>MA_NOACTIVATE as _,
    _=>DefWindowProcW(hwnd,msg,w,l)
}}
pub struct Player {
    // Explicit shutdown before HWND/platform teardown; callback owns no player.
    engine:IMFMediaEngineEx,device:IDCompositionDevice,target:IDCompositionTarget,
    visual:Option<IDCompositionVisual>,surface:Option<windows::core::IUnknown>,
    hwnd:HWND,attach:Option<desktop::Attach>,loaded:Arc<AtomicBool>,error:Arc<AtomicU32>,
    ready:bool,paused:bool,started:Instant,world:RECT,_platform:Platform,
}
impl Player {
    pub fn new(path:&Path,world:RECT,_headless:bool)->Result<Self,String>{unsafe{
        let platform=Platform::new()?;
        let cls=wide(CLASS);let mut wc:WNDCLASSW=std::mem::zeroed();wc.lpfnWndProc=Some(video_proc);wc.lpszClassName=cls.as_ptr();RegisterClassW(&wc);
        let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,cls.as_ptr(),cls.as_ptr(),WS_POPUP,world.left,world.top,world.right-world.left,world.bottom-world.top,std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null());
        if hwnd.is_null(){return Err("Không tạo được cửa sổ video".into());}
        let result=(||->windows::core::Result<(IMFMediaEngineEx,IDCompositionDevice,IDCompositionTarget,Arc<AtomicBool>,Arc<AtomicU32>)>{
            let loaded=Arc::new(AtomicBool::new(false));let error=Arc::new(AtomicU32::new(0));
            let callback:IMFMediaEngineNotify=Notify{loaded:loaded.clone(),error:error.clone()}.into();
            let mut attrs=None;MFCreateAttributes(&mut attrs,3)?;let attrs=attrs.unwrap();
            attrs.SetUnknown(&MF_MEDIA_ENGINE_CALLBACK,&callback)?;
            attrs.SetUINT64(&MF_MEDIA_ENGINE_OPM_HWND,hwnd as u64)?;
            use windows::Win32::Graphics::{Direct3D::*,Direct3D11::*};
            let mut gpu=None;
            D3D11CreateDevice(None,D3D_DRIVER_TYPE_HARDWARE,windows::Win32::Foundation::HMODULE::default(),D3D11_CREATE_DEVICE_BGRA_SUPPORT|D3D11_CREATE_DEVICE_VIDEO_SUPPORT,None,D3D11_SDK_VERSION,Some(&mut gpu),None,None)?;
            let gpu=gpu.unwrap();let protected:ID3D11Multithread=gpu.cast()?;protected.SetMultithreadProtected(true);
            let mut token=0;let mut manager=None;MFCreateDXGIDeviceManager(&mut token,&mut manager)?;let manager=manager.unwrap();manager.ResetDevice(&gpu,token)?;
            attrs.SetUnknown(&MF_MEDIA_ENGINE_DXGI_MANAGER,&manager)?;
            let factory:IMFMediaEngineClassFactory=CoCreateInstance(&CLSID_MFMediaEngineClassFactory,None,CLSCTX_INPROC_SERVER)?;
            let base=factory.CreateInstance(MF_MEDIA_ENGINE_FORCEMUTE.0 as u32,&attrs)?;
            let engine:IMFMediaEngineEx=base.cast()?;
            engine.SetLoop(true)?;engine.SetMuted(true)?;engine.SetAutoPlay(false)?;
            let dxgi:windows::Win32::Graphics::Dxgi::IDXGIDevice=gpu.cast()?;
            let device:IDCompositionDevice=DCompositionCreateDevice(&dxgi)?;
            let target=device.CreateTargetForHwnd(windows::Win32::Foundation::HWND(hwnd),true)?;
            engine.SetSource(&BSTR::from(path.to_string_lossy().as_ref()))?;
            Ok((engine,device,target,loaded,error))
        })();
        let(engine,device,target,loaded,error)=match result{Ok(v)=>v,Err(e)=>{DestroyWindow(hwnd);return Err(e.to_string());}};
        // Attach only once the first decoded frame is ready. Until then the
        // old wallpaper remains visible; never publish a black loading window.
        Ok(Self{engine,device,target,visual:None,surface:None,hwnd,attach:None,loaded,error,ready:false,paused:true,started:Instant::now(),world,_platform:platform})
    }}
    pub fn tick(&mut self,pause:bool,headless:bool)->Result<bool,String>{unsafe{
        let error=self.error.load(Ordering::Acquire);if error!=0{return Err(format!("Giải mã video lỗi 0x{error:08x}"));}
        if !self.ready&&self.loaded.load(Ordering::Acquire){
            self.engine.EnableWindowlessSwapchainMode(true).map_err(|e|format!("Video swapchain: {e}"))?;
            let dest=windows::Win32::Foundation::RECT{left:0,top:0,right:self.world.right-self.world.left,bottom:self.world.bottom-self.world.top};
            self.engine.UpdateVideoStream(None,Some(&dest),None).map_err(|e|e.to_string())?;
            let handle=self.engine.GetVideoSwapchainHandle().map_err(|e|e.to_string())?;
            let surface=self.device.CreateSurfaceFromHandle(handle).map_err(|e|e.to_string())?;
            // The surface holds its own reference to the kernel composition
            // resource; the duplicated handle returned by MediaEngine is ours.
            let _=windows::Win32::Foundation::CloseHandle(handle);
            let visual=self.device.CreateVisual().map_err(|e|e.to_string())?;
            visual.SetContent(&surface).map_err(|e|e.to_string())?;
            self.target.SetRoot(&visual).map_err(|e|e.to_string())?;self.device.Commit().map_err(|e|e.to_string())?;
            self.visual=Some(visual);self.surface=Some(surface);
            if !headless {self.attach=Some(desktop::attach(self.hwnd,self.world).ok_or("Không gắn được video vào desktop")?);}
            self.ready=true;crate::log!("Video nền: DirectComposition sẵn sàng, một timeline cho tất cả màn hình");
        }
        if !self.ready&&self.started.elapsed()>Duration::from_secs(12){return Err("Quá thời gian mở video nền".into());}
        if self.ready&&pause!=self.paused {if pause{self.engine.Pause()}else{self.engine.Play()}.map_err(|e|e.to_string())?;self.paused=pause;}
        if let Some(a)=&self.attach{desktop::ensure_z_order(self.hwnd,a);}
        Ok(self.ready)
    }}
    pub fn ready(&self)->bool{self.ready}
    pub fn time(&self)->f64{unsafe{self.engine.GetCurrentTime()}}
}
impl Drop for Player {fn drop(&mut self){unsafe{
    let _=self.engine.Shutdown();let _=self.target.SetRoot(None::<&IDCompositionVisual>);let _=self.device.Commit();
    self.visual=None;self.surface=None;
    if let Some(a)=self.attach.take(){desktop::detach(self.hwnd,&a);}DestroyWindow(self.hwnd);
}}}

fn packed(a:u32,b:u32)->u64{((a as u64)<<32)|b as u64}
unsafe fn video_type(layout:Layout,encoded:bool)->windows::core::Result<IMFMediaType>{
    let t=MFCreateMediaType()?;t.SetGUID(&MF_MT_MAJOR_TYPE,&MFMediaType_Video)?;t.SetGUID(&MF_MT_SUBTYPE,if encoded{&MFVideoFormat_H264}else{&MFVideoFormat_NV12})?;
    t.SetUINT64(&MF_MT_FRAME_SIZE,packed(layout.width as u32,layout.height as u32))?;t.SetUINT64(&MF_MT_FRAME_RATE,packed(FPS,1))?;t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO,packed(1,1))?;
    t.SetUINT32(&MF_MT_INTERLACE_MODE,MFVideoInterlace_Progressive.0 as u32)?;t.SetUINT32(&MF_MT_YUV_MATRIX,MFVideoTransferMatrix_BT709.0 as u32)?;
    if encoded{t.SetUINT32(&MF_MT_AVG_BITRATE,((layout.width*layout.height*FPS as usize/6)as u32).clamp(2_000_000,16_000_000))?;}Ok(t)
}
unsafe fn sample(bytes:&[u8],index:usize)->windows::core::Result<IMFSample>{
    let buffer=MFCreateMemoryBuffer(bytes.len()as u32)?;let mut ptr=std::ptr::null_mut();buffer.Lock(&mut ptr,None,None)?;std::ptr::copy_nonoverlapping(bytes.as_ptr(),ptr,bytes.len());buffer.Unlock()?;buffer.SetCurrentLength(bytes.len()as u32)?;
    let s=MFCreateSample()?;s.AddBuffer(&buffer)?;let start=index as i64*10_000_000/FPS as i64;s.SetSampleTime(start)?;s.SetSampleDuration((index+1)as i64*10_000_000/FPS as i64-start)?;Ok(s)
}

// Share the proven BT.709 converter with the desktop recorder.
fn nv12(bgra:&[u8],layout:Layout,out:&mut[u8]){crate::mini_tools::recorder::nv12(bgra,layout.width,layout.height,out);}

pub struct Bake {
    pub key:String,pub layout:Layout,pub captured:usize,pub next:f64,pub start:f64,
    input:Option<mpsc::SyncSender<Vec<u8>>>,recycle:mpsc::Receiver<Vec<u8>>,
    result:mpsc::Receiver<Result<PathBuf,String>>,cancel:Arc<AtomicBool>,pub path:PathBuf,
}
impl Bake {
    pub fn start(key:String,layout:Layout,path:PathBuf,now:f64)->Self{
        let(tx,rx)=mpsc::sync_channel::<Vec<u8>>(1);let(back_tx,back_rx)=mpsc::sync_channel(1);let(done_tx,done_rx)=mpsc::channel();let cancel=Arc::new(AtomicBool::new(false));let flag=cancel.clone();let file=path.clone();
        std::thread::spawn(move||{
            let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||encode(layout,&file,rx,back_tx,&flag)))
                .unwrap_or_else(|_|Err("Bộ tạo video nền dừng đột ngột".into()));
            if result.is_err(){let _=std::fs::remove_file(&file);}let _=done_tx.send(result.map(|_|file));
        });
        Self{key,layout,captured:0,next:now,start:now,input:Some(tx),recycle:back_rx,result:done_rx,cancel,path}
    }
    pub fn buffer(&self)->Vec<u8>{self.recycle.try_recv().unwrap_or_else(|_|vec![0;self.layout.bytes()])}
    pub fn submit(&mut self,frame:Vec<u8>,now:f64){
        let Some(tx)=self.input.as_ref()else{return};
        if tx.try_send(frame).is_ok(){self.captured+=1;self.next=now+1000.0/FPS as f64;}
        if self.captured>=FPS as usize*SECONDS as usize{self.input=None;}
    }
    pub fn due(&self,now:f64)->bool{self.input.is_some()&&now>=self.next}
    pub fn poll(&self)->Option<Result<PathBuf,String>>{self.result.try_recv().ok()}
    pub fn cancel(&mut self){self.cancel.store(true,Ordering::Relaxed);self.input=None;}
}
impl Drop for Bake{fn drop(&mut self){self.cancel.store(true,Ordering::Relaxed);self.input=None;}}
fn encode(layout:Layout,path:&Path,input:mpsc::Receiver<Vec<u8>>,recycle:mpsc::SyncSender<Vec<u8>>,cancel:&AtomicBool)->Result<(),String>{unsafe{
    let _platform=Platform::new()?;
    let result=(||->windows::core::Result<()>{
        let mut attrs=None;MFCreateAttributes(&mut attrs,2)?;let attrs=attrs.unwrap();attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS,1)?;
        let writer=MFCreateSinkWriterFromURL(windows::core::PCWSTR(wide(&path.to_string_lossy()).as_ptr()),None,&attrs)?;
        let id=writer.AddStream(&video_type(layout,true)?)?;writer.SetInputMediaType(id,&video_type(layout,false)?,None)?;writer.BeginWriting()?;
        let n=layout.width*layout.height*3/2;let overlap=(FPS as usize/2).min(MAX_OVERLAP/n).max(1);
        let mut first=Vec::with_capacity(overlap);let mut pixels=vec![0;n];let total=FPS as usize*SECONDS as usize;let mut written=0;
        for (index,mut bgra) in input.into_iter().enumerate(){
            if cancel.load(Ordering::Relaxed){break;}
            nv12(&bgra,layout,&mut pixels);bgra.fill(0);let _=recycle.try_send(bgra);
            if index<overlap{first.push(pixels.clone());continue;}
            // Overlap the final half-second with the omitted opening frames.
            // The next loop starts at exactly the following simulation frame.
            if index>=total-overlap{let i=index-(total-overlap);blend(&mut pixels,&first[i],i+1,overlap);}
            writer.WriteSample(id,&sample(&pixels,written)?)?;written+=1;
            if index%24==0&&std::fs::metadata(path).map_or(false,|m|m.len()>MAX_FILE){return Err(windows::core::Error::from_hresult(windows::core::HRESULT(0x80070070u32 as i32)));}
        }
        if cancel.load(Ordering::Relaxed)||written!=total-overlap{return Err(windows::core::Error::from_hresult(windows::core::HRESULT(0x80004004u32 as i32)));}
        writer.Finalize()?;Ok(())
    })();
    result.map_err(|e|format!("Tạo video nền: {e}"))?;
    if std::fs::metadata(path).map_err(|e|e.to_string())?.len()>MAX_FILE{return Err("Video nền vượt giới hạn 32 MB".into());}Ok(())
}}
fn blend(frame:&mut[u8],opening:&[u8],weight:usize,total:usize){for(a,b)in frame.iter_mut().zip(opening){*a=(((*a as usize)*(total-weight)+(*b as usize)*weight+total/2)/total)as u8;}}

/// Runtime diagnostic on the user's GPU without attaching to the desktop,
/// changing preferences, or requiring a local Rust installation.
pub fn probe()->Result<serde_json::Value,String>{unsafe{
    let path=std::env::temp_dir().join(format!("tuanpc-desk-video-probe-{}.mp4",std::process::id()));
    let result=(||->Result<serde_json::Value,String>{
        let layout=Layout{world:RECT{left:0,top:0,right:64,bottom:32},width:64,height:32,scale:1.0};
        let(tx,rx)=mpsc::sync_channel(1);let(back,_)=mpsc::sync_channel(1);
        let producer=std::thread::spawn(move||{for i in 0..FPS*SECONDS{let mut pixels=vec![0;layout.bytes()];for p in pixels.chunks_exact_mut(4){p.copy_from_slice(&[i as u8,80,200,255]);}if tx.send(pixels).is_err(){break;}}});
        let encoded=encode(layout,&path,rx,back,&AtomicBool::new(false));let _=producer.join();encoded?;
        let mut player=Player::new(&path,layout.world,true)?;
        let pump=|p:&mut Player,pause:bool,seconds:f64|->Result<(),String>{let until=Instant::now()+Duration::from_secs_f64(seconds);while Instant::now()<until{let mut m:MSG=std::mem::zeroed();while PeekMessageW(&mut m,std::ptr::null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&m);DispatchMessageW(&m);}p.tick(pause,true)?;std::thread::sleep(Duration::from_millis(10));}Ok(())};
        pump(&mut player,false,3.0)?;let advanced=player.ready()&&player.time()>0.3;
        pump(&mut player,true,0.2)?;let t=player.time();pump(&mut player,true,0.5)?;let paused=(player.time()-t).abs()<0.08;
        let duration=player.engine.GetDuration();player.engine.SetCurrentTime(duration-0.3).map_err(|e|e.to_string())?;pump(&mut player,false,1.2)?;let looped=player.time()<2.0;
        if !advanced||!paused||!looped||duration<10.0{return Err(format!("video advance={advanced}, pause={paused}, loop={looped}, duration={duration}"));}
        Ok(serde_json::json!({"pass":true,"advanced":advanced,"paused":paused,"looped":looped,"duration":duration,"renderer":"MediaEngine + D3D11 hardware device + DirectComposition"}))
    })();let _=std::fs::remove_file(path);result
}}

pub struct Cache {
    pub bake:Option<Bake>,pub player:Option<Player>,key:String,stable_at:f64,
    failed:bool,tried_disk:bool,slot:usize,dir:PathBuf,player_key:String,
}
impl Cache {
    pub fn new()->Self{Self{bake:None,player:None,key:String::new(),stable_at:0.0,failed:false,tried_disk:false,slot:0,dir:crate::lock::data_dir().join("video-cache"),player_key:String::new()}}
    pub fn reset(&mut self){
        self.player=None;if let Some(b)=self.bake.as_mut(){b.cancel();}
        self.key.clear();self.player_key.clear();self.tried_disk=true;self.failed=false;
    }
    pub fn active(&self)->bool{self.player.as_ref().is_some_and(Player::ready)}
    pub fn tick(&mut self,key:String,layout:Layout,enabled:bool,hidden:bool,any_visible:bool,interacting:bool,headless:bool,now:f64)->bool{
        if !enabled||interacting {
            self.reset();self.stable_at=now+2000.0;
        }else if key!=self.key {
            self.key=key;self.stable_at=now+2000.0;
            if let Some(b)=self.bake.as_mut(){if b.key!=self.key{b.cancel();}}
        }
        // A cancelled worker retains its filename until it actually exits.
        // Never let two encoder threads write the same pending cache slot.
        if let Some(result)=self.bake.as_ref().and_then(Bake::poll){
            let b=self.bake.take().unwrap();
            if enabled&&!interacting&&b.key==self.key {match result{
                Ok(path)=>{
                    // Drop the old decoder before replacing the manifest or
                    // reusing its file on the next refresh (Windows file locks).
                    self.player=None;
                    match Player::new(&path,layout.world,headless){
                        Ok(p)=>{self.player=Some(p);self.player_key=self.key.clone();let _=std::fs::write(self.dir.join("current.json"),serde_json::json!({"key":self.key,"slot":self.slot}).to_string());},
                        Err(e)=>{crate::log!("Video nền, trở về dựng cảnh: {e}");self.failed=true;}
                    }
                },Err(e)=>{crate::log!("{e}; tiếp tục dùng dựng cảnh");self.failed=true;}
            }}
        }
        if enabled&&!interacting&&!self.failed&&!self.tried_disk {
            self.tried_disk=true;
            if let Ok(text)=std::fs::read_to_string(self.dir.join("current.json")) {
                if let Ok(v)=serde_json::from_str::<serde_json::Value>(&text){
                    self.slot=v["slot"].as_u64().unwrap_or(0).min(1)as usize;
                    let path=self.dir.join(format!("scene-{}.mp4",self.slot));
                    if v["key"].as_str()==Some(&self.key)&&std::fs::metadata(&path).is_ok_and(|m|m.len()<=MAX_FILE){
                        match Player::new(&path,layout.world,headless){Ok(p)=>{self.player=Some(p);self.player_key=self.key.clone();},Err(e)=>{crate::log!("Cache video cũ: {e}");self.failed=true;}}
                    }
                }
            }
        }
        if let Some(p)=self.player.as_mut(){
            if let Err(e)=p.tick(hidden,headless){crate::log!("Video nền, khôi phục dựng cảnh: {e}");self.player=None;self.failed=true;}
        }
        // The brief bake renders all outputs into the atlas, even when an app
        // covers one monitor. It samples only our own FBOs. If no wallpaper is
        // visible the producer sleeps and the bounded encoder queue blocks.
        if enabled&&!hidden&&any_visible&&!interacting&&!self.failed&&now>=self.stable_at&&self.bake.is_none(){
            let cached=self.player.is_some()&&self.player_key==self.key;
            if !cached {
                if let Err(e)=std::fs::create_dir_all(&self.dir){crate::log!("Thư mục video nền: {e}");self.failed=true;}else{
                    self.slot=1-self.slot;
                    let path=self.dir.join(format!("scene-{}.mp4",self.slot));
                    self.bake=Some(Bake::start(self.key.clone(),layout,path,now));
                    crate::log!("Tạo cache video nền {}x{} @ {FPS} FPS, giới hạn hai tệp / 64 MB",layout.width,layout.height);
                }
            }
        }
        // Rendering is needed for a replacement clip even while the old video
        // continues to play above it. Otherwise the entire GL/GDI path sleeps.
        self.active()&&self.bake.is_none()
    }
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn atlas_retains_negative_coordinates_orientation_and_gaps(){
        let a=RECT{left:0,top:0,right:6,bottom:4};let b=RECT{left:-4,top:-2,right:0,bottom:4};
        let l=Layout{world:RECT{left:-4,top:-2,right:6,bottom:4},width:10,height:6,scale:1.0};let mut atlas=vec![0;l.bytes()];
        let mut px=vec![0;6*4*4];for y in 0..4{for x in 0..6{px[(y*6+x)*4]=(y+1)as u8;}}
        l.copy_output(&mut atlas,a,6,4,&px);l.copy_output(&mut atlas,b,4,6,&vec![9;4*6*4]);
        assert_eq!(atlas[(2*10+4)*4],4);assert_eq!(atlas[(5*10+4)*4],1);assert_eq!(atlas[0],9);assert_eq!(atlas[4*4],0);
        let bounded=Layout::new([RECT{left:-9000,top:0,right:9000,bottom:9000}].into_iter()).unwrap();assert!(bounded.width<=3840&&bounded.height<=2160&&bounded.width%2==0&&bounded.height%2==0);
    }
    #[test]fn loop_overlap_finishes_at_the_opening_frame(){let mut f=vec![220,16,128];blend(&mut f,&[20,235,128],5,5);assert_eq!(f,[20,235,128]);}
    #[test]
    #[ignore="Requires a hardware video device and an interactive Windows session"]
    fn hardware_video_decodes_loops_and_pauses(){unsafe{
        let path=std::env::temp_dir().join(format!("tuanpc-desk-video-test-{}.mp4",std::process::id()));
        let layout=Layout{world:RECT{left:0,top:0,right:64,bottom:32},width:64,height:32,scale:1.0};
        let(tx,rx)=mpsc::sync_channel(1);let(back,_)=mpsc::sync_channel(1);
        let producer=std::thread::spawn(move||{for i in 0..FPS*SECONDS{let mut pixels=vec![0;layout.bytes()];for p in pixels.chunks_exact_mut(4){p.copy_from_slice(&[i as u8,80,200,255]);}if tx.send(pixels).is_err(){break;}}});
        encode(layout,&path,rx,back,&AtomicBool::new(false)).unwrap();producer.join().unwrap();
        let mut player=Player::new(&path,layout.world,true).unwrap();
        let pump=|p:&mut Player,pause:bool,seconds:f64|{let until=Instant::now()+Duration::from_secs_f64(seconds);while Instant::now()<until{let mut m:MSG=std::mem::zeroed();while PeekMessageW(&mut m,std::ptr::null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&m);DispatchMessageW(&m);}p.tick(pause,true).unwrap();std::thread::sleep(Duration::from_millis(10));}};
        pump(&mut player,false,3.0);assert!(player.ready());let time=player.time();assert!(time>0.3,"video did not advance: {time}");
        pump(&mut player,true,0.2);let t=player.time();pump(&mut player,true,0.5);assert!((player.time()-t).abs()<0.08);
        let duration=player.engine.GetDuration();assert!(duration>10.0&&duration<=12.0);
        player.engine.SetCurrentTime(duration-0.3).unwrap();pump(&mut player,false,1.2);assert!(player.time()<2.0,"video did not loop: {}",player.time());
        drop(player);std::fs::remove_file(path).unwrap();
    }}
}
