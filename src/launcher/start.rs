//! Dock Start has a separate window, layout and launch history. Only the
//! app catalog, Shell launching and icon extraction are shared with Spotlight.
use super::*;
use crate::dock::{panel_chrome as chrome,PopupPalette,PopupPixels};
use std::{collections::{HashMap,HashSet},ptr,rc::Rc,sync::mpsc::{self,Receiver},time::{Duration,Instant}};

const CLASS:&str="TuanpcDeskStartPanel";
#[path = "start_layout.rs"]
mod layout;
use layout::*;
#[path = "start_account.rs"]
mod account;
const EDIT_ID:usize=101;
const APPLY:u32=WM_APP+115;
const POLL:usize=1;
const DISMISS_OUTSIDE:u32=WM_APP+116;
const FOLLOW_DOCK:u32=WM_APP+117;
const IDLE_TIMER:usize=2;
const FOCUS_TIMER:usize=3;
const IDLE_TIMEOUT:Duration=Duration::from_secs(20);
static VISIBLE_START:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);
const SETTINGS:&str="deskland:settings";

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum Area { Home, Apps }
#[derive(Clone,Debug,PartialEq,Eq)]
enum Hit { Home, Apps, Settings, Power, Restart, Page(i32), Explorer, Windows, TaskManager, Account, Index, Entry(usize) }
#[derive(Clone)]
struct Tile { rect:RECT, hit:Hit, text:String, entry:Option<Entry>, compact:bool, pinned:bool }
struct Model {
    hwnd:HWND, edit:HWND, owner:HWND, anchor:DockAnchor, work:RECT, dock_dpi:u32, scale:f32, frame:Frame, dark:bool, area:Area,
    all:Vec<Entry>, pins:Vec<String>, usage:Usage, query:String, page:usize, selected:usize,
    pressed:Option<(Hit,i32,i32)>, hover:Option<Hit>, keyboard:bool, view:StartView, error:String, glass:Rc<[u8]>, glass_height:i32, fonts:Fonts, avatar:Option<IconPixels>, avatar_rx:Option<Receiver<Option<IconPixels>>>, avatar_at:Option<Instant>,
    icons:HashMap<(PathBuf,i32),Option<IconPixels>>, loading:HashSet<(PathBuf,i32)>,
    icon_tx:IconRequests,icon_rx:Receiver<((PathBuf,i32),Option<IconPixels>)>,
    catalog:Option<Receiver<Vec<Entry>>>, indexed_at:Option<Instant>,
}
thread_local!{
    static CORNERS:RefCell<Option<chrome::StartCorners>>=const{RefCell::new(None)};
    static SHADOW:RefCell<Option<chrome::PanelShadow>>=const{RefCell::new(None)};
    static START:RefCell<Option<Model>>=const{RefCell::new(None)};
    static EDIT_PROC:Cell<WNDPROC>=const{Cell::new(None)};
    static POINTER_HOOK:Cell<HHOOK>=const{Cell::new(ptr::null_mut())};
    static ACTIVITY:Cell<Option<Instant>>=const{Cell::new(None)};
    static FOCUS:Cell<Option<(HWND,InputHandoff,Instant)>>=const{Cell::new(None)};
    static MENU:Cell<bool>=const{Cell::new(false)};
    #[cfg(test)] static CAPTURE:RefCell<Option<PathBuf>>=const{RefCell::new(None)};
}

#[derive(Default)]
struct Usage {
    recent: History,
    counts: HashMap<String,u32>,
}
impl Usage {
    fn key(path:&Path)->String{path.to_string_lossy().to_lowercase()}
    fn from_json(value:&serde_json::Value)->Self{
        let recent=History::from_json(value);
        let counts=recent.paths.iter().map(|path|{
            let key=Self::key(path);
            let count=value.get("counts").and_then(|v|v.get(&key)).and_then(|v|v.as_u64())
                .unwrap_or(1).clamp(1,u32::MAX as u64) as u32;
            (key,count)
        }).collect();
        Self{recent,counts}
    }
    fn remember(&mut self,path:&Path)->bool{
        let key=Self::key(path);
        if key.is_empty()||key.len()>32768||key==SETTINGS||key.starts_with("ms-settings:"){return false;}
        self.recent.remember(path);
        let n=self.counts.entry(key).or_default();
        *n=n.saturating_add(1);
        if self.counts.len()>HISTORY_LIMIT{
            self.counts.retain(|key,_|self.recent.paths.iter().any(|p|Self::key(p)==*key));
        }
        true
    }
    fn frequent(&self)->Vec<&PathBuf>{
        let mut paths=self.recent.paths.iter().collect::<Vec<_>>();
        // Stable sort: recent order breaks ties. Migrated launches count once.
        paths.sort_by_cached_key(|path|std::cmp::Reverse(self.counts.get(&Self::key(path)).copied().unwrap_or(1)));
        paths
    }
    fn json(&self)->serde_json::Value{
        serde_json::json!({"version":2,"items":self.recent.paths,"counts":self.counts})
    }
}
fn usage_load()->Usage{
    if cfg!(test){return Usage::default();}
    std::fs::read(crate::lock::data_dir().join("start-history.json")).ok()
        .and_then(|bytes|serde_json::from_slice(&bytes).ok()).map(|v|Usage::from_json(&v)).unwrap_or_default()
}
fn remember(model:&mut Model,path:&Path){
    if !model.usage.remember(path){return;}
    if !cfg!(test){
        let _=std::fs::write(crate::lock::data_dir().join("start-history.json"),model.usage.json().to_string());
    }
}
fn make_pin(value:&str)->Entry{
    let path=PathBuf::from(value);
    let name=if value==SETTINGS{"Cài đặt TLauncher".into()}
        else{crate::dock::command_pin::decode(value).map(|c|c.label)
            .unwrap_or_else(||path.file_stem().and_then(|s|s.to_str()).unwrap_or(value).replace('_'," "))};
    Entry{name,path,kind:"Pinned",detail:String::new()}
}
fn catalog_worker()->Option<Receiver<Vec<Entry>>>{
    catalog::request(false,false)
}
fn px(v:i32,scale:f32)->i32{(v as f32*scale).round() as i32}
fn rr(x:i32,y:i32,w:i32,h:i32,s:f32)->RECT{RECT{left:px(x,s),top:px(y,s),right:px(x+w,s),bottom:px(y+h,s)}}
struct StartView {
    tiles:Vec<Tile>, page:usize, count:usize, flow:Flow,
    columns:usize, index_label:String,
}

