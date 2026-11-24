//! TLauncher draws its own context menus; a native HMENU remains the source
//! for IDs, enabled state and submenus, and is used if rendering cannot start.
use super::{motion::{self,Tween},glass::Glass,glass_probe::Layer,panel_chrome as chrome,surface::Palette};
use crate::desktop::wide;
use std::{cell::{Cell,RefCell},mem,ptr};
const FRAME_TIMER:usize=8;
thread_local!{static TRACKING:Cell<bool>=const{Cell::new(false)};}
struct TrackScope;
impl Drop for TrackScope{fn drop(&mut self){TRACKING.set(false);}}
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
    layer:Layer,scaled:Option<Layer>,base:Vec<u8>,glyphs:Layer,font:HFONT,dark:bool,dpi:u32,
    tween:Option<Tween>,pivot:(f32,f32),hover_from:Option<usize>,hover_at:Option<motion::Span>,shadow:Option<chrome::PanelShadow>,
    pages:Vec<(Vec<Entry>,usize,Option<usize>)>,hot:Option<usize>,offset:usize,pressed:Option<usize>,
    rows:usize,row:i32,pad:i32,done:bool,result:u32,
}
impl Drop for Menu{fn drop(&mut self){unsafe{if !self.font.is_null(){DeleteObject(self.font);}}}}
fn composite_glyphs(dst:&mut[u8],mask:&[u8],ink:[u8;3]){
    for (p,m) in dst.chunks_exact_mut(4).zip(mask.chunks_exact(4)){
        let a=(u32::from(m[0])+u32::from(m[1])+u32::from(m[2])+1)/3;
        for c in 0..3{p[c]=((u32::from(ink[c])*a+u32::from(p[c])*(255-a)+127)/255) as u8;}
        p[3]=255;
    }
}
impl Menu{
    unsafe fn prepare_open(&mut self,hwnd:HWND)->bool{
        self.tween=Some(Tween::new(true,motion::MENU,None).prepared());
        self.present(hwnd)
    }
    fn begin_close(&mut self){self.tween=Some(Tween::new(false,motion::MENU,self.tween));}
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
        let (x,y)=self.tween.filter(|_|self.scaled.is_some()).map_or((x,y),|t|{
            let z=motion::zoom(motion::MENU.scale,t.openness());
            (motion::inverse(x as f32,self.pivot.0,z).floor() as i32,
             motion::inverse(y as f32,self.pivot.1,z).floor() as i32)
        });
        let (w,h)=self.layer.size;
        if x<0||x>=w||y<self.pad||y>=h-self.pad{return None;}
        let i=self.offset+((y-self.pad)/self.row) as usize;
        self.entries().get(i).filter(|e|!e.disabled&&!e.separator).map(|_|i)
    }
    unsafe fn present(&mut self,hwnd:HWND)->bool{
        let Some(t)=self.tween else{
            if let Some(shadow)=&self.shadow{shadow.opacity(255);}
            return self.layer.present(hwnd);
        };
        let k=t.openness();
        let opacity=(255.0*k).round().clamp(1.0,255.0) as u8;
        if let Some(shadow)=&self.shadow{shadow.opacity(opacity);}
        let Some(scaled)=self.scaled.as_mut()else{return self.layer.present_at(hwnd,None,opacity);};
        let z=motion::zoom(motion::MENU.scale,k);
        let (w,h)=self.layer.size;
        let anchor=|p:f32|(p*(1.0-z)).round() as i32;
        let (x,y)=(anchor(self.pivot.0),anchor(self.pivot.1));
        GdiFlush();
        scaled.pixels().fill(0);
        let blend=BLENDFUNCTION{BlendOp:AC_SRC_OVER as u8,BlendFlags:0,SourceConstantAlpha:255,AlphaFormat:AC_SRC_ALPHA as u8};
        if AlphaBlend(scaled.dc(),x,y,(w as f32*z).round().max(1.0) as i32,(h as f32*z).round().max(1.0) as i32,
            self.layer.dc(),0,0,w,h,blend)==0{return self.layer.present_at(hwnd,None,opacity);}
        GdiFlush();
        scaled.present_at(hwnd,None,opacity)
    }
    unsafe fn render(&mut self,hwnd:HWND)->bool{
        let (w,h)=self.layer.size;
        let g=self.layer.dc();
        let palette=Palette::new(self.dark);
        GdiFlush();
        self.layer.pixels().copy_from_slice(&self.base);
        let old=SelectObject(self.glyphs.dc(),self.font);
        let px=|n|chrome::scale(self.dpi,n);
        let visible=self.entries().iter().skip(self.offset).take(self.rows).map(|e|
            (e.text.clone(),e.separator,e.disabled,e.check,!e.children.is_empty())).collect::<Vec<_>>();
        for (slot,(label,separator,disabled,checked,submenu)) in visible.into_iter().enumerate(){
            let top=self.pad+slot as i32*self.row;
            let r=RECT{left:px(7),top,right:w-px(7),bottom:top+self.row};
            if separator{
                super::fill(g,RECT{left:px(15),top:top+self.row/2,right:w-px(15),bottom:top+self.row/2+1},chrome::color(palette.edge));
                continue;
            }
            let index=self.offset+slot;
            let phase=self.hover_at.map_or(1.0,|t|motion::ease(t.progress()));
            let lift=if self.hot==Some(index){phase}else if self.hover_from==Some(index){1.0-phase}else{0.0};
            if lift>0.0{self.layer.shape(r,px(10) as f32,if self.dark{[83,84,90]}else{[221,235,248]},(215.0*lift).round() as u8);}
            let flags=DT_SINGLELINE|DT_VCENTER|DT_END_ELLIPSIS;
            let ink=if disabled{palette.muted}else{palette.ink};
            // DirectWrite's alpha-ignored target is confined to a black mask.
            // Never mix its output or a GDI-written alpha byte into the glass.
            GdiFlush();
            let stride=w as usize*4;
            let extent=top as usize*stride..(top+self.row) as usize*stride;
            self.glyphs.pixels()[extent.clone()].fill(0);
            let mask=self.glyphs.dc();
            chrome::text(mask,RECT{left:px(28),right:r.right-px(22),..r},&label,[255;3],flags);
            if checked{chrome::text(mask,RECT{left:px(12),right:px(27),..r},"✓",[255;3],flags);}
            if submenu{chrome::text(mask,RECT{left:r.right-px(18),..r},"›",[255;3],flags);}
            GdiFlush();
            composite_glyphs(&mut self.layer.pixels()[extent.clone()],&self.glyphs.pixels()[extent],ink);
        }
        SelectObject(self.glyphs.dc(),old);GdiFlush();
        chrome::mask_popup(self.layer.pixels(),w,h,px(16) as f32,255);
        self.present(hwnd)
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
        WM_TIMER if w==FRAME_TIMER=>{
            if let Ok(mut m)=state.try_borrow_mut(){
                if m.tween.is_some_and(|t|(t.progress()>=1.0||!motion::allowed())&&t.to>0.0){m.tween=None;}
                let hovering=m.hover_at.is_some();
                if m.hover_at.is_some_and(|t|t.progress()>=1.0||!motion::allowed()){
                    m.hover_at=None;m.hover_from=None;
                }
                if hovering{m.render(hwnd);}else{m.present(hwnd);}
                if m.tween.is_none()&&m.hover_at.is_none(){KillTimer(hwnd,FRAME_TIMER);}
            }0
        }
        WM_MOUSEMOVE|WM_LBUTTONDOWN|WM_RBUTTONDOWN|WM_LBUTTONUP|WM_RBUTTONUP|WM_MOUSEWHEEL|WM_KEYDOWN=>{
            let Ok(mut m)=state.try_borrow_mut()else{return 0;};
            if m.done{return 0;}
            let hit=m.hit(x,y);
            let previous=m.hot;
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
            if previous!=m.hot&&motion::allowed(){
                m.hover_from=previous;m.hover_at=Some(motion::Span::hover());SetTimer(hwnd,FRAME_TIMER,16,None);
            }
            if !m.done{InvalidateRect(hwnd,ptr::null(),0);}0
        }
        _=>DefWindowProcW(hwnd,msg,w,l),
    }
}

