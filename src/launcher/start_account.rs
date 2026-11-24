//! Use only the current token's Windows account picture. The Shell cache
//! varies by Windows version; fall back to the installed default picture.
use super::*;
use std::{ffi::OsString, os::windows::ffi::OsStringExt, ptr, sync::mpsc};
use crate::desktop::current_sid;
use windows_sys::Win32::System::Registry::{RegGetValueW,HKEY_LOCAL_MACHINE,RRF_RT_REG_SZ};

unsafe fn picture_path(size:i32)->Option<PathBuf>{
    let sid=current_sid()?;
    let key=wide(&format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\AccountPicture\Users\{sid}"));
    let sizes=if size<=64{[64,96,192,48,32]}else{[192,96,64,48,32]};
    for n in sizes{
        let value=wide(&format!("Image{n}"));
        let mut buf=[0u16;2048];
        let mut len=(buf.len()*2) as u32;
        if RegGetValueW(HKEY_LOCAL_MACHINE,key.as_ptr(),value.as_ptr(),RRF_RT_REG_SZ,ptr::null_mut(),
            buf.as_mut_ptr().cast(),&mut len)!=0{continue;}
        let units=(len as usize/2).min(buf.len());
        let n=buf[..units].iter().position(|&c|c==0).unwrap_or(units);
        let path=PathBuf::from(OsString::from_wide(&buf[..n]));
        // Profile cache entries are ordinary local Windows paths; don't open UNC/device paths.
        if path.components().next().is_some_and(|p|matches!(p,
            std::path::Component::Prefix(prefix) if matches!(prefix.kind(),std::path::Prefix::Disk(_))))&&path.is_absolute()&&path.is_file(){
            return Some(path);
        }
    }
    None
}

fn load(path:&Path,size:i32)->Option<IconPixels>{
    if std::fs::metadata(path).ok()?.len()>8*1024*1024{return None;}
    let mut reader=image::ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    let mut limits=image::Limits::default();
    limits.max_image_width=Some(2048);
    limits.max_image_height=Some(2048);
    limits.max_alloc=Some(16*1024*1024);
    reader.limits(limits);
    let source=reader.decode().ok()?;
    let rgba=source.resize_to_fill(size as u32,size as u32,image::imageops::FilterType::Lanczos3).to_rgba8();
    let mut pixels=Vec::with_capacity((size*size*4) as usize);
    let radius=size as f32*0.5;
    for (x,y,c) in rgba.enumerate_pixels(){
        let edge=(radius+0.5-(x as f32+0.5-radius).hypot(y as f32+0.5-radius)).clamp(0.0,1.0);
        let alpha=(edge*f32::from(c[3])).round() as u8;
        for i in [2,1,0]{pixels.push(((u16::from(c[i])*u16::from(alpha)+127)/255) as u8);}
        pixels.push(alpha);
    }
    Some(IconPixels{width:size,height:size,pixels})
}

pub(super) fn worker(size:i32)->Option<mpsc::Receiver<Option<IconPixels>>>{
    let (tx,rx)=mpsc::channel();
    std::thread::Builder::new().name("TLauncher Start avatar".into()).spawn(move||{
        let pixels=unsafe{picture_path(size)}.and_then(|path|load(&path,size)).or_else(||{
            let dir=PathBuf::from(std::env::var_os("ProgramData")?).join("Microsoft/User Account Pictures");
            ["user.png","user-192.png","user.bmp"].into_iter().find_map(|p|load(&dir.join(p),size))
        });
        let _=tx.send(pixels);
    }).ok()?;
    Some(rx)
}
