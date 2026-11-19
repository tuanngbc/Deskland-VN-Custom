//! Small static skin mesh, deformed on the GPU. No runtime asset downloads.
use crate::engine::Prog;
use crate::whales::Motion;
use glow::HasContext;
use std::f32::consts::PI;

pub const VERT:&str=include_str!("../assets/shaders/whale.vert");
pub const FRAG:&str=include_str!("../assets/shaders/whale.frag");
pub const WATER_VERT:&str=include_str!("../assets/shaders/whale-water.vert");
pub const WATER_FRAG:&str=include_str!("../assets/shaders/whale-water.frag");
const STRIDE:usize=8; // position xyz, normal xyz, part, side

#[derive(Clone,Copy)]
struct Vertex { p:[f32;3],n:[f32;3],part:f32,side:f32 }
fn normal(v:[f32;3])->[f32;3] { let d=(v[0]*v[0]+v[1]*v[1]+v[2]*v[2]).sqrt().max(1e-6); v.map(|x|x/d) }
fn triangle(out:&mut Vec<f32>,a:Vertex,mut b:Vertex,mut c:Vertex) {
    let u=[b.p[0]-a.p[0],b.p[1]-a.p[1],b.p[2]-a.p[2]];
    let v=[c.p[0]-a.p[0],c.p[1]-a.p[1],c.p[2]-a.p[2]];
    let cross=[u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]];
    let dot=(0..3).map(|i|cross[i]*(a.n[i]+b.n[i]+c.n[i])).sum::<f32>();
    if dot<0.0 {std::mem::swap(&mut b,&mut c);}
    for v in [a,b,c] {out.extend(v.p);out.extend(v.n);out.extend([v.part,v.side]);}
}
fn surface(out:&mut Vec<f32>,rows:usize,cols:usize,point:impl Fn(usize,usize)->Vertex) {
    for i in 0..rows {for j in 0..cols {
        let (a,b,c,d)=(point(i,j),point(i+1,j),point(i+1,j+1),point(i,j+1));
        triangle(out,a,b,c);triangle(out,a,c,d);
    }}
}
fn profile(x:f32)->(f32,f32) {
    const KEYS:[[f32;3];10]=[[-73.0,0.8,0.7],[-61.0,3.0,2.2],[-46.0,5.7,4.0],[-25.0,12.4,7.7],
        [0.0,17.7,10.4],[22.0,16.6,9.8],[40.0,12.8,7.4],[53.0,8.6,4.8],[61.0,3.8,2.3],[65.0,0.25,0.4]];
    let i=KEYS.windows(2).position(|k|x<=k[1][0]).unwrap_or(KEYS.len()-2);
    let u=((x-KEYS[i][0])/(KEYS[i+1][0]-KEYS[i][0])).clamp(0.0,1.0);
    let blend=|axis:usize|{
        let a=KEYS[i.saturating_sub(1)][axis];let b=KEYS[i][axis];
        let c=KEYS[i+1][axis];let d=KEYS[(i+2).min(KEYS.len()-1)][axis];
        (0.5*((2.0*b)+(-a+c)*u+(2.0*a-5.0*b+4.0*c-d)*u*u+(-a+3.0*b-3.0*c+d)*u*u*u)).max(0.2)
    };
    (blend(1),blend(2))
}

