//! Native Control Center cards. Updates only for input or a visible metrics view.
use super::{panel_chrome as chrome, surface::{self,Palette},geometry::Rect,glass_probe::Layer,UtilityWidget};
use crate::{desktop::wide,system_monitor::{Feed,Snapshot}};
use std::{cell::{Cell,RefCell},mem,ptr,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}}};
use windows_sys::Win32::{
    Foundation::*,Graphics::Gdi::*,
    System::{LibraryLoader::GetModuleHandleW,Threading::{TrySubmitThreadpoolCallback,PTP_CALLBACK_INSTANCE}},
    UI::{Input::KeyboardAndMouse::{GetCapture,GetKeyState,ReleaseCapture,SetCapture,SetFocus,TrackMouseEvent,TRACKMOUSEEVENT,TME_LEAVE,
        VK_DOWN,VK_END,VK_ESCAPE,VK_HOME,VK_LEFT,VK_RETURN,VK_RIGHT,VK_SHIFT,VK_SPACE,VK_TAB,VK_UP},
        WindowsAndMessaging::*},
};
use super::utilities::Action;
pub(super) const CLASS:&str="TuanpcDeskControlCenter";
const METRIC_UPDATE:u32=WM_APP+99;
const METRIC_TIMER:usize=1;
const ENTRANCE_TIMER:usize=2;
const SINGLE_LINE:u32=DT_SINGLELINE|DT_VCENTER|DT_END_ELLIPSIS;
#[derive(Clone)]
pub(super) enum Command{AddApp,WindowsTray,BackgroundApps,HiddenApps,ShowHiddenApp(Option<String>),AudioMixer,Media,Calendar,Notifications(super::notifications::Action),News(super::news::Action),Weather(super::weather_panel::Action),Overview,SnapLayouts,Topmost(Option<super::WindowIdentity>),SnapWindow(super::WindowIdentity),Shelf(super::shelf::Action),CaptureAccess(super::capture_access::Action),Desktop(bool),Comfort(super::comfort::Action),Recycle(super::recycle_bin::Action),Clipboard,ClipboardCopy(u64),ClipboardRemove(Option<u64>),Widget(UtilityWidget),Run(Action),Wifi(super::wifi::Change),Radio(super::radios::Request),Brightness(Option<super::brightness::Query>),DisplaySettings,App{path:String,operation:super::AppOperation},Window{hwnd:usize,pid:u32,tid:u32},StageMember(super::WindowIdentity)}
#[derive(Clone)]
pub(super) struct Card{pub icon:Option<UtilityWidget>,pub title:String,pub hint:String,pub action:Option<Command>}
pub(super) struct View{pub title:String,pub hint:String,pub cards:Vec<Card>,pub metrics:Option<MetricOptions>}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct MetricOptions{pub enabled:bool,pub inline:bool,pub mask:u8,pub interval:u64,pub compact:bool}
impl Default for MetricOptions{
    fn default()->Self{Self{enabled:false,inline:false,mask:31,interval:2,compact:true}}
}
impl MetricOptions{
    pub(super) fn from_json(v:&serde_json::Value)->Self{
        let mask=v["mask"].as_u64().unwrap_or(31).min(31) as u8;
        let interval=match v["interval"].as_u64(){Some(1)=>1,Some(5)=>5,Some(10)=>10,_=>2};
        Self{enabled:v["enabled"].as_bool().unwrap_or(false),inline:v["mode"].as_str()==Some("inline"),
            mask,interval,compact:v["compact"].as_bool().unwrap_or(true)}
    }
    pub(super) fn json(self)->serde_json::Value{
        serde_json::json!({"enabled":self.enabled,"mode":if self.inline{"inline"}else{"popover"},"mask":self.mask,"interval":self.interval,"compact":self.compact})
    }
}
struct Stream{
    values:Mutex<Option<Snapshot>>,
    pending:AtomicBool,closed:AtomicBool,
}
struct Metrics{cfg:MetricOptions,feed:Feed,stream:Arc<Stream>}
struct Job{h:usize,tag:usize,message:u32,feed:Feed,cfg:MetricOptions,stream:Arc<Stream>}
static ACTIVE:AtomicBool=AtomicBool::new(false);
unsafe extern "system" fn metrics_worker(_:PTP_CALLBACK_INSTANCE,data:*mut std::ffi::c_void){
    let job=*Box::from_raw(data.cast::<Job>());
    if !job.stream.closed.load(Ordering::Acquire){
        let snap=job.feed.sample(job.cfg.mask,job.cfg.interval);
        if let Ok(mut slot)=job.stream.values.lock(){*slot=Some(snap);}
    }
    job.stream.pending.store(false,Ordering::Release);
    ACTIVE.store(false,Ordering::Release);
    if !job.stream.closed.load(Ordering::Acquire){PostMessageW(job.h as HWND,job.message,job.tag,0);}
}
impl Metrics{
    fn new(cfg:MetricOptions)->Self{
        Self{cfg,feed:Feed::shared(),stream:Arc::new(Stream{
            values:Mutex::new(None),pending:AtomicBool::new(false),closed:AtomicBool::new(false),
        })}
    }
    fn request(&self,h:HWND,tag:usize,message:u32){
        if self.cfg.mask==0||self.stream.pending.swap(true,Ordering::AcqRel){return;}
        if ACTIVE.swap(true,Ordering::AcqRel){
            self.stream.pending.store(false,Ordering::Release);return;
        }
        let j=Box::into_raw(Box::new(Job{h:h as usize,tag,message,feed:self.feed.clone(),cfg:self.cfg,stream:self.stream.clone()}));
        if unsafe{TrySubmitThreadpoolCallback(Some(metrics_worker),j.cast(),ptr::null())}==0{
            unsafe{drop(Box::from_raw(j));}
            self.stream.pending.store(false,Ordering::Release);
            ACTIVE.store(false,Ordering::Release);
        }
    }
}
impl Drop for Metrics{fn drop(&mut self){self.stream.closed.store(true,Ordering::Release);}}
fn percent(value:Option<f64>)->Option<String>{value.filter(|n|n.is_finite()).map(|n|format!("{:.0}%",n.clamp(0.0,100.0)))}
fn rate(value:Option<f64>)->Option<String>{
    let v=value.filter(|n|n.is_finite()&&*n>=0.0)?;
    Some(if v>=1_000_000.0{format!("{:.1} MB/s",v/1_000_000.0)}
        else if v>=1000.0{format!("{:.1} KB/s",v/1000.0)}
        else{format!("{v:.0} B/s")})
}
fn metric_cards(view:&Snapshot,cfg:MetricOptions)->Vec<Card>{
    let ram=if view.ram_total>0{
        Some(if cfg.compact{format!("{:.1} GiB",view.ram_used as f64/1_073_741_824.0)}
            else{format!("{:.1} / {:.1} GiB",view.ram_used as f64/1_073_741_824.0,view.ram_total as f64/1_073_741_824.0)})
    }else{None};
    [("CPU",percent(view.cpu)),("RAM",ram),("GPU",percent(view.gpu)),
        ("↓ Tải xuống",rate(view.down)),("↑ Tải lên",rate(view.up))]
        .into_iter().enumerate().filter_map(|(i,(title,value))|{
            if cfg.mask&(1<<i)==0{return None;}
            Some(Card{icon:None,title:title.into(),hint:value?,action:None})
        }).collect()
}
struct Panel {
    layered:bool,dirty:bool,
    hwnd:HWND,owner:HWND,tag:usize,anchor:chrome::Anchor,dpi:u32,layer:Layer,ink:Option<Layer>,
    glass:super::glass::Glass,fonts:[HFONT;4], view:View, metrics:Option<Metrics>, index:Option<usize>, pressed:Option<usize>, pressed_point:Option<POINT>, page:usize,
    cols:usize,per_page:usize, row_height:i32,list:bool,entrance:Option<super::motion::Tween>,shadow:Option<chrome::PanelShadow>,hub:bool,
}
impl Drop for Panel {
    fn drop(&mut self){unsafe{for &f in &self.fonts{if !f.is_null(){DeleteObject(f);}}}}
}

