//! User-selected app artwork. Deskland copies a bounded image into the active
//! profile. Launch targets and Windows shortcuts are never modified.
use std::{collections::{BTreeMap,HashMap,VecDeque},io::{Read,Write},path::{Path,PathBuf},sync::{Arc,LazyLock,Mutex,RwLock,atomic::{AtomicU64,Ordering}},time::{Duration,Instant,SystemTime}};
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
    let file=override_path(path)?;
    let decoded=image(&file).ok()?;
    let (width,height)=decoded.dimensions();
    if width.max(height)>256{return None;}
    Some(IconPixels{width:width as i32,height:height as i32,pixels:premultiply(decoded.into_raw())})
}
fn override_path(path:&Path)->Option<PathBuf>{
    if OVERRIDES.read().ok()?.is_empty(){return None;}
    let raw=path.to_str()?;
    let key=key(raw)?;
    let file=OVERRIDES.read().ok()?.get(&key)?.clone();
    if !valid_file(&file){return None;}
    Some(directory().join(file))
}

// Source rasters are shared across the launcher worker and the Dock worker.
// Keep the source separate from destination size and never cache derived
// pixels across two different source stamps.
const SOURCE_RAM:usize=24*1024*1024;
const SOURCE_DISK:u64=128*1024*1024;
const MAX_BYTES:usize=512*512*4;
const MAGIC:&[u8;8]=b"TLBGRA01";
type SourceRows=VecDeque<(String,Arc<IconPixels>)>;
static SOURCES:LazyLock<Mutex<SourceRows>>=LazyLock::new(||Mutex::new(VecDeque::new()));
static SOURCE_FETCH:Mutex<()>=Mutex::new(());
static LAST_PRUNE:Mutex<Option<Instant>>=Mutex::new(None);

pub(crate) fn source_tier(edge:i32)->i32 {
    [96,128,192,256].into_iter().find(|&tier|tier>=edge).unwrap_or(256)
}

fn stamp(path:&Path)->Option<String>{
    let raw=path.to_str()?;
    if raw.starts_with(r"\\") {return None;}
    let meta=std::fs::metadata(path).ok()?;
    if !meta.is_file(){return None;}
    let ms=meta.modified().ok()?.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_nanos();
    Some(format!("{}:{}:{ms}",normalized(raw)?,meta.len()))
}

fn package_stamp(raw:&str)->Option<String>{
    use windows_sys::Win32::{Foundation::ERROR_INSUFFICIENT_BUFFER,Storage::Packaging::Appx::GetPackagesByPackageFamily};
    let id=raw.strip_prefix("shell:appsfolder\\")?;
    let (family,_)=id.split_once('!')?;
    let family=crate::desktop::wide(family);
    unsafe{
        let mut count=0u32;let mut len=0u32;
        if GetPackagesByPackageFamily(family.as_ptr(),&mut count,std::ptr::null_mut(),&mut len,std::ptr::null_mut())!=ERROR_INSUFFICIENT_BUFFER ||
            !(1..=64).contains(&count)||!(1..=32768).contains(&len){return None;}
        let mut names=vec![std::ptr::null_mut();count as usize];
        let mut buffer=vec![0u16;len as usize];
        if GetPackagesByPackageFamily(family.as_ptr(),&mut count,names.as_mut_ptr(),&mut len,buffer.as_mut_ptr())!=0{return None;}
        let mut rows=buffer.get(..len as usize)?.split(|&c|c==0).filter(|s|!s.is_empty())
            .map(String::from_utf16_lossy).collect::<Vec<_>>();
        rows.sort_unstable();
        (!rows.is_empty()).then(||rows.join(";"))
    }
}

