//! Original standalone volume glass, only after confirmed master-volume UI
//! actions. Native volume-key replacement remains a separate input route.
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::*,UI::WindowsAndMessaging::*};
use std::cell::Cell;
pub(super) const CLOSED:u32=WM_APP+166;
thread_local!{static TICKET:Cell<usize>=const{Cell::new(0)};}
#[cfg(target_env="msvc")]extern "C"{
    fn tlauncher_volume_osd_show(controller:HWND,notice:u32,ticket:usize,target:*const RECT,percent:i32,muted:bool,speed:f64,animate:bool,muted_label:*const u16,name:*const u16)->HWND;
    fn tlauncher_volume_osd_close(ticket:usize);
    fn tlauncher_battery_osd_show(controller:HWND,notice:u32,ticket:usize,target:*const RECT,percent:i32,charging:bool,speed:f64,animate:bool,text:*const u16)->HWND;
    fn tlauncher_headphones_osd_show(controller:HWND,notice:u32,ticket:usize,target:*const RECT,percent:i32,speed:f64,animate:bool,text:*const u16)->HWND;
}
pub(super) unsafe fn show(panel:HWND,audio:super::utilities::Audio){
    if IsWindow(panel)==0||!super::STATE.with(|s|s.try_borrow().is_ok_and(|s|s.prefs.enabled&&!s.suspended)){return;}
    let mut info:MONITORINFO=std::mem::zeroed();info.cbSize=std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(MonitorFromWindow(panel,MONITOR_DEFAULTTONULL),&mut info)==0{return;}
    let (level,muted)=audio.osd_value();
    let muted_text=crate::launcher::localized("Tắt tiếng","Muted","静音");
    let name=format!("{}: {}",crate::launcher::localized("Âm lượng","Volume","音量"),if muted{muted_text.into()}else{format!("{level}%")});
    super::shell_menu::configure_menu();let ticket=TICKET.with(|t|{let value=t.get().wrapping_add(1).max(1);t.set(value);value});
    #[cfg(target_env="msvc")]tlauncher_volume_osd_show(super::CONTROL.get(),CLOSED,ticket,&info.rcWork,level as i32,muted,super::motion::settings().speed as f64/100.,super::popup_motion_allowed(),crate::desktop::wide(muted_text).as_ptr(),crate::desktop::wide(&name).as_ptr());
}
pub(super) unsafe fn close(ticket:usize){
    #[cfg(target_env="msvc")]tlauncher_volume_osd_close(ticket);
}
pub(super) unsafe fn battery(level:u8,charging:bool,text:&str){
    utility_notice(Some(level),Some(charging),text);
}
pub(super) unsafe fn headphones(level:Option<u8>,text:&str){
    utility_notice(level,None,text);
}
unsafe fn utility_notice(level:Option<u8>,charging:Option<bool>,text:&str){
    let owner=super::STATE.with(|s|s.try_borrow().ok().and_then(|s|{
        if !s.prefs.enabled||s.suspended{return None;}s.bars.iter().find(|b|!b.hidden).map(|b|b.hwnd)
    }));let Some(owner)=owner else{return;};
    let mut info:MONITORINFO=std::mem::zeroed();info.cbSize=std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(MonitorFromWindow(owner,MONITOR_DEFAULTTONULL),&mut info)==0||super::window_fullscreen(GetForegroundWindow(),info.rcMonitor){return;}
    super::shell_menu::configure_menu();let ticket=TICKET.with(|t|{let value=t.get().wrapping_add(1).max(1);t.set(value);value});
    #[cfg(target_env="msvc")]match charging{
        Some(charging)=>{tlauncher_battery_osd_show(super::CONTROL.get(),CLOSED,ticket,&info.rcWork,level.unwrap_or(0) as i32,charging,super::motion::settings().speed as f64/100.,super::popup_motion_allowed(),crate::desktop::wide(text).as_ptr());},
        None=>{tlauncher_headphones_osd_show(super::CONTROL.get(),CLOSED,ticket,&info.rcWork,level.map_or(-1,i32::from),super::motion::settings().speed as f64/100.,super::popup_motion_allowed(),crate::desktop::wide(text).as_ptr());},
    }
}

