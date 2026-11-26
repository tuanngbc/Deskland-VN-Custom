//! One weather owner shared by wallpaper lighting and native Control Center.
//! Open-Meteo current/five-day fields follow FluidWin widget.h, with structured
//! JSON decoding, identity-checked publication and no persistent network loop.
use std::{ffi::c_void,io::Read,ptr,sync::{Arc,Mutex,OnceLock},time::{Duration,Instant}};
use serde_json::Value;
use windows_sys::Win32::{Foundation::HWND,System::Threading::{TrySubmitThreadpoolCallback,PTP_CALLBACK_INSTANCE},UI::WindowsAndMessaging::{PostMessageW,WM_APP}};
pub(crate) const READY:u32=WM_APP+161;
#[derive(Clone,Debug,PartialEq)]pub(crate) struct Location{pub name:String,pub latitude:f64,pub longitude:f64}
impl Location{
    pub(crate) fn read(value:&Value)->Option<Self>{
        let (latitude,longitude)=(value["latitude"].as_f64()?,value["longitude"].as_f64()?);
        if !latitude.is_finite()||!longitude.is_finite()||!(-90.0..=90.0).contains(&latitude)||!(-180.0..=180.0).contains(&longitude){return None;}
        Some(Self{name:value["name"].as_str().unwrap_or("").chars().filter(|c|!c.is_control()).take(160).collect(),latitude,longitude})
    }
}
// Shared by the existing Settings page and the native city picker. Keep only
// bounded location metadata; neither caller persists a query/result history.
fn geocode_rows(root:&Value)->Vec<Value>{
    root["results"].as_array().into_iter().flatten().take(6).filter_map(|row|{
        let location=Location::read(row)?;if location.name.trim().is_empty(){return None;}
        let mut result=serde_json::json!({"name":location.name,"latitude":location.latitude,"longitude":location.longitude});
        for key in ["admin1","country","country_code","timezone"]{
            if let Some(value)=row[key].as_str(){result[key]=Value::String(value.chars().filter(|c|!c.is_control()).take(160).collect());}
        }
        if let Some(id)=row["id"].as_i64(){result["id"]=Value::from(id);}
        Some(result)
    }).collect()
}
pub(crate) fn geocode(query:&str,language:&str)->Result<Vec<Value>,()>{
    let query=query.trim();if query.chars().count()<2{return Ok(Vec::new());}
    if query.encode_utf16().count()>60||query.chars().any(char::is_control){return Err(());}
    if cfg!(test){return Err(());}
    let language=match language{"vi"=>"vi","zh"|"zh-CN"=>"zh",_=>"en"};
    let client=reqwest::blocking::Client::builder().user_agent("TLauncher/1.0.3").https_only(true)
        .connect_timeout(Duration::from_secs(4)).timeout(Duration::from_secs(15)).redirect(reqwest::redirect::Policy::none()).build().map_err(|_|())?;
    let response=client.get("https://geocoding-api.open-meteo.com/v1/search")
        .query(&[("name",query),("count","6"),("language",language),("format","json")]).send().map_err(|_|())?;
    if response.status()!=reqwest::StatusCode::OK{return Err(());}
    let mut bytes=Vec::new();response.take(128*1024+1).read_to_end(&mut bytes).map_err(|_|())?;
    if bytes.len()>128*1024{return Err(());}
    let root=serde_json::from_slice::<Value>(&bytes).map_err(|_|())?;
    if root["error"].as_bool()==Some(true){return Err(());}
    Ok(geocode_rows(&root))
}
#[derive(Clone,Debug)]pub(crate) struct Day{pub date:String,pub code:i32,pub high:f64,pub low:f64}
#[derive(Clone,Debug)]pub(crate) struct Reading{
    pub temperature:f64,pub code:i32,pub wind:f64,pub humidity:f64,
    pub cloud:f32,pub rain:f32,pub snow:f32,pub days:Vec<Day>,
}
fn number(v:&Value,key:&str,min:f64,max:f64)->Option<f64>{let n=v[key].as_f64()?;(n.is_finite()&&(min..=max).contains(&n)).then_some(n)}
fn parse(value:&Value)->Option<Reading>{
    let current=&value["current"];
    let code=number(current,"weather_code",0.0,99.0)? as i32;
    let mut reading=Reading{temperature:number(current,"temperature_2m",-100.0,70.0)?,code,
        wind:number(current,"wind_speed_10m",0.0,500.0)?,humidity:number(current,"relative_humidity_2m",0.0,100.0)?,
        cloud:(number(current,"cloud_cover",0.0,100.0).unwrap_or(0.0)/100.0) as f32,
        rain:((number(current,"precipitation",0.0,1000.0).unwrap_or(0.0)+number(current,"rain",0.0,1000.0).unwrap_or(0.0)+number(current,"showers",0.0,1000.0).unwrap_or(0.0))/4.0).clamp(0.0,1.0) as f32,
        snow:(number(current,"snowfall",0.0,1000.0).unwrap_or(0.0)/3.0).clamp(0.0,1.0) as f32,days:Vec::new()};
    let d=&value["daily"];
    for i in 0..5{
        let (Some(date),Some(code),Some(high),Some(low))=(d["time"][i].as_str(),d["weather_code"][i].as_i64(),d["temperature_2m_max"][i].as_f64(),d["temperature_2m_min"][i].as_f64())else{break;};
        // Keep the forecast location's ISO date, independent of the PC's date.
        if date.len()!=10||!date.bytes().enumerate().all(|(i,c)|if i==4||i==7{c==b'-'}else{c.is_ascii_digit()})
            ||!(0..=99).contains(&code)||!high.is_finite()||!low.is_finite()||!(-100.0..=70.0).contains(&high)||!(-100.0..=70.0).contains(&low)||low>high{break;}
        reading.days.push(Day{date:date.into(),code:code as i32,high,low});
    }
    Some(reading)
}
fn fetch(location:&Location)->Option<Reading>{
    if cfg!(test){return None;}
    let client=reqwest::blocking::Client::builder().user_agent("TLauncher/1.0.3").https_only(true)
        .connect_timeout(Duration::from_secs(4)).timeout(Duration::from_secs(15)).redirect(reqwest::redirect::Policy::none()).build().ok()?;
    let url=format!("https://api.open-meteo.com/v1/forecast?latitude={:.5}&longitude={:.5}&timezone=auto&current=temperature_2m,relative_humidity_2m,precipitation,rain,showers,snowfall,weather_code,cloud_cover,wind_speed_10m&daily=weather_code,temperature_2m_max,temperature_2m_min&forecast_days=5",location.latitude,location.longitude);
    let response=client.get(url).send().ok()?;if response.status()!=reqwest::StatusCode::OK{return None;}
    let mut bytes=Vec::new();response.take(256*1024+1).read_to_end(&mut bytes).ok()?;if bytes.len()>256*1024{return None;}
    parse(&serde_json::from_slice::<Value>(&bytes).ok()?)
}
#[derive(Clone)]pub(crate) struct Snapshot{pub location:Option<Location>,pub generation:u64,pub pending:bool,pub failed:bool,pub reading:Option<Arc<Reading>>}
#[derive(Default)]struct Store{
    location:Option<Location>,generation:u64,active:Option<u64>,requested:bool,failed:bool,
    updated:Option<Instant>,attempted:Option<Instant>,reading:Option<Arc<Reading>>,subscribers:Vec<(usize,usize)>,
}
impl Store{
    fn configure(&mut self,location:Option<Location>)->bool{
        if self.location==location{return false;}
        self.location=location;self.generation=self.generation.wrapping_add(1);self.reading=None;
        self.failed=false;self.updated=None;self.attempted=None;self.requested=false;true
    }
    fn publish(&mut self,generation:u64,reading:Option<Reading>){
        if self.active==Some(generation){self.active=None;}
        if self.generation!=generation{return;}
        self.failed=reading.is_none();if let Some(reading)=reading{self.reading=Some(Arc::new(reading));self.updated=Some(Instant::now());}
    }
    fn job(&mut self,force:bool)->Option<Job>{
        let location=self.location.clone()?;
        if let Some(active)=self.active{if active!=self.generation{self.requested=true;}return None;}
        if !force&&(self.updated.is_some_and(|t|t.elapsed()<Duration::from_secs(600))||self.attempted.is_some_and(|t|t.elapsed()<Duration::from_secs(30))){return None;}
        self.active=Some(self.generation);self.attempted=Some(Instant::now());self.requested=false;
        Some(Job{location,generation:self.generation})
    }
}
fn store()->&'static Mutex<Store>{static STORE:OnceLock<Mutex<Store>>=OnceLock::new();STORE.get_or_init(||Mutex::new(Store::default()))}
fn lock()->std::sync::MutexGuard<'static,Store>{store().lock().unwrap_or_else(|e|e.into_inner())}
fn notify(subscribers:Vec<(usize,usize)>){for (hwnd,ticket) in subscribers{unsafe{PostMessageW(hwnd as HWND,READY,ticket,0);}}}
pub(crate) fn configure(location:Option<Location>){let subscribers={let mut s=lock();s.configure(location).then(||s.subscribers.clone())};if let Some(s)=subscribers{notify(s);}}
pub(crate) fn snapshot()->Snapshot{let s=lock();Snapshot{location:s.location.clone(),generation:s.generation,pending:s.active==Some(s.generation)||s.requested,failed:s.failed,reading:s.reading.clone()}}
pub(crate) fn current()->Option<Arc<Reading>>{lock().reading.clone()}
struct Job{location:Location,generation:u64}
unsafe fn submit(job:Job){
    let generation=job.generation;let raw=Box::into_raw(Box::new(job));
    if TrySubmitThreadpoolCallback(Some(worker),raw.cast(),ptr::null())==0{
        drop(Box::from_raw(raw));let (next,subscribers)={let mut s=lock();s.publish(generation,None);let next=if s.requested{s.job(true)}else{None};(next,s.subscribers.clone())};
        notify(subscribers);if let Some(next)=next{submit(next);}
    }
}
unsafe extern "system" fn worker(_:PTP_CALLBACK_INSTANCE,raw:*mut c_void){
    let job=Box::from_raw(raw.cast::<Job>());let reading=fetch(&job.location);
    let (next,subscribers)={let mut s=lock();s.publish(job.generation,reading);let next=if s.requested{s.job(true)}else{None};(next,s.subscribers.clone())};
    notify(subscribers);if let Some(next)=next{submit(next);}
}
pub(crate) fn request(force:bool){let job=lock().job(force);if let Some(job)=job{unsafe{submit(job);}}}
pub(crate) fn subscribe(hwnd:HWND,ticket:usize){let mut s=lock();s.subscribers.retain(|(h,_)|*h!=hwnd as usize);if s.subscribers.len()<4{s.subscribers.push((hwnd as usize,ticket));}}
pub(crate) fn unsubscribe(hwnd:HWND){lock().subscribers.retain(|(h,_)|*h!=hwnd as usize);}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn geocoder_bounds_metadata_and_rejects_invalid_coordinates(){
        let valid=serde_json::json!({"name":"Hồ Chí Minh","admin1":"Region","country":"Việt Nam","latitude":10.8,"longitude":106.7,"timezone":"Asia/Ho_Chi_Minh","unrelated":"discard"});
        let mut rows=vec![valid.clone();8];rows[1]["latitude"]=Value::from(91);rows[2]["name"]=Value::from("\n");
        let result=geocode_rows(&serde_json::json!({"results":rows}));assert_eq!(result.len(),4);
        assert_eq!(result[0]["name"],"Hồ Chí Minh");assert!(result[0].get("unrelated").is_none());
        assert_eq!(result[0]["timezone"],"Asia/Ho_Chi_Minh");
        assert!(geocode_rows(&serde_json::json!({"results":[]})).is_empty());
        assert!(geocode(&"x".repeat(61),"en").is_err());assert!(geocode("x\ny","en").is_err());
        assert!(geocode("x","vi").unwrap().is_empty());
    }
    fn sample()->Reading{parse(&serde_json::json!({"current":{"temperature_2m":25,"weather_code":63,"wind_speed_10m":14,"relative_humidity_2m":80,"cloud_cover":90,"precipitation":1,"rain":2,"showers":0,"snowfall":0},"daily":{"time":["2026-11-27"],"weather_code":[61],"temperature_2m_max":[29],"temperature_2m_min":[20]}})).unwrap()}
    #[test]fn shared_owner_never_publishes_old_city_or_creates_parallel_jobs(){
        let a=Location{name:"First".into(),latitude:10.0,longitude:106.0};let b=Location{name:"Second".into(),latitude:20.0,longitude:105.0};
        let mut s=Store::default();s.configure(Some(a));let old=s.job(false).unwrap();assert!(s.job(true).is_none());
        s.configure(Some(b));assert!(s.job(true).is_none());assert!(s.requested);s.publish(old.generation,Some(sample()));assert!(s.reading.is_none());
        let new=s.job(true).unwrap();assert_ne!(new.generation,old.generation);s.publish(new.generation,Some(sample()));assert!(s.job(false).is_none());assert!(s.reading.is_some());
        s.configure(None);assert!(s.reading.is_none());assert!(s.job(true).is_none());
    }
    #[test]fn original_forecast_fields_keep_location_date_and_wallpaper_values(){
        let r=sample();assert_eq!(r.days[0].date,"2026-11-27");assert_eq!(r.days[0].low,20.0);assert_eq!(r.cloud,0.9);assert_eq!(r.rain,0.75);
        assert!(Location::read(&serde_json::json!({"latitude":91,"longitude":106})).is_none());
        assert!(parse(&serde_json::json!({"current":{"temperature_2m":25}})).is_none());
        let mut value=serde_json::json!({"current":{"temperature_2m":25,"weather_code":0,"wind_speed_10m":10,"relative_humidity_2m":40},"daily":{
            "time":["2026-11-27","2026-11-28","2026-11-29","2026-11-30","2026-12-01","2026-12-02"],
            "weather_code":[0,1,2,3,61,63],"temperature_2m_max":[26,27,28,29,30,31],"temperature_2m_min":[20,20,20,20,20,20]}});
        assert_eq!(parse(&value).unwrap().days.len(),5);
        value["daily"]["temperature_2m_min"][2]=Value::from(80);assert_eq!(parse(&value).unwrap().days.len(),2);
    }
    #[cfg(test)]pub(crate) fn synthetic()->Reading{sample()}
}
#[cfg(test)]pub(crate) fn fixture_reading(){let mut s=lock();s.configure(Some(Location{name:"Synthetic city".into(),latitude:10.0,longitude:106.0}));s.reading=Some(Arc::new(tests::synthetic()));s.updated=Some(Instant::now());}
#[cfg(test)]pub(crate) fn fixture_subscribers()->usize{lock().subscribers.len()}
