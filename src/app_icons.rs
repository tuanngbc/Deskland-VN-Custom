//! User-selected app artwork. Deskland copies a bounded image into the active
//! profile. Launch targets and Windows shortcuts are never modified.
use std::{collections::{BTreeMap,HashMap},path::{Path,PathBuf},sync::{LazyLock,Mutex,RwLock,atomic::{AtomicU64,Ordering}}};
use crate::launcher::IconPixels;

pub type Overrides=BTreeMap<String,String>;
pub const MAX_ICONS:usize=128;
static OVERRIDES:LazyLock<RwLock<Overrides>>=LazyLock::new(||RwLock::new(Overrides::new()));
static SHORTCUTS:LazyLock<Mutex<HashMap<String,Option<String>>>>=LazyLock::new(||Mutex::new(HashMap::new()));
static REVISION:AtomicU64=AtomicU64::new(0);
static SERIAL:AtomicU64=AtomicU64::new(0);

pub fn revision()->u64{REVISION.load(Ordering::Acquire)}
pub fn set(values:&Overrides)->bool{
    let mut old=OVERRIDES.write().unwrap_or_else(|err|err.into_inner());
    if &*old==values{return false;}
    *old=values.clone();
    REVISION.fetch_add(1,Ordering::AcqRel);
    true
}
pub fn valid_file(file:&str)->bool{
    file.strip_suffix(".png").is_some_and(|s|s.len()==32&&s.bytes().all(|v|v.is_ascii_hexdigit()))
}
pub fn normalized(path:&str)->Option<String>{
    (!path.is_empty()&&path.len()<=32760&&!path.chars().any(char::is_control))
        .then(||path.replace('/',"\\").to_lowercase())
}

unsafe fn shortcut_target(path:&str)->Option<String>{
    use windows::{core::{Interface,PCWSTR},Win32::{System::Com::{CoCreateInstance,CLSCTX_INPROC_SERVER,IPersistFile,STGM_READ},
        UI::Shell::{IShellLinkW,ShellLink,SLGP_RAWPATH}}};
    let path_w=crate::desktop::wide(path);
    let link:IShellLinkW=CoCreateInstance(&ShellLink,None,CLSCTX_INPROC_SERVER).ok()?;
    link.cast::<IPersistFile>().ok()?.Load(PCWSTR(path_w.as_ptr()),STGM_READ).ok()?;
    let mut args=[0u16;2];
    link.GetArguments(&mut args).ok()?;
    if args[0]!=0{return None;}
    let mut target=[0u16;32768];
    link.GetPath(&mut target,std::ptr::null_mut(),SLGP_RAWPATH.0 as u32).ok()?;
    let len=target.iter().position(|&c|c==0)?;
    let target=String::from_utf16(&target[..len]).ok()?;
    let file=Path::new(&target);
    (file.is_absolute()&&file.extension().is_some_and(|e|e.eq_ignore_ascii_case("exe"))).then_some(target)
}
/// Use full EXE path or packaged app ID. Plain .lnk aliases with no arguments
/// resolve to the same identity; argument-specific shortcuts stay distinct.
pub fn key(path:&str)->Option<String>{
    let raw=normalized(path)?;
    if !raw.ends_with(".lnk")||raw.starts_with(r"\\"){return Some(raw);}
    if let Some(target)=SHORTCUTS.lock().unwrap_or_else(|e|e.into_inner()).get(&raw).cloned(){
        return target.or(Some(raw));
    }
    let target=unsafe{shortcut_target(path)}.and_then(|p|normalized(&p));
    let mut cache=SHORTCUTS.lock().unwrap_or_else(|e|e.into_inner());
    if cache.len()>=768{cache.clear();}
    cache.insert(raw.clone(),target.clone());
    target.or(Some(raw))
}
fn directory()->PathBuf{crate::lock::data_dir().join("dock-icons")}

