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
        let (w,h)=(r.w as f32,r.h as f32);
        let radius=9.0_f32.min(h/4.0).min(w/4.0);
        let qx=(u*w-w/2.0).abs()-(w/2.0-radius);
        let qy=(v*h-h/2.0).abs()-(h/2.0-radius);
        qx.max(0.0).hypot(qy.max(0.0))+qx.max(qy).min(0.0)<=radius
    }
    pub fn paint(self,pixels:&mut [u8],w:i32,h:i32,src:&[u8],sw:i32,sh:i32){
        draw(pixels,w,h,self.rect.rect(),|x,y|{
            let passed=[(-0.27,-0.27),(0.27,-0.27),(-0.27,0.27),(0.27,0.27)]
                .into_iter().filter(|&(dx,dy)|self.contains(x+dx,y+dy)).count();
            if passed==0{return None;}
            let (u,v)=self.uv(x,y);
            // Preserve the source aspect inside the canonical unwarped UV
            // card, including portrait windows. Empty space is letterboxed.
            let source=sw as f32/sh as f32;
            let slot=self.rect.w as f32/self.rect.h as f32;
            // Inset the real pixels from the frame and preserve the entire
            // source. A portrait source uses almost all available height.
            let fit_v=(slot/source).min(0.94);
            let fit_u=(source/slot*fit_v).min(0.96);
            let fit_v=fit_u*slot/source;
            let (u,v)=(0.5+(u-0.5)/fit_u,0.5+(v-0.5)/fit_v);
            let opacity=255.0*passed as f32/4.0;
            if !(0.0..=1.0).contains(&u)||!(0.0..=1.0).contains(&v){return Some(([48,43,39],opacity));}
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
            Some((tone,opacity))
        });
    }
}
