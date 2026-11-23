//! Isolated renderer proof. The source DIBs are drawn by the fixture; this
//! module has no desktop capture and is not compiled into the product.

use super::{Box2,Layout};
use crate::dock::{glass_probe::Layer,surface::{draw,rounded},geometry::Rect};
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::*,UI::WindowsAndMessaging::*};
use std::ptr::null_mut;

#[derive(Clone,Copy)]
struct Card{rect:Box2,right:bool,depth:f32}
impl Card{
    fn uv(self,x:f32,y:f32)->(f32,f32){
        let mut t=(x-self.rect.x as f32)/self.rect.w as f32;
        if self.right{t=1.0-t;}
        // Inverse of the homogeneous map:
        // t = (1+k)u/(1+ku), y = .5+(v-.5)/(1+ku).
        let u=t/(1.0+self.depth-self.depth*t);
        let v=0.5+((y-self.rect.y as f32)/self.rect.h as f32-0.5)*(1.0+self.depth*u);
        (if self.right{1.0-u}else{u},v)
    }
    fn contains(self,x:f32,y:f32)->bool{
        let r=self.rect;
        if x<r.x as f32||x>(r.x+r.w) as f32||y<r.y as f32||y>(r.y+r.h) as f32{return false;}
        let (u,v)=self.uv(x,y);
        (0.0..=1.0).contains(&u)&&(0.0..=1.0).contains(&v)
    }
    fn paint(self,pixels:&mut [u8],w:i32,h:i32,src:&[u8],sw:i32,sh:i32){
        draw(pixels,w,h,self.rect.rect(),|x,y|{
            let passed=[(-0.27,-0.27),(0.27,-0.27),(-0.27,0.27),(0.27,0.27)]
                .into_iter().filter(|&(dx,dy)|self.contains(x+dx,y+dy)).count();
            if passed==0{return None;}
            let (u,v)=self.uv(x,y);
            // Bilinear sampling after projective inverse maps all content,
            // rather than just tapering or rotating the thumbnail container.
            let fx=u.clamp(0.0,1.0)*(sw-1) as f32;
            let fy=v.clamp(0.0,1.0)*(sh-1) as f32;
            let (ix,iy)=(fx.floor() as usize,fy.floor() as usize);
            let (jx,jy)=((ix+1).min(sw as usize-1),(iy+1).min(sh as usize-1));
            let (a,b)=(fx.fract(),fy.fract());
            let at=|x,y,k|src[(y*sw as usize+x)*4+k] as f32;
            let tone=std::array::from_fn(|k|{
                let t=at(ix,iy,k)*(1.0-a)+at(jx,iy,k)*a;
                let end=at(ix,jy,k)*(1.0-a)+at(jx,jy,k)*a;
                (t*(1.0-b)+end*b).round() as u8
            });
            Some((tone,255.0*passed as f32/4.0))
        });
    }
}

fn hit(cards:&[Card],icon:Box2,close:Option<Box2>,x:f32,y:f32)->Option<usize>{
    if icon.hit(x as i32,y as i32){return Some(0);}
    if close.is_some_and(|control|control.hit(x as i32,y as i32)){return Some(0);}
    cards.iter().position(|card|card.contains(x,y))
}

#[test]
fn polygon_stack_respects_front_mask_rear_hit_and_mirror(){
    for dpi in [96,120,144]{for right in [false,true]{
        let mut l=Layout::landscape(252*dpi/96,600,dpi as u32,8,5,1,right,true);
        l.count=1;l.layers[0]=5;
        let cards=(0..5).map(|i|Card{rect:l.rear(0,i),right,depth:0.34}).collect::<Vec<_>>();
        let r=l.card(0);
        let near=if right{r.x+r.w-2}else{r.x+2};
        let far=if right{r.x+2}else{r.x+r.w-2};
        let icon=l.detached_icon(0);
        assert!(cards[0].contains(near as f32,(r.y+3) as f32));
        assert!(!cards[0].contains(far as f32,(r.y+3) as f32));
        let (x,y)=icon.center();
        assert_eq!(hit(&cards,icon,None,x as f32,y as f32),Some(0));
        let close=l.close_button(0).unwrap();
        let (x,y)=close.center();
        assert_eq!(hit(&cards,icon,Some(close),x as f32,y as f32),Some(0));
        assert_eq!(hit(&cards,icon,None,r.x as f32,r.y as f32-4.0),None);
        for (i,card) in cards.iter().enumerate(){
            let x=if right{card.rect.x+2}else{card.rect.x+card.rect.w-2};
            assert_eq!(hit(&cards,icon,None,x as f32,(r.y+r.h/2) as f32),Some(i));
        }
    }}
}

