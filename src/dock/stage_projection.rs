//! Shared pinhole stage projection for consented snapshots and fixtures.
use super::{stage_surface::Box2,surface::draw};
#[derive(Clone,Copy)]
pub(super) struct Card{pub rect:Box2,pub right:bool,pub depth:f32}
impl Card{
    /// Normalized pinhole projection. Translate the near corner to the
    /// start of the slot; keep the midpoint of the vertical edges fixed.
    pub fn new(rect:Box2,right:bool,yaw:f32)->Self{
        let half_depth=yaw.to_radians().sin().abs()*0.5;
        let camera=1.75;
        Self{rect,right,depth:2.0*half_depth/(camera-half_depth)}
    }
    #[cfg(test)]
    pub fn polygon(self)->[(f32,f32);4]{
        let r=self.rect;
        let (x,y,w,h)=(r.x as f32,r.y as f32,r.w as f32,r.h as f32);
        let pad=h*0.5*(1.0-1.0/(1.0+self.depth));
        if self.right{[(x,y+pad),(x+w,y),(x+w,y+h),(x,y+h-pad)]}
        else{[(x,y),(x+w,y+pad),(x+w,y+h-pad),(x,y+h)]}
    }
    pub fn uv(self,x:f32,y:f32)->(f32,f32){
        let mut t=(x-self.rect.x as f32)/self.rect.w as f32;
        if self.right{t=1.0-t;}
        // Inverse of the homogeneous map:
        // t = (1+k)u/(1+ku), y = .5+(v-.5)/(1+ku).
        let u=t/(1.0+self.depth-self.depth*t);
        let v=0.5+((y-self.rect.y as f32)/self.rect.h as f32-0.5)*(1.0+self.depth*u);
        (if self.right{1.0-u}else{u},v)
    }
    pub fn contains(self,x:f32,y:f32)->bool{
        let r=self.rect;
        if x<r.x as f32||x>(r.x+r.w) as f32||y<r.y as f32||y>(r.y+r.h) as f32{return false;}
        let (u,v)=self.uv(x,y);
        self.edge_distance(u,v)<=0.0
    }
    fn edge_distance(self,u:f32,v:f32)->f32{
        let (w,h)=(self.rect.w as f32,self.rect.h as f32);
        let radius=(0.087*h).clamp(2.0,16.0).min(h/4.0).min(w/4.0);
        let qx=(u*w-w/2.0).abs()-(w/2.0-radius);
        let qy=(v*h-h/2.0).abs()-(h/2.0-radius);
        qx.max(0.0).hypot(qy.max(0.0))+qx.max(qy).min(0.0)-radius
    }
    pub(super) fn shadow(self,pixels:&mut[u8],w:i32,h:i32,layer:usize){
        use super::geometry::Rect;
        let r=self.rect.rect();
        let k=(r.h*0.067).clamp(3.0,10.0);
        draw(pixels,w,h,Rect{x:r.x-k,y:r.y-k,w:r.w+2.0*k,h:r.h+2.0*k},|x,y|{
            let (u,v)=self.uv(x,y-k*0.55);
            let dist=self.edge_distance(u,v);
            Some(([7,8,11],(77.0-layer as f32*6.0)*(1.0-dist.max(0.0)/k).clamp(0.0,1.0).powi(2)))
        });
    }
    pub fn paint(self,pixels:&mut [u8],w:i32,h:i32,src:&[u8],sw:i32,sh:i32){
        self.paint_depth(pixels,w,h,src,sw,sh,0);
    }
    pub(super) fn paint_depth(self,pixels:&mut[u8],w:i32,h:i32,src:&[u8],sw:i32,sh:i32,layer:usize){
        let source=sw as f32/sh as f32;
        let slot=self.rect.w as f32/self.rect.h as f32;
        let tall=source<=1.12;
        let fit_v=(slot/source).min(if tall{0.96}else{0.94});
        let fit_u=(source/slot*fit_v).min(0.96);
        let fit_v=fit_u*slot/source;
        let ambient=|side:usize|std::array::from_fn::<u8,3,_>(|channel|{
            // Derive the matte from the real window's edge colors, not
            // duplicated/blurred content. Preserve the complete middle image.
            let x=if side==0{sw as usize/8}else{sw as usize*7/8};
            let value=(1..=5).map(|t|{
                let y=(t*sh as usize/6).min(sh as usize-1);
                u32::from(src[(y*sw as usize+x)*4+channel])
            }).sum::<u32>()/5;
            (31+value*29/100).min(140) as u8
        });
        let (matte_left,matte_right)=(ambient(0),ambient(1));
        draw(pixels,w,h,self.rect.rect(),|x,y|{
            let passed=[(-0.27,-0.27),(0.27,-0.27),(-0.27,0.27),(0.27,0.27)]
                .into_iter().filter(|&(dx,dy)|self.contains(x+dx,y+dy)).count();
            if passed==0{return None;}
            let (u,v)=self.uv(x,y);
            let far=if self.right{1.0-u}else{u}.clamp(0.0,1.0);
            let shade=(1.02-0.12*far-0.055*layer as f32).clamp(0.61,1.0);
            let center=if tall{if self.right{0.48}else{0.52}}else{0.50};
            let (su,sv)=(0.5+(u-center)/fit_u,0.5+(v-0.5)/fit_v);
            let opacity=255.0*passed as f32/4.0;
            if !(0.0..=1.0).contains(&su)||!(0.0..=1.0).contains(&sv){
                let color=std::array::from_fn(|c|{
                    let matte=matte_left[c] as f32*(1.0-u.clamp(0.0,1.0))+matte_right[c] as f32*u.clamp(0.0,1.0);
                    let rim=(1.6+self.edge_distance(u,v)).clamp(0.0,1.0)*24.0;
                    ((matte+rim)*shade).round() as u8
                });
                return Some((color,opacity));
            }
            // Bilinear sampling after projective inverse maps all content,
            // rather than just tapering or rotating the thumbnail container.
            let fx=su.clamp(0.0,1.0)*(sw-1) as f32;
            let fy=sv.clamp(0.0,1.0)*(sh-1) as f32;
            let (ix,iy)=(fx.floor() as usize,fy.floor() as usize);
            let (jx,jy)=((ix+1).min(sw as usize-1),(iy+1).min(sh as usize-1));
            let (a,b)=(fx.fract(),fy.fract());
            let at=|x,y,k|src[(y*sw as usize+x)*4+k] as f32;
            let tone=std::array::from_fn(|k|{
                let t=at(ix,iy,k)*(1.0-a)+at(jx,iy,k)*a;
                let end=at(ix,jy,k)*(1.0-a)+at(jx,jy,k)*a;
                ((t*(1.0-b)+end*b)*shade).round() as u8
            });
            Some((tone,opacity))
        });
    }
}
