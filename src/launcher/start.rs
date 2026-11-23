//! Dock Start has a separate window, layout and launch history. Only the
//! app catalog, Shell launching and icon extraction are shared with Spotlight.
use super::*;
use std::{collections::{HashMap,HashSet},ptr,sync::mpsc::{self,Receiver,Sender},time::{Duration,Instant}};

const CLASS:&str="TuanpcDeskStartPanel";
const WIDTH:i32=608;
const HEIGHT:i32=530;
const COLS:usize=6;
const GRID_PAGE:usize=18;
const LIST_PAGE:usize=16;
const EDIT_ID:usize=101;
const APPLY:u32=WM_APP+115;
const POLL:usize=1;
const SETTINGS:&str="deskland:settings";

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum Area { Home, Apps }
#[derive(Clone,Debug,PartialEq,Eq)]
enum Hit { Home, Apps, Settings, Power, Page(i32), Explorer, Windows, TaskManager, Entry(usize) }
#[derive(Clone)]
struct Tile { rect:RECT, hit:Hit, text:String, entry:Option<Entry>, compact:bool }
struct Model {
    hwnd:HWND, edit:HWND, owner:HWND, anchor:DockAnchor, scale:f32, dark:bool, area:Area,
    all:Vec<Entry>, pins:Vec<String>, recent:History, query:String, page:usize, selected:usize,
    pressed:Option<(Hit,i32,i32)>, error:String, glass:Vec<u8>, edit_font:HFONT,
    icons:HashMap<(PathBuf,i32),Option<IconPixels>>, loading:HashSet<(PathBuf,i32)>,
    icon_tx:Sender<(PathBuf,i32)>,icon_rx:Receiver<((PathBuf,i32),Option<IconPixels>)>,
    catalog:Option<Receiver<Vec<Entry>>>, indexed_at:Option<Instant>,
}
thread_local!{
    static START:RefCell<Option<Model>>=const{RefCell::new(None)};
    static EDIT_PROC:Cell<WNDPROC>=const{Cell::new(None)};
    static MENU:Cell<bool>=const{Cell::new(false)};
    #[cfg(test)] static CAPTURE:Cell<bool>=const{Cell::new(false)};
}

#[cfg(test)]
fn recent_load()->History{History::default()}
#[cfg(not(test))]
fn recent_load()->History{
    std::fs::read(crate::lock::data_dir().join("start-history.json")).ok()
        .and_then(|bytes|serde_json::from_slice(&bytes).ok()).map(|v|History::from_json(&v)).unwrap_or_default()
}
fn remember(model:&mut Model,path:&Path){
    if model.recent.remember(path)&&!cfg!(test){
        let _=std::fs::write(crate::lock::data_dir().join("start-history.json"),model.recent.json().to_string());
    }
}
fn make_pin(value:&str)->Entry{
    let path=PathBuf::from(value);
    let name=if value==SETTINGS{"Cài đặt tuanpc-desk".into()}
        else{crate::dock::command_pin::decode(value).map(|c|c.label)
            .unwrap_or_else(||path.file_stem().and_then(|s|s.to_str()).unwrap_or(value).replace('_'," "))};
    Entry{name,path,kind:"Pinned",detail:String::new()}
}
fn catalog_worker()->Option<Receiver<Vec<Entry>>>{
    let (tx,rx)=mpsc::channel();
    std::thread::Builder::new().name("deskland-start-catalog".into()).spawn(move||{
        let mut apps=app_entries();
        use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
        unsafe{
            if CoInitializeEx(None,COINIT_APARTMENTTHREADED).is_ok(){
                apps.extend(installed_apps());CoUninitialize();
            }
        }
        let mut seen=HashSet::new();
        apps.retain(|app|app.kind=="Application"&&seen.insert(app.path.to_string_lossy().to_lowercase()));
        apps.sort_by_cached_key(|app|app.name.to_lowercase());
        let _=tx.send(apps);
    }).ok()?;
    Some(rx)
}
fn entries_for(model:&Model)->Vec<Entry>{
    let mut all=model.all.clone();
    for pin in &model.pins{
        if !all.iter().any(|entry|same_path(&entry.path,Path::new(pin))){all.push(make_pin(pin));}
    }
    all
}
fn px(v:i32,scale:f32)->i32{(v as f32*scale).round() as i32}
fn rr(x:i32,y:i32,w:i32,h:i32,s:f32)->RECT{RECT{left:px(x,s),top:px(y,s),right:px(x+w,s),bottom:px(y+h,s)}}
fn entry_group(model:&Model,all:&[Entry])->(Vec<Entry>,bool){
    if !model.query.trim().is_empty(){
        return (filter(all,&model.query,&History::default()).into_iter().map(|i|all[i].clone()).collect(),false);
    }
    match model.area {
        Area::Apps=>(model.all.clone(),false),
        Area::Home=>(model.pins.iter().filter_map(|p|all.iter().find(|a|same_path(&a.path,Path::new(p))).cloned()).collect(),true),
    }
}
fn tiles(model:&Model)->Vec<Tile>{
    let mut tiles=Vec::new();
    let s=model.scale;
    let mut put=|hit,text:&str,r:(i32,i32,i32,i32),entry,compact|{
        tiles.push(Tile{hit,text:text.into(),rect:rr(r.0,r.1,r.2,r.3,s),entry,compact});
    };
    put(Hit::Settings,"Cài đặt",(421,18,86,34),None,true);
    put(Hit::Power,"Nguồn",(513,18,73,34),None,true);
    put(Hit::Home,"Đã ghim",(22,106,90,33),None,true);
    put(Hit::Apps,"Tất cả ứng dụng",(118,106,133,33),None,true);
    let all=entries_for(model);
    let (entries,grid)=entry_group(model,&all);
    let limit=if grid{GRID_PAGE}else{LIST_PAGE};
    let pages=entries.len().div_ceil(limit).max(1);
    let page=model.page.min(pages-1);
    if pages>1{
        if page>0{put(Hit::Page(-1),"‹",(475,106,35,33),None,true);}
        if page+1<pages{put(Hit::Page(1),"›",(551,106,35,33),None,true);}
    }
    let start=page*limit;
    for (i,entry) in entries.iter().skip(start).take(limit).enumerate(){
        let r=if grid{(22+(i%COLS) as i32*94,155+(i/COLS) as i32*76,88,68)}
            else{(22+(i%2) as i32*285,151+(i/2) as i32*39,279,35)};
        put(Hit::Entry(i),&entry.name,r,Some(entry.clone()),!grid);
    }
    if grid{
        let recent=filter(&all,"",&model.recent);
        for (i,&index) in recent.iter().take(4).enumerate(){
            let entry=&all[index];
            put(Hit::Entry(GRID_PAGE+i),&entry.name,(22+(i%2) as i32*285,413+(i/2) as i32*34,279,31),Some(entry.clone()),true);
        }
    }
    put(Hit::Explorer,"File Explorer",(22,492,112,27),None,true);
    put(Hit::Windows,"Cài đặt Windows",(141,492,138,27),None,true);
    put(Hit::TaskManager,"Task Manager",(286,492,118,27),None,true);
    tiles
}