#[cfg(all(test,target_env="msvc"))]
mod tests{
    use super::*;
    use std::{ptr,time::{Duration,Instant}};
    use crate::desktop::wide;
    unsafe fn pump(){let mut msg:MSG=std::mem::zeroed();while PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)!=0{if msg.message==CLOSED{close(msg.wParam);}else{TranslateMessage(&msg);DispatchMessageW(&msg);}}}
    #[test]
    #[ignore="actual original volume OSD pixels, no-activation, tickets and scoped teardown on hosted Windows; no audio read/write"]
    fn native_volume_osd_displays_confirmed_values_without_audio_or_input(){
        if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
        unsafe{
            let owner=CreateWindowExW(0,wide("STATIC").as_ptr(),wide("OSD fixture controller").as_ptr(),0,0,0,1,1,HWND_MESSAGE,ptr::null_mut(),ptr::null_mut(),ptr::null());assert!(!owner.is_null());
            struct Cleanup(HWND);impl Drop for Cleanup{fn drop(&mut self){unsafe{close(0);DestroyWindow(self.0);}}}let _cleanup=Cleanup(owner);
            let mut info:MONITORINFO=std::mem::zeroed();info.cbSize=std::mem::size_of::<MONITORINFO>() as u32;assert_ne!(GetMonitorInfoW(MonitorFromPoint(POINT{x:0,y:0},MONITOR_DEFAULTTOPRIMARY),&mut info),0);
            let focus=GetForegroundWindow();
            for (index,muted,name) in [(0,"Tắt tiếng","Âm lượng: Tắt tiếng"),(1,"Muted","Volume: Muted"),(2,"静音","音量: 静音")]{
                let ticket=index+10;let hwnd=tlauncher_volume_osd_show(owner,CLOSED,ticket,&info.rcWork,47,true,0.70,true,wide(muted).as_ptr(),wide(name).as_ptr());assert!(!hwnd.is_null());
                let settle=Instant::now();while settle.elapsed()<Duration::from_millis(420){pump();std::thread::sleep(Duration::from_millis(10));}windows_sys::Win32::Graphics::Dwm::DwmFlush();
                assert_ne!(IsWindowVisible(hwnd),0);assert_eq!(GetForegroundWindow(),focus);
                let flags=GetWindowLongPtrW(hwnd,GWL_EXSTYLE) as u32;assert_eq!(flags&(WS_EX_NOACTIVATE|WS_EX_TRANSPARENT|WS_EX_TOOLWINDOW),WS_EX_NOACTIVATE|WS_EX_TRANSPARENT|WS_EX_TOOLWINDOW);
                let mut text=[0u16;80];let n=GetWindowTextW(hwnd,text.as_mut_ptr(),80);assert_eq!(String::from_utf16_lossy(&text[..n as usize]),name);
                let mut rect:RECT=std::mem::zeroed();assert_ne!(GetWindowRect(hwnd,&mut rect),0);assert!(rect.left>=info.rcWork.left&&rect.right<=info.rcWork.right&&rect.top>=info.rcWork.top&&rect.bottom<=info.rcWork.bottom);
                if let Some(dir)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){
                    let (w,h)=(rect.right-rect.left,rect.bottom-rect.top);let mut layer=super::super::glass_probe::Layer::new(w,h).unwrap();let dc=GetDC(ptr::null_mut());assert_ne!(BitBlt(layer.dc(),0,0,w,h,dc,rect.left,rect.top,SRCCOPY|CAPTUREBLT),0);ReleaseDC(ptr::null_mut(),dc);
                    let mut rgba=layer.pixels().to_vec();for p in rgba.chunks_exact_mut(4){p.swap(0,2);p[3]=255;}image::save_buffer(std::path::PathBuf::from(dir).join(format!("volume-osd-{index}.png")),&rgba,w as u32,h as u32,image::ColorType::Rgba8).unwrap();
                }
                // A stale hide notice from the previous display cannot tear
                // down the new confirmed reading on the reused native HWND.
                close(ticket-1);assert_ne!(IsWindow(hwnd),0);close(ticket);assert_eq!(IsWindow(hwnd),0);
            }
            for cycle in 0..3{
                let ticket=100+cycle;let hwnd=tlauncher_volume_osd_show(owner,CLOSED,ticket,&info.rcWork,73,false,0.70,false,wide("Muted").as_ptr(),wide("Volume: 73%").as_ptr());assert!(!hwnd.is_null());
                let deadline=Instant::now()+Duration::from_secs(4);while IsWindow(hwnd)!=0&&Instant::now()<deadline{pump();std::thread::sleep(Duration::from_millis(10));}assert_eq!(IsWindow(hwnd),0,"expiry must release native HWND, font and D2D surface");
            }
            for (index,(headphone,text)) in [(false,"Pin yếu · 10%"),(false,"Low battery · 10%"),(false,"电量低 · 10%"),(true,"Tai nghe mẫu · đã kết nối"),(true,"Synthetic headset · 73%"),(true,"示例耳机 · 已连接")].into_iter().enumerate(){
                let ticket=200+index;let hwnd=if headphone{tlauncher_headphones_osd_show(owner,CLOSED,ticket,&info.rcWork,if index==4{73}else{-1},0.70,false,wide(text).as_ptr())}else{tlauncher_battery_osd_show(owner,CLOSED,ticket,&info.rcWork,10,false,0.70,false,wide(text).as_ptr())};assert!(!hwnd.is_null());
                let settle=Instant::now();while settle.elapsed()<Duration::from_millis(200){pump();std::thread::sleep(Duration::from_millis(10));}
                windows_sys::Win32::Graphics::Dwm::DwmFlush();let mut label=[0u16;160];let n=GetWindowTextW(hwnd,label.as_mut_ptr(),160);assert_eq!(String::from_utf16_lossy(&label[..n as usize]),text);assert_eq!(GetForegroundWindow(),focus);
                let mut rect:RECT=std::mem::zeroed();assert_ne!(GetWindowRect(hwnd,&mut rect),0);
                let (w,h)=(rect.right-rect.left,rect.bottom-rect.top);let mut layer=super::super::glass_probe::Layer::new(w,h).unwrap();let dc=GetDC(ptr::null_mut());assert_ne!(BitBlt(layer.dc(),0,0,w,h,dc,rect.left,rect.top,SRCCOPY|CAPTUREBLT),0);ReleaseDC(ptr::null_mut(),dc);
                let mut rgba=layer.pixels().to_vec();for p in rgba.chunks_exact_mut(4){p.swap(0,2);p[3]=255;}
                if let Some(dir)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){
                    let name=if headphone{format!("volume-osd-headphones-{}.png",index-3)}else{format!("volume-osd-battery-{index}.png")};
                    image::save_buffer(std::path::PathBuf::from(dir).join(name),&rgba,w as u32,h as u32,image::ColorType::Rgba8).unwrap();
                }
                // A correct HWND/title alone does not prove the content reached
                // the display. The synthetic icon/text must differ from the
                // actual glass background after the first composition settles.
                let base=&rgba[((h/2*w+w-4)*4) as usize..][..3];
                let ink=rgba.chunks_exact(4).filter(|p|p[..3].iter().zip(base).any(|(a,b)|a.abs_diff(*b)>70)).count();
                assert!(ink>80,"actual battery notice is blank for locale {index}: {ink}");
                let glyph=rgba.chunks_exact(4).enumerate().filter(|(i,p)|{let x=*i as i32%w;let y=*i as i32/w;x>w*12/372&&x<w*38/372&&y>h/4&&y<h*3/4&&p[..3].iter().zip(base).any(|(a,b)|a.abs_diff(*b)>70)}).count();
                assert!(glyph>12,"actual utility glyph is blank for notice {index}: {glyph}");
                close(ticket-1);assert_ne!(IsWindow(hwnd),0);close(ticket);assert_eq!(IsWindow(hwnd),0);
            }
            assert_eq!(GetForegroundWindow(),focus);
            println!("FLUIDWIN_VOLUME_OSD native_original=VERIFIED vi_en_zh=EXPORTED no_activation=VERIFIED stale_close=REFUSED expiry_teardown=VERIFIED audio_read_write=NOT_INVOKED global_volume_keys=NOT_IMPLEMENTED");
        }
    }
}
