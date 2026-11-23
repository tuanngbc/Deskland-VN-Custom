//! TLauncher draws its own context menus; a native HMENU remains the source
//! for IDs, enabled state and submenus, and is used if rendering cannot start.
use super::{glass::Glass,glass_probe::Layer,panel_chrome as chrome,surface::Palette};
use crate::desktop::wide;
use std::{cell::RefCell,mem,ptr};
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::*,System::LibraryLoader::GetModuleHandleW,
    UI::{HiDpi::GetDpiForWindow,Input::KeyboardAndMouse::*,WindowsAndMessaging::*}};

#[derive(Clone)]
struct Entry {id:u32,text:String,disabled:bool,check:bool,separator:bool,children:Vec<Entry>}
unsafe fn items(menu:HMENU,depth:usize)->Vec<Entry>{
    if depth>4{return Vec::new();}
    (0..GetMenuItemCount(menu).clamp(0,96)).filter_map(|i|{
        let mut text=[0u16;512];
        let mut mi:MENUITEMINFOW=mem::zeroed();
        mi.cbSize=mem::size_of::<MENUITEMINFOW>() as u32;
        mi.fMask=MIIM_FTYPE|MIIM_STATE|MIIM_ID|MIIM_SUBMENU|MIIM_STRING;
        mi.dwTypeData=text.as_mut_ptr();mi.cch=text.len() as u32-1;
        if GetMenuItemInfoW(menu,i as u32,1,&mut mi)==0{return None;}
        let text=String::from_utf16_lossy(&text[..(mi.cch as usize).min(text.len())]).replace("&&","&");
        Some(Entry{id:mi.wID,text,disabled:mi.fState&(MFS_DISABLED|MFS_GRAYED)!=0,check:mi.fState&MFS_CHECKED!=0,
            separator:mi.fType&MFT_SEPARATOR!=0,children:if mi.hSubMenu.is_null(){Vec::new()}else{items(mi.hSubMenu,depth+1)}})
    }).collect()
}
struct Menu{
    layer:Layer,glass:Glass,font:HFONT,dark:bool,dpi:u32,
    pages:Vec<(Vec<Entry>,usize,Option<usize>)>,hot:Option<usize>,offset:usize,pressed:Option<usize>,
    rows:usize,row:i32,pad:i32,done:bool,result:u32,
}
impl Drop for Menu{fn drop(&mut self){unsafe{if !self.font.is_null(){DeleteObject(self.font);}}}}
impl Menu{
    fn entries(&self)->&[Entry]{&self.pages.last().unwrap().0}
    fn back(&mut self){
        if self.pages.len()>1{
            self.pages.pop();let page=self.pages.last().unwrap();self.offset=page.1;self.hot=page.2;
        }else{self.done=true;}
    }
    fn choose(&mut self,i:usize){
        let Some(entry)=self.entries().get(i)else{return;};
        if entry.separator||entry.disabled{return;}
        if !entry.children.is_empty(){
            let mut children=Vec::with_capacity(entry.children.len()+1);
            children.push(Entry{id:0,text:"‹  Quay lại".into(),disabled:false,check:false,separator:false,children:Vec::new()});
            children.extend_from_slice(&entry.children);
            let page=self.pages.last_mut().unwrap();page.1=self.offset;page.2=self.hot;
            self.pages.push((children,0,None));self.offset=0;self.hot=Some(0);
        }else if self.pages.len()>1&&i==0{self.back();}
        else{self.result=entry.id;self.done=true;}
    }
    fn ensure_visible(&mut self,i:usize){
        if i<self.offset{self.offset=i;}
        if i>=self.offset+self.rows{self.offset=i+1-self.rows;}
    }
    fn step(&mut self,dir:isize){
        let n=self.entries().len();
        if n==0{return;}
        let at=self.hot.map_or(if dir>0{n-1}else{0},|i|i);
        for step in 1..=n{
            let next=(at as isize+step as isize*dir).rem_euclid(n as isize) as usize;
            let entry=&self.entries()[next];
            if !entry.separator&&!entry.disabled{
                self.hot=Some(next);self.ensure_visible(next);break;
            }
        }
    }
    fn hit(&self,x:i32,y:i32)->Option<usize>{
        let (w,h)=self.layer.size;
        if x<0||x>=w||y<self.pad||y>=h-self.pad{return None;}
        let i=self.offset+((y-self.pad)/self.row) as usize;
        self.entries().get(i).filter(|e|!e.disabled&&!e.separator).map(|_|i)
    }
    unsafe fn render(&mut self,hwnd:HWND)->bool{
        let (w,h)=self.layer.size;
        let g=self.layer.dc();
        let palette=Palette::new(self.dark);
        if !self.glass.draw(g,w,h){super::fill(g,RECT{left:0,top:0,right:w,bottom:h},chrome::color(palette.panel));}
        let old=SelectObject(g,self.font);
        let px=|n|chrome::scale(self.dpi,n);
        for (slot,item) in self.entries().iter().skip(self.offset).take(self.rows).enumerate(){
            let top=self.pad+slot as i32*self.row;
            let r=RECT{left:px(7),top,right:w-px(7),bottom:top+self.row};
            if item.separator{
                super::fill(g,RECT{left:px(15),top:top+self.row/2,right:w-px(15),bottom:top+self.row/2+1},chrome::color(palette.edge));
                continue;
            }
            if self.hot==Some(self.offset+slot){chrome::box_fill(g,r,px(11),if self.dark{[72,73,79]}else{[216,232,247]},None);}
            let flags=DT_SINGLELINE|DT_VCENTER|DT_END_ELLIPSIS;
            let ink=if item.disabled{palette.muted}else{palette.ink};
            chrome::text(g,RECT{left:px(28),right:r.right-px(22),..r},&item.text,ink,flags);
            if item.check{chrome::text(g,RECT{left:px(12),right:px(27),..r},"✓",ink,flags);}
            if !item.children.is_empty(){chrome::text(g,RECT{left:r.right-px(18),..r},"›",ink,flags);}
        }
        SelectObject(g,old);GdiFlush();
        chrome::mask_popup(self.layer.pixels(),w,h,px(16) as f32,255);
        self.layer.present(hwnd)
    }
}
unsafe extern "system" fn proc(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
    let state=GetWindowLongPtrW(hwnd,GWLP_USERDATA) as *const RefCell<Menu>;
    let Some(state)=state.as_ref()else{return DefWindowProcW(hwnd,msg,w,l);};
    let (x,y)=(l as i16 as i32,(l>>16) as i16 as i32);
    match msg{
        WM_ERASEBKGND=>1,
        WM_ACTIVATE if w&0xffff==WA_INACTIVE as usize=>{if let Ok(mut m)=state.try_borrow_mut(){m.done=true;}0},
        WM_CLOSE|WM_CANCELMODE|WM_CAPTURECHANGED=>{if let Ok(mut m)=state.try_borrow_mut(){m.done=true;}0},
        WM_PAINT=>{
            let mut ps=mem::zeroed();BeginPaint(hwnd,&mut ps);EndPaint(hwnd,&ps);
            if let Ok(mut m)=state.try_borrow_mut(){m.render(hwnd);}0
        }
        WM_MOUSEMOVE|WM_LBUTTONDOWN|WM_RBUTTONDOWN|WM_LBUTTONUP|WM_RBUTTONUP|WM_MOUSEWHEEL|WM_KEYDOWN=>{
            let Ok(mut m)=state.try_borrow_mut()else{return 0;};
            let hit=m.hit(x,y);
            match msg{
                WM_MOUSEMOVE=>if m.hot==hit{return 0;}else{m.hot=hit;},
                WM_LBUTTONDOWN|WM_RBUTTONDOWN=>{
                    let (width,height)=m.layer.size;
                    if x<0||x>=width||y<0||y>=height{m.done=true;}
                    m.pressed=hit;
                }
                WM_LBUTTONUP|WM_RBUTTONUP=>if let Some(i)=m.pressed.take().filter(|i|Some(*i)==hit){m.choose(i);},
                WM_MOUSEWHEEL=>{
                    let delta=if ((w>>16) as i16)<0{1}else{-1};
                    m.offset=m.offset.saturating_add_signed(delta).min(m.entries().len().saturating_sub(m.rows));m.hot=None;
                }
                WM_KEYDOWN=>match w as u16{
                    VK_ESCAPE=>m.done=true,VK_LEFT=>m.back(),VK_UP=>m.step(-1),VK_DOWN=>m.step(1),
                    VK_RETURN|VK_SPACE|VK_RIGHT=>if let Some(i)=m.hot{m.choose(i);},
                    VK_HOME|VK_END=>{m.hot=None;m.step(if w as u16==VK_HOME{1}else{-1});}
                    _=>{}
                }
                _=>{}
            }
            if !m.done{InvalidateRect(hwnd,ptr::null(),0);}0
        }
        _=>DefWindowProcW(hwnd,msg,w,l),
    }
}

