//! Dock window transitions. The existing Stage WGC worker supplies a bounded
//! frame. Scale and FluidWin Genie share one Dock-thread D3D device.
use super::{fluidwin_mesh,motion,stage_snapshot,DockSide,WindowIdentity,STATE,CONTROL,queue_refresh};
use crate::desktop::wide;
use std::{cell::RefCell,collections::HashMap,mem,ptr,rc::Rc,sync::{Arc,Mutex},time::{Duration,Instant}};
use serde_json::{json,Value};
use windows::{core::Interface,Win32::{Foundation::{HWND as WinHwnd,POINT as WinPoint},
    Graphics::{Direct3D11::*,DirectComposition::*,Dxgi::{IDXGISurface,Common::*}}}};
use windows_sys::Win32::{Foundation::*,Graphics::{Dwm::*,Gdi::*},
    System::LibraryLoader::GetModuleHandleW,UI::WindowsAndMessaging::*};

pub(super) const TIMER:usize=17;
pub(super) const RELEASE_TIMER:usize=18;
const PREPARE:Duration=Duration::from_millis(550);
const SETTLE:Duration=Duration::from_millis(350);
const MAX_PREVIEW_AGE:Duration=Duration::from_secs(30*60);
const MAX_TEXTURE_SIDE:i32=stage_snapshot::MOTION_EDGE as i32;
const FRAME_INTERVAL_MS:u32=10;
const PRESENT_TIMEOUT:Duration=Duration::from_millis(150);
type Reason=&'static str;
#[derive(Clone,Copy,PartialEq,Eq)]
enum Action{Minimize,Restore}
impl Action{fn name(self)->&'static str{match self{Self::Minimize=>"minimize",Self::Restore=>"restore"}}}
#[derive(Clone)]
struct Request{
    source:WindowIdentity,area:RECT,normal:RECT,dpi:u32,monitor:usize,title:String,
    action:Action,icon:RECT,side:DockSide,effect:motion::WindowEffect,dock:usize,
}
struct Recent{request:Request,revision:u64,stored:Instant}
thread_local!{
    static ACTIVE:RefCell<Option<Transition>>=const{RefCell::new(None)};
    static RECENT:RefCell<HashMap<WindowIdentity,Recent>>=RefCell::new(HashMap::new());
    static FACTORY:RefCell<Option<ProxyFactory>>=const{RefCell::new(None)};
}
static LAST:Mutex<Option<Value>>=Mutex::new(None);

#[derive(Clone,Copy)]
struct Policy{motion:motion::Settings,client_animation:Option<bool>,native_minimize:Option<i32>}
impl Policy{
    unsafe fn read()->Self{
        let mut client=0i32;
        let client_animation=(SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION,0,(&mut client as *mut i32).cast(),0)!=0).then_some(client!=0);
        let mut info:ANIMATIONINFO=mem::zeroed();info.cbSize=mem::size_of::<ANIMATIONINFO>() as u32;
        let native_minimize=(SystemParametersInfoW(SPI_GETANIMATION,info.cbSize,(&mut info as *mut ANIMATIONINFO).cast(),0)!=0).then_some(info.iMinAnimate);
        Self{motion:motion::settings(),client_animation,native_minimize}
    }
    fn gate(self)->Result<(),Reason>{
        if self.motion.window_effect==motion::WindowEffect::Windows{return Err("scale-not-selected");}
        if !self.motion.enabled{return Err("motion-disabled");}
        if self.motion.respect_system&&self.client_animation!=Some(true){return Err("system-reduced-motion");}
        if self.native_minimize!=Some(0){return Err("native-animation-overlap");}
        Ok(())
    }
    fn json(self)->Value{json!({
        "requestedEffect":self.motion.window_effect.name(),
        "animationsEnabled":self.motion.enabled,"animationSpeed":self.motion.speed as f64/100.0,
        "respectSystemReducedMotion":self.motion.respect_system,
        "SPI_GETCLIENTAREAANIMATION":self.client_animation,
        "SPI_GETANIMATION":self.native_minimize.is_some(),"iMinAnimate":self.native_minimize,
        "availability":self.gate().err().unwrap_or("available")
    })}
}
pub(super) fn diagnostics()->Value{
    let policy=unsafe{Policy::read()}.json();
    let last=if crate::trial::active(){LAST.lock().ok().and_then(|g|g.clone())}else{None};
    json!({"policy":policy,"last":last})
}
struct Attempt{
    values:Value,source:WindowIdentity,started:Instant,animation:Option<Instant>,
    timing:Option<stage_snapshot::CaptureTimingHandle>,frames:u32,done:bool,
    last_submit:Option<Instant>,longest_gap_us:u64,
}
impl Attempt{
    unsafe fn new(source:WindowIdentity,action:Action,policy:Policy)->Self{
        let mut values=policy.json();
        values["action"]=json!(action.name());
        values["actualEffect"]=json!("windows");
        values["identityValid"]=json!(source.matches(source.hwnd as HWND));
        values["windowWidth"]=Value::Null;values["windowHeight"]=Value::Null;values["windowPixels"]=Value::Null;
        values["captureWidth"]=Value::Null;values["captureHeight"]=Value::Null;
        values["captureDurationMs"]=Value::Null;values["captureValid"]=Value::Null;
        values["dockTarget"]=json!(false);values["proxyCreated"]=json!(false);values["proxyPresented"]=json!(false);
        values["reversals"]=json!(0);
        Self{values,source,started:Instant::now(),animation:None,timing:None,frames:0,done:false,last_submit:None,longest_gap_us:0}
    }
    fn area(&mut self,r:RECT){
        let (w,h)=dims(r);self.values["windowWidth"]=json!(w);self.values["windowHeight"]=json!(h);
        self.values["windowPixels"]=json!(i64::from(w)*i64::from(h));
    }
    fn capture(&mut self,frame:&stage_snapshot::Frame,area:RECT)->Result<(),Reason>{
        self.values["captureWidth"]=json!(frame.1);self.values["captureHeight"]=json!(frame.2);
        let t=Instant::now();let result=validate_pixels(frame,area);
        self.values["validationMs"]=json!(t.elapsed().as_millis() as u64);
        self.values["captureValid"]=json!(result.is_ok());result
    }
    unsafe fn proxy(&mut self,frame:&stage_snapshot::MotionFrame,req:&Request)->Result<Proxy,Reason>{
        match frame{
            stage_snapshot::MotionFrame::Cpu(frame)=>self.capture(frame,req.area)?,
            stage_snapshot::MotionFrame::Gpu(frame)=>{
                self.values["captureWidth"]=json!(frame.width);self.values["captureHeight"]=json!(frame.height);
                self.values["gpuToGpu"]=json!(true);
                self.values["captureValid"]=json!(frame.width==req.area.right-req.area.left&&frame.height==req.area.bottom-req.area.top);
                if self.values["captureValid"]!=true{return Err("capture-aspect-mismatch");}
            }
        }
        let t=Instant::now();let proxy=Proxy::new_motion_for(frame,req.area,req.icon,req.side,req.effect,req.dpi);
        self.values["proxyPreparationMs"]=json!(t.elapsed().as_millis() as u64);
        self.values["proxyCreated"]=json!(proxy.is_some());proxy.ok_or("proxy-create-failed")
    }
    unsafe fn present(&mut self,p:&Proxy,r:RECT,progress:f64,opacity:u8)->Result<(),Reason>{
        match p.paint(r,progress,opacity){
            1=>{
                let now=Instant::now();
                if let Some(prev)=self.last_submit.replace(now){
                    self.longest_gap_us=self.longest_gap_us.max(now.duration_since(prev).as_micros() as u64);
                }
                self.values["proxyPresented"]=json!(true);self.frames+=1;Ok(())
            }
            2=>{
                self.values["compositorBusyFrames"]=json!(self.values["compositorBusyFrames"].as_u64().unwrap_or(0)+1);
                if self.last_submit.is_some_and(|last|last.elapsed()>PRESENT_TIMEOUT){return Err("proxy-present-timeout");}
                Ok(()) // keep the last accepted frame
            }
            status=>{
                self.values["proxyPresentStatus"]=json!(format!("{status:#010x}"));
                Err("proxy-present-failed")
            }
        }
    }
    fn finish(&mut self,reason:Option<Reason>){
        if self.done{return;}self.done=true;
        if reason.is_none(){self.values["actualEffect"]=self.values["requestedEffect"].clone();}
        self.values["fallbackReason"]=json!(reason);
        self.values["presentedFrames"]=json!(self.frames);
        self.values["longestSubmitGapUs"]=json!(self.longest_gap_us);
        self.values["transitionDurationMs"]=json!(self.animation.map(|t|t.elapsed().as_millis() as u64));
        self.values["totalDurationMs"]=json!(self.started.elapsed().as_millis() as u64);
        if let Some(t)=&self.timing{if let Ok(t)=t.lock(){self.values["captureTiming"]=json!(*t);}}
        let hwnd=self.source.hwnd as HWND;
        let observed=unsafe{if !self.source.matches(hwnd){"invalid"}else if IsIconic(hwnd)!=0{"minimized"}else{"open"}};
        self.values["observedWindowState"]=json!(observed);
        let expected=if self.values["action"]=="minimize"{"minimized"}else{"open"};
        self.values["finalWindowState"]=json!(if reason.is_some()&&observed!=expected&&observed!="invalid"{"pending-native"}else{observed});
        if crate::trial::active(){crate::log!("DOCK_WINDOW_SCALE {}",self.values);}
        if let Ok(mut last)=LAST.lock(){*last=Some(self.values.clone());}
        #[cfg(test)]
        println!("WINDOW_SCALE_REQUEST {}",self.values);
    }
}
impl Drop for Attempt{fn drop(&mut self){self.finish(Some("transition-interrupted"));}}

