//! Dock Start has a separate window, layout and launch history. Only the
//! app catalog, Shell launching and icon extraction are shared with Spotlight.
use super::*;
use crate::dock::{PopupPalette,PopupPixels};
use std::{collections::{HashMap,HashSet},ptr,rc::Rc,sync::mpsc::{self,Receiver,Sender},time::{Duration,Instant}};

const CLASS:&str="TuanpcDeskStartPanel";
#[path = "start_layout.rs"]
mod layout;
use layout::*;
const EDIT_ID:usize=101;
const APPLY:u32=WM_APP+115;
const POLL:usize=1;
const SETTINGS:&str="deskland:settings";

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum Area { Home, Apps }
#[derive(Clone,Debug,PartialEq,Eq)]
enum Hit { Home, Apps, Settings, Power, Page(i32), Explorer, Windows, TaskManager, Account, Index, Entry(usize) }
#[derive(Clone)]
struct Tile { rect:RECT, hit:Hit, text:String, entry:Option<Entry>, compact:bool, pinned:bool }
struct Model {
    hwnd:HWND, edit:HWND, owner:HWND, anchor:DockAnchor, work:RECT, scale:f32, frame:Frame, dark:bool, area:Area,
    all:Vec<Entry>, pins:Vec<String>, recent:History, query:String, page:usize, selected:usize,
    pressed:Option<(Hit,i32,i32)>, hover:Option<Hit>, keyboard:bool, view:StartView, error:String, glass:Rc<[u8]>, glass_height:i32, edit_font:HFONT,
    icons:HashMap<(PathBuf,i32),Option<IconPixels>>, loading:HashSet<(PathBuf,i32)>,
    icon_tx:Sender<(PathBuf,i32)>,icon_rx:Receiver<((PathBuf,i32),Option<IconPixels>)>,
    catalog:Option<Receiver<Vec<Entry>>>, indexed_at:Option<Instant>,
}
thread_local!{
    static START:RefCell<Option<Model>>=const{RefCell::new(None)};
    static EDIT_PROC:Cell<WNDPROC>=const{Cell::new(None)};
    static MENU:Cell<bool>=const{Cell::new(false)};
    #[cfg(test)] static CAPTURE:RefCell<Option<PathBuf>>=const{RefCell::new(None)};
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
        Area::Apps=>(model.all.clone(),true),
        Area::Home=>(model.pins.iter().filter_map(|p|all.iter().find(|a|same_path(&a.path,Path::new(p))).cloned()).collect(),true),
    }
}
struct StartView {
    tiles:Vec<Tile>, page:usize, count:usize, flow:Flow,
    columns:usize, index_label:String,
}

fn is_home(model:&Model)->bool{model.area==Area::Home&&model.query.trim().is_empty()}

