//! A full-size memory target for exercising the real GDI scaling path without
//! hiding the user's applications. Hidden window DCs clip the transfer away and
//! cannot measure the cost of presenting a wallpaper on an uncovered desktop.
use std::ffi::c_void;
use windows_sys::Win32::Graphics::Gdi::*;

pub struct MemoryTarget {
    pub dc:HDC,
    bitmap:HBITMAP,
    old:HGDIOBJ,
    bits:*mut u8,
    length:usize,
}
impl MemoryTarget {
    pub fn new(width:i32,height:i32)->Result<Self,String> {unsafe {
        let dc=CreateCompatibleDC(std::ptr::null_mut());
        if dc.is_null() {return Err("Không tạo được DC kiểm tra hiển thị".into());}
        let mut info:BITMAPINFO=std::mem::zeroed();
        info.bmiHeader.biSize=std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth=width;info.bmiHeader.biHeight=-height;
        info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;
        let mut bits:*mut c_void=std::ptr::null_mut();
        let bitmap=CreateDIBSection(dc,&info,DIB_RGB_COLORS,&mut bits,std::ptr::null_mut(),0);
        if bitmap.is_null() || bits.is_null() {DeleteDC(dc);return Err("Không tạo được ảnh kiểm tra hiển thị".into());}
        let old=SelectObject(dc,bitmap as _);
        let length=width as usize*height as usize*4;
        std::ptr::write_bytes(bits,0,length);
        SetStretchBltMode(dc,HALFTONE);SetBrushOrgEx(dc,0,0,std::ptr::null_mut());
        Ok(Self{dc,bitmap,old,bits:bits as _,length})
    }}
    pub fn pixels(&self)->Vec<u8> {unsafe {
        GdiFlush();std::slice::from_raw_parts(self.bits,self.length).to_vec()
    }}
}
impl Drop for MemoryTarget {fn drop(&mut self){unsafe {
    SelectObject(self.dc,self.old);DeleteObject(self.bitmap as _);DeleteDC(self.dc);
}}}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_target_executes_the_full_size_gdi_transfer() {unsafe {
        let target=MemoryTarget::new(31,17).unwrap();
        let pixels=vec![0x33u8,0x66,0xcc,0].repeat(8*5);
        let mut info:BITMAPINFO=std::mem::zeroed();
        info.bmiHeader.biSize=std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth=8;info.bmiHeader.biHeight=5;
        info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;
        assert_ne!(StretchDIBits(target.dc,0,0,31,17,0,0,8,5,pixels.as_ptr() as _,&info,DIB_RGB_COLORS,SRCCOPY),0);
        let result=target.pixels();
        assert!(result.chunks_exact(4).all(|p|p[..3]==[0x33,0x66,0xcc]));
    }}
}
