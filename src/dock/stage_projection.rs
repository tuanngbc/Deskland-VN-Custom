//! Shared pinhole stage projection for consented snapshots and fixtures.
use super::{stage_surface::Box2,surface::draw};

/// Box prefilter with fractional pixel weights. The caller keeps the same
/// frame and projection cache limits.
pub(super) fn downsample(src:&[u8],sw:usize,sh:usize,stride:usize,dw:usize,dh:usize)->Vec<u8>{
    debug_assert!(stride>=sw*4);
    downsample_rows(sw,sh,dw,dh,|y|&src[y*stride..y*stride+sw*4])
}
pub(super) fn downsample_rows<'a>(sw:usize,sh:usize,dw:usize,dh:usize,row_at:impl Fn(usize)->&'a [u8])->Vec<u8>{
    debug_assert!(dw>0&&dh>0&&dw<=sw&&dh<=sh);
    if sw==dw&&sh==dh{
        return (0..sh).flat_map(|y|row_at(y).iter().copied()).collect();
    }
    let bands=|source:usize,dest:usize|(0..dest).map(|i|{
        let (start,end)=(i*source,(i+1)*source);
        (start/dest..end.div_ceil(dest)).map(|s|
            (s,(end.min((s+1)*dest)-start.max(s*dest)) as u32)
        ).collect::<Vec<_>>()
    }).collect::<Vec<_>>();
    let xs=bands(sw,dw);
    let ys=bands(sh,dh);
    let mut result=vec![0u8;dw*dh*4];
    let denominator=(sw*sh) as u32;
    for (y,rows) in ys.iter().enumerate(){
        for (x,cols) in xs.iter().enumerate(){
            let mut sum=[0u32;4];
            for &(sy,wy) in rows{
                let row=row_at(sy);
                for &(sx,wx) in cols{
                    let i=sx*4;
                    let weight=wx*wy;
                    for c in 0..4{sum[c]+=row[i+c] as u32*weight;}
                }
            }
            let at=(y*dw+x)*4;
            for c in 0..4{result[at+c]=((sum[c]+denominator/2)/denominator) as u8;}
        }
    }
    result
}

#[derive(Clone,Copy)]
pub(super) struct Card{pub rect:Box2,pub right:bool,pub depth:f32}

