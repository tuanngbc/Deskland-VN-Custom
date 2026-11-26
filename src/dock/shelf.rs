//! Original notch.h File Shelf semantics in the existing native Control Center.
//! Only paths are retained for this unlocked Dock session; no file storage.
use windows_sys::Win32::Foundation::HWND;
pub(super) const CHANGED:u32=windows_sys::Win32::UI::WindowsAndMessaging::WM_APP+147;
#[derive(Clone,Copy)]pub(super) enum Action{Open,Add,Item(u64),Forget(Option<u64>),Drag(Option<u64>),Copy(Option<u64>)}
#[cfg(target_env="msvc")]pub(super) use native::*;
#[cfg(not(target_env="msvc"))]pub(super) fn modal()->bool{false}
#[cfg(not(target_env="msvc"))]pub(super) fn reset(){}
#[cfg(not(target_env="msvc"))]pub(super) fn detach(_:HWND){}
#[cfg(not(target_env="msvc"))]pub(super) unsafe fn changed(_:HWND){}
#[cfg(not(target_env="msvc"))]pub(super) unsafe fn perform(_:HWND,_:HWND,_:Action){}
#[cfg(not(target_env="msvc"))]pub(super) fn drag_command(_: &super::control_center::Command)->Option<super::control_center::Command>{None}

