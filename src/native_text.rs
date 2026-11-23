//! DirectWrite shaping and grayscale Direct2D rasterization into existing
//! native DIBs. One software target per UI thread; no GPU device or RGB
//! ClearType masks. Native EDIT controls retain Windows text input.
use std::{cell::RefCell,collections::VecDeque,mem::{size_of,zeroed}};
use windows::{core::PCWSTR,Win32::{Foundation::RECT as WinRect,Graphics::{
    Direct2D::{*,Common::*},DirectWrite::*,Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,Gdi::HDC as WinDc,
}}};
use windows_sys::Win32::{Foundation::RECT,Graphics::Gdi::*};

const FAMILY:&str="Segoe UI Variable Text";
const FALLBACK:&str="Segoe UI";
const CACHE:usize=192;

pub(crate) unsafe fn font_pixels(size:i32,weight:i32)->HFONT{
    CreateFontW(-size.max(1),0,0,0,weight,0,0,0,DEFAULT_CHARSET as u32,0,0,
        ANTIALIASED_QUALITY as u32,DEFAULT_PITCH as u32,crate::desktop::wide(FAMILY).as_ptr())
}

#[derive(PartialEq,Eq)]
struct Key{width:i32,height:i32,size:i32,weight:i32,center:bool,middle:bool}
struct Ctx{
    write:IDWriteFactory,render:ID2D1DCRenderTarget,brush:ID2D1SolidColorBrush,family:PCWSTR,
    lines:VecDeque<(Key,String,IDWriteTextLayout)>,
}
thread_local!{static CTX:RefCell<Option<Ctx>>=const{RefCell::new(None)};}

impl Ctx{
    unsafe fn new()->windows::core::Result<Self>{
        let write:IDWriteFactory=DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let mut collection=None;
        write.GetSystemFontCollection(&mut collection,false)?;
        let mut index=0;
        let mut found=windows::core::BOOL(0);
        let primary=windows::core::w!("Segoe UI Variable Text");
        if let Some(collection)=collection{collection.FindFamilyName(primary,&mut index,&mut found)?;}
        let family=if found.as_bool(){primary}else{windows::core::w!("Segoe UI")};
        let factory:ID2D1Factory=D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED,None)?;
        let properties=D2D1_RENDER_TARGET_PROPERTIES{
            r#type:D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat:D2D1_PIXEL_FORMAT{format:DXGI_FORMAT_B8G8R8A8_UNORM,alphaMode:D2D1_ALPHA_MODE_IGNORE},
            dpiX:96.0,dpiY:96.0,..Default::default()
        };
        let render=factory.CreateDCRenderTarget(&properties)?;
        render.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        let brush=render.CreateSolidColorBrush(&D2D1_COLOR_F{r:1.0,g:1.0,b:1.0,a:1.0},None)?;
        Ok(Self{write,render,brush,family,lines:VecDeque::new()})
    }
    unsafe fn layout(&mut self,k:Key,text:&str)->windows::core::Result<IDWriteTextLayout>{
        if let Some(i)=self.lines.iter().position(|(key,value,_)|*key==k&&value==text){
            if let Some(line)=self.lines.remove(i){let value=line.2.clone();self.lines.push_back(line);return Ok(value);}
        }
        let format=self.write.CreateTextFormat(self.family,None::<&IDWriteFontCollection>,
            DWRITE_FONT_WEIGHT(k.weight),DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,k.size as f32,windows::core::w!("vi-VN"))?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        if k.center{format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;}
        if k.middle{format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;}
        let ellipsis=self.write.CreateEllipsisTrimmingSign(&format)?;
        format.SetTrimming(&DWRITE_TRIMMING{granularity:DWRITE_TRIMMING_GRANULARITY_CHARACTER,..Default::default()},&ellipsis)?;
        let text=text.chars().take(256).collect::<String>();
        let value=self.write.CreateTextLayout(&text.encode_utf16().collect::<Vec<_>>(),&format,k.width as f32,k.height as f32)?;
        if self.lines.len()>=CACHE{self.lines.pop_front();}
        self.lines.push_back((k,text,value.clone()));
        Ok(value)
    }
}

