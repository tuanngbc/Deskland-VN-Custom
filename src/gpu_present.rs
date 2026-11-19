//! Live OpenGL -> D3D11 -> DirectComposition. The registered texture stays on
//! the GPU for its entire lifetime; no readback, video or GDI in normal frames.
//! All calls and destruction run on the owning thread with its GL context current.
use std::{ffi::{c_void, CString}, rc::Rc};
use glow::HasContext;
use windows::core::Interface;
use windows::Win32::{Foundation::{HMODULE, HWND}, Graphics::{Direct3D::*, Direct3D11::*, DirectComposition::*, Dxgi::{*, Common::*}}};

type Handle = *mut c_void;
type Open = unsafe extern "system" fn(*mut c_void) -> Handle;
type Close = unsafe extern "system" fn(Handle) -> i32;
type Register = unsafe extern "system" fn(Handle, *mut c_void, u32, u32, u32) -> Handle;
type Unregister = unsafe extern "system" fn(Handle, Handle) -> i32;
type Access = unsafe extern "system" fn(Handle, i32, *const Handle) -> i32;

#[derive(Clone, Copy)]
struct Interop { open:Open, close:Close, register:Register, unregister:Unregister, lock:Access, unlock:Access }
impl Interop {
    unsafe fn load() -> Result<Self,String> {
        unsafe fn proc(name:&str)->Result<unsafe extern "system" fn()->isize,String>{
            let name_c=CString::new(name).unwrap();
            let f=windows_sys::Win32::Graphics::OpenGL::wglGetProcAddress(name_c.as_ptr() as _);
            match f {Some(f) if !matches!(f as usize,0|1|2|3|usize::MAX)=>Ok(f),_=>Err(format!("Driver thiếu {name}"))}
        }
        Ok(Self{open:std::mem::transmute(proc("wglDXOpenDeviceNV")?),close:std::mem::transmute(proc("wglDXCloseDeviceNV")?),
            register:std::mem::transmute(proc("wglDXRegisterObjectNV")?),unregister:std::mem::transmute(proc("wglDXUnregisterObjectNV")?),
            lock:std::mem::transmute(proc("wglDXLockObjectsNV")?),unlock:std::mem::transmute(proc("wglDXUnlockObjectsNV")?)})
    }
}
fn wgl_error(operation:&str)->String{format!("{operation}: Windows {}",unsafe{windows_sys::Win32::Foundation::GetLastError()})}

pub struct Device {
    device:ID3D11Device, context:ID3D11DeviceContext, factory:IDXGIFactory2,
    composition:IDCompositionDevice, interop:Interop, handle:Handle,
}
impl Device {
    pub fn new()->Result<Rc<Self>,String>{unsafe{
        let interop=Interop::load()?;
        let factory:IDXGIFactory1=CreateDXGIFactory1().map_err(|e|e.to_string())?;
        let mut reasons=Vec::new();
        for index in 0..16 {
            let Ok(adapter)=factory.EnumAdapters1(index) else{break};
            let desc=adapter.GetDesc1().map_err(|e|e.to_string())?;
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0{continue;}
            let name=String::from_utf16_lossy(&desc.Description).trim_end_matches('\0').to_string();
            let mut device=None;let mut context=None;
            if let Err(e)=D3D11CreateDevice(&adapter,D3D_DRIVER_TYPE_UNKNOWN,HMODULE::default(),D3D11_CREATE_DEVICE_BGRA_SUPPORT,None,D3D11_SDK_VERSION,Some(&mut device),None,Some(&mut context)){
                reasons.push(format!("{name}: {e}"));continue;
            }
            let device=device.unwrap();let context=context.unwrap();
            let handle=(interop.open)(device.as_raw());
            if handle.is_null(){reasons.push(format!("{name}: {}",wgl_error("OpenDevice")));continue;}
            let build=(||->windows::core::Result<Self>{
                let dxgi:IDXGIDevice1=device.cast()?;dxgi.SetMaximumFrameLatency(1)?;
                let composition:IDCompositionDevice=DCompositionCreateDevice(&dxgi)?;
                Ok(Self{device,context,factory:factory.cast()?,composition,interop,handle})
            })();
            match build {
                Ok(value)=>{crate::log!("GPU trực tiếp: {name}, WGL/D3D11/DirectComposition");return Ok(Rc::new(value));},
                Err(e)=>{(interop.close)(handle);reasons.push(format!("{name}: {e}"));}
            }
        }
        Err(format!("Không có adapter chia sẻ được với OpenGL: {}",reasons.join("; ")))
    }}
}
impl Drop for Device{fn drop(&mut self){unsafe{(self.interop.close)(self.handle);}}}

