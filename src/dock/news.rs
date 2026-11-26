//! Original widget.h RSS parser on an on-demand, bounded HTTPS request.
use super::{STATE,control_center::{self,Card,Command,View}};
use crate::desktop::wide;
use std::{cell::RefCell,ffi::c_void,io::Read,ptr,sync::{Arc,Mutex,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
use windows_sys::Win32::{Foundation::*,System::Threading::{TrySubmitThreadpoolCallback,PTP_CALLBACK_INSTANCE},UI::{Shell::ShellExecuteW,WindowsAndMessaging::*}};
pub(super) const READY:u32=WM_APP+160;
#[derive(Clone)]pub(super) enum Action{Open,Refresh(u64),Sources(u64),Source(u64,u8),Article(u64,usize),Back(u64)}
#[derive(Clone,Debug,PartialEq,Eq)]struct Item{title:String,url:String,time:String}
#[derive(Clone,Default)]struct Snapshot{items:Vec<Item>,failed:bool}
#[derive(Default)]struct Shared{closed:AtomicBool,answer:Mutex<Option<Snapshot>>}
struct Panel{owner:usize,popup:usize,ticket:usize,generation:u64,source:u8,sources:bool,pending:bool,view:Snapshot,work:Arc<Shared>}
impl Drop for Panel{fn drop(&mut self){self.work.closed.store(true,Ordering::Release);}}
thread_local!{
    static PANEL:RefCell<Option<Panel>>=const{RefCell::new(None)};
    static CACHE:RefCell<Option<(u8,Instant,Snapshot)>>=const{RefCell::new(None)};
}
static WORKER:AtomicBool=AtomicBool::new(false);
const FEEDS:[(&str,&str);4]=[
    ("ANSA","https://www.ansa.it/sito/ansait_rss.xml"),
    ("BBC News","https://feeds.bbci.co.uk/news/world/rss.xml"),
    ("The Guardian","https://www.theguardian.com/world/rss"),
    ("Al Jazeera","https://www.aljazeera.com/xml/rss/all.xml"),
];
fn text(vi:&str,en:&str,zh:&str)->String{crate::launcher::localized(vi,en,zh).into()}
fn title()->String{text("Tin tức","News","新闻")}
fn source_name(source:u8)->String{match source{0=>text("Theo ngôn ngữ · BBC News","Automatic · BBC News","自动 · BBC News"),1..=4=>FEEDS[source as usize-1].0.into(),_=>text("Tắt tin tức","News off","关闭新闻")}}
fn enabled()->bool{STATE.with(|s|{let s=s.borrow();s.prefs.enabled&&!s.suspended})}
fn effective(source:u8)->Option<usize>{match source{0=>Some(1),1..=4=>Some(source as usize-1),_=>None}}
fn valid_url(value:&str)->bool{
    !value.is_empty()&&value.len()<=2048&&!value.chars().any(char::is_control)
        &&reqwest::Url::parse(value).is_ok_and(|u|u.scheme()=="https"&&u.host_str().is_some()&&u.username().is_empty()&&u.password().is_none())
}
#[cfg(target_env="msvc")]unsafe extern "C"{
    fn tlauncher_news_parse(bytes:*const u8,size:u32)->*mut c_void;fn tlauncher_news_free(raw:*mut c_void);
    fn tlauncher_news_count(raw:*mut c_void)->u32;
    fn tlauncher_news_item(raw:*mut c_void,index:u32,title:*mut u16,link:*mut u16,time:*mut u16,size:u32)->bool;
}
fn parse(xml:&[u8])->Snapshot{
    let mut result=Snapshot::default();
    if xml.len()>1024*1024{return Snapshot{failed:true,..result};}
    #[cfg(target_env="msvc")]unsafe{
        struct Native(*mut c_void);impl Drop for Native{fn drop(&mut self){unsafe{tlauncher_news_free(self.0);}}}
        let native=Native(tlauncher_news_parse(xml.as_ptr(),xml.len() as u32));
        if native.0.is_null(){return Snapshot{failed:true,..result};}
        for i in 0..tlauncher_news_count(native.0).min(8){
            let (mut title,mut link,mut time)=([0u16;2049],[0u16;2049],[0u16;2049]);
            if !tlauncher_news_item(native.0,i,title.as_mut_ptr(),link.as_mut_ptr(),time.as_mut_ptr(),2049){continue;}
            let read=|v:&[u16]|String::from_utf16_lossy(&v[..v.iter().position(|&v|v==0).unwrap_or(v.len())]);
            let item=Item{title:read(&title),url:read(&link),time:read(&time)};
            if !item.title.is_empty()&&valid_url(&item.url)&&!result.items.iter().any(|old|old.url==item.url){result.items.push(item);}
        }
    }
    result.failed=result.items.is_empty();result
}
fn fetch(source:u8)->Snapshot{
    let Some(index)=effective(source)else{return Snapshot::default();};
    // Native fixtures are network-free and never open a browser.
    if cfg!(test){return Snapshot{failed:true,..Default::default()};}
    let read=||->Option<Snapshot>{
        let url=reqwest::Url::parse(FEEDS[index].1).ok()?;let host=url.host_str()?.to_owned();
        let client=reqwest::blocking::Client::builder().user_agent("TLauncher/1.0.3")
            .connect_timeout(Duration::from_secs(4)).timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::custom(move|attempt|{
                if attempt.previous().len()<3&&attempt.url().scheme()=="https"&&attempt.url().host_str()==Some(host.as_str()){attempt.follow()}else{attempt.stop()}
            })).build().ok()?;
        let response=client.get(url).send().ok()?.error_for_status().ok()?;
        if response.status()!=reqwest::StatusCode::OK{return None;}
        let mut bytes=Vec::new();response.take(1024*1024+1).read_to_end(&mut bytes).ok()?;
        Some(parse(&bytes))
    };
    read().unwrap_or_else(||Snapshot{failed:true,..Default::default()})
}
struct Job{popup:usize,ticket:usize,source:u8,work:Arc<Shared>}
unsafe extern "system" fn worker(_:PTP_CALLBACK_INSTANCE,raw:*mut c_void){
    let job=Box::from_raw(raw.cast::<Job>());
    struct Permit;impl Drop for Permit{fn drop(&mut self){WORKER.store(false,Ordering::Release);}}let _permit=Permit;
    if job.work.closed.load(Ordering::Acquire){return;}
    let snapshot=fetch(job.source);
    if !job.work.closed.load(Ordering::Acquire){
        if let Ok(mut answer)=job.work.answer.lock(){*answer=Some(snapshot);}
        PostMessageW(job.popup as HWND,READY,job.ticket,0);
    }
}
impl Panel{
    unsafe fn submit(&mut self){
        if self.pending||self.source==5{return;}
        if WORKER.swap(true,Ordering::AcqRel){self.view.failed=true;return;}
        self.pending=true;
        let raw=Box::into_raw(Box::new(Job{popup:self.popup,ticket:self.ticket,source:self.source,work:self.work.clone()}));
        if TrySubmitThreadpoolCallback(Some(worker),raw.cast(),ptr::null())==0{
            drop(Box::from_raw(raw));WORKER.store(false,Ordering::Release);self.pending=false;self.view.failed=true;
        }
    }
    fn view(&self)->View{
        let action=|a|(!self.pending).then_some(Command::News(a));let mut cards=Vec::new();
        if self.sources{
            for source in 0..=5{cards.push(Card{title:source_name(source),hint:if source==self.source{text("Đang chọn","Selected","已选择")}else{String::new()},icon:None,action:action(Action::Source(self.generation,source))});}
            cards.push(Card{title:text("Trở lại","Back","返回"),hint:String::new(),icon:None,action:action(Action::Back(self.generation))});
        }else{
            cards.push(Card{title:source_name(self.source),hint:text("Chọn nguồn tin","Choose news source","选择新闻来源"),icon:None,action:action(Action::Sources(self.generation))});
            for (i,item) in self.view.items.iter().enumerate(){cards.push(Card{title:item.title.clone(),hint:item.time.clone(),icon:None,action:action(Action::Article(self.generation,i))});}
            if self.source!=5{cards.push(Card{title:text("Làm mới","Refresh","刷新"),hint:String::new(),icon:None,action:action(Action::Refresh(self.generation))});}
        }
        let hint=if self.pending{text("Đang tải…","Loading…","正在加载…")}
            else if self.source==5{text("Chọn nguồn để đọc tin","Choose a source to read news","选择来源以阅读新闻")}
            else if self.view.failed{text("Chưa tải được tin mới. Nội dung cũ có thể đã thay đổi.","Could not refresh news. Previous entries may have changed.","无法刷新新闻，先前条目可能已更改。")}
            else{text("Mở bài viết bằng trình duyệt","Open articles in your browser","在浏览器中打开文章")};
        View{title:title(),hint,cards,metrics:None}
    }
}
pub(super) fn detach(popup:HWND){PANEL.with(|s|{let mut s=s.borrow_mut();if s.as_ref().is_some_and(|p|p.popup==popup as usize){s.take();}});}
pub(super) fn view_for(popup:HWND,ticket:usize)->Option<View>{PANEL.with(|s|s.borrow().as_ref().filter(|p|p.popup==popup as usize&&p.ticket==ticket).map(|p|p.view()))}
pub(super) unsafe fn attach(owner:HWND,popup:HWND,ticket:usize){
    let source=STATE.with(|s|s.borrow().prefs.news_source);
    let cached=CACHE.with(|c|c.borrow().as_ref().filter(|(s,at,_)|*s==source&&at.elapsed()<Duration::from_secs(600)).map(|(_,_,v)|v.clone()));
    let mut panel=Panel{owner:owner as usize,popup:popup as usize,ticket,generation:1,source,sources:false,pending:false,view:cached.clone().unwrap_or_default(),work:Arc::default()};
    if cached.is_none(){panel.submit();}PANEL.with(|s|s.replace(Some(panel)));
}
unsafe fn publish(owner:HWND,popup:HWND,ticket:usize,view:View){
    if super::weather_panel::combined_for(owner,popup,ticket){super::weather_panel::refresh_combined(owner,popup,ticket);}
    else{control_center::update(owner,ticket,view);}
}
pub(super) unsafe fn receive(popup:HWND,ticket:usize){
    let update=PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().filter(|p|p.popup==popup as usize&&p.ticket==ticket)?;
        let incoming=p.work.answer.lock().ok()?.take()?;p.pending=false;p.generation=p.generation.wrapping_add(1);
        if !incoming.failed{CACHE.with(|c|c.replace(Some((p.source,Instant::now(),incoming.clone()))));p.view=incoming;}else{p.view.failed=true;}
        Some((p.owner,p.ticket,p.view()))
    });
    if let Some((owner,ticket,view))=update{if enabled(){publish(owner as HWND,popup,ticket,view);}}
}
pub(super) unsafe fn perform(owner:HWND,popup:HWND,action:Action){
    if !enabled()||IsWindow(owner)==0{return;}
    if matches!(action,Action::Open){
        super::utilities::close_panels();
        control_center::show_news(owner,super::dock_app_anchor(owner,None),View{title:title(),hint:String::new(),cards:Vec::new(),metrics:None});
        let (Some(window),Some(ticket))=(control_center::window(owner),control_center::ticket(owner))else{return;};
        attach(owner,window,ticket);if let Some(view)=view_for(window,ticket){control_center::update(owner,ticket,view);}return;
    }
    let mut article=None;let mut changed_source=None;
    let update=PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().filter(|p|p.owner==owner as usize&&p.popup==popup as usize&&!p.pending)?;
        match action{
            Action::Refresh(g) if g==p.generation=>p.submit(),
            Action::Sources(g) if g==p.generation=>p.sources=true,
            Action::Back(g) if g==p.generation=>p.sources=false,
            Action::Source(g,source) if g==p.generation&&source<=5=>{
                p.source=source;p.sources=false;p.view=Snapshot::default();p.generation=p.generation.wrapping_add(1);
                CACHE.with(|c|c.take());p.submit();changed_source=Some(source);
            },
            Action::Article(g,i) if g==p.generation=>article=p.view.items.get(i).filter(|n|valid_url(&n.url)).map(|n|n.url.clone()),
            _=>return None,
        }Some((p.ticket,p.view()))
    });
    if let Some(source)=changed_source{STATE.with(|s|s.borrow_mut().prefs.news_source=source);super::emit(super::Event::Edit(super::Edit::NewsSource(source)));}
    if let Some(url)=article{if !cfg!(test){ShellExecuteW(owner,wide("open").as_ptr(),wide(&url).as_ptr(),ptr::null(),ptr::null(),SW_SHOWNORMAL);}}
    if let Some((ticket,view))=update{control_center::first_page(owner,ticket);publish(owner,popup,ticket,view);}
}
#[cfg(test)]pub(super) fn fixture_idle()->bool{!WORKER.load(Ordering::Acquire)&&PANEL.with(|s|s.borrow().as_ref().is_none_or(|p|!p.pending))}
#[cfg(test)]pub(super) unsafe fn fixture_article(popup:HWND,ticket:usize)->u64{
    let generation=PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().unwrap();assert_eq!((p.popup,p.ticket),(popup as usize,ticket));assert!(!p.pending);
        p.view=Snapshot{failed:false,items:vec![Item{title:text("Tin thử nghiệm: thời tiết hôm nay và dự báo những ngày tiếp theo trong tuần","Synthetic headline about today's weather and the forecast for the coming week","这是用于检验天气和新闻同一面板的两行标题示例内容"),url:"https://example.invalid/widgets".into(),time:"14:35".into()}]};p.generation
    });
    let owner=PANEL.with(|s|s.borrow().as_ref().unwrap().owner);publish(owner as HWND,popup,ticket,view_for(popup,ticket).unwrap());generation
}
#[cfg(test)]pub(super) fn fixture_closed_delivery(popup:HWND,ticket:usize)->impl FnOnce(){
    let work=PANEL.with(|s|s.borrow().as_ref().unwrap().work.clone());
    move||{
        assert!(work.closed.load(Ordering::Acquire));
        *work.answer.lock().unwrap()=Some(Snapshot::default());
        unsafe{receive(popup,ticket);}
        assert!(PANEL.with(|s|s.borrow().is_none()));
    }
}
pub(super) fn reset(){PANEL.with(|s|s.take());CACHE.with(|s|s.take());}