thread_local!{
    static PANE:RefCell<Option<Panel>>=const{RefCell::new(None)};
    static SERIAL:Cell<usize>=const{Cell::new(0)};
    static INLINE_REVISION:Cell<usize>=const{Cell::new(0)};
    static INLINE:RefCell<Option<Inline>>=const{RefCell::new(None)};
}
pub(super) const INLINE_UPDATE:u32=WM_APP+100;
pub(super) const INLINE_TIMER:usize=10;
struct Inline{metrics:Metrics,tag:usize,last:Option<Snapshot>}
fn serial()->usize{SERIAL.with(|s|{let tag=s.get().wrapping_add(1);s.set(tag);tag})}
pub(super) unsafe fn inline_config(control:HWND,cfg:Option<MetricOptions>){
    if control.is_null(){return;}
    INLINE.with(|s|{
        let mut slot=s.borrow_mut();
        if slot.as_ref().map(|s|s.metrics.cfg)==cfg{return;}
        KillTimer(control,INLINE_TIMER);
        advance_inline_revision();
        *slot=cfg.map(|cfg|{
            let state=Inline{metrics:Metrics::new(cfg),tag:serial(),last:None};
            state.metrics.request(control,state.tag,INLINE_UPDATE);
            SetTimer(control,INLINE_TIMER,(cfg.interval*1000) as u32,None);
            state
        });
    });
}
pub(super) fn inline_tick(control:HWND){
    INLINE.with(|s|if let Some(state)=s.borrow().as_ref(){state.metrics.request(control,state.tag,INLINE_UPDATE);});
}
pub(super) fn inline_receive(tag:usize)->bool{
    INLINE.with(|s|{
        let mut slot=s.borrow_mut();
        let Some(state)=slot.as_mut().filter(|s|s.tag==tag)else{return false;};
        let Some(value)=state.metrics.stream.values.lock().ok().and_then(|mut s|s.take())else{return false;};
        state.last=Some(value);
        advance_inline_revision();
        true
    })
}
fn advance_inline_revision(){INLINE_REVISION.with(|n|n.set(n.get().wrapping_add(1)));}
pub(super) fn inline_revision()->usize{INLINE_REVISION.with(Cell::get)}
fn short_rate(value:Option<f64>)->Option<String>{
    let v=value.filter(|v|v.is_finite()&&*v>=0.0)?;
    Some(if v>=1_000_000_000.0{format!("{:.1} GB/s",v/1_000_000_000.0)}
        else if v>=1_000_000.0{format!("{:.1} MB/s",v/1_000_000.0)}
        else if v>=1_000.0{format!("{:.0} KB/s",v/1_000.0)}
        else{format!("{v:.0} B/s")})
}