pub(crate) unsafe fn track(menu:HMENU,owner:HWND,point:POINT,flags:u32,dark:bool,transparency:u32)->u32{
    if let Some(result)=show(menu,owner,point,flags,dark,transparency){return result;}
    TrackPopupMenu(menu,flags,point.x,point.y,0,owner,ptr::null()) as u32
}
unsafe fn show(menu:HMENU,owner:HWND,point:POINT,flags:u32,dark:bool,transparency:u32)->Option<u32>{
    let entries=items(menu,0);
    if entries.is_empty(){return None;}
    let dpi=GetDpiForWindow(owner).clamp(72,288);
    let px=|n|chrome::scale(dpi,n);
    let anchor=chrome::Anchor{point,side:super::DockSide::Bottom,dark,transparency};
    let mut mi:MONITORINFO=mem::zeroed();mi.cbSize=mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(MonitorFromPoint(point,MONITOR_DEFAULTTONEAREST),&mut mi)==0{return None;}
    let area=mi.rcMonitor;
    let row=px(27);let pad=px(8);
    let rows=((area.bottom-area.top-pad*2-px(8))/row).clamp(1,22) as usize;
    let children=entries.iter().map(|e|e.children.len()+1).max().unwrap_or(1);
    let rows=entries.len().max(children).min(rows);
    let longest=entries.iter().chain(entries.iter().flat_map(|e|e.children.iter())).map(|e|e.text.chars().count()).max().unwrap_or(16);
    let w=px((54+longest as i32*7).clamp(220,490)).min(area.right-area.left);
    let h=pad*2+rows as i32*row;
    let x=(if flags&TPM_RIGHTALIGN!=0{point.x-w}else{point.x}).clamp(area.left,(area.right-w).max(area.left));
    let prefer_above=flags&TPM_BOTTOMALIGN!=0||point.y+h>area.bottom;
    let y=(if prefer_above{point.y-h}else{point.y}).clamp(area.top,(area.bottom-h).max(area.top));
    let rect=RECT{left:x,top:y,right:x+w,bottom:y+h};
    let glass=Glass::capture(rect,dpi,dark,transparency);
    let layer=Layer::new(w,h)?;
    let class=wide("TLauncherGlassMenu");
    if !chrome::register_with_style(&class,Some(proc),0){return None;}
    let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_TOPMOST|WS_EX_LAYERED,class.as_ptr(),wide("Menu TLauncher").as_ptr(),
        WS_POPUP,x,y,w,h,owner,ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
    if hwnd.is_null(){return None;}
    chrome::disable_system_border(hwnd);
    let model=RefCell::new(Menu{layer,glass,font:chrome::choose_font(dpi,14,480),dark,dpi,pages:vec![(entries,0,None)],
        hot:None,offset:0,pressed:None,rows,row,pad,done:false,result:0});
    let ready=model.borrow_mut().render(hwnd);
    if !ready{DestroyWindow(hwnd);return None;}
    SetWindowLongPtrW(hwnd,GWLP_USERDATA,&model as *const _ as isize);
    ShowWindow(hwnd,SW_SHOW);
    SetForegroundWindow(hwnd);
    if GetForegroundWindow()!=hwnd{
        SetWindowLongPtrW(hwnd,GWLP_USERDATA,0);DestroyWindow(hwnd);return None;
    }
    SetFocus(hwnd);SetCapture(hwnd);
    let mut shadow=None;
    chrome::PanelShadow::sync(&mut shadow,hwnd,owner,rect,dpi,if prefer_above{anchor.side}else{super::DockSide::Right},dark);
    let mut msg:MSG=mem::zeroed();
    while !model.borrow().done&&IsWindow(hwnd)!=0{
        let running=GetMessageW(&mut msg,ptr::null_mut(),0,0);
        if running<=0{if running==0{PostQuitMessage(msg.wParam as i32);}break;}
        TranslateMessage(&msg);DispatchMessageW(&msg);
    }
    let foreground=GetForegroundWindow();
    if GetCapture()==hwnd{ReleaseCapture();}
    drop(shadow);
    if IsWindow(hwnd)!=0{SetWindowLongPtrW(hwnd,GWLP_USERDATA,0);DestroyWindow(hwnd);}
    if foreground==hwnd&&IsWindow(owner)!=0{SetForegroundWindow(owner);}
    let result=model.borrow().result;
    Some(result)
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn glass_menu_preserves_command_ids_checks_and_submenu_state(){
        unsafe{
            let menu=CreatePopupMenu();let child=CreatePopupMenu();
            assert!(!menu.is_null()&&!child.is_null());
            assert_ne!(AppendMenuW(child,MF_STRING|MF_CHECKED,27,wide("Ready && enabled").as_ptr()),0);
            assert_ne!(AppendMenuW(menu,MF_STRING|MF_GRAYED,12,wide("Disabled").as_ptr()),0);
            assert_ne!(AppendMenuW(menu,MF_SEPARATOR,0,ptr::null()),0);
            assert_ne!(AppendMenuW(menu,MF_POPUP,child as usize,wide("Displays").as_ptr()),0);
            let parsed=items(menu,0);
            DestroyMenu(menu);
            assert_eq!(parsed.len(),3);
            assert!(parsed[0].disabled);
            assert_eq!(parsed[0].id,12);
            assert!(parsed[1].separator);
            assert_eq!(parsed[2].text,"Displays");
            assert_eq!(parsed[2].children[0].id,27);
            assert_eq!(parsed[2].children[0].text,"Ready & enabled");
            assert!(parsed[2].children[0].check);
        }
    }
}
