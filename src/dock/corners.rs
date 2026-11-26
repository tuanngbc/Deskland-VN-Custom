//! Shared FluidWin own-UI corner policy. Never changes a foreign app's HWND.
use super::appearance::CornerStyle;
use std::sync::atomic::{AtomicU32,Ordering};
use windows_sys::Win32::{Foundation::HWND,Graphics::Dwm::DwmSetWindowAttribute,
    System::Threading::GetCurrentProcessId,UI::WindowsAndMessaging::GetWindowThreadProcessId};
static STYLE:AtomicU32=AtomicU32::new(0);
pub(crate) fn current()->CornerStyle{match STYLE.load(Ordering::Acquire){1=>CornerStyle::Small,2=>CornerStyle::Square,_=>CornerStyle::Rounded}}
pub(crate) fn configure(style:CornerStyle)->bool{STYLE.swap(style as u32,Ordering::AcqRel)!=style as u32}
pub(crate) fn radius(base:f32)->f32{base.max(0.0)*current().factor()}
pub(crate) unsafe fn apply_dwm(hwnd:HWND)->bool{
    let mut pid=0;if hwnd.is_null()||GetWindowThreadProcessId(hwnd,&mut pid)==0||pid!=GetCurrentProcessId(){return false;}
    let preference=current().dwm();DwmSetWindowAttribute(hwnd,33,(&preference as *const u32).cast(),4)>=0
}

#[cfg(test)]mod tests{
    use super::*;
    use std::ptr;
    use crate::desktop::wide;
    use windows_sys::Win32::{Foundation::*,Graphics::{Dwm::DwmGetWindowAttribute,Gdi::*},System::LibraryLoader::GetModuleHandleW,UI::WindowsAndMessaging::*};
    #[test]
    #[ignore="real own HWND region, native DWM and foreign refusal; hosted Windows only"]
    fn native_owned_corner_masks_regions_and_foreign_refusal(){unsafe{
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(),Ok("true"));
        struct Restore(CornerStyle);impl Drop for Restore{fn drop(&mut self){configure(self.0);}}
        let _restore=Restore(current());
        let window=CreateWindowExW(0,wide("STATIC").as_ptr(),wide("Owned corner fixture").as_ptr(),WS_OVERLAPPEDWINDOW,10,10,320,240,
            ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());assert!(!window.is_null());
        let foreign=GetDesktopWindow();let mut foreign_before=0xdeadbeefu32;
        let foreign_hr=DwmGetWindowAttribute(foreign,33,(&mut foreign_before as *mut u32).cast(),4);
        let mut samples=Vec::new();let mut native=0;
        for style in [CornerStyle::Rounded,CornerStyle::Small,CornerStyle::Square]{
            configure(style);let mut pixels=vec![255u8;64*64*4];super::super::panel_chrome::mask_popup(&mut pixels,64,64,16.0,230);
            samples.push(pixels[(2*64+2)*4+3]);assert!(pixels.chunks_exact(4).all(|p|p[..3].iter().all(|&c|c<=p[3])));
            super::super::panel_chrome::clip_corners(window,320,240,96);
            let region=CreateRectRgn(0,0,0,0);assert_ne!(GetWindowRgn(window,region),0);
            assert_ne!(PtInRegion(region,160,120),0);
            assert_eq!(PtInRegion(region,0,0)!=0,style==CornerStyle::Square);DeleteObject(region);
            let applied=apply_dwm(window);let mut preference=0u32;
            if applied&&DwmGetWindowAttribute(window,33,(&mut preference as *mut u32).cast(),4)>=0{assert_eq!(preference,style.dwm());native+=1;}
            assert!(!apply_dwm(foreign));
        }
        assert!(samples[0]<samples[1]&&samples[1]<samples[2]);
        let mut foreign_after=0xdeadbeefu32;let after_hr=DwmGetWindowAttribute(foreign,33,(&mut foreign_after as *mut u32).cast(),4);
        assert_eq!(after_hr,foreign_hr);assert_eq!(foreign_after,foreign_before);DestroyWindow(window);
        println!("FLUIDWIN_CORNERS mask_three_styles=PASS owned_regions=PASS native_dwm_readback_styles={native}/3 foreign_desktop=REFUSED unchanged_foreign_attribute=PASS");
    }}
}
