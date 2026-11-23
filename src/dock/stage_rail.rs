//! Contrast for the small reveal handle beside a collapsed Stage.
use windows_sys::Win32::{Foundation::{HWND,RECT},Graphics::Gdi::*};

const CHARCOAL:usize=0x003f3f3f;
const WHITE:usize=0x00f6f6f6;

fn face(samples:[u32;5],previous:usize)->usize{
    let mut levels=[0u32;5];
    let mut count=0;
    for rgb in samples.into_iter().filter(|&x|x!=u32::MAX){
        let red=rgb&255;let green=(rgb>>8)&255;let blue=(rgb>>16)&255;
        levels[count]=(54*red+183*green+19*blue)>>8;
        count+=1;
    }
    if count==0{return if previous==0{WHITE}else{previous};}
    levels[..count].sort_unstable();
    let median=levels[count/2];
    // A narrow band prevents color changes over texture or antialiased text.
    let threshold=if previous==CHARCOAL{145}else{158};
    if median>=threshold{CHARCOAL}else{WHITE}
}

pub(super) unsafe fn sample(bounds:RECT,right:bool,previous:usize)->usize{
    let screen:HWND=std::ptr::null_mut();
    let dc=GetDC(screen);
    if dc.is_null(){return face([u32::MAX;5],previous);}
    // Look immediately inside the display, outside our topmost window, so
    // the rail never samples its own color on subsequent refreshes.
    let x=if right{bounds.left-2}else{bounds.right+1};
    let height=bounds.bottom-bounds.top;
    let samples=std::array::from_fn(|i|GetPixel(dc,x,bounds.top+height*(i as i32+1)/6));
    ReleaseDC(screen,dc);
    face(samples,previous)
}

pub(super) fn colors(face:usize)->(u32,u32){
    if face==CHARCOAL{(0x00888888,CHARCOAL as u32)}else{(0x00787878,WHITE as u32)}
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn responds_to_background_and_ignores_small_outliers(){
        assert_eq!(face([0xffffff,0xfcfcfc,0xeeeeee,0x000000,u32::MAX],WHITE),CHARCOAL);
        assert_eq!(face([0xffffff,0x0e0e0e,0x171717,0x111111,u32::MAX],CHARCOAL),WHITE);
        assert_eq!(face([0x969696;5],CHARCOAL),CHARCOAL);
        assert_eq!(face([0x969696;5],WHITE),WHITE);
        assert_eq!(face([u32::MAX;5],CHARCOAL),CHARCOAL);
    }
}
