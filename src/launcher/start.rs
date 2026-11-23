//! Dock Start has a separate window, layout and launch history. Only the
//! app catalog, Shell launching and icon extraction are shared with Spotlight.
use super::*;
use std::{collections::{HashMap,HashSet},ptr,rc::Rc,sync::mpsc::{self,Receiver,Sender},time::{Duration,Instant}};

const CLASS:&str="TuanpcDeskStartPanel";
const WIDTH:i32=490;
const MAX_HEIGHT:i32=530;
const COLS:usize=5;
const GRID_PAGE:usize=15;
const LIST_PAGE:usize=14;
const INSET:i32=20;
const CONTENT_Y:i32=148;
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
    hwnd:HWND, edit:HWND, owner:HWND, anchor:DockAnchor, work:RECT, scale:f32, dark:bool, area:Area,
    all:Vec<Entry>, pins:Vec<String>, recent:History, query:String, page:usize, selected:usize,
    pressed:Option<(Hit,i32,i32)>, hover:Option<Hit>, keyboard:bool, view:StartView, error:String, glass:Rc<[u8]>, edit_font:HFONT,
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
#[derive(Clone,Copy)]
struct Flow{height:i32,heading_y:i32,recent_y:Option<i32>,footer_y:i32}
struct StartView{tiles:Vec<Tile>,page:usize,pages:usize,count:usize,recent_empty:bool,flow:Flow}
fn flow(grid:bool,visible:usize,recent_count:usize,error:bool)->Flow{
    let row_count=visible.div_ceil(if grid{COLS}else{2});
    let mut end=CONTENT_Y+if visible==0{38}else{row_count as i32*if grid{69}else{36}};
    let recent_y=grid.then_some(end+9);
    if let Some(y)=recent_y{end=y+26+if recent_count==0{27}else{recent_count.div_ceil(2) as i32*33};}
    if error{end+=24;}
    let footer_y=end+8;
    Flow{height:footer_y+42,heading_y:115,recent_y,footer_y}
}
fn content(model:&Model)->(Vec<Entry>,bool,Vec<Entry>,usize,usize,Flow){
    let all=entries_for(model);
    let (entries,grid)=entry_group(model,&all);
    let limit=if grid{GRID_PAGE}else{LIST_PAGE};
    let pages=entries.len().div_ceil(limit).max(1);
    let page=model.page.min(pages-1);
    let recent=if grid{filter(&all,"",&model.recent).into_iter().take(4).map(|i|all[i].clone()).collect()}else{Vec::new()};
    let layout=flow(grid,entries.len().saturating_sub(page*limit).min(limit),recent.len(),!model.error.is_empty());
    (entries,grid,recent,page,pages,layout)
}
fn build_view(model:&Model)->StartView{
    let mut tiles=Vec::new();
    let s=model.scale;
    let mut put=|hit,text:&str,r:(i32,i32,i32,i32),entry,compact|{
        tiles.push(Tile{hit,text:text.into(),rect:rr(r.0,r.1,r.2,r.3,s),entry,compact});
    };
    put(Hit::Settings,"Cài đặt",(338,16,69,30),None,true);
    put(Hit::Power,"Nguồn",(412,16,58,30),None,true);
    put(Hit::Home,"Trang chính",(20,82,91,27),None,true);
    put(Hit::Apps,"Tất cả ứng dụng",(116,82,115,27),None,true);
    let (entries,grid,recent,page,pages,flow)=content(model);
    let count=entries.len();let recent_empty=recent.is_empty();
    let limit=if grid{GRID_PAGE}else{LIST_PAGE};
    if pages>1{
        if page>0{put(Hit::Page(-1),"‹",(351,114,29,27),None,true);}
        if page+1<pages{put(Hit::Page(1),"›",(441,114,29,27),None,true);}
    }
    let start=page*limit;
    for (i,entry) in entries.iter().skip(start).take(limit).enumerate(){
        let r=if grid{(INSET+(i%COLS) as i32*91,CONTENT_Y+(i/COLS) as i32*69,86,66)}
            else{(INSET+(i%2) as i32*228,CONTENT_Y+(i/2) as i32*36,222,33)};
        put(Hit::Entry(i),&entry.name,r,Some(entry.clone()),!grid);
    }
    if let Some(y)=flow.recent_y{
        for (i,entry) in recent.into_iter().enumerate(){
            let title=entry.name.clone();
            put(Hit::Entry(GRID_PAGE+i),&title,(INSET+(i%2) as i32*228,y+26+(i/2) as i32*33,222,30),Some(entry),true);
        }
    }
    put(Hit::Explorer,"File Explorer",(INSET,flow.footer_y+7,91,24),None,true);
    put(Hit::Windows,"Cài đặt Windows",(117,flow.footer_y+7,117,24),None,true);
    put(Hit::TaskManager,"Task Manager",(240,flow.footer_y+7,98,24),None,true);
    StartView{tiles,page,pages,count,recent_empty,flow}
}