fn dims(r:RECT)->(i32,i32){(r.right-r.left,r.bottom-r.top)}
// A geometry guard, not a texture/canvas allocation limit. The capture
// worker has its own independently bounded frame-pool and thumbnail budgets.
fn valid_area(r:RECT)->bool{let (w,h)=dims(r);w>0&&h>0&&w<=16384&&h<=16384}
fn icon_fit(area:RECT,icon:RECT)->RECT{
    let (w,h)=dims(area);let (iw,ih)=dims(icon);
    let s=(iw as f32/w as f32).min(ih as f32/h as f32);
    let (w,h)=((w as f32*s).round().max(1.0) as i32,(h as f32*s).round().max(1.0) as i32);
    let (x,y)=((icon.left+icon.right-w)/2,(icon.top+icon.bottom-h)/2);
    RECT{left:x,top:y,right:x+w,bottom:y+h}
}
fn interpolate(a:RECT,b:RECT,k:f32)->RECT{
    let mix=|x:i32,y:i32|(x as f32+(y as f32-x as f32)*k).round() as i32;
    RECT{left:mix(a.left,b.left),top:mix(a.top,b.top),right:mix(a.right,b.right),bottom:mix(a.bottom,b.bottom)}
}
fn validate_pixels(frame:&stage_snapshot::Frame,area:RECT)->Result<(),Reason>{
    let (data,w,h)=frame;
    let (aw,ah)=dims(area);
    if *w<=0||*h<=0||*w>MAX_TEXTURE_SIDE||*h>MAX_TEXTURE_SIDE||data.len()!=*w as usize* *h as usize*4||aw<=0||ah<=0{return Err("invalid-capture-frame");}
    if ((*w as f64/ *h as f64)/(aw as f64/ah as f64)-1.0).abs()>0.08{return Err("capture-aspect-mismatch");}
    let mut lo=255u8;let mut hi=0u8;
    for p in data.chunks_exact(4).step_by(7){
        let value=((u16::from(p[0])+u16::from(p[1])+u16::from(p[2]))/3) as u8;
        lo=lo.min(value);hi=hi.max(value);
    }
    if hi<24||hi.saturating_sub(lo)<12{Err("invalid-capture-frame")}else{Ok(())}
}
fn valid_pixels(frame:&stage_snapshot::Frame,area:RECT)->bool{validate_pixels(frame,area).is_ok()}
unsafe fn placement(hwnd:HWND)->Option<WINDOWPLACEMENT>{
    let mut p:WINDOWPLACEMENT=mem::zeroed();p.length=mem::size_of::<WINDOWPLACEMENT>() as u32;
    (GetWindowPlacement(hwnd,&mut p)!=0).then_some(p)
}
unsafe fn title(hwnd:HWND)->String{
    let mut buf=[0u16;256];let n=GetWindowTextW(hwnd,buf.as_mut_ptr(),buf.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&buf[..n])
}
unsafe fn visible_area(hwnd:HWND)->Option<RECT>{
    let mut r:RECT=mem::zeroed();
    (DwmGetWindowAttribute(hwnd,DWMWA_EXTENDED_FRAME_BOUNDS as u32,(&mut r as *mut RECT).cast(),mem::size_of::<RECT>() as u32)>=0).then_some(r)
}
unsafe fn elevation(pid:u32)->Option<bool>{
    use windows_sys::Win32::{Security::{GetTokenInformation,TokenElevation,TOKEN_ELEVATION,TOKEN_QUERY},
        System::Threading::{OpenProcess,OpenProcessToken,PROCESS_QUERY_LIMITED_INFORMATION}};
    let process=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,0,pid);
    if process.is_null(){return None;}
    let mut token=ptr::null_mut();let open=OpenProcessToken(process,TOKEN_QUERY,&mut token);CloseHandle(process);
    if open==0{return None;}
    let mut data:TOKEN_ELEVATION=mem::zeroed();let mut len=0;
    let ok=GetTokenInformation(token,TokenElevation,(&mut data as *mut TOKEN_ELEVATION).cast(),mem::size_of::<TOKEN_ELEVATION>() as u32,&mut len);
    CloseHandle(token);(ok!=0).then_some(data.TokenIsElevated!=0)
}
unsafe fn without_native_overlap(_hwnd:HWND)->bool{Policy::read().native_minimize==Some(0)}
unsafe fn permitted(id:WindowIdentity)->Result<(),Reason>{
    let h=id.hwnd as HWND;
    if !id.matches(h){return Err("invalid-window-identity");}
    if IsWindowVisible(h)==0||GetAncestor(h,GA_ROOT)!=h||GetWindowLongPtrW(h,GWL_STYLE)&WS_MINIMIZEBOX as isize==0{return Err("source-unavailable");}
    use super::previews::PreviewStep;
    match super::previews::permitted_pid(h){
        Ok(pid) if pid==id.pid=>Ok(()),
        Err(PreviewStep::Protected|PreviewStep::AffinityQuery(_))=>Err("protected-window"),
        Err(PreviewStep::Gone|PreviewStep::ForeignIdentity)=>Err("invalid-window-identity"),
        _=>Err("source-unavailable"),
    }
}
unsafe fn target(owner:HWND,path:&str)->Result<(RECT,DockSide),Reason>{
    let (slot,layout,openness)=STATE.with(|s|{
        let s=s.borrow();
        let b=s.bars.iter().find(|b|b.hwnd==owner&&!b.hidden).ok_or("dock-not-ready")?;
        let layout=b.layout.as_ref().ok_or("dock-not-ready")?;
        let i=b.items.iter().position(|item|item.path.eq_ignore_ascii_case(path)).ok_or("dock-target-not-found")?;
        let slot=*layout.slots.get(i).ok_or("dock-target-not-found")?;
        Ok::<_,Reason>((slot,(layout.glass,layout.side),b.reveal.value))
    })?;
    // Snapshot the actual rendered transform. Do not require the magnifier
    // or reveal spring to have settled, or retarget to a different icon.
    if openness<0.18||IsWindowVisible(owner)==0{return Err("dock-not-ready");}
    let (g,side)=layout;let (along,depth)=super::geometry::Layout::collapse_scale(openness);
    use super::DockSide::*;
    let (cx,cy,sx,sy)=match side{
        Bottom=>(g.x+g.w/2.0,g.y+g.h,along,depth),
        Left=>(g.x,g.y+g.h/2.0,depth,along),
        Right=>(g.x+g.w,g.y+g.h/2.0,depth,along),
    };
    let size=slot.w.min(slot.h)*0.84;
    let center_x=cx+(slot.x+slot.w*0.5-cx)*sx;
    let center_y=cy+(slot.y+slot.h*0.5-cy)*sy;
    let size=(size*sx.min(sy)).round() as i32;
    if size<8{return Err("dock-not-ready");}
    let mut p=POINT{x:(center_x-size as f32*0.5).round() as i32,y:(center_y-size as f32*0.5).round() as i32};
    if ClientToScreen(owner,&mut p)==0{return Err("dock-not-ready");}
    Ok((RECT{left:p.x,top:p.y,right:p.x+size,bottom:p.y+size},side))
}
unsafe fn request(owner:HWND,path:&str,id:WindowIdentity,action:Action,policy:Policy,attempt:&mut Attempt)->Result<Request,Reason>{
    let h=id.hwnd as HWND;
    // Read geometry even when policy fails, for a useful diagnostic.
    if id.matches(h)&&IsIconic(h)==0{if let Some(r)=visible_area(h){attempt.area(r);}}
    policy.gate()?;permitted(id)?;
    if elevation(id.pid)!=Some(false){return Err("elevated-window");}
    let p=placement(h).ok_or("source-unavailable")?;
    let monitor=MonitorFromWindow(h,MONITOR_DEFAULTTONULL);
    if monitor.is_null(){return Err("monitor-changed");}
    let dpi=windows_sys::Win32::UI::HiDpi::GetDpiForWindow(h);
    let name=title(h);
    let area=match action{
        Action::Minimize=>visible_area(h).ok_or("source-unavailable")?,
        Action::Restore=>RECENT.with(|r|{
            let r=r.borrow();let recent=r.get(&id).ok_or("restore-preview-missing")?;
            if recent.stored.elapsed()>MAX_PREVIEW_AGE{return Err("restore-preview-expired");}
            let saved=&recent.request;
            if saved.monitor!=monitor as usize{return Err("monitor-changed");}
            if saved.dpi!=dpi{return Err("dpi-changed");}
            if saved.title!=name{return Err("restore-preview-replaced");}
            if !super::same_rect(saved.normal,p.rcNormalPosition){return Err("window-placement-changed");}
            Ok(saved.area)
        })?,
    };
    attempt.area(area);
    if !valid_area(area){return Err("window-area-limit");}
    let (icon,side)=target(owner,path)?;attempt.values["dockTarget"]=json!(true);
    Ok(Request{source:id,area,normal:p.rcNormalPosition,dpi,monitor:monitor as usize,title:name,action,icon,side,effect:policy.motion.window_effect,dock:owner as usize})
}