fn content(model:&Model)->(Vec<Entry>,bool,Vec<Entry>,usize,usize,usize,Flow){
    let all=entries_for(model);
    let (entries,grid)=entry_group(model,&all);
    let frame=model.frame;
    let error=!model.error.is_empty();
    let mut recent=if is_home(model){filter(&all,"",&model.recent).into_iter().take(3)
        .map(|i|all[i].clone()).collect::<Vec<_>>()}else{Vec::new()};
    if !frame.show_recents(!recent.is_empty(),error){recent.clear();}
    let limit=frame.capacity(grid,!recent.is_empty(),error);
    let pages=entries.len().div_ceil(limit).max(1);
    let page=model.page.min(pages-1);
    let flow=frame.flow(grid,entries.len().saturating_sub(page*limit).min(limit),!recent.is_empty(),error);
    (entries,grid,recent,page,pages,limit,flow)
}
fn initial(name:&str)->String{
    name.chars().next().map(|c|if c.is_alphabetic(){c.to_uppercase().collect()}else{"#".into()})
        .unwrap_or_else(||"#".into())
}
fn build_view(model:&Model)->StartView{
    let mut tiles=Vec::new();
    let s=model.scale;
    let frame=model.frame;
    let mut put=|hit,text:&str,r:(i32,i32,i32,i32),entry,compact,pinned|{
        tiles.push(Tile{hit,text:text.into(),rect:rr(r.0,r.1,r.2,r.3,s),entry,compact,pinned});
    };
    let controls=INSET+frame.search_width()+CONTROL_STEP-CONTROL;
    for (i,hit,text) in [(0,Hit::Settings,"Cài đặt Deskland"),(1,Hit::Account,"Tài khoản") ]{
        put(hit,text,(controls+i*CONTROL_STEP,HEADER_Y+2,CONTROL,CONTROL),None,true,false);
    }
    put(Hit::Home,"Đã ghim",(INSET,TABS_Y,66,26),None,true,false);
    put(Hit::Apps,"Tất cả ứng dụng",(INSET+72,TABS_Y,112,26),None,true,false);
    let (entries,grid,recent,page,pages,limit,flow)=content(model);
    let count=entries.len();
    let y=flow.footer_y+7;
    let power_x=frame.width-INSET-POWER;
    put(Hit::Power,"Nguồn và phiên",(power_x,flow.footer_y+(FOOTER_H-POWER)/2,POWER,POWER),None,true,false);
    let edge=power_x-POWER_GAP;
    let index_room=frame.width>=330;
    if page>0{put(Hit::Page(-1),"‹",(edge-if index_room{106}else{54},y,25,30),None,true,false);}
    if page+1<pages{put(Hit::Page(1),"›",(edge-25,y,25,30),None,true,false);}
    let index_label=if index_room&&model.area==Area::Apps&&grid&&count>0{
        let first=initial(&entries[page*limit].name);
        let last=initial(&entries[((page+1)*limit).min(count)-1].name);
        let title=if first==last{first}else{format!("{first}–{last}")};
        put(Hit::Index,&title,(edge-79,y,52,30),None,true,false);
        String::new()
    }else if index_room&&pages>1{format!("{} / {pages}",page+1)}else{String::new()};
    for (i,entry) in entries.iter().skip(page*limit).take(limit).enumerate(){
        put(Hit::Entry(i),&entry.name,frame.app(i,grid),Some(entry.clone()),!grid,is_home(model));
    }
    if let Some(y)=flow.recent_y{
        let cols=recent.len().max(1) as i32;
        for (i,entry) in recent.into_iter().enumerate(){
            let title=entry.name.clone();
            let w=frame.inner()/cols;
            put(Hit::Entry(limit+i),&title,(INSET+i as i32*w,y+24,w-4,34),Some(entry),true,false);
        }
    }
    for (i,hit,label) in [(0,Hit::Explorer,"File Explorer"),(1,Hit::Windows,"Cài đặt Windows"),(2,Hit::TaskManager,"Task Manager")]{
        put(hit,label,(INSET+i*32,y,28,30),None,true,false);
    }
    StartView{tiles,page,count,flow,columns:frame.columns(grid),index_label}
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
        let width=px(m.frame.width,m.scale);
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
pub(crate) fn icons_changed(){
    let hwnd=START.with(|s|{
        let mut slot=s.borrow_mut();
        let m=slot.as_mut()?;
        m.icons.clear();m.loading.clear();
        (m.icon_tx,m.icon_rx)=icon_worker();
        Some(m.hwnd)
    });
    if let Some(hwnd)=hwnd{unsafe{repaint(hwnd);}}
}
unsafe fn fill(dc:HDC,r:&RECT,color:u32){
    let brush=CreateSolidBrush(color);FillRect(dc,r,brush);DeleteObject(brush);
}
fn color(bgr:[u8;3])->u32{u32::from(bgr[2])|u32::from(bgr[1])<<8|u32::from(bgr[0])<<16}
fn search_ink(dark:bool)->[u8;3]{if dark{[58,57,54]}else{[235,232,228]}}
#[derive(Clone,Copy)]
enum Glyph{Search,Sliders,Power,Folder,Window,Activity}
unsafe fn glyph(surface:&mut PopupPixels,r:RECT,glyph:Glyph,ink:[u8;3],s:f32){
    let cx=(r.left+r.right) as f32*0.5;let cy=(r.top+r.bottom) as f32*0.5;
    let s=s.max(0.5);
    surface.shape_ink(r,ink,|x,y|{
        let (x,y)=((x-cx)/s,(y-cy)/s);
        let line=|ax:f32,ay:f32,bx:f32,by:f32|{
            let (dx,dy)=(bx-ax,by-ay);
            let t=(((x-ax)*dx+(y-ay)*dy)/(dx*dx+dy*dy)).clamp(0.0,1.0);
            (x-ax-t*dx).hypot(y-ay-t*dy)
        };
        let d=match glyph{
            Glyph::Search=>((x+1.5).hypot(y+1.5)-4.1).abs().min(line(1.6,1.8,5.5,5.7)),
            Glyph::Power=>{let ring=if y<0.0&&x.abs()<2.5{20.0}else{(x.hypot(y-0.7)-5.7).abs()};ring.min(line(0.0,-7.1,0.0,-0.3))},
            Glyph::Folder=>line(-6.5,-2.5,-6.5,4.0).min(line(-6.5,4.0,6.5,4.0))
                .min(line(6.5,4.0,6.5,-3.0)).min(line(6.5,-3.0,-0.3,-3.0))
                .min(line(-0.3,-3.0,-2.1,-5.0)).min(line(-2.1,-5.0,-6.5,-5.0))
                .min(line(-6.5,-5.0,-6.5,-2.5)),
            Glyph::Window=>line(-6.0,-5.0,6.0,-5.0).min(line(-6.0,5.0,6.0,5.0))
                .min(line(-6.0,-5.0,-6.0,5.0)).min(line(6.0,-5.0,6.0,5.0))
                .min(line(-6.0,-1.8,6.0,-1.8)),
            Glyph::Activity=>line(-6.0,1.5,-3.0,1.5).min(line(-3.0,1.5,-1.0,-3.8))
                .min(line(-1.0,-3.8,2.0,4.5)).min(line(2.0,4.5,4.0,0.0))
                .min(line(4.0,0.0,6.0,0.0)),
            Glyph::Sliders=>{
                let mut d=20.0f32;
                for (ty,tx) in [(-4.5,-2.0),(0.0,2.7),(4.5,-0.5)]{
                    d=d.min(line(-6.3,ty,tx-1.8,ty)).min(line(tx+1.8,ty,6.3,ty)).min(((x-tx).hypot(y-ty)-1.65).abs());
                }
                d
            }
        };
        ((0.85-d)*s+0.5).clamp(0.0,1.0)*255.0
    });
}
unsafe fn center_label(dc:HDC,text:&str,mut rect:RECT,font:HFONT,color:u32){
    if crate::native_text::draw(dc,rect,text,font,[(color>>16) as u8,(color>>8) as u8,color as u8],true,true){return;}

    let old=SelectObject(dc,font);SetTextColor(dc,color);SetBkMode(dc,TRANSPARENT as i32);
    DrawTextW(dc,wide(text).as_ptr(),-1,&mut rect,DT_SINGLELINE|DT_CENTER|DT_VCENTER|DT_END_ELLIPSIS|DT_NOPREFIX);
    SelectObject(dc,old);
}
unsafe fn label(dc:HDC,text:&str,r:RECT,font:HFONT,color:u32){
    if !crate::native_text::draw(dc,r,text,font,[(color>>16) as u8,(color>>8) as u8,color as u8],false,true){
        super::label(dc,text,r,font,color);
    }
}
unsafe fn start_font(size:i32,weight:i32)->HFONT{crate::native_text::font_pixels(size,weight)}
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
            let key=entry.map(|e|(e.path.clone(),px(if tile.compact{LIST_ICON}else{GRID_ICON},m.scale).max(16)));
            let pixels=key.as_ref().and_then(|key|m.icons.get(key)).cloned().flatten();
            if let Some(key)=key{
                if key.0!=Path::new(SETTINGS)&&!m.icons.contains_key(&key)&&m.loading.len()<24&&m.loading.insert(key.clone()){
                    if m.icon_tx.send(key.clone()).is_err(){m.loading.remove(&key);}
                }
            }
            icons.push(pixels);
        }
        let selected=selected.filter(|_|m.keyboard||!m.query.is_empty());
        Some((m.scale,m.frame,m.dark,m.glass.clone(),tiles,icons,selected,m.hover.clone(),m.area,m.query.trim().is_empty(),
            m.error.clone(),m.view.count,m.catalog.is_some()||!m.loading.is_empty(),m.view.flow,m.view.index_label.clone(),m.anchor,m.work))
    });
    if let Some((s,frame,dark,glass,tiles,icons,selected,hover,area,empty_query,error,count,pending,flow,index_label,anchor,work))=view{
        let width=px(frame.width,s);let height=px(flow.height,s);
        if let Some(mut surface)=PopupPixels::new(width,height){
            let dc=surface.dc();
            let palette=PopupPalette::new(dark);
            let neutral=if dark{[43,39,37]}else{[248,244,241]};
            let (bg,fg,sub)=(color(neutral),color(palette.ink),color(palette.muted));
            let tile_ink=if dark{[83,76,70]}else{[224,217,211]};
            let select_ink=if dark{[105,97,83]}else{[218,231,224]};
            fill(dc,&rr(0,0,frame.width,flow.height,s),bg);
            if !glass.is_empty(){
                // The sample dimensions stay fixed for this opening so search
                // contractions do not displace the material behind the panel.
                let full_height=glass.len()/4/(width/4).max(1) as usize;
                let envelope=START.with(|slot|slot.borrow().as_ref().map(|m|m.glass_height)).unwrap_or(height);
                let (_,start)=anchor.position(work,width,envelope,px(10,s));
                let (_,current)=anchor.position(work,width,height,px(10,s));
                let crop=((current-start)/4).max(0).min(full_height.saturating_sub(1) as i32);
                let mut info:BITMAPINFO=std::mem::zeroed();
                info.bmiHeader.biSize=std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                info.bmiHeader.biWidth=(width/4).max(1);info.bmiHeader.biHeight=-(full_height as i32);
                info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;
                SetStretchBltMode(dc,HALFTONE);
                StretchDIBits(dc,0,0,width,height,0,crop,info.bmiHeader.biWidth,(height/4).max(1).min(full_height as i32-crop),
                    glass.as_ptr().cast(),&info,DIB_RGB_COLORS,SRCCOPY);
            }else{
                GdiFlush();
                for (y,row) in surface.pixels().chunks_exact_mut(width as usize*4).enumerate(){
                    let t=y as f32/height as f32;
                    let lift=if dark{(1.0-t)*14.0}else{(1.0-t)*11.0-3.0};
                    let shade=neutral.map(|channel|(channel as f32+lift).clamp(0.0,255.0).round() as u8);
                    for p in row.chunks_exact_mut(4){p.copy_from_slice(&[shade[0],shade[1],shade[2],255]);}
                }
            }
            let body=start_font(px(14,s),470);
            let small=start_font(px(12,s),470);
            let heading=start_font(px(12,s),600);
            surface.shape(rr(INSET,HEADER_Y,frame.search_width(),HEADER_H,s),13.0*s,search_ink(dark),255);
            glyph(&mut surface,rr(INSET+2,HEADER_Y,30,HEADER_H,s),Glyph::Search,palette.muted,s);
            #[cfg(test)]
            if CAPTURE.with(|c|c.borrow().is_some()){
                let query=START.with(|s|s.borrow().as_ref().map(|m|m.query.clone()).unwrap_or_default());
                label(dc,if empty_query{"Tìm ứng dụng…"}else{&query},
                    rr(INSET+34,HEADER_Y,frame.search_width()-45,HEADER_H,s),body,if empty_query{sub}else{fg});
            }
            if !empty_query{label(dc,"Kết quả tìm kiếm",rr(INSET,TABS_Y,frame.inner(),26,s),heading,sub);}
            if !index_label.is_empty(){
                center_label(dc,&index_label,rr(frame.width-INSET-POWER-POWER_GAP-79,flow.footer_y+7,52,30,s),small,sub);
            }
            for (tile,img) in tiles.iter().zip(icons.iter()){
                let tab=matches!(tile.hit,Hit::Home|Hit::Apps);
                if tab&&!empty_query{continue;}
                let selected_tab=empty_query&&matches!((&tile.hit,area),(Hit::Home,Area::Home)|(Hit::Apps,Area::Apps));
                let picked=selected.as_ref()==Some(&tile.hit);
                let hovered=hover.as_ref()==Some(&tile.hit);
                if selected_tab{
                    let r=tile.rect;
                    surface.shape(RECT{left:r.left+px(5,s),right:r.right-px(5,s),top:r.bottom-px(3,s),bottom:r.bottom-px(1,s)},
                        s,if dark{[200,215,147]}else{[137,110,56]},255);
                }
                let symbol=match tile.hit{
                    Hit::Settings=>Some(Glyph::Sliders),Hit::Power=>Some(Glyph::Power),
                    Hit::Explorer=>Some(Glyph::Folder),Hit::Windows=>Some(Glyph::Window),
                    Hit::TaskManager=>Some(Glyph::Activity),_=>None,
                };
                let header=matches!(tile.hit,Hit::Settings|Hit::Account);
                if tile.hit==Hit::Power{
                    let fill=if dark{[72,63,233]}else{[59,58,196]};
                    surface.shape(tile.rect,13.0*s,fill,if hovered{255}else{218});
                    glyph(&mut surface,tile.rect,Glyph::Power,[251,249,255],s*1.24);
                    continue;
                }
                if picked||hovered||header{
                    surface.shape(tile.rect,if header{16.0*s}else{12.0*s},if picked{select_ink}else{tile_ink},if picked||hovered{188}else{110});
                }
                if let Some(symbol)=symbol{glyph(&mut surface,tile.rect,symbol,palette.ink,s);continue;}
                if tile.hit==Hit::Account{
                    let user=std::env::var("USERNAME").unwrap_or_else(|_|"Deskland".into());
                    center_label(dc,&initial(&user),tile.rect,heading,fg);
                    continue;
                }
                let mut r=tile.rect;
                r.left+=px(8,s);r.right-=px(5,s);
                if let Some(entry)=&tile.entry{
                    let size=px(if tile.compact{LIST_ICON}else{GRID_ICON},s);
                    let x=if tile.compact{r.left}else{(r.left+r.right-size)/2};
                    let y=if tile.compact{(r.top+r.bottom-size)/2}else{r.top+px(6,s)};
                    if let Some(img)=img{icon(dc,img,x+(size-img.width)/2,y+(size-img.height)/2);}
                    else if entry.path==Path::new(SETTINGS){DrawIconEx(dc,x,y,crate::app_icon(size,size),size,size,0,ptr::null_mut(),DI_NORMAL);}
                    else{label(dc,"◇",RECT{left:x,top:y,right:x+size,bottom:y+size},body,sub);}
                    if tile.compact{r.left+=size+px(8,s);}else{r=tile.rect;r.top+=px(51,s);}
                }
                if tile.entry.is_some()&&tile.compact{
                    label(dc,&tile.text,r,body,fg);
                }else{center_label(dc,&tile.text,if tile.entry.is_some(){r}else{tile.rect},small,fg);}
            }
            if count==0{
                let prompt=if !empty_query{"Không tìm thấy ứng dụng"}else if area==Area::Home{"Chưa có ứng dụng ghim. Mở Tất cả ứng dụng để ghim."}
                    else if pending{"Đang lấy danh sách ứng dụng…"}else{"Không có ứng dụng"};
                label(dc,prompt,rr(INSET,CONTENT_Y,frame.inner(),36,s),body,sub);
            }
            if let Some(y)=flow.recent_y{
                label(dc,"Đã mở gần đây",rr(INSET,y,frame.inner(),24,s),heading,sub);
            }
            if !error.is_empty(){label(dc,&error,rr(INSET,flow.footer_y-ERROR_H,frame.inner(),ERROR_H,s),small,fg);}
            fill(dc,&rr(INSET,flow.footer_y,frame.inner(),1,s),color(tile_ink));
            if let Some(tile)=tiles.iter().find(|t|hover.as_ref()==Some(&t.hit)&&
                matches!(t.hit,Hit::Explorer|Hit::Windows|Hit::TaskManager|Hit::Settings|Hit::Power|Hit::Account)){
                let hint=if tile.hit==Hit::Account{std::env::var("USERNAME").unwrap_or_else(|_|"Tài khoản".into())}else{tile.text.clone()};
                let hint_width=(frame.inner()-210-POWER-POWER_GAP).max(0);
                if hint_width>=60{label(dc,&hint,rr(INSET+102,flow.footer_y+7,hint_width,30,s),small,sub);}
            }
            #[cfg(test)]
            CAPTURE.with(|c|{
                if let Some(path)=c.borrow_mut().take(){capture_render(dc,width,height,&path).expect("native Start fixture ink");}
            });
            surface.blit(target);
            for f in [body,small,heading]{DeleteObject(f);}
        }
        if pending{SetTimer(hwnd,POLL,70,None);}
    }
    EndPaint(hwnd,&ps);
}