#[cfg(target_env="msvc")]mod native{
    use super::*;
    use std::{cell::{Cell,RefCell},ffi::c_void,ptr::{self,NonNull},path::PathBuf};
    use windows_sys::Win32::{UI::{Shell::ShellExecuteW,WindowsAndMessaging::*,Input::KeyboardAndMouse::GetKeyState},Foundation::*};
    use crate::desktop::wide;
    use super::super::{control_center::{self,Card,Command,View},STATE};
    extern "C"{
        fn tlauncher_shelf_create()->*mut c_void;
        fn tlauncher_shelf_free(raw:*mut c_void);
        fn tlauncher_shelf_count(raw:*mut c_void)->u32;
        fn tlauncher_shelf_item(raw:*mut c_void,index:u32,path:*mut u16,capacity:u32)->u64;
        fn tlauncher_shelf_remove(raw:*mut c_void,id:u64);
        fn tlauncher_shelf_register(raw:*mut c_void,window:HWND,notice:u32)->*mut c_void;
        fn tlauncher_shelf_unregister(raw:*mut c_void);
        fn tlauncher_shelf_add_dialog(raw:*mut c_void,owner:HWND,title:*const u16)->i32;
        fn tlauncher_shelf_drag(raw:*mut c_void,id:u64,read_only:bool)->i32;
        fn tlauncher_shelf_copy(raw:*mut c_void,id:u64,read_only:bool)->i32;
        #[cfg(test)]fn tlauncher_shelf_fixture(raw:*mut c_void,registration:*mut c_void,path:*const u16)->i32;
        #[cfg(test)]fn tlauncher_shelf_disconnected_fixture(registration:*mut c_void,path:*const u16)->i32;
    }
    struct Native(NonNull<c_void>);impl Drop for Native{fn drop(&mut self){unsafe{tlauncher_shelf_free(self.0.as_ptr());}}}
    struct Registration{owner:HWND,popup:HWND,raw:NonNull<c_void>}
    impl Drop for Registration{fn drop(&mut self){unsafe{tlauncher_shelf_unregister(self.raw.as_ptr());}}}
    thread_local!{
        static MODEL:RefCell<Option<Native>>=const{RefCell::new(None)};
        static TARGET:RefCell<Option<Registration>>=const{RefCell::new(None)};
        static MODAL:Cell<bool>=const{Cell::new(false)};
        static ERROR:Cell<Option<i32>>=const{Cell::new(None)};
    }
    pub(crate) fn modal()->bool{MODAL.get()}
    struct Modal(bool);impl Modal{fn enter()->Self{Self(MODAL.replace(true))}}impl Drop for Modal{fn drop(&mut self){MODAL.set(self.0);}}
    fn text(vi:&str,en:&str,zh:&str)->String{crate::launcher::localized(vi,en,zh).into()}
    fn title()->String{text("Kệ tệp","File Shelf","文件暂存架")}
    fn raw()->*mut c_void{MODEL.with(|s|s.borrow().as_ref().map_or(ptr::null_mut(),|m|m.0.as_ptr()))}
    fn enabled()->bool{STATE.with(|s|{let Ok(s)=s.try_borrow()else{return false;};s.prefs.enabled&&!s.suspended})}
    fn items()->Vec<(u64,PathBuf)>{unsafe{
        let model=raw();if model.is_null(){return Vec::new();}
        let mut result=Vec::new();let mut path=vec![0u16;32768];
        for i in 0..tlauncher_shelf_count(model).min(200){
            let id=tlauncher_shelf_item(model,i,path.as_mut_ptr(),path.len() as u32);
            if id!=0{let length=path.iter().position(|c|*c==0).unwrap_or(path.len());result.push((id,PathBuf::from(String::from_utf16_lossy(&path[..length]))));}
        }result
    }}
    fn view()->View{
        let items=items();let read_only=crate::trial::active()||cfg!(test);
        let hint=if ERROR.get().is_some(){text("Chưa hoàn tất yêu cầu. Hãy kiểm tra tệp nguồn và chỗ trống trên kệ.","Request not completed. Check the source files and shelf capacity.","请求未完成，请检查源文件及暂存架容量。")}
            else{text("Thả tệp vào đây · Kéo một mục ra khi cần","Drop files here · Drag an item out to use it","将文件拖入此处 · 需要时拖出一项")};
        let mut cards=vec![Card{icon:None,title:text("Thêm tệp…","Add files…","添加文件…"),hint:String::new(),action:Some(Command::Shelf(Action::Add))}];
        cards.push(Card{icon:None,title:text("Kéo tất cả ra","Drag all out","全部拖出"),hint:text("Hoặc nhấn Enter để sao chép","Or press Enter to copy","或按 Enter 复制"),action:(!items.is_empty()&&!read_only).then_some(Command::Shelf(Action::Drag(None)))});
        cards.push(Card{icon:None,title:text("Dọn kệ","Clear shelf","清空暂存架"),hint:text("Không xóa tệp gốc","Keeps the original files","不会删除原文件"),action:(!items.is_empty()).then_some(Command::Shelf(Action::Forget(None)))});
        for (id,path) in items{
            cards.push(Card{icon:None,title:path.file_name().unwrap_or(path.as_os_str()).to_string_lossy().into(),
                hint:path.to_string_lossy().into(),action:Some(Command::Shelf(Action::Item(id)))});
        }
        View{title:format!("{} · {}",title(),cards.len()-3),hint,cards,metrics:None}
    }
    unsafe fn open(owner:HWND){
        if !enabled()||IsWindow(owner)==0{return;}
        if raw().is_null(){MODEL.with(|s|s.replace(NonNull::new(tlauncher_shelf_create()).map(Native)));if raw().is_null(){ERROR.set(Some(-101));}}
        control_center::show_shelf(owner,super::super::dock_app_anchor(owner,None),view());
        if let Some(popup)=control_center::window(owner){
            if let Some(raw)=NonNull::new(tlauncher_shelf_register(raw(),popup,CHANGED)){
                TARGET.with(|s|s.replace(Some(Registration{owner,popup,raw})));
            }else{ERROR.set(Some(-100));if let Some(tag)=control_center::ticket(owner){control_center::update(owner,tag,view());}}
        }
    }
    pub(crate) fn detach(popup:HWND){
        let old=TARGET.with(|s|{let mut s=s.borrow_mut();if s.as_ref().is_some_and(|t|t.popup==popup){s.take()}else{None}});drop(old);
    }
    pub(crate) fn reset(){
        let old=MODEL.with(|s|s.borrow_mut().take());drop(old);
        let old=TARGET.with(|s|s.borrow_mut().take());drop(old);ERROR.set(None);
    }
    pub(crate) unsafe fn changed(popup:HWND){
        let owner=TARGET.with(|s|s.borrow().as_ref().filter(|t|t.popup==popup).map(|t|t.owner));
        if let Some(owner)=owner{if enabled(){ERROR.set(None);open(owner);}}
    }
    pub(crate) fn drag_command(command:&Command)->Option<Command>{
        if crate::trial::active()||cfg!(test){return None;}
        match command{Command::Shelf(Action::Item(id))=>Some(Command::Shelf(Action::Drag(Some(*id)))),
            Command::Shelf(Action::Drag(id))=>Some(Command::Shelf(Action::Drag(*id))),_=>None}
    }
    pub(crate) unsafe fn perform(owner:HWND,popup:HWND,action:Action){
        if !enabled(){return;}if matches!(action,Action::Open){ERROR.set(None);open(owner);return;}
        if raw().is_null(){return;}
        let _modal=Modal::enter();
        let mut rebuild=true;
        match action{
            Action::Open=>{},
            Action::Add=>{let result=tlauncher_shelf_add_dialog(raw(),popup,wide(&title()).as_ptr());ERROR.set((result<0).then_some(result));},
            Action::Forget(id)=>{tlauncher_shelf_remove(raw(),id.unwrap_or(0));ERROR.set(None);},
            Action::Drag(id)=>{
                let read_only=crate::trial::active()||cfg!(test);
                let result=if GetKeyState(windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_LBUTTON as i32)<0{
                    tlauncher_shelf_drag(raw(),id.unwrap_or(0),read_only)
                }else{tlauncher_shelf_copy(raw(),id.unwrap_or(0),read_only)};
                ERROR.set((result<0).then_some(result));rebuild=false;
                if GetForegroundWindow()!=popup{control_center::close_for(owner);}
                else if let Some(tag)=control_center::ticket(owner){control_center::update(owner,tag,view());}
            },
            Action::Copy(id)=>{let result=tlauncher_shelf_copy(raw(),id.unwrap_or(0),crate::trial::active()||cfg!(test));ERROR.set((result<0).then_some(result));},
            Action::Item(id)=>{
                let Some((_,path))=items().into_iter().find(|(key,_)|*key==id)else{return;};
                let menu=CreatePopupMenu();if menu.is_null(){return;}
                let read_only=crate::trial::active()||cfg!(test);
                for (id,label) in [(1,text("Mở","Open","打开")),(2,text("Sao chép","Copy","复制")),(3,text("Bỏ khỏi kệ","Remove from shelf","从暂存架移除"))]{
                    AppendMenuW(menu,MF_STRING|if read_only&&id!=3{MF_GRAYED}else{0},id,wide(&label).as_ptr());
                }
                let mut point=POINT{x:0,y:0};GetCursorPos(&mut point);
                let command=TrackPopupMenuEx(menu,TPM_RETURNCMD|TPM_RIGHTBUTTON,point.x,point.y,popup,ptr::null()) as u32;DestroyMenu(menu);
                match command{
                    1 if !read_only=>{ShellExecuteW(popup,wide("open").as_ptr(),wide(&path.to_string_lossy()).as_ptr(),ptr::null(),ptr::null(),SW_SHOWNORMAL);rebuild=false;control_center::close_for(owner);},
                    2 if !read_only=>{perform(owner,popup,Action::Copy(Some(id)));return;},
                    3=>tlauncher_shelf_remove(raw(),id),_=>{rebuild=false;}
                }
            }
        }
        if rebuild&&enabled()&&IsWindow(popup)!=0{open(owner);}
    }
    #[cfg(test)]mod tests{
        use super::*;
        #[test]#[ignore="hosted real Shell data-object/drop target reference semantics; no file operations or drag injection"]
        fn native_shelf_holds_references_and_releases_drop_registration(){
            if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
            unsafe{
                use windows::Win32::System::Ole::{OleInitialize,OleUninitialize};
                OleInitialize(None).unwrap();
                let old_enabled=STATE.with(|s|{let mut s=s.borrow_mut();let old=s.prefs.enabled;s.prefs.enabled=true;old});
                let path=PathBuf::from(std::env::var_os("RUNNER_TEMP").unwrap()).join(format!("shelf-fixture-tệp-{}.txt",std::process::id()));
                std::fs::write(&path,"File Shelf fixture; never opened or copied").unwrap();
                for pass in 0..4{
                    let window=CreateWindowExW(WS_EX_TOOLWINDOW,wide("STATIC").as_ptr(),wide("Shelf fixture").as_ptr(),WS_POPUP,100,100,320,200,ptr::null_mut(),ptr::null_mut(),ptr::null_mut(),ptr::null());assert!(!window.is_null());ShowWindow(window,SW_SHOWNOACTIVATE);
                    let model=Native(NonNull::new(tlauncher_shelf_create()).unwrap());let target=NonNull::new(tlauncher_shelf_register(model.0.as_ptr(),window,CHANGED)).unwrap();
                    assert_eq!(tlauncher_shelf_fixture(model.0.as_ptr(),target.as_ptr(),wide(&path.to_string_lossy()).as_ptr()),0);
                    assert_eq!(tlauncher_shelf_count(model.0.as_ptr()),1);assert_eq!(tlauncher_shelf_drag(model.0.as_ptr(),0,true),-1);
                    let mut stored=vec![0u16;32768];assert_ne!(tlauncher_shelf_item(model.0.as_ptr(),0,stored.as_mut_ptr(),stored.len() as u32),0);
                    assert_eq!(String::from_utf16_lossy(&stored[..stored.iter().position(|c|*c==0).unwrap()]),path.to_string_lossy());
                    assert_eq!(std::fs::read_to_string(&path).unwrap(),"File Shelf fixture; never opened or copied");
                    MODEL.with(|s|s.replace(Some(model)));open(window);
                    let popup=control_center::window(window).expect("real Shelf popup");assert_ne!(IsWindowVisible(popup),0);
                    assert!(view().cards.iter().any(|c|c.title==path.file_name().unwrap().to_string_lossy()));
                    assert!(TARGET.with(|s|s.borrow().as_ref().is_some_and(|t|t.popup==popup)));
                    if pass==0{
                        use windows_sys::Win32::Graphics::{Dwm::DwmFlush,Gdi::*};
                        let started=std::time::Instant::now();while started.elapsed()<std::time::Duration::from_millis(400){
                            let mut message:MSG=std::mem::zeroed();while PeekMessageW(&mut message,ptr::null_mut(),0,0,PM_REMOVE)!=0{TranslateMessage(&message);DispatchMessageW(&message);}std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        DwmFlush();let mut bounds:RECT=std::mem::zeroed();assert_ne!(GetWindowRect(popup,&mut bounds),0);
                        let (width,height)=(bounds.right-bounds.left,bounds.bottom-bounds.top);let mut layer=super::super::super::glass_probe::Layer::new(width,height).unwrap();
                        let dc=GetDC(ptr::null_mut());assert_ne!(BitBlt(layer.dc(),0,0,width,height,dc,bounds.left,bounds.top,SRCCOPY|CAPTUREBLT),0);ReleaseDC(ptr::null_mut(),dc);GdiFlush();
                        let rgba=layer.pixels().chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],255]).collect::<Vec<_>>();assert!(rgba.chunks_exact(4).any(|p|p[..3]!=rgba[..3]));
                        if let Some(dir)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){image::save_buffer(PathBuf::from(dir).join("file-shelf-panel.png"),&rgba,width as u32,height as u32,image::ColorType::Rgba8).unwrap();}
                    }
                    control_center::close_for(window);assert_eq!(IsWindow(popup),0);assert!(TARGET.with(|s|s.borrow().is_none()));
                    reset(); // outstanding synthetic target becomes inert before revoke
                    assert_eq!(tlauncher_shelf_disconnected_fixture(target.as_ptr(),wide(&path.to_string_lossy()).as_ptr()),0);
                    tlauncher_shelf_unregister(target.as_ptr());DestroyWindow(window);
                }
                STATE.with(|s|s.borrow_mut().prefs.enabled=old_enabled);std::fs::remove_file(path).unwrap();OleUninitialize();
            }
            println!("FLUIDWIN_SHELF real_Shell_data=PASS reference_only=PASS duplicate_and_capacity=PASS cancel_preserves=PASS accepted_drop_removes=PASS self_drop=REFUSED read_only=PASS repeated_revoke=PASS real_drag=NOT_INJECTED");
        }
    }
}