// One small uploaded composition texture, no physical-size CPU DIB and
// no per-frame readback/StretchBlt. Reuse the device across a burst; release
// it after the same idle period as the existing Stage capture worker.
struct ProxyFactory{comp:IDCompositionDevice,gpu:Arc<crate::shell_gpu::ShellGpu>,mesh:Option<Rc<fluidwin_mesh::Pipeline>>}
impl ProxyFactory{
    unsafe fn new()->windows::core::Result<Self>{
        let gpu=crate::shell_gpu::ShellGpu::acquire()?;
        let comp=gpu.composition()?;
        Ok(Self{comp,gpu,mesh:None})
    }
}
impl Drop for ProxyFactory{
    fn drop(&mut self){
        if self.mesh.is_some(){
            let batch=self.gpu.context();
            unsafe{batch.ClearState();batch.Flush();}
        }
    }
}
struct Proxy{
    hwnd:HWND,comp:IDCompositionDevice,target:IDCompositionTarget,visual:IDCompositionVisual,
    render:Render,gpu:Arc<crate::shell_gpu::ShellGpu>,
    #[cfg(test)] marker:[u8;3],
}
enum Render{
    Scale{transform:IDCompositionScaleTransform,effect:IDCompositionEffectGroup,_surface:IDCompositionSurface,w:i32,h:i32},
    Mesh(fluidwin_mesh::Layer),
}
unsafe extern "system" fn proxy_proc(h:HWND,m:u32,w:WPARAM,l:LPARAM)->LRESULT{
    match m{WM_NCHITTEST=>HTTRANSPARENT as isize,WM_MOUSEACTIVATE=>MA_NOACTIVATE as isize,_=>DefWindowProcW(h,m,w,l)}
}
impl Drop for Proxy{fn drop(&mut self){unsafe{
    let _=self.target.SetRoot(None::<&IDCompositionVisual>);
    let _=self.visual.SetContent(None::<&windows::core::IUnknown>);
    let _=self.comp.Commit();
    DestroyWindow(self.hwnd);
}}}
impl Proxy{
    #[cfg(test)]
    unsafe fn new(frame:&stage_snapshot::Frame,area:RECT)->Option<Self>{
        Self::new_for(frame,area,area,DockSide::Bottom,motion::WindowEffect::Scale,96)
    }
    unsafe fn new_for(frame:&stage_snapshot::Frame,area:RECT,icon:RECT,side:DockSide,effect:motion::WindowEffect,dpi:u32)->Option<Self>{
        Self::new_motion_for(&stage_snapshot::MotionFrame::Cpu(frame.clone()),area,icon,side,effect,dpi)
    }
    unsafe fn new_motion_for(frame:&stage_snapshot::MotionFrame,area:RECT,icon:RECT,side:DockSide,effect:motion::WindowEffect,dpi:u32)->Option<Self>{
        let (data,w,h)=match frame{
            stage_snapshot::MotionFrame::Cpu(cpu)=>{if !valid_pixels(cpu,area){return None;}(&cpu.0[..],&cpu.1,&cpu.2)},
            stage_snapshot::MotionFrame::Gpu(gpu)=>(&[][..],&gpu.width,&gpu.height),
        };
        let gpu_frame=matches!(frame,stage_snapshot::MotionFrame::Gpu(_));
        let (comp,gpu,mesh)=FACTORY.with(|f|->Option<_>{
            let mut f=f.borrow_mut();if f.is_none(){*f=ProxyFactory::new().ok();}
            let f=f.as_mut()?;
            if f.gpu.device.GetDeviceRemovedReason().is_err(){*f=ProxyFactory::new().ok()?;}
            if (effect.mesh()||gpu_frame)&&f.mesh.is_none(){f.mesh=fluidwin_mesh::Pipeline::new(&f.gpu.context());}
            Some((f.comp.clone(),f.gpu.clone(),f.mesh.clone()))
        })?;
        let mesh=if effect.mesh()||gpu_frame{
            let _batch=gpu.context();
            Some(match frame{
                stage_snapshot::MotionFrame::Cpu(frame)=>fluidwin_mesh::Layer::new(mesh?,frame,area,icon,side,effect==motion::WindowEffect::Drop,dpi)?,
                stage_snapshot::MotionFrame::Gpu(frame)=>fluidwin_mesh::Layer::from_gpu(mesh?,frame,area,icon,side,effect,dpi)?,
            })
        }else{None};
        let class=wide("TLauncherScaleProxy");
        let instance=GetModuleHandleW(ptr::null());
        let wc=WNDCLASSW{lpfnWndProc:Some(proxy_proc),hInstance:instance,lpszClassName:class.as_ptr(),..mem::zeroed()};
        if RegisterClassW(&wc)==0&&GetLastError()!=ERROR_CLASS_ALREADY_EXISTS{return None;}
        let hwnd=CreateWindowExW(WS_EX_NOREDIRECTIONBITMAP|WS_EX_TRANSPARENT|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_TOPMOST,
            class.as_ptr(),wide("TLauncher window transition").as_ptr(),WS_POPUP,area.left,area.top,1,1,
            ptr::null_mut(),ptr::null_mut(),instance,ptr::null_mut());
        if hwnd.is_null(){return None;}
        // Only the owned proxy is exempt from Windows's show/hide animation.
        let disable=1i32;
        let _=DwmSetWindowAttribute(hwnd,DWMWA_TRANSITIONS_FORCEDISABLED as u32,(&disable as *const i32).cast(),4);
        let result=(||->windows::core::Result<Self>{
            let target=comp.CreateTargetForHwnd(WinHwnd(hwnd),true)?;
            let visual=comp.CreateVisual()?;
            let render=if let Some(mesh)=mesh{
                mesh.attach(&visual)?;Render::Mesh(mesh)
            }else{
            let scale=comp.CreateScaleTransform()?;let effect=comp.CreateEffectGroup()?;
            let surface=comp.CreateSurface(*w as u32,*h as u32,DXGI_FORMAT_B8G8R8A8_UNORM,DXGI_ALPHA_MODE_IGNORE)?;
            let mut offset=WinPoint::default();
            let drawing:IDXGISurface=surface.BeginDraw(None,&mut offset)?;
            let upload=(||->windows::core::Result<()>{
                let texture:ID3D11Texture2D=drawing.cast()?;
                let box_=D3D11_BOX{left:offset.x as u32,top:offset.y as u32,front:0,
                    right:(offset.x+*w) as u32,bottom:(offset.y+*h) as u32,back:1};
                gpu.context().UpdateSubresource(&texture,0,Some(&box_),data.as_ptr().cast(),*w as u32*4,0);
                Ok(())
            })();
            let end=surface.EndDraw();upload?;end?;
            visual.SetContent(&surface)?;visual.SetTransform(&scale)?;visual.SetEffect(&effect)?;
            Render::Scale{transform:scale,effect,_surface:surface,w:*w,h:*h}
            };
            target.SetRoot(&visual)?;comp.Commit()?;
            Ok(Self{hwnd,comp,target,visual,render,gpu,
                #[cfg(test)] marker:if data.is_empty(){[0,0,0]}else{let i=((*h/2)* *w+*w/10) as usize*4;[data[i+2],data[i+1],data[i]]}})
        })();
        if result.is_err(){DestroyWindow(hwnd);}
        result.ok()
    }
    #[cfg(test)]
    unsafe fn present(&self,r:RECT,opacity:u8)->bool{self.paint(r,0.0,opacity)==1}
    unsafe fn paint(&self,mut r:RECT,progress:f64,opacity:u8)->i32{
        let first=IsWindowVisible(self.hwnd)==0;
        match &self.render{
            Render::Scale{transform,effect,w,h,..}=>{
                if !valid_area(r){return 0;}
                let (rw,rh)=dims(r);
                let updated=(||->windows::core::Result<()>{
                    transform.SetScaleX2(rw as f32/ *w as f32)?;
                    transform.SetScaleY2(rh as f32/ *h as f32)?;
                    effect.SetOpacity2(opacity as f32/255.0)?;
                    self.comp.Commit()
                })();
                if updated.is_err(){return 0;}
            }
            Render::Mesh(mesh)=>{
                let status={let _batch=self.gpu.context();mesh.paint(progress,opacity)};
                if status!=1{return status;}
                if !first{return 1;} // canvas stays fixed over the source and Dock
                r=mesh.canvas;
            }
        }
        if first{
            if let Err(error)=self.comp.WaitForCommitCompletion(){return error.code().0;}
        }
        let (w,h)=dims(r);
        let shown=SetWindowPos(self.hwnd,HWND_TOPMOST,r.left,r.top,w,h,SWP_NOACTIVATE|SWP_SHOWWINDOW)!=0;
        if shown&&first{let _=DwmFlush();}
        i32::from(shown)
    }
}
enum Phase{
    Capture{job:stage_snapshot::TransitionCapture,started:Instant},
    // Keep the real window in its current state until the proxy has a frame.
    Priming{proxy:Proxy,frame:Option<stage_snapshot::MotionFrame>,started:Instant},
    WaitingMinimize{proxy:Proxy,frame:stage_snapshot::MotionFrame,started:Instant},
    Animate{proxy:Proxy,span:motion::Span},
    WaitingRestore{_proxy:Proxy,started:Instant},
}
struct Transition{req:Request,phase:Option<Phase>,attempt:Attempt,last_progress:f64}
impl Transition{
    unsafe fn prime(&mut self,proxy:Proxy,frame:Option<stage_snapshot::MotionFrame>,started:Instant)->Result<Phase,Reason>{
        let h=self.req.source.hwnd as HWND;
        let restoring=self.req.action==Action::Restore;
        if restoring!=(IsIconic(h)!=0)||(!restoring&&visible_area(h).is_none_or(|r|!super::same_rect(r,self.req.area))){
            return Err("window-placement-changed");
        }
        let (r,progress,opacity)=if restoring{
            (icon_fit(self.req.area,self.req.icon),1.0,if self.req.effect.mesh(){255}else{64})
        }else{(self.req.area,0.0,255)};
        let submitted=self.attempt.frames;
        self.attempt.present(&proxy,r,progress,opacity)?;
        if self.attempt.frames==submitted{
            if started.elapsed()>=PRESENT_TIMEOUT{return Err("proxy-present-timeout");}
            return Ok(Phase::Priming{proxy,frame,started});
        }
        if let Some(frame)=frame{
            if ShowWindowAsync(h,if super::taskbar_hidden(){SW_SHOWMINNOACTIVE}else{SW_MINIMIZE})==0{
                return Err("state-transition-timeout");
            }
            Ok(Phase::WaitingMinimize{proxy,frame,started:Instant::now()})
        }else{
            self.attempt.animation=Some(Instant::now());
            Ok(Phase::Animate{proxy,span:motion::Span::new(if self.req.effect.mesh(){450}else{340})})
        }
    }
    unsafe fn fallback(&mut self,reason:Reason)->bool{
        self.phase.take();
        let id=self.req.source;let h=id.hwnd as HWND;
        if id.matches(h){
            match self.req.action{
                Action::Minimize if IsIconic(h)==0=>{ShowWindowAsync(h,if super::taskbar_hidden(){SW_SHOWMINNOACTIVE}else{SW_MINIMIZE});}
                Action::Restore=>{if IsIconic(h)!=0{ShowWindowAsync(h,SW_RESTORE);}SetForegroundWindow(h);}
                _=>{}
            }
            queue_refresh();
        }
        self.attempt.finish(Some(reason));false
    }
    unsafe fn stable(&self)->Result<(),Reason>{
        let h=self.req.source.hwnd as HWND;
        Policy::read().gate()?;permitted(self.req.source)?;
        if windows_sys::Win32::UI::HiDpi::GetDpiForWindow(h)!=self.req.dpi{return Err("dpi-changed");}
        if MonitorFromWindow(h,MONITOR_DEFAULTTONULL) as usize!=self.req.monitor{return Err("monitor-changed");}
        if !placement(h).is_some_and(|p|super::same_rect(p.rcNormalPosition,self.req.normal)){return Err("window-placement-changed");}
        Ok(())
    }
    unsafe fn tick(&mut self)->bool{
        let h=self.req.source.hwnd as HWND;
        if let Err(reason)=self.stable(){return self.fallback(reason);}
        let icon=icon_fit(self.req.area,self.req.icon);
        let action=self.req.action;
        let phase=self.phase.take().expect("active phase");
        self.phase=Some(match phase{
            Phase::Capture{job,started}=>{
                if IsIconic(h)!=0||visible_area(h).is_none_or(|r|!super::same_rect(r,self.req.area)){
                    return self.fallback("window-placement-changed");
                }
                let result=job.receiver.try_recv();
                match result{
                    Ok(Ok(frame))=>{
                        self.attempt.values["captureDurationMs"]=json!(started.elapsed().as_millis() as u64);
                        let proxy=match self.attempt.proxy(&frame,&self.req){Ok(p)=>p,Err(r)=>return self.fallback(r)};
                        match self.prime(proxy,Some(frame),Instant::now()){Ok(p)=>p,Err(r)=>return self.fallback(r)}
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) if started.elapsed()<PREPARE=>Phase::Capture{job,started},
                    other=>{
                        self.attempt.values["captureDurationMs"]=json!(started.elapsed().as_millis() as u64);
                        self.attempt.values["captureValid"]=json!(false);
                        let reason=match other{
                            Ok(Err(ref e)) if e.contains("budget")||e.contains("capture limit")=>"window-area-limit",
                            Ok(Err(ref e)) if e.contains("blank")||e.contains("mapping")||e.contains("format")=>"invalid-capture-frame",
                            Ok(Err(ref e)) if e.contains("deadline")=>"capture-timeout",
                            Ok(Err(_))=>"source-unavailable",
                            Err(std::sync::mpsc::TryRecvError::Disconnected)=>"source-unavailable",
                            _=>"capture-timeout",
                        };
                        drop(job);return self.fallback(reason);
                    }
                }
            }
            Phase::Priming{proxy,frame,started}=>{
                match self.prime(proxy,frame,started){Ok(p)=>p,Err(r)=>return self.fallback(r)}
            }
            Phase::WaitingMinimize{proxy,frame,started}=>{
                if IsIconic(h)!=0{
                    let revision=stage_snapshot::remember_motion(self.req.source,frame);
                    if let Some(revision)=revision{RECENT.with(|r|{
                        let mut r=r.borrow_mut();
                        r.retain(|_,v|v.stored.elapsed()<=MAX_PREVIEW_AGE);
                        if r.len()>=80{if let Some(old)=r.iter().min_by_key(|(_,v)|v.stored).map(|(id,_)|*id){r.remove(&old);}}
                        r.insert(self.req.source,Recent{request:self.req.clone(),revision,stored:Instant::now()});
                    });}
                    self.attempt.animation=Some(Instant::now());queue_refresh();
                    Phase::Animate{proxy,span:motion::Span::new(if self.req.effect.mesh(){500}else{300})}
                }else if started.elapsed()>SETTLE{drop(proxy);return self.fallback("state-transition-timeout");}
                else{Phase::WaitingMinimize{proxy,frame,started}}
            }
            Phase::Animate{proxy,span}=>{
                if IsIconic(h)==0{self.attempt.finish(Some("transition-interrupted"));return false;}
                let t=span.progress();let k=motion::ease(t);
                let (a,b,opacity)=match action{
                    Action::Minimize=>(self.req.area,icon,(255.0*(1.0-((t-0.82)/0.18).clamp(0.0,1.0))) as u8),
                    Action::Restore=>(icon,self.req.area,(255.0*(0.25+0.75*(t*2.0).min(1.0))) as u8),
                };
                let progress=if self.req.effect.mesh(){
                    fluidwin_mesh::progress(t,action==Action::Restore,self.req.effect==motion::WindowEffect::Drop)
                }else{f64::from(if action==Action::Restore{1.0-k}else{k})};
                let opacity=if self.req.effect.mesh(){255}else{opacity};
                let submitted=self.attempt.frames;
                if let Err(reason)=self.attempt.present(&proxy,interpolate(a,b,k),progress,opacity){drop(proxy);return self.fallback(reason);}
                if self.attempt.frames!=submitted{self.last_progress=progress;}
                // Do not restore/minimize over an old pose if DXGI declined
                // the terminal frame. Retry within the bounded present timeout.
                if t>=1.0&&self.attempt.frames!=submitted{
                    if action==Action::Minimize{self.attempt.finish(None);return false;}
                    if ShowWindowAsync(h,SW_RESTORE)==0{drop(proxy);return self.fallback("state-transition-timeout");}
                    Phase::WaitingRestore{_proxy:proxy,started:Instant::now()}
                }else{Phase::Animate{proxy,span}}
            }
            Phase::WaitingRestore{_proxy,started}=>{
                if IsIconic(h)==0{SetForegroundWindow(h);queue_refresh();self.attempt.finish(None);return false;}
                if started.elapsed()>SETTLE{drop(_proxy);return self.fallback("state-transition-timeout");}
                Phase::WaitingRestore{_proxy,started}
            }
        });
        true
    }
}
unsafe fn begin(owner:HWND,path:&str,id:WindowIdentity,action:Action)->bool{
    if reverse(owner,id,action){return true;}
    cancel();
    let policy=Policy::read();let mut attempt=Attempt::new(id,action,policy);
    let result=(||->Result<(Request,Phase),Reason>{
        let req=request(owner,path,id,action,policy,&mut attempt)?;
        let controller=CONTROL.with(|c|c.get());
        if controller.is_null(){return Err("dock-not-ready");}
        let phase=match action{
            Action::Minimize=>{
                RECENT.with(|r|{r.borrow_mut().remove(&id);});
                let job=stage_snapshot::transition_capture(id,controller)?;
                attempt.timing=Some(job.timing.clone());attempt.values["cachedCapture"]=json!(job.cached);
                Phase::Capture{job,started:Instant::now()}
            }
            Action::Restore=>{
                let revision=RECENT.with(|r|r.borrow().get(&id).map(|r|r.revision)).ok_or("restore-preview-missing")?;
                let started=Instant::now();
                let frame=stage_snapshot::recent_motion(id,revision)?;
                attempt.values["cachedCapture"]=json!(true);
                attempt.values["captureDurationMs"]=json!(started.elapsed().as_millis() as u64);
                let proxy=attempt.proxy(&frame,&req)?;
                Phase::Priming{proxy,frame:None,started:Instant::now()}
            }
        };
        if SetTimer(controller,TIMER,FRAME_INTERVAL_MS,None)==0{return Err("dock-not-ready");}
        KillTimer(controller,RELEASE_TIMER);
        // Transfer the diagnostic into the active transition below.
        Ok((req,phase))
    })();
    match result{
        Ok((req,phase))=>{
            let last_progress=if req.action==Action::Restore{1.0}else{0.0};
            let restoring=req.action==Action::Restore;
            ACTIVE.with(|a|{a.replace(Some(Transition{req,phase:Some(phase),attempt,last_progress}));});
            if restoring{timer();}
            true
        }
        Err(reason)=>{attempt.finish(Some(reason));schedule_release();false}
    }
}
unsafe fn reverse(owner:HWND,id:WindowIdentity,action:Action)->bool{
    ACTIVE.with(|a|{
        let mut a=a.borrow_mut();
        let Some(t)=a.as_mut().filter(|t|t.req.source==id&&t.req.dock==owner as usize&&t.req.effect.mesh())else{return false;};
        if !id.matches(id.hwnd as HWND)||IsIconic(id.hwnd as HWND)==0{return false;}
        let Some(Phase::Animate{span,..})=t.phase.as_mut()else{return false;};
        let policy=Policy::read();
        if policy.gate().is_err()||policy.motion.window_effect!=t.req.effect{return false;}
        if t.req.action==action{return true;}
        let restoring=action==Action::Restore;
        let elapsed=fluidwin_mesh::invert(t.last_progress,restoring,t.req.effect==motion::WindowEffect::Drop);
        *span=motion::Span::resume(if restoring{450}else{500},elapsed);
        t.req.action=action;
        t.attempt.values["action"]=json!(action.name());
        t.attempt.values["reversals"]=json!(t.attempt.values["reversals"].as_u64().unwrap_or(0)+1);
        true
    })
}
pub(super) unsafe fn minimize(owner:HWND,path:&str,id:WindowIdentity)->bool{begin(owner,path,id,Action::Minimize)}
pub(super) unsafe fn restore(owner:HWND,path:&str,id:WindowIdentity)->bool{begin(owner,path,id,Action::Restore)}
unsafe fn schedule_release(){
    let controller=CONTROL.with(|c|c.get());
    KillTimer(controller,TIMER);
    if FACTORY.with(|f|f.borrow().is_some())&&SetTimer(controller,RELEASE_TIMER,3000,None)==0{release_factory();}
}
pub(super) unsafe fn release_factory(){
    KillTimer(CONTROL.with(|c|c.get()),RELEASE_TIMER);
    if ACTIVE.with(|a|a.borrow().is_none()){FACTORY.with(|f|{f.borrow_mut().take();});}
}
pub(super) unsafe fn timer(){
    let active=ACTIVE.with(|a|a.borrow_mut().take());
    let next=active.and_then(|mut t|if t.tick(){Some(t)}else{None});
    if next.is_none(){schedule_release();}
    ACTIVE.with(|a|{a.replace(next);});
}
pub(super) unsafe fn capture_ready(){
    if ACTIVE.with(|a|a.borrow().as_ref().is_some_and(|t|matches!(t.phase.as_ref(),Some(Phase::Capture{..})))){timer();}
}
pub(super) fn restoring(id:WindowIdentity)->bool{
    ACTIVE.with(|a|a.borrow().as_ref().is_some_and(|t|t.req.source==id&&t.req.action==Action::Restore))
}
pub(super) unsafe fn cancel(){
    ACTIVE.with(|a|{a.borrow_mut().take();});
    schedule_release();
}
pub(super) fn invalidate_recent(hwnd:HWND){
    RECENT.with(|r|r.borrow_mut().retain(|id,_|id.hwnd!=hwnd as usize));
}
pub(super) unsafe fn reset(){
    cancel();release_factory();RECENT.with(|r|r.borrow_mut().clear());
}

#[cfg(test)]
pub(super) unsafe fn probe_owned_window(source:HWND,frame:&stage_snapshot::Frame){
    use windows_sys::Win32::System::Threading::*;
    use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
    struct Apartment(bool);
    impl Drop for Apartment{fn drop(&mut self){if self.0{unsafe{CoUninitialize();}}}}
    let _com=Apartment(CoInitializeEx(None,COINIT_APARTMENTTHREADED).is_ok());
    let area=visible_area(source).unwrap();
    if !valid_pixels(frame,area){println!("WINDOW_SCALE_NATIVE=BLOCKED capture aspect or pixel validation");return;}
    let icon=RECT{left:area.right-48,top:area.bottom-48,right:area.right, bottom:area.bottom};
    let end=icon_fit(area,icon);
    let counts=||{let h=GetCurrentProcess();let mut n=0;GetProcessHandleCount(h,&mut n);
        (GetGuiResources(h,0),GetGuiResources(h,1),n)};
    // Native policy is read-only even in the hosted fixture. The owned
    // transient proxy has its own DWM policy; the source is not modified.
    println!("WINDOW_SCALE_NATIVE_GATE source_policy_already_disabled={}",without_native_overlap(source));

    let cold=crate::private_mb();
    let cold_handles=counts();
    let mut before=cold;
    let mut handles=cold_handles;
    let original=placement(source).unwrap();
    let mut visible_pixels=false;
    let mut peak=before;
    let mut longest=0.0f64;
    let settle=||{
        GdiFlush();let _=DwmFlush();
        let mut msg:MSG=mem::zeroed();
        while PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}
    };
    // Warm the system-owned layered HWND and native show-state machinery,
    // then measure 100 additional cycles from a settled baseline.
    for i in 0..105{
        let proxy=Proxy::new(frame,area).expect("valid owned WGC proxy");
        let started=Instant::now();
        assert!(proxy.present(area,255));
        ShowWindow(source,SW_SHOWMINNOACTIVE);
        assert_ne!(IsIconic(source),0);
        if i==0{
            DwmFlush();
            let dc=GetDC(ptr::null_mut());
            if !dc.is_null(){
                let (w,h)=dims(area);
                let color=GetPixel(dc,area.left+w/10,area.top+h/2);
                let x=frame.1/10;let y=frame.2/2;
                let p=&frame.0[((y*frame.1+x)*4) as usize..][..3];
                if color!=u32::MAX{
                    visible_pixels=[color as u8,(color>>8) as u8,(color>>16) as u8].into_iter()
                        .zip([p[2],p[1],p[0]]).all(|(a,b)|a.abs_diff(b)<28);
                }
                ReleaseDC(ptr::null_mut(),dc);
            }
        }
        for k in [0.25,0.5,0.75,1.0]{assert!(proxy.present(interpolate(area,end,k),255));}
        for k in [0.25,0.5,0.75,1.0]{assert!(proxy.present(interpolate(end,area,k),255));}
        ShowWindow(source,SW_RESTORE);
        assert_eq!(IsIconic(source),0);
        assert!(super::same_rect(placement(source).unwrap().rcNormalPosition,original.rcNormalPosition));
        let now=crate::private_mb();peak=(peak.0.max(now.0),peak.1.max(now.1));
        longest=longest.max(started.elapsed().as_secs_f64()*1000.0);
        let h=proxy.hwnd;drop(proxy);assert_eq!(IsWindow(h),0);
        settle();
        if i==4{
            for _ in 0..6{settle();std::thread::sleep(Duration::from_millis(40));}
            before=crate::private_mb();handles=counts();peak=before;longest=0.0;
            println!("WINDOW_SCALE_NATIVE_WARM handles={cold_handles:?}->{handles:?} memory={cold:?}->{before:?}");
        }
        if i>=24&&(i-4)%20==0{println!("WINDOW_SCALE_NATIVE_PROGRESS cycles={} handles={:?} memory={:?}",i-4,counts(),crate::private_mb());}
    }
    for _ in 0..12{settle();std::thread::sleep(Duration::from_millis(40));}
    FACTORY.with(|f|{f.borrow_mut().take();});
    let after=crate::private_mb();
    let final_handles=counts();
    println!("WINDOW_SCALE_NATIVE cycles=100 rendered_source_pixels={} actual_screen_pixels={} placement=PASS proxy_cleanup=PASS handles={handles:?}->{final_handles:?} memory={before:?}->{after:?} peak={peak:?} max_submit_cycle_ms={longest:.2} native_gate={} overlap=NOT_VERIFIED",
        if visible_pixels{"PASS"}else{"NOT_VERIFIED"},if visible_pixels{"MATCH"}else{"NOT_VERIFIED"},without_native_overlap(source));
    assert!(final_handles.0<=handles.0+2&&final_handles.1<=handles.1+2&&final_handles.2<=handles.2+10,"steady-state proxy resources grew: {handles:?} -> {final_handles:?}");
    assert!(after.0-before.0<12.0,"private bytes did not recover after 100 bounded window proxies");
}

#[cfg(all(test,target_env="msvc"))]
pub(super) unsafe fn probe_mesh_pixels(source:HWND,frame:&stage_snapshot::Frame){
    use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
    struct Apartment(bool);
    impl Drop for Apartment{fn drop(&mut self){if self.0{unsafe{CoUninitialize();}}}}
    let _com=Apartment(CoInitializeEx(None,COINIT_APARTMENTTHREADED).is_ok());
    let mut value=-1i32;
    let hr=DwmGetWindowAttribute(source,DWMWA_TRANSITIONS_FORCEDISABLED as u32,(&mut value as *mut i32).cast(),4);
    println!("FOREIGN_TRANSITION_POLICY_READBACK_PROBE owned_fixture_hr={hr:#x} changed={}",value!=-1);
    let area=visible_area(source).unwrap();
    if !valid_pixels(frame,area){println!("FLUIDWIN_GENIE_SCREEN_PIXELS=NOT_VERIFIED invalid WGC frame");return;}
    let icon=RECT{left:area.right-40,top:area.bottom+8,right:area.right,bottom:area.bottom+48};
    let (w,h)=dims(area);
    let pixel=||{
        let _=DwmFlush();
        let dc=GetDC(ptr::null_mut());
        if dc.is_null(){return u32::MAX;}
        let color=GetPixel(dc,area.left+w/10,area.top+h/2);
        ReleaseDC(ptr::null_mut(),dc);
        color
    };
    for effect in [motion::WindowEffect::Genie,motion::WindowEffect::Drop]{
        let probe=Proxy::new_for(frame,area,icon,DockSide::Bottom,effect,96)
            .expect("initialize the upstream effect using the production proxy");
        let Render::Mesh(mesh)=&probe.render else{panic!("wrong native renderer");};
        let mips=mesh.source_mips();
        assert!(mips>0,"motion source must keep premultiplied BGRA and use mipmaps when supported");
        println!("FLUIDWIN_SOURCE_SAMPLING {} premultiplied_bgra=true mip_levels={mips}",effect.name());
        let submit=|progress|{
            let started=Instant::now();
            loop{
                let status=probe.paint(area,progress,255);
                if status!=2||started.elapsed()>=PRESENT_TIMEOUT{return status;}
                std::thread::sleep(Duration::from_millis(u64::from(FRAME_INTERVAL_MS)));
            }
        };
        let status=submit(0.0);
        assert_eq!(status,1,"{} could not submit the first frame ({status:#010x})",effect.name());
        ShowWindow(source,SW_HIDE);
        let full=pixel();
        let status=submit(0.68);
        assert_eq!(status,1,"{} could not submit an intermediate frame ({status:#010x})",effect.name());
        let bent=pixel();
        let matched=full!=u32::MAX&&[full as u8,(full>>8) as u8,(full>>16) as u8].into_iter()
            .zip(probe.marker).all(|(a,b)|a.abs_diff(b)<35);
        let hwnd=probe.hwnd;drop(probe);assert_eq!(IsWindow(hwnd),0);
        ShowWindow(source,SW_SHOWNOACTIVATE);
        println!("FLUIDWIN_{}_SCREEN_PIXELS={} proxy_released=true",effect.name().to_uppercase(),
            if matched&&bent!=u32::MAX&&bent!=full{"MATCH_AND_INTERMEDIATE_CHANGED"}else{"NOT_VERIFIED"});
    }
    FACTORY.with(|f|{f.borrow_mut().take();});
}

#[cfg(test)]
pub(super) unsafe fn probe_production_path(ids:&[WindowIdentity]){
    use super::{AppClick,Bar,ClickKey,Item,Motion,RailFade};
    use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
    let com=CoInitializeEx(None,COINIT_APARTMENTTHREADED);
    let previous=motion::settings();
    struct Reset{settings:motion::Settings,hwnd:HWND,com:bool}
    impl Drop for Reset{fn drop(&mut self){unsafe{
        reset();stage_snapshot::shutdown();CONTROL.set(ptr::null_mut());STATE.with(|s|s.borrow_mut().bars.clear());
        DestroyWindow(self.hwnd);motion::configure(self.settings);if self.com{CoUninitialize();}
    }}}
    let source=ids[0].hwnd as HWND;
    let monitor=MonitorFromWindow(source,MONITOR_DEFAULTTONEAREST);
    let mut mi:MONITORINFO=mem::zeroed();mi.cbSize=mem::size_of::<MONITORINFO>() as u32;
    assert_ne!(GetMonitorInfoW(monitor,&mut mi),0);
    let display=super::Display{id:"fixture".into(),name:"fixture".into(),bounds:mi.rcMonitor,work:mi.rcWork,primary:true,
        dpi:windows_sys::Win32::UI::HiDpi::GetDpiForWindow(source)};
    super::stage_peek::probe_external_window(ids[0],display.bounds);
    let style=super::Appearance::default();
    // A real displayed Dock layout with a magnified icon and in-flight reveal.
    let layout=super::geometry::measure(&display,&style,1,Some(0.0),0.83);
    let r=layout.window;
    let bar=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_TOPMOST,
        wide("STATIC").as_ptr(),wide("Owned Scale Dock fixture").as_ptr(),WS_POPUP,r.left,r.top,r.right-r.left,r.bottom-r.top,
        ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null_mut());
    assert!(!bar.is_null());ShowWindow(bar,SW_SHOWNOACTIVATE);
    let _reset=Reset{settings:previous,hwnd:bar,com:com.is_ok()};
    let path="fixture.exe";
    let item=|id:WindowIdentity|Item{path:path.into(),title:"Owned fixture".into(),hwnd:id.hwnd as HWND,count:ids.len(),pinned:true};
    STATE.with(|s|s.borrow_mut().bars.push(Bar{hwnd:bar,bounds:display.bounds,items:vec![item(ids[0])],
        hovered:0,hidden:false,layout:Some(layout),pinned_widgets:vec![],motion:Motion::default(),collapsed:false,
        auto_hide:false,status_hover:None,pressed:None,keyboard:None,previous_focus:0,pointer:None,
        reveal:motion::Spring::rest(0.85),rail_face:0,rail:RailFade::default(),feedback:None,
        active:None,minimized:0,attention:0,metric_repaint:None}));
    CONTROL.set(bar);
    let last=||LAST.lock().unwrap().clone().unwrap();
    let policy=motion::Settings{enabled:true,speed:70,respect_system:false,window_effect:motion::WindowEffect::Scale};
    motion::configure(motion::Settings{enabled:false,..policy});
    assert!(!minimize(bar,path,ids[0]));
    assert_eq!(last()["fallbackReason"],"motion-disabled");
    motion::configure(policy);
    let actual_policy=Policy::read();
    if actual_policy.client_animation==Some(false){
        motion::configure(motion::Settings{respect_system:true,..policy});
        assert!(!minimize(bar,path,ids[0]));assert_eq!(last()["fallbackReason"],"system-reduced-motion");
        motion::configure(policy);
    }
    let eligibility=actual_policy.gate().and_then(|_|permitted(ids[0])).and_then(|_|if elevation(ids[0].pid)==Some(false){Ok(())}else{Err("elevated-window")});
    if let Err(reason)=eligibility{
        assert!(!minimize(bar,path,ids[0]));assert_eq!(last()["fallbackReason"],reason);
        panic!("WINDOW_SCALE_PRODUCTION=NOT_VERIFIED runner eligibility={reason} policy={}",actual_policy.json());
        return;
    }
    let mut pixel_matches=0usize;
    let mut peak=crate::private_mb();
    let before=peak;
    let mut drain=|id:WindowIdentity|{
        let start=Instant::now();
        while ACTIVE.with(|a|a.borrow().is_some()){
            assert!(start.elapsed()<Duration::from_secs(5),"production Scale transition did not complete");
            timer();
            // Verify the source is still minimized. The sampled marker must
            // therefore come from the actual compositor proxy, not the app.
            ACTIVE.with(|a|{
                if let Some(t)=a.borrow().as_ref(){
                    if let Some(Phase::Animate{proxy,span})=&t.phase{
                        if IsIconic(id.hwnd as HWND)!=0&&(0.18..0.65).contains(&span.progress()){
                            let _=proxy.comp.WaitForCommitCompletion();let _=DwmFlush();
                            let mut r:RECT=mem::zeroed();GetWindowRect(proxy.hwnd,&mut r);
                            let (w,h)=dims(r);
                            let dc=GetDC(ptr::null_mut());
                            if !dc.is_null(){
                                let color=GetPixel(dc,r.left+w/10,r.top+h/2);
                                if color!=u32::MAX&&[color as u8,(color>>8) as u8,(color>>16) as u8].into_iter()
                                    .zip(proxy.marker).all(|(a,b)|a.abs_diff(b)<35){pixel_matches+=1;}
                                ReleaseDC(ptr::null_mut(),dc);
                            }
                        }
                    }
                }
            });
            let n=crate::private_mb();peak=(peak.0.max(n.0),peak.1.max(n.1));
            let mut msg:MSG=mem::zeroed();
            while PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&msg);DispatchMessageW(&msg);}
            std::thread::sleep(Duration::from_millis(16));
        }
        let result=last();
        assert_eq!(result["actualEffect"],"scale","actual production fallback: {result}");
        assert!(result["presentedFrames"].as_u64().unwrap_or(0)>=5,"animation must present multiple frames: {result}");
        assert!(result["transitionDurationMs"].as_u64().unwrap_or(0)>=400,"animation too short: {result}");
    };
    let placements=ids.iter().map(|id|placement(id.hwnd as HWND).unwrap().rcNormalPosition).collect::<Vec<_>>();
    for &id in ids{
        // Exercise the production double-click handler, not a direct proxy.
        assert!(super::finish_double_click(AppClick{owner:bar as usize,key:ClickKey::Dock(path.into()),
            window:Some(id),was_open:true,at:Instant::now()}).is_some());
        drain(id);assert_ne!(IsIconic(id.hwnd as HWND),0);
    }
    // Cross the old 15s expiration boundary and allow the idle factory to
    // release. Restore the older of two windows sharing one Dock app icon.
    std::thread::sleep(Duration::from_secs(16));
    release_factory();
    for (&id,original) in ids.iter().zip(placements){
        super::activate(bar,&item(id));
        drain(id);assert_eq!(IsIconic(id.hwnd as HWND),0);
        assert!(super::same_rect(placement(id.hwnd as HWND).unwrap().rcNormalPosition,original));
    }
    drop(drain);
    // Fault-inject a bounded delay in the existing worker, never the
    // eligibility gates. The real Dock action must finish natively once.
    stage_snapshot::forget(source);
    stage_snapshot::FIXTURE_TRANSITION_DELAY_MS.store(850,std::sync::atomic::Ordering::Relaxed);
    assert!(super::finish_double_click(AppClick{owner:bar as usize,key:ClickKey::Dock(path.into()),
        window:Some(ids[0]),was_open:true,at:Instant::now()}).is_some());
    let start=Instant::now();
    while ACTIVE.with(|a|a.borrow().is_some()){
        assert!(start.elapsed()<Duration::from_secs(3));
        timer();std::thread::sleep(Duration::from_millis(16));
    }
    stage_snapshot::FIXTURE_TRANSITION_DELAY_MS.store(0,std::sync::atomic::Ordering::Relaxed);
    assert_eq!(last()["fallbackReason"],"capture-timeout");
    while IsIconic(source)==0{
        assert!(start.elapsed()<Duration::from_secs(3),"native fallback did not minimize");
        std::thread::sleep(Duration::from_millis(16));
    }
    ShowWindowAsync(source,SW_RESTORE);
    let start=Instant::now();while IsIconic(source)!=0{
        assert!(start.elapsed()<Duration::from_secs(3));std::thread::sleep(Duration::from_millis(16));
    }
    println!("WINDOW_SCALE_PRODUCTION_FALLBACK capture_timeout=PASS single_request_native_minimize=PASS");
    let (mw,mh)=dims(display.bounds);
    println!("WINDOW_SCALE_PRODUCTION_GEOMETRY runner={}x{} dpi={} owner_2560x1080_1080x1920_4k=NOT_VERIFIED geometry_and_budget_unit=PASS",mw,mh,display.dpi);
    release_factory();
    println!("WINDOW_SCALE_PRODUCTION=PASS gestures=double-click/activate source_identities={} restore_after_16s=PASS magnification=PASS duration=PASS placement=PASS compositor_pixels={} samples={pixel_matches} memory={before:?}->{:?} peak={peak:?}",
        ids.len(),if pixel_matches>0{"MATCH"}else{"NOT_VERIFIED"},crate::private_mb());
    if cfg!(target_env="msvc"){
        let id=ids[0];
        let original=placement(source).unwrap().rcNormalPosition;
        for effect in [motion::WindowEffect::Genie,motion::WindowEffect::Drop]{
            motion::configure(motion::Settings{window_effect:effect,..policy});
            let start=||super::finish_double_click(AppClick{owner:bar as usize,key:ClickKey::Dock(path.into()),
                window:Some(id),was_open:true,at:Instant::now()});
            let pump=|until_reversible:bool|{
                let started=Instant::now();
                while ACTIVE.with(|a|a.borrow().as_ref().is_some_and(|t|!until_reversible||t.last_progress<=0.18)){
                    assert!(started.elapsed()<Duration::from_secs(5),"experimental Dock transition did not settle");
                    timer();std::thread::sleep(Duration::from_millis(16));
                }
            };
            assert!(start().is_some());
            pump(true);
            assert!(ACTIVE.with(|a|a.borrow().is_some()),"reverse during visible minimize frames");
            let proxy=ACTIVE.with(|a|match a.borrow().as_ref().and_then(|t|t.phase.as_ref()){
                Some(Phase::Animate{proxy,..})=>proxy.hwnd,
                _=>ptr::null_mut()
            });
            assert_ne!(IsIconic(source),0);
            super::activate(bar,&item(id));
            assert!(ACTIVE.with(|a|matches!(a.borrow().as_ref().and_then(|t|t.phase.as_ref()),
                Some(Phase::Animate{proxy:p,..}) if p.hwnd==proxy)));
            pump(false);
            let result=last();
            assert_eq!(result["actualEffect"],effect.name(),"{result}");
            assert_eq!(result["reversals"],1);
            assert_eq!(IsIconic(source),0);
            assert!(super::same_rect(placement(source).unwrap().rcNormalPosition,original));
            assert_eq!(IsWindow(proxy),0);
            assert!(start().is_some());
            pump(false);assert_ne!(IsIconic(source),0);
            assert_eq!(last()["actualEffect"],effect.name());
            super::activate(bar,&item(id));pump(false);
            assert_eq!(last()["actualEffect"],effect.name());
            assert_eq!(IsIconic(source),0);
            println!("FLUIDWIN_DOCK_PRODUCTION {} minimize=PASS restore=PASS reversal=PASS placement=PASS",effect.name());
        }
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn source_geometry_is_independent_of_bounded_texture_and_policy(){
        for (w,h) in [(432,246),(2560,1080),(1080,1920),(3840,2160)]{
            let area=RECT{left:-w,top:0,right:0,bottom:h};
            assert!(valid_area(area));
            let side=w.max(h).max(MAX_TEXTURE_SIDE);
            let tw=w*MAX_TEXTURE_SIDE/side;let th=h*MAX_TEXTURE_SIDE/side;
            let mut pixels=vec![255;(tw*th*4) as usize];
            let half=pixels.len()/2;pixels[..half].fill(35);
            assert!(validate_pixels(&(pixels,tw,th),area).is_ok());
        }
        let settings=motion::Settings{window_effect:motion::WindowEffect::Scale,enabled:true,respect_system:false,speed:70};
        let mut policy=Policy{motion:settings,client_animation:Some(false),native_minimize:Some(0)};
        assert_eq!(policy.gate(),Ok(()));
        policy.native_minimize=Some(1);assert_eq!(policy.gate(),Err("native-animation-overlap"));
        policy.native_minimize=Some(0);policy.motion.respect_system=true;
        assert_eq!(policy.gate(),Err("system-reduced-motion"));
        policy.motion.enabled=false;assert_eq!(policy.gate(),Err("motion-disabled"));
    }

    #[test]
    fn scale_keeps_center_aspect_and_negative_monitor_coordinates(){
        let source=RECT{left:-1160,top:-400,right:-200,bottom:140};
        let icon=RECT{left:-664,top:650,right:-616,bottom:698};
        let end=icon_fit(source,icon);
        assert_eq!(dims(end),(48,27));
        assert_eq!((end.left+end.right)/2,-640);
        assert!((end.top+end.bottom-1348).abs()<=1);
        assert!(super::super::same_rect(interpolate(source,end,0.0),source));
        assert!(super::super::same_rect(interpolate(source,end,1.0),end));
        let middle=interpolate(source,end,0.5);
        let (w,h)=dims(middle);
        assert!((w as f64/h as f64-960.0/540.0).abs()<0.015);
        assert!(!valid_pixels(&(vec![0;4*48*27],48,27),source));
    }
}