pub(super) struct InlineReading {
    pub label:&'static str,
    pub value:String,
    pub network:bool,
}
fn readings(value:&Snapshot,cfg:MetricOptions)->Vec<InlineReading>{
    let ram=if value.ram_total>0{percent(value.ram)}else{None};
    [("CPU",percent(value.cpu)),("RAM",ram),("GPU",percent(value.gpu)),
        ("↓",short_rate(value.down)),("↑",short_rate(value.up))]
        .into_iter().enumerate().filter_map(|(i,(label,v))|{
            (cfg.mask&(1<<i)!=0).then(||InlineReading{label,value:v.unwrap_or_else(||"—".into()),network:i>=3})
        }).collect()
}
pub(super) fn inline_readings(cfg:MetricOptions)->Vec<InlineReading>{
    INLINE.with(|s|{
        let slot=s.borrow();
        slot.as_ref().and_then(|s|s.last.as_ref()).map_or_else(Vec::new,|sample|readings(sample,cfg))
    })
}
impl Panel{
    unsafe fn present(&self)->bool{
        let alpha=self.entrance.map_or(255,|t|(255.0*t.openness()).round().clamp(1.0,255.0) as u8);
        if let Some(shadow)=&self.shadow{shadow.opacity(alpha);}
        self.layer.present_at(self.hwnd,None,alpha)
    }
    fn rect(&self,i:usize)->RECT{
        let pad=chrome::scale(self.dpi,14);
        let gap=chrome::scale(self.dpi,if self.list{3}else{8});
        let top=chrome::scale(self.dpi,67)+(i/self.cols) as i32*(self.row_height+gap);
        let w=((self.layer.size.0-2*pad-gap*(self.cols as i32-1))/self.cols as i32).max(1);
        let left=pad+(i%self.cols) as i32*(w+gap);
        RECT{left,top,right:left+w,bottom:top+self.row_height}
    }
    fn cards(&self)->&[Card]{
        let start=(self.page*self.per_page).min(self.view.cards.len());
        let end=(start+self.per_page).min(self.view.cards.len());
        &self.view.cards[start..end]
    }
    fn hit(&self,x:i32,y:i32)->Option<usize>{
        self.cards().iter().enumerate().find_map(|(i,c)|{
            let r=self.rect(i);
            (c.action.is_some()&&x>=r.left&&x<r.right&&y>=r.top&&y<r.bottom).then_some(i)
        })
    }
    fn pages(&self)->usize{self.view.cards.len().div_ceil(self.per_page).max(1)}
    fn page_delta(&mut self,delta:isize){
        self.page=self.page.saturating_add_signed(delta).min(self.pages()-1);
        self.index=self.cards().iter().position(|c|c.action.is_some());self.pressed=None;
    }
    unsafe fn paint(&mut self){
        let (w,h)=self.layer.size;
        let g=self.ink.as_ref().unwrap_or(&self.layer).dc();
        let p=Palette::new(self.anchor.dark);
        let px=|n|chrome::scale(self.dpi,n);
        if !self.glass.draw(g,w,h){super::fill(g,RECT{left:0,top:0,right:w,bottom:h},chrome::color(p.panel));}
        let old=SelectObject(g,self.fonts[0]);
        chrome::text(g,RECT{left:px(18),top:px(13),right:w-px(18),bottom:px(41)},&self.view.title,p.ink,SINGLE_LINE);
        SelectObject(g,self.fonts[2]);
        chrome::text(g,RECT{left:px(18),top:px(41),right:w-px(14),bottom:px(59)},&self.view.hint,p.muted,SINGLE_LINE);
        let cards=self.cards();
        let mut glyphs=Vec::with_capacity(cards.len());
        for (i,c) in cards.iter().enumerate(){
            let r=self.rect(i);
            let focused=self.index==Some(i);
            let is_metric=self.metrics.is_some();
            let fill=match (self.anchor.dark,focused){
                (true,true)=>[82,72,63],(true,false)=>[70,62,56],
                (false,true)=>[255,239,226],(false,false)=>[255,253,251],
            };
            if self.list {
                if focused {chrome::box_fill(g,r,px(12),fill,None);}
                let boundary=i>0&&matches!(cards[i-1].action.as_ref(),Some(Command::Window{..}))
                    && !matches!(c.action.as_ref(),Some(Command::Window{..}));
                if boundary{
                    super::fill(g,RECT{left:r.left+px(8),top:r.top-px(2),right:r.right-px(8),bottom:r.top-px(1)},chrome::color(p.edge));
                }
            }else{chrome::box_fill(g,r,px(20),fill,focused.then_some(p.accent));}
            let inner=r.left+px(if c.icon.is_some(){if self.list{38}else{44}}else{12});
            let first=px(if is_metric||self.list{4}else{6});
            let text_bounds=RECT{left:inner,top:r.top+first,right:r.right-px(9),
                bottom:if c.hint.is_empty(){r.bottom-first}else{r.top+px(if self.list{20}else{25})}};
            SelectObject(g,self.fonts[if is_metric{2}else{1}]);
            chrome::text(g,text_bounds,&c.title,if is_metric{p.muted}else{p.ink},SINGLE_LINE);
            if !c.hint.is_empty(){
                SelectObject(g,self.fonts[if is_metric{3}else{2}]);
                chrome::text(g,RECT{top:r.top+px(if self.list{20}else{25}),bottom:r.bottom-px(5),..text_bounds},&c.hint,
                    if is_metric{p.ink}else{p.muted},SINGLE_LINE);
            }
            if let Some(icon)=c.icon{
                let size=px(if self.list{24}else{30});
                let badge=Rect{x:(r.left+px(8)) as f32,y:(r.top+(r.bottom-r.top-size)/2) as f32,w:size as f32,h:size as f32};
                glyphs.push((icon,badge));
            }
        }
        if cards.is_empty(){
            SelectObject(g,self.fonts[2]);
            chrome::text(g,RECT{left:px(18),top:px(68),right:w-px(18),bottom:h-px(14)},
                if self.metrics.as_ref().is_some_and(|m|m.cfg.mask==0){"Chưa chọn chỉ số."}else{"Đang chờ số liệu từ Windows…"},p.muted,DT_WORDBREAK);
        }
        if self.pages()>1 {
            SelectObject(g,self.fonts[2]);
            chrome::text(g,RECT{left:px(14),top:h-px(30),right:w-px(14),bottom:h-px(4)},
                &format!("‹     {} / {}     ›",self.page+1,self.pages()),p.muted,DT_CENTER|SINGLE_LINE);
        }
        if !old.is_null(){SelectObject(g,old);}
        GdiFlush();
        if let Some(ink)=self.ink.as_mut(){
            let width=w as usize;
            let original=ink.pixels();
            for (i,dst) in self.layer.pixels().chunks_exact_mut(4).enumerate(){
                let top=((i/width)*4*width+(i%width)*2)*4;
                let bottom=top+width*8;
                for c in 0..3{
                    dst[c]=((original[top+c] as u16+original[top+4+c] as u16
                        +original[bottom+c] as u16+original[bottom+4+c] as u16+2)/4) as u8;
                }
                dst[3]=255;
            }
        }else{
            for pixel in self.layer.pixels().chunks_exact_mut(4){pixel[3]=255;}
        }
        let scale=self.dpi as f32/96.0;
        for (icon,badge) in glyphs{
            let data=self.layer.pixels();
            surface::rounded(data,w,h,badge,badge.w*0.5,p.accent,if self.anchor.dark{46}else{30});
            let inset=3.0*scale;
            let slot=Rect{x:badge.x+inset,y:badge.y+inset,w:badge.w-2.0*inset,h:badge.h-2.0*inset};
            surface::status_on(data,w,h,slot,scale,p.accent,Some(icon));
        }
        if self.layered{chrome::mask_popup(self.layer.pixels(),w,h,px(16) as f32,if self.glass.is_ready(){255}else{self.anchor.alpha()});}
    }
}
pub(super) fn wants_status()->bool{
    PANE.with(|s|s.borrow().as_ref().is_some_and(|p|p.view.cards.iter()
        .any(|card|card.icon.is_some_and(super::utilities::live_widget))))
}
pub(super) unsafe fn status_changed(){
    let hwnd=PANE.with(|s|{
        let mut s=s.borrow_mut();let p=s.as_mut()?;
        let overview=p.view.title=="Trung tâm điều khiển";
        let mut changed=false;
        for card in &mut p.view.cards{
            if let Some(widget)=card.icon.filter(|&w|super::utilities::live_widget(w)){
                changed=true;
                if overview&&!card.hint.is_empty(){
                    card.hint=super::utilities::status_detail(widget).unwrap_or_default();
                }
            }
        }
        if changed{p.dirty=true;Some(p.hwnd)}else{None}
    });
    if let Some(hwnd)=hwnd{InvalidateRect(hwnd,ptr::null(),0);}
}
pub(super) fn ticket(owner:HWND)->Option<usize>{
    PANE.with(|s|s.borrow().as_ref().filter(|p|p.owner==owner).map(|p|p.tag))
}
pub(super) fn window(owner:HWND)->Option<HWND>{PANE.with(|s|s.borrow().as_ref().filter(|p|p.owner==owner).map(|p|p.hwnd))}
pub(super) unsafe fn update(owner:HWND,ticket:usize,view:View){
    let hwnd=PANE.with(|s|{
        let mut slot=s.borrow_mut();
        let panel=slot.as_mut().filter(|p|p.owner==owner&&p.tag==ticket)?;
        panel.view=view;
        panel.page=panel.page.min(panel.pages()-1);
        panel.index=None;panel.pressed=None;panel.pressed_point=None;
        panel.dirty=true;
        Some(panel.hwnd)
    });
    if let Some(hwnd)=hwnd{InvalidateRect(hwnd,ptr::null(),0);}
}
pub(super) fn open_for(owner:HWND)->bool{
    PANE.with(|s|s.borrow().as_ref().is_some_and(|p|p.owner==owner))
}
pub(super) unsafe fn close_for(owner:HWND){
    let hwnd=PANE.with(|s|s.borrow().as_ref().filter(|p|owner.is_null()||owner==p.owner).map(|p|p.hwnd));
    if let Some(h)=hwnd{DestroyWindow(h);}
}
pub(super) unsafe fn follow_hub(owner:HWND,dock:crate::launcher::DockAnchor){
    let target=PANE.with(|slot|{
        let mut slot=slot.borrow_mut();
        let p=slot.as_mut().filter(|p|p.owner==owner&&p.hub)?;
        let same=p.anchor.point.x==dock.point.x&&p.anchor.point.y==dock.point.y&&p.anchor.side==dock.side;
        if same{return None;}
        p.anchor.point=dock.point;p.anchor.side=dock.side;
        Some((p.hwnd,p.anchor,p.dpi,p.layer.size))
    });
    let Some((hwnd,anchor,dpi,(w,h)))=target else{return;};
    let Some(rect)=chrome::follow_rect(anchor,dpi,w,h)else{DestroyWindow(hwnd);return;};
    SetWindowPos(hwnd,ptr::null_mut(),rect.left,rect.top,0,0,SWP_NOACTIVATE|SWP_NOSIZE|SWP_NOZORDER);
    PANE.with(|slot|if let Some(p)=slot.borrow_mut().as_mut().filter(|p|p.hwnd==hwnd){
        chrome::PanelShadow::sync(&mut p.shadow,hwnd,owner,rect,dpi,anchor.side,anchor.dark);
    });
}
pub(super) unsafe fn show(owner:HWND,anchor:chrome::Anchor,view:View){show_inner(owner,anchor,view,false,false,None);}
pub(super) unsafe fn show_list(owner:HWND,anchor:chrome::Anchor,view:View){show_inner(owner,anchor,view,true,false,None);}
pub(super) unsafe fn show_list_capacity(owner:HWND,anchor:chrome::Anchor,view:View,capacity:usize){show_inner(owner,anchor,view,true,false,Some(capacity));}
pub(super) unsafe fn show_hub(owner:HWND,anchor:chrome::Anchor,view:View){show_inner(owner,anchor,view,false,true,None);}
pub(super) fn first_page(owner:HWND,ticket:usize){PANE.with(|s|if let Some(p)=s.borrow_mut().as_mut().filter(|p|p.owner==owner&&p.tag==ticket){p.page=0;});}
unsafe fn show_inner(owner:HWND,anchor:chrome::Anchor,view:View,list:bool,hub:bool,capacity:Option<usize>){
    super::utilities::close_panels();
    let dpi=windows_sys::Win32::UI::HiDpi::GetDpiForWindow(owner).clamp(72,288);
    let px=|n|chrome::scale(dpi,n);
    let Some(work)=(if hub{anchor.panel_work(dpi)}else{anchor.work()})else{return;};
    let double=!list&&view.metrics.map_or(view.cards.len()>3,|o|o.compact);
    let cols=if double&&work.right-work.left>=px(365){2}else{1};
    let width=px(if list{350}else if cols==2{432}else{328}).min(work.right-work.left);
    let row_height=px(if list{40}else if view.metrics.is_some(){57}else{52});
    let gap=px(if list{3}else{8});
    let budget=(work.bottom-work.top-px(132)).max(row_height);
    let rows=(budget/(row_height+gap)).clamp(1,9) as usize;
    let per_page=cols*rows;
    let expected=capacity.unwrap_or_else(||view.metrics.map_or(view.cards.len(),|m|m.mask.count_ones() as usize));
    let height=(px(84)+expected.clamp(1,per_page).div_ceil(cols) as i32*(row_height+gap)
        +if expected>per_page{px(30)}else{0}).min(work.bottom-work.top).max(1);
    let Some(layer)=Layer::new(width,height)else{return;};
    let ink=Layer::new(width*2,height*2).inspect(|ink|{
        let dc=ink.dc();
        SetMapMode(dc,MM_ANISOTROPIC);
        SetWindowExtEx(dc,width,height,ptr::null_mut());
        SetViewportExtEx(dc,width*2,height*2,ptr::null_mut());
    });
    let class=wide(CLASS);
    if !chrome::register_with_style(&class,Some(proc),0){return;}
    let (x,y)=chrome::placement(work,anchor,width,height,px(if hub{chrome::PANEL_GAP}else{8}));
    let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_TOPMOST|WS_EX_LAYERED,class.as_ptr(),wide(&view.title).as_ptr(),
        WS_POPUP,x,y,width,height,owner,ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
    if hwnd.is_null(){return;}
    chrome::disable_system_border(hwnd);
    let glass=super::glass::Glass::capture(RECT{left:x,top:y,right:x+width,bottom:y+height},dpi,anchor.dark,anchor.transparency);
    let tag=serial();
    let fonts=[chrome::choose_font(dpi,if list{18}else{21},550),chrome::choose_font(dpi,14,if list{455}else{530}),chrome::choose_font(dpi,12,420),chrome::choose_font(dpi,19,540)];
    let metrics=view.metrics.map(Metrics::new);
    let start=std::time::Instant::now();
    let animated=super::motion::native_allowed();
    let mut p=Panel{hwnd,owner,tag,anchor,dpi,layered:true,dirty:false,layer,ink,glass,fonts,view,metrics,index:None,pressed:None,pressed_point:None,page:0,cols,per_page,row_height,list,entrance:None,shadow:None,hub};
    // A hidden HWND can't be relied on to dispatch WM_PAINT. Submit the
    // completed bitmap directly, then show the window with content.
    p.paint();
    if animated&&SetTimer(hwnd,ENTRANCE_TIMER,16,None)!=0{p.entrance=Some(super::motion::Tween::new(true,super::motion::CONTROL,None).prepared());}
    if !p.present(){
        crate::log!("DOCK_PANEL layered submission failed: {}",GetLastError());
        p.layered=false;
        p.entrance=None;
        KillTimer(hwnd,ENTRANCE_TIMER);
        SetWindowLongPtrW(hwnd,GWL_EXSTYLE,GetWindowLongPtrW(hwnd,GWL_EXSTYLE)& !(WS_EX_LAYERED as isize));
        chrome::clip_corners(hwnd,width,height,dpi);
        p.paint();
    }
    PANE.with(|s|s.replace(Some(p)));
    ShowWindow(hwnd,SW_SHOW);UpdateWindow(hwnd);
    if hub&&IsWindow(hwnd)!=0{
        PANE.with(|s|if let Some(p)=s.borrow_mut().as_mut().filter(|p|p.hwnd==hwnd){
            chrome::PanelShadow::sync(&mut p.shadow,hwnd,owner,RECT{left:x,top:y,right:x+width,bottom:y+height},dpi,anchor.side,anchor.dark);
        });
    }
    SetForegroundWindow(hwnd);SetFocus(hwnd);
    PANE.with(|s|if let Some(p)=s.borrow_mut().as_mut().filter(|p|p.hwnd==hwnd){
        if let Some(t)=p.entrance{let _=p.present();p.entrance=Some(t.restart_clock());}
    });
    crate::log!("DOCK_PANEL first frame {} in {} ms",if list{"list"}else{"cards"},start.elapsed().as_millis());
    PANE.with(|s|{if let Some(m)=s.borrow().as_ref().and_then(|p|p.metrics.as_ref()){
        if m.cfg.mask!=0{
            m.request(hwnd,tag,METRIC_UPDATE);
            SetTimer(hwnd,METRIC_TIMER,(m.cfg.interval*1000) as u32,None);
        }
    }});
}
unsafe extern "system" fn proc(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
    match msg{
        WM_ERASEBKGND=>1,
        WM_ACTIVATE if w&0xffff==WA_INACTIVE as usize=>{if !super::shelf::modal()&&!super::capture_access::pending(){PostMessageW(hwnd,WM_CLOSE,0,0);}0},
        super::shelf::CHANGED=>{super::shelf::changed(hwnd);0},
        super::notifications::READY=>{super::notifications::receive(hwnd,w);0},
        super::news::READY=>{super::news::receive(hwnd,w);0},
        crate::weather::READY=>{super::weather_panel::receive(hwnd,w);0},
        WM_DPICHANGED=>{DestroyWindow(hwnd);0},
        WM_CLOSE=>{
            let animated=PANE.with(|s|{
                let mut slot=s.borrow_mut();
                let Some(p)=slot.as_mut().filter(|p|p.hwnd==hwnd&&p.layered)else{return false;};
                if !super::motion::native_allowed(){return false;}
                if p.entrance.is_some_and(|t|t.to==0.0){return true;}
                if SetTimer(hwnd,ENTRANCE_TIMER,16,None)==0{return false;}
                p.entrance=Some(super::motion::Tween::new(false,super::motion::CONTROL,p.entrance));
                p.pressed=None;
                true
            });
            if !animated{DestroyWindow(hwnd);}
            else{
                KillTimer(hwnd,METRIC_TIMER);
                if GetCapture()==hwnd{ReleaseCapture();}
            }
            0
        },
        WM_NCDESTROY=>{
            super::shelf::detach(hwnd);
            super::notifications::detach(hwnd);
            super::news::detach(hwnd);
            super::weather_panel::detach(hwnd);
            KillTimer(hwnd,METRIC_TIMER);
            KillTimer(hwnd,ENTRANCE_TIMER);
            let owner=PANE.with(|s|{
                let mut p=s.borrow_mut();
                if p.as_ref().is_some_and(|p|p.hwnd==hwnd){p.take().map(|p|p.owner)}else{None}
            });
            if let Some(owner)=owner{super::panel_closed(owner);}
            DefWindowProcW(hwnd,msg,w,l)
        },
        WM_PAINT=>{
            let mut ps:PAINTSTRUCT=mem::zeroed();let dc=BeginPaint(hwnd,&mut ps);
            let panel=PANE.with(|s|{
                let mut slot=s.borrow_mut();
                if slot.as_ref().is_some_and(|p|p.hwnd==hwnd){slot.take()}else{None}
            });
            if let Some(mut p)=panel{
                if p.dirty{p.paint();p.dirty=false;
                    if p.layered{
                        EndPaint(hwnd,&ps);
                        let _=p.present();
                    }else{p.layer.blit(dc);EndPaint(hwnd,&ps);}
                }else{
                    if !p.layered{p.layer.blit(dc);}
                    EndPaint(hwnd,&ps);
                }
                if IsWindow(hwnd)!=0 {
                    PANE.with(|s|{let mut slot=s.borrow_mut();if slot.is_none(){*slot=Some(p);}});
                }else{super::panel_closed(p.owner);}
            }else{EndPaint(hwnd,&ps);}
            0
        },
        WM_TIMER if w==METRIC_TIMER=>{
            PANE.with(|s|{if let Some(p)=s.borrow().as_ref().filter(|p|p.hwnd==hwnd){
                if let Some(m)=&p.metrics{m.request(hwnd,p.tag,METRIC_UPDATE);}
            }});0
        },
        WM_TIMER if w==ENTRANCE_TIMER=>{
            let panel=PANE.with(|s|{let mut s=s.borrow_mut();if s.as_ref().is_some_and(|p|p.hwnd==hwnd){s.take()}else{None}});
            if let Some(mut p)=panel{
                let settled=p.entrance.is_none_or(|t|t.progress()>=1.0||!super::popup_motion_allowed());
                let closing=settled&&p.entrance.is_some_and(|t|t.to==0.0);
                if settled{KillTimer(hwnd,ENTRANCE_TIMER);p.entrance=None;}
                if !closing{let _=p.present();}
                if IsWindow(hwnd)!=0{
                    PANE.with(|s|{let mut s=s.borrow_mut();if s.is_none(){*s=Some(p);}});
                    if closing{DestroyWindow(hwnd);}
                }else{super::panel_closed(p.owner);}
            }else{KillTimer(hwnd,ENTRANCE_TIMER);}
            0
        },
        METRIC_UPDATE=>{
            let ready=PANE.with(|s|{
                let mut p=s.borrow_mut();
                let Some(p)=p.as_mut().filter(|p|p.hwnd==hwnd&&p.tag==w)else{return false;};
                let Some(m)=&p.metrics else{return false;};
                let Some(v)=m.stream.values.lock().ok().and_then(|mut s|s.take())else{return false;};
                p.view.cards=metric_cards(&v,m.cfg);
                p.dirty=true;
                p.view.hint=if p.view.cards.is_empty(){"Windows chưa cung cấp số liệu."}else{"Cập nhật trực tiếp · Chỉ số khả dụng"}.into();
                true
            });
            if ready{InvalidateRect(hwnd,ptr::null(),0);}0
        },
        WM_MOUSEMOVE|super::WM_DOCK_MOUSELEAVE|WM_LBUTTONDOWN|WM_LBUTTONUP|WM_CAPTURECHANGED|WM_MOUSEWHEEL|WM_KEYDOWN=>{
            if msg==WM_KEYDOWN&&w as u16==VK_ESCAPE{SendMessageW(hwnd,WM_CLOSE,0,0);return 0;}
            let (action,repaint,capture)=PANE.with(|s|{
                let mut p=s.borrow_mut();
                let Some(p)=p.as_mut().filter(|p|p.hwnd==hwnd&&!p.entrance.is_some_and(|t|t.to==0.0))else{return (None,false,false);};
                let x=l as i16 as i32;let y=(l>>16) as i16 as i32;
                let before=(p.index,p.page);
                let mut action=None;
                match msg{
                    WM_MOUSEMOVE=>{
                        p.index=p.hit(x,y);
                        if let Some(point)=p.pressed_point.filter(|_|w&1!=0){ // MK_LBUTTON is still physically held
                            if (x-point.x).abs()>=windows_sys::Win32::UI::HiDpi::GetSystemMetricsForDpi(SM_CXDRAG,p.dpi).max(4)||(y-point.y).abs()>=windows_sys::Win32::UI::HiDpi::GetSystemMetricsForDpi(SM_CYDRAG,p.dpi).max(4){
                                action=p.pressed.and_then(|i|p.cards().get(i)).and_then(|c|c.action.as_ref()).and_then(super::shelf::drag_command);
                                if action.is_some(){p.pressed=None;p.pressed_point=None;}
                            }
                        }
                    }
                    super::WM_DOCK_MOUSELEAVE=>{
                        let shelf_drag=p.pressed.and_then(|i|p.cards().get(i)).and_then(|c|c.action.as_ref()).and_then(super::shelf::drag_command).is_some();
                        if GetCapture()!=hwnd||!shelf_drag{p.index=None;p.pressed=None;p.pressed_point=None;}
                    },
                    WM_CAPTURECHANGED=>{p.index=None;p.pressed=None;p.pressed_point=None;},
                    WM_LBUTTONDOWN=>{p.pressed=p.hit(x,y);p.pressed_point=Some(POINT{x,y});}
                    WM_LBUTTONUP=>{
                        let from=p.pressed.take();
                        p.pressed_point=None;
                        if let Some(i)=p.hit(x,y).filter(|i|Some(*i)==from){action=p.cards()[i].action.clone();}
                        else if p.pages()>1&&y>=p.layer.size.1-chrome::scale(p.dpi,33){
                            p.page_delta(if x>=p.layer.size.0/2{1}else{-1});
                        }
                    }
                    WM_MOUSEWHEEL=>p.page_delta(if ((w>>16) as i16)<0{1}else{-1}),
                    WM_KEYDOWN=>{
                        let n=p.cards().len();
                        let key=w as u16;
                        if n>0{
                            let i=p.index.unwrap_or(0);
                            match key{
                                VK_RETURN|VK_SPACE=>action=p.cards().get(i).and_then(|c|c.action.clone()),
                                VK_HOME=>p.index=Some(0),VK_END=>p.index=Some(n-1),
                                VK_UP|VK_DOWN|VK_LEFT|VK_RIGHT|VK_TAB=>{
                                    let step=match key{VK_UP=>-(p.cols as isize),VK_DOWN=>p.cols as isize,
                                        VK_LEFT=>-1,VK_TAB if GetKeyState(VK_SHIFT as i32)<0=>-1,_=>1};
                                    let next=if p.index.is_some(){i as isize+step}else{0};
                                    if (next<0||next>=n as isize)&&p.pages()>1{p.page_delta(step.signum());}
                                    else{p.index=Some(next.rem_euclid(n as isize) as usize);}
                                }
                                _=>{}
                            }
                        }
                    }
                    _=>{}
                }
                let changed=before!=(p.index,p.page);
                p.dirty|=changed;
                (action.map(|a|(p.owner,a)),changed,msg==WM_LBUTTONDOWN&&p.pressed.is_some())
            });
            if msg==WM_MOUSEMOVE{
                let mut event=TRACKMOUSEEVENT{cbSize:mem::size_of::<TRACKMOUSEEVENT>() as u32,dwFlags:TME_LEAVE,hwndTrack:hwnd,dwHoverTime:0};
                TrackMouseEvent(&mut event);
            }
            if capture{SetCapture(hwnd);}
            if msg==WM_LBUTTONUP{ReleaseCapture();}
            if let Some((owner,action))=action{
                if let Command::Shelf(action)=action{
                    if GetCapture()==hwnd{ReleaseCapture();}
                    super::shelf::perform(owner,hwnd,action);
                }else if let Command::Notifications(action)=action{
                    if GetCapture()==hwnd{ReleaseCapture();}
                    super::notifications::perform(owner,hwnd,action);
                }else if let Command::News(action)=action{
                    if GetCapture()==hwnd{ReleaseCapture();}
                    super::news::perform(owner,hwnd,action);
                }else if let Command::Weather(action)=action{
                    if GetCapture()==hwnd{ReleaseCapture();}
                    super::weather_panel::perform(owner,hwnd,action);
                }else{
                    DestroyWindow(hwnd);
                    super::perform_control(owner,action);
                }
            }else if repaint{InvalidateRect(hwnd,ptr::null(),0);}
            0
        },
        _=>DefWindowProcW(hwnd,msg,w,l),
    }
}
#[cfg(test)] mod tests{
    use super::*;
    #[test]
    #[ignore = "native layered popup presentation; run individually in CI"]
    fn native_popup_has_its_first_layer_before_pumping_input(){
        unsafe{
            let owner=CreateWindowExW(WS_EX_TOOLWINDOW,wide("STATIC").as_ptr(),wide("Deskland popup test").as_ptr(),
                WS_POPUP,20,20,300,80,ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
            assert!(!owner.is_null());
            show_list(owner,chrome::Anchor{point:POINT{x:220,y:550},side:super::super::DockSide::Bottom,dark:true,transparency:30},
                View{title:"Menu test".into(),hint:"First frame".into(),
                    cards:vec![Card{title:"Open".into(),hint:String::new(),icon:None,action:None}],metrics:None});
            let (popup,alpha)=PANE.with(|p|{
                let mut p=p.borrow_mut();
                let p=p.as_mut().expect("popup should be ready before the loop pumps input");
                assert!(p.layered&&!p.dirty);
                let (w,h)=p.layer.size;
                (p.hwnd,p.layer.pixels()[((h/2*w+w/2)*4+3) as usize])
            });
            assert!(IsWindowVisible(popup)!=0&&alpha>200);
            close_for(owner);
            assert_eq!(IsWindow(popup),0);
            show_hub(owner,chrome::Anchor{point:POINT{x:220,y:550},side:super::super::DockSide::Bottom,dark:false,transparency:30},
                View{title:"Control Center".into(),hint:String::new(),cards:vec![],metrics:None});
            let shadow=PANE.with(|p|p.borrow().as_ref().unwrap().shadow.as_ref().expect("hub shadow").hwnd);
            assert_ne!(IsWindowVisible(shadow),0);
            assert_ne!(GetWindowLongPtrW(shadow,GWL_EXSTYLE)&WS_EX_TRANSPARENT as isize,0);
            close_for(owner);
            assert_eq!(IsWindow(shadow),0);
            for speed in [25,50,70,75,100,150,200]{
                super::super::motion::native_fixture(speed,||{
                    show_hub(owner,chrome::Anchor{point:POINT{x:220,y:550},side:super::super::DockSide::Bottom,dark:false,transparency:30},
                        View{title:"Control Center".into(),hint:String::new(),cards:vec![],metrics:None});
                    let (h,t)=PANE.with(|p|{let p=p.borrow();let p=p.as_ref().unwrap();(p.hwnd,p.entrance.unwrap())});
                    assert_ne!(IsWindowVisible(h),0);
                    assert_eq!(t.from,0.0);
                    assert_eq!(t.duration,(22000.0/speed as f64).round() as u32);
                    SendMessageW(h,WM_KEYDOWN,VK_ESCAPE as usize,0);
                    assert_ne!(IsWindow(h),0);
                    PANE.with(|p|{let mut p=p.borrow_mut();let p=p.as_mut().unwrap();let t=p.entrance.unwrap();
                        assert_eq!(t.duration,(16000.0/speed as f64).round() as u32);p.entrance=Some(t.finish());});
                    SendMessageW(h,WM_TIMER,ENTRANCE_TIMER,0);
                    assert_eq!(IsWindow(h),0);assert!(!open_for(owner));
                });
            }
            println!("MOTION_SPEED control_center_native=PASS six_speeds=PASS closing_endpoint=PASS");
            DestroyWindow(owner);
        }
    }
    #[test] fn inline_readings_keep_slots_stable_when_an_engine_has_no_sample(){
        let mut sample=Snapshot::default();
        sample.cpu=Some(12.0);
        sample.ram=Some(43.0);
        sample.ram_total=16;
        sample.down=Some(244700.0);
        let all=readings(&sample,MetricOptions::default());
        assert_eq!(all.iter().map(|r|(r.label,r.value.as_str())).collect::<Vec<_>>(),
            [("CPU","12%"),("RAM","43%"),("GPU","—"),("↓","245 KB/s"),("↑","—")]);
        let net=readings(&sample,MetricOptions{mask:8,..Default::default()});
        assert_eq!(net.len(),1);
        assert!(net[0].network);
    }
    #[test] fn metrics_only_show_selected_available_values(){
        let mut s=Snapshot::default();s.cpu=Some(17.8);s.ram_used=3*1_073_741_824;s.ram_total=16*1_073_741_824;s.down=Some(1_515_000.0);s.up=Some(12.0);
        let c=metric_cards(&s,MetricOptions{mask:1|4|8,interval:5,enabled:true,..MetricOptions::default()});
        assert_eq!(c.iter().map(|x|(&*x.title,&*x.hint)).collect::<Vec<_>>(),[("CPU","18%"),("↓ Tải xuống","1.5 MB/s")]);
    }
}