fn image(path:&Path)->Result<image::RgbaImage,String>{
    let mut file=std::fs::File::open(path).map_err(|e|e.to_string())?;
    if file.metadata().map_err(|e|e.to_string())?.len()>12*1024*1024{return Err("Ảnh vượt giới hạn 12 MB.".into());}
    use std::io::Read;
    let mut bytes=Vec::new();
    (&mut file).take(12*1024*1024+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
    if bytes.len()>12*1024*1024{return Err("Ảnh vượt giới hạn 12 MB.".into());}
    let format=if path.extension().is_some_and(|e|e.eq_ignore_ascii_case("ico")){image::ImageFormat::Ico}
        else{image::guess_format(&bytes).map_err(|_|"Tệp không phải ảnh được hỗ trợ.")?};
    if !matches!(format,image::ImageFormat::Png|image::ImageFormat::Ico|image::ImageFormat::Jpeg|image::ImageFormat::Bmp|image::ImageFormat::WebP){
        return Err("Chọn ảnh PNG, ICO, JPG, BMP hoặc WebP.".into());
    }
    let mut reader=image::ImageReader::with_format(std::io::Cursor::new(bytes),format);
    let mut limits=image::Limits::default();
    limits.max_image_width=Some(4096);limits.max_image_height=Some(4096);limits.max_alloc=Some(96*1024*1024);
    reader.limits(limits);
    reader.decode().map(|v|v.into_rgba8()).map_err(|e|format!("Không đọc được ảnh: {e}"))
}
pub fn import(path:&Path)->Result<String,String>{
    let source=image(path)?;
    let (w,h)=source.dimensions();
    let scale=(256.0/w.max(h) as f32).min(1.0);
    let dim=|n:u32|(n as f32*scale).round().max(1.0) as u32;
    let source=if scale<1.0{image::imageops::resize(&source,dim(w),dim(h),image::imageops::FilterType::Lanczos3)}else{source};
    if !source.pixels().any(|p|p[3]!=0){return Err("Ảnh không có điểm ảnh nhìn thấy được.".into());}
    let dir=directory();
    std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?;
    let file=format!("{:016x}{:08x}{:08x}.png",now.as_nanos() as u64,std::process::id(),SERIAL.fetch_add(1,Ordering::Relaxed) as u32);
    source.save(dir.join(&file)).map_err(|e|e.to_string())?;
    Ok(file)
}
pub fn has(values:&Overrides,path:&str)->bool{
    !values.is_empty()&&key(path).is_some_and(|k|values.contains_key(&k))
}
pub fn load(path:&Path)->Option<IconPixels>{
    if OVERRIDES.read().ok()?.is_empty(){return None;}
    let raw=path.to_str()?;
    let key=key(raw)?;
    let file=OVERRIDES.read().ok()?.get(&key)?.clone();
    if !valid_file(&file){return None;}
    let decoded=image(&directory().join(file)).ok()?;
    let (width,height)=decoded.dimensions();
    if width.max(height)>256{return None;}
    Some(IconPixels{width:width as i32,height:height as i32,pixels:premultiply(decoded.into_raw())})
}
fn premultiply(mut pixels:Vec<u8>)->Vec<u8>{
    for p in pixels.chunks_exact_mut(4){
        p.swap(0,2);
        for i in 0..3{p[i]=((u16::from(p[i])*u16::from(p[3])+127)/255) as u8;}
    }
    pixels
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn uses_full_identity_and_safe_file_names_with_premultiplied_pixels(){
        assert_eq!(normalized("C:/Apps/A.EXE"),normalized(r"c:\apps\a.exe"));
        assert_ne!(normalized("C:/Apps/A.EXE"),normalized("C:/Tools/A.EXE"));
        assert_eq!(normalized(r"shell:AppsFolder\Family!App").unwrap(),r"shell:appsfolder\family!app");
        assert!(!valid_file("../icon.png"));
        assert!(valid_file("0123456789abcdef0123456789abcdef.png"));
        assert_eq!(premultiply(vec![255,20,80,128,9,22,43,0]),[40,10,128,128,0,0,0,0]);
    }
}