fn link_icon_stamp(path:&Path)->Option<String>{
    use windows::{core::{Interface,PCWSTR},Win32::{System::Com::{CoCreateInstance,CLSCTX_INPROC_SERVER,IPersistFile,STGM_READ},
        UI::Shell::{IShellLinkW,ShellLink}}};
    unsafe {
        let name=crate::desktop::wide(path.to_str()?);
        let link:IShellLinkW=CoCreateInstance(&ShellLink,None,CLSCTX_INPROC_SERVER).ok()?;
        link.cast::<IPersistFile>().ok()?.Load(PCWSTR(name.as_ptr()),STGM_READ).ok()?;
        let mut file=[0u16;1024];
        let mut index=0;
        link.GetIconLocation(&mut file,&mut index).ok()?;
        let end=file.iter().position(|&c|c==0)?;
        if end==0{return Some(String::new());}
        let icon=String::from_utf16(&file[..end]).ok()?;
        let icon=Path::new(&icon);
        // Do not persist if a separately referenced icon cannot be stamped.
        Some(format!("{index}:{}",stamp(icon)?))
    }
}

fn source_stamp(path:&Path)->Option<String>{
    let raw=normalized(path.to_str()?)?;
    if raw.starts_with("shell:appsfolder\\"){return package_stamp(&raw);}
    if !path.is_absolute(){return None;}
    let src=stamp(path)?;
    if raw.ends_with(".lnk"){
        let referent=unsafe{shortcut_target(path.to_str()?)}.and_then(|p|stamp(Path::new(&p))).unwrap_or_default();
        let icon=link_icon_stamp(path)?;
        Some(format!("{src}|{referent}|{icon}"))
    }else{Some(src)}
}

fn hash(bytes:&[u8])->u64 {
    bytes.iter().fold(0xcbf29ce484222325u64,|h,&b|(h^u64::from(b)).wrapping_mul(0x100000001b3))
}

fn cache_path(dir:&Path,key:&str)->PathBuf{
    dir.join(format!("{:016x}.bgra",hash(key.as_bytes())))
}

fn read_raster(path:&Path,key:&str)->Option<IconPixels>{
    let mut f=std::fs::File::open(path).ok()?;
    let len=f.metadata().ok()?.len() as usize;
    if len<24||len>MAX_BYTES+64*1024{return None;}
    let mut header=[0;24];f.read_exact(&mut header).ok()?;
    if &header[..8]!=MAGIC{return None;}
    let width=u16::from_le_bytes(header[8..10].try_into().ok()?) as i32;
    let height=u16::from_le_bytes(header[10..12].try_into().ok()?) as i32;
    let size=u32::from_le_bytes(header[12..16].try_into().ok()?) as usize;
    let sum=u64::from_le_bytes(header[16..24].try_into().ok()?);
    if !(1..=512).contains(&width)||!(1..=512).contains(&height)||
        size!=key.len()||len!=24+size+(width*height*4) as usize{return None;}
    let mut stored=vec![0;size];f.read_exact(&mut stored).ok()?;
    if stored!=key.as_bytes(){return None;}
    let mut pixels=vec![0;(width*height*4) as usize];f.read_exact(&mut pixels).ok()?;
    (hash(&pixels)==sum).then_some(IconPixels{width,height,pixels})
}

fn write_raster(path:&Path,key:&str,icon:&IconPixels)->std::io::Result<()>{
    let parent=path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    std::fs::create_dir_all(parent)?;
    let tmp=path.with_extension("part");
    let result=(||{
        let mut f=std::fs::File::create(&tmp)?;
        f.write_all(MAGIC)?;
        f.write_all(&(icon.width as u16).to_le_bytes())?;
        f.write_all(&(icon.height as u16).to_le_bytes())?;
        f.write_all(&(key.len() as u32).to_le_bytes())?;
        f.write_all(&hash(&icon.pixels).to_le_bytes())?;
        f.write_all(key.as_bytes())?;
        f.write_all(&icon.pixels)?;
        f.flush()?;
        drop(f);
        std::fs::rename(&tmp,path)
    })();
    if result.is_err(){let _=std::fs::remove_file(tmp);}
    result
}