pub fn mesh()->Vec<f32> {
    let mut out=Vec::with_capacity(32_000);
    // Fins are drawn below the body; winding/culling selects the visible skin.
    for side in [-1.0f32,1.0] {
        for part in [1.0f32,2.0] {
            surface(&mut out,14,8,|i,j|{
                let t=i as f32/14.0;let a=j as f32/8.0*2.0*PI;
                let (x,y,chord,z)=if part==1.0 {
                    let x=(1.0-t).powi(2)*13.0+2.0*(1.0-t)*t*1.0+t*t*(-25.0);
                    let y=(1.0-t).powi(2)*13.0+2.0*(1.0-t)*t*39.0+t*t*52.0;
                    (x,y,3.6*(1.0-t)+4.0*(PI*t).sin().max(0.0).sqrt()+0.2,-3.0-t*1.5)
                } else {
                    (-70.0-18.0*t+8.0*(PI*t).sin(),32.0*t,
                     2.8*(1.0-t)+6.5*(PI*t).sin().max(0.0).sqrt()+0.2,0.0)
                };
                Vertex{p:[x+a.cos()*chord,y*side,z+a.sin()*0.65],
                    n:normal([a.cos()*0.65/chord,side*0.06,a.sin()]),part,side}
            });
        }
    }
    surface(&mut out,42,16,|i,j|{
        let x=-73.0+i as f32/42.0*138.0;let a=j as f32/16.0*2.0*PI;
        let (w,h)=profile(x);let (w0,h0)=profile(x-0.2);let (w1,h1)=profile(x+0.2);
        let dx=-(a.cos().powi(2)*(w1-w0)/w+a.sin().powi(2)*(h1-h0)/h)/0.4;
        Vertex{p:[x,a.cos()*w,a.sin()*h],n:normal([dx,a.cos()/w,a.sin()/h]),part:0.0,side:0.0}
    });
    // A small swept dorsal fin rooted in the curved back.
    let p=[[-29.0,0.0,8.0],[-17.0,0.0,10.0],[-25.0,0.0,17.0],[-31.0,0.0,11.0]];
    for side in [-1.0f32,1.0] {
        let v=|i:usize|Vertex{p:[p[i][0],side*0.5,p[i][2]],n:[0.0,side,0.1],part:3.0,side};
        triangle(&mut out,v(0),v(1),v(2));triangle(&mut out,v(0),v(2),v(3));
    }
    out
}

pub struct Renderer { skin:Prog,water:Prog,vao:glow::VertexArray,vbo:glow::Buffer,count:i32 }
impl Renderer {
    pub fn new(gl:&glow::Context)->Result<Self,String> {
        let skin=Prog::from_parts(gl,VERT,FRAG)?;
        let water=match Prog::from_parts(gl,WATER_VERT,WATER_FRAG) {
            Ok(p)=>p,Err(e)=>{unsafe{gl.delete_program(skin.prog)};return Err(e)}
        };
        let values=mesh();
        let mut bytes=Vec::with_capacity(values.len()*4);
        for v in &values {bytes.extend(v.to_ne_bytes());}
        unsafe {
            let previous=gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING);
            let vao=gl.create_vertex_array()?;let vbo=gl.create_buffer()?;
            gl.bind_vertex_array(Some(vao));gl.bind_buffer(glow::ARRAY_BUFFER,Some(vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER,&bytes,glow::STATIC_DRAW);
            for (index,size,offset) in [(0,3,0),(1,3,12),(2,2,24)] {
                gl.enable_vertex_attrib_array(index);
                gl.vertex_attrib_pointer_f32(index,size,glow::FLOAT,false,(STRIDE*4) as i32,offset);
            }
            // GLuint handles from the same current context; preserve the theme's
            // empty VAO because the remaining shaders use gl_VertexID.
            let old=std::num::NonZeroU32::new(previous as u32).map(glow::NativeVertexArray);
            gl.bind_vertex_array(old);gl.bind_buffer(glow::ARRAY_BUFFER,None);
            Ok(Self{skin,water,vao,vbo,count:(values.len()/STRIDE) as i32})
        }
    }

    pub fn draw(&mut self,gl:&glow::Context,res:[f32;2],whale:[f32;4],motion:Motion,water:f32) {
        if water<65.0 || motion.depth < -92.0 {return;}
        let (pose,stroke,fx)=motion.uniforms();
        unsafe {
            let previous=gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING);
            gl.enable(glow::BLEND);gl.blend_func(glow::ONE,glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(self.water.prog));
            self.water.f2(gl,"uRes",res[0],res[1]);self.water.f4v(gl,"uWhale",&whale);
            self.water.f4v(gl,"uPose",&pose);self.water.f4v(gl,"uEffect",&fx);
            gl.draw_arrays(glow::TRIANGLE_STRIP,0,4);
            gl.bind_vertex_array(Some(self.vao));gl.use_program(Some(self.skin.prog));
            self.skin.f2(gl,"uRes",res[0],res[1]);self.skin.f4v(gl,"uWhale",&whale);
            self.skin.f4v(gl,"uPose",&pose);self.skin.f4v(gl,"uStroke",&stroke);
            gl.enable(glow::CULL_FACE);gl.front_face(glow::CW);
            gl.draw_arrays(glow::TRIANGLES,0,self.count);
            gl.front_face(glow::CCW);gl.disable(glow::CULL_FACE);
            let old=std::num::NonZeroU32::new(previous as u32).map(glow::NativeVertexArray);
            gl.bind_vertex_array(old);gl.disable(glow::BLEND);gl.use_program(None);
        }
    }

    pub fn destroy(self,gl:&glow::Context) {unsafe {
        gl.delete_program(self.skin.prog);gl.delete_program(self.water.prog);
        gl.delete_buffer(self.vbo);gl.delete_vertex_array(self.vao);
    }}
}