fn selected(model:&Model)->Option<Tile>{
    tiles(model).into_iter().filter(|t|matches!(t.hit,Hit::Entry(_))).nth(model.selected)
}
fn repaint(hwnd:HWND){unsafe{InvalidateRect(hwnd,ptr::null(),0);}}
pub(super) fn pins_changed(){
    let pins=DOCK_PINS.with(|p|p.borrow().clone());
    let hwnd=START.with(|s|{let mut g=s.borrow_mut();let m=g.as_mut()?;m.pins=pins;Some(m.hwnd)});
    if let Some(hwnd)=hwnd{repaint(hwnd);}
}
unsafe fn fill(dc:HDC,r:&RECT,color:u32){
    let brush=CreateSolidBrush(color);FillRect(dc,r,brush);DeleteObject(brush);
}
unsafe fn icon(dc:HDC,icon:&IconPixels,x:i32,y:i32){
    let mut info:BITMAPINFO=std::mem::zeroed();
    info.bmiHeader.biSize=std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth=icon.width;info.bmiHeader.biHeight=-icon.height;
    info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;
    let mut bits=ptr::null_mut();
    let bitmap=CreateDIBSection(dc,&info,DIB_RGB_COLORS,&mut bits,ptr::null_mut(),0);
    if bitmap.is_null()||bits.is_null(){if !bitmap.is_null(){DeleteObject(bitmap);}return;}
    let src=CreateCompatibleDC(dc);
    if !src.is_null(){
        ptr::copy_nonoverlapping(icon.pixels.as_ptr(),bits.cast(),icon.pixels.len());
        let old=SelectObject(src,bitmap);
        let blend=BLENDFUNCTION{BlendOp:AC_SRC_OVER as u8,BlendFlags:0,SourceConstantAlpha:255,AlphaFormat:AC_SRC_ALPHA as u8};
        AlphaBlend(dc,x,y,icon.width,icon.height,src,0,0,icon.width,icon.height,blend);
        SelectObject(src,old);DeleteDC(src);
    }
    DeleteObject(bitmap);
}
type TileIcon=Option<IconPixels>;
unsafe fn paint_start(hwnd:HWND){
    let mut ps:PAINTSTRUCT=std::mem::zeroed();let target=BeginPaint(hwnd,&mut ps);
    // Do not hold a RefCell borrow across window/control calls.
    let view=START.with(|cell|{
        let mut b=cell.borrow_mut();let m=b.as_mut()?;
        let tiles=tiles(m);
        let selected=tiles.iter().filter(|t|matches!(t.hit,Hit::Entry(_))).nth(m.selected).map(|t|t.hit.clone());
        let mut icons=Vec::<TileIcon>::new();
        for tile in &tiles{
            let entry=tile.entry.as_ref();
            let key=entry.map(|e|(e.path.clone(),px(if tile.compact{22}else{35},m.scale).max(16)));
            let pixels=key.as_ref().and_then(|key|m.icons.get(key)).cloned().flatten();
            if let Some(key)=key{
                if key.0!=Path::new(SETTINGS)&&!m.icons.contains_key(&key)&&m.loading.len()<24&&m.loading.insert(key.clone()){
                    if m.icon_tx.send(key.clone()).is_err(){m.loading.remove(&key);}
                }
            }
            icons.push(pixels);
        }
        let all=entries_for(m);
        let (items,grid)=entry_group(m,&all);
        let pages=items.len().div_ceil(if grid{GRID_PAGE}else{LIST_PAGE}).max(1);
        Some((m.scale,m.dark,m.glass.clone(),tiles,icons,selected,m.area,m.query.is_empty(),
            m.recent.paths.is_empty(),m.error.clone(),m.page.min(pages-1),pages,items.len(),m.catalog.is_some()||!m.loading.is_empty()))
    });
    if let Some((s,dark,glass,tiles,icons,selected,area,empty_query,empty_recent,error,page,pages,count,pending))=view{
        let width=px(WIDTH,s);let height=px(HEIGHT,s);
        let dc=CreateCompatibleDC(target);let bm=CreateCompatibleBitmap(target,width,height);
        if !dc.is_null()&&!bm.is_null(){
            let old=SelectObject(dc,bm);
            let (bg,fg,sub,card,active) = if dark{(0x00231E1B,0x00F5F1EE,0x00B8B0A9,0x003C342C,0x00695142)}
                else{(0x00F8F6F3,0x00262422,0x00645951,0x00F3EFEB,0x00EAD8C6)};
            fill(dc,&rr(0,0,WIDTH,HEIGHT,s),bg);
            if !glass.is_empty(){
                let mut info:BITMAPINFO=std::mem::zeroed();
                info.bmiHeader.biSize=std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                info.bmiHeader.biWidth=(width/4).max(1);info.bmiHeader.biHeight=-(height/4).max(1);
                info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;
                SetStretchBltMode(dc,HALFTONE);
                StretchDIBits(dc,0,0,width,height,0,0,info.bmiHeader.biWidth,-info.bmiHeader.biHeight,
                    glass.as_ptr().cast(),&info,DIB_RGB_COLORS,SRCCOPY);
            }
            let body=font(px(14,s),400);let small=font(px(12,s),400);
            let heading=font(px(21,s),600);let detail=font(px(15,s),600);
            DrawIconEx(dc,px(22,s),px(21,s),crate::app_icon(30,30),px(30,s),px(30,s),0,ptr::null_mut(),DI_NORMAL);
            let user=std::env::var("USERNAME").unwrap_or_else(|_|"Deskland".into());
            label(dc,&format!("Start · {user}"),rr(62,14,330,40,s),heading,fg);
            if pages>1{label(dc,&format!("{} / {pages}",page+1),rr(514,106,38,33,s),small,sub);}
            for (tile,img) in tiles.iter().zip(icons.iter()){
                let picked=selected.as_ref()==Some(&tile.hit)
                    || (empty_query&&matches!((&tile.hit,area),(Hit::Home,Area::Home)|(Hit::Apps,Area::Apps)));
                fill(dc,&tile.rect,if picked{active}else{card});
                let mut r=tile.rect;
                r.left+=px(8,s);r.right-=px(5,s);
                if let Some(entry)=&tile.entry{
                    let size=px(if tile.compact{22}else{35},s);
                    let x=if tile.compact{r.left}else{(r.left+r.right-size)/2};
                    let y=if tile.compact{(r.top+r.bottom-size)/2}else{r.top+px(6,s)};
                    if let Some(img)=img{icon(dc,img,x+(size-img.width)/2,y+(size-img.height)/2);}
                    else if entry.path==Path::new(SETTINGS){DrawIconEx(dc,x,y,crate::app_icon(size,size),size,size,0,ptr::null_mut(),DI_NORMAL);}
                    else{label(dc,"◇",RECT{left:x,top:y,right:x+size,bottom:y+size},body,sub);}
                    if tile.compact{r.left+=size+px(8,s);}else{r.top+=px(42,s);}
                }
                label(dc,&tile.text,r,if tile.compact{body}else{small},fg);
            }
            if count==0{
                let prompt=if !empty_query{"Không tìm thấy ứng dụng"}else if area==Area::Home{"Chưa có ứng dụng ghim. Mở Tất cả ứng dụng để ghim."}
                    else if pending{"Đang lấy danh sách ứng dụng…"}else{"Không có ứng dụng"};
                label(dc,prompt,rr(22,153,566,36,s),body,sub);
            }
            if area==Area::Home&&empty_query{
                label(dc,"Đã mở gần đây",rr(22,376,530,33,s),detail,fg);
                if empty_recent{label(dc,"Ứng dụng mở từ Start sẽ xuất hiện ở đây.",rr(22,412,560,31,s),small,sub);}
            }
            if !error.is_empty(){label(dc,&error,rr(22,463,564,24,s),small,fg);}
            #[cfg(test)]
            if let Some(path)=std::env::var_os("TUANPC_DESK_START_EVIDENCE").map(PathBuf::from){
                if CAPTURE.with(Cell::get)&&!path.exists(){let _=capture_render(dc,width,height,&path);}
            }
            BitBlt(target,0,0,width,height,dc,0,0,SRCCOPY);
            SelectObject(dc,old);
            for f in [body,small,heading,detail]{DeleteObject(f);}
        }
        if !bm.is_null(){DeleteObject(bm);}if !dc.is_null(){DeleteDC(dc);}
        if pending{SetTimer(hwnd,POLL,70,None);}
    }
    EndPaint(hwnd,&ps);
}