unsafe fn source(index:usize)->Layer{
    use crate::desktop::wide;
    let (w,h)=(480,270);
    let texture=Layer::new(w,h).unwrap();
    let fill=|x,y,right,bottom,c|{
        let brush=CreateSolidBrush(c);FillRect(texture.dc(),&RECT{left:x,top:y,right,bottom},brush);DeleteObject(brush);
    };
    let themes=[0x00facf99,0x00d1c6f7,0x00dbdf9d,0x00b4dff7,0x00f6badc];
    fill(0,0,w,h,0x00262420);
    fill(2,2,w-2,h-2,0x00f7f6f3);
    fill(2,2,w-2,44,0x002e2b26);
    fill(2,44,108,h-2,themes[index%themes.len()]);
    for y in [122,143,164,185,206,227]{fill(130,y,432-y/6,y+4,0x00a9a196);}
    let f=CreateFontW(-23,0,0,0,600,0,0,0,DEFAULT_CHARSET as u32,0,0,ANTIALIASED_QUALITY as u32,0,wide("Segoe UI Variable Text").as_ptr());
    let prev=SelectObject(texture.dc(),f);
    SetBkMode(texture.dc(),TRANSPARENT as i32);SetTextColor(texture.dc(),0x00ffffff);
    let title=wide(&format!("Stage fixture {}",index+1));TextOutW(texture.dc(),16,9,title.as_ptr(),title.len() as i32-1);
    SetTextColor(texture.dc(),0x003d3732);
    let text=wide("Native Windows source pixels");
    TextOutW(texture.dc(),129,76,text.as_ptr(),text.len() as i32-1);
    SelectObject(texture.dc(),prev);DeleteObject(f);GdiFlush();
    texture
}

#[test]
#[ignore = "native Windows layered renderer with owned fixture textures only"]
fn native_owned_window_pixels_are_projected_into_stage_stack(){
    unsafe{
        use crate::desktop::wide;
        let output=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE").map(std::path::PathBuf::from);
        if let Some(dir)=&output{std::fs::create_dir_all(dir).unwrap();}
        let hwnd=CreateWindowExW(WS_EX_LAYERED|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,
            wide("STATIC").as_ptr(),wide("Deskland perspective fixture").as_ptr(),WS_POPUP,
            20,20,252,190,null_mut(),null_mut(),null_mut(),null_mut());
        assert!(!hwnd.is_null());
        struct Window(HWND);
        impl Drop for Window{fn drop(&mut self){unsafe{DestroyWindow(self.0);}}}
        let _guard=Window(hwnd);
        let mut textures=(0..5).map(|i|source(i)).collect::<Vec<_>>();
        for (n,right,dpi,width) in [(1,false,96,252),(2,false,96,252),(3,false,120,315),
            (5,false,96,252),(8,false,96,252),(5,true,96,252),(8,true,144,302)]{
            let mut l=Layout::landscape(width,330,dpi,8,5,1,right,true);
            l.count=1;l.layers[0]=n.min(5);
            l.height=l.content_height();
            let mut target=Layer::new(l.width,l.height).unwrap();
            let data=target.pixels();data.fill(0);
            let started=std::time::Instant::now();
            for i in (0..l.depth(0)).rev(){
                let mut card=Card{rect:l.rear(0,i as i32),right,depth:0.34};
                card.rect.y+=2;
                draw(data,l.width,l.height,card.rect.rect(),|x,y|{
                    card.contains(x,y).then_some(([12,14,20],134.0))
                });
                card.rect.y-=2;
                let src=&mut textures[i];
                let (w,h)=src.size;
                card.paint(data,l.width,l.height,src.pixels(),w,h);
            }
            let icon=l.detached_icon(0).rect();
            rounded(data,l.width,l.height,icon,icon.w*0.25,[224,180,114],255);
            rounded(data,l.width,l.height,Rect{x:icon.x+5.0,y:icon.y+5.0,w:icon.w-10.0,h:icon.h-10.0},4.0,[73,47,28],255);
            if n>5{
                let c=l.card(0);
                let b=Box2{x:if right{c.x+8}else{c.x+c.w-42},y:c.y+c.h-28,w:33,h:20};
                rounded(data,l.width,l.height,b.rect(),8.0,[25,29,36],246);
                let mut mask=Layer::new(66,40).unwrap();
                let font=CreateFontW(-24,0,0,0,600,0,0,0,DEFAULT_CHARSET as u32,0,0,ANTIALIASED_QUALITY as u32,0,wide("Segoe UI").as_ptr());
                super::draw_mask_scaled(data,l.width,l.height,&mut mask,font,b,&format!("+{}",n-5),true,[255,255,255],2);
                DeleteObject(font);
            }
            assert!(data.chunks_exact(4).any(|p|p[3]==0)&&data.chunks_exact(4).any(|p|p[3]==255));
            println!("stage projection lab: windows={n} side={} dpi={dpi} cpu_draw_us={}",if right{"right"}else{"left"},started.elapsed().as_micros());
            assert!(target.present(hwnd));
            if let Some(dir)=&output{
                let rgba=target.pixels().chunks_exact(4).flat_map(|p|{
                    let a=p[3];
                    let decode=|i:usize|if a==0{0}else{((u16::from(p[i])*255+u16::from(a)/2)/u16::from(a)).min(255) as u8};
                    [decode(2),decode(1),decode(0),a]
                }).collect::<Vec<_>>();
                let path=dir.join(format!("stage-fixture-{n}-{}-{dpi}.png",if right{"right"}else{"left"}));
                image::save_buffer(path,&rgba,l.width as u32,l.height as u32,image::ColorType::Rgba8).unwrap();
            }
        }
    }
}