fn is_home(model:&Model)->bool{model.area==Area::Home&&model.query.trim().is_empty()}

fn content(model:&Model)->(Vec<std::borrow::Cow<'_,Entry>>,bool,[Vec<Entry>;2],usize,usize,usize,Flow){
    use std::borrow::Cow;
    let home=is_home(model);
    let grid=model.query.trim().is_empty();
    let pins=||model.pins.iter().map(|p|{
        model.all.iter().find(|a|same_path(&a.path,Path::new(p))).map(Cow::Borrowed)
            .unwrap_or_else(||Cow::Owned(make_pin(p)))
    });
    let entries:Vec<_>=if home{pins().collect()}
        else if grid{model.all.iter().map(Cow::Borrowed).collect()}
        else{
            // The catalog can contain hundreds of entries. Rank borrowed
            // entries and only copy visible results into the paint model.
            let q=model.query.trim().to_lowercase();
            let mut matches=model.all.iter().map(Cow::Borrowed)
                .chain(model.pins.iter().filter(|p|!model.all.iter().any(|a|same_path(&a.path,Path::new(p)))).map(|p|Cow::Owned(make_pin(p))))
                .filter_map(|e|{
                    let rank=catalog::search_rank(&e.name,&q)?;
                    Some((rank,e))
                }).collect::<Vec<_>>();
            matches.sort_by_key(|(rank,e)|(*rank,e.kind!="Application"));
            matches.into_iter().take(100).map(|(_,e)|e).collect()
        };
    let frame=model.frame;
    let error=!model.error.is_empty();
    let usage=frame.show_recents(home,error);
    let rows=if usage{
        let find=|path:&PathBuf|model.all.iter().find(|e|same_path(&e.path,path)).cloned()
            .or_else(||entries.iter().find(|e|same_path(&e.path,path)&&e.path!=Path::new(SETTINGS))
                .map(|e|e.as_ref().clone()));
        let recents=model.usage.recent.paths.iter().filter_map(&find).take(3).collect();
        let frequent=model.usage.frequent().into_iter().filter_map(find).take(3).collect();
        [recents,frequent]
    }else{[Vec::new(),Vec::new()]};
    let limit=frame.capacity(grid,usage,error);
    let pages=entries.len().div_ceil(limit).max(1);
    let page=model.page.min(pages-1);
    let flow=frame.flow(grid,entries.len().saturating_sub(page*limit).min(limit),usage,error);
    (entries,grid,rows,page,pages,limit,flow)
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
    for (i,hit,text) in [(0,Hit::Settings,"Cài đặt TLauncher"),(1,Hit::Account,"Tài khoản") ]{
        put(hit,text,(controls+i*CONTROL_STEP,HEADER_Y+2,CONTROL,CONTROL),None,true,false);
    }
    put(Hit::Home,"Đã ghim",(INSET,TABS_Y,66,26),None,true,false);
    put(Hit::Apps,"Tất cả ứng dụng",(INSET+72,TABS_Y,112,26),None,true,false);
    let (entries,grid,rows,page,pages,limit,flow)=content(model);
    let count=entries.len();
    let y=flow.footer_y+7;
    let power_x=frame.width-INSET-POWER;
    put(Hit::Power,"Nguồn",(power_x,flow.footer_y+(FOOTER_H-POWER)/2,POWER,POWER),None,true,false);
    let restart_x=frame.width-INSET-POWER_CLUSTER;
    put(Hit::Restart,"Khởi động lại",(restart_x,flow.footer_y+(FOOTER_H-RESTART)/2,RESTART,RESTART),None,true,false);
    let edge=restart_x-POWER_GAP;
    let index_room=frame.width>=350;
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
        put(Hit::Entry(i),&entry.name,frame.app(i,grid),Some(entry.as_ref().clone()),!grid,is_home(model));
    }
    for (row,(y,entries)) in [flow.recent_y,flow.frequent_y].into_iter().zip(rows).enumerate(){
        if let Some(y)=y{
            let cols=entries.len().max(1) as i32;
            let w=frame.inner()/cols;
            for (i,entry) in entries.into_iter().enumerate(){
                let x=INSET+i as i32*w;
                let title=entry.name.clone();
                put(Hit::Entry(limit+3*row+i),&title,(x,y+21,w-4,32),Some(entry),true,false);
            }
        }
    }
    for (i,hit,label) in [(0,Hit::Explorer,"File Explorer"),(1,Hit::Windows,"Cài đặt Windows"),(2,Hit::TaskManager,"Task Manager")]{
        put(hit,label,(INSET+i*28,y,26,30),None,true,false);
    }
    StartView{tiles,page,count,flow,columns:frame.columns(grid),index_label}
}