unsafe fn hide(hwnd:HWND){KillTimer(hwnd,POLL);ShowWindow(hwnd,SW_HIDE);}
fn hit_at(m:&Model,x:i32,y:i32)->Option<Tile>{
    tiles(m).into_iter().find(|t|x>=t.rect.left&&x<t.rect.right&&y>=t.rect.top&&y<t.rect.bottom)
}
fn coords(l:LPARAM)->(i32,i32){(l as i16 as i32,(l>>16) as i16 as i32)}
unsafe fn edit_value(edit:HWND)->String{
    let mut b=[0u16;257];let n=GetWindowTextW(edit,b.as_mut_ptr(),b.len() as i32);
    String::from_utf16_lossy(&b[..n.max(0) as usize])
}
unsafe fn refresh_query(hwnd:HWND){
    let edit=START.with(|s|s.borrow().as_ref().map(|m|m.edit));
    if let Some(edit)=edit{
        let query=edit_value(edit);
        START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){m.query=query;m.page=0;m.selected=0;m.error.clear();});
        repaint(hwnd);
    }
}
unsafe fn launch(hwnd:HWND,entry:Entry){
    let owner=START.with(|s|s.borrow().as_ref().map(|m|m.owner)).unwrap_or_default();
    if entry.path==Path::new(SETTINGS){hide(hwnd);PostMessageW(owner,crate::WM_OPEN_SETTINGS,0,0);return;}
    hide(hwnd);
    let code=execute(hwnd,&entry.path);
    if code>32{
        START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){remember(m,&entry.path);});
    }else{
        START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){m.error=format!("Không mở được {} (Windows {code})",entry.name);});
        ShowWindow(hwnd,SW_SHOW);repaint(hwnd);
    }
}
unsafe fn action_menu(hwnd:HWND,point:POINT,actions:&[&str])->u32{
    let menu=CreatePopupMenu();if menu.is_null(){return 0;}
    for (i,text) in actions.iter().enumerate(){AppendMenuW(menu,MF_STRING,i+1,wide(text).as_ptr());}
    MENU.with(|flag|flag.set(true));
    let result=TrackPopupMenu(menu,TPM_RETURNCMD|TPM_NONOTIFY|TPM_RIGHTBUTTON,point.x,point.y,0,hwnd,ptr::null_mut());
    DestroyMenu(menu);MENU.with(|flag|flag.set(false));
    result as u32
}
unsafe fn power(hwnd:HWND,r:RECT){
    let mut point=POINT{x:r.left,y:r.bottom};ClientToScreen(hwnd,&mut point);
    let actions=["Khóa máy","Ngủ","Đăng xuất","Khởi động lại","Tắt máy"];
    let choice=action_menu(hwnd,point,&actions);
    if choice==0||choice as usize>actions.len(){return;}
    if cfg!(test)||crate::trial::protect_windows().is_err(){
        START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){m.error="Bản thử không thực hiện thao tác nguồn hoặc phiên Windows.".into();});
        repaint(hwnd);return;
    }
    if choice>1{
        MENU.with(|flag|flag.set(true));
        let title=actions[(choice-1) as usize];
        let confirm=MessageBoxW(hwnd,wide(&format!("{title}? Hãy lưu công việc đang mở trước khi tiếp tục.")).as_ptr(),
            wide("Xác nhận thao tác Windows").as_ptr(),MB_YESNO|MB_ICONWARNING|MB_DEFBUTTON2);
        MENU.with(|flag|flag.set(false));
        if confirm!=IDYES{return;}
    }
    hide(hwnd);
    let ok=match choice{
        1=>{crate::lock::lock_now();true},
        2=>windows_sys::Win32::System::Power::SetSuspendState(0,0,0)!=0,
        _=>{
            let args=match choice{3=>"/l",4=>"/r /t 0",_=>"/s /t 0"};
            let mut path=[0u16;32768];
            let len=windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(path.as_mut_ptr(),path.len() as u32) as usize;
            if len==0||len>=path.len(){false}else{
                let mut path=PathBuf::from(String::from_utf16_lossy(&path[..len]));
                path.push("shutdown.exe");
                ShellExecuteW(hwnd,wide("open").as_ptr(),wide(&path.to_string_lossy()).as_ptr(),wide(args).as_ptr(),ptr::null(),SW_HIDE) as isize>32
            }
        }
    };
    if !ok{
        START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){m.error="Windows từ chối thao tác nguồn.".into();});
        ShowWindow(hwnd,SW_SHOW);repaint(hwnd);
    }
}
unsafe fn activate(hwnd:HWND,tile:Tile){
    let open=|name:&str,path:&str|launch(hwnd,Entry{name:name.into(),path:path.into(),kind:"Application",detail:String::new()});
    match tile.hit{
        Hit::Entry(_)=>if let Some(entry)=tile.entry{launch(hwnd,entry);},
        Hit::Settings=>open("Cài đặt Deskland",SETTINGS),
        Hit::Explorer=>open("File Explorer","explorer.exe"),
        Hit::Windows=>open("Cài đặt Windows","ms-settings:"),
        Hit::TaskManager=>open("Task Manager","Taskmgr.exe"),
        Hit::Power=>power(hwnd,tile.rect),
        Hit::Home|Hit::Apps|Hit::Page(_)=>{
            let edit=START.with(|s|{
                let mut b=s.borrow_mut();let m=b.as_mut()?;
                match tile.hit{
                    Hit::Home|Hit::Apps=>{m.area=if tile.hit==Hit::Home{Area::Home}else{Area::Apps};m.page=0;m.query.clear();}
                    Hit::Page(delta)=>{m.page=m.page.saturating_add_signed(delta as isize);}
                    _=>{}
                }
                m.selected=0;Some(m.edit)
            });
            if !matches!(tile.hit,Hit::Page(_)){if let Some(edit)=edit{SetWindowTextW(edit,wide("").as_ptr());}}
            repaint(hwnd);
        }
    }
}
unsafe fn entry_menu(hwnd:HWND,tile:Tile,mut at:POINT){
    let Some(entry)=tile.entry else{return;};
    let edit=pin_change(&entry);
    let text=if matches!(&edit,Some(crate::dock::Edit::Unpin(_))){"Bỏ ghim khỏi Dock và Start"}else{"Ghim vào Dock và Start"};
    ClientToScreen(hwnd,&mut at);
    match action_menu(hwnd,at,&["Mở ứng dụng",text]){
        1=>launch(hwnd,entry),
        2=>if let Some(edit)=edit{PIN_EDIT.with(|p|p.replace(Some(edit)));},
        _=>{}
    }
}

