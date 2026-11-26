//! A local, anchored month view. No Explorer panel or background calendar service.
use super::{glass_probe::Layer,panel_chrome as chrome,surface::Palette};
use crate::desktop::wide;
use std::{cell::RefCell,mem,ptr};
use windows_sys::Win32::{
    Foundation::*,Graphics::Gdi::*,System::{LibraryLoader::GetModuleHandleW,SystemInformation::GetLocalTime},
    UI::{HiDpi::GetDpiForWindow,Input::KeyboardAndMouse::{ReleaseCapture,SetCapture,SetFocus,TrackMouseEvent,
        TRACKMOUSEEVENT,TME_LEAVE,VK_ESCAPE,VK_HOME,VK_LEFT,VK_RETURN,VK_RIGHT,VK_SHIFT,VK_SPACE,VK_TAB,GetKeyState},
        WindowsAndMessaging::*},
};

const CLASS:&str="TuanpcDeskCalendar";
const TIMER:usize=1;
const WIDTH:i32=338;
const HEIGHT:i32=354;
const TEXT:u32=DT_SINGLELINE|DT_VCENTER|DT_END_ELLIPSIS;
fn text<'a>(vi:&'a str,en:&'a str,zh:&'a str)->&'a str{crate::launcher::localized(vi,en,zh)}
fn weekdays()->[&'static str;7]{match text("vi","en","zh"){
    "vi"=>["T2","T3","T4","T5","T6","T7","CN"],"zh"=>["一","二","三","四","五","六","日"],_=>["Mon","Tue","Wed","Thu","Fri","Sat","Sun"]}}
fn day_name(index:usize)->&'static str{match text("vi","en","zh"){
    "vi"=>["Thứ Hai","Thứ Ba","Thứ Tư","Thứ Năm","Thứ Sáu","Thứ Bảy","Chủ Nhật"][index],
    "zh"=>["星期一","星期二","星期三","星期四","星期五","星期六","星期日"][index],
    _=>["Monday","Tuesday","Wednesday","Thursday","Friday","Saturday","Sunday"][index]}}
fn month_name(month:Month)->String{match text("vi","en","zh"){
    "vi"=>format!("Tháng {} · {}",month.number,month.year),"zh"=>format!("{}年{}月",month.year,month.number),
    _=>format!("{} {}",["January","February","March","April","May","June","July","August","September","October","November","December"][(month.number-1) as usize],month.year)}}