fn selected(model:&Model)->Option<Tile>{
    model.view.tiles.iter().filter(|t|matches!(t.hit,Hit::Entry(_))).nth(model.selected).cloned()
}
fn repaint(hwnd:HWND){unsafe{InvalidateRect(hwnd,ptr::null(),0);}}
pub(crate) fn follow_dock(anchor:DockAnchor){
    let target=VISIBLE_START.load(std::sync::atomic::Ordering::Relaxed) as HWND;
    if !target.is_null(){
        let (w,l)=anchor.encode();
        unsafe{PostMessageW(target,FOLLOW_DOCK,w,l);}
    }
}
unsafe fn place(hwnd:HWND){
    let geometry=START.with(|s|{
        let b=s.borrow();let m=b.as_ref()?;
        let (w,h)=(px(m.frame.width,m.scale),px(m.view.flow.height,m.scale));
        if w>m.work.right-m.work.left||h>m.work.bottom-m.work.top{return None;}
        let (x,y)=m.anchor.position(m.work,w,h,chrome::scale(m.dock_dpi,chrome::PANEL_GAP));
        Some((RECT{left:x,top:y,right:x+w,bottom:y+h},m.dock_dpi,m.anchor.side,m.dark))
    });
    let Some((r,dpi,side,dark))=geometry else{if IsWindowVisible(hwnd)!=0{hide(hwnd);}return;};
    let mut previous:RECT=std::mem::zeroed();
    GetWindowRect(hwnd,&mut previous);
    let size_changed=r.right-r.left!=previous.right-previous.left||r.bottom-r.top!=previous.bottom-previous.top;
    if size_changed||r.left!=previous.left||r.top!=previous.top{
        SetWindowPos(hwnd,ptr::null_mut(),r.left,r.top,r.right-r.left,r.bottom-r.top,SWP_NOACTIVATE|SWP_NOZORDER);
        if size_changed{
            CORNERS.with(|c|c.replace(None));
            chrome::start_frame(hwnd,r.right-r.left,r.bottom-r.top,dpi);
        }else{
            CORNERS.with(|c|if let Some(edge)=c.borrow().as_ref(){edge.move_to(r.left,r.top);});
        }
    }
    if IsWindowVisible(hwnd)!=0{sync_shadow(hwnd,r,dpi,side,dark);}
}
unsafe fn sync_shadow(hwnd:HWND,rect:RECT,dpi:u32,side:crate::dock::DockSide,dark:bool){
    SHADOW.with(|s|chrome::PanelShadow::sync(&mut s.borrow_mut(),hwnd,ptr::null_mut(),rect,dpi,side,dark));
}
unsafe fn relayout(hwnd:HWND){
    START.with(|s|{if let Some(m)=s.borrow_mut().as_mut(){m.view=build_view(m);}});
    place(hwnd);
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
enum Glyph{Search,Sliders,Power,Restart,Profile,Folder,Window,Activity}
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
            Glyph::Restart=>{
                let ring=if x>1.0&&y< -2.0{20.0}else{(x.hypot(y)-5.3).abs()};
                ring.min(line(5.4,-4.8,5.4,-0.4)).min(line(5.4,-0.4,1.6,-2.1))
            },
            Glyph::Profile=>((x.hypot(y+3.1))-2.5).abs().min(
                if y>=0.5{(x.hypot(y-6.8)-5.2).abs()}else{20.0}),
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
#[derive(Clone, Copy)]
struct Fonts { body:HFONT, small:HFONT, heading:HFONT }
impl Fonts{
    unsafe fn new(scale:f32)->Self{
        Self{body:start_font(px(14,scale),470),small:start_font(px(12,scale),470),heading:start_font(px(12,scale),600)}
    }
    unsafe fn delete(self){for font in [self.body,self.small,self.heading]{if !font.is_null(){DeleteObject(font);}}}
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
            let key=entry.map(|e|(e.path.clone(),px(if tile.compact{LIST_ICON}else{GRID_ICON},m.scale).max(16)));
            let pixels=key.as_ref().and_then(|key|m.icons.get(key)).cloned().flatten();
            if let Some(key)=key{
                if key.0!=Path::new(SETTINGS)&&!m.icons.contains_key(&key)&&m.loading.len()<24&&m.loading.insert(key.clone()){
                    if m.icon_tx.send(key.clone()).is_err(){m.loading.remove(&key);}
                }
            }
            icons.push(if tile.hit==Hit::Account{m.avatar.clone()}else{pixels});
        }
        let selected=selected.filter(|_|m.keyboard||!m.query.is_empty());
        Some((m.scale,m.frame,m.dark,m.glass.clone(),tiles,icons,selected,m.hover.clone(),m.area,m.query.trim().is_empty(),
            m.error.clone(),m.view.count,m.catalog.is_some()||!m.loading.is_empty()||m.avatar_rx.is_some(),m.view.flow,m.view.index_label.clone(),m.anchor,m.work,m.dock_dpi,m.fonts))
    });
    if let Some((s,frame,dark,glass,tiles,icons,selected,hover,area,empty_query,error,count,pending,flow,index_label,anchor,work,dock_dpi,fonts))=view{
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
                let (_,start)=anchor.position(work,width,envelope,chrome::scale(dock_dpi,chrome::PANEL_GAP));
                let (_,current)=anchor.position(work,width,height,chrome::scale(dock_dpi,chrome::PANEL_GAP));
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
            let Fonts{body,small,heading}=fonts;
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
                center_label(dc,&index_label,rr(frame.width-INSET-POWER_CLUSTER-POWER_GAP-79,flow.footer_y+7,52,30,s),small,sub);
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
                    Hit::Restart=>Some(Glyph::Restart),Hit::Explorer=>Some(Glyph::Folder),Hit::Windows=>Some(Glyph::Window),
                    Hit::TaskManager=>Some(Glyph::Activity),_=>None,
                };
                let header=matches!(tile.hit,Hit::Settings|Hit::Account);
                if tile.hit==Hit::Power{
                    let fill=if dark{[72,63,233]}else{[59,58,196]};
                    surface.shape(tile.rect,13.0*s,fill,if hovered{255}else{218});
                    glyph(&mut surface,tile.rect,Glyph::Power,[251,249,255],s*1.24);
                    continue;
                }
                if picked||hovered||header||tile.hit==Hit::Restart{
                    surface.shape(tile.rect,if header{16.0*s}else{12.0*s},if picked{select_ink}else{tile_ink},if picked||hovered{188}else{110});
                }
                if let Some(symbol)=symbol{glyph(&mut surface,tile.rect,symbol,palette.ink,s);continue;}
                if tile.hit==Hit::Account{
                    if let Some(img)=img{
                        icon(dc,img,(tile.rect.left+tile.rect.right-img.width)/2,(tile.rect.top+tile.rect.bottom-img.height)/2);
                    }else{glyph(&mut surface,tile.rect,Glyph::Profile,palette.ink,s*1.15);}
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
            for (y,title) in [(flow.recent_y,"Đã mở gần đây"),(flow.frequent_y,"Dùng nhiều nhất")]{
                if let Some(y)=y{
                    label(dc,title,rr(INSET,y,frame.inner(),21,s),heading,sub);
                    if !tiles.iter().any(|tile|tile.entry.is_some()&&tile.rect.top==px(y+21,s)){
                        label(dc,"Chưa có lượt mở từ Start",rr(INSET,y+22,frame.inner(),30,s),small,sub);
                    }
                }
            }
            if !error.is_empty(){label(dc,&error,rr(INSET,flow.footer_y-ERROR_H,frame.inner(),ERROR_H,s),small,fg);}
            fill(dc,&rr(INSET,flow.footer_y,frame.inner(),1,s),color(tile_ink));
            if let Some(tile)=tiles.iter().find(|t|hover.as_ref()==Some(&t.hit)&&
                matches!(t.hit,Hit::Explorer|Hit::Windows|Hit::TaskManager|Hit::Settings|Hit::Power|Hit::Restart|Hit::Account)){
                let hint=if tile.hit==Hit::Account{std::env::var("USERNAME").unwrap_or_else(|_|"Tài khoản".into())}else{tile.text.clone()};
                let hint_width=(frame.inner()-210-POWER_CLUSTER-POWER_GAP).max(0);
                if hint_width>=60{label(dc,&hint,rr(INSET+102,flow.footer_y+7,hint_width,30,s),small,sub);}
            }
            #[cfg(test)]
            CAPTURE.with(|c|{
                if let Some(path)=c.borrow_mut().take(){capture_render(dc,width,height,&path).expect("native Start fixture ink");}
            });
            if IsWindowVisible(hwnd)!=0{
                CORNERS.with(|c|chrome::StartCorners::paint(&mut c.borrow_mut(),hwnd,dc,width,height,dock_dpi));
            }
            surface.blit(target);

        }
        if pending{SetTimer(hwnd,POLL,70,None);}
    }
    EndPaint(hwnd,&ps);
}

