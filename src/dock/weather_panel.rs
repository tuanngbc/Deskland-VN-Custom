//! Native forecast view over the same request/cache used by wallpaper weather.
use super::{STATE,control_center::{self,Card,Command,View}};
use crate::{weather,desktop::wide};
use std::{cell::RefCell,ptr};
use windows_sys::Win32::{Foundation::*,Globalization::*,UI::WindowsAndMessaging::*};
#[derive(Clone)]pub(super) enum Action{Open,Refresh(u64),Unit(u64,u8),Settings}
struct Panel{owner:usize,popup:usize,ticket:usize,generation:u64,unit:u8,weather:weather::Snapshot}
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
    fn view(&self)->View{
        let f=fahrenheit(self.unit);let unit=if f{"°F"}else{"°C"};let mut cards=Vec::new();
        if let Some(reading)=self.weather.reading.as_ref(){
            cards.push(Card{title:format!("{:.0}{unit} · {}",degrees(reading.temperature,f),condition(reading.code)),
                hint:format!("{} {:.0} {} · {} {:.0}%",text("Gió","Wind","风速"),if f{reading.wind/1.609344}else{reading.wind},if f{"mph"}else{"km/h"},text("Độ ẩm","Humidity","湿度"),reading.humidity),icon:None,action:None});
            for (index,day) in reading.days.iter().enumerate(){cards.push(Card{title:format!("{} · {:.0}–{:.0}{unit}",day_label(&day.date,index),degrees(day.low,f),degrees(day.high,f)),hint:condition(day.code),icon:None,action:None});}
        }
        if self.weather.location.is_some(){
            cards.push(Card{title:text("Làm mới","Refresh","刷新"),hint:String::new(),icon:None,action:(!self.weather.pending).then_some(Command::Weather(Action::Refresh(self.generation)))});
            for (choice,label) in [(0,text("Theo đơn vị Windows","Windows units","跟随 Windows 单位")),(1,"Celsius · °C".into()),(2,"Fahrenheit · °F".into())]{
                cards.push(Card{title:label,hint:if choice==self.unit{text("Đang chọn","Selected","已选择")}else{String::new()},icon:None,action:Some(Command::Weather(Action::Unit(self.generation,choice)))});
            }
        }
        cards.push(Card{title:text("Vị trí thời tiết trong Cài đặt","Weather location in Settings","在设置中选择天气位置"),hint:"Open-Meteo".into(),icon:None,action:Some(Command::Weather(Action::Settings))});
        let status=if self.weather.location.is_none(){text("Chọn thành phố trong Cài đặt","Choose a city in Settings","请在设置中选择城市")}
            else if self.weather.pending{text("Đang tải…","Loading…","正在加载…")}
            else if self.weather.failed{text("Chưa tải được thời tiết mới. Dữ liệu cũ có thể đã thay đổi.","Could not refresh weather. Previous data may have changed.","无法刷新天气，先前数据可能已更改。")}
            else{"Open-Meteo".into()};
        let location=self.weather.location.as_ref().map(|l|l.name.as_str()).unwrap_or("");
        View{title:if location.is_empty(){title()}else{format!("{} · {location}",title())},hint:status,cards,metrics:None}
    }
}
pub(super) fn detach(popup:HWND){PANEL.with(|s|{let mut s=s.borrow_mut();if s.as_ref().is_some_and(|p|p.popup==popup as usize){s.take();}});}
pub(super) unsafe fn receive(popup:HWND,ticket:usize){
    if !enabled()||!PANEL.with(|s|s.borrow().as_ref().is_some_and(|p|p.popup==popup as usize&&p.ticket==ticket)){return;}
    let current=weather::snapshot();if current.location.is_some()&&current.reading.is_none()&&!current.failed&&!current.pending{weather::request(false);}
    let update=PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().filter(|p|p.popup==popup as usize&&p.ticket==ticket)?;
        p.weather=weather::snapshot();p.generation=p.generation.wrapping_add(1);Some((p.owner,p.ticket,p.view()))
    });if let Some((owner,ticket,view))=update{if enabled(){control_center::update(owner as HWND,ticket,view);}}
}
pub(super) unsafe fn perform(owner:HWND,popup:HWND,action:Action){
    if !enabled()||IsWindow(owner)==0{return;}
    if matches!(action,Action::Settings){super::emit(super::Event::Utility(super::HostAction::Settings));return;}
    if matches!(action,Action::Open){
        super::utilities::close_panels();weather::request(false);
        control_center::show_list_capacity(owner,super::dock_app_anchor(owner,None),View{title:title(),hint:String::new(),cards:Vec::new(),metrics:None},12);
        let (Some(window),Some(ticket))=(control_center::window(owner),control_center::ticket(owner))else{return;};
        weather::subscribe(window,ticket);
        let panel=Panel{owner:owner as usize,popup:window as usize,ticket,generation:1,unit:STATE.with(|s|s.borrow().prefs.weather_unit),weather:weather::snapshot()};
        let view=panel.view();PANEL.with(|s|s.replace(Some(panel)));control_center::update(owner,ticket,view);return;
    }
    let mut changed_unit=None;
    let update=PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().filter(|p|p.owner==owner as usize&&p.popup==popup as usize)?;
        match action{
            Action::Refresh(g) if g==p.generation&&!p.weather.pending&&p.weather.generation==weather::snapshot().generation=>{weather::request(true);p.weather=weather::snapshot();},
            Action::Unit(g,unit) if g==p.generation&&unit<=2=>{p.unit=unit;changed_unit=Some(unit);},_=>return None,
        }
        p.generation=p.generation.wrapping_add(1);Some((p.ticket,p.view()))
    });
    if let Some(unit)=changed_unit{STATE.with(|s|s.borrow_mut().prefs.weather_unit=unit);super::emit(super::Event::Edit(super::Edit::WeatherUnit(unit)));}
    if let Some((ticket,view))=update{control_center::update(owner,ticket,view);}
}
pub(super) fn reset(){PANEL.with(|s|s.take());}
#[cfg(test)]mod tests{
    use super::*;
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
                let (w,h)=(bounds.right-bounds.left,bounds.bottom-bounds.top);let mut layer=super::super::glass_probe::Layer::new(w,h).unwrap();
                let dc=GetDC(ptr::null_mut());assert_ne!(BitBlt(layer.dc(),0,0,w,h,dc,bounds.left,bounds.top,SRCCOPY|CAPTUREBLT),0);ReleaseDC(ptr::null_mut(),dc);GdiFlush();
                if let Some(root)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){
                    let rgba=layer.pixels().chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],255]).collect::<Vec<_>>();
                    image::save_buffer(std::path::PathBuf::from(root).join(format!("weather-synthetic-{locale}.png")),&rgba,w as u32,h as u32,image::ColorType::Rgba8).unwrap();
                }
                // Changing city clears the old forecast immediately; an old
                // panel action cannot refresh or relabel it as the new city.
                weather::configure(None);receive(popup,ticket);
                assert!(PANEL.with(|s|s.borrow().as_ref().unwrap().weather.reading.is_none()));
                perform(owner,popup,Action::Refresh(generation));assert!(weather::current().is_none());
                control_center::close_for(owner);assert!(PANEL.with(|s|s.borrow().is_none()));
                assert_eq!(weather::fixture_subscribers(),0);
                receive(popup,ticket);assert!(PANEL.with(|s|s.borrow().is_none()));assert_eq!(IsWindow(popup),0);
            }
            reset();weather::configure(None);DestroyWindow(owner);
        }
        println!("FLUIDWIN_WEATHER shared_wallpaper_reading=PASS locales=vi,en,zh unit_conversion=PASS stale_city=REFUSED closed_subscription=RELEASED actual_network=NOT_INVOKED native_city_search=SETTINGS_ROUTE_ONLY");
    }
    #[test]fn original_wmo_groups_and_unit_conversion_preserve_forecast_values(){
        assert_eq!(degrees(0.0,true),32.0);assert_eq!(degrees(100.0,true),212.0);assert_eq!(degrees(-15.0,false),-15.0);
        assert!(!condition(95).is_empty());assert!(!day_label("2026-11-27",1).is_empty());
        let mut options=super::super::Options::default();super::super::Edit::WeatherUnit(2).apply(&mut options);assert_eq!(super::super::Options::from_json(&options.json()).weather_unit,2);
    }
}
