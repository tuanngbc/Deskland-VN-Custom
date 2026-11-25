//! Shared on-demand D3D11 device for the native shell surfaces and WGC.
//! Based on FluidWin gpu.h/viva.h (MIT, Copyright (c) 2026 Clooudy).
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use windows::{core::{Error, Interface, Result}, Win32::{Foundation::{E_FAIL, HMODULE}, Graphics::{
    Direct3D::D3D_DRIVER_TYPE_HARDWARE, Direct3D10::ID3D10Multithread, Direct3D11::*,
    DirectComposition::{DCompositionCreateDevice,IDCompositionDevice}, Dxgi::{IDXGIDevice,IDXGIDevice1},
}}};

static SHELL_GPU: Mutex<Weak<ShellGpu>> = Mutex::new(Weak::new());

pub(crate) struct ShellGpu {
    pub(crate) device: ID3D11Device,
    context: ID3D11DeviceContext,
    batch: Mutex<()>,
}

impl ShellGpu {
    pub(crate) unsafe fn acquire() -> Result<Arc<Self>> {
        let mut cache = SHELL_GPU.lock().unwrap_or_else(|e|e.into_inner());
        if let Some(existing) = cache.upgrade().filter(|old|old.device.GetDeviceRemovedReason().is_ok()){
            return Ok(existing);
        }
        let mut device=None;let mut context=None;
        D3D11CreateDevice(None,D3D_DRIVER_TYPE_HARDWARE,HMODULE::default(),D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,D3D11_SDK_VERSION,Some(&mut device),None,Some(&mut context))?;
        let device:ID3D11Device=device.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let context:ID3D11DeviceContext=context.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        // WGC also submits commands from its worker threads.
        context.cast::<ID3D10Multithread>()?.SetMultithreadProtected(true);
        if let Ok(dxgi)=device.cast::<IDXGIDevice1>() { let _=dxgi.SetMaximumFrameLatency(1); }
        let gpu=Arc::new(Self{device,context,batch:Mutex::new(())});
        *cache=Arc::downgrade(&gpu);
        Ok(gpu)
    }
    pub(crate) unsafe fn composition(&self)->Result<IDCompositionDevice>{
        DCompositionCreateDevice(&self.device.cast::<IDXGIDevice>()?)
    }
    pub(crate) fn context(&self)->Context<'_>{
        Context{_batch:self.batch.lock().unwrap_or_else(|e|e.into_inner()),context:&self.context}
    }
}
impl Drop for ShellGpu{
    fn drop(&mut self){
        unsafe{self.context.ClearState();self.context.Flush();}
    }
}
pub(crate) struct Context<'a>{_batch:MutexGuard<'a,()>,context:&'a ID3D11DeviceContext}
impl std::ops::Deref for Context<'_>{
    type Target=ID3D11DeviceContext;
    fn deref(&self)->&Self::Target{self.context}
}

#[cfg(test)]
#[test]
#[ignore="hardware D3D identity and release on Windows GitHub Actions"]
fn native_shell_graphics_shares_the_device_across_ui_and_capture_threads(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    let Ok(ui)=(unsafe{ShellGpu::acquire()})else{println!("SHELL_GPU_SHARE=NOT_VERIFIED hardware device unavailable");return;};
    let capture=std::thread::spawn(||unsafe{ShellGpu::acquire().unwrap()}).join().unwrap();
    assert!(Arc::ptr_eq(&ui,&capture));
    let allocation=Arc::downgrade(&ui);
    drop(capture);drop(ui);
    assert!(allocation.upgrade().is_none(),"shared shell GPU has no idle strong owner");
    println!("SHELL_GPU_SHARE=PASS device_identity=shared idle_owner=none");
}