fn selected(model:&Model)->Option<Tile>{
    model.view.tiles.iter().filter(|t|matches!(t.hit,Hit::Entry(_))).nth(model.selected).cloned()
}
fn repaint(hwnd:HWND){unsafe{InvalidateRect(hwnd,ptr::null(),0);}}
unsafe fn relayout(hwnd:HWND){
    let geometry=START.with(|s|{
        let mut b=s.borrow_mut();let m=b.as_mut()?;
        m.view=build_view(m);
        let height=px(m.view.flow.height,m.scale);
        let width=px(WIDTH,m.scale);
        let (x,y)=m.anchor.position(m.work,width,height,px(10,m.scale));
        Some((x,y,width,height))
    });
    let Some((x,y,width,height))=geometry else{return;};
    let mut previous:RECT=std::mem::zeroed();
    GetWindowRect(hwnd,&mut previous);
    if [previous.left,previous.top,previous.right,previous.bottom]!=[x,y,x+width,y+height]{
        SetWindowPos(hwnd,ptr::null_mut(),x,y,width,height,SWP_NOACTIVATE|SWP_NOZORDER);
    }
    repaint(hwnd);
}
pub(super) fn pins_changed(){
    let pins=DOCK_PINS.with(|p|p.borrow().clone());
    let hwnd=START.with(|s|{let mut g=s.borrow_mut();let m=g.as_mut()?;m.pins=pins;Some(m.hwnd)});
    if let Some(hwnd)=hwnd{unsafe{relayout(hwnd);}}
}
unsafe fn fill(dc:HDC,r:&RECT,color:u32){
    let brush=CreateSolidBrush(color);FillRect(dc,r,brush);DeleteObject(brush);
}
unsafe fn pill(dc:HDC,r:RECT,radius:i32,color:u32){
    let brush=CreateSolidBrush(color);if brush.is_null(){return;}
    let old_brush=SelectObject(dc,brush);let old_pen=SelectObject(dc,GetStockObject(NULL_PEN));
    RoundRect(dc,r.left,r.top,r.right,r.bottom,radius,radius);
    SelectObject(dc,old_pen);SelectObject(dc,old_brush);DeleteObject(brush);
}
unsafe fn center_label(dc:HDC,text:&str,mut rect:RECT,font:HFONT,color:u32){
    let old=SelectObject(dc,font);SetTextColor(dc,color);SetBkMode(dc,TRANSPARENT as i32);
    DrawTextW(dc,wide(text).as_ptr(),-1,&mut rect,DT_SINGLELINE|DT_CENTER|DT_VCENTER|DT_END_ELLIPSIS|DT_NOPREFIX);
    SelectObject(dc,old);
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
        let tiles=m.view.tiles.clone();
        let selected=tiles.iter().filter(|t|matches!(t.hit,Hit::Entry(_))).nth(m.selected).map(|t|t.hit.clone());
        let mut icons=Vec::<TileIcon>::new();
        for tile in &tiles{
            let entry=tile.entry.as_ref();
            let key=entry.map(|e|(e.path.clone(),px(if tile.compact{22}else{42},m.scale).max(16)));
            let pixels=key.as_ref().and_then(|key|m.icons.get(key)).cloned().flatten();
            if let Some(key)=key{
                if key.0!=Path::new(SETTINGS)&&!m.icons.contains_key(&key)&&m.loading.len()<24&&m.loading.insert(key.clone()){
                    if m.icon_tx.send(key.clone()).is_err(){m.loading.remove(&key);}
                }
            }
            icons.push(pixels);
        }
        let StartView{page,pages,count,recent_empty,flow,..}=m.view;
        let selected=selected.filter(|_|m.keyboard||!m.query.is_empty());
        Some((m.scale,m.dark,m.glass.clone(),tiles,icons,selected,m.hover.clone(),m.area,m.query.is_empty(),
            recent_empty,m.error.clone(),page,pages,count,m.catalog.is_some()||!m.loading.is_empty(),flow,m.anchor,m.work))
    });
    if let Some((s,dark,glass,tiles,icons,selected,hover,area,empty_query,empty_recent,error,page,pages,count,pending,flow,anchor,work))=view{
        let width=px(WIDTH,s);let height=px(flow.height,s);
        let dc=CreateCompatibleDC(target);let bm=CreateCompatibleBitmap(target,width,height);
        if !dc.is_null()&&!bm.is_null(){
            let old=SelectObject(dc,bm);
            let (bg,fg,sub,card,active,accent) = if dark{(0x00242120,0x00FAF8F6,0x00C2BCB8,0x003B3734,0x00514A3F,0x00FFCA86)}
                else{(0x00F9F8F7,0x00292623,0x006C6560,0x00EDEAE7,0x00F1DFC9,0x008C4B16)};
            fill(dc,&rr(0,0,WIDTH,flow.height,s),bg);
            if !glass.is_empty(){
                let full_height=px(MAX_HEIGHT,s);
                let (_,start)=anchor.position(work,width,full_height,px(10,s));
                let (_,current)=anchor.position(work,width,height,px(10,s));
                let crop=((current-start)/4).max(0);
                let sample_height=(full_height/4).max(1);
                let mut info:BITMAPINFO=std::mem::zeroed();
                info.bmiHeader.biSize=std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                info.bmiHeader.biWidth=(width/4).max(1);info.bmiHeader.biHeight=-sample_height;
                info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;
                SetStretchBltMode(dc,HALFTONE);
                StretchDIBits(dc,0,0,width,height,0,crop,info.bmiHeader.biWidth,(height/4).max(1).min(sample_height-crop),
                    glass.as_ptr().cast(),&info,DIB_RGB_COLORS,SRCCOPY);
            }
            let body=font(px(13,s),500);let small=font(px(12,s),400);
            let heading=font(px(16,s),600);let detail=font(px(14,s),600);
            pill(dc,rr(20,13,34,34,s),px(34,s),card);
            let user=std::env::var("USERNAME").unwrap_or_else(|_|"Deskland".into());
            center_label(dc,&user.chars().next().unwrap_or('D').to_uppercase().to_string(),rr(20,13,34,34,s),heading,accent);
            label(dc,&user,rr(64,11,263,24,s),heading,fg);
            label(dc,"Ứng dụng",rr(64,33,260,15,s),small,sub);
            pill(dc,rr(20,49,450,28,s),px(14,s),card);
            center_label(dc,"⌕",rr(22,49,30,27,s),heading,sub);
            fill(dc,&rr(20,112,450,1,s),card);
            label(dc,if !empty_query{"Kết quả tìm kiếm"}else if area==Area::Home{"Ứng dụng đã ghim"}else{"Tất cả ứng dụng"},
                rr(20,flow.heading_y,300,26,s),detail,fg);
            if pages>1{center_label(dc,&format!("{} / {pages}",page+1),rr(382,114,56,27,s),small,sub);}
            for (tile,img) in tiles.iter().zip(icons.iter()){
                let picked=selected.as_ref()==Some(&tile.hit)
                    || (empty_query&&matches!((&tile.hit,area),(Hit::Home,Area::Home)|(Hit::Apps,Area::Apps)));
                let hovered=hover.as_ref()==Some(&tile.hit);
                if picked||hovered||matches!(tile.hit,Hit::Power|Hit::Settings){
                    pill(dc,tile.rect,px(if tile.compact{12}else{18},s),if picked{active}else{card});
                }
                let mut r=tile.rect;
                r.left+=px(8,s);r.right-=px(5,s);
                if let Some(entry)=&tile.entry{
                    let size=px(if tile.compact{22}else{42},s);
                    let x=if tile.compact{r.left}else{(r.left+r.right-size)/2};
                    let y=if tile.compact{(r.top+r.bottom-size)/2}else{r.top+px(6,s)};
                    if let Some(img)=img{icon(dc,img,x+(size-img.width)/2,y+(size-img.height)/2);}
                    else if entry.path==Path::new(SETTINGS){DrawIconEx(dc,x,y,crate::app_icon(size,size),size,size,0,ptr::null_mut(),DI_NORMAL);}
                    else{label(dc,"◇",RECT{left:x,top:y,right:x+size,bottom:y+size},body,sub);}
                    if tile.compact{r.left+=size+px(8,s);}else{r=tile.rect;r.top+=px(48,s);}
                }
                if tile.entry.is_some()&&tile.compact{
                    label(dc,&tile.text,r,body,fg);
                }else{center_label(dc,&tile.text,if tile.entry.is_some(){r}else{tile.rect},small,if picked{accent}else{fg});}
            }
            if count==0{
                let prompt=if !empty_query{"Không tìm thấy ứng dụng"}else if area==Area::Home{"Chưa có ứng dụng ghim. Mở Tất cả ứng dụng để ghim."}
                    else if pending{"Đang lấy danh sách ứng dụng…"}else{"Không có ứng dụng"};
                label(dc,prompt,rr(20,CONTENT_Y,450,36,s),body,sub);
            }
            if let Some(y)=flow.recent_y{
                fill(dc,&rr(20,y-2,450,1,s),card);
                label(dc,"Đã mở gần đây",rr(20,y,450,24,s),detail,fg);
                if empty_recent{label(dc,"Ứng dụng mở từ Start sẽ xuất hiện ở đây.",rr(20,y+26,450,27,s),small,sub);}
            }
            if !error.is_empty(){label(dc,&error,rr(20,flow.footer_y-25,450,24,s),small,fg);}
            fill(dc,&rr(20,flow.footer_y,450,1,s),card);
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
    m.view.tiles.iter().find(|t|x>=t.rect.left&&x<t.rect.right&&y>=t.rect.top&&y<t.rect.bottom).cloned()
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
        START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){m.query=query;m.page=0;m.selected=0;m.hover=None;m.error.clear();});
        relayout(hwnd);
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
        relayout(hwnd);ShowWindow(hwnd,SW_SHOW);
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
        relayout(hwnd);return;
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
        relayout(hwnd);ShowWindow(hwnd,SW_SHOW);
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
                m.selected=0;m.hover=None;Some(m.edit)
            });
            if !matches!(tile.hit,Hit::Page(_)){if let Some(edit)=edit{SetWindowTextW(edit,wide("").as_ptr());}}
            relayout(hwnd);
        }
    }
}
unsafe fn entry_menu(hwnd:HWND,tile:Tile,mut at:POINT){
    let Some(entry)=tile.entry else{return;};
    let edit=pin_change(&entry);
    let text=if matches!(&edit,Some(crate::dock::Edit::Unpin(_))){"Bỏ ghim khỏi Dock và Start"}else{"Ghim vào Dock và Start"};
    let location=entry.path.is_absolute()&&entry.path.file_name().is_some();
    let actions=if location{vec!["Mở ứng dụng",text,"Mở vị trí tệp"]}else{vec!["Mở ứng dụng",text]};
    ClientToScreen(hwnd,&mut at);
    match action_menu(hwnd,at,&actions){
        1=>launch(hwnd,entry),
        2=>if let Some(edit)=edit{PIN_EDIT.with(|p|p.replace(Some(edit)));},
        3 if location=>{
            let args=format!("/select,\"{}\"",entry.path.display());
            hide(hwnd);
            ShellExecuteW(hwnd,wide("open").as_ptr(),wide("explorer.exe").as_ptr(),wide(&args).as_ptr(),ptr::null(),SW_SHOWNORMAL);
        }
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
            let color=if dark{0x003B3734}else{0x00EDEAE7};
            SetBkColor(w as HDC,color);SetDCBrushColor(w as HDC,color);
            GetStockObject(DC_BRUSH) as LRESULT
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
                    let tile=START.with(|s|{let b=s.borrow();let m=b.as_ref()?;m.view.tiles.iter().find(|t|t.hit==(if m.area==Area::Home{Hit::Apps}else{Hit::Home})).cloned()});
                    if let Some(tile)=tile{activate(hwnd,tile);}
                }
                VK_UP|VK_DOWN=>{
                    START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){
                        let count=m.view.tiles.iter().filter(|t|t.entry.is_some()).count();
                        m.keyboard=true;
                        m.selected=m.selected.saturating_add_signed(if w as u16==VK_UP{-1}else{1}).min(count.saturating_sub(1));
                    });repaint(hwnd);
                }
                VK_PRIOR|VK_NEXT=>{
                    let delta=if w as u16==VK_PRIOR{-1}else{1};
                    let tile=START.with(|s|{let b=s.borrow();b.as_ref()?.view.tiles.iter().find(|t|t.hit==Hit::Page(delta)).cloned()});
                    if let Some(tile)=tile{activate(hwnd,tile);}
                }
                _=>{}
            }0
        }
        WM_MOUSEWHEEL=>{
            PostMessageW(hwnd,APPLY,if (w>>16) as i16>0{VK_PRIOR as usize}else{VK_NEXT as usize},0);0
        }
        WM_MOUSEMOVE=>{
            let (x,y)=coords(l);
            let changed=START.with(|s|{
                let mut s=s.borrow_mut();let Some(m)=s.as_mut()else{return false;};
                let hover=hit_at(m,x,y).map(|tile|tile.hit);
                if m.hover==hover{return false;}
                m.hover=hover;true
            });
            if changed{
                let mut track=windows_sys::Win32::UI::Input::KeyboardAndMouse::TRACKMOUSEEVENT{
                    cbSize:std::mem::size_of::<windows_sys::Win32::UI::Input::KeyboardAndMouse::TRACKMOUSEEVENT>() as u32,
                    dwFlags:windows_sys::Win32::UI::Input::KeyboardAndMouse::TME_LEAVE,hwndTrack:hwnd,dwHoverTime:0
                };
                windows_sys::Win32::UI::Input::KeyboardAndMouse::TrackMouseEvent(&mut track);
                repaint(hwnd);
            }
            0
        }
        0x02A3=>{
            START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){m.hover=None;});
            repaint(hwnd);0
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
                    let src=m.view.tiles.iter().find(|t|t.hit==press)?.entry.clone()?;
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
            let (changed,busy,new_catalog)=START.with(|s|{
                let mut b=s.borrow_mut();let Some(m)=b.as_mut()else{return(false,false,false);};
                let mut changed=false;
                if let Some(catalog)=&m.catalog{
                    match catalog.try_recv(){
                        Ok(apps)=>{m.all=apps;m.catalog=None;m.indexed_at=Some(Instant::now());changed=true;},
                        Err(mpsc::TryRecvError::Disconnected)=>m.catalog=None,
                        Err(mpsc::TryRecvError::Empty)=>{}
                    }
                }
                let new_catalog=changed;
                while let Ok((key,icon))=m.icon_rx.try_recv(){
                    if m.icons.len()>=192{m.icons.clear();}
                    m.loading.remove(&key);m.icons.insert(key,icon);changed=true;
                }
                (changed,!m.loading.is_empty()||m.catalog.is_some(),new_catalog)
            });
            if new_catalog{relayout(hwnd);}else if changed{repaint(hwnd);}if !busy{KillTimer(hwnd,POLL);}0
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
            START.with(|s|s.replace(Some(Model{hwnd,edit,owner,anchor,work:std::mem::zeroed(),scale:1.0,dark:false,area:Area::Home,all:Vec::new(),
                pins:Vec::new(),recent:recent_load(),query:String::new(),page:0,selected:0,pressed:None,hover:None,keyboard:false,
                view:StartView{tiles:Vec::new(),page:0,pages:1,count:0,recent_empty:true,flow:flow(true,0,0,false)},
                error:String::new(),glass:Rc::from([]),
                edit_font:ptr::null_mut(),icons:HashMap::new(),loading:HashSet::new(),icon_tx,icon_rx,catalog:None,indexed_at:None})));
        }
        let Some((hwnd,edit))=START.with(|s|s.borrow().as_ref().map(|m|(m.hwnd,m.edit)))else{return;};
        let monitor=MonitorFromPoint(anchor.point,MONITOR_DEFAULTTONEAREST);
        let mut info:MONITORINFO=std::mem::zeroed();info.cbSize=std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor,&mut info)==0{return;}
        let work=info.rcWork;
        let _=SetWindowPos(hwnd,ptr::null_mut(),work.left,work.top,1,1,SWP_NOACTIVATE|SWP_NOZORDER);
        let s=(windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd).clamp(72,288) as f32/96.0)
            .min((work.right-work.left) as f32/WIDTH as f32).min((work.bottom-work.top) as f32/MAX_HEIGHT as f32);
        if s<=0.0{return;}
        let (width,height)=(px(WIDTH,s),px(MAX_HEIGHT,s));
        let (x,y)=anchor.position(work,width,height,px(10,s));
        let prefs=PREFS.with(|p|p.borrow().clone());
        let dark=prefs.theme=="dark";
        let glass=frost(&capture_glass(x,y,width,height),width,height,s,&prefs);
        let font=font(px(14,s),400);
        let pins=DOCK_PINS.with(|p|p.borrow().clone());
        let old_font=START.with(|sref|{
            let mut b=sref.borrow_mut();let m=b.as_mut().unwrap();
            m.scale=s;m.anchor=anchor;m.work=work;m.owner=owner;m.dark=dark;m.pins=pins;m.area=Area::Home;m.query.clear();
            m.page=0;m.selected=0;m.pressed=None;m.hover=None;m.keyboard=false;m.error.clear();m.glass=glass.into();m.recent=recent_load();
            if m.catalog.is_none()&&m.indexed_at.is_none_or(|t|t.elapsed()>Duration::from_secs(180)){m.catalog=catalog_worker();}
            std::mem::replace(&mut m.edit_font,font)
        });
        SendMessageW(edit,WM_SETFONT,font as usize,1);
        if !old_font.is_null(){DeleteObject(old_font);}
        SetWindowTextW(edit,wide("").as_ptr());
        MoveWindow(edit,px(55,s),px(54,s),px(398,s),px(20,s),0);
        use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
        let round=2i32;let border=0xffff_fffeu32;let mode=i32::from(dark);
        DwmSetWindowAttribute(hwnd,33,(&round as *const i32).cast(),4);
        DwmSetWindowAttribute(hwnd,34,(&border as *const u32).cast(),4);
        DwmSetWindowAttribute(hwnd,20,(&mode as *const i32).cast(),4);
        relayout(hwnd);
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
    fn start_panel_height_tracks_content_and_keeps_the_footer_inside_its_envelope(){
        let sparse=flow(true,2,0,false);
        let filled=flow(true,GRID_PAGE,4,false);
        assert!(sparse.height+100<filled.height);
        assert!(flow(false,1,0,false).height<flow(false,LIST_PAGE,0,false).height);
        for grid in [true,false]{
            for visible in 0..=if grid{GRID_PAGE}else{LIST_PAGE}{
                for recent in 0..=4{
                    let f=flow(grid,visible,recent,true);
                    assert!(f.footer_y+31<=f.height&&f.height<=MAX_HEIGHT);
                }
            }
        }
    }
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
            relayout(host);
            if let Some(path)=std::env::var_os("TUANPC_DESK_START_EVIDENCE").map(PathBuf::from){
                START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){
                    m.glass=Rc::from([]);m.pins=vec!["mspaint.exe".into(),"notepad.exe".into(),"deskland:settings".into()];
                });
                CAPTURE.with(|c|c.set(true));relayout(host);UpdateWindow(host);CAPTURE.with(|c|c.set(false));
                assert!(path.is_file(),"native Start render evidence is required");
                START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){m.pins=vec![r"C:\Test\Notepad.exe".into()];});
                relayout(host);
            }
            assert_ne!(IsWindowVisible(host),0);
            assert!(handles().is_none(),"opening Start must not start Spotlight");
            let mut rc:RECT=std::mem::zeroed();
            assert_ne!(GetWindowRect(host,&mut rc),0);
            assert!(rc.left>=display.bounds.left&&rc.top>=display.bounds.top&&rc.right<=display.bounds.right&&rc.bottom<=display.bounds.bottom);
            START.with(|s|assert_eq!(selected(s.borrow().as_ref().unwrap()).unwrap().text,"Notepad"));
            SetWindowTextW(edit,wide("paint").as_ptr());
            START.with(|s|assert_eq!(selected(s.borrow().as_ref().unwrap()).unwrap().text,"Paint"));
            let mut search:RECT=std::mem::zeroed();GetWindowRect(host,&mut search);
            assert!(search.bottom-search.top < rc.bottom-rc.top);
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