unsafe extern "system" fn edit_proc(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
    if msg==WM_KEYDOWN&&matches!(w as u16,VK_UP|VK_DOWN|VK_RETURN|VK_ESCAPE|VK_PRIOR|VK_NEXT|VK_TAB|VK_APPS){
        PostMessageW(GetParent(hwnd),APPLY,w,0);return 0;
    }
    if msg==WM_CHAR&&matches!(w,9|13|27){return 0;}
    CallWindowProcW(EDIT_PROC.with(Cell::get),hwnd,msg,w,l)
}
unsafe extern "system" fn proc_start(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
    match msg{
        WM_ERASEBKGND=>1,
        WM_PAINT=>{paint_start(hwnd);0}
        WM_COMMAND if w&0xffff==EDIT_ID&&(w>>16)&0xffff==EN_CHANGE as usize=>{refresh_query(hwnd);0}
        WM_CTLCOLOREDIT=>{
            let dark=START.with(|s|s.borrow().as_ref().is_some_and(|m|m.dark));
            SetTextColor(w as HDC,if dark{0x00FFFFFF}else{0x00262422});
            SetBkColor(w as HDC,if dark{0x00000000}else{0x00FFFFFF});
            GetStockObject(if dark{BLACK_BRUSH}else{WHITE_BRUSH}) as LRESULT
        }
        WM_ACTIVATE if w&0xffff==WA_INACTIVE as usize=>{if !MENU.with(Cell::get){hide(hwnd);}0}
        WM_CLOSE=>{hide(hwnd);0}
        WM_KEYDOWN|APPLY=>{
            match w as u16{
                VK_ESCAPE=>hide(hwnd),
                VK_RETURN=>{
                    let tile=START.with(|s|s.borrow().as_ref().and_then(selected));
                    if let Some(tile)=tile{activate(hwnd,tile);}
                }
                VK_APPS=>{
                    let tile=START.with(|s|s.borrow().as_ref().and_then(selected));
                    if let Some(tile)=tile{let rect=tile.rect;entry_menu(hwnd,tile,POINT{x:rect.left,y:rect.bottom});}
                }
                VK_TAB=>{
                    let tile=START.with(|s|{let b=s.borrow();let m=b.as_ref()?;tiles(m).into_iter().find(|t|t.hit==(if m.area==Area::Home{Hit::Apps}else{Hit::Home}))});
                    if let Some(tile)=tile{activate(hwnd,tile);}
                }
                VK_UP|VK_DOWN=>{
                    START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){
                        let count=tiles(m).iter().filter(|t|t.entry.is_some()).count();
                        m.selected=m.selected.saturating_add_signed(if w as u16==VK_UP{-1}else{1}).min(count.saturating_sub(1));
                    });repaint(hwnd);
                }
                VK_PRIOR|VK_NEXT=>{
                    let delta=if w as u16==VK_PRIOR{-1}else{1};
                    let tile=START.with(|s|{let b=s.borrow();tiles(b.as_ref()?).into_iter().find(|t|t.hit==Hit::Page(delta))});
                    if let Some(tile)=tile{activate(hwnd,tile);}
                }
                _=>{}
            }0
        }
        WM_MOUSEWHEEL=>{
            PostMessageW(hwnd,APPLY,if (w>>16) as i16>0{VK_PRIOR as usize}else{VK_NEXT as usize},0);0
        }
        WM_RBUTTONUP=>{
            let (x,y)=coords(l);
            let tile=START.with(|s|hit_at(s.borrow().as_ref()?,x,y));
            if let Some(tile)=tile{entry_menu(hwnd,tile,POINT{x,y});}0
        }
        WM_LBUTTONDOWN=>{
            let (x,y)=coords(l);
            let captured=START.with(|s|{
                let mut b=s.borrow_mut();let Some(m)=b.as_mut()else{return false;};
                m.pressed=hit_at(m,x,y).map(|tile|(tile.hit,x,y));m.pressed.is_some()
            });
            if captured{SetCapture(hwnd);}0
        }
        WM_CAPTURECHANGED=>{START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){m.pressed=None;});0}
        WM_LBUTTONUP=>{
            let (x,y)=coords(l);
            let click=START.with(|s|{
                let mut b=s.borrow_mut();let m=b.as_mut()?;
                let (press,sx,sy)=m.pressed.take()?;
                let mut tile=hit_at(m,x,y)?;
                let far=(x-sx).abs().max((y-sy).abs())>px(6,m.scale);
                if press==tile.hit&&!far{return Some(tile);}
                if far&&m.area==Area::Home&&m.query.is_empty()
                    && matches!(press,Hit::Entry(i) if i<GRID_PAGE)&&matches!(tile.hit,Hit::Entry(i) if i<GRID_PAGE){
                    let src=tiles(m).into_iter().find(|t|t.hit==press)?.entry?;
                    let dst=tile.entry.take()?;
                    if !same_path(&src.path,&dst.path){
                        PIN_EDIT.with(|p|p.replace(Some(crate::dock::Edit::MovePin{
                            path:src.path.to_string_lossy().into_owned(),anchor:dst.path.to_string_lossy().into_owned(),
                            after:x>=(tile.rect.left+tile.rect.right)/2,
                        })));
                    }
                }
                None
            });
            if GetCapture()==hwnd{ReleaseCapture();}
            if let Some(tile)=click{activate(hwnd,tile);}0
        }
        WM_TIMER if w==POLL=>{
            let (changed,busy)=START.with(|s|{
                let mut b=s.borrow_mut();let Some(m)=b.as_mut()else{return(false,false);};
                let mut changed=false;
                if let Some(catalog)=&m.catalog{
                    match catalog.try_recv(){
                        Ok(apps)=>{m.all=apps;m.catalog=None;m.indexed_at=Some(Instant::now());changed=true;},
                        Err(mpsc::TryRecvError::Disconnected)=>m.catalog=None,
                        Err(mpsc::TryRecvError::Empty)=>{}
                    }
                }
                while let Ok((key,icon))=m.icon_rx.try_recv(){
                    if m.icons.len()>=192{m.icons.clear();}
                    m.loading.remove(&key);m.icons.insert(key,icon);changed=true;
                }
                (changed,!m.loading.is_empty()||m.catalog.is_some())
            });
            if changed{repaint(hwnd);}if !busy{KillTimer(hwnd,POLL);}0
        }
        _=>DefWindowProcW(hwnd,msg,w,l),
    }
}

