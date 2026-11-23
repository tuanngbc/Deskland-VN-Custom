//! Isolated renderer proof. The source DIBs are drawn by the fixture; this
//! module has no desktop capture and is not compiled into the product.

use super::{Box2,Layout};
use crate::dock::{glass_probe::Layer,surface::{draw,rounded},geometry::Rect};
use windows_sys::Win32::{Foundation::*,Graphics::Gdi::*,UI::WindowsAndMessaging::*};
use std::ptr::null_mut;

use crate::dock::stage_projection::Card;

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
        let cards=(0..5).map(|i|Card::new(l.rear(0,i),right,28.0)).collect::<Vec<_>>();
        let r=l.card(0);
        let near=if right{r.x+r.w-2}else{r.x+2};
        let far=if right{r.x+2}else{r.x+r.w-2};
        let icon=l.detached_icon(0);
        assert!(cards[0].contains(near as f32,(r.y+8) as f32));
        assert!(!cards[0].contains(far as f32,(r.y+8) as f32));
        let hover=Card::new(r,right,19.0);
        assert!(hover.depth>0.0&&hover.depth<cards[0].depth);
        assert_eq!(Card::new(r,right,0.0).depth,0.0);
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
    source_with_size(index,480,270)
}
unsafe fn source_with_size(index:usize,w:i32,h:i32)->Layer{
    use crate::desktop::wide;
    let texture=Layer::new(w,h).unwrap();
    let fill=|x,y,right,bottom,c|{
        let brush=CreateSolidBrush(c);FillRect(texture.dc(),&RECT{left:x,top:y,right,bottom},brush);DeleteObject(brush);
    };
    let themes=[0x00facf99,0x00d1c6f7,0x00dbdf9d,0x00b4dff7,0x00f6badc];
    fill(0,0,w,h,0x00262420);
    fill(2,2,w-2,h-2,0x00f7f6f3);
    fill(2,2,w-2,44,0x002e2b26);
    let divider=w*24/100;
    fill(2,44,divider,h-2,themes[index%themes.len()]);
    for y in (120..h-6).step_by(21){
        fill(divider+16,y,w-14-(y/21)%17,y+4,0x00a9a196);
        fill(divider+16,y+8,w-20,y+9,0x00c1bcaf);
    }
    let f=CreateFontW(-23,0,0,0,600,0,0,0,DEFAULT_CHARSET as u32,0,0,ANTIALIASED_QUALITY as u32,0,wide("Segoe UI Variable Text").as_ptr());
    let prev=SelectObject(texture.dc(),f);
    SetBkMode(texture.dc(),TRANSPARENT as i32);SetTextColor(texture.dc(),0x00ffffff);
    let title=wide(&format!("Stage fixture {}",index+1));TextOutW(texture.dc(),16,9,title.as_ptr(),title.len() as i32-1);
    SetTextColor(texture.dc(),0x003d3732);
    let text=wide("Windows preview");
    TextOutW(texture.dc(),divider+16,76,text.as_ptr(),text.len() as i32-1);
    SelectObject(texture.dc(),prev);DeleteObject(f);GdiFlush();
    texture
}

