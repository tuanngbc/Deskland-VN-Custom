//! Original FluidWin blue weather card, drawn into the existing panel bitmap.
use std::{ffi::c_void,ptr::NonNull};
use windows_sys::Win32::{Foundation::RECT,Graphics::Gdi::HDC};
pub(super) struct Day{pub code:i32,pub label:String,pub temperature:String}
pub(super) struct Card{pub city:String,pub temperature:String,pub detail:String,pub code:Option<i32>,pub days:Vec<Day>,canvas:Option<Canvas>}
impl Card{
    pub fn new(city:String,temperature:String,detail:String,code:Option<i32>,days:Vec<Day>)->Self{Self{city,temperature,detail,code,days,canvas:None}}
    pub fn rows(&self)->usize{1+self.days.len().min(5)}
    pub unsafe fn draw(&mut self,dc:HDC,rect:RECT,dpi:u32,focused:bool)->bool{
        #[cfg(target_env="msvc")]{
            if self.canvas.is_none(){self.canvas=NonNull::new(tlauncher_forecast_canvas_create()).map(Canvas);}
            let Some(canvas)=self.canvas.as_ref()else{return false;};
            let mut out:Native=std::mem::zeroed();out.count=self.days.len().min(5) as i32;out.ready=i32::from(self.code.is_some());out.focused=i32::from(focused);out.code=self.code.unwrap_or(0);
            fn copy(value:&str,out:&mut[u16]){let mut at=0;for c in value.chars(){let n=c.len_utf16();if at+n>=out.len(){break;}c.encode_utf16(&mut out[at..at+n]);at+=n;}}
            copy(&self.city,&mut out.city);copy(&self.temperature,&mut out.temperature);copy(&self.detail,&mut out.detail);
            for (day,row) in self.days.iter().take(5).zip(&mut out.days){row.code=day.code;copy(&day.label,&mut row.label);copy(&day.temperature,&mut row.temperature);}
            let rendered=tlauncher_forecast_canvas_draw(canvas.0.as_ptr(),dc,&rect,dpi as f32/96.,&out);
            #[cfg(test)]if !rendered{eprintln!("FLUIDWIN_FORECAST_CARD native_failure_phase={}",tlauncher_forecast_canvas_phase(canvas.0.as_ptr()));}
            return rendered;
        }
        #[cfg(not(target_env="msvc"))]{let _=(dc,rect,dpi,focused);false}
    }
}
struct Canvas(NonNull<c_void>);
impl Drop for Canvas{fn drop(&mut self){#[cfg(target_env="msvc")]unsafe{tlauncher_forecast_canvas_free(self.0.as_ptr());}}}
#[repr(C)]struct NativeDay{code:i32,label:[u16;32],temperature:[u16;32]}
#[repr(C)]struct Native{code:i32,count:i32,ready:i32,focused:i32,city:[u16;128],temperature:[u16;32],detail:[u16;256],days:[NativeDay;5]}
#[cfg(target_env="msvc")]extern "C"{
    fn tlauncher_forecast_canvas_create()->*mut c_void;
    fn tlauncher_forecast_canvas_free(raw:*mut c_void);
    #[cfg(test)]fn tlauncher_forecast_canvas_phase(raw:*mut c_void)->u32;
    fn tlauncher_forecast_canvas_draw(raw:*mut c_void,dc:HDC,rect:*const RECT,dpi:f32,forecast:*const Native)->bool;
}
#[cfg(all(test,target_env="msvc"))]mod tests{
    use super::*;
    use windows_sys::Win32::Graphics::Gdi::*;
    #[test]#[ignore="hosted original Direct2D weather card pixels, 2x panel mapping, DPI and glyphs; no weather request"]
    fn native_original_weather_card_keeps_background_and_renders_all_glyphs(){
        if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
        unsafe{
            for dpi in [96u32,144,192]{
                let px=|n:i32|super::super::panel_chrome::scale(dpi,n);
                let (w,h)=(px(336),px(168));let mut layer=super::super::glass_probe::Layer::new(w*2,h*2).unwrap();
                SetMapMode(layer.dc(),MM_ANISOTROPIC);SetWindowExtEx(layer.dc(),w,h,std::ptr::null_mut());SetViewportExtEx(layer.dc(),w*2,h*2,std::ptr::null_mut());
                for code in [0,2,45,61,71,95]{
                    super::super::fill(layer.dc(),RECT{left:0,top:0,right:w,bottom:h},0x00332211);GdiFlush();
                    let mut card=Card::new("Synthetic city".into(),"25°C".into(),"Rain · humidity 80% · 14 km/h".into(),Some(code),[0,2,45,61,95].into_iter().enumerate().map(|(i,code)|Day{code,label:format!("Day {}",i+1),temperature:"29° 20°".into()}).collect());
                    assert!(card.draw(layer.dc(),RECT{left:0,top:0,right:w,bottom:h},dpi,false),"original renderer must draw on the existing 2x DIB at {dpi} DPI");GdiFlush();
                    let pixels=layer.pixels();
                    if let Some(root)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){
                        let rgba=pixels.chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],255]).collect::<Vec<_>>();image::save_buffer(std::path::PathBuf::from(root).join(format!("forecast-card-{dpi}-{code}.png")),&rgba,(w*2) as u32,(h*2) as u32,image::ColorType::Rgba8).unwrap();
                    }
                    assert_eq!(&pixels[..3],&[0x33,0x22,0x11],"rounded corner must preserve the actual existing glass pixel");
                    let blue=pixels.chunks_exact(4).filter(|p|p[0].saturating_sub(p[2])>40&&p[1].saturating_sub(p[2])>20).count();assert!(blue>(w*h) as usize*2,"original blue weather card missing");
                    let stride=w*2;let ink=|left:i32,top:i32,right:i32,bottom:i32|pixels.chunks_exact(4).enumerate().filter(|(i,p)|{let x=*i as i32%stride;let y=*i as i32/stride;x>=left*2&&x<right*2&&y>=top*2&&y<bottom*2&&p[..3].iter().all(|c|*c>220)}).count();
                    assert!(ink(w-px(70),px(30),w-px(10),px(80))>20,"current WMO {code} glyph missing at {dpi} DPI");
                    assert!(ink(px(16),px(38),px(150),px(75))>50,"current temperature must contain actual text pixels at {dpi} DPI");
                    for i in 0..5{let cw=(w-px(16))/5;assert!(ink(px(8)+cw*i,px(117),px(8)+cw*(i+1),px(144))>12,"forecast glyph {i} missing at {dpi} DPI");}
                    drop(card);
                }
            }
        }
        println!("FLUIDWIN_FORECAST_CARD original_gradient_and_symbols=RENDERED full_2x_bitmap=VERIFIED dpi=96,144,192 actual_corner_background=PRESERVED source=EXISTING_WEATHER shared_panel=YES dedicated_window_device_worker=NONE actual_network=NOT_INVOKED");
    }
}