/// CI exports the actual native mesh and simulation frames for a browser capture.
/// It needs neither a desktop session nor an installed copy on the user's PC.
pub fn export_preview(path:&std::path::Path)->Result<(),String> {
    use crate::engine::{Field,Noise,IconBox,icon_rects};
    let (width,height)=(1280.0,800.0);
    let noise=Noise{seed:71};let field=Field::new(width,height,&noise);
    let icons:Vec<_>=(0..12).map(|i|IconBox{x:20.0+(i%4) as f32*86.0,y:10.0+(i/4) as f32*90.0,w:72.0,h:82.0}).collect();
    let values=field.compute(&icon_rects(&icons),85.0,34.0);
    let mut pod=crate::whales::Pod::new(3,0x48554d50);
    pod.populate(&[[0.0,0.0,width,height]],|x,y|field.sample(&values,x,y));
    if pod.whales.len()!=3 {return Err("Preview needs three visible offshore whales".into());}
    let fps=24;
    let mut frames=Vec::with_capacity(36*fps+1);
    for _ in 0..=36*fps {
        frames.push(pod.whales.iter().map(|whale|{
            let (pose,stroke,fx)=whale.motion().uniforms();
            let mut row=vec![whale.x,whale.y,whale.heading,whale.scale];
            row.extend(pose);row.extend(stroke);row.extend(fx);row
        }).collect::<Vec<_>>());
        pod.step(1.0/fps as f32,|x,y|field.sample(&values,x,y));
    }
    let detail:Vec<_>=(0..=36*fps).map(|frame|{
        let pose=Motion::at(frame as f32/fps as f32);
        let (p,s,f)=pose.uniforms();let mut row=vec![width*0.5,height*0.56,-0.75,2.5];
        row.extend(p);row.extend(s);row.extend(f);row
    }).collect();
    let (noise_size,noise_rgba)=noise.make_noise_image();
    let data=serde_json::json!({"width":width,"height":height,"fps":fps,"duration":36,
        "mesh":mesh(),"frames":frames,"detail":detail,"noiseSize":noise_size,"noise":noise_rgba,
        "fieldWidth":field.gw,"fieldHeight":field.gh,"field":values,
        "skinVertex":VERT,"skinFragment":FRAG,"waterVertex":WATER_VERT,"waterFragment":WATER_FRAG,
        "beachFragment":crate::themes::beach::FRAG});
    let bytes=serde_json::to_vec(&data).map_err(|e|e.to_string())?;
    std::fs::write(path,bytes).map_err(|e|e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mesh_is_small_finite_and_has_outward_winding() {
        let data=mesh();assert!(data.len()/STRIDE<7000);assert_eq!(data.len()%(STRIDE*3),0);
        assert!(data.iter().all(|n|n.is_finite()));
        for v in data.chunks_exact(STRIDE) {
            let n=v[3..6].iter().map(|n|n*n).sum::<f32>().sqrt();assert!(n>0.95&&n<1.05);
            assert!(v[0].abs()<100.0&&v[1].abs()<60.0&&v[2].abs()<20.0);
        }
        for tri in data.chunks_exact(STRIDE*3) {
            let a=&tri[..STRIDE];let b=&tri[STRIDE..STRIDE*2];let c=&tri[STRIDE*2..];
            let (u,v)=([b[0]-a[0],b[1]-a[1],b[2]-a[2]],[c[0]-a[0],c[1]-a[1],c[2]-a[2]]);
            let cross=[u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]];
            assert!((0..3).map(|i|cross[i]*(a[i+3]+b[i+3]+c[i+3])).sum::<f32>()>=-0.001);
        }
    }
}