#[test]
#[ignore = "Windows DIBs and layered fixture; exports no owner pixels"]
fn native_stage_aa_comparison_at_three_dpi(){
    unsafe{
        let mut textures=[source_with_size(0,240,480),source_with_size(1,480,270),source_with_size(2,360,350)];
        let hwnd=CreateWindowExW(WS_EX_LAYERED|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,
            crate::desktop::wide("STATIC").as_ptr(),crate::desktop::wide("Stage AA comparison fixture").as_ptr(),
            WS_POPUP,20,20,260,500,null_mut(),null_mut(),null_mut(),null_mut());
        assert!(!hwnd.is_null());
        struct Guard(HWND);
        impl Drop for Guard{fn drop(&mut self){unsafe{DestroyWindow(self.0);}}}
        let _guard=Guard(hwnd);
        let output=std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE").map(std::path::PathBuf::from);
        if let Some(dir)=&output{std::fs::create_dir_all(dir).unwrap();}
        for dpi in [96,120,144]{
            let scale=|n:i32|n*dpi/96;
            let (w,h)=(scale(260),scale(370));
            let mut project=|filtered:bool,hover:bool,shadows:bool,layer:&mut Layer|{
                let data=layer.pixels();data.fill(0);
                for (row,depth) in [1,1,1,3].into_iter().enumerate(){
                    for j in (0..depth).rev(){
                        let right=row%2==1;
                        let push=scale(13*j as i32);
                        let r=Box2{x:scale(38)+if right{-push}else{push},y:scale(14+88*row as i32)+push/4,
                            w:scale(176),h:scale(73)-push/4};
                        let card=Card::new(r,right,if hover{19.0}else{28.0});
                        let src=&mut textures[(row+j)%3];
                        let (sw,sh)=src.size;
                        if shadows{card.shadow_with_filter(data,w,h,j,filtered);}
                        card.paint_with_filter(data,w,h,src.pixels(),sw,sh,j,filtered);
                    }
                }
            };
            let (mut old,mut new)=(Layer::new(w,h).unwrap(),Layer::new(w,h).unwrap());
            let mut times=[Vec::new(),Vec::new()];
            for repeat in 0..16{
                for filtered in [repeat%2==1,repeat%2==0]{
                    let then=std::time::Instant::now();
                    project(filtered,repeat%4>=2,true,if filtered{&mut new}else{&mut old});
                    times[usize::from(filtered)].push(then.elapsed().as_micros());
                }
            }
            for t in &mut times{t.sort_unstable();}
            project(false,false,false,&mut old);project(true,false,false,&mut new);
            let edge_levels=|p:&mut Layer|{
                let mut counts=[false;256];
                for px in p.pixels().chunks_exact(4){
                    assert!(px[..3].iter().all(|v|*v<=px[3]),"non-premultiplied edge at {dpi}");
                    counts[px[3] as usize]=true;
                }
                counts[1..255].iter().filter(|b|**b).count()
            };
            let (old_levels,new_levels)=(edge_levels(&mut old),edge_levels(&mut new));
            assert!(new_levels>old_levels+12);
            project(false,false,true,&mut old);project(true,false,true,&mut new);
            assert!(new.present(hwnd));
            println!("STAGE_AA_FIXTURE dpi={dpi} baseline_cpu_median_us={} filtered_cpu_median_us={} edge_alpha_levels={old_levels}->{new_levels} result_bytes={} includes_stacks=3",
                times[0][8],times[1][8],w*h*4);
            if let Some(dir)=&output{
                let (old,new)=(old.pixels(),new.pixels());
                let bg=|dark:bool,x:usize,y:usize|{
                    let tone=((x+2*y)%149/7) as u8;
                    if dark{[32+tone,25+tone,17+tone]}else{[234+tone,222+tone,203+tone]}
                };
                for dark in [false,true]{
                    // Previous on the left; improved on the right. Pixel scale 1:1.
                    let mut result=image::RgbImage::new(2*w as u32,h as u32);
                    for (x,y,p) in result.enumerate_pixels_mut(){
                        let (base,xx)=if x<w as u32{(&*old,x)}else{(&*new,x-w as u32)};
                        let i=(y as usize*w as usize+xx as usize)*4;
                        let under=bg(dark,xx as usize,y as usize);
                        let inv=255-base[i+3] as u16;
                        *p=image::Rgb(std::array::from_fn(|k|
                            (base[i+2-k] as u16+(under[2-k] as u16*inv+127)/255).min(255) as u8));
                    }
                    let name=if dark{"dark"}else{"light"};
                    result.save(dir.join(format!("stage-aa-{dpi}-{name}-before-after.png"))).unwrap();
                    let clip=|x:u32|image::imageops::crop_imm(&result,x,scale(12) as u32,scale(140) as u32,scale(50) as u32).to_image();
                    let a=clip(scale(67) as u32);let b=clip((w+scale(67)) as u32);
                    let mut detail=image::RgbImage::new(a.width()*2,a.height());
                    image::imageops::replace(&mut detail,&a,0,0);
                    image::imageops::replace(&mut detail,&b,i64::from(a.width()),0);
                    detail.save(dir.join(format!("stage-aa-{dpi}-{name}-edge-1to1.png"))).unwrap();
                }
            }
        }
    }
}

#[test]
#[ignore = "native Windows layered renderer with owned fixture textures only"]
fn native_fixture_dibs_are_projected_into_stage_stack(){
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
                let card=Card::new(l.rear(0,i as i32),right,28.0);
                for (dy,alpha) in [(5,24.0),(3,36.0),(1,48.0)]{
                    let shadow=Card{rect:Box2{y:card.rect.y+dy,..card.rect},..card};
                    draw(data,l.width,l.height,shadow.rect.rect(),|x,y|
                        shadow.contains(x,y).then_some(([12,14,20],alpha)));
                }
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
            let front=Card::new(l.card(0),right,28.0);
            let stack=(0..l.depth(0)).map(|depth|Card::new(l.rear(0,depth as i32),right,28.0).polygon()).collect::<Vec<_>>();
            let next=l.rear(0,1);
            println!("stage projection fixture: members={n} side={} dpi={dpi} front={:?} near={} far={:.1} ratio={:.3} yaw=28 stack_dx={} stack_dy={} icon={icon:?} polygons={stack:?} cpu_draw_us={}",
                if right{"right"}else{"left"},front.rect,front.rect.h,front.rect.h as f32/(1.0+front.depth),1.0+front.depth,
                next.x-front.rect.x,next.y-front.rect.y,started.elapsed().as_micros());
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
