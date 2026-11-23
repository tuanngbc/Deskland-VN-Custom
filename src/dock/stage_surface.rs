//! The layered Stage surface paints premultiplied cards and caption masks.
//! A regular owned HWND displays the live DWM rectangles above it.
use super::surface::Palette;
use super::{geometry::Rect,surface::{rounded,draw,distance,control_backing,CONTROL_OPACITY,IconRasters,fallback_icon,Atlas},glass_probe::Layer,Item};
use crate::desktop::wide;
use windows_sys::Win32::{Foundation::{HWND,RECT,SIZE},Graphics::Gdi::*};

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) struct Box2 { pub x:i32,pub y:i32,pub w:i32,pub h:i32 }
impl Box2 {
    pub(super) fn rect(self)->Rect{Rect{x:self.x as f32,y:self.y as f32,w:self.w as f32,h:self.h as f32}}
    pub fn win(self)->RECT{RECT{left:self.x,top:self.y,right:self.x+self.w,bottom:self.y+self.h}}
    fn hit(self,x:i32,y:i32)->bool{x>=self.x&&x<self.x+self.w&&y>=self.y&&y<self.y+self.h}
    pub fn center(self)->(i32,i32){(self.x+self.w/2,self.y+self.h/2)}
}

/// One geometry for drawing the miniature page switcher and targeting it.
#[derive(Clone, Copy)]
pub(super) struct PageStrip { pub previous:Box2,pub next:Box2,pub middle:Box2,pub pages:usize }
impl PageStrip {
    fn new(boxes:Box2,pages:usize)->Self {
        let edge=boxes.h.min(boxes.w/4).max(1);
        Self{previous:Box2{w:edge,..boxes},next:Box2{x:boxes.x+boxes.w-edge,w:edge,..boxes},
            middle:Box2{x:boxes.x+edge,w:boxes.w-2*edge,..boxes},pages}
    }
    pub fn uses_dots(self)->bool{self.pages<=7&&self.middle.w>=7*self.pages as i32}
    fn dot(self,i:usize)->Box2{
        let a=self.middle.x+self.middle.w*i as i32/self.pages as i32;
        let b=self.middle.x+self.middle.w*(i+1) as i32/self.pages as i32;
        Box2{x:a,w:b-a,..self.middle}
    }
    pub fn target(self,x:i32,y:i32,page:usize)->Option<usize>{
        if self.previous.hit(x,y){return page.checked_sub(1);}
        if self.next.hit(x,y){return (page+1<self.pages).then_some(page+1);}
        if !self.uses_dots(){return None;}
        (0..self.pages).find(|&i|i!=page&&self.dot(i).hit(x,y))
    }
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) struct Layout {
    pub width:i32,pub height:i32,pub card_h:i32,pub inset:i32,pub gap:i32,
    pub radius:i32,pub caption:i32,pub rows:usize,pub count:usize,pub paged:bool,
    pub stack:u8,pub nav_h:i32,pub right:bool,
    pub layers:[u8;8],pub stack_design:bool,
}
impl Layout {
    pub fn new(width:i32,height:i32,dpi:u32,gap:i32,limit:usize,total:usize,right:bool)->Self{
        let px=|n:i32|(n*dpi.clamp(72,288) as i32+48)/96;
        let inset=px(6);
        let nav_h=px(21);
        let gap=gap.max(px(8));
        let min_h=px(68);
        let rows_for=|nav:i32|((height-inset*2-nav+gap)/(min_h+gap)).max(1) as usize;
        let limit=limit.clamp(1,8);
        let paged=total>rows_for(0).min(limit);
        let reserve=if paged{nav_h+gap}else{0};
        let rows=rows_for(reserve).min(limit);
        let shown=total.min(rows).max(1) as i32;
        let available=(height-2*inset-reserve-(shown-1)*gap)/shown;
        let ideal=((width-4*inset)*3/5+px(42)).clamp(min_h,px(160));
        let card_h=available.min(ideal).max(1);
        Self{width,height,card_h,inset,gap,radius:px(18),caption:px(37),rows,count:0,paged,stack:0,nav_h,right,layers:[0;8],stack_design:false}
    }
    pub fn card(self,i:usize)->Box2 {
        // Reserve the same margin for the stack on every card so thumbnails
        // do not jump sideways when a window joins a group.
        let room=if self.stack_design{self.step()*4}else{(self.inset*2).min((self.width-12).max(0)/3)};
        let icon_room=if self.stack_design{self.icon_size()+self.inset}else{0};
        let x=self.inset+if self.right{room}else{icon_room};
        Box2{x,y:self.inset+i as i32*(self.card_h+self.gap),w:(self.width-2*self.inset-room-icon_room).max(1),h:self.card_h}
    }
    fn step(self)->i32{(self.inset*4/5).max(3)}
    fn icon_size(self)->i32{(self.inset*5).min(self.card_h/2).max(1)}
    pub fn depth(self,i:usize)->usize{
        if self.stack_design{self.layers.get(i).copied().unwrap_or(0).max(1) as usize}
        else if self.stack&(1<<i)!=0{3}else{1}
    }
    fn rear(self,i:usize,depth:i32)->Box2{
        let mut c=self.card(i);
        if depth==0{return c;}
        let d=if self.stack_design{self.step()*depth}else{(self.inset*(depth+1)/2).max(1)};
        c.x+=if self.right{-d}else{d};
        c.y+=d/3;c.h=(c.h-d/2).max(1);c
    }
    fn detached_icon(self,i:usize)->Box2{
        let card=self.card(i);let size=self.icon_size();
        Box2{x:if self.right{card.x+card.w+self.inset}else{card.x-size-self.inset},
            y:card.y+(card.h-size)/2,w:size,h:size}
    }
    pub fn icon_hit(self,i:usize,x:i32,y:i32)->bool{self.stack_design&&self.detached_icon(i).hit(x,y)}
    pub fn close_button(self,i:usize)->Option<Box2>{
        if !self.stack_design||i>=self.count{return None;}
        let c=self.card(i);
        let size=(self.inset*7/2).clamp(14,self.caption.max(14));
        if c.w<2*size+3*self.inset||c.h<self.caption{return None;}
        Some(Box2{x:c.x+c.w-self.inset-size,y:c.y+c.h-(self.caption+size)/2,w:size,h:size})
    }
    pub fn close_hit(self,i:usize,x:i32,y:i32)->bool{
        self.close_button(i).is_some_and(|b|b.hit(x,y))
    }
    pub fn preview(self,i:usize)->RECT{self.preview_layer(i,0).0}
    pub fn preview_layer(self,i:usize,depth:usize)->(RECT,Option<RECT>){
        let c=self.rear(i,depth as i32);let p=self.inset.max(2);
        let mut slot=RECT{left:c.x+p,top:c.y+p,right:c.x+c.w-p,bottom:c.y+c.h-self.caption};
        if depth==0{return (slot,None);}
        let top=slot.top;let bottom=slot.bottom;
        let previous=self.rear(i,depth as i32-1);
        let clip=if self.right{
            slot.left=c.x;
            RECT{left:c.x,top,right:previous.x,bottom}
        }else{
            slot.right=c.x+c.w;
            RECT{left:previous.x+previous.w,top,right:c.x+c.w,bottom}
        };
        // Avoid covering rounded corners with rectangular DWM content. The
        // rear panel itself remains rounded and painted below these strips.
        let margin=(self.radius*2/3).min((bottom-top).max(0)/2);
        (slot,Some(RECT{top:top+margin,bottom:bottom-margin,..clip}))
    }
    pub fn member_hit(self,i:usize,x:i32,y:i32)->Option<usize>{
        if i>=self.count{return None;}
        if self.stack_design&&self.detached_icon(i).hit(x,y){return Some(0);}
        let inside=|b:Box2|b.hit(x,y)&&distance(b.rect(),self.radius.min(b.h/2) as f32,x as f32+0.5,y as f32+0.5)<=0.0;
        (0..self.depth(i)).find(|&layer|inside(self.rear(i,layer as i32)))
    }
    pub fn nav(self)->Option<Box2>{
        if !self.paged||self.count==0{return None;}
        let c=self.card(self.count-1);
        let w=(c.w*5/6).max(1);
        let y=c.y+c.h+self.gap;
        (y+self.nav_h<=self.height).then_some(Box2{x:c.x+(c.w-w)/2,y,w,h:self.nav_h})
    }
    pub fn pages(self,count:usize)->Option<PageStrip>{self.nav().map(|bounds|PageStrip::new(bounds,count))}
    pub fn content_height(self)->i32{
        let bottom=self.nav().map(|n|n.y+n.h)
            .or_else(||self.count.checked_sub(1).map(|i|{let r=self.card(i);r.y+r.h}))
            .unwrap_or(0);
        (bottom+self.inset.max(7)).clamp(1,self.height.max(1))
    }
    pub fn hit(self,x:i32,y:i32)->Option<usize>{
        (0..self.count).find(|&i|self.member_hit(i,x,y).is_some())
    }
    pub unsafe fn apply(self,hwnd:HWND)->bool {
        let region=CreateRectRgn(0,0,0,0);
        if region.is_null(){return false;}
        let add=|b:Box2,r:i32|{
            let diameter=(2*r).min(b.w).min(b.h).max(1);
            let tile=CreateRoundRectRgn(b.x,b.y,b.x+b.w+1,b.y+b.h+1,diameter,diameter);
            if tile.is_null(){return false;}
            let ok=CombineRgn(region,region,tile,RGN_OR)!=0;
            DeleteObject(tile);
            ok
        };
        let mut ok=true;
        for i in 0..self.count{
            for layer in 1..self.depth(i){ok&=add(self.rear(i,layer as i32),self.radius);}
            if self.stack_design{ok&=add(self.detached_icon(i),self.radius/2);}
            ok&=add(self.card(i),self.radius);
        }
        if let Some(nav)=self.nav(){ok&=add(nav,nav.h/2);}
        if ok&&SetWindowRgn(hwnd,region,1)!=0{return true;} // Win32 owns it after success.
        DeleteObject(region);
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Look {pub dark:bool,pub layout:Layout,pub active:u8,pub alpha:Option<u8>}

pub(super) struct Panel {layer:Layer,look:Option<Look>,static_pixels:Vec<u8>,mask:Option<Layer>,font:HFONT,large_fonts:[HFONT;2],dpi:u32,icons:IconRasters}
impl Panel {
    pub unsafe fn new(w:i32,h:i32,dpi:u32)->Option<Self>{
        let layer=Layer::new(w,h)?;
        let font=|size,weight|CreateFontW(-super::panel_chrome::scale(dpi,size),0,0,0,weight,0,0,0,
            DEFAULT_CHARSET as u32,0,0,ANTIALIASED_QUALITY as u32,DEFAULT_PITCH as u32,wide("Segoe UI Variable Text").as_ptr());
        Some(Self{layer,look:None,static_pixels:Vec::new(),mask:None,font:font(13,540),large_fonts:[font(26,540),font(22,420)],dpi,icons:IconRasters::default()})
    }
    pub fn geometry(&self)->(i32,i32,u32){(self.layer.size.0,self.layer.size.1,self.dpi)}
    pub fn font(&self)->HFONT{self.font}
    pub unsafe fn paint(&mut self,target:HDC,look:Look,items:&[Item],atlas:&Atlas,ready:&[bool],emphasis:Option<(usize,u8)>,layered:Option<(HWND,&[String],usize,usize,Option<usize>)>)->bool {
        let (w,h)=self.layer.size;
        let layout=look.layout;
        let palette=Palette::new(look.dark);
        if self.look!=Some(look){
            self.static_pixels.resize(w as usize*h as usize*4,0);
            self.static_pixels.fill(0);
            let data=&mut self.static_pixels;
            let alpha=look.alpha.unwrap_or(255);
            for i in 0..layout.count {
                let b=layout.card(i).rect();
                let radius=(layout.radius as f32).min(b.h/2.0);
                if look.alpha.is_some(){
                    let k=(layout.inset as f32*0.82).max(2.0);
                    let spread=Rect{x:b.x-k,y:b.y-k,w:b.w+2.0*k,h:b.h+2.0*k};
                    draw(data,w,h,spread,|x,y|{
                        let d=distance(b,radius,x,y-1.8);
                        Some(([6,5,4],58.0*(1.0-d.max(0.0)/k).clamp(0.0,1.0).powi(2)))
                    });
                }
                for layer in (1..layout.depth(i)).rev(){
                    let v=(layout.depth(i)-layer) as u8*5;
                    let c=if look.dark{[78+v,69+v,61+v]}else{[208+v,202+v,197+v]};
                    rounded(data,w,h,layout.rear(i,layer as i32).rect(),radius,c,alpha.max(205));
                }
                let active=look.active&(1<<i)!=0;
                let edge=if active{palette.accent}else if look.dark{[102,92,82]}else{[224,215,208]};
                let tint=if look.dark{[50,43,37]}else{[253,249,246]};
                draw(data,w,h,b,|x,y|{
                    let d=distance(b,radius,x,y);
                    let coverage=(0.5-d).clamp(0.0,1.0);
                    let line=(if active{1.8+d}else{1.1+d}).clamp(0.0,1.0);
                    let shine=(1.0-((y-b.y)/b.h).clamp(0.0,1.0)).powi(4);
                    let rgb=std::array::from_fn(|i|{
                        let base=(tint[i] as f32+shine*if look.dark{29.0}else{13.0}).min(255.0);
                        (base+(edge[i] as f32-base)*line).round() as u8
                    });
                    Some((rgb,coverage*(alpha as f32+40.0*line+14.0*shine).min(255.0)))
                });
                // Local caption backing keeps text stable on bright, dark and
                // moving content. The thumbnail and gaps keep the chosen alpha.
                let backing=control_backing(alpha as f32);
                if backing>0.0 {
                    let cap=(layout.caption as f32).min(b.h);
                    let fade=(cap*0.38).max(1.0);
                    let y0=b.y+b.h-cap;
                    draw(data,w,h,Rect{x:b.x,y:(y0-fade).max(b.y),w:b.w,h:(cap+fade).min(b.h)},|x,y|{
                        let coverage=(0.5-distance(b,radius,x,y)).clamp(0.0,1.0);
                        let amount=((y-y0+fade)/fade).clamp(0.0,1.0);
                        Some((tint,backing*coverage*amount))
                    });
                }
                // Recessed neutral well stays behind aspect-fit previews.
                let slot=layout.preview(i);
                let p=Rect{x:slot.left as f32,y:slot.top as f32,w:(slot.right-slot.left) as f32,h:(slot.bottom-slot.top) as f32};
                if p.w>0.0&&p.h>0.0 {
                    rounded(data,w,h,p,4.0,if look.dark{[32,29,27]}else{[224,218,213]},if look.alpha.is_some(){48}else{255});
                }
            }
            if let Some(nav)=layout.nav(){
                rounded(data,w,h,nav.rect(),nav.h as f32/2.0,if look.dark{[39,33,29]}else{[247,241,236]},alpha.max(CONTROL_OPACITY));
            }
            self.look=Some(look);
        }
        let data=self.layer.pixels();
        data.copy_from_slice(&self.static_pixels);
        if let Some((i,strength))=emphasis.filter(|&(i,a)|i<layout.count&&a>0){
            let b=layout.card(i).rect();
            let radius=(layout.radius as f32).min(b.h/2.0);
            let tint=palette.accent;
            draw(data,w,h,b,|x,y|{
                let d=distance(b,radius,x,y);
                let coverage=(0.5-d).clamp(0.0,1.0)*(d+2.0).clamp(0.0,1.0);
                Some((tint,coverage*0.82*strength as f32))
            });
        }
        let tint=if look.dark{[222,219,216]}else{[64,59,53]};
        for (i,item) in items.iter().enumerate(){
            let c=layout.card(i);
            let size=(layout.caption*2/3).max(1) as f32;
            let small=if layout.stack_design{layout.detached_icon(i).rect()}
                else{Rect{x:(c.x+layout.inset) as f32,y:(c.y+c.h) as f32-(layout.caption as f32+size)/2.0,w:size,h:size}};
            if let Some(Some(icon))=atlas.get(&item.path){self.icons.paint(data,w,h,small,icon);}
            else{fallback_icon(data,w,h,small,tint);}
            let slot=layout.preview(i);
            if !layout.stack_design&&!ready[i]&&slot.bottom-slot.top>6 {
                let s=((slot.bottom-slot.top-5) as f32/1.4).min(size*1.7);
                let tile=Rect{x:(slot.left+slot.right) as f32/2.0-s*0.69,y:(slot.top+slot.bottom) as f32/2.0-s*0.69,w:s*1.38,h:s*1.38};
                rounded(data,w,h,tile,tile.w*0.31,palette.accent,if look.dark{42}else{33});
                let center=Rect{x:tile.x+s*0.19,y:tile.y+s*0.19,w:s,h:s};
                if let Some(Some(icon))=atlas.get(&item.path){self.icons.paint(data,w,h,center,icon);}
                else{fallback_icon(data,w,h,center,tint);}
            }
        }
        if let Some((hwnd,labels,page,pages,hover))=layered {
            let mask_h=layout.caption.max(layout.nav_h)*2;
            if self.mask.as_ref().is_none_or(|m|m.size!=(w*2,mask_h)){self.mask=Layer::new(w*2,mask_h);}
            if let Some(mask)=self.mask.as_mut(){
                let ink=palette.ink;
                for (i,title) in labels.iter().enumerate(){
                    let mut r=caption_box(layout,i);
                    if let Some(button)=hover.filter(|&index|index==i).and_then(|_|layout.close_button(i)){
                        r.w=(button.x-r.x-layout.inset/2).max(1);
                        rounded(data,w,h,button.rect(),button.w as f32/2.0,
                            if look.dark{[116,71,68]}else{[247,219,216]},255);
                        let mark=if look.dark{[255,218,215]}else{[117,36,34]};
                        draw_mask_scaled(data,w,h,mask,self.large_fonts[0],button,"×",true,mark,2);
                    }
                    let main=Box2{y:r.y+(r.h/10),h:r.h/2,..r};
                    let small=Box2{y:main.y+main.h,h:r.h-main.h-r.h/5,..r};
                    draw_mask_scaled(data,w,h,mask,self.large_fonts[0],main,title,false,ink,2);
                    let context=if layout.stack&(1<<i)!=0{
                        if items[i].count>5&&layout.stack_design{format!("{} cửa sổ · +{}",items[i].count,items[i].count-5)}else{format!("{} cửa sổ",items[i].count)}
                    }else if look.active&(1<<i)!=0{
                        "Đang dùng".to_owned()
                    }else{
                        "Cửa sổ".to_owned()
                    };
                    draw_mask_scaled(data,w,h,mask,self.large_fonts[1],small,&context,false,palette.muted,2);
                }
                if let Some(nav)=layout.pages(pages){
                    let muted=if look.dark{[161,154,148]}else{[125,114,103]};
                    draw_mask(data,w,h,mask,self.font,nav.previous,"‹",true,if page>0{ink}else{muted});
                    draw_mask(data,w,h,mask,self.font,nav.next,"›",true,if page+1<pages{ink}else{muted});
                    if nav.uses_dots(){
                        let weight=(self.dpi as f32/96.0).clamp(0.75,3.0);
                        for i in 0..pages{
                            let (x,y)=nav.dot(i).center();
                            let dot=if i==page{2.15}else{1.5}*weight;
                            rounded(data,w,h,Rect{x:x as f32-dot,y:y as f32-dot,w:2.0*dot,h:2.0*dot},dot,
                                if i==page{ink}else{muted},if i==page{245}else{174});
                        }
                    }else{
                        draw_mask(data,w,h,mask,self.font,nav.middle,&format!("{}/{pages}",page+1),true,ink);
                    }
                }
            }
            self.layer.present(hwnd)
        } else {self.layer.blit(target)}
    }
}
impl Drop for Panel {
    fn drop(&mut self){for font in [self.font,self.large_fonts[0],self.large_fonts[1]]{if !font.is_null(){unsafe{DeleteObject(font);}}}}
}
fn caption_box(layout:Layout,i:usize)->Box2{
    let c=layout.card(i);
    let caption_offset=if layout.stack_design{0}else{layout.caption};
    Box2{x:c.x+layout.inset+caption_offset,y:c.y+c.h-layout.caption,w:c.w-2*layout.inset-caption_offset,h:layout.caption}
}
fn fit_caption(text:&str,mut fits:impl FnMut(&str)->bool)->String {
    if fits(text){return text.to_owned();}
    if !fits("…"){return String::new();}
    for (at,c) in text.char_indices().skip(1){
        if matches!(c,'\u{0300}'..='\u{036f}'){continue;}
        let candidate=format!("…{}",text[at..].trim_start());
        if fits(&candidate){return candidate;}
    }
    "…".into()
}
unsafe fn fit_on_dc(dc:HDC,text:&str,width:i32)->String{
    // Limit work on external titles; reserve the final identifying part.
    let count=text.chars().count();
    let clipped=if count>96{format!("…{}",text.chars().skip(count-95).collect::<String>())}else{text.to_owned()};
    fit_caption(&clipped,|s|{
        let ws=wide(s);
        let mut size:SIZE=std::mem::zeroed();
        GetTextExtentPoint32W(dc,ws.as_ptr(),(ws.len()-1) as i32,&mut size)!=0&&size.cx<=width
    })
}
pub(super) unsafe fn draw_mask(data:&mut[u8],w:i32,h:i32,mask:&mut Layer,font:HFONT,b:Box2,text:&str,center:bool,ink:[u8;3]){
    draw_mask_scaled(data,w,h,mask,font,b,text,center,ink,1);
}
pub(super) unsafe fn draw_mask_scaled(data:&mut[u8],w:i32,h:i32,mask:&mut Layer,font:HFONT,b:Box2,text:&str,center:bool,ink:[u8;3],scale:i32){
    let (mw,mh)=(b.w*scale,b.h*scale);
    if b.w<1||b.h<1||b.x<0||b.y<0||b.x+b.w>w||b.y+b.h>h||mw>mask.size.0||mh>mask.size.1{return;}
    let stride=mask.size.0 as usize*4;
    for row in mask.pixels()[..mh as usize*stride].chunks_exact_mut(stride){row[..mw as usize*4].fill(0);}
    let dc=mask.dc();
    let previous=if font.is_null(){std::ptr::null_mut()}else{SelectObject(dc,font)};
    SetBkMode(dc,TRANSPARENT as i32);SetTextColor(dc,0x00ffffff);
    let mut rect=RECT{left:0,top:0,right:mw,bottom:mh};
    let value=if center{text.to_owned()}else{fit_on_dc(dc,text,mw)};
    DrawTextW(dc,wide(&value).as_ptr(),-1,&mut rect,DT_SINGLELINE|DT_END_ELLIPSIS|DT_VCENTER|DT_NOPREFIX|if center{DT_CENTER}else{0});
    GdiFlush();
    if !previous.is_null(){SelectObject(dc,previous);}
    let mask=mask.pixels();
    let scale=scale as usize;
    for y in 0..b.h as usize {
        for x in 0..b.w as usize {
            let at_mask=(y*stride+x*4)*scale;
            let a=if scale==2 {
                let sum=mask[at_mask] as u32+mask[at_mask+4] as u32+mask[at_mask+stride] as u32+mask[at_mask+stride+4] as u32;
                (sum+2)/4
            }else{mask[at_mask] as u32};
            if a==0{continue;}
            let at=((b.y as usize+y)*w as usize+b.x as usize+x)*4;
            let d=&mut data[at..at+4];
            for c in 0..3 {d[c]=((a*ink[c] as u32+(255-a)*d[c] as u32+127)/255) as u8;}
            d[3]=(a+(255-a)*d[3] as u32/255) as u8;
        }
    }
}
pub(super) unsafe fn caption(dc:HDC,layout:Layout,i:usize,text:&str,ink:u32){
    let mut r=caption_box(layout,i).win();
    if r.left>=r.right{return;}
    SetTextColor(dc,ink);SetBkMode(dc,TRANSPARENT as i32);
    let value=fit_on_dc(dc,text,r.right-r.left);
    DrawTextW(dc,wide(&value).as_ptr(),-1,&mut r,DT_SINGLELINE|DT_END_ELLIPSIS|DT_VCENTER|DT_NOPREFIX);
}
pub(super) unsafe fn nav_label(dc:HDC,b:Box2,label:&str,ink:u32){
    let mut r=b.win();
    SetTextColor(dc,ink);SetBkMode(dc,TRANSPARENT as i32);
    DrawTextW(dc,wide(label).as_ptr(),-1,&mut r,DT_CENTER|DT_SINGLELINE|DT_VCENTER|DT_NOPREFIX);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn measured_caption_keeps_a_vietnamese_suffix_and_short_titles(){
        assert_eq!(fit_caption("Lịch",|t|t.chars().count()<=8),"Lịch");
        assert_eq!(fit_caption("Tài liệu của nhóm · Đà Nẵng",|t|t.chars().count()<=8),"…Đà Nẵng");
        assert_eq!(fit_caption("Ảnh",|_|false),"");
    }

    #[test]
    fn floating_layout_reserves_click_through_gaps_and_paging() {
        for dpi in [96,120,144,192] {
            let scale=dpi as i32/24;
            let mut l=Layout::new(40*scale,136*scale,dpi,2*scale,5,8,false);
            assert!(l.paged);
            l.count=l.rows;
            l.stack=1;
            let c=l.card(0);
            let b=l.card(1);
            let mut opposite=l;opposite.right=true;
            assert!(opposite.card(0).x>c.x);
            assert!(c.w>0&&c.h>0&&b.y>c.y+c.h);
            assert_eq!(l.hit(c.x,c.y+c.h/2),Some(0));
            assert_eq!(l.hit(c.x,c.y+c.h+1),None);
            assert_eq!(l.hit(c.x,c.y-1),None);
            let nav=l.nav().unwrap();
            assert!(nav.y+nav.h<=l.height);
            assert_eq!(l.hit(nav.x+nav.w/2,nav.y+nav.h/2),None);
        }
    }
    #[test]
    fn stacked_targets_hit_each_exposed_layer_on_both_sides_and_multiple_dpi(){
        for dpi in [96,120,144]{for right in [false,true]{for count in 1..=5{
            let mut l=Layout::new(160*dpi as i32/96,600,dpi,8,5,1,right);
            l.count=1;l.stack_design=true;l.layers[0]=count;
            let front=l.card(0);
            let icon=l.detached_icon(0);let (x,y)=icon.center();
            assert!(l.icon_hit(0,x,y));assert_eq!(l.member_hit(0,x,y),Some(0));
            let (x,y)=l.close_button(0).unwrap().center();
            assert!(l.close_hit(0,x,y));assert_eq!(l.member_hit(0,x,y),Some(0));
            for layer in 1..count as usize{
                let (slot,clip)=l.preview_layer(0,layer);
                let clip=clip.unwrap();
                assert!(clip.right>clip.left&&slot.right>slot.left);
                let x=(clip.left+clip.right)/2;let y=front.y+front.h/2;
                assert_eq!(l.member_hit(0,x,y),Some(layer),"side={right}, layer={layer}");
            }
        }}}
    }
    #[test]
    fn pager_uses_the_same_cells_for_painted_dots_and_page_selection() {
        for dpi in [96,144,192] {
            let mut layout=Layout::new(200*dpi as i32/96, 480*dpi as i32/96,dpi,12,3,13,false);
            layout.count=3;
            let strip=layout.pages(5).unwrap();
            assert!(strip.uses_dots());
            for i in 0..5 {
                let (x,y)=strip.dot(i).center();
                assert_eq!(strip.target(x,y,0),(i!=0).then_some(i));
            }
            let (x,y)=strip.previous.center();
            assert_eq!(strip.target(x,y,0),None);
            assert_eq!(strip.target(x,y,3),Some(2));
            let (x,y)=strip.next.center();
            assert_eq!(strip.target(x,y,4),None);
            let (x,y)=layout.card(0).center();
            assert_eq!(strip.target(x,y,0),None);
            assert_eq!(layout.hit(x,y),Some(0));
            let card=layout.card(0);
            assert_eq!(layout.hit(card.x,card.y),None);
        }
    }
    #[test]
    #[ignore = "native Win32 paired overlay positions; explicitly run in CI"]
    fn native_slide_keeps_live_preview_host_aligned_without_activating_either_window() {
        use std::{mem::zeroed,ptr::null_mut};
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        unsafe {
            let make=|owner|CreateWindowExW(WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST,wide("STATIC").as_ptr(),
                null_mut(),WS_POPUP,14,20,155,250,owner,null_mut(),null_mut(),null_mut());
            let stage=make(null_mut());
            assert!(!stage.is_null());
            let preview=make(stage);
            assert!(!preview.is_null());
            let foreground=GetForegroundWindow();
            let rect=RECT{left:36,top:94,right:196,bottom:420};
            for offset in [-6,-2,0] {
                super::super::position_stage(stage,preview,rect,offset);
                for window in [stage,preview] {
                    let mut r:RECT=zeroed();
                    assert_ne!(GetWindowRect(window,&mut r),0);
                    assert_eq!((r.left,r.top,r.right,r.bottom),(rect.left+offset,rect.top,rect.right+offset,rect.bottom));
                    assert_eq!(IsWindowVisible(window),0);
                }
                assert_eq!(GetWindow(preview,GW_OWNER),stage);
            }
            assert_eq!(GetForegroundWindow(),foreground);
            DestroyWindow(preview);DestroyWindow(stage);
        }
    }
    #[test]
    #[ignore = "native Win32 window region; explicitly run in CI"]
    fn native_region_clips_desktop_gaps_and_preserves_thumbnail_slots() {
        use std::ptr::null_mut;
        use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW,DestroyWindow,WS_POPUP};
        unsafe {
            let hwnd=CreateWindowExW(0,wide("STATIC").as_ptr(),null_mut(),WS_POPUP,0,0,164,400,
                null_mut(),null_mut(),null_mut(),null_mut());
            assert!(!hwnd.is_null());
            let mut l=Layout::new(164,400,96,10,3,8,false);
            l.count=3;l.stack=0b010;
            assert!(l.apply(hwnd));
            let rgn=CreateRectRgn(0,0,0,0);
            assert_ne!(GetWindowRgn(hwnd,rgn),0);
            for i in 0..l.count{
                let r=l.preview(i);
                assert_ne!(PtInRegion(rgn,r.left+1,r.top+1),0);
                assert_ne!(PtInRegion(rgn,r.right-2,r.bottom-2),0);
                let b=l.card(i);
                assert_eq!(PtInRegion(rgn,b.x+b.w/2,b.y+b.h+2),0);
            }
            assert_eq!(PtInRegion(rgn,0,0),0);
            let nav=l.nav().unwrap();
            assert_ne!(PtInRegion(rgn,nav.x+nav.w/2,nav.y+nav.h/2),0);
            DeleteObject(rgn);
            DestroyWindow(hwnd);
        }
    }
    #[test]
    #[ignore = "native layered Win32 presentation; explicitly run in CI"]
    fn native_stage_surface_submits_premultiplied_cards_with_desktop_gaps() {
        use std::ptr::null_mut;
        use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW,DestroyWindow,WS_EX_LAYERED,WS_POPUP};
        unsafe {
            let w=164;let h=400;
            let hwnd=CreateWindowExW(WS_EX_LAYERED,wide("STATIC").as_ptr(),null_mut(),WS_POPUP,0,0,w,h,
                null_mut(),null_mut(),null_mut(),null_mut());
            assert!(!hwnd.is_null());
            let mut l=Layout::new(w,h,96,16,3,8,false);
            l.count=3;l.stack=0b010;
            let items=(0..3).map(|_|Item{path:String::new(),title:"Fixture".into(),hwnd:null_mut(),count:1,pinned:false}).collect::<Vec<_>>();
            let labels=vec!["Fixture".to_owned();3];
            let mut panel=Panel::new(w,h,96).unwrap();
            let layer=Look{dark:true,layout:l,active:1,alpha:Some(155)};
            let presented=panel.paint(null_mut(),layer,&items,&Atlas::new(),&[false,true,false],Some((0,200)),Some((hwnd,&labels,0,3,None)));
            DestroyWindow(hwnd);
            assert!(presented,"the isolated layered Stage surface must submit pixels successfully");
            let pixels=panel.layer.pixels();
            assert!(pixels.chunks_exact(4).all(|p|p[0]<=p[3]&&p[1]<=p[3]&&p[2]<=p[3]));
            let alpha=|x,y|pixels[((y*w+x)*4+3) as usize];
            let c=l.card(0);
            assert_eq!(alpha(0,0),0);
            assert_eq!(alpha(c.x+c.w/2,c.y+c.h+9),0);
            assert!((155..255).contains(&alpha(c.x+c.w/2,c.y+c.h/2)));
        }
    }
}