pub(crate) fn toggle(owner:HWND,anchor:DockAnchor){
    unsafe{
        let previous=START.with(|s|s.borrow().as_ref().map(|m|(m.hwnd,m.anchor)));
        if let Some((hwnd,last))=previous{
            if IsWindowVisible(hwnd)!=0{
                hide(hwnd);
                if MonitorFromPoint(last.point,MONITOR_DEFAULTTONEAREST)==MonitorFromPoint(anchor.point,MONITOR_DEFAULTTONEAREST){return;}
            }
        }else{
            let cls=wide(CLASS);
            let module=windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(ptr::null());
            let wc=WNDCLASSW{lpfnWndProc:Some(proc_start),hInstance:module,hCursor:LoadCursorW(ptr::null_mut(),IDC_ARROW),lpszClassName:cls.as_ptr(),..std::mem::zeroed()};
            RegisterClassW(&wc);
            let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW,cls.as_ptr(),wide("tuanpc-desk Start").as_ptr(),WS_POPUP|WS_CLIPCHILDREN,
                0,0,1,1,ptr::null_mut(),ptr::null_mut(),module,ptr::null());
            if hwnd.is_null(){return;}
            let edit=CreateWindowExW(0,wide("EDIT").as_ptr(),wide("").as_ptr(),WS_CHILD|WS_VISIBLE|ES_AUTOHSCROLL as u32,
                0,0,1,1,hwnd,EDIT_ID as HMENU,module,ptr::null());
            if edit.is_null(){DestroyWindow(hwnd);return;}
            let old=SetWindowLongPtrW(edit,GWLP_WNDPROC,edit_proc as *const () as isize);
            if old==0{DestroyWindow(hwnd);return;}
            EDIT_PROC.with(|c|c.set(Some(std::mem::transmute::<isize,unsafe extern "system" fn(HWND,u32,WPARAM,LPARAM)->LRESULT>(old))));
            SendMessageW(edit,0x00c5,128,0); // EM_LIMITTEXT
            SendMessageW(edit,0x1501,1,wide("Tìm ứng dụng…").as_ptr() as LPARAM); // EM_SETCUEBANNER
            let (icon_tx,icon_rx)=icon_worker();
            START.with(|s|s.replace(Some(Model{hwnd,edit,owner,anchor,scale:1.0,dark:false,area:Area::Home,all:Vec::new(),
                pins:Vec::new(),recent:recent_load(),query:String::new(),page:0,selected:0,pressed:None,error:String::new(),glass:Vec::new(),
                edit_font:ptr::null_mut(),icons:HashMap::new(),loading:HashSet::new(),icon_tx,icon_rx,catalog:None,indexed_at:None})));
        }
        let Some((hwnd,edit))=START.with(|s|s.borrow().as_ref().map(|m|(m.hwnd,m.edit)))else{return;};
        let monitor=MonitorFromPoint(anchor.point,MONITOR_DEFAULTTONEAREST);
        let mut info:MONITORINFO=std::mem::zeroed();info.cbSize=std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor,&mut info)==0{return;}
        let work=info.rcWork;
        let _=SetWindowPos(hwnd,ptr::null_mut(),work.left,work.top,1,1,SWP_NOACTIVATE|SWP_NOZORDER);
        let s=(windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd).clamp(72,288) as f32/96.0)
            .min((work.right-work.left) as f32/WIDTH as f32).min((work.bottom-work.top) as f32/HEIGHT as f32);
        if s<=0.0{return;}
        let (width,height)=(px(WIDTH,s),px(HEIGHT,s));
        let (x,y)=anchor.position(work,width,height,px(12,s));
        let prefs=PREFS.with(|p|p.borrow().clone());
        let dark=prefs.theme=="dark";
        let glass=frost(&capture_glass(x,y,width,height),width,height,s,&prefs);
        let font=font(px(18,s),400);
        let pins=DOCK_PINS.with(|p|p.borrow().clone());
        let old_font=START.with(|sref|{
            let mut b=sref.borrow_mut();let m=b.as_mut().unwrap();
            m.scale=s;m.anchor=anchor;m.owner=owner;m.dark=dark;m.pins=pins;m.area=Area::Home;m.query.clear();
            m.page=0;m.selected=0;m.pressed=None;m.error.clear();m.glass=glass;m.recent=recent_load();
            if m.catalog.is_none()&&m.indexed_at.is_none_or(|t|t.elapsed()>Duration::from_secs(180)){m.catalog=catalog_worker();}
            std::mem::replace(&mut m.edit_font,font)
        });
        SendMessageW(edit,WM_SETFONT,font as usize,1);
        if !old_font.is_null(){DeleteObject(old_font);}
        SetWindowTextW(edit,wide("").as_ptr());
        MoveWindow(edit,px(22,s),px(64,s),px(564,s),px(31,s),0);
        use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
        let round=2i32;let border=0xffff_fffeu32;let mode=i32::from(dark);
        DwmSetWindowAttribute(hwnd,33,(&round as *const i32).cast(),4);
        DwmSetWindowAttribute(hwnd,34,(&border as *const u32).cast(),4);
        DwmSetWindowAttribute(hwnd,20,(&mode as *const i32).cast(),4);
        SetWindowPos(hwnd,HWND_TOP,x,y,width,height,SWP_NOACTIVATE);
        SetWindowPos(hwnd,ptr::null_mut(),0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOZORDER|SWP_SHOWWINDOW);
        SetForegroundWindow(hwnd);SetFocus(edit);repaint(hwnd);
        crate::log!("DOCK_START opened separate panel at ({x},{y}) scale={s:.2}");
    }
}


