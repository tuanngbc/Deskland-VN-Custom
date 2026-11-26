//! Original centro.h two-line News rows on the existing Control Center bitmap.
use std::{ffi::c_void,ptr::NonNull};
use windows_sys::Win32::{Foundation::RECT,Graphics::Gdi::HDC};
#[cfg(test)]thread_local!{static FAIL_NEXT:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
#[cfg(test)]pub(super) fn fail_next(){FAIL_NEXT.set(true);}
pub(super) struct Canvas(NonNull<c_void>);
#[repr(C)]struct Surface{pixels:*const u8,bytes:usize,width:i32,height:i32,stride:u32}
#[cfg(target_env="msvc")]extern "C"{
    fn tlauncher_forecast_canvas_create()->*mut c_void;
    fn tlauncher_forecast_canvas_free(canvas:*mut c_void);
    fn tlauncher_news_row_draw(canvas:*mut c_void,dc:HDC,surface:*const Surface,rect:*const RECT,dpi:f32,title:*const u16,time:*const u16,dark:bool,focused:bool)->bool;
}
impl Canvas{
    pub(super) fn new()->Option<Self>{
        #[cfg(target_env="msvc")]unsafe{NonNull::new(tlauncher_forecast_canvas_create()).map(Self)}
        #[cfg(not(target_env="msvc"))]{None}
    }
    pub(super) unsafe fn draw(&mut self,layer:&mut super::glass_probe::Layer,rect:RECT,dpi:u32,title:&str,time:&str,dark:bool,focused:bool)->bool{
        #[cfg(test)]if FAIL_NEXT.replace(false){super::fill(layer.dc(),rect,0x00ff00ff);return false;}
        #[cfg(target_env="msvc")]{
            let dc=layer.dc();let (width,height)=layer.size;let pixels=layer.pixels();let surface=Surface{pixels:pixels.as_ptr(),bytes:pixels.len(),width,height,stride:width as u32*4};
            tlauncher_news_row_draw(self.0.as_ptr(),dc,&surface,&rect,dpi as f32/96.0,crate::desktop::wide(title).as_ptr(),crate::desktop::wide(time).as_ptr(),dark,focused)
        }
        #[cfg(not(target_env="msvc"))]{let _=(layer,rect,dpi,title,time,dark,focused);false}
    }
}
impl Drop for Canvas{fn drop(&mut self){#[cfg(target_env="msvc")]unsafe{tlauncher_forecast_canvas_free(self.0.as_ptr());}}}

#[cfg(all(test,target_env="msvc"))]mod tests{
    use super::*;
    use windows_sys::Win32::Graphics::Gdi::*;
    #[test]#[ignore="hosted original News two-line DirectWrite pixels at three DPI values, no feed/network/browser"]
    fn native_news_rows_wrap_two_lines_preserve_glass_and_timestamp(){
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(),Ok("true"));
        unsafe{for dpi in [96,144,192]{let px=|n|super::super::panel_chrome::scale(dpi,n);let (w,h)=(px(322),px(56));let mut layer=super::super::glass_probe::Layer::new(w*2,h*2).unwrap();
            SetMapMode(layer.dc(),MM_ANISOTROPIC);SetWindowExtEx(layer.dc(),w,h,std::ptr::null_mut());SetViewportExtEx(layer.dc(),w*2,h*2,std::ptr::null_mut());
            for (locale,title) in [("vi","Tin thử nghiệm dài: thời tiết hôm nay và dự báo những ngày tiếp theo trong tuần"),("en","Synthetic news headline about the weather today and the forecast for the coming week"),("zh-CN","这是用于检验两行新闻标题的示例内容，包含本周天气与未来几天的预报信息")]{
                super::super::fill(layer.dc(),RECT{left:0,top:0,right:w,bottom:h},0x00c8c8c8);GdiFlush();let mut c=Canvas::new().unwrap();
                assert!(c.draw(&mut layer,RECT{left:0,top:0,right:w,bottom:h},dpi,title,"14:35",false,false));GdiFlush();
                let p=layer.pixels();assert_eq!(&p[..3],&[200,200,200],"row corners preserve actual glass");
                if let Some(root)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){let rgba=p.chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],255]).collect::<Vec<_>>();image::save_buffer(std::path::PathBuf::from(root).join(format!("news-row-{locale}-{dpi}.png")),&rgba,(w*2) as u32,(h*2) as u32,image::ColorType::Rgba8).unwrap();}
                let ink=|left,top,right,bottom|p.chunks_exact(4).enumerate().filter(|(i,p)|{let x=*i as i32%(w*2);let y=*i as i32/(w*2);x>=left*2&&x<right*2&&y>=top*2&&y<bottom*2&&p[..3].iter().all(|c|*c<150)}).count();
                assert!(ink(px(8),px(7),w-px(52),px(22))>20,"first news line missing for {locale}/{dpi}");
                assert!(ink(px(8),px(24),w-px(52),px(39))>20,"second news line missing for {locale}/{dpi}");
                assert!(ink(w-px(44),px(6),w-px(8),px(23))>10,"independent time missing for {locale}/{dpi}");
                assert_eq!(ink(px(8),px(42),w-px(8),h),0,"title must not spill into a third line");
            }
        }}
        println!("FLUIDWIN_NEWS_ROW original_two_lines=VERIFIED independent_timestamp=VERIFIED glass=PRESERVED dpi=96,144,192 locales=vi,en,zh scoped_software_target=RELEASED network=NOT_INVOKED");
    }
}