fn prune_cache(dir:&Path,limit:u64){
    let Ok(list)=std::fs::read_dir(dir) else{return;};
    let mut rows=list.filter_map(Result::ok).filter_map(|entry|{
        let path=entry.path();
        if path.extension()? != "bgra"{return None;}
        let meta=entry.metadata().ok()?;
        meta.is_file().then(||(meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),meta.len(),path))
    }).collect::<Vec<_>>();
    let mut total=rows.iter().map(|row|row.1).sum::<u64>();
    if total<=limit{return;}
    rows.sort_unstable_by_key(|row|row.0);
    for (_,len,path) in rows {
        if std::fs::remove_file(path).is_ok(){total=total.saturating_sub(len);}
        if total<=limit{return;}
    }
}

pub(crate) fn shared_source(path:&Path,shell_path:&Path,identity:&Path,tier:i32)->Option<Arc<IconPixels>>{
    let _serialize=SOURCE_FETCH.lock().ok()?;
    let custom=override_path(path).or_else(||override_path(identity));
    let stamp=if let Some(custom)=&custom {stamp(custom)} else{source_stamp(shell_path)};
    let key=stamp.map(|s| {
        // Custom files are already resolution-normalized and identical at all tiers.
        format!("v1|{}|{}|{}|{s}",normalized(identity.to_str().unwrap_or("")).unwrap_or_default(),
            if custom.is_some(){0}else{tier},u8::from(custom.is_some()))
    });
    if let Some(key)=&key{
        let mut ram=SOURCES.lock().ok()?;
        if let Some(index)=ram.iter().position(|(k,_)|k==key){
            let pair=ram.remove(index)?;
            let image=pair.1.clone();
            ram.push_back(pair);
            return Some(image);
        }
    }
    let cache=crate::lock::data_dir().join("cache").join("app-icons-v1");
    let file=key.as_ref().map(|k|cache_path(&cache,k));
    let mut result=file.as_ref().zip(key.as_ref()).and_then(|(f,k)|read_raster(f,k));
    if result.is_none(){
        result=load(path).or_else(||load(identity)).or_else(||unsafe{crate::launcher::shell_icon(shell_path,tier)});
        if let Some(((f,k),icon))=file.as_ref().zip(key.as_ref()).zip(result.as_ref()){
            let _=write_raster(f,k,icon);
            if let Ok(mut last)=LAST_PRUNE.lock(){
                if last.is_none_or(|t|t.elapsed()>Duration::from_secs(20)){
                    prune_cache(&cache,SOURCE_DISK);*last=Some(Instant::now());
                }
            }
        }
    }
    let result=Arc::new(result?);
    if let Some(key)=key{
        let mut ram=SOURCES.lock().ok()?;
        ram.push_back((key,result.clone()));
        let mut bytes=ram.iter().map(|(_,img)|img.pixels.len()).sum::<usize>();
        while ram.len()>512||bytes>SOURCE_RAM{
            let Some((_,old))=ram.pop_front() else{break;};bytes=bytes.saturating_sub(old.pixels.len());
        }
    }
    Some(result)
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
    #[test]
    fn disk_raster_round_trip_rejects_stale_identity_and_corruption(){
        let dir=std::env::temp_dir().join(format!("tlauncher-icon-cache-{}-{}",std::process::id(),SERIAL.fetch_add(1,Ordering::Relaxed)));
        let key="v1|c:\\apps\\example.exe|128|0|stamp1";
        let path=cache_path(&dir,key);
        let image=IconPixels{width:1,height:1,pixels:vec![21,78,108,128]};
        write_raster(&path,key,&image).unwrap();
        assert_eq!(read_raster(&path,key).unwrap().pixels,image.pixels);
        assert!(read_raster(&path,"v1|other").is_none());
        let mut broken=std::fs::read(&path).unwrap();
        *broken.last_mut().unwrap()^=1;
        std::fs::write(&path,broken).unwrap();
        assert!(read_raster(&path,key).is_none());
        write_raster(&path,key,&image).unwrap();
        assert_eq!(read_raster(&path,key).unwrap().pixels,image.pixels);
        prune_cache(&dir,0);
        assert!(!path.exists());
        std::fs::remove_dir(dir).unwrap();
    }
}