// Area of a unit pixel cut by the tangent of the projected edge. Continuous
// coverage avoids the visible alpha steps of a fixed subpixel sample grid.
fn pixel_coverage(distance:f32,dx:f32,dy:f32)->f32{
    let a=dx.abs().max(dy.abs());
    let b=dx.abs().min(dy.abs());
    if a<0.001{return (distance<=0.0) as u8 as f32;}
    let t=(a+b)*0.5-distance;
    if t<=0.0{0.0}
    else if t>=a+b{1.0}
    else if b<0.001{(t/a).clamp(0.0,1.0)}
    else if t<b{t*t/(2.0*a*b)}
    else if t>a{let tail=a+b-t;1.0-tail*tail/(2.0*a*b)}
    else{(t-b*0.5)/a}
}
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
        if qx>0.0&&qy>0.0{qx.hypot(qy)-radius}else{qx.max(qy)-radius}
    }
    fn local_gradient(self,x:f32,y:f32)->(f32,f32){
        let d=|x,y|{let (u,v)=self.uv(x,y);self.edge_distance(u,v)};
        (d(x+0.5,y)-d(x-0.5,y),d(x,y+0.5)-d(x,y-0.5))
    }
    fn coverage(self,x:f32,y:f32,distance:f32)->f32{
        // The inverse transform has a bounded local scale at the supported
        // yaw. Only pixels near the silhouette need the local filter.
        let band=0.85*(1.0+self.depth)*(1.0+self.depth*self.rect.h as f32/self.rect.w as f32);
        if distance<=-band{return 1.0;}
        if distance>=band{return 0.0;}
        let (dx,dy)=self.local_gradient(x,y);
        pixel_coverage(distance,1.18*dx,1.18*dy)
    }
    pub(super) fn highlight(self,pixels:&mut[u8],w:i32,h:i32,ink:[u8;3],width:f32,glass:Option<&super::glass::Glass>){
        use super::geometry::Rect;
        let r=self.rect.rect();
        let width=width.max(1.0);
        let band=0.85*(1.0+self.depth)*(1.0+self.depth*r.h/r.w);
        draw(pixels,w,h,Rect{x:r.x-1.0,y:r.y-1.0,w:r.w+2.0,h:r.h+2.0},|x,y|{
            let (u,v)=self.uv(x,y);
            let d=self.edge_distance(u,v);
            if d>=band{return None;}
            if d<=-width-band{
                let glass=glass.filter(|_|d>-(width+band+3.0))?;
                return Some((glass.tint(x,y,ink,230),125.0*(1.0+(d+width+band)/3.0)));
            }
            // Use the very same area filter on both sides of the thin line.
            let (dx,dy)=self.local_gradient(x,y);
            let outer=pixel_coverage(d,1.18*dx,1.18*dy);
            let inner=pixel_coverage(d+width,1.18*dx,1.18*dy);
            Some((ink,(outer-inner).max(0.0)*168.0))
        });
    }
    pub(super) fn shadow(self,pixels:&mut[u8],w:i32,h:i32,layer:usize){
        self.shadow_with_filter(pixels,w,h,layer,true);
    }
    pub(super) fn shadow_with_filter(self,pixels:&mut[u8],w:i32,h:i32,layer:usize,filtered:bool){
        use super::geometry::Rect;
        let r=self.rect.rect();
        let k=(r.h*0.067).clamp(3.0,10.0);
        draw(pixels,w,h,Rect{x:r.x-k,y:r.y-k,w:r.w+2.0*k,h:r.h+2.0*k},|x,y|{
            let (u,v)=self.uv(x,y-k*0.55);
            let dist=self.edge_distance(u,v);
            let amount=(1.0-dist.max(0.0)/k).clamp(0.0,1.0);
            let opacity=if filtered{amount*amount*(3.0-2.0*amount)}else{amount*amount};
            Some(([7,8,11],(77.0-layer as f32*6.0)*opacity))
        });
    }
    pub fn paint(self,pixels:&mut [u8],w:i32,h:i32,src:&[u8],sw:i32,sh:i32){
        self.paint_depth(pixels,w,h,src,sw,sh,0);
    }
    pub(super) fn paint_depth(self,pixels:&mut[u8],w:i32,h:i32,src:&[u8],sw:i32,sh:i32,layer:usize){
        self.paint_with_filter(pixels,w,h,src,sw,sh,layer,true);
    }
    pub(super) fn paint_with_filter(self,pixels:&mut[u8],w:i32,h:i32,src:&[u8],sw:i32,sh:i32,layer:usize,filtered:bool){
        if sw<=0||sh<=0||src.len()<(sw as usize*sh as usize*4){return;}
        let source=sw as f32/sh as f32;
        let slot=self.rect.w as f32/self.rect.h as f32;
        let tall=source<=1.12;
        let portrait=source<0.92;
        let fit_v=(slot/source).min(if filtered&&portrait{0.995}else if tall{0.96}else{0.94});
        let fit_u=(source/slot*fit_v).min(if filtered&&portrait{0.985}else{0.96});
        let fit_v=fit_u*slot/source;
        let ambient=|side:usize|std::array::from_fn::<u8,3,_>(|channel|{
            // Derive the matte from the real window's edge colors, not
            // duplicated/blurred content. Preserve the complete middle image.
            let x=if side==0{sw as usize/8}else{sw as usize*7/8};
            let value=(1..=5).map(|t|{
                let y=(t*sh as usize/6).min(sh as usize-1);
                u32::from(src[(y*sw as usize+x)*4+channel])
            }).sum::<u32>()/5;
            if filtered&&portrait{(18+value*22/100) as u8}else{(31+value*29/100).min(140) as u8}
        });
        let (matte_left,matte_right)=(ambient(0),ambient(1));
        // Prepare one bounded reduction per projected face, never in the
        // animation compositor. Preserve slightly more detail than the
        // minimum pixel footprint at the farther edge.
        let level=|source:i32,pixels:f32|{
            let wanted=(pixels*(1.35/(1.0+self.depth))).ceil() as i32;
            if filtered&&source>wanted*3/2{wanted.clamp(1,source)}else{source}
        };
        let (mw,mh)=(level(sw,self.rect.w as f32*fit_u),level(sh,self.rect.h as f32*fit_v));
        let reduced=(mw!=sw||mh!=sh).then(||downsample(src,sw as usize,sh as usize,sw as usize*4,mw as usize,mh as usize));
        let src=reduced.as_deref().unwrap_or(src);
        let (sw,sh)=(mw,mh);
        draw(pixels,w,h,self.rect.rect(),|x,y|{
            let (u,v)=self.uv(x,y);
            let dist=self.edge_distance(u,v);
            let coverage=if filtered{self.coverage(x,y,dist)}else{
                [(-0.27,-0.27),(0.27,-0.27),(-0.27,0.27),(0.27,0.27)]
                    .into_iter().filter(|&(dx,dy)|self.contains(x+dx,y+dy)).count() as f32/4.0
            };
            if coverage<=0.0{return None;}
            let far=if self.right{1.0-u}else{u}.clamp(0.0,1.0);
            let shade=(1.02-0.12*far-0.055*layer as f32).clamp(0.61,1.0);
            let center=if filtered&&portrait{0.50}else if tall{if self.right{0.48}else{0.52}}else{0.50};
            let (su,sv)=(0.5+(u-center)/fit_u,0.5+(v-0.5)/fit_v);
            let opacity=255.0*coverage;
            let matte=|c:usize|{
                let mix=u.clamp(0.0,1.0);
                let color=matte_left[c] as f32*(1.0-mix)+matte_right[c] as f32*mix;
                let rim=if filtered{(1.2+dist).clamp(0.0,1.0)*12.0}else{(1.6+dist).clamp(0.0,1.0)*24.0};
                color+rim
            };
            let content=if filtered{
                let (uw,vh)=(fit_u*self.rect.w as f32,fit_v*self.rect.h as f32);
                let edge=((su-0.5).abs()-0.5)*uw;
                let cap=((sv-0.5).abs()-0.5)*vh;
                (0.5-edge.max(cap)/(1.0+self.depth)).clamp(0.0,1.0)
            }else if (0.0..=1.0).contains(&su)&&(0.0..=1.0).contains(&sv){1.0}else{0.0};
            if content<=0.0{return Some((std::array::from_fn(|c|(matte(c)*shade).round() as u8),opacity));}
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
                let sample=t*(1.0-b)+end*b;
                let fill=if content>=1.0{sample}else{matte(k)*(1.0-content)+sample*content};
                (fill*shade).round() as u8
            });
            Some((tone,opacity))
        });
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn diagonal_coverage_changes_continuously_with_subpixel_motion(){
        let samples=(0..16).map(|x|pixel_coverage((x as f32-7.5)/24.0,0.7,1.0)).collect::<Vec<_>>();
        assert!(samples.windows(2).all(|w|w[0]>w[1]&&w[0]-w[1]<0.06));
        assert!((samples[0]+samples[15]-1.0).abs()<0.0001);
        assert_eq!(pixel_coverage(0.0,1.0,0.0),0.5);
    }
    #[test]
    fn fractional_box_preserves_thin_strokes_and_respects_stride(){
        let row=[0u8,0,0,255,255,255,255,255,0,0,0,255,99,99,99,99];
        let actual=downsample(&row,3,1,16,2,1);
        assert_eq!(actual,[85,85,85,255,85,85,85,255]);
        assert_eq!(downsample(&row,3,1,16,1,1),[85,85,85,255]);
        assert_eq!(downsample(&row,3,1,16,3,1),row[..12]);
    }
}