fn touch(){ACTIVITY.set(Some(Instant::now()));}

fn idle_expired(last:Option<Instant>,now:Instant,modal:bool)->bool{
    !modal&&last.is_some_and(|last|now.saturating_duration_since(last)>=IDLE_TIMEOUT)
}

/// Observe only mouse presses while Start is visible. Never capture or consume
/// the click: it must still reach the application, including no-activate panels.
unsafe extern "system" fn observe_press(code:i32,w:WPARAM,l:LPARAM)->LRESULT{
    if code==HC_ACTION as i32&&matches!(w as u32,WM_LBUTTONDOWN|WM_RBUTTONDOWN|WM_MBUTTONDOWN|WM_XBUTTONDOWN)
        && !MENU.get(){
        let hwnd=VISIBLE_START.load(std::sync::atomic::Ordering::Relaxed) as HWND;
        if !hwnd.is_null(){
            let point=(*(l as *const MSLLHOOKSTRUCT)).pt;
            let hit=WindowFromPoint(point);
            if hit==hwnd||IsChild(hwnd,hit)!=0{touch();}
            // Dock/Stage left presses are handled by their own window proc,
            // so a press on the Start button can still toggle this panel.
            else if w as u32!=WM_LBUTTONDOWN||!crate::dock::is_input_surface(hit){
                PostMessageW(hwnd,DISMISS_OUTSIDE,0,0);
            }
        }
    }
    CallNextHookEx(ptr::null_mut(),code,w,l)
}