unsafe fn hide(hwnd:HWND){KillTimer(hwnd,POLL);ShowWindow(hwnd,SW_HIDE);}
fn hit_at(m:&Model,x:i32,y:i32)->Option<Tile>{
    m.view.tiles.iter().filter(|t|m.query.trim().is_empty()||!matches!(t.hit,Hit::Home|Hit::Apps)).find(|t|x>=t.rect.left&&x<t.rect.right&&y>=t.rect.top&&y<t.rect.bottom).cloned()
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
unsafe fn action_menu(hwnd:HWND,point:POINT,actions:&[&str],align:u32)->u32{
    let menu=CreatePopupMenu();if menu.is_null(){return 0;}
    for (i,text) in actions.iter().enumerate(){AppendMenuW(menu,MF_STRING,i+1,wide(text).as_ptr());}
    MENU.with(|flag|flag.set(true));
    let result=TrackPopupMenu(menu,TPM_RETURNCMD|TPM_NONOTIFY|TPM_RIGHTBUTTON|align,point.x,point.y,0,hwnd,ptr::null_mut());
    DestroyMenu(menu);MENU.with(|flag|flag.set(false));
    result as u32
}
unsafe fn power(hwnd:HWND,r:RECT){
    let mut point=POINT{x:r.right,y:r.top};ClientToScreen(hwnd,&mut point);
    let actions=["Khóa máy","Ngủ","Đăng xuất","Khởi động lại","Tắt máy"];
    let choice=action_menu(hwnd,point,&actions,TPM_RIGHTALIGN|TPM_BOTTOMALIGN);
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
        Hit::Account=>open("Tài khoản","ms-settings:yourinfo"),
        Hit::Index=>{
            let choices=START.with(|slot|{
                let state=slot.borrow();let Some(m)=state.as_ref()else{return Vec::new();};
                let (entries,_,_,_,_,limit,_)=content(m);
                let mut seen=HashSet::new();
                entries.iter().enumerate().filter_map(|(i,e)|{
                    let letter=initial(&e.name);
                    seen.insert(letter.clone()).then_some((letter,i/limit))
                }).collect::<Vec<_>>()
            });
            let labels=choices.iter().map(|(letter,_)|letter.as_str()).collect::<Vec<_>>();
            let mut point=POINT{x:tile.rect.left,y:tile.rect.bottom};ClientToScreen(hwnd,&mut point);
            let choice=action_menu(hwnd,point,&labels,0);
            if let Some((_,page))=choice.checked_sub(1).and_then(|i|choices.get(i as usize)){
                START.with(|slot|if let Some(m)=slot.borrow_mut().as_mut(){m.page=*page;m.selected=0;m.hover=None;});
                relayout(hwnd);
            }
        }
        Hit::Home|Hit::Apps|Hit::Page(_)=>{
            let edit=START.with(|s|{
                let mut b=s.borrow_mut();let m=b.as_mut()?;
                match tile.hit{
                    Hit::Home|Hit::Apps=>{m.area=if tile.hit==Hit::Home{Area::Home}else{Area::Apps};m.page=0;m.query.clear();}
                    Hit::Page(delta)=>{m.page=m.view.page.saturating_add_signed(delta as isize);}
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
    match action_menu(hwnd,at,&actions,0){
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
    // Keep Left/Right available for the native edit caret while searching.
    if msg==WM_KEYDOWN&&matches!(w as u16,VK_LEFT|VK_RIGHT)&&
        START.with(|s|s.borrow().as_ref().is_some_and(|m|m.query.trim().is_empty())){
        PostMessageW(GetParent(hwnd),APPLY,w,0);return 0;
    }
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
            let color=color(search_ink(dark));
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
                VK_LEFT|VK_RIGHT|VK_UP|VK_DOWN=>{
                    START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){
                        let count=m.view.tiles.iter().filter(|t|t.entry.is_some()).count();
                        m.keyboard=true;
                        let columns=m.view.tiles.iter().filter(|t|t.entry.is_some()&&!t.compact).count().min(m.view.columns).max(1);
                        let step=match w as u16{VK_LEFT=>-1,VK_RIGHT=>1,VK_UP=>-(columns as isize),_=>columns as isize};
                        m.selected=m.selected.saturating_add_signed(step).min(count.saturating_sub(1));
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
                    && tile.pinned&&m.view.tiles.iter().any(|t|t.hit==press&&t.pinned){
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
            START.with(|s|s.replace(Some(Model{hwnd,edit,owner,anchor,work:std::mem::zeroed(),scale:1.0,frame:Frame::new(MAX_WIDTH,MAX_HEIGHT),dark:false,area:Area::Home,all:Vec::new(),
                pins:Vec::new(),recent:recent_load(),query:String::new(),page:0,selected:0,pressed:None,hover:None,keyboard:false,
                view:StartView{tiles:Vec::new(),page:0,count:0,columns:1,index_label:String::new(),flow:Frame::new(MAX_WIDTH,MAX_HEIGHT).flow(true,0,false,false)},
                error:String::new(),glass:Rc::from([]),glass_height:0,
                edit_font:ptr::null_mut(),icons:HashMap::new(),loading:HashSet::new(),icon_tx,icon_rx,catalog:None,indexed_at:None})));
        }
        let Some((hwnd,edit))=START.with(|s|s.borrow().as_ref().map(|m|(m.hwnd,m.edit)))else{return;};
        let monitor=MonitorFromPoint(anchor.point,MONITOR_DEFAULTTONEAREST);
        let mut info:MONITORINFO=std::mem::zeroed();info.cbSize=std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor,&mut info)==0{return;}
        let work=info.rcWork;
        let _=SetWindowPos(hwnd,ptr::null_mut(),work.left,work.top,1,1,SWP_NOACTIVATE|SWP_NOZORDER);
        let s=(windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd).clamp(72,288) as f32/96.0)
            .min((work.right-work.left) as f32/272.0).min((work.bottom-work.top) as f32/248.0).min(2.75);
        if s<=0.0{return;}
        let logical=|n:i32|(n as f32/s).floor() as i32;
        let frame=Frame::new(logical(work.right-work.left)-16,logical(work.bottom-work.top)-16);
        let (width,height)=(px(frame.width,s),px(frame.max_height(),s));
        let (x,y)=anchor.position(work,width,height,px(10,s));
        let prefs=PREFS.with(|p|p.borrow().clone());
        let dark=prefs.theme=="dark";
        #[cfg(not(test))]
        let glass=frost(&capture_glass(x,y,width,height),width,height,s,&prefs);
        #[cfg(test)]
        let glass=Vec::<u8>::new();
        let font=start_font(px(14,s),470);
        let pins=DOCK_PINS.with(|p|p.borrow().clone());
        let old_font=START.with(|sref|{
            let mut b=sref.borrow_mut();let m=b.as_mut().unwrap();
            m.scale=s;m.frame=frame;m.glass_height=height;m.anchor=anchor;m.work=work;m.owner=owner;m.dark=dark;m.pins=pins;m.area=Area::Home;m.query.clear();
            m.page=0;m.selected=0;m.pressed=None;m.hover=None;m.keyboard=false;m.error.clear();m.glass=glass.into();m.recent=recent_load();
            if m.catalog.is_none()&&m.indexed_at.is_none_or(|t|t.elapsed()>Duration::from_secs(180)){m.catalog=catalog_worker();}
            std::mem::replace(&mut m.edit_font,font)
        });
        SendMessageW(edit,WM_SETFONT,font as usize,1);
        if !old_font.is_null(){DeleteObject(old_font);}
        SetWindowTextW(edit,wide("").as_ptr());
        MoveWindow(edit,px(INSET+34,s),px(HEADER_Y+8,s),px(frame.search_width()-45,s),px(20,s),0);
        use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
        let round=2i32;let border=0xffff_fffeu32;let mode=i32::from(dark);
        DwmSetWindowAttribute(hwnd,33,(&round as *const i32).cast(),4);
        DwmSetWindowAttribute(hwnd,34,(&border as *const u32).cast(),4);
        DwmSetWindowAttribute(hwnd,20,(&mode as *const i32).cast(),4);
        relayout(hwnd);
        let input=super::InputHandoff::capture();
        SetWindowPos(hwnd,ptr::null_mut(),0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOZORDER|SWP_SHOWWINDOW|SWP_NOACTIVATE);
        SetForegroundWindow(hwnd);SetFocus(edit);
        input.finish(hwnd,"Start");
        repaint(hwnd);
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
            relayout(host);
            if let Some(path)=std::env::var_os("TUANPC_DESK_START_EVIDENCE").map(PathBuf::from){
                let root=PathBuf::from(std::env::var_os("SystemRoot").expect("Windows directory"));
                let fixtures=[("File Explorer",root.join("explorer.exe")),
                    ("Notepad",root.join("System32").join("notepad.exe")),
                    ("Task Manager",root.join("System32").join("taskmgr.exe"))];
                use windows::Win32::System::Com::{CoInitializeEx,CoUninitialize,COINIT_APARTMENTTHREADED};
                let com=CoInitializeEx(None,COINIT_APARTMENTTHREADED);
                let capture=|path:PathBuf|{
                    CAPTURE.with(|c|{c.replace(Some(path.clone()));});
                    relayout(host);UpdateWindow(host);
                    assert!(path.is_file(),"native Start render evidence is required: {}",path.display());
                    START.with(|s|if let Some(m)=s.borrow().as_ref(){
                        let (_,grid,_,_,_,_,flow)=content(m);
                        println!("start geometry: file={} dpi_scale={:.2} logical={}x{} content={} columns={} icon={} row={} h_gap={} header={} footer={} entries={}/{} font=Segoe_UI_Variable text_dp=12,14 dark={}",
                            path.file_name().unwrap().to_string_lossy(),m.scale,m.frame.width,flow.height,flow.footer_y-CONTENT_Y,
                            m.view.columns,if grid{GRID_ICON}else{LIST_ICON},if grid{GRID_ROW}else{LIST_ROW},GRID_GAP,HEADER_H,FOOTER_H,
                            m.view.tiles.iter().filter(|t|t.entry.is_some()).count(),m.view.count,m.dark);
                    });
                };
                START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){
                    m.glass=Rc::from([]);m.pins=fixtures.iter().map(|(_,p)|p.to_string_lossy().into_owned()).chain([SETTINGS.into()]).collect();
                    m.all=fixtures.iter().map(|(name,path)|Entry{name:(*name).into(),path:path.clone(),kind:"Application",detail:String::new()}).collect();
                    for (_,path) in &fixtures{
                        for size in [px(GRID_ICON,m.scale).max(16),px(LIST_ICON,m.scale).max(16)]{
                            m.icons.insert((path.clone(),size),extract_icon(path,size));
                        }
                    }
                    assert!(m.icons.values().any(Option::is_some),"real Shell icon at least once in evidence");
                    m.dark=false;
                });
                if com.is_ok(){CoUninitialize();}
                capture(path.clone());
                START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){m.dark=true;});
                capture(path.with_file_name("start-native-dark.png"));
                START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){m.dark=false;m.area=Area::Apps;});
                capture(path.with_file_name("start-native-apps.png"));
                SetWindowTextW(edit,wide("note").as_ptr());
                capture(path.with_file_name("start-native-search.png"));
                SetWindowTextW(edit,wide("").as_ptr());
                START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){
                    m.area=Area::Home;m.pins=vec![r"C:\Test\Notepad.exe".into()];
                    m.all=vec![Entry{name:"Paint".into(),path:"mspaint.exe".into(),kind:"Application",detail:String::new()}];
                });
                relayout(host);
            }
            START.with(|s|{
                let s=s.borrow();
                let m=s.as_ref().unwrap();
                let power=m.view.tiles.iter().find(|t|t.hit==Hit::Power).unwrap().rect;
                let account=m.view.tiles.iter().find(|t|t.hit==Hit::Account).unwrap().rect;
                assert!(power.top>=px(m.view.flow.footer_y,m.scale)&&power.right==account.right);
                assert!(power.right-power.left>account.right-account.left);
                assert!(hit_at(m,(power.left+power.right)/2,(power.top+power.bottom)/2).is_some_and(|t|t.hit==Hit::Power));
            });
            assert_ne!(IsWindowVisible(host),0);
            assert!(handles().is_none(),"opening Start must not start Spotlight");
            let mut rc:RECT=std::mem::zeroed();
            assert_ne!(GetWindowRect(host,&mut rc),0);
            assert!(rc.left>=display.bounds.left&&rc.top>=display.bounds.top&&rc.right<=display.bounds.right&&rc.bottom<=display.bounds.bottom);
            START.with(|s|assert_eq!(selected(s.borrow().as_ref().unwrap()).unwrap().text,"Notepad"));
            SetWindowTextW(edit,wide("paint").as_ptr());
            START.with(|s|assert_eq!(selected(s.borrow().as_ref().unwrap()).unwrap().text,"Paint"));
            let mut search:RECT=std::mem::zeroed();GetWindowRect(host,&mut search);
            assert!(search.bottom-search.top <= rc.bottom-rc.top);
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