pub(crate) unsafe fn track(menu:HMENU,owner:HWND,point:POINT,flags:u32,dark:bool,transparency:u32)->u32{
    if TRACKING.replace(true){return 0;}
    let _scope=TrackScope;
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
    let glyphs=Layer::new(w,h)?;
    let base=glass.menu_base(w,h,Palette::new(dark).panel);
    let class=wide("TLauncherGlassMenu");
    if !chrome::register_with_style(&class,Some(proc),0){return None;}
    let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_TOPMOST|WS_EX_LAYERED,class.as_ptr(),wide("Menu TLauncher").as_ptr(),
        WS_POPUP,x,y,w,h,owner,ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
    if hwnd.is_null(){return None;}
    chrome::disable_system_border(hwnd);
    let animate=super::popup_motion_allowed()&&SetTimer(hwnd,FRAME_TIMER,16,None)!=0;
    let model=RefCell::new(Menu{layer,scaled:if animate{Layer::new(w,h)}else{None},
        base,glyphs,font:chrome::choose_font(dpi,14,500),dark,dpi,pages:vec![(entries,0,None)],
        tween:None,pivot:((point.x-x).clamp(0,w) as f32,(point.y-y).clamp(0,h) as f32),
        hover_from:None,hover_at:None,shadow:None,
        hot:None,offset:0,pressed:None,rows,row,pad,done:false,result:0});
    let ready=model.borrow_mut().render(hwnd);
    if !ready{DestroyWindow(hwnd);return None;}
    if animate{
        if !model.borrow_mut().prepare_open(hwnd){DestroyWindow(hwnd);return None;}
    }
    SetWindowLongPtrW(hwnd,GWLP_USERDATA,&model as *const _ as isize);
    ShowWindow(hwnd,SW_SHOW);
    SetForegroundWindow(hwnd);
    if GetForegroundWindow()!=hwnd{
        SetWindowLongPtrW(hwnd,GWLP_USERDATA,0);DestroyWindow(hwnd);return None;
    }
    SetFocus(hwnd);SetCapture(hwnd);
    let mut shadow=None;
    chrome::PanelShadow::sync(&mut shadow,hwnd,owner,rect,dpi,if prefer_above{anchor.side}else{super::DockSide::Right},dark);
    {
        let mut m=model.borrow_mut();m.shadow=shadow.take();
        if let Some(t)=m.tween{let _=m.present(hwnd);m.tween=Some(t.restart_clock());}
    }
    let mut msg:MSG=mem::zeroed();
    while !model.borrow().done&&IsWindow(hwnd)!=0{
        let running=GetMessageW(&mut msg,ptr::null_mut(),0,0);
        if running<=0{if running==0{PostQuitMessage(msg.wParam as i32);}break;}
        TranslateMessage(&msg);DispatchMessageW(&msg);
    }
    let foreground=GetForegroundWindow();
    if GetCapture()==hwnd{ReleaseCapture();}
    let mut restore=foreground==hwnd;
    if restore&&IsWindow(hwnd)!=0&&motion::allowed()&&model.borrow().result==0&&IsWindow(owner)!=0{
        model.borrow_mut().begin_close();
        if SetTimer(hwnd,FRAME_TIMER,16,None)!=0{
            while motion::allowed()&&IsWindow(hwnd)!=0&&GetForegroundWindow()==hwnd&&model.borrow().tween.is_some_and(|t|t.progress()<1.0){
                let running=GetMessageW(&mut msg,ptr::null_mut(),0,0);
                if running<=0{if running==0{PostQuitMessage(msg.wParam as i32);}break;}
                TranslateMessage(&msg);DispatchMessageW(&msg);
            }
        }
        restore=GetForegroundWindow()==hwnd;
    }
    KillTimer(hwnd,FRAME_TIMER);
    drop(model.borrow_mut().shadow.take());
    if IsWindow(hwnd)!=0{SetWindowLongPtrW(hwnd,GWLP_USERDATA,0);DestroyWindow(hwnd);}
    if restore&&IsWindow(owner)!=0{SetForegroundWindow(owner);}
    let result=model.borrow().result;
    Some(result)
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    #[ignore="native layered menu fixture on GitHub Actions"]
    fn native_menu_prepares_frame_and_uses_all_six_speeds(){
        unsafe{
            let name=wide("STATIC");
            let hwnd=CreateWindowExW(WS_EX_LAYERED|WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW,name.as_ptr(),ptr::null(),
                WS_POPUP,30,30,220,80,ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
            assert!(!hwnd.is_null());
            for speed in [25,50,70,75,100,150,200]{
                motion::native_fixture(speed,||{
                    let mut m=Menu{layer:Layer::new(220,80).unwrap(),scaled:Layer::new(220,80),
                        base:Glass::capture(RECT{left:30,top:30,right:250,bottom:110},96,false,30).menu_base(220,80,Palette::new(false).panel),
                        glyphs:Layer::new(220,80).unwrap(),
                        font:chrome::choose_font(96,14,500),dark:false,dpi:96,pages:vec![(vec![Entry{id:1,text:"Open".into(),
                            disabled:false,check:false,separator:false,children:vec![]}],0,None)],
                        tween:None,pivot:(0.0,0.0),hover_from:None,hover_at:None,shadow:None,
                        hot:None,offset:0,pressed:None,rows:1,row:32,pad:8,done:false,result:0};
                    assert!(m.render(hwnd));
                    let clean=m.layer.pixels().to_vec();
                    assert!(clean.chunks_exact(4).all(|p|p[..3].iter().all(|c|*c<=p[3])));
                    m.layer.pixels().fill(0xe7);m.glyphs.pixels().fill(0xb5);
                    assert!(m.render(hwnd));assert_eq!(m.layer.pixels(),clean.as_slice(),"repaint must overwrite all stale pixels");
                    if speed==70{
                        m.dark=true;
                        m.base=Glass::capture(RECT{left:30,top:30,right:250,bottom:110},96,true,30).menu_base(220,80,Palette::new(true).panel);
                        m.hot=Some(0);
                        assert!(m.render(hwnd));
                        assert!(m.layer.pixels().chunks_exact(4).all(|p|p[..3].iter().all(|c|*c<=p[3])));
                    }
                    assert!(m.prepare_open(hwnd));
                    let first=m.tween.unwrap();
                    assert_eq!(first.openness(),0.0);
                    assert_eq!(first.duration,(18000.0/speed as f64).round() as u32);
                    m.tween=Some(first.restart_clock());
                    assert!(m.hit(20,16).is_some());
                    m.begin_close();
                    let close=m.tween.unwrap();
                    assert_eq!(close.duration,(12000.0/speed as f64).round() as u32);
                    m.tween=Some(close.finish());
                    assert!(m.present(hwnd));assert_eq!(m.tween.unwrap().openness(),0.0);
                });
            }
            DestroyWindow(hwnd);
            println!("MOTION_SPEED dock_menu_native=PASS six_speeds=PASS prepared_frame=PASS closing_endpoint=PASS");
        }
    }
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
