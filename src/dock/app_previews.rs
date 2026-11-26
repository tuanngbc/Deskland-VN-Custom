//! Original FluidWin multi-window DWM cards. No capture worker or idle timer.
use super::{WindowIdentity,Item,DockSide,STATE,CONTROL};
use std::{cell::RefCell,ffi::c_void,ptr::{self,NonNull},time::{Instant,Duration}};
use windows_sys::Win32::{Foundation::*,UI::WindowsAndMessaging::*};
pub(super) const TIMER:usize=25;
#[cfg(target_env="msvc")]
extern "C"{
    fn tlauncher_previews_open(sources:*const HWND,count:u32,icon:*const RECT,area:*const RECT,side:i32,owner:HWND,
        speed:f64,reduced:bool,minimized:*const u16,unavailable:*const u16)->*mut c_void;
    fn tlauncher_previews_poll(raw:*mut c_void,target:*mut HWND)->i32;
    fn tlauncher_previews_hover(raw:*mut c_void)->bool;
    fn tlauncher_previews_visible(raw:*mut c_void)->bool;
    fn tlauncher_previews_close(raw:*mut c_void);
    fn tlauncher_previews_free(raw:*mut c_void);
    #[cfg(test)] fn tlauncher_previews_card(raw:*mut c_void,index:u32)->HWND;
    #[cfg(test)] fn tlauncher_previews_thumbnails(raw:*mut c_void)->u32;
}
struct Native(NonNull<c_void>);
impl Drop for Native{fn drop(&mut self){#[cfg(target_env="msvc")]unsafe{tlauncher_previews_free(self.0.as_ptr());}}}
struct Pending{owner:usize,path:String,since:Instant}
struct Panel{native:Native,owner:usize,path:String,sources:Vec<WindowIdentity>,last_inside:Instant}
thread_local!{
    static PENDING:RefCell<Option<Pending>>=const{RefCell::new(None)};
    static PANEL:RefCell<Option<Panel>>=const{RefCell::new(None)};
}
unsafe fn hovered(owner:HWND)->Option<String>{
    STATE.with(|s|{
        let state=s.borrow();if state.suspended||!state.prefs.enabled{return None;}
        let bar=state.bars.iter().find(|b|b.hwnd==owner&&!b.hidden&&!b.collapsed)?;
        if bar.pressed.is_some(){return None;}
        let index=usize::try_from(bar.hovered).ok()?;
        let item=bar.items.get(index)?;
        (item.count>0).then(||item.path.clone())
    })
}
pub(super) unsafe fn mouse(owner:HWND){
    let path=hovered(owner);
    let same=PENDING.with(|p|p.borrow().as_ref().is_some_and(|p|p.owner==owner as usize&&Some(p.path.as_str())==path.as_deref()));
    if !same{PENDING.with(|p|p.replace(path.map(|path|Pending{owner:owner as usize,path,since:Instant::now()})));}
    if PENDING.with(|p|p.borrow().is_some())||PANEL.with(|p|p.borrow().is_some()){
        SetTimer(CONTROL.with(std::cell::Cell::get),TIMER,80,None);
    }
}
pub(super) fn leave(owner:HWND){PENDING.with(|p|{let mut p=p.borrow_mut();if p.as_ref().is_some_and(|p|p.owner==owner as usize){p.take();}});}
pub(super) fn open_for(owner:HWND)->bool{PANEL.with(|p|p.borrow().as_ref().is_some_and(|p|p.owner==owner as usize))}
pub(super) unsafe fn close_for(owner:HWND){
    PENDING.with(|p|{let mut p=p.borrow_mut();if owner.is_null()||p.as_ref().is_some_and(|p|p.owner==owner as usize){p.take();}});
    let old=PANEL.with(|p|{let mut p=p.borrow_mut();if owner.is_null()||p.as_ref().is_some_and(|p|p.owner==owner as usize){p.take()}else{None}});
    drop(old);
    stop_if_idle();
}
unsafe fn stop_if_idle(){if PENDING.with(|p|p.borrow().is_none())&&PANEL.with(|p|p.borrow().is_none()){KillTimer(CONTROL.with(std::cell::Cell::get),TIMER);}}
pub(super) unsafe fn poll(){
    #[cfg(target_env="msvc")]
    {
        let active=PANEL.with(|p|p.borrow_mut().take());
        if let Some(mut panel)=active{
            let owner=panel.owner as HWND;
            let valid=IsWindow(owner)!=0&&!super::window_effect::active()&&!super::window_motion::active()&&!super::window_geometry::active()&&panel.sources.iter().all(|id|
                id.matches(id.hwnd as HWND)&&super::previews::permitted_pid(id.hwnd as HWND).is_ok());
            if valid{
                let mut target=ptr::null_mut();let action=tlauncher_previews_poll(panel.native.0.as_ptr(),&mut target);
                let selected=panel.sources.iter().find(|id|id.hwnd==target as usize).copied();
                if action!=0{
                    if let Some(id)=selected.filter(|id|id.matches(target)&&super::previews::permitted_pid(target).is_ok()){
                        if action==1{
                            leave(owner);
                            let item=Item{path:panel.path.clone(),title:String::new(),hwnd:target,count:1,pinned:false};
                            drop(panel);super::activate(owner,&item);super::panel_closed(owner);stop_if_idle();return;
                        }else if action==2{PostMessageW(target,WM_CLOSE,0,0);}
                    }
                }
                let inside=hovered(owner).as_deref()==Some(panel.path.as_str())||tlauncher_previews_hover(panel.native.0.as_ptr());
                if inside{panel.last_inside=Instant::now();}
                else if panel.last_inside.elapsed()>Duration::from_millis(350){tlauncher_previews_close(panel.native.0.as_ptr());}
                if tlauncher_previews_visible(panel.native.0.as_ptr()){PANEL.with(|p|p.replace(Some(panel)));}
                else{drop(panel);super::panel_closed(owner);}
            }else{drop(panel);super::panel_closed(owner);}
        }
        let ready=PENDING.with(|p|p.borrow().as_ref().filter(|p|p.since.elapsed()>=Duration::from_millis(500)).map(|p|(p.owner,p.path.clone())));
        if let Some((owner,path))=ready{
            if PANEL.with(|p|p.borrow().as_ref().is_some_and(|p|p.owner==owner&&p.path==path)){stop_if_idle();return;}
            let owner=owner as HWND;
            if hovered(owner).as_deref()!=Some(path.as_str())||super::window_effect::active()||super::window_motion::active()||super::window_geometry::active()||super::utilities::panel_open_for(owner){stop_if_idle();return;}
            let sources=STATE.with(|s|{
                let state=s.borrow();let bar=state.bars.iter().find(|b|b.hwnd==owner)?;
                Some(state.windows.iter().filter(|w|w.path.eq_ignore_ascii_case(&path)&&
                    (state.prefs.all_windows||super::rect_intersects(w.rect,bar.bounds)))
                    .filter_map(|w|WindowIdentity::read(w.hwnd)).filter(|id|super::previews::permitted_pid(id.hwnd as HWND).is_ok()).take(8).collect::<Vec<_>>())
            }).unwrap_or_default();
            if sources.is_empty(){leave(owner);stop_if_idle();return;}
            let anchor=super::dock_app_anchor(owner,Some(&path));
            let Some(area)=anchor.work()else{leave(owner);stop_if_idle();return;};
            let p=anchor.point;let icon=RECT{left:p.x-24,top:p.y-24,right:p.x+24,bottom:p.y+24};
            let side=match anchor.side{DockSide::Bottom=>0,DockSide::Top=>1,DockSide::Left=>2,DockSide::Right=>3};
            let handles=sources.iter().map(|id|id.hwnd as HWND).collect::<Vec<_>>();
            let text=|vi,en,zh|crate::desktop::wide(crate::launcher::localized(vi,en,zh));
            let native=NonNull::new(tlauncher_previews_open(handles.as_ptr(),handles.len() as u32,&icon,&area,side,owner,
                f64::from(super::motion::settings().speed)/100.0,!super::motion::allowed(),text("Đã thu nhỏ","Minimized","已最小化").as_ptr(),
                text("Không có bản xem trước","No preview","预览不可用").as_ptr()));
            if let Some(native)=native{
                PANEL.with(|p|p.replace(Some(Panel{native:Native(native),owner:owner as usize,path,sources,last_inside:Instant::now()})));
            }else{leave(owner);}
        }
    }
    stop_if_idle();
}

#[cfg(all(test,target_env="msvc"))]
#[test]
#[ignore="original FluidWin multi-window DWM previews on hosted Windows; no foreign close or input injection"]
fn native_original_previews_select_close_reconcile_and_release(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    use crate::desktop::wide;
    use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
    unsafe{
        CoInitializeEx(None,COINIT_APARTMENTTHREADED).unwrap();
        struct Window(HWND);impl Drop for Window{fn drop(&mut self){unsafe{DestroyWindow(self.0);}}}
        let window=|name:&str,x:i32|{
            let h=CreateWindowExW(0,wide("STATIC").as_ptr(),wide(name).as_ptr(),WS_OVERLAPPEDWINDOW|WS_VISIBLE,
                x,50,280,220,ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),ptr::null());
            assert!(!h.is_null());Window(h)
        };
        let owner=window("Preview Dock fixture",10);let source=window("Live source",320);let minimized=window("Minimized source",610);
        ShowWindow(minimized.0,SW_MINIMIZE);
        let foreground=GetForegroundWindow();
        let pump=||{
            let start=Instant::now();let mut msg:MSG=std::mem::zeroed();
            while start.elapsed()<Duration::from_millis(120){
                for _ in 0..64{if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&msg);DispatchMessageW(&msg);}
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        let sources=[source.0,minimized.0];let icon=RECT{left:460,top:690,right:508,bottom:738};let area=RECT{left:0,top:0,right:1024,bottom:768};
        for _ in 0..4{
            let native=Native(NonNull::new(tlauncher_previews_open(sources.as_ptr(),2,&icon,&area,0,owner.0,0.70,true,
                wide("Minimized").as_ptr(),wide("No preview").as_ptr())).unwrap());
            pump();assert_eq!(GetForegroundWindow(),foreground,"preview does not take focus");
            let card=tlauncher_previews_card(native.0.as_ptr(),0);let second=tlauncher_previews_card(native.0.as_ptr(),1);
            assert!(!card.is_null()&&!second.is_null());assert_eq!(tlauncher_previews_thumbnails(native.0.as_ptr()),1);
            let mut client:RECT=std::mem::zeroed();GetClientRect(card,&mut client);
            let click=(((20u32)<<16)|((client.right-20) as u32 & 0xffff)) as isize;
            PostMessageW(card,WM_LBUTTONUP,0,click);pump();
            let mut target=ptr::null_mut();assert_eq!(tlauncher_previews_poll(native.0.as_ptr(),&mut target),2);
            assert_eq!(target,source.0);assert_ne!(IsWindow(source.0),0,"bridge only returns a close request");
            PostMessageW(card,WM_LBUTTONUP,0,20|(20<<16));pump();
            assert_eq!(tlauncher_previews_poll(native.0.as_ptr(),&mut target),1);assert_eq!(target,source.0);
            drop(native);assert_eq!(IsWindow(card),0);assert_eq!(IsWindow(second),0);
        }
        let native=Native(NonNull::new(tlauncher_previews_open(sources.as_ptr(),2,&icon,&area,0,owner.0,0.70,true,
            wide("Minimized").as_ptr(),wide("No preview").as_ptr())).unwrap());
        pump();DestroyWindow(source.0);let mut target=ptr::null_mut();tlauncher_previews_poll(native.0.as_ptr(),&mut target);
        assert_eq!(tlauncher_previews_thumbnails(native.0.as_ptr()),0,"destroyed source unregisters its DWM link");
        tlauncher_previews_close(native.0.as_ptr());pump();assert!(!tlauncher_previews_visible(native.0.as_ptr()));drop(native);
        drop(minimized);drop(source);drop(owner);CoUninitialize();
    }
    println!("FLUIDWIN_APP_PREVIEWS live_dwm=PASS minimized_placeholder=PASS no_activation=PASS select_close_requests=PASS destroyed_source=PASS repeated_teardown=PASS");
}
