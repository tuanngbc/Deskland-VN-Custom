//! Original FluidWin Spotlight ripple and edge shader on the shared Shell GPU.
//! A scoped, click-through overlay; no permanent upstream Onda thread/device.
use std::{cell::{Cell,RefCell},ffi::c_void,ptr::{self,NonNull},sync::Arc,time::{Duration,Instant}};
use windows::core::Interface;
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::*,UI::{WindowsAndMessaging::*,HiDpi::GetDpiForWindow},System::{LibraryLoader::GetModuleHandleW,Threading::WaitForSingleObject}};
const CLASS:&str="TLauncherSpotlightWave";
const TIMER:usize=1;
#[cfg(target_env="msvc")]extern "C"{
    fn tlauncher_wave_create(device:*mut c_void,hwnd:HWND,w:u32,h:u32,pixels:*const u32)->*mut c_void;
    fn tlauncher_wave_free(raw:*mut c_void);
    fn tlauncher_wave_ready(raw:*mut c_void)->HANDLE;
    fn tlauncher_wave_draw(raw:*mut c_void,context:*mut c_void,time:f32,x:f32,y:f32,dpi:f32,fade:f32,photo:bool)->i32;
    fn tlauncher_wave_present(raw:*mut c_void,settled:bool)->i32;
    fn tlauncher_wave_release_photo(raw:*mut c_void);
    fn tlauncher_fluidwin_capture_budget(device:*mut c_void)->u64;
    #[cfg(test)]fn tlauncher_wave_evidence(raw:*mut c_void)->bool;
    #[cfg(test)]fn tlauncher_wave_pixels(raw:*mut c_void,context:*mut c_void,pixels:*mut u8,stride:u32)->bool;
}
struct Wave{raw:NonNull<c_void>,hwnd:HWND,owner:HWND,area:RECT,center:POINT,dpi:f32,speed:f32,gpu:Arc<crate::shell_gpu::ShellGpu>,
    started:Instant,closing:Option<Instant>,photo_until:f32,photo:bool,shown:bool,last_present:Instant}