#[cfg(test)]mod tests{
    use super::*;
    #[test]
    #[ignore="native News panel/lifetime with synthetic RSS; hosted Windows only, no network or browser"]
    fn native_news_panel_rejects_stale_actions_and_closed_results(){
        use windows_sys::Win32::{System::LibraryLoader::GetModuleHandleW,Graphics::{Dwm::DwmFlush,Gdi::*}};
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(),Ok("true"));
        struct Locale(String);impl Drop for Locale{fn drop(&mut self){crate::launcher::spotlight_locale(&self.0);}}let _locale=Locale(text("vi","en","zh-CN"));
        unsafe{
            STATE.with(|s|{let mut s=s.borrow_mut();s.prefs.enabled=true;s.prefs.appearance.theme=super::super::Theme::Light;});
            let owner=CreateWindowExW(WS_EX_TOOLWINDOW,wide("STATIC").as_ptr(),wide("News fixture").as_ptr(),WS_POPUP,
                10,10,260,80,ptr::null_mut(),ptr::null_mut(),GetModuleHandleW(ptr::null()),ptr::null());assert!(!owner.is_null());
            let pump=|done:&dyn Fn()->bool|{
                let start=Instant::now();while !done(){
                    let mut msg:MSG=std::mem::zeroed();for _ in 0..64{if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&msg);DispatchMessageW(&msg);}
                    assert!(start.elapsed()<Duration::from_secs(5));std::thread::sleep(Duration::from_millis(10));
                }
            };
            for cycle in 0..3{
                let locale=["vi","en","zh-CN"][cycle];crate::launcher::spotlight_locale(locale);
                perform(owner,ptr::null_mut(),Action::Open);let popup=control_center::window(owner).unwrap();
                pump(&||!WORKER.load(Ordering::Acquire)&&PANEL.with(|s|s.borrow().as_ref().is_some_and(|p|!p.pending)));
                let (generation,ticket,work,view)=PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut().unwrap();
                    p.view=parse("<rss><channel><item><title>TLauncher · Tin tức thử nghiệm</title><link>https://example.invalid/story</link><pubDate>Thu, 19 Nov 2026 04:42:52 GMT</pubDate></item></channel></rss>".as_bytes());
                    p.view.items[0].title=text("Tin thử nghiệm dài: thời tiết hôm nay và dự báo những ngày tiếp theo trong tuần","Synthetic news headline about the weather today and the forecast for the coming week","这是用于检验两行新闻标题的示例内容，包含本周天气与未来几天的预报信息");p.view.items[0].time="14:35".into();
                    (p.generation,p.ticket,p.work.clone(),p.view())
                });control_center::update(owner,ticket,view);
                perform(owner,popup,Action::Sources(generation.wrapping_sub(1)));assert!(!PANEL.with(|s|s.borrow().as_ref().unwrap().sources));
                perform(owner,popup,Action::Article(generation,0)); // TEST never opens the URL.
                perform(owner,popup,Action::Sources(generation));assert!(PANEL.with(|s|s.borrow().as_ref().unwrap().sources));
                perform(owner,popup,Action::Back(generation));
                {
                    let until=Instant::now()+Duration::from_millis(450);pump(&||Instant::now()>=until);
                    UpdateWindow(popup);DwmFlush();let mut bounds:RECT=std::mem::zeroed();GetWindowRect(popup,&mut bounds);
                    let (w,h)=(bounds.right-bounds.left,bounds.bottom-bounds.top);let mut layer=super::super::glass_probe::Layer::new(w,h).unwrap();
                    let screen=GetDC(ptr::null_mut());assert_ne!(BitBlt(layer.dc(),0,0,w,h,screen,bounds.left,bounds.top,SRCCOPY|CAPTUREBLT),0);ReleaseDC(ptr::null_mut(),screen);GdiFlush();
                    let rgba=layer.pixels().chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],255]).collect::<Vec<_>>();
                    if let Some(root)=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE"){image::save_buffer(std::path::PathBuf::from(root).join(format!("news-synthetic-{locale}.png")),&rgba,w as u32,h as u32,image::ColorType::Rgba8).unwrap();}
                    let px=|n|super::super::panel_chrome::scale(windows_sys::Win32::UI::HiDpi::GetDpiForWindow(popup),n);
                    // Article is row 1, after the source selector. Both actual
                    // line bands need dark ink on the fixture's light glass.
                    let top=px(67+59);for (a,b) in [(7,22),(24,39)]{let count=rgba.chunks_exact(4).enumerate().filter(|(i,p)|{let x=*i as i32%w;let y=*i as i32/w;x>=px(22)&&x<w-px(66)&&y>=top+px(a)&&y<top+px(b)&&p[..3].iter().all(|c|*c<100)}).count();assert!(count>15,"actual News line {a} missing for {locale}: {count}");}
                    #[cfg(target_env="msvc")]{let access=super::super::control_access::get(popup);assert!(!access.is_null());assert_eq!(super::super::control_access::probe(access,control_center::fixture_access_count(owner).unwrap(),0,0),0);super::super::control_access::release(access);}
                }
                perform(owner,popup,Action::Source(generation,5));assert_eq!(STATE.with(|s|s.borrow().prefs.news_source),5);assert!(!PANEL.with(|s|s.borrow().as_ref().unwrap().pending));
                perform(owner,popup,Action::Source(generation,1));assert_eq!(STATE.with(|s|s.borrow().prefs.news_source),5,"old source generation must be refused");
                control_center::close_for(owner);assert!(work.closed.load(Ordering::Acquire));assert_eq!(IsWindow(popup),0);
                receive(popup,ticket);assert!(PANEL.with(|s|s.borrow().is_none()));STATE.with(|s|s.borrow_mut().prefs.news_source=0);
            }
            perform(owner,ptr::null_mut(),Action::Open);control_center::close_for(owner);pump(&||!WORKER.load(Ordering::Acquire));assert!(PANEL.with(|s|s.borrow().is_none()));
            reset();assert!(CACHE.with(|s|s.borrow().is_none()));DestroyWindow(owner);
        }
        println!("FLUIDWIN_NEWS original_decoder=PASS synthetic_panel=PASS stale_source=REFUSED late_result=IGNORED teardown=PASS actual_network=NOT_INVOKED browser=NOT_INVOKED");
    }
    #[test]fn original_rss_decoder_bounds_unicode_entities_links_and_local_times(){
        let xml="<rss><channel><item><title><![CDATA[Tin &amp; &#x1F30F;]]></title><link>https://example.invalid/article?a=1&amp;b=2</link><pubDate>Thu, 19 Nov 2026 04:42:52 GMT</pubDate></item><item><title>Rejected</title><link>file:///C:/private.txt</link></item><item><title>Credentials</title><link>https://u:p@example.invalid/</link></item></channel></rss>";
        let read=parse(xml.as_bytes());assert!(!read.failed);assert_eq!(read.items.len(),1);assert_eq!(read.items[0].title,"Tin & 🌏");assert_eq!(read.items[0].url,"https://example.invalid/article?a=1&b=2");
        assert_eq!(read.items[0].time.len(),5);assert_eq!(&read.items[0].time[2..3],":");
        let many=format!("<rss>{}</rss>","<item><title>News</title><link>https://example.invalid/a</link></item>".repeat(200));
        assert!(parse(many.as_bytes()).items.len()<=8);assert!(parse(&vec![b'x';1024*1024+1]).failed);
        assert!(parse(b"<rss><item><title>Incomplete").failed);
        for bad in ["javascript:alert(1)","https://example.invalid/\n","https:///","file:///a"]{assert!(!valid_url(bad));}
        let mut options=super::super::Options::default();super::super::Edit::NewsSource(3).apply(&mut options);assert_eq!(super::super::Options::from_json(&options.json()).news_source,3);
    }
}