fn first_weekday()->usize{
    use windows_sys::Win32::Globalization::{GetLocaleInfoEx,LOCALE_IFIRSTDAYOFWEEK};
    // Win32 defines LOCALE_NAME_USER_DEFAULT as NULL, not an exported constant.
    let mut value=[0u16;8];let count=unsafe{GetLocaleInfoEx(ptr::null(),LOCALE_IFIRSTDAYOFWEEK,value.as_mut_ptr(),8)};
    if count<=1{return 0;}String::from_utf16_lossy(&value[..count as usize-1]).parse::<usize>().ok().filter(|v|*v<=6).unwrap_or(0)
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
struct Month{year:i32,number:i32}
impl Month{
    fn days(self)->i32{
        match self.number{
            2 if self.year%4==0&&(self.year%100!=0||self.year%400==0)=>29,
            2=>28,4|6|9|11=>30,_=>31,
        }
    }
    fn step(self,delta:i32)->Self{
        let serial=self.year*12+self.number-1+delta;
        Self{year:serial.div_euclid(12),number:serial.rem_euclid(12)+1}
    }
    fn first(self)->i32{
        // Gregorian weekday, normalized to Monday == 0.
        let y=self.year-i32::from(self.number<3);
        let offset=[0,3,2,5,0,3,5,1,4,6,2,4][(self.number-1) as usize];
        (y+y/4-y/100+y/400+offset+1+6).rem_euclid(7)
    }
    fn rows(self)->i32{((self.first()+self.days()+6)/7).max(5)}
    fn date_at(self,index:i32)->(Self,i32){
        let day=index-self.first()+1;
        if day<1{let prior=self.step(-1);(prior,prior.days()+day)}
        else if day>self.days(){(self.step(1),day-self.days())}
        else{(self,day)}
    }
}
#[derive(Clone,Copy,PartialEq,Eq)]
struct Local{month:Month,day:i32,weekday:usize,hour:u16,minute:u16}
impl Local{
    fn now()->Self{
        let mut t:SYSTEMTIME=unsafe{mem::zeroed()};
        unsafe{GetLocalTime(&mut t);}
        Self{month:Month{year:t.wYear as i32,number:t.wMonth as i32},day:t.wDay as i32,
            weekday:((t.wDayOfWeek+6)%7) as usize,hour:t.wHour,minute:t.wMinute}
    }
}
#[derive(Clone,Copy,PartialEq,Eq)]
enum Button{Previous,Next,Today}
impl Button{
    const ALL:[Self;3]=[Self::Previous,Self::Next,Self::Today];
    fn bounds(self)->(i32,i32,i32,i32){
        match self{Self::Previous=>(248,20,32,32),Self::Next=>(287,20,32,32),Self::Today=>(240,308,79,30)}
    }
}
struct Panel{
    hwnd:HWND,owner:HWND,anchor:chrome::Anchor,layer:Layer,dpi:u32,glass:super::glass::Glass,shadow:Option<chrome::PanelShadow>,
    fonts:[HFONT;4],local:Local,view:Month,following:bool,dirty:bool,
    first_weekday:usize,
    hover:Option<Button>,focus:Option<Button>,pressed:Option<Button>,
    motion:Option<super::motion::Tween>,
}
thread_local!{static PANEL:RefCell<Option<Panel>>=const{RefCell::new(None)};}
impl Drop for Panel{
    fn drop(&mut self){unsafe{for font in self.fonts{if !font.is_null(){DeleteObject(font);}}}}
}
const MOTION_TIMER:usize=7;
impl Panel{
    unsafe fn present(&self)->bool{
        let alpha=self.motion.map_or(255,|t|(255.0*t.openness()).round().clamp(1.0,255.0) as u8);
        if let Some(shadow)=&self.shadow{shadow.opacity(alpha);}
        self.layer.present_at(self.hwnd,None,alpha)
    }
    fn in_rect(&self,x:i32,y:i32,w:i32,h:i32)->RECT{
        let (a,b)=self.layer.size;
        RECT{left:x*a/WIDTH,top:y*b/HEIGHT,right:(x+w)*a/WIDTH,bottom:(y+h)*b/HEIGHT}
    }
    fn hit(&self,x:i32,y:i32)->Option<Button>{
        Button::ALL.into_iter().find(|btn|{
            let (a,b,c,d)=btn.bounds();
            let r=self.in_rect(a,b,c,d);
            x>=r.left&&x<r.right&&y>=r.top&&y<r.bottom
        })
    }
    fn activate(&mut self,button:Button){
        self.following=button==Button::Today;
        if self.following{self.local=Local::now();}
        self.view=if self.following{self.local.month}else{
            let target=self.view.step(if button==Button::Previous{-1}else{1});
            if (1601..=9999).contains(&target.year){target}else{self.view}
        };
        self.dirty=true;
    }
    fn update_time(&mut self,now:Local){
        if self.local!=now{
            self.local=now;
            if self.following{self.view=now.month;}
            self.dirty=true;
        }
    }
    unsafe fn paint(&mut self){
        let p=Palette::new(self.anchor.dark);
        let faded=std::array::from_fn(|c|((p.panel[c] as u16*2+p.muted[c] as u16)/3) as u8);
        let field=std::array::from_fn(|c|((p.panel[c] as u16*7+p.ink[c] as u16)/8) as u8);
        let dc=self.layer.dc();
        let (w,h)=self.layer.size;
        let rect=|x,y,width,height|self.in_rect(x,y,width,height);
        let radius=|logical:i32| (logical*w/WIDTH).max(2);
        let prev=SelectObject(dc,self.fonts[0]);
        let c=|bounds,value:&str,ink|chrome::text(dc,bounds,value,ink,TEXT|DT_CENTER);
        let l=|bounds,value:&str,ink|chrome::text(dc,bounds,value,ink,TEXT|DT_LEFT);
        if !self.glass.draw(dc,w,h){
            SetDCBrushColor(dc,chrome::color(p.panel));
            FillRect(dc,&rect(0,0,WIDTH,HEIGHT),GetStockObject(DC_BRUSH) as HBRUSH);
        }
        l(rect(20,15,216,32),&month_name(self.view),p.ink);
        SelectObject(dc,self.fonts[1]);
        l(rect(21,50,215,22),&format!("{:02}:{:02}  ·  {}",self.local.hour,self.local.minute,text("Lịch TLauncher","TLauncher Calendar","TLauncher 日历")),p.muted);
        for button in Button::ALL{
            let (x,y,bw,bh)=button.bounds();
            let r=rect(x,y,bw,bh);
            let hot=self.hover==Some(button)||self.focus==Some(button)||self.pressed==Some(button);
            chrome::box_fill(dc,r,radius(15),if hot{p.accent}else{field},None);
            let ink=if hot{p.panel}else{p.ink};
            let label=match button{Button::Previous=>"‹",Button::Next=>"›",Button::Today=>text("Hôm nay","Today","今天")};
            SelectObject(dc,self.fonts[if button==Button::Today{1}else{3}]);
            c(r,label,ink);
        }
        SelectObject(dc,self.fonts[2]);
        let names=weekdays();
        for i in 0..7{let name=names[(i+self.first_weekday)%7];
            c(rect(18+i as i32*43,84,43,24),name,p.muted);
        }
        let first=(self.view.first()-self.first_weekday as i32).rem_euclid(7);
        let rows=((first+self.view.days()+6)/7).max(5);
        let row_height=181/rows;
        for i in 0..rows*7{
            let (month,day)=self.view.date_at(i+self.view.first()-first);
            let x=18+(i%7)*43;
            let y=110+(i/7)*row_height;
            let today=month==self.local.month&&day==self.local.day;
            let ink=if today{p.panel}else if month!=self.view{faded}else{p.ink};
            if today{
                chrome::box_fill(dc,rect(x+7,y+1,29,row_height-2),radius(25),p.accent,None);
            }
            c(rect(x,y,43,row_height),&day.to_string(),ink);
        }
        SelectObject(dc,self.fonts[1]);
        l(rect(19,303,212,40),&format!("{}, {:02}/{:02}/{}",day_name(self.local.weekday),
            self.local.day,self.local.month.number,self.local.month.year),p.muted);
        SelectObject(dc,prev);
        GdiFlush();
        chrome::mask_popup(self.layer.pixels(),w,h,radius(16) as f32,if self.glass.is_ready(){255}else{self.anchor.alpha()});
        self.dirty=false;
    }
}

pub(super) fn open_for(owner:HWND)->bool{
    PANEL.with(|slot|slot.borrow().as_ref().is_some_and(|p|p.owner==owner))
}
pub(super) unsafe fn close_for(owner:HWND){
    let hwnd=PANEL.with(|s|s.borrow().as_ref().filter(|p|owner.is_null()||p.owner==owner).map(|p|p.hwnd));
    if let Some(hwnd)=hwnd{DestroyWindow(hwnd);}
}
pub(super) unsafe fn follow(owner:HWND,dock:crate::launcher::DockAnchor){
    let target=PANEL.with(|slot|{
        let mut slot=slot.borrow_mut();
        let p=slot.as_mut().filter(|p|p.owner==owner)?;
        let same=p.anchor.point.x==dock.point.x&&p.anchor.point.y==dock.point.y&&p.anchor.side==dock.side;
        if same{return None;}
        p.anchor.point=dock.point;p.anchor.side=dock.side;
        Some((p.hwnd,p.anchor,p.dpi,p.layer.size))
    });
    let Some((hwnd,anchor,dpi,(w,h)))=target else{return;};
    let Some(rect)=chrome::follow_rect(anchor,dpi,w,h)else{PostMessageW(hwnd,WM_CLOSE,0,0);return;};
    SetWindowPos(hwnd,ptr::null_mut(),rect.left,rect.top,0,0,SWP_NOACTIVATE|SWP_NOSIZE|SWP_NOZORDER);
    PANEL.with(|slot|if let Some(p)=slot.borrow_mut().as_mut().filter(|p|p.hwnd==hwnd){
        chrome::PanelShadow::sync(&mut p.shadow,hwnd,owner,rect,dpi,anchor.side,anchor.dark);
    });
}
pub(super) unsafe fn show(owner:HWND,anchor:chrome::Anchor){
    super::utilities::close_panels();
    let dock_dpi=GetDpiForWindow(owner).clamp(72,288);
    let Some(work)=anchor.panel_work(dock_dpi)else{return;};
    let dpi=dock_dpi
        .min(((work.right-work.left)*96/WIDTH).max(48) as u32)
        .min(((work.bottom-work.top)*96/HEIGHT).max(48) as u32);
    let (w,h)=(WIDTH*dpi as i32/96,HEIGHT*dpi as i32/96);
    let Some(RECT{left:x,top:y,..})=chrome::follow_rect(anchor,dock_dpi,w,h)else{return;};
    let Some(layer)=Layer::new(w,h)else{return;};
    let class=wide(CLASS);
    if !chrome::register_with_style(&class,Some(proc),0){return;}
    let hwnd=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_TOPMOST|WS_EX_LAYERED,class.as_ptr(),wide(text("Lịch TLauncher","TLauncher Calendar","TLauncher 日历")).as_ptr(),
        WS_POPUP,x,y,w,h,owner,ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
    if hwnd.is_null(){return;}
    chrome::disable_system_border(hwnd);
    let fonts=[(22,560),(12,460),(14,510),(25,490)].map(|(size,weight)|
        crate::native_text::font_pixels((size*dpi as i32+48)/96,weight));
    let local=Local::now();
    let glass=super::glass::Glass::capture(RECT{left:x,top:y,right:x+w,bottom:y+h},dock_dpi,anchor.dark,anchor.transparency);
    let mut panel=Panel{hwnd,owner,anchor,layer,dpi:dock_dpi,glass,shadow:None,fonts,local,view:local.month,following:true,
        dirty:true,first_weekday:first_weekday(),hover:None,focus:None,pressed:None,motion:None};
    panel.paint();
    if super::motion::native_allowed()&&SetTimer(hwnd,MOTION_TIMER,16,None)!=0{
        panel.motion=Some(super::motion::Tween::new(true,super::motion::CALENDAR,None).prepared());
    }
    if !panel.present(){DestroyWindow(hwnd);return;}
    PANEL.with(|slot|slot.replace(Some(panel)));
    SetTimer(hwnd,TIMER,1000,None);
    ShowWindow(hwnd,SW_SHOW);
    PANEL.with(|s|if let Some(p)=s.borrow_mut().as_mut().filter(|p|p.hwnd==hwnd){
        chrome::PanelShadow::sync(&mut p.shadow,hwnd,owner,RECT{left:x,top:y,right:x+w,bottom:y+h},dock_dpi,anchor.side,anchor.dark);
    });
    SetForegroundWindow(hwnd);
    SetFocus(hwnd);
    PANEL.with(|s|if let Some(p)=s.borrow_mut().as_mut().filter(|p|p.hwnd==hwnd){
        if let Some(t)=p.motion{let _=p.present();p.motion=Some(t.restart_clock());}
    });
}

unsafe extern "system" fn proc(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
    match msg{
        WM_ERASEBKGND=>1,
        WM_ACTIVATE if w&0xffff==WA_INACTIVE as usize=>{PostMessageW(hwnd,WM_CLOSE,0,0);0}
        WM_DPICHANGED=>{DestroyWindow(hwnd);0}
        WM_CLOSE=>{
            let animated=PANEL.with(|s|{
                let mut p=s.borrow_mut();let Some(p)=p.as_mut().filter(|p|p.hwnd==hwnd)else{return false;};
                if !super::motion::native_allowed(){return false;}
                if p.motion.is_some_and(|t|t.to==0.0){return true;}
                if SetTimer(hwnd,MOTION_TIMER,16,None)==0{return false;}
                p.motion=Some(super::motion::Tween::new(false,super::motion::CALENDAR,p.motion));true
            });
            if !animated{DestroyWindow(hwnd);}0
        }
        WM_TIMER if w==MOTION_TIMER=>{
            let mut finish=false;
            PANEL.with(|s|if let Ok(mut slot)=s.try_borrow_mut(){
                if let Some(p)=slot.as_mut().filter(|p|p.hwnd==hwnd){
                    if p.motion.is_some_and(|t|t.progress()>=1.0||!super::popup_motion_allowed()){
                        finish=p.motion.take().is_some_and(|t|t.to==0.0);KillTimer(hwnd,MOTION_TIMER);
                    }
                    if !finish{p.present();}
                }else{KillTimer(hwnd,MOTION_TIMER);}
            });
            if finish{DestroyWindow(hwnd);}0
        }
        WM_NCDESTROY=>{
            KillTimer(hwnd,TIMER);
            KillTimer(hwnd,MOTION_TIMER);
            let owner=PANEL.with(|s|{
                let mut p=s.borrow_mut();
                if p.as_ref().is_some_and(|p|p.hwnd==hwnd){p.take().map(|p|p.owner)}else{None}
            });
            if let Some(owner)=owner{super::panel_closed(owner);}
            DefWindowProcW(hwnd,msg,w,l)
        }
        WM_PAINT=>{
            let mut ps:PAINTSTRUCT=mem::zeroed();
            BeginPaint(hwnd,&mut ps);EndPaint(hwnd,&ps);
            let panel=PANEL.with(|s|{
                let mut s=s.borrow_mut();
                if s.as_ref().is_some_and(|p|p.hwnd==hwnd&&p.dirty){s.take()}else{None}
            });
            if let Some(mut p)=panel{
                p.paint();let _=p.present();
                if IsWindow(hwnd)!=0{PANEL.with(|s|{let mut s=s.borrow_mut();if s.is_none(){*s=Some(p);}});}
                else{super::panel_closed(p.owner);}
            }
            0
        }
        WM_KEYDOWN if w as u16==VK_ESCAPE=>{SendMessageW(hwnd,WM_CLOSE,0,0);0}
        WM_TIMER|WM_MOUSEMOVE|super::WM_DOCK_MOUSELEAVE|WM_LBUTTONDOWN|WM_LBUTTONUP|WM_CAPTURECHANGED|WM_KEYDOWN=>{
            let (dirty,capture)=PANEL.with(|s|{
                let mut s=s.borrow_mut();
                let Some(p)=s.as_mut().filter(|p|p.hwnd==hwnd)else{return (false,0);};
                let hit=p.hit(l as i16 as i32,(l>>16) as i16 as i32);
                let before=(p.hover,p.focus,p.pressed);
                let mut capture=0;
                match msg{
                    WM_TIMER if w==TIMER=>p.update_time(Local::now()),
                    WM_MOUSEMOVE=>{
                        TrackMouseEvent(&mut TRACKMOUSEEVENT{cbSize:mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags:TME_LEAVE,hwndTrack:hwnd,dwHoverTime:0});
                        p.hover=hit;
                    }
                    super::WM_DOCK_MOUSELEAVE=>p.hover=None,
                    WM_LBUTTONDOWN=>{p.pressed=hit;p.focus=None;if hit.is_some(){capture=1;}}
                    WM_LBUTTONUP=>if let Some(pressed)=p.pressed.take(){capture=-1;if hit==Some(pressed){p.activate(pressed);}},
                    WM_CAPTURECHANGED=>p.pressed=None,
                    WM_KEYDOWN=>match w as u16{
                        VK_LEFT=>p.activate(Button::Previous),VK_RIGHT=>p.activate(Button::Next),VK_HOME=>p.activate(Button::Today),
                        VK_TAB=>{
                            let back=GetKeyState(VK_SHIFT as i32)<0;
                            let current=p.focus.and_then(|b|Button::ALL.iter().position(|v|*v==b));
                            let i=current.map_or(if back{2}else{0},|i|(i+if back{2}else{1})%3);
                            p.focus=Some(Button::ALL[i]);
                        },
                        VK_SPACE|VK_RETURN=>if let Some(btn)=p.focus{p.activate(btn);},
                        _=>{},
                    },
                    _=>{},
                }
                p.dirty|=before!=(p.hover,p.focus,p.pressed);
                (p.dirty,capture)
            });
            if capture>0{SetCapture(hwnd);}else if capture<0{ReleaseCapture();}
            if dirty{InvalidateRect(hwnd,ptr::null(),0);}0
        }
        _=>DefWindowProcW(hwnd,msg,w,l)
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    const fn month(year:i32,number:i32)->Month{Month{year,number}}
    #[test]
    fn monday_first_full_grid_and_gregorian_rollover(){
        assert_eq!([month(2023,2).days(),month(2024,2).days(),month(2100,2).days(),month(2000,2).days()],[28,29,28,29]);
        assert_eq!(month(2024,4).days(),30);
        assert_eq!(month(2024,4).first(),0);
        let march=month(2026,3);
        assert_eq!(march.first(),6);assert_eq!(march.rows(),6);
        assert_eq!(march.date_at(0),(month(2026,2),23));assert_eq!(march.date_at(6),(march,1));
        assert_eq!(march.date_at(36),(march,31));assert_eq!(march.date_at(37),(month(2026,4),1));
        assert_eq!(month(2025,12).step(1),month(2026,1));
        assert_eq!(month(2026,1).step(-1),month(2025,12));
        assert_eq!(month(2026,2).date_at(0),(month(2026,1),26));
    }

    #[test]
    #[ignore = "uses a real layered popover and a fixture HWND"]
    fn native_calendar_navigates_and_presents_light_dark_months(){
        unsafe{
            struct Locale(String);impl Drop for Locale{fn drop(&mut self){crate::launcher::spotlight_locale(&self.0);}}
            let _locale=Locale(text("vi","en","zh-CN").into());
            let owner=CreateWindowExW(WS_EX_TOOLWINDOW,wide("STATIC").as_ptr(),wide("Calendar fixture").as_ptr(),
                WS_POPUP,40,40,160,48,ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());
            assert!(!owner.is_null());
            for language in ["vi","en","zh-CN"]{crate::launcher::spotlight_locale(language);for dark in [false,true]{
                let anchor=chrome::Anchor{point:POINT{x:260,y:480},side:super::super::DockSide::Bottom,dark,transparency:32};
                show(owner,anchor);
                assert!(open_for(owner));
                let hwnd=PANEL.with(|s|s.borrow().as_ref().unwrap().hwnd);
                assert_ne!(IsWindowVisible(hwnd),0);
                let work=anchor.work().unwrap();
                let mut rect:RECT=mem::zeroed();
                assert_ne!(GetWindowRect(hwnd,&mut rect),0);
                assert!(rect.left>=work.left&&rect.right<=work.right&&rect.top>=work.top&&rect.bottom<=work.bottom);
                SendMessageW(hwnd,WM_KEYDOWN,VK_LEFT as usize,0);
                PANEL.with(|s|{
                    let mut s=s.borrow_mut();let p=s.as_mut().unwrap();
                    assert_eq!(p.view,p.local.month.step(-1));
                    let next=Local{month:p.local.month.step(1),day:1,weekday:0,hour:0,minute:0};
                    p.update_time(next);
                    assert_eq!(p.view,next.month.step(-2)); // browsing remains on the chosen month
                    p.following=true;p.view=p.local.month;
                    p.update_time(Local{month:month(2026,3),day:23,weekday:0,hour:9,minute:41});
                    assert_eq!(p.view,month(2026,3)); // six rows, starts Sunday
                    p.paint();
                    if let Some(dir)=std::env::var_os("TUANPC_DESK_CALENDAR_EVIDENCE"){
                        let dir=std::path::PathBuf::from(dir);
                        std::fs::create_dir_all(&dir).unwrap();
                        let (w,h)=p.layer.size;
                        let rgba=p.layer.pixels().chunks_exact(4).flat_map(|src|{
                            let a=src[3] as u16;
                            let unpremul=|v:u8|if a==0{0}else{((v as u16*255+a/2)/a).min(255) as u8};
                            [unpremul(src[2]),unpremul(src[1]),unpremul(src[0]),src[3]]
                        }).collect::<Vec<_>>();
                        image::save_buffer(dir.join(format!("calendar-{language}-{}.png",if dark{"dark"}else{"light"})),
                            &rgba,w as u32,h as u32,image::ColorType::Rgba8).unwrap();
                    }
                });
                let shadow=PANEL.with(|s|s.borrow().as_ref().unwrap().shadow.as_ref().expect("calendar shadow").hwnd);
                let moved=crate::launcher::DockAnchor{point:POINT{x:anchor.point.x+18,y:anchor.point.y+32},side:anchor.side};
                follow(owner,moved);
                let mut moved_rect:RECT=mem::zeroed();GetWindowRect(hwnd,&mut moved_rect);
                let gap=chrome::scale(GetDpiForWindow(owner),chrome::PANEL_GAP);
                assert_eq!(moved.point.y-moved_rect.bottom,gap);
                PANEL.with(|s|{
                    let s=s.borrow();let p=s.as_ref().unwrap();
                    assert_eq!(p.view,month(2026,3));
                    assert_eq!(p.shadow.as_ref().unwrap().hwnd,shadow,"moving must reuse the shadow window");
                });
                SendMessageW(hwnd,WM_KEYDOWN,VK_HOME as usize,0);
                PANEL.with(|s|{let s=s.borrow();let p=s.as_ref().unwrap();assert_eq!(p.view,p.local.month);});
                SendMessageW(hwnd,WM_KEYDOWN,VK_ESCAPE as usize,0);
                assert!(!open_for(owner));
                assert_eq!(IsWindow(hwnd),0);
            }}
            super::super::perform_control(owner,super::super::control_center::Command::Calendar);
            assert!(open_for(owner),"Control Center opens the existing native calendar");close_for(owner);assert!(!open_for(owner));
            for speed in [25,50,70,75,100,150,200]{
                super::super::motion::native_fixture(speed,||{
                    let anchor=chrome::Anchor{point:POINT{x:260,y:480},side:super::super::DockSide::Bottom,dark:false,transparency:32};
                    show(owner,anchor);
                    let (hwnd,tween)=PANEL.with(|s|{let s=s.borrow();let p=s.as_ref().unwrap();(p.hwnd,p.motion.unwrap())});
                    assert_ne!(IsWindowVisible(hwnd),0);
                    assert_eq!(tween.from,0.0);
                    assert_eq!(tween.duration,(20000.0/speed as f64).round() as u32);
                    SendMessageW(hwnd,WM_KEYDOWN,VK_ESCAPE as usize,0);
                    assert_ne!(IsWindow(hwnd),0,"animated close keeps HWND until endpoint");
                    PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().unwrap();let t=p.motion.unwrap();
                        assert_eq!(t.duration,(15000.0/speed as f64).round() as u32);p.motion=Some(t.finish());});
                    SendMessageW(hwnd,WM_TIMER,MOTION_TIMER,0);
                    assert_eq!(IsWindow(hwnd),0);assert!(!open_for(owner));
                });
            }
            println!("MOTION_SPEED calendar_native=PASS six_speeds=PASS closing_endpoint=PASS");
            DestroyWindow(owner);
        }
    }
}