thread_local!{
    static ENABLED:Cell<bool>=const{Cell::new(false)};
    static ACTIVE:RefCell<Option<Wave>>=const{RefCell::new(None)};
}
impl Drop for Wave{fn drop(&mut self){unsafe{
    KillTimer(self.hwnd,TIMER);
    #[cfg(target_env="msvc")]tlauncher_wave_free(self.raw.as_ptr());
    DestroyWindow(self.hwnd);
}}}
unsafe extern "system" fn proc(h:HWND,m:u32,w:WPARAM,l:LPARAM)->LRESULT{match m{
    WM_NCHITTEST=>HTTRANSPARENT as isize,WM_MOUSEACTIVATE=>MA_NOACTIVATE as isize,
    WM_ERASEBKGND=>1,WM_TIMER if w==TIMER=>{watchdog(h);0},
    WM_DISPLAYCHANGE|WM_DPICHANGED|WM_SETTINGCHANGE|WM_CLOSE=>{stop();0},
    _=>DefWindowProcW(h,m,w,l),
}}
pub(super) fn configure(enabled:bool){ENABLED.set(enabled);if !enabled||!crate::dock::popup_motion_allowed(){stop();}}
pub(super) fn stop(){let old=ACTIVE.with(|s|s.borrow_mut().take());drop(old);}
pub(super) fn close(owner:HWND){ACTIVE.with(|s|if let Some(w)=s.borrow_mut().as_mut().filter(|w|w.owner==owner){w.closing.get_or_insert_with(Instant::now);});}
pub(super) fn resume(owner:HWND){ACTIVE.with(|s|if let Some(w)=s.borrow_mut().as_mut().filter(|w|w.owner==owner){w.closing=None;});}
fn bounds(point:POINT)->Option<RECT>{unsafe{
    let m=MonitorFromPoint(point,MONITOR_DEFAULTTONULL);let mut info:MONITORINFO=std::mem::zeroed();info.cbSize=std::mem::size_of::<MONITORINFO>() as u32;
    (!m.is_null()&&GetMonitorInfoW(m,&mut info)!=0).then_some(info.rcMonitor)
}}
unsafe fn photograph(area:RECT)->Option<Vec<u32>>{
    let (w,h)=(area.right.checked_sub(area.left)?,area.bottom.checked_sub(area.top)?);
    let count=usize::try_from(w).ok()?.checked_mul(usize::try_from(h).ok()?)?;
    if count==0||count>33_554_432{return None;}
    // Native fixtures never photograph/log unrelated runner windows.
    if cfg!(test){return Some(vec![0xfff0f0f0;count]);}
    let screen=GetDC(ptr::null_mut());if screen.is_null(){return None;}
    let dc=CreateCompatibleDC(screen);let mut info:BITMAPINFO=std::mem::zeroed();
    info.bmiHeader.biSize=std::mem::size_of::<BITMAPINFOHEADER>() as u32;info.bmiHeader.biWidth=w;info.bmiHeader.biHeight=-h;
    info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;info.bmiHeader.biCompression=BI_RGB;
    let mut bits=ptr::null_mut();let bitmap=CreateDIBSection(screen,&info,DIB_RGB_COLORS,&mut bits,ptr::null_mut(),0);
    let result=if !dc.is_null()&&!bitmap.is_null()&&!bits.is_null(){
        let old=SelectObject(dc,bitmap);let copied=BitBlt(dc,0,0,w,h,screen,area.left,area.top,SRCCOPY)!=0;GdiFlush();
        let pixels=copied.then(||std::slice::from_raw_parts(bits.cast::<u32>(),count).iter().map(|p|p|0xff000000).collect());
        SelectObject(dc,old);pixels
    }else{None};
    if !bitmap.is_null(){DeleteObject(bitmap);}if !dc.is_null(){DeleteDC(dc);}ReleaseDC(ptr::null_mut(),screen);result
}
impl Wave{
    unsafe fn new(owner:HWND,area:RECT,center:POINT,pixels:&[u32])->Option<Self>{
        let (w,h)=(area.right.checked_sub(area.left)?,area.bottom.checked_sub(area.top)?);
        if w<=0||h<=0||pixels.len()!=usize::try_from(w).ok()?.checked_mul(usize::try_from(h).ok()?)?{return None;}
        let gpu=crate::shell_gpu::ShellGpu::acquire().ok()?;
        #[cfg(target_env="msvc")]if pixels.len() as u64*16>tlauncher_fluidwin_capture_budget(gpu.device.as_raw()){return None;}
        let instance=GetModuleHandleW(ptr::null());let class=crate::desktop::wide(CLASS);
        let wc=WNDCLASSW{lpfnWndProc:Some(proc),hInstance:instance,lpszClassName:class.as_ptr(),..std::mem::zeroed()};RegisterClassW(&wc);
        let hwnd=CreateWindowExW(WS_EX_NOREDIRECTIONBITMAP|WS_EX_TOPMOST|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_TRANSPARENT|WS_EX_LAYERED,
            class.as_ptr(),crate::desktop::wide("").as_ptr(),WS_POPUP,area.left,area.top,w,h,ptr::null_mut(),ptr::null_mut(),instance,ptr::null());
        if hwnd.is_null(){return None;}SetLayeredWindowAttributes(hwnd,0,255,LWA_ALPHA);
        #[cfg(target_env="msvc")]let raw=NonNull::new(tlauncher_wave_create(gpu.device.as_raw(),hwnd,w as u32,h as u32,pixels.as_ptr()));
        #[cfg(not(target_env="msvc"))]let raw=None;
        let Some(raw)=raw else{DestroyWindow(hwnd);return None;};
        let dpi=GetDpiForWindow(owner).clamp(72,384) as f32/96.0;
        let center=POINT{x:(center.x-area.left).clamp(0,w),y:(center.y-area.top).clamp(0,h)};
        let far=(center.x.max(w-center.x) as f32).hypot(center.y.max(h-center.y) as f32);
        Some(Self{raw,hwnd,owner,area,center,dpi,speed:f32::from(crate::dock::motion::settings().speed)/100.0,gpu,
            started:Instant::now(),closing:None,photo_until:far/(1700.0*dpi)+0.65,photo:true,shown:false,last_present:Instant::now()})
    }
    unsafe fn draw_at(&mut self,time:f32,fade:f32)->bool{
        #[cfg(target_env="msvc")]{
            let context=self.gpu.context();
            if tlauncher_wave_draw(self.raw.as_ptr(),context.as_raw(),time,self.center.x as f32,self.center.y as f32,self.dpi,fade,self.photo)!=1{return false;}
            drop(context);
            let result=tlauncher_wave_present(self.raw.as_ptr(),!self.photo&&self.closing.is_none());
            if result==1{self.last_present=Instant::now();true}else{result==2&&self.last_present.elapsed()<Duration::from_millis(250)}
        }
        #[cfg(not(target_env="msvc"))]{let _=(time,fade);false}
    }
    unsafe fn valid(&self)->bool{
        if !ENABLED.get()||!crate::dock::popup_motion_allowed(){return false;}
        if self.closing.is_none()&&(IsWindow(self.owner)==0||IsWindowVisible(self.owner)==0||IsIconic(self.owner)!=0){return false;}
        let point=POINT{x:self.area.left+(self.area.right-self.area.left)/2,y:self.area.top+(self.area.bottom-self.area.top)/2};
        if self.closing.is_none()&&MonitorFromWindow(self.owner,MONITOR_DEFAULTTONULL)!=MonitorFromPoint(point,MONITOR_DEFAULTTONULL){return false;}
        if !bounds(point).is_some_and(|a|a.left==self.area.left&&a.top==self.area.top&&a.right==self.area.right&&a.bottom==self.area.bottom){return false;}
        true
    }
    unsafe fn step(&mut self)->bool{
        if !self.valid(){return false;}
        let fade=self.closing.map_or(1.0,|at|1.0-(at.elapsed().as_secs_f32()*self.speed/0.32).clamp(0.0,1.0));
        if fade<=0.0{return false;}
        let time=self.started.elapsed().as_secs_f32()*self.speed;
        if self.photo&&time>=self.photo_until{#[cfg(target_env="msvc")]tlauncher_wave_release_photo(self.raw.as_ptr());self.photo=false;}
        if !self.draw_at(time,fade*fade*(3.0-2.0*fade)){return false;}
        if !self.shown{
            if IsWindow(self.owner)==0{return false;}
            if SetWindowPos(self.hwnd,self.owner,self.area.left,self.area.top,self.area.right-self.area.left,self.area.bottom-self.area.top,SWP_NOACTIVATE|SWP_SHOWWINDOW)==0{return false;}
            self.shown=true;self.started=Instant::now();
        }
        true
    }
}
pub(super) fn open(owner:HWND,point:POINT,center:POINT){
    stop();if !ENABLED.get()||!super::fullscreen::motion_allowed()||!crate::dock::popup_motion_allowed(){return;}
    unsafe{
        let Some(area)=bounds(point)else{return;};
        let Some(pixels)=photograph(area)else{return;};
        let Some(wave)=Wave::new(owner,area,center,&pixels)else{crate::log!("SPOTLIGHT_WAVE unavailable; search remains available");return;};
        // Rendering is woken by the queue handle in the existing application
        // message wait. This timer only bounds device/visibility failure.
        if SetTimer(wave.hwnd,TIMER,250,None)==0{return;}
        ACTIVE.with(|s|s.replace(Some(wave)));
    }
}
unsafe fn tick(hwnd:HWND){
    let Some(mut wave)=ACTIVE.with(|s|s.borrow_mut().take())else{return;};
    if wave.hwnd==hwnd&&wave.step(){ACTIVE.with(|s|s.replace(Some(wave)));}
}
pub(super) fn frame_clock()->HANDLE{
    #[cfg(target_env="msvc")]return ACTIVE.with(|s|s.borrow().as_ref().map_or(ptr::null_mut(),|w|unsafe{tlauncher_wave_ready(w.raw.as_ptr())}));
    #[cfg(not(target_env="msvc"))]ptr::null_mut()
}
pub(super) fn frame_wake(){
    let hwnd=ACTIVE.with(|s|s.borrow().as_ref().map(|w|w.hwnd));if let Some(hwnd)=hwnd{unsafe{tick(hwnd);}}
}
unsafe fn watchdog(hwnd:HWND){
    let old=ACTIVE.with(|s|s.borrow_mut().take());
    if let Some(wave)=old{if wave.hwnd==hwnd&&wave.valid()&&wave.last_present.elapsed()<Duration::from_millis(500){ACTIVE.with(|s|s.replace(Some(wave)));}}
}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="hosted original wave shader, entire-canvas alpha/perimeters, shared device and scoped teardown"]
fn native_original_wave_renders_perimeters_and_releases_shared_resources(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
    unsafe{
        let com=CoInitializeEx(None,COINIT_APARTMENTTHREADED);assert!(com.is_ok());
        struct Com;impl Drop for Com{fn drop(&mut self){unsafe{CoUninitialize();}}}let _com=Com;
        let owner=CreateWindowExW(WS_EX_TOOLWINDOW,crate::desktop::wide("STATIC").as_ptr(),crate::desktop::wide("Synthetic wave source").as_ptr(),WS_POPUP,
            20,20,320,180,ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),ptr::null());assert!(!owner.is_null());
        struct Owner(HWND);impl Drop for Owner{fn drop(&mut self){unsafe{DestroyWindow(self.0);}}}let _owner=Owner(owner);
        const W:usize=640;const H:usize=400;
        let pixels=(0..W*H).map(|i|{let x=i%W;let y=i/W;0xff000000|((x*255/(W-1)) as u32)<<16|((y*255/(H-1)) as u32)<<8|0x50}).collect::<Vec<_>>();
        let area=RECT{left:20,top:20,right:20+W as i32,bottom:20+H as i32};let center=POINT{x:340,y:62};
        for cycle in 0..4{
            let mut wave=Wave::new(owner,area,center,&pixels).expect("shared-device wave creation");
            wave.dpi=1.0; // canonical shader fixture; owner display scaling is not inferred
            let shared=crate::shell_gpu::ShellGpu::acquire().unwrap();assert!(Arc::ptr_eq(&shared,&wave.gpu));drop(shared);
            let allocation=Arc::downgrade(&wave.gpu);let hwnd=wave.hwnd;
            assert_eq!(SendMessageW(hwnd,WM_NCHITTEST,0,0),HTTRANSPARENT as isize);
            assert_ne!(GetWindowLongW(hwnd,GWL_EXSTYLE) as u32&WS_EX_NOACTIVATE,0);
            assert!(tlauncher_wave_evidence(wave.raw.as_ptr()));
            let mut lit_perimeter=false;let mut partial=false;
            for (index,(time,fade,photo)) in [(0.0,1.0,true),(0.06,1.0,true),(0.18,1.0,true),(0.42,1.0,true),(1.4,1.0,false),(1.5,0.0,false)].into_iter().enumerate(){
                wave.photo=photo;assert_eq!(WaitForSingleObject(tlauncher_wave_ready(wave.raw.as_ptr()),3000),WAIT_OBJECT_0);
                assert!(wave.draw_at(time,fade));
                if index==0{ShowWindow(hwnd,SW_SHOWNOACTIVATE);}
                let mut output=vec![0u8;W*H*4];let context=wave.gpu.context();
                assert!(tlauncher_wave_pixels(wave.raw.as_ptr(),context.as_raw(),output.as_mut_ptr(),W as u32*4));drop(context);
                for (pixel_index,p) in output.chunks_exact(4).enumerate(){
                    assert!(p[..3].iter().all(|c|*c<=p[3].saturating_add(2)),"premultiplied full canvas at pose {index}");
                    partial|=p[3]>0&&p[3]<255;
                    let (x,y)=(pixel_index%W,pixel_index/W);if x==0||x==W-1||y==0||y==H-1{lit_perimeter|=p[3]>0;}
                }
                if index==0||fade==0.0{assert!(output.iter().all(|p|*p==0),"start/final canvas must be completely transparent");}
                if !photo&&fade==1.0{assert_eq!(&output[((H/2)*W+W/2)*4..((H/2)*W+W/2)*4+4],&[0u8;4],"settled glow leaves the screen center transparent");}
                if cycle==0{if let Some(root)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){
                    let rgba=output.chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],p[3]]).collect::<Vec<_>>();
                    image::save_buffer(std::path::PathBuf::from(root).join(format!("spotlight-wave-{index}.png")),&rgba,W as u32,H as u32,image::ColorType::Rgba8).unwrap();
                }}
            }
            assert!(partial&&lit_perimeter);drop(wave);assert_eq!(IsWindow(hwnd),0);assert!(allocation.upgrade().is_none(),"no idle wave GPU owner");
        }
    }
    println!("FLUIDWIN_SPOTLIGHT_WAVE original_shader=EXECUTED full_canvas=PREMULTIPLIED edge_glow=PASS settled_center=TRANSPARENT first_final=TRANSPARENT shared_device=PASS repeated_teardown=PASS physical_pacing=NOT_MEASURED");
}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="hosted actual Compact/Fullscreen wave routes; synthetic screenshot content only"]
fn native_wave_follows_both_spotlight_modes_and_stops_on_disable(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
    unsafe{
        let com=CoInitializeEx(None,COINIT_APARTMENTTHREADED);assert!(com.is_ok());
        struct Reset;impl Drop for Reset{fn drop(&mut self){super::close_spotlight();super::configure_spotlight(super::library::Preferences::default());unsafe{CoUninitialize();}}}let _reset=Reset;
        let previous=crate::dock::motion::settings();
        struct Motion(crate::dock::motion::Settings);impl Drop for Motion{fn drop(&mut self){crate::dock::motion::configure(self.0);}}let _motion=Motion(previous);
        crate::dock::motion::configure(crate::dock::motion::Settings{enabled:true,respect_system:false,speed:70,..previous});
        for fullscreen in [false,true]{
            super::configure_spotlight(super::library::Preferences{edge_wave:true,hotkey_fullscreen:fullscreen,..Default::default()});
            super::toggle(ptr::null_mut());
            let owner=if fullscreen{super::fullscreen::window().unwrap()}else{super::handles().unwrap().0};
            let hwnd=ACTIVE.with(|s|s.borrow().as_ref().filter(|w|w.owner==owner).map(|w|w.hwnd)).expect("actual Spotlight route created wave");
            let before=GetForegroundWindow();let start=Instant::now();
            while !ACTIVE.with(|s|s.borrow().as_ref().is_some_and(|w|w.shown)){
                let mut msg:MSG=std::mem::zeroed();for _ in 0..64{if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&msg);DispatchMessageW(&msg);}
                let clock=frame_clock();if !clock.is_null()&&WaitForSingleObject(clock,0)==WAIT_OBJECT_0{frame_wake();}
                assert!(start.elapsed()<Duration::from_secs(3));std::thread::sleep(Duration::from_millis(8));
            }
            assert_eq!(GetForegroundWindow(),before,"wave must not activate");
            close(owner);assert!(ACTIVE.with(|s|s.borrow().as_ref().unwrap().closing.is_some()));
            resume(owner);assert!(ACTIVE.with(|s|s.borrow().as_ref().unwrap().closing.is_none()));
            close(owner);ACTIVE.with(|s|s.borrow_mut().as_mut().unwrap().closing=Some(Instant::now()-Duration::from_secs(1)));
            tick(hwnd);assert!(ACTIVE.with(|s|s.borrow().is_none()));assert_eq!(IsWindow(hwnd),0);
            let mut rect:RECT=std::mem::zeroed();GetWindowRect(owner,&mut rect);let point=POINT{x:(rect.left+rect.right)/2,y:(rect.top+rect.bottom)/2};
            open(owner,point,point);assert!(ACTIVE.with(|s|s.borrow().is_some()));configure(false);assert!(ACTIVE.with(|s|s.borrow().is_none()));
            super::close_spotlight();
        }
    }
    println!("FLUIDWIN_SPOTLIGHT_WAVE compact_route=PASS fullscreen_route=PASS nonactivating=PASS close_resume=PASS fade_teardown=PASS disable=RELEASED source_pixels=SYNTHETIC owner_quality=NOT_VERIFIED");
}
