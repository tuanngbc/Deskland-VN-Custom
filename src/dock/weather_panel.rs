//! Native forecast view over the same request/cache used by wallpaper weather.
use super::{STATE,control_center::{self,Card,Command,View}};
use crate::{weather,desktop::wide};
use std::{cell::RefCell,ptr};
use windows_sys::Win32::{Foundation::*,Globalization::*,UI::WindowsAndMessaging::*};
#[derive(Clone)]pub(super) enum Action{Open,Widgets,Refresh(u64),Unit(u64,u8),City,Settings}
struct Panel{owner:usize,popup:usize,ticket:usize,generation:u64,unit:u8,weather:weather::Snapshot,combined:bool}
thread_local!{static PANEL:RefCell<Option<Panel>>=const{RefCell::new(None)};}
impl Drop for Panel{fn drop(&mut self){weather::unsubscribe(self.popup as HWND);}}
fn text(vi:&str,en:&str,zh:&str)->String{crate::launcher::localized(vi,en,zh).into()}
fn title()->String{text("Thời tiết","Weather","天气")}
fn enabled()->bool{STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended})}
fn fahrenheit(unit:u8)->bool{
    if unit!=0{return unit==2;}
    let mut value=[0u16;4];unsafe{GetLocaleInfoEx(ptr::null(),LOCALE_IMEASURE,value.as_mut_ptr(),4)>0&&value[0]==b'1' as u16}
}
fn degrees(value:f64,f:bool)->f64{if f{value*9.0/5.0+32.0}else{value}}
fn condition(code:i32)->String{match code{
    0=>text("Trời quang","Clear","晴"),1=>text("Ít mây","Mostly clear","大部晴朗"),2=>text("Có mây","Partly cloudy","局部多云"),3=>text("Nhiều mây","Overcast","阴"),
    45|48=>text("Sương mù","Fog","雾"),51..=57=>text("Mưa phùn","Drizzle","毛毛雨"),61..=67=>text("Mưa","Rain","雨"),71..=77=>text("Tuyết","Snow","雪"),
    80..=82=>text("Mưa rào","Showers","阵雨"),85..=86=>text("Mưa tuyết","Snow showers","阵雪"),95..=99=>text("Giông","Thunderstorm","雷暴"),_=>text("Thời tiết","Weather","天气"),
}}
fn day_label(date:&str,index:usize)->String{
    if index==0{return text("Hôm nay","Today","今天");}
    let parts=date.split('-').filter_map(|v|v.parse::<u16>().ok()).collect::<Vec<_>>();if parts.len()!=3{return date.into();}
    let time=SYSTEMTIME{wYear:parts[0],wMonth:parts[1],wDay:parts[2],..unsafe{std::mem::zeroed()}};
    let mut value=[0u16;32];let count=unsafe{GetDateFormatEx(wide(&text("vi-VN","en-US","zh-CN")).as_ptr(),0,&time,wide("ddd").as_ptr(),value.as_mut_ptr(),32,ptr::null())};
    if count>1{format!("{} · {}",String::from_utf16_lossy(&value[..count as usize-1]),date)}else{date.into()}
}
impl Panel{
    fn forecast(&self)->super::forecast_card::Card{
        let f=fahrenheit(self.unit);let unit=if f{"°F"}else{"°C"};
        let city=self.weather.location.as_ref().map(|l|l.name.clone()).unwrap_or_else(||text("Chọn thành phố","Choose a city","选择城市"));
        let Some(reading)=self.weather.reading.as_ref()else{return super::forecast_card::Card::new(city,String::new(),if self.weather.failed{text("Chưa tải được thời tiết","Weather unavailable","暂时无法获取天气")}else{text("Đang tải thời tiết…","Loading weather…","正在加载天气…")},None,Vec::new());};
        let detail=format!("{} · {} {}% · {:.0} {}",condition(reading.code),text("ẩm","humidity","湿度"),reading.humidity,if f{reading.wind/1.609344}else{reading.wind},if f{"mph"}else{"km/h"});
        let days=reading.days.iter().take(5).enumerate().map(|(i,day)|super::forecast_card::Day{code:day.code,label:day_label(&day.date,i).split(" · ").next().unwrap_or("").into(),temperature:format!("{:.0}°  {:.0}°",degrees(day.high,f),degrees(day.low,f))}).collect();
        super::forecast_card::Card::new(city,format!("{:.0}{unit}",degrees(reading.temperature,f)),detail,Some(reading.code),days)
    }
    fn view(&self)->View{
        let f=fahrenheit(self.unit);let unit=if f{"°F"}else{"°C"};let mut cards=Vec::new();
        if let Some(reading)=self.weather.reading.as_ref(){
            cards.push(Card{title:format!("{} · {:.0}{unit} · {}",self.weather.location.as_ref().map(|l|l.name.as_str()).unwrap_or(""),degrees(reading.temperature,f),condition(reading.code)),
                hint:format!("{} {:.0} {} · {} {:.0}%",text("Gió","Wind","风速"),if f{reading.wind/1.609344}else{reading.wind},if f{"mph"}else{"km/h"},text("Độ ẩm","Humidity","湿度"),reading.humidity),icon:None,action:Some(Command::Weather(Action::City))});
            for (index,day) in reading.days.iter().enumerate(){cards.push(Card{title:format!("{} · {:.0}–{:.0}{unit}",day_label(&day.date,index),degrees(day.low,f),degrees(day.high,f)),hint:condition(day.code),icon:None,action:None});}
        }else{cards.push(Card{title:self.weather.location.as_ref().map(|l|l.name.clone()).unwrap_or_else(||text("Chọn thành phố để xem thời tiết","Choose a city to view weather","选择城市以查看天气")),hint:if self.weather.pending{text("Đang tải…","Loading…","正在加载…")}else{String::new()},icon:None,action:Some(Command::Weather(Action::City))});}
        if self.weather.location.is_some(){
            cards.push(Card{title:text("Làm mới","Refresh","刷新"),hint:String::new(),icon:None,action:(!self.weather.pending).then_some(Command::Weather(Action::Refresh(self.generation)))});
            for (choice,label) in [(0,text("Theo đơn vị Windows","Windows units","跟随 Windows 单位")),(1,"Celsius · °C".into()),(2,"Fahrenheit · °F".into())]{
                cards.push(Card{title:label,hint:if choice==self.unit{text("Đang chọn","Selected","已选择")}else{String::new()},icon:None,action:Some(Command::Weather(Action::Unit(self.generation,choice)))});
            }
        }
        cards.push(Card{title:text("Chọn thành phố","Choose a city","选择城市"),hint:"Open-Meteo".into(),icon:None,action:Some(Command::Weather(Action::City))});
        cards.push(Card{title:text("Vị trí thời tiết trong Cài đặt","Weather location in Settings","在设置中选择天气位置"),hint:String::new(),icon:None,action:Some(Command::Weather(Action::Settings))});
        let status=if self.weather.location.is_none(){text("Chọn thành phố","Choose a city","请选择城市")}
            else if self.weather.pending{text("Đang tải…","Loading…","正在加载…")}
            else if self.weather.failed{text("Chưa tải được thời tiết mới. Dữ liệu cũ có thể đã thay đổi.","Could not refresh weather. Previous data may have changed.","无法刷新天气，先前数据可能已更改。")}
            else{"Open-Meteo".into()};
        let location=self.weather.location.as_ref().map(|l|l.name.as_str()).unwrap_or("");
        if self.combined{
            // The illustrated weather card pins its summary/five forecast
            // actions. Keep News and its original source/action owner below.
            cards.truncate(self.forecast().rows());
            cards.push(Card{title:format!("{} · {unit}",text("Đơn vị thời tiết","Weather units","天气单位")),hint:text("Đổi đơn vị","Change units","切换单位"),icon:None,action:Some(Command::Weather(Action::Unit(self.generation,(self.unit+1)%3)))});
            if self.weather.location.is_some(){cards.push(Card{title:text("Làm mới thời tiết","Refresh weather","刷新天气"),hint:String::new(),icon:None,action:(!self.weather.pending).then_some(Command::Weather(Action::Refresh(self.generation)))});}
            let news=super::news::view_for(self.popup as HWND,self.ticket);
            if let Some(news)=news{
                cards.push(Card{title:text("Tin tức","News","新闻"),hint:news.hint,icon:None,action:None});
                cards.extend(news.cards);
            }
            return View{title:text("Thời tiết và Tin tức","Weather and News","天气和新闻"),hint:status,cards,metrics:None};
        }
        View{title:if location.is_empty(){title()}else{format!("{} · {location}",title())},hint:status,cards,metrics:None}
    }
}
pub(super) fn detach(popup:HWND){PANEL.with(|s|{let mut s=s.borrow_mut();if s.as_ref().is_some_and(|p|p.popup==popup as usize){s.take();}});}
pub(super) fn combined_for(owner:HWND,popup:HWND,ticket:usize)->bool{
    PANEL.with(|s|s.borrow().as_ref().is_some_and(|p|p.combined&&p.owner==owner as usize&&p.popup==popup as usize&&p.ticket==ticket))
}
pub(super) unsafe fn refresh_combined(owner:HWND,popup:HWND,ticket:usize){
    let update=PANEL.with(|s|{let s=s.borrow();let p=s.as_ref().filter(|p|p.combined&&p.owner==owner as usize&&p.popup==popup as usize&&p.ticket==ticket)?;Some((p.view(),p.forecast()))});
    if let Some((view,forecast))=update{if enabled(){control_center::update_forecast(owner,ticket,view,forecast);}}
}
pub(super) unsafe fn receive(popup:HWND,ticket:usize){
    if !enabled()||!PANEL.with(|s|s.borrow().as_ref().is_some_and(|p|p.popup==popup as usize&&p.ticket==ticket)){return;}
    let current=weather::snapshot();if current.location.is_some()&&current.reading.is_none()&&!current.failed&&!current.pending{weather::request(false);}
    let update=PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().filter(|p|p.popup==popup as usize&&p.ticket==ticket)?;
        p.weather=weather::snapshot();p.generation=p.generation.wrapping_add(1);Some((p.owner,p.ticket,p.view(),p.forecast()))
    });if let Some((owner,ticket,view,forecast))=update{if enabled(){control_center::update_forecast(owner as HWND,ticket,view,forecast);}}
}
pub(super) unsafe fn perform(owner:HWND,popup:HWND,action:Action){
    if !enabled()||IsWindow(owner)==0{return;}
    if matches!(action,Action::City){super::weather_city::open(owner);return;}
    if matches!(action,Action::Settings){super::emit(super::Event::Utility(super::HostAction::Settings));return;}
    if matches!(action,Action::Open|Action::Widgets){
        super::utilities::close_panels();weather::request(false);
        let combined=matches!(action,Action::Widgets);
        let mut panel=Panel{owner:owner as usize,popup:0,ticket:0,generation:1,unit:STATE.with(|s|s.borrow().prefs.weather_unit),weather:weather::snapshot(),combined};
        if combined{control_center::show_widgets(owner,super::dock_app_anchor(owner,None),panel.view(),panel.forecast());}
        else{control_center::show_forecast(owner,super::dock_app_anchor(owner,None),panel.view(),panel.forecast());}
        let (Some(window),Some(ticket))=(control_center::window(owner),control_center::ticket(owner))else{return;};
        weather::subscribe(window,ticket);
        panel.popup=window as usize;panel.ticket=ticket;PANEL.with(|s|s.replace(Some(panel)));
        if combined{super::news::attach(owner,window,ticket);refresh_combined(owner,window,ticket);}return;
    }
    let mut changed_unit=None;
    let update=PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().filter(|p|p.owner==owner as usize&&p.popup==popup as usize)?;
        match action{
            Action::Refresh(g) if g==p.generation&&!p.weather.pending&&p.weather.generation==weather::snapshot().generation=>{weather::request(true);p.weather=weather::snapshot();},
            Action::Unit(g,unit) if g==p.generation&&unit<=2=>{p.unit=unit;changed_unit=Some(unit);},_=>return None,
        }
        p.generation=p.generation.wrapping_add(1);Some((p.ticket,p.view(),p.forecast()))
    });
    if let Some(unit)=changed_unit{STATE.with(|s|s.borrow_mut().prefs.weather_unit=unit);super::emit(super::Event::Edit(super::Edit::WeatherUnit(unit)));}
    if let Some((ticket,view,forecast))=update{control_center::update_forecast(owner,ticket,view,forecast);}
}
pub(super) fn reset(){PANEL.with(|s|s.take());}
#[cfg(test)]mod tests{
    use super::*;
    #[test]
    #[ignore="hosted combined Weather/News native pixels, independent actions and shared lifetime; no network/browser"]
    fn native_combined_widgets_keep_forecast_news_actions_and_lifetime(){
        use windows_sys::Win32::{System::LibraryLoader::GetModuleHandleW,Graphics::{Dwm::DwmFlush,Gdi::*},UI::Input::KeyboardAndMouse::*};
        use std::time::{Instant,Duration};
        use super::super::news;
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(),Ok("true"));
        struct Locale(String);impl Drop for Locale{fn drop(&mut self){crate::launcher::spotlight_locale(&self.0);}}
        let _locale=Locale(text("vi","en","zh-CN"));
        unsafe{
            STATE.with(|s|{let mut s=s.borrow_mut();s.prefs.enabled=true;s.prefs.appearance.theme=super::super::Theme::Light;s.prefs.weather_unit=1;s.prefs.news_source=2;});
            let owner=CreateWindowExW(WS_EX_TOOLWINDOW,wide("STATIC").as_ptr(),wide("Combined widgets fixture").as_ptr(),WS_POPUP,
                10,10,260,80,ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());assert!(!owner.is_null());
            let pump=|done:&dyn Fn()->bool|{
                let start=Instant::now();while !done(){
                    let mut msg:MSG=std::mem::zeroed();for _ in 0..64{if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&msg);DispatchMessageW(&msg);}
                    assert!(start.elapsed()<Duration::from_secs(5));std::thread::sleep(Duration::from_millis(10));
                }
            };
            for locale in ["vi","en","zh-CN"]{
                crate::launcher::spotlight_locale(locale);weather::fixture_reading();
                STATE.with(|s|{let mut s=s.borrow_mut();s.prefs.weather_unit=1;s.prefs.news_source=2;});
                let shared=weather::current().unwrap();perform(owner,ptr::null_mut(),Action::Widgets);
                let popup=control_center::window(owner).unwrap();let ticket=control_center::ticket(owner).unwrap();
                pump(&news::fixture_idle);let news_generation=news::fixture_article(popup,ticket);
                assert!(combined_for(owner,popup,ticket));assert_eq!(weather::fixture_subscribers(),1);
                let generation=PANEL.with(|s|{let s=s.borrow();let p=s.as_ref().unwrap();assert!(std::sync::Arc::ptr_eq(&shared,p.weather.reading.as_ref().unwrap()));p.generation});
                // Each model keeps its own action generation. News source
                // navigation must not replace or restart the weather owner.
                news::perform(owner,popup,news::Action::Sources(news_generation.wrapping_sub(1)));
                assert!(news::view_for(popup,ticket).unwrap().cards.iter().any(|c|matches!(c.action.as_ref(),Some(Command::News(news::Action::Article(..))))));
                news::perform(owner,popup,news::Action::Sources(news_generation));
                assert!(news::view_for(popup,ticket).unwrap().cards.iter().any(|c|matches!(c.action.as_ref(),Some(Command::News(news::Action::Source(..))))));
                // Exercise actual native keyboard paging while the forecast
                // remains pinned above a longer source list.
                SendMessageW(popup,WM_KEYDOWN,VK_END as usize,0);SendMessageW(popup,WM_KEYDOWN,VK_RIGHT as usize,0);
                assert_eq!(control_center::window(owner),Some(popup));assert_eq!(control_center::ticket(owner),Some(ticket));
                assert_eq!(PANEL.with(|s|s.borrow().as_ref().unwrap().generation),generation);
                news::perform(owner,popup,news::Action::Back(news_generation));
                perform(owner,popup,Action::Unit(generation,2));
                perform(owner,popup,Action::Unit(generation,1)); // old action is refused
                assert_eq!(STATE.with(|s|s.borrow().prefs.weather_unit),2);
                assert!(news::view_for(popup,ticket).unwrap().cards.iter().any(|c|c.hint=="14:35"));
                // Find the article through real keyboard page changes, rather
                // than assuming a runner resolution or fixed row index.
                let mut article=control_center::fixture_news_rect(owner);
                for _ in 0..12{if article.is_some(){break;}SendMessageW(popup,WM_KEYDOWN,VK_END as usize,0);SendMessageW(popup,WM_KEYDOWN,VK_RIGHT as usize,0);article=control_center::fixture_news_rect(owner);}
                let article=article.expect("combined panel must expose the original News article on a reachable page");
                let until=Instant::now()+Duration::from_millis(450);pump(&||Instant::now()>=until);
                UpdateWindow(popup);DwmFlush();let mut bounds:RECT=std::mem::zeroed();GetWindowRect(popup,&mut bounds);
                let (w,h)=(bounds.right-bounds.left,bounds.bottom-bounds.top);let mut layer=super::super::glass_probe::Layer::new(w,h).unwrap();
                let screen=GetDC(ptr::null_mut());assert_ne!(BitBlt(layer.dc(),0,0,w,h,screen,bounds.left,bounds.top,SRCCOPY|CAPTUREBLT),0);ReleaseDC(ptr::null_mut(),screen);GdiFlush();
                let rgba=layer.pixels().chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],255]).collect::<Vec<_>>();
                if let Some(root)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){image::save_buffer(std::path::PathBuf::from(root).join(format!("widgets-synthetic-{locale}.png")),&rgba,w as u32,h as u32,image::ColorType::Rgba8).unwrap();}
                let blue=rgba.chunks_exact(4).filter(|p|p[2].saturating_sub(p[0])>40&&p[1].saturating_sub(p[0])>20).count();
                assert!(blue>4000,"same displayed HWND must retain actual illustrated weather: {locale} {blue}");
                let px=|n|super::super::panel_chrome::scale(windows_sys::Win32::UI::HiDpi::GetDpiForWindow(popup),n);
                assert!(article.top>=px(243)&&article.bottom<=h);
                for (a,b) in [(7,22),(24,39)]{
                    let ink=rgba.chunks_exact(4).enumerate().filter(|(i,p)|{let x=*i as i32%w;let y=*i as i32/w;x>=article.left+px(8)&&x<article.right-px(52)&&y>=article.top+px(a)&&y<article.top+px(b)&&p[..3].iter().all(|c|*c<100)}).count();
                    assert!(ink>15,"same displayed HWND must contain News line {a}: {locale} {ink}");
                }
                #[cfg(target_env="msvc")]let access={let a=super::super::control_access::get(popup);assert!(!a.is_null());assert_eq!(super::super::control_access::probe(a,control_center::fixture_access_count(owner).unwrap(),0,0),0);a};
                // Clearing a city must clear only its forecast, leaving the
                // current news article/model and this exact HWND intact.
                weather::configure(None);receive(popup,ticket);
                assert!(PANEL.with(|s|s.borrow().as_ref().unwrap().weather.reading.is_none()));
                assert!(news::view_for(popup,ticket).unwrap().cards.iter().any(|c|c.hint=="14:35"));
                assert_eq!(control_center::window(owner),Some(popup));perform(owner,popup,Action::Refresh(generation));assert!(weather::current().is_none());
                news::perform(owner,popup,news::Action::Source(news_generation,5));
                news::perform(owner,popup,news::Action::Source(news_generation,2));assert_eq!(STATE.with(|s|s.borrow().prefs.news_source),5);
                let closed_delivery=news::fixture_closed_delivery(popup,ticket);control_center::close_for(owner);
                assert!(PANEL.with(|s|s.borrow().is_none()));assert_eq!(weather::fixture_subscribers(),0);
                closed_delivery();receive(popup,ticket);assert_eq!(IsWindow(popup),0);
                #[cfg(target_env="msvc")]{assert_eq!(super::super::control_access::probe(access,0,0,3),0);super::super::control_access::release(access);}
                news::reset();
            }
            reset();news::reset();weather::configure(None);DestroyWindow(owner);
        }
        println!("FLUIDWIN_WIDGETS same_HWND_ticket=PASS shared_weather=PASS independent_generations=PASS native_keyboard_pages=PASS actual_weather_and_two_news_lines=PASS locales=vi,en,zh MSAA=PASS closed_delivery=REFUSED subscription=RELEASED network_browser=NOT_INVOKED inline_city_layout=INCOMPLETE");
    }
    #[test]
    #[ignore="native forecast panel over shared synthetic weather, hosted Windows only"]
    fn native_shared_forecast_panel_keeps_city_generation_and_releases_subscription(){
        use windows_sys::Win32::{System::LibraryLoader::GetModuleHandleW,Graphics::{Dwm::DwmFlush,Gdi::*}};
        use std::time::{Instant,Duration};
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(),Ok("true"));
        struct Locale(String);impl Drop for Locale{fn drop(&mut self){crate::launcher::spotlight_locale(&self.0);}}
        let _locale=Locale(text("vi","en","zh-CN"));
        unsafe{
            STATE.with(|s|s.borrow_mut().prefs.enabled=true);
            let owner=CreateWindowExW(WS_EX_TOOLWINDOW,wide("STATIC").as_ptr(),wide("Weather fixture").as_ptr(),WS_POPUP,
                10,10,260,80,ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());assert!(!owner.is_null());
            for locale in ["vi","en","zh-CN"]{
                crate::launcher::spotlight_locale(locale);weather::fixture_reading();
                let shared=weather::current().unwrap();perform(owner,ptr::null_mut(),Action::Open);
                let popup=control_center::window(owner).unwrap();let (generation,ticket)=PANEL.with(|s|{let s=s.borrow();let p=s.as_ref().unwrap();
                    assert!(std::sync::Arc::ptr_eq(&shared,p.weather.reading.as_ref().unwrap()));(p.generation,p.ticket)
                });
                perform(owner,popup,Action::Unit(generation.wrapping_sub(1),2));assert_eq!(PANEL.with(|s|s.borrow().as_ref().unwrap().generation),generation);
                perform(owner,popup,Action::Unit(generation,2));assert_eq!(STATE.with(|s|s.borrow().prefs.weather_unit),2);
                let view=PANEL.with(|s|s.borrow().as_ref().unwrap().view());assert!(view.cards.iter().any(|c|c.title.contains("77°F")));assert!(view.cards.iter().any(|c|c.title.contains("68–84°F")));
                let until=Instant::now()+Duration::from_millis(450);while Instant::now()<until{
                    let mut msg:MSG=std::mem::zeroed();for _ in 0..64{if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&msg);DispatchMessageW(&msg);}
                    std::thread::sleep(Duration::from_millis(10));
                }
                UpdateWindow(popup);DwmFlush();let mut bounds:RECT=std::mem::zeroed();GetWindowRect(popup,&mut bounds);
                #[cfg(target_env="msvc")]let accessible={
                    let accessible=super::super::control_access::get(popup);assert!(!accessible.is_null());
                    assert_eq!(super::super::control_access::probe(accessible,control_center::fixture_access_count(owner).unwrap(),0,0),0,"illustrated forecast needs actual MSAA names/roles/bounds/hit tests");accessible
                };
                let (w,h)=(bounds.right-bounds.left,bounds.bottom-bounds.top);let mut layer=super::super::glass_probe::Layer::new(w,h).unwrap();
                let dc=GetDC(ptr::null_mut());assert_ne!(BitBlt(layer.dc(),0,0,w,h,dc,bounds.left,bounds.top,SRCCOPY|CAPTUREBLT),0);ReleaseDC(ptr::null_mut(),dc);GdiFlush();
                if let Some(root)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){
                    let rgba=layer.pixels().chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],255]).collect::<Vec<_>>();
                    image::save_buffer(std::path::PathBuf::from(root).join(format!("weather-synthetic-{locale}.png")),&rgba,w as u32,h as u32,image::ColorType::Rgba8).unwrap();
                }
                let blue=layer.pixels().chunks_exact(4).filter(|p|p[0].saturating_sub(p[2])>40&&p[1].saturating_sub(p[2])>20).count();
                assert!(blue>4000,"actual native weather card must reach the display for {locale}: {blue}");
                let dpi=windows_sys::Win32::UI::HiDpi::GetDpiForWindow(popup);let px=|n|super::super::panel_chrome::scale(dpi,n);
                let cw=(w-px(44))/5;let pixels=layer.pixels();
                for day in 0..5{let left=px(22)+cw*day;let right=left+cw;let glyph=pixels.chunks_exact(4).enumerate().filter(|(i,p)|{let x=*i as i32%w;let y=*i as i32/w;x>=left&&x<right&&y>=px(184)&&y<px(212)&&p[..3].iter().all(|c|*c>220)}).count();assert!(glyph>6,"actual five-day glyph {day} is missing for {locale}: {glyph}");}
                // Changing city clears the old forecast immediately; an old
                // panel action cannot refresh or relabel it as the new city.
                weather::configure(None);receive(popup,ticket);
                assert!(PANEL.with(|s|s.borrow().as_ref().unwrap().weather.reading.is_none()));
                perform(owner,popup,Action::Refresh(generation));assert!(weather::current().is_none());
                control_center::close_for(owner);assert!(PANEL.with(|s|s.borrow().is_none()));
                #[cfg(target_env="msvc")]{assert_eq!(super::super::control_access::probe(accessible,0,0,3),0);super::super::control_access::release(accessible);}
                assert_eq!(weather::fixture_subscribers(),0);
                receive(popup,ticket);assert!(PANEL.with(|s|s.borrow().is_none()));assert_eq!(IsWindow(popup),0);
            }
            reset();weather::configure(None);DestroyWindow(owner);
        }
        println!("FLUIDWIN_WEATHER shared_wallpaper_reading=PASS locales=vi,en,zh unit_conversion=PASS stale_city=REFUSED closed_subscription=RELEASED actual_network=NOT_INVOKED native_city_search=SEPARATE_NATIVE_FIXTURE");
    }
    #[test]fn original_wmo_groups_and_unit_conversion_preserve_forecast_values(){
        assert_eq!(degrees(0.0,true),32.0);assert_eq!(degrees(100.0,true),212.0);assert_eq!(degrees(-15.0,false),-15.0);
        assert!(!condition(95).is_empty());assert!(!day_label("2026-11-27",1).is_empty());
        let mut options=super::super::Options::default();super::super::Edit::WeatherUnit(2).apply(&mut options);assert_eq!(super::super::Options::from_json(&options.json()).weather_unit,2);
    }
}
