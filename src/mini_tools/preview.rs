//! Quick Look: selected shell item, bounded image/text loading and one PDF page.
use super::screen::{self,Area};
use crate::desktop::wide;
use image::RgbaImage;
use std::cell::RefCell;
use std::io::Read;
use std::path::PathBuf;
use windows::core::{Interface,HSTRING};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Shell::*;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Controls::Dialogs::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

unsafe fn selected_file()->Option<PathBuf>{
    let foreground=GetForegroundWindow();let windows:IShellWindows=CoCreateInstance(&ShellWindows,None,CLSCTX_ALL).ok()?;
    for i in 0..windows.Count().ok()?{
        let Ok(dispatch)=windows.Item(&VARIANT::from(i))else{continue;};
        let Ok(service)=dispatch.cast::<IServiceProvider>()else{continue;};
        let Ok(browser)=service.QueryService::<IShellBrowser>(&SID_STopLevelBrowser)else{continue;};
        let Ok(hwnd)=browser.GetWindow()else{continue;};if hwnd.0!=foreground{continue;}
        if let Some(path)=browser_path(&browser){return Some(path);}
    }
    // Explorer's desktop is not included in the ordinary shell-window list.
    let mut class=[0u16;128];let n=GetClassNameW(foreground,class.as_mut_ptr(),128);let class=String::from_utf16_lossy(&class[..n.max(0)as usize]);
    if matches!(class.as_str(),"Progman"|"WorkerW"){
        let mut hwnd=0;let empty=VARIANT::default();if let Ok(d)=windows.FindWindowSW(&empty,&empty,SWC_DESKTOP,&mut hwnd,SWFO_NEEDDISPATCH){if let Ok(service)=d.cast::<IServiceProvider>(){if let Ok(browser)=service.QueryService::<IShellBrowser>(&SID_STopLevelBrowser){return browser_path(&browser);}}}
    }
    None
}
unsafe fn browser_path(browser:&IShellBrowser)->Option<PathBuf>{let view=browser.QueryActiveShellView().ok()?;let data:IDataObject=view.GetItemObject(SVGIO_SELECTION).ok()?;let items:IShellItemArray=SHCreateShellItemArrayFromDataObject(&data).ok()?;if items.GetCount().ok()?==0{return None;}let item=items.GetItemAt(0).ok()?;let name=item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;let path=PathBuf::from(name.to_string().ok()?);CoTaskMemFree(Some(name.0 as *const _));Some(path)}
fn choose_file()->Option<PathBuf>{unsafe{let mut path=[0u16;32768];let title=wide("Deskland Quick Look — Chọn tệp xem trước");let mut o:OPENFILENAMEW=std::mem::zeroed();o.lStructSize=std::mem::size_of::<OPENFILENAMEW>()as u32;o.lpstrFile=path.as_mut_ptr();o.nMaxFile=path.len()as u32;o.lpstrTitle=title.as_ptr();o.Flags=OFN_FILEMUSTEXIST|OFN_PATHMUSTEXIST|OFN_NOCHANGEDIR;if GetOpenFileNameW(&mut o)==0{None}else{Some(PathBuf::from(String::from_utf16_lossy(&path[..path.iter().position(|&c|c==0).unwrap_or(0)])))}}}
enum Content {Image(RgbaImage),Text(String),Pdf{document:windows::Data::Pdf::PdfDocument,page:u32,count:u32,image:RgbaImage},Info(String)}
fn pdf_page(document:&windows::Data::Pdf::PdfDocument,index:u32)->Result<RgbaImage,String>{use windows::Storage::Streams::*;let page=document.GetPage(index).map_err(|e|e.to_string())?;let stream=InMemoryRandomAccessStream::new().map_err(|e|e.to_string())?;let options=windows::Data::Pdf::PdfPageRenderOptions::new().map_err(|e|e.to_string())?;let size=page.Size().map_err(|e|e.to_string())?;let scale=(1600.0/size.Width.max(size.Height).max(1.0)).min(2.0);options.SetDestinationWidth((size.Width*scale).max(1.0)as u32).map_err(|e|e.to_string())?;options.SetDestinationHeight((size.Height*scale).max(1.0)as u32).map_err(|e|e.to_string())?;page.RenderWithOptionsToStreamAsync(&stream,&options).map_err(|e|e.to_string())?.join().map_err(|e|e.to_string())?;let n=stream.Size().map_err(|e|e.to_string())?;let reader=DataReader::CreateDataReader(&stream.GetInputStreamAt(0).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;reader.LoadAsync(n as u32).map_err(|e|e.to_string())?.join().map_err(|e|e.to_string())?;let mut data=vec![0;n as usize];reader.ReadBytes(&mut data).map_err(|e|e.to_string())?;page.Close().map_err(|e|e.to_string())?;image::load_from_memory(&data).map(|i|i.into_rgba8()).map_err(|e|e.to_string())}
fn load(path:&std::path::Path)->Result<Content,String>{
    let ext=path.extension().and_then(|x|x.to_str()).unwrap_or("").to_lowercase();
    if path.is_dir(){let entries=std::fs::read_dir(path).map_err(|e|e.to_string())?.flatten().take(101).collect::<Vec<_>>();let mut text=entries.iter().take(100).map(|e|format!("{}{}",if e.file_type().is_ok_and(|t|t.is_dir()){"📁 "}else{""},e.file_name().to_string_lossy())).collect::<Vec<_>>().join("\n");if entries.len()>100{text.push_str("\n…");}return Ok(Content::Text(text));}
    if ["png","jpg","jpeg","bmp","gif","webp"].contains(&ext.as_str()){
        let mut reader=image::ImageReader::open(path).map_err(|e|e.to_string())?.with_guessed_format().map_err(|e|e.to_string())?;let mut limits=image::Limits::default();limits.max_alloc=Some(256*1024*1024);limits.max_image_width=Some(20000);limits.max_image_height=Some(20000);reader.limits(limits);let image=reader.decode().map_err(|e|format!("Không xem được ảnh: {e}"))?.into_rgba8();return Ok(Content::Image(image));
    }
    if ext=="pdf"{let file=windows::Storage::StorageFile::GetFileFromPathAsync(&HSTRING::from(path.to_string_lossy().as_ref())).map_err(|e|e.to_string())?.join().map_err(|e|e.to_string())?;let document=windows::Data::Pdf::PdfDocument::LoadFromFileAsync(&file).map_err(|e|e.to_string())?.join().map_err(|e|format!("Không xem được PDF (có thể cần mật khẩu): {e}"))?;let count=document.PageCount().map_err(|e|e.to_string())?;let image=pdf_page(&document,0)?;return Ok(Content::Pdf{document,page:0,count,image});}
    if ["txt","md","log","json","csv","rs","toml","ini","xml","html","css","js","yaml","yml","srt","bat","ps1"].contains(&ext.as_str()){
        let mut bytes=Vec::new();std::fs::File::open(path).map_err(|e|e.to_string())?.take(1024*1024).read_to_end(&mut bytes).map_err(|e|e.to_string())?;let mut text=if bytes.starts_with(&[0xff,0xfe]){let words=bytes[2..].chunks_exact(2).map(|p|u16::from_le_bytes([p[0],p[1]])).collect::<Vec<_>>();String::from_utf16_lossy(&words)}else{String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_string()};if text.chars().count()>100000{text=text.chars().take(100000).collect();text.push_str("\n… Bản xem trước đã được giới hạn. Nhấn Enter để mở đầy đủ.");}return Ok(Content::Text(text));
    }
    Ok(Content::Info("Định dạng này chưa có trình xem trước.\nNhấn Enter để mở bằng ứng dụng mặc định.".into()))
}
struct State{path:PathBuf,content:Content,scroll:i32,zoom:f32}
thread_local!{static VIEW:RefCell<Option<State>>=const{RefCell::new(None)};}
pub fn run(path:Option<PathBuf>)->Result<(),String>{
    let Some(path)=path.or_else(||unsafe{selected_file()}).or_else(choose_file)else{return Ok(());};let content=load(&path)?;
    VIEW.with(|c|*c.borrow_mut()=Some(State{path:path.clone(),content,scroll:0,zoom:1.0}));unsafe{let cls=wide("DesklandQuickLook");let mut wc:WNDCLASSW=std::mem::zeroed();wc.lpfnWndProc=Some(proc);wc.lpszClassName=cls.as_ptr();wc.hCursor=LoadCursorW(std::ptr::null_mut(),IDC_ARROW);RegisterClassW(&wc);let mut mouse=POINT::default();GetCursorPos(&mut mouse);let m=screen::monitor_at(mouse.x,mouse.y);let w=1000.min(m.w-50).max(450);let h=800.min(m.h-100).max(350);let title=format!("{} — Deskland Quick Look",path.file_name().unwrap_or_default().to_string_lossy());let hwnd=CreateWindowExW(0,cls.as_ptr(),wide(&title).as_ptr(),WS_OVERLAPPEDWINDOW,m.x+(m.w-w)/2,m.y+(m.h-h)/2,w,h,std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null());if hwnd.is_null(){VIEW.with(|c|c.borrow_mut().take());return Err("Không mở được Quick Look".into());}ShowWindow(hwnd,SW_SHOW);SetForegroundWindow(hwnd);super::loop_messages();}VIEW.with(|c|c.borrow_mut().take());Ok(())
}
unsafe fn page(hwnd:HWND,delta:i32){let error=VIEW.with(|c|{let mut g=c.borrow_mut();let s=g.as_mut().unwrap();if let Content::Pdf{document,page,count,image}=&mut s.content{let next=(*page as i32+delta).clamp(0,*count as i32-1)as u32;if next!=*page{match pdf_page(document,next){Ok(i)=>{*image=i;*page=next;s.scroll=0;s.zoom=1.0;},Err(e)=>return Some(e)}}}None});if let Some(error)=error{super::message(&error);}InvalidateRect(hwnd,std::ptr::null(),0);}
unsafe extern "system" fn proc(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{match msg{
    WM_ERASEBKGND=>1,WM_SIZE=>{InvalidateRect(hwnd,std::ptr::null(),0);0},
    WM_PAINT=>{let mut ps=std::mem::zeroed();let target=BeginPaint(hwnd,&mut ps);let mut r=RECT::default();GetClientRect(hwnd,&mut r);let dc=CreateCompatibleDC(target);let bitmap=CreateCompatibleBitmap(target,r.right.max(1),r.bottom.max(1));let old=SelectObject(dc,bitmap);let b=CreateSolidBrush(0x00221D19);FillRect(dc,&r,b);DeleteObject(b);let font=super::editor::make_font(16,400);let oldfont=SelectObject(dc,font);SetBkMode(dc,TRANSPARENT as i32);SetTextColor(dc,0x00F3EEE9);
        VIEW.with(|c|{let g=c.borrow();let s=g.as_ref().unwrap();let top=12;let height=(r.bottom-65).max(1);match &s.content{
            Content::Image(image)|Content::Pdf{image,..}=>{let k=((r.right-24).max(1)as f32/image.width()as f32).min(height as f32/image.height()as f32).min(1.0)*s.zoom;let(w,h)=((image.width()as f32*k)as i32,(image.height()as f32*k)as i32);let saved=SaveDC(dc);IntersectClipRect(dc,0,0,r.right,height);screen::draw_image(dc,image,Area{x:(r.right-w)/2,y:top+(height-h)/2-s.scroll,w,h});RestoreDC(dc,saved);},
            Content::Text(text)|Content::Info(text)=>{let saved=SaveDC(dc);IntersectClipRect(dc,12,8,r.right-12,height);let mut text_rect=RECT{left:18,top:12-s.scroll,right:r.right-18,bottom:200000-s.scroll};DrawTextW(dc,wide(text).as_ptr(),-1,&mut text_rect,DT_LEFT|DT_WORDBREAK|DT_NOPREFIX);RestoreDC(dc,saved);},
        }let metadata=std::fs::metadata(&s.path).ok();let info=format!("{}  ·  {} KB  {}",s.path.file_name().unwrap_or_default().to_string_lossy(),metadata.map(|m|m.len().div_ceil(1024)).unwrap_or(0),if let Content::Pdf{page,count,..}=&s.content{format!("· Trang {}/{}  ← →",page+1,count)}else{String::new()});let mut footer=RECT{left:16,top:r.bottom-53,right:r.right-16,bottom:r.bottom-29};DrawTextW(dc,wide(&info).as_ptr(),-1,&mut footer,DT_LEFT|DT_SINGLELINE|DT_END_ELLIPSIS);footer.top+=25;footer.bottom+=25;SetTextColor(dc,0x00C9BDB2);DrawTextW(dc,wide("Space / Esc: đóng   ·   Enter: mở tệp   ·   Cuộn: di chuyển   ·   Ctrl + cuộn: phóng to").as_ptr(),-1,&mut footer,DT_LEFT|DT_SINGLELINE|DT_END_ELLIPSIS);});
        BitBlt(target,0,0,r.right,r.bottom,dc,0,0,SRCCOPY);SelectObject(dc,oldfont);DeleteObject(font);SelectObject(dc,old);DeleteObject(bitmap);DeleteDC(dc);EndPaint(hwnd,&ps);0},
    WM_MOUSEWHEEL=>{let delta=(w>>16)as i16;VIEW.with(|c|if let Some(s)=c.borrow_mut().as_mut(){if GetKeyState(VK_CONTROL as i32)<0{s.zoom=(s.zoom*if delta>0{1.2}else{1.0/1.2}).clamp(0.25,8.0);}else{s.scroll=(s.scroll-if delta>0{75}else{-75}).clamp(0,150000);}});InvalidateRect(hwnd,std::ptr::null(),0);0},
    WM_KEYDOWN=>{match w as u16{VK_ESCAPE|VK_SPACE=>{DestroyWindow(hwnd);},VK_LEFT|VK_PRIOR=>page(hwnd,-1),VK_RIGHT|VK_NEXT=>page(hwnd,1),VK_RETURN=>{let path=VIEW.with(|c|c.borrow().as_ref().unwrap().path.clone());windows_sys::Win32::UI::Shell::ShellExecuteW(hwnd,wide("open").as_ptr(),wide(&path.to_string_lossy()).as_ptr(),std::ptr::null(),std::ptr::null(),SW_SHOWNORMAL);DestroyWindow(hwnd);},_=>{}}0},
    WM_CLOSE=>{DestroyWindow(hwnd);0},WM_DESTROY=>{PostQuitMessage(0);0},_=>DefWindowProcW(hwnd,msg,w,l)
}}

#[cfg(test)]mod tests{use super::*;#[test]fn image_preview_and_utf16_text_loading_are_bounded(){let dir=std::env::temp_dir().join(format!("deskland-preview-test-{}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();let p=dir.join("image.png");RgbaImage::from_pixel(40,20,image::Rgba([12,34,56,255])).save(&p).unwrap();assert!(matches!(load(&p).unwrap(),Content::Image(i)if i.dimensions()==(40,20)));let p=dir.join("text.txt");std::fs::write(&p,[0xff,0xfe,b'A',0,b'B',0]).unwrap();assert!(matches!(load(&p).unwrap(),Content::Text(t)if t=="AB"));std::fs::remove_dir_all(dir).unwrap();}}
