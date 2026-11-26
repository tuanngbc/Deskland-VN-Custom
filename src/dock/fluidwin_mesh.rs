//! Bounded Dock-thread adapter for the upstream FluidWin GPU mesh.
use super::DockSide;
use std::{ffi::c_void, ptr::NonNull, rc::Rc};
use windows::{core::Interface, Win32::Graphics::{Direct3D11::ID3D11DeviceContext, Dxgi::IDXGISwapChain1}};
use windows_sys::Win32::Foundation::RECT;

#[cfg(target_env = "msvc")]
extern "C" {
    fn tlauncher_fluidwin_pipeline(context: *mut c_void) -> *mut c_void;
    fn tlauncher_fluidwin_release_pipeline(pipeline: *mut c_void);
    fn tlauncher_fluidwin_layer(pipeline: *mut c_void, width: u32, height: u32,
        bgra: *const u8, tw: u32, th: u32) -> *mut c_void;
    fn tlauncher_fluidwin_release_layer(layer: *mut c_void);
    fn tlauncher_fluidwin_gpu_layer(pipeline:*mut c_void,width:u32,height:u32,texture:*mut c_void)->*mut c_void;
    fn tlauncher_fluidwin_content(layer: *mut c_void) -> *mut c_void;
    fn tlauncher_fluidwin_frame_ready(layer:*mut c_void)->windows_sys::Win32::Foundation::HANDLE;
    #[cfg(test)]
    fn tlauncher_fluidwin_source_mips(layer: *mut c_void) -> u32;
    #[cfg(test)]
    fn tlauncher_fluidwin_rendered_pixels(pipeline:*mut c_void,layer:*mut c_void,pixels:*mut u8,stride:u32)->i32;
    #[cfg(test)]
    fn tlauncher_fluidwin_enable_evidence(pipeline:*mut c_void,layer:*mut c_void)->i32;
    fn tlauncher_fluidwin_draw(pipeline: *mut c_void, layer: *mut c_void,
        window: *const RECT, icon: *const RECT, canvas: *const RECT,
        side: i32, progress: f64, opacity: f32, drop: i32, dark: i32, scale: f64) -> i32;
    fn tlauncher_fluidwin_progress(t: f64, restore: i32, drop: i32) -> f64;
    fn tlauncher_fluidwin_invert(progress: f64, restore: i32, drop: i32) -> f64;
}

pub(super) struct Pipeline(NonNull<c_void>);
impl Pipeline {
    pub(super) unsafe fn new(context: &ID3D11DeviceContext) -> Option<Rc<Self>> {
        #[cfg(target_env = "msvc")]
        { NonNull::new(tlauncher_fluidwin_pipeline(context.as_raw())).map(|p| Rc::new(Self(p))) }
        #[cfg(not(target_env = "msvc"))]
        { let _ = context; None }
    }
}
impl Drop for Pipeline {
    fn drop(&mut self) {
        #[cfg(target_env = "msvc")]
        unsafe { tlauncher_fluidwin_release_pipeline(self.0.as_ptr()); }
    }
}