pub struct Output {
    device:Rc<Device>, texture:ID3D11Texture2D, swapchain:IDXGISwapChain1,
    target:IDCompositionTarget, _visual:IDCompositionVisual,
    framebuffer:glow::Framebuffer, renderbuffer:glow::Renderbuffer, object:Handle,
    width:u32, height:u32, locked:bool, pub presents:u64,
}
impl Output {
    pub fn new(device:Rc<Device>,gl:&glow::Context,hwnd:*mut c_void,width:i32,height:i32,physical_width:i32,physical_height:i32)->Result<Self,String>{unsafe{
        let build=(||->windows::core::Result<_>{
            let desc=D3D11_TEXTURE2D_DESC{Width:width as _,Height:height as _,MipLevels:1,ArraySize:1,Format:DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc:DXGI_SAMPLE_DESC{Count:1,Quality:0},Usage:D3D11_USAGE_DEFAULT,BindFlags:(D3D11_BIND_RENDER_TARGET.0|D3D11_BIND_SHADER_RESOURCE.0) as _,..Default::default()};
            let mut texture=None;device.device.CreateTexture2D(&desc,None,Some(&mut texture))?;
            let swap_desc=DXGI_SWAP_CHAIN_DESC1{Width:width as _,Height:height as _,Format:desc.Format,SampleDesc:desc.SampleDesc,
                BufferUsage:DXGI_USAGE_RENDER_TARGET_OUTPUT,BufferCount:2,Scaling:DXGI_SCALING_STRETCH,SwapEffect:DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,AlphaMode:DXGI_ALPHA_MODE_IGNORE,..Default::default()};
            let swapchain=device.factory.CreateSwapChainForComposition(&device.device,&swap_desc,None)?;
            let target=device.composition.CreateTargetForHwnd(HWND(hwnd),true)?;
            let visual=device.composition.CreateVisual()?;
            let scale=device.composition.CreateScaleTransform()?;
            scale.SetScaleX2(physical_width as f32/width as f32)?;scale.SetScaleY2(physical_height as f32/height as f32)?;
            visual.SetTransform(&scale)?;visual.SetContent(&swapchain)?;
            target.SetRoot(&visual)?;device.composition.Commit()?;
            Ok((texture.unwrap(),swapchain,target,visual))
        })();
        let(texture,swapchain,target,visual)=build.map_err(|e|e.to_string())?;
        let renderbuffer=gl.create_renderbuffer()?;
        let framebuffer=match gl.create_framebuffer(){Ok(f)=>f,Err(e)=>{gl.delete_renderbuffer(renderbuffer);return Err(e);}};
        let object=(device.interop.register)(device.handle,texture.as_raw(),renderbuffer.0.get(),glow::RENDERBUFFER,0x0002 /* WRITE_DISCARD */);
        if object.is_null(){gl.delete_framebuffer(framebuffer);gl.delete_renderbuffer(renderbuffer);return Err(wgl_error("RegisterObject"));}
        let mut output=Self{device,texture,swapchain,target,_visual:visual,framebuffer,renderbuffer,object,width:width as _,height:height as _,locked:false,presents:0};
        let result=(||->Result<(),String>{
            output.begin()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER,Some(framebuffer));
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER,glow::COLOR_ATTACHMENT0,glow::RENDERBUFFER,Some(renderbuffer));
            let status=gl.check_framebuffer_status(glow::FRAMEBUFFER);
            gl.bind_framebuffer(glow::FRAMEBUFFER,None);
            output.unlock()?;
            if status!=glow::FRAMEBUFFER_COMPLETE{return Err(format!("FBO chia sẻ chưa hoàn chỉnh: 0x{status:x}"));}
            Ok(())
        })();
        if let Err(e)=result{output.destroy(gl);return Err(e);}
        Ok(output)
    }}
    pub fn begin(&mut self)->Result<glow::Framebuffer,String>{unsafe{
        if self.locked{return Err("Texture đang được OpenGL sử dụng".into());}
        if (self.device.interop.lock)(self.device.handle,1,&self.object)==0{return Err(wgl_error("LockObjects"));}
        self.locked=true;Ok(self.framebuffer)
    }}
    fn unlock(&mut self)->Result<(),String>{unsafe{
        if self.locked{
            if (self.device.interop.unlock)(self.device.handle,1,&self.object)==0{return Err(wgl_error("UnlockObjects"));}
            self.locked=false;
        }Ok(())
    }}
    pub fn present(&mut self,gl:&glow::Context)->Result<(),String>{unsafe{
        gl.bind_framebuffer(glow::FRAMEBUFFER,None);
        self.unlock()?; // Includes GL/D3D synchronization, no CPU readback.
        let buffer:ID3D11Texture2D=self.swapchain.GetBuffer(0).map_err(|e|e.to_string())?;
        self.device.context.CopyResource(&buffer,&self.texture);
        self.swapchain.Present(0,DXGI_PRESENT(0)).ok().map_err(|e|e.to_string())?;
        self.presents+=1;Ok(())
    }}
    /// Explicit diagnostics / lock-screen still image only. Never called by
    /// normal frame presentation, including when either output is covered.
    pub fn read_bgra(&self)->Result<Vec<u8>,String>{unsafe{
        if self.locked{return Err("Không đọc texture đang bị khóa".into());}
        let mut desc=D3D11_TEXTURE2D_DESC::default();self.texture.GetDesc(&mut desc);
        desc.Usage=D3D11_USAGE_STAGING;desc.BindFlags=0;desc.CPUAccessFlags=D3D11_CPU_ACCESS_READ.0 as _;desc.MiscFlags=0;
        let mut staging=None;self.device.device.CreateTexture2D(&desc,None,Some(&mut staging)).map_err(|e|e.to_string())?;let staging=staging.unwrap();
        self.device.context.CopyResource(&staging,&self.texture);
        let mut mapped=D3D11_MAPPED_SUBRESOURCE::default();
        self.device.context.Map(&staging,0,D3D11_MAP_READ,0,Some(&mut mapped)).map_err(|e|e.to_string())?;
        let stride=self.width as usize*4;let mut pixels=vec![0;stride*self.height as usize];
        for (y,row) in pixels.chunks_exact_mut(stride).enumerate(){std::ptr::copy_nonoverlapping((mapped.pData as *const u8).add(y*mapped.RowPitch as usize),row.as_mut_ptr(),stride);}
        self.device.context.Unmap(&staging,0);Ok(pixels)
    }}
    pub fn destroy(&mut self,gl:&glow::Context){unsafe{
        let _=self.target.SetRoot(None::<&IDCompositionVisual>);let _=self.device.composition.Commit();
        let _=self.unlock();
        if !self.object.is_null(){(self.device.interop.unregister)(self.device.handle,self.object);self.object=std::ptr::null_mut();
            gl.delete_framebuffer(self.framebuffer);gl.delete_renderbuffer(self.renderbuffer);}
    }}
}