#[cfg(test)]
unsafe fn capture_render(dc:HDC,width:i32,height:i32,path:&Path)->Result<(),String>{
    let memory=CreateCompatibleDC(dc);
    if memory.is_null(){return Err("No screenshot DC".into());}
    let mut info:BITMAPINFO=std::mem::zeroed();
    info.bmiHeader.biSize=std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth=width;info.bmiHeader.biHeight=-height;
    info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;
    let mut bits=ptr::null_mut();
    let bitmap=CreateDIBSection(dc,&info,DIB_RGB_COLORS,&mut bits,ptr::null_mut(),0);
    let result=if !bitmap.is_null()&&!bits.is_null(){
        let old=SelectObject(memory,bitmap);
        let copied=BitBlt(memory,0,0,width,height,dc,0,0,SRCCOPY)!=0;GdiFlush();
        let result=if copied{
            let bytes=std::slice::from_raw_parts(bits.cast::<u8>(),(width*height*4) as usize);
            let pixels:Vec<_>=bytes.chunks_exact(4).flat_map(|p|[p[2],p[1],p[0]]).collect();
            image::save_buffer(path,&pixels,width as u32,height as u32,image::ColorType::Rgb8).map_err(|e|e.to_string())
        }else{Err("No Start pixels received".into())};
        SelectObject(memory,old);result
    }else{Err("No screenshot bitmap".into())};
    if !bitmap.is_null(){DeleteObject(bitmap);}DeleteDC(memory);result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "real isolated Start popup, edit control and Win32 keyboard dispatch"]
    fn native_panel_search_keyboard_and_anchor_are_independent_of_spotlight(){
        unsafe {
            let screens=crate::dock::displays();
            let display=screens.first().unwrap();
            let point=POINT{x:display.bounds.left+120,y:display.bounds.bottom-60};
            let anchor=DockAnchor{point,side:crate::dock::DockSide::Bottom};
            DOCK_PINS.with(|p|p.replace(vec![r"C:\Test\Notepad.exe".into()]));
            toggle(ptr::null_mut(),anchor);
            let (host,edit)=START.with(|s|{
                let mut s=s.borrow_mut();
                let m=s.as_mut().expect("separate Start HWND");
                m.catalog=None;
                m.all=vec![Entry{name:"Paint".into(),path:"mspaint.exe".into(),kind:"Application",detail:String::new()}];
                (m.hwnd,m.edit)
            });
            if let Some(path)=std::env::var_os("TUANPC_DESK_START_EVIDENCE").map(PathBuf::from){
                START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){
                    m.glass.clear();m.pins=vec!["mspaint.exe".into(),"notepad.exe".into(),"deskland:settings".into()];
                });
                CAPTURE.with(|c|c.set(true));repaint(host);UpdateWindow(host);CAPTURE.with(|c|c.set(false));
                assert!(path.is_file(),"native Start render evidence is required");
                START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){m.pins=vec![r"C:\Test\Notepad.exe".into()];});
            }
            assert_ne!(IsWindowVisible(host),0);
            assert!(handles().is_none(),"opening Start must not start Spotlight");
            let mut rc:RECT=std::mem::zeroed();
            assert_ne!(GetWindowRect(host,&mut rc),0);
            assert!(rc.left>=display.bounds.left&&rc.top>=display.bounds.top&&rc.right<=display.bounds.right&&rc.bottom<=display.bounds.bottom);
            START.with(|s|assert_eq!(selected(s.borrow().as_ref().unwrap()).unwrap().text,"Notepad"));
            SetWindowTextW(edit,wide("paint").as_ptr());
            START.with(|s|assert_eq!(selected(s.borrow().as_ref().unwrap()).unwrap().text,"Paint"));
            SendMessageW(host,APPLY,VK_RETURN as usize,0);
            assert_eq!(IsWindowVisible(host),0);
            START.with(|s|assert_eq!(s.borrow().as_ref().unwrap().recent.paths,vec![PathBuf::from("mspaint.exe")]));
            let display=screens.get(1).unwrap_or(display);
            toggle(ptr::null_mut(),DockAnchor{point:POINT{x:display.bounds.left+90,y:display.bounds.top+150},side:crate::dock::DockSide::Left});
            assert_ne!(GetWindowRect(host,&mut rc),0);
            assert!(rc.left>=display.bounds.left&&rc.top>=display.bounds.top&&rc.right<=display.bounds.right&&rc.bottom<=display.bounds.bottom);
            SendMessageW(host,APPLY,VK_ESCAPE as usize,0);
            assert_eq!(IsWindowVisible(host),0);
            DestroyWindow(host);
            START.with(|s|if let Some(m)=s.borrow_mut().take(){DeleteObject(m.edit_font);});
        }
    }
}