pub(super) struct Layer {
    raw: NonNull<c_void>,
    pipeline: Rc<Pipeline>,
    pub(super) canvas: RECT,
    window: RECT,
    icon: RECT,
    side: i32,
    drop_effect: bool,
    dark: bool,
    scale: f64,
    scale_effect:bool,
}
impl Layer {
    pub(super) unsafe fn frame_ready(&self)->windows_sys::Win32::Foundation::HANDLE{
        #[cfg(target_env="msvc")]{tlauncher_fluidwin_frame_ready(self.raw.as_ptr())}
        #[cfg(not(target_env="msvc"))]{std::ptr::null_mut()}
    }
    #[cfg(all(test,target_env="msvc"))]
    pub(super) unsafe fn source_mips(&self)->u32{tlauncher_fluidwin_source_mips(self.raw.as_ptr())}
    #[cfg(all(test,target_env="msvc"))]
    pub(super) unsafe fn enable_evidence(&self)->bool{tlauncher_fluidwin_enable_evidence(self.pipeline.0.as_ptr(),self.raw.as_ptr())!=0}
    #[cfg(all(test,target_env="msvc"))]
    pub(super) unsafe fn rendered_pixels(&self)->Option<(Vec<u8>,u32,u32)>{
        let w=u32::try_from(self.canvas.right-self.canvas.left).ok()?;
        let h=u32::try_from(self.canvas.bottom-self.canvas.top).ok()?;
        let mut pixels=vec![0;w as usize*h as usize*4];
        (tlauncher_fluidwin_rendered_pixels(self.pipeline.0.as_ptr(),self.raw.as_ptr(),pixels.as_mut_ptr(),w*4)!=0).then_some((pixels,w,h))
    }
    pub(super) unsafe fn new(pipeline: Rc<Pipeline>, frame: &super::stage_snapshot::Frame,
        window: RECT, icon: RECT, side: DockSide, drop_effect: bool, dpi: u32) -> Option<Self> {
        #[cfg(target_env = "msvc")]
        {
            let side = match side { DockSide::Bottom => 0, DockSide::Left => 2, DockSide::Right => 3 };
            // The upstream mesh extends one pixel past the source when the
            // source overlaps the Dock. Include that strip in the canvas.
            let mut canvas = RECT { left: window.left.min(icon.left).checked_sub(2)?,
                top: window.top.min(icon.top).checked_sub(2)?, right: window.right.max(icon.right).checked_add(2)?,
                bottom: window.bottom.max(icon.bottom).checked_add(2)? };
            let scale=(f64::from(dpi)/96.0).clamp(0.75,4.0);
            if drop_effect{
                let reach=5i32.checked_mul(icon.right.checked_sub(icon.left)?)?.checked_add(12)?;
                match side {
                    2=>canvas.right=canvas.right.max(icon.right.checked_add(reach)?),
                    3=>canvas.left=canvas.left.min(icon.left.checked_sub(reach)?),
                    _=>canvas.top=canvas.top.min(icon.top.checked_sub(reach)?),
                }
                let margin=(2.0*scale).ceil() as i32;
                canvas.left=canvas.left.checked_sub(margin)?;canvas.top=canvas.top.checked_sub(margin)?;
                canvas.right=canvas.right.checked_add(margin)?;canvas.bottom=canvas.bottom.checked_add(margin)?;
            }
            let width = u32::try_from(canvas.right.checked_sub(canvas.left)?).ok()?;
            let height = u32::try_from(canvas.bottom.checked_sub(canvas.top)?).ok()?;
            let raw = NonNull::new(tlauncher_fluidwin_layer(pipeline.0.as_ptr(), width, height,
                frame.0.as_ptr(), frame.1 as u32, frame.2 as u32))?;
            Some(Self { raw, pipeline, canvas, window, icon, side, drop_effect, dark:super::material_dark(), scale,scale_effect:false })
        }
        #[cfg(not(target_env = "msvc"))]
        { let _ = (pipeline, frame, window, icon, side, drop_effect, dpi); None }
    }
    pub(super) unsafe fn from_gpu(pipeline:Rc<Pipeline>,frame:&super::stage_snapshot::GpuFrame,
        window:RECT,icon:RECT,side:DockSide,effect:super::motion::WindowEffect,dpi:u32)->Option<Self>{
        #[cfg(target_env="msvc")]
        {
            let side=match side{DockSide::Bottom=>0,DockSide::Left=>2,DockSide::Right=>3};
            let scale=(f64::from(dpi)/96.0).clamp(0.75,4.0);
            let margin=if effect==super::motion::WindowEffect::Drop{5*(icon.right-icon.left)+16}else{2};
            let canvas=RECT{left:window.left.min(icon.left).checked_sub(margin)?,top:window.top.min(icon.top).checked_sub(margin)?,
                right:window.right.max(icon.right).checked_add(margin)?,bottom:window.bottom.max(icon.bottom).checked_add(margin)?};
            let raw=NonNull::new(tlauncher_fluidwin_gpu_layer(pipeline.0.as_ptr(),
                u32::try_from(canvas.right.checked_sub(canvas.left)?).ok()?,u32::try_from(canvas.bottom.checked_sub(canvas.top)?).ok()?,frame.texture.as_raw()))?;
            Some(Self{raw,pipeline,canvas,window,icon,side,drop_effect:effect==super::motion::WindowEffect::Drop,
                scale_effect:effect==super::motion::WindowEffect::Scale,dark:super::material_dark(),scale})
        }
        #[cfg(not(target_env="msvc"))]{let _=(pipeline,frame,window,icon,side,effect,dpi);None}
    }
    pub(super) unsafe fn attach(&self, visual: &windows::Win32::Graphics::DirectComposition::IDCompositionVisual) -> windows::core::Result<()> {
        #[cfg(target_env = "msvc")]
        {
            let raw = tlauncher_fluidwin_content(self.raw.as_ptr());
            let content = IDXGISwapChain1::from_raw_borrowed(&raw).expect("non-null swap chain");
            visual.SetContent(content)
        }
        #[cfg(not(target_env = "msvc"))]
        { let _ = visual; unreachable!("unsupported renderer") }
    }
    pub(super) unsafe fn paint(&self, progress: f64, opacity: u8) -> i32 {
        #[cfg(target_env = "msvc")]
        { tlauncher_fluidwin_draw(self.pipeline.0.as_ptr(), self.raw.as_ptr(),
            &self.window, &self.icon, &self.canvas, self.side, progress, f32::from(opacity) / 255.0,
            if self.scale_effect{2}else{i32::from(self.drop_effect)},i32::from(self.dark),self.scale) }
        #[cfg(not(target_env = "msvc"))]
        { let _ = (progress, opacity); 0 }
    }
}
impl Drop for Layer {
    fn drop(&mut self) {
        #[cfg(target_env = "msvc")]
        unsafe { tlauncher_fluidwin_release_layer(self.raw.as_ptr()); }
    }
}

pub(super) fn progress(t: f32, restore: bool, drop_effect: bool) -> f64 {
    #[cfg(target_env = "msvc")]
    { unsafe { tlauncher_fluidwin_progress(f64::from(t), i32::from(restore), i32::from(drop_effect)) } }
    #[cfg(not(target_env = "msvc"))]
    { let _ = (t, restore, drop_effect); 0.0 }
}
pub(super) fn invert(p: f64, restore: bool, drop_effect: bool) -> f64 {
    #[cfg(target_env = "msvc")]
    { unsafe { tlauncher_fluidwin_invert(p, i32::from(restore), i32::from(drop_effect)) } }
    #[cfg(not(target_env = "msvc"))]
    { let _ = (p, restore, drop_effect); 0.0 }
}

#[cfg(all(test,target_env="msvc"))]
#[test]
fn opposite_curve_begins_at_the_exact_presented_geometry() {
    for drop_effect in [false,true] {
        for restore in [false,true] {
            for t in [0.0,0.15,0.28,0.61,0.89,1.0] {
                let p=progress(t,restore,drop_effect);
                let reverse=invert(p,!restore,drop_effect);
                let resumed=progress(reverse as f32,!restore,drop_effect);
                assert!((resumed-p).abs()<0.000002,"same geometry across reversal: {p} -> {resumed}");
            }
        }
    }
}