/// Draw on an opaque DC or into a black mask. A false result means use GDI
/// on that surface; no changes to system font configuration.
pub(crate) unsafe fn draw(dc:HDC,r:RECT,text:&str,font:HFONT,ink:[u8;3],center:bool,middle:bool)->bool{
    if text.is_empty(){return true;}
    if dc.is_null()||font.is_null()||r.right<=r.left||r.bottom<=r.top{return false;}
    let mut chosen:LOGFONTW=zeroed();
    if GetObjectW(font,size_of::<LOGFONTW>() as i32,(&mut chosen as *mut LOGFONTW).cast())==0{return false;}
    let key=Key{width:r.right-r.left,height:r.bottom-r.top,size:chosen.lfHeight.saturating_abs().max(1),
        weight:chosen.lfWeight.clamp(100,950),center,middle};
    CTX.with(|ctx|{
        let Ok(mut slot)=ctx.try_borrow_mut()else{return false;};
        if slot.is_none(){*slot=Ctx::new().ok();}
        let Some(c)=slot.as_mut()else{return false;};
        let Ok(layout)=c.layout(key,text)else{return false;};
        GdiFlush();
        let rect=WinRect{left:r.left,top:r.top,right:r.right,bottom:r.bottom};
        if c.render.BindDC(WinDc(dc),&rect).is_err(){*slot=None;return false;}
        c.brush.SetColor(&D2D1_COLOR_F{r:ink[2] as f32/255.0,g:ink[1] as f32/255.0,b:ink[0] as f32/255.0,a:1.0});
        c.render.BeginDraw();
        c.render.DrawTextLayout(Default::default(),&layout,&c.brush,D2D1_DRAW_TEXT_OPTIONS_CLIP);
        if c.render.EndDraw(None,None).is_err(){*slot=None;return false;}
        true
    })
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    #[ignore="Windows native DirectWrite render into offscreen DIB"]
    fn grayscale_typography_sheet_at_96_120_144(){
        use crate::dock::PopupPixels;
        let output=std::env::var_os("TUANPC_DESK_TYPE_EVIDENCE").map(std::path::PathBuf::from);
        let samples=[("Tất cả ứng dụng · Cài đặt",12,450),("CPU 23%    RAM 64%    GPU 7%",12,600),
            ("↑ 12.8 MB/s   ↓ 860 KB/s",11,550),("Explorer · cửa sổ nhóm",13,550),("11:06    23/11/2026",14,540)];
        unsafe{
            for dpi in [96,120,144]{
                let scale=|n:i32|(n*dpi+48)/96;
                let w=scale(620);let h=scale(154);let mut pixels=PopupPixels::new(w,h).unwrap();
                let dc=pixels.dc();
                let fill=|r:RECT,c|{
                    let b=CreateSolidBrush(c);FillRect(dc,&r,b);DeleteObject(b);
                };
                for (row,(line,size,weight)) in samples.into_iter().enumerate(){
                    for mode in 0..2{
                        let y=scale(31*row as i32);
                        for col in 0..2{
                            let x=scale(154*(col+mode*2));
                            let rect=RECT{left:x,top:y,right:x+scale(154),bottom:y+scale(29)};
                            let (bg,fg)=if mode==0{(0x00f6f6f6,0x00222222)}else{(0x00232323,0x00f7f7f7)};
                            fill(rect,bg);
                            let font=font_pixels(scale(size),weight);
                            let mut area=RECT{left:x+scale(5),right:rect.right-scale(2),..rect};
                            if col==0{
                                let old=SelectObject(dc,font);SetBkMode(dc,TRANSPARENT as i32);SetTextColor(dc,fg);
                                DrawTextW(dc,crate::desktop::wide(line).as_ptr(),-1,&mut area,DT_SINGLELINE|DT_VCENTER|DT_END_ELLIPSIS|DT_NOPREFIX);
                                SelectObject(dc,old);
                            }else{
                                let v=(fg&255) as u8;
                                assert!(draw(dc,area,line,font,[v;3],false,true));
                            }
                            DeleteObject(font);
                        }
                    }
                }
                GdiFlush();
                let data=pixels.pixels();
                assert!(data.chunks_exact(4).all(|p|p[0]==p[1]&&p[1]==p[2]),"grayscale text must not introduce fringe");
                assert!(data.chunks_exact(4).any(|p|p[0]>48&&p[0]<230),"expected fractional glyph coverage");
                if let Some(dir)=&output{
                    std::fs::create_dir_all(dir).unwrap();
                    let rgb=data.chunks_exact(4).flat_map(|p|[p[2],p[1],p[0]]).collect::<Vec<_>>();
                    image::save_buffer(dir.join(format!("type-{dpi}.png")),&rgb,w as u32,h as u32,image::ColorType::Rgb8).unwrap();
                }
                println!("native grayscale typography dpi={dpi} font={FAMILY} fallback={FALLBACK}");
            }
        }
    }
}