pub(crate) fn dismiss_from_dock(){
    let hwnd=VISIBLE_START.load(std::sync::atomic::Ordering::Relaxed) as HWND;
    if !hwnd.is_null(){unsafe{PostMessageW(hwnd,DISMISS_OUTSIDE,0,0);}}
}
unsafe fn stop_dismissal(hwnd:HWND){
    VISIBLE_START.store(0,std::sync::atomic::Ordering::Relaxed);
    let hook=POINTER_HOOK.replace(ptr::null_mut());
    if !hook.is_null(){UnhookWindowsHookEx(hook);}
    KillTimer(hwnd,IDLE_TIMER);
    KillTimer(hwnd,POLL);
    ACTIVITY.set(None);
}
unsafe fn hide(hwnd:HWND){
    FOCUS.set(None);KillTimer(hwnd,FOCUS_TIMER);
    stop_dismissal(hwnd);
    SHADOW.with(|s|s.replace(None));
    CORNERS.with(|c|c.replace(None));
    ShowWindow(hwnd,SW_HIDE);
    SetWindowPos(hwnd,HWND_NOTOPMOST,0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE);
}
unsafe fn search_focus(hwnd:HWND,edit:HWND){
    if GetForegroundWindow()!=hwnd{return;}
    if let Some((_,input,_))=FOCUS.take(){input.finish(hwnd,"Start");}
    KillTimer(hwnd,FOCUS_TIMER);
    if GetFocus()!=edit{SetFocus(edit);}
}
unsafe fn show(hwnd:HWND,edit:HWND){
    let input=super::InputHandoff::capture();
    if SetWindowPos(hwnd,HWND_TOPMOST,0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_SHOWWINDOW|SWP_NOACTIVATE)==0{return;}
    // Foreground permission is best effort; TOPMOST still makes Start visible
    // even when Windows declines focus for an asynchronous Win-key request.
    let previous=GetForegroundWindow();
    FOCUS.set(Some((previous,input,Instant::now())));
    SetForegroundWindow(hwnd);
    if IsWindowVisible(hwnd)==0{FOCUS.set(None);return;}
    place(hwnd);
    if IsWindowVisible(hwnd)==0{return;}
    search_focus(hwnd,edit);
    if FOCUS.get().is_some(){SetTimer(hwnd,FOCUS_TIMER,40,None);}
    touch();
    VISIBLE_START.store(hwnd as usize,std::sync::atomic::Ordering::Relaxed);
    SetTimer(hwnd,IDLE_TIMER,250,None);
    if POINTER_HOOK.get().is_null(){
        let module=windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(ptr::null());
        let hook=SetWindowsHookExW(WH_MOUSE_LL,Some(observe_press),module,0);
        if hook.is_null(){crate::log!("DOCK_START outside-click observer unavailable: {}",GetLastError());}
        POINTER_HOOK.set(hook);
    }
}
unsafe fn restore_after_error(hwnd:HWND){
    if let Some(edit)=START.with(|s|s.borrow().as_ref().map(|m|m.edit)){
        relayout(hwnd);show(hwnd,edit);
    }
}
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
unsafe fn launch(hwnd:HWND,entry:Entry,track:bool){
    let owner=START.with(|s|s.borrow().as_ref().map(|m|m.owner)).unwrap_or_default();
    if entry.path==Path::new(SETTINGS){hide(hwnd);PostMessageW(owner,crate::WM_OPEN_SETTINGS,0,0);return;}
    hide(hwnd);
    let code=execute(hwnd,&entry.path);
    if code>32{
        if !track{return;}
        START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){remember(m,&entry.path);});
    }else{
        START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){m.error=format!("Không mở được {} (Windows {code})",entry.name);});
        restore_after_error(hwnd);
    }
}
unsafe fn action_menu(hwnd:HWND,point:POINT,actions:&[&str],align:u32)->u32{
    let menu=CreatePopupMenu();if menu.is_null(){return 0;}
    for (i,text) in actions.iter().enumerate(){AppendMenuW(menu,MF_STRING,i+1,wide(text).as_ptr());}
    MENU.with(|flag|flag.set(true));
    let dark=START.with(|s|s.borrow().as_ref().is_some_and(|m|m.dark));
    let transparency=PREFS.with(|p|(p.borrow().transparency*100.0) as u32);
    let result=crate::dock::glass_menu::track(menu,hwnd,point,TPM_RETURNCMD|TPM_NONOTIFY|TPM_RIGHTBUTTON|align,dark,transparency);
    DestroyMenu(menu);MENU.with(|flag|flag.set(false));touch();
    result as u32
}
#[derive(Clone,Copy)]
enum SessionAction{Lock,Sleep,Switch,SignOut,Restart,Shutdown}
impl SessionAction{
    fn label(self)->&'static str{
        match self{Self::Lock=>"Khóa máy",Self::Sleep=>"Ngủ",Self::Switch=>"Chuyển người dùng",
            Self::SignOut=>"Đăng xuất",Self::Restart=>"Khởi động lại",Self::Shutdown=>"Tắt máy"}
    }
}
unsafe fn session_action(hwnd:HWND,action:SessionAction){
    use SessionAction::*;
    if cfg!(test)||crate::trial::protect_windows().is_err(){
        START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){m.error="Bản thử không thực hiện thao tác nguồn hoặc phiên Windows.".into();});
        relayout(hwnd);return;
    }
    if matches!(action,Sleep|SignOut|Restart|Shutdown){
        MENU.set(true);
        let title=action.label();
        let confirm=MessageBoxW(hwnd,wide(&format!("{title}? Hãy lưu công việc đang mở trước khi tiếp tục.")).as_ptr(),
            wide("Xác nhận thao tác Windows").as_ptr(),MB_YESNO|MB_ICONWARNING|MB_DEFBUTTON2);
        MENU.set(false);touch();
        if confirm!=IDYES{return;}
    }
    hide(hwnd);
    let ok=match action{
        Lock=>windows_sys::Win32::System::Shutdown::LockWorkStation()!=0,
        Switch=>{
            use windows_sys::Win32::System::RemoteDesktop::{WTSDisconnectSession,WTS_CURRENT_SESSION};
            WTSDisconnectSession(ptr::null_mut(),WTS_CURRENT_SESSION,0)!=0
        }
        Sleep=>windows_sys::Win32::System::Power::SetSuspendState(0,0,0)!=0,
        SignOut|Restart|Shutdown=>{
            let args=match action{SignOut=>"/l",Restart=>"/r /t 0",_=>"/s /t 0"};
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
        START.with(|s|if let Some(m)=s.borrow_mut().as_mut(){m.error="Windows từ chối thao tác nguồn hoặc tài khoản.".into();});
        restore_after_error(hwnd);
    }
}
unsafe fn power(hwnd:HWND,r:RECT){
    let mut point=POINT{x:r.right,y:r.top};ClientToScreen(hwnd,&mut point);
    let actions=[SessionAction::Sleep,SessionAction::Shutdown];
    let choice=action_menu(hwnd,point,&actions.map(SessionAction::label),TPM_RIGHTALIGN|TPM_BOTTOMALIGN);
    if let Some(&action)=choice.checked_sub(1).and_then(|i|actions.get(i as usize)){session_action(hwnd,action);}
}
unsafe fn account_menu(hwnd:HWND,r:RECT){
    let mut point=POINT{x:r.right,y:r.bottom};ClientToScreen(hwnd,&mut point);
    let actions=[SessionAction::Lock,SessionAction::Switch,SessionAction::SignOut];
    match action_menu(hwnd,point,&["Cài đặt tài khoản",actions[0].label(),actions[1].label(),actions[2].label()],TPM_RIGHTALIGN){
        1=>launch(hwnd,Entry{name:"Cài đặt tài khoản".into(),path:"ms-settings:yourinfo".into(),kind:"Settings",detail:String::new()},false),
        choice=>if let Some(&action)=choice.checked_sub(2).and_then(|i|actions.get(i as usize)){session_action(hwnd,action);},
    }
}
unsafe fn activate(hwnd:HWND,tile:Tile){
    let open=|name:&str,path:&str|launch(hwnd,Entry{name:name.into(),path:path.into(),kind:"Application",detail:String::new()},false);
    match tile.hit{
        Hit::Entry(_)=>if let Some(entry)=tile.entry{launch(hwnd,entry,true);},
        Hit::Settings=>open("Cài đặt TLauncher",SETTINGS),
        Hit::Explorer=>open("File Explorer","explorer.exe"),
        Hit::Windows=>open("Cài đặt Windows","ms-settings:"),
        Hit::TaskManager=>open("Task Manager","Taskmgr.exe"),
        Hit::Power=>power(hwnd,tile.rect),
        Hit::Restart=>session_action(hwnd,SessionAction::Restart),
        Hit::Account=>account_menu(hwnd,tile.rect),
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
        1=>launch(hwnd,entry,true),
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
    if matches!(msg,WM_MOUSEMOVE|WM_KEYDOWN|WM_SYSKEYDOWN|WM_CHAR|WM_IME_COMPOSITION){touch();}
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
    if matches!(msg,WM_MOUSEMOVE|WM_KEYDOWN|WM_SYSKEYDOWN|WM_MOUSEWHEEL|WM_MOUSEHWHEEL|WM_LBUTTONDOWN|WM_RBUTTONUP|APPLY){touch();}
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
        WM_ACTIVATE if w&0xffff==WA_INACTIVE as usize=>{if !MENU.get()&&IsWindowVisible(hwnd)!=0{hide(hwnd);}0}
        WM_CLOSE|DISMISS_OUTSIDE=>{if !MENU.get()&&IsWindowVisible(hwnd)!=0{hide(hwnd);}0}
        WM_ACTIVATE=>{
            if let Some(edit)=START.with(|s|s.borrow().as_ref().map(|m|m.edit)){
                if FOCUS.get().is_some(){PostMessageW(hwnd,WM_TIMER,FOCUS_TIMER,0);}
                if GetFocus()==hwnd||FOCUS.get().is_some(){SetFocus(edit);}
            }
            0
        }
        FOLLOW_DOCK=>{
            let anchor=DockAnchor::decode(w,l);
            let update=IsWindowVisible(hwnd)!=0&&START.with(|s|{
                let mut slot=s.borrow_mut();
                let Some(m)=slot.as_mut()else{return false;};
                if m.anchor.same_position(anchor)||MonitorFromPoint(m.anchor.point,MONITOR_DEFAULTTONEAREST)!=
                    MonitorFromPoint(anchor.point,MONITOR_DEFAULTTONEAREST){return false;}
                let Some(work)=chrome::panel_work(anchor.point,anchor.side,chrome::scale(m.dock_dpi,chrome::PANEL_GAP))else{return false;};
                m.anchor=anchor;m.work=work;
                true
            });
            if update{place(hwnd);}
            0
        },
        WM_NCDESTROY=>{CORNERS.with(|c|c.replace(None));SHADOW.with(|s|s.replace(None));FOCUS.set(None);KillTimer(hwnd,FOCUS_TIMER);stop_dismissal(hwnd);DefWindowProcW(hwnd,msg,w,l)}
        WM_TIMER if w==FOCUS_TIMER=>{
            let foreground=GetForegroundWindow();
            if !MENU.get()&&IsWindowVisible(hwnd)!=0&&foreground==hwnd{
                if let Some(edit)=START.with(|s|s.borrow().as_ref().map(|m|m.edit)){search_focus(hwnd,edit);}
            }else if !MENU.get()&&IsWindowVisible(hwnd)!=0&&FOCUS.get().is_some_and(|(original,_,since)|
                foreground==original&&since.elapsed()<Duration::from_millis(180)){
                SetForegroundWindow(hwnd);
            }else{FOCUS.set(None);KillTimer(hwnd,FOCUS_TIMER);}
            0
        }
        WM_TIMER if w==IDLE_TIMER=>{
            if idle_expired(ACTIVITY.get(),Instant::now(),MENU.get()){hide(hwnd);}
            0
        }
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
                        Ok(mut apps)=>{fullscreen::merge_manual(&mut apps);m.all=apps;m.catalog=None;m.indexed_at=Some(Instant::now());changed=true;},
                        Err(mpsc::TryRecvError::Disconnected)=>m.catalog=None,
                        Err(mpsc::TryRecvError::Empty)=>{}
                    }
                }
                let new_catalog=changed;
                if let Some(rx)=&m.avatar_rx{
                    match rx.try_recv(){
                        Ok(avatar)=>{m.avatar=avatar;m.avatar_rx=None;changed=true;},
                        Err(mpsc::TryRecvError::Disconnected)=>m.avatar_rx=None,
                        Err(mpsc::TryRecvError::Empty)=>{}
                    }
                }
                while let Ok((key,icon))=m.icon_rx.try_recv(){
                    if m.icons.len()>=192{m.icons.clear();}
                    m.loading.remove(&key);m.icons.insert(key,icon);changed=true;
                }
                (changed,!m.loading.is_empty()||m.catalog.is_some()||m.avatar_rx.is_some(),new_catalog)
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
            let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW,cls.as_ptr(),wide("TLauncher Start").as_ptr(),WS_POPUP|WS_CLIPCHILDREN,
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
            START.with(|s|s.replace(Some(Model{hwnd,edit,owner,anchor,work:std::mem::zeroed(),dock_dpi:96,scale:1.0,frame:Frame::new(MAX_WIDTH,MAX_HEIGHT),dark:false,area:Area::Home,all:Vec::new(),
                pins:Vec::new(),usage:usage_load(),query:String::new(),page:0,selected:0,pressed:None,hover:None,keyboard:false,
                view:StartView{tiles:Vec::new(),page:0,count:0,columns:1,index_label:String::new(),flow:Frame::new(MAX_WIDTH,MAX_HEIGHT).flow(true,0,false,false)},
                error:String::new(),glass:Rc::from([]),glass_height:0,
                fonts:Fonts::new(1.0),avatar:None,avatar_rx:None,avatar_at:None,icons:HashMap::new(),loading:HashSet::new(),icon_tx,icon_rx,catalog:None,indexed_at:None})));
        }
        let Some((hwnd,edit))=START.with(|s|s.borrow().as_ref().map(|m|(m.hwnd,m.edit)))else{return;};
        let monitor=MonitorFromPoint(anchor.point,MONITOR_DEFAULTTONEAREST);
        let mut info:MONITORINFO=std::mem::zeroed();info.cbSize=std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor,&mut info)==0{return;}
        let _=SetWindowPos(hwnd,ptr::null_mut(),anchor.point.x,anchor.point.y,1,1,SWP_NOACTIVATE|SWP_NOZORDER);
        let dock_dpi=windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd).clamp(72,288);
        let gap=chrome::scale(dock_dpi,chrome::PANEL_GAP);
        let Some(work)=chrome::panel_work(anchor.point,anchor.side,gap)else{return;};
        let s=(dock_dpi as f32/96.0)
            .min((work.right-work.left) as f32/272.0).min((work.bottom-work.top) as f32/248.0).min(2.75);
        if s<=0.0{return;}
        let logical=|n:i32|(n as f32/s).floor() as i32;
        let frame=Frame::new(logical(work.right-work.left)-16,logical(work.bottom-work.top)-16);
        let (width,height)=(px(frame.width,s),px(frame.max_height(),s));
        let (x,y)=anchor.position(work,width,height,gap);
        let prefs=PREFS.with(|p|p.borrow().clone());
        let dark=prefs.theme=="dark";
        #[cfg(not(test))]
        let glass=frost(&capture_glass(x,y,width,height),width,height,s,&prefs);
        #[cfg(test)]
        let glass=Vec::<u8>::new();
        let fonts=Fonts::new(s);
        let pins=DOCK_PINS.with(|p|p.borrow().clone());
        let old_fonts=START.with(|sref|{
            let mut b=sref.borrow_mut();let m=b.as_mut().unwrap();
            m.scale=s;m.dock_dpi=dock_dpi;m.frame=frame;m.glass_height=height;m.anchor=anchor;m.work=work;m.owner=owner;m.dark=dark;m.pins=pins;m.area=Area::Home;m.query.clear();
            m.page=0;m.selected=0;m.pressed=None;m.hover=None;m.keyboard=false;m.error.clear();m.glass=glass.into();
            if m.catalog.is_none()&&m.indexed_at.is_none_or(|t|t.elapsed()>Duration::from_secs(180)){m.catalog=catalog_worker();}
            if !cfg!(test)&&m.avatar_rx.is_none()&&(m.avatar_at.is_none_or(|t|t.elapsed()>Duration::from_secs(300))||
                m.avatar.as_ref().is_some_and(|a|a.width!=px(28,s))){
                m.avatar_rx=account::worker(px(28,s));m.avatar_at=Some(Instant::now());
            }
            std::mem::replace(&mut m.fonts,fonts)
        });
        SendMessageW(edit,WM_SETFONT,fonts.body as usize,1);
        old_fonts.delete();
        SetWindowTextW(edit,wide("").as_ptr());
        MoveWindow(edit,px(INSET+34,s),px(HEADER_Y+8,s),px(frame.search_width()-45,s),px(20,s),0);
        use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
        let mode=i32::from(dark);
        DwmSetWindowAttribute(hwnd,20,(&mode as *const i32).cast(),4);
        relayout(hwnd);
        show(hwnd,edit);
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
    fn usage_migrates_and_separates_recency_from_frequency(){
        let mut usage=Usage::from_json(&serde_json::json!({"version":1,"items":["Paint.exe","Notepad.exe"]}));
        usage.remember(Path::new("notepad.exe"));
        usage.remember(Path::new("NOTEPAD.EXE"));
        usage.remember(Path::new("Paint.exe"));
        assert!(!usage.remember(Path::new(SETTINGS)));
        assert_eq!(Usage::key(&usage.recent.paths[0]),"paint.exe");
        assert_eq!(Usage::key(usage.frequent()[0]),"notepad.exe");
        let restored=Usage::from_json(&usage.json());
        assert_eq!(restored.counts["notepad.exe"],3);
        assert_eq!(restored.recent.paths.len(),2);
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
                let restart=m.view.tiles.iter().find(|t|t.hit==Hit::Restart).unwrap().rect;
                assert!(restart.right<power.left&&restart.top>=px(m.view.flow.footer_y,m.scale));
                assert!(m.view.flow.recent_y.unwrap()<m.view.flow.frequent_y.unwrap());
                assert!(power.top>=px(m.view.flow.footer_y,m.scale)&&power.right==account.right);
                assert!(power.right-power.left>account.right-account.left);
                assert!(hit_at(m,(power.left+power.right)/2,(power.top+power.bottom)/2).is_some_and(|t|t.hit==Hit::Power));
            });
            assert_ne!(IsWindowVisible(host),0);
            if GetForegroundWindow()==host{assert_eq!(GetFocus(),edit,"when Windows grants activation, typing must go to Search");}
            assert_ne!(GetWindowLongPtrW(host,GWL_EXSTYLE)&WS_EX_TOPMOST as isize,0);
            assert!(handles().is_none(),"opening Start must not start Spotlight");
            let mut rc:RECT=std::mem::zeroed();
            assert_ne!(GetWindowRect(host,&mut rc),0);
            assert!(rc.left>=display.bounds.left&&rc.top>=display.bounds.top&&rc.right<=display.bounds.right&&rc.bottom<=display.bounds.bottom);
            START.with(|s|assert_eq!(selected(s.borrow().as_ref().unwrap()).unwrap().text,"Notepad"));
            SetWindowTextW(edit,wide("paint").as_ptr());
            START.with(|s|assert_eq!(selected(s.borrow().as_ref().unwrap()).unwrap().text,"Paint"));
            let mut search:RECT=std::mem::zeroed();GetWindowRect(host,&mut search);
            assert!(search.bottom-search.top <= rc.bottom-rc.top);
            let following=DockAnchor{point:POINT{x:point.x,y:point.y-22},side:crate::dock::DockSide::Bottom};
            let (w,l)=following.encode();
            SendMessageW(host,FOLLOW_DOCK,w,l);
            let mut moved:RECT=std::mem::zeroed();GetWindowRect(host,&mut moved);
            assert_eq!(search.bottom-moved.bottom,22);
            START.with(|s|assert_eq!(s.borrow().as_ref().unwrap().query,"paint"));
            SendMessageW(host,APPLY,VK_RETURN as usize,0);
            assert_eq!(IsWindowVisible(host),0);
            START.with(|s|assert_eq!(s.borrow().as_ref().unwrap().usage.recent.paths,vec![PathBuf::from("mspaint.exe")]));
            let display=screens.get(1).unwrap_or(display);
            toggle(ptr::null_mut(),DockAnchor{point:POINT{x:display.bounds.left+90,y:display.bounds.top+150},side:crate::dock::DockSide::Left});
            assert_ne!(GetWindowRect(host,&mut rc),0);
            assert!(rc.left>=display.bounds.left&&rc.top>=display.bounds.top&&rc.right<=display.bounds.right&&rc.bottom<=display.bounds.bottom);
            ACTIVITY.set(Some(Instant::now()-IDLE_TIMEOUT));
            MENU.set(true);
            SendMessageW(host,WM_TIMER,IDLE_TIMER,0);
            assert_ne!(IsWindowVisible(host),0,"a modal action must survive the idle deadline");
            MENU.set(false);
            SendMessageW(host,WM_TIMER,IDLE_TIMER,0);
            assert_eq!(IsWindowVisible(host),0);
            assert_eq!(GetWindowLongPtrW(host,GWL_EXSTYLE)&WS_EX_TOPMOST as isize,0);
            assert!(POINTER_HOOK.get().is_null(),"hidden Start must release the click observer");
            let anchor=DockAnchor{point,side:crate::dock::DockSide::Bottom};
            toggle(ptr::null_mut(),anchor);
            assert_ne!(IsWindowVisible(host),0);
            SendMessageW(host,DISMISS_OUTSIDE,0,0);
            assert_eq!(IsWindowVisible(host),0);
            toggle(ptr::null_mut(),anchor);
            SendMessageW(host,APPLY,VK_ESCAPE as usize,0);
            assert_eq!(IsWindowVisible(host),0);
            DestroyWindow(host);
            START.with(|s|if let Some(m)=s.borrow_mut().take(){m.fonts.delete();});
        }
    }
}
