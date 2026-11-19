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
    // Humpback: a robust thorax, broad rounded rostrum and slender peduncle.
    // The anterior head is flattened vertically, never a pointed fish snout.
    const KEYS:[[f32;3];13]=[[-73.0,1.1,0.9],[-64.0,2.3,1.9],[-49.0,4.4,3.5],
        [-31.0,8.2,6.9],[-12.0,13.8,10.6],[8.0,16.5,11.7],[25.0,17.0,10.4],
        [40.0,15.2,7.8],[52.0,13.0,5.2],[60.0,11.5,3.9],[65.0,8.7,3.1],
        [68.0,4.5,2.1],[69.0,0.25,0.4]];
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
    let mut out=Vec::with_capacity(140_000);
    // Fins are drawn below the body; winding/culling selects the visible skin.
    for side in [-1.0f32,1.0] {
        for part in [1.0f32,2.0] {
            surface(&mut out,20,12,|i,j|{
                let t=i as f32/20.0;let a=j as f32/12.0*2.0*PI;
                let (x,y,chord,z)=if part==1.0 {
                    // Root-to-tip length ~30% of total length; long rounded
                    // flippers with a mild sweep instead of hooked sickles.
                    let x=23.0-7.0*t-11.0*t*t;
                    let y=13.5+42.0*t;
                    let bumps=(t*PI*12.0).cos().max(0.0)*0.30*(PI*t).sin();
                    (x,y,(4.0*(1.0-t)+2.6*(PI*t).sin()).max(0.15)+bumps,-3.0-t)
                } else {
                    // Broad paired flukes: concave trailing margin, a central
                    // notch and pointed tips, rather than two leaf shapes.
                    let leading=-66.5-12.5*t.powf(1.15);
                    let trailing=-76.0-12.0*(PI*t).sin().max(0.0).powf(0.65)-3.0*t;
                    ((leading+trailing)*0.5,28.5*t,((leading-trailing)*0.5).max(0.12),0.0)
                };
                Vertex{p:[x+a.cos()*chord,y*side,z+a.sin()*0.65],
                    n:normal([a.cos()*0.65/chord,side*0.06,a.sin()]),part,side}
            });
        }
    }
    surface(&mut out,64,24,|i,j|{
        let x=-73.0+i as f32/64.0*142.0;let a=j as f32/24.0*2.0*PI;
        let (w,h)=profile(x);let (w0,h0)=profile(x-0.2);let (w1,h1)=profile(x+0.2);
        let dx=-(a.cos().powi(2)*(w1-w0)/w+a.sin().powi(2)*(h1-h0)/h)/0.4;
        Vertex{p:[x,a.cos()*w,a.sin()*h],n:normal([dx,a.cos()/w,a.sin()/h]),part:0.0,side:0.0}
    });
    // Low, broad-based dorsal nubbin on the hump, not a tall shark fin.
    let p=[[-36.0,0.0,6.0],[-15.0,0.0,9.8],[-21.0,0.0,11.0],
        [-26.0,0.0,14.3],[-28.0,0.0,14.8],[-28.7,0.0,12.6],[-32.0,0.0,9.0]];
    for side in [-1.0f32,1.0] {
        let v=|i:usize|Vertex{p:[p[i][0],side*0.5,p[i][2]],n:[0.0,side,0.1],part:3.0,side};
        for i in 1..p.len()-1 {triangle(&mut out,v(0),v(i),v(i+1));}
    }
    // Discrete head tubercles identify the species without a noisy skin texture.
    for (x,y) in [(40.0,0.0),(47.0,0.0),(54.0,0.0),(60.0,0.0),
        (39.0,-9.0),(48.0,-7.0),(57.0,-4.5),(39.0,9.0),(48.0,7.0),(57.0,4.5)] {
        let (w,h)=profile(x);let z=h*(1.0-(y/w).powi(2)).max(0.0).sqrt();
        surface(&mut out,4,8,|i,j|{
            let a=i as f32/4.0*PI;let b=j as f32/8.0*2.0*PI;
            let n=[a.sin()*b.cos(),a.sin()*b.sin(),a.cos()];
            Vertex{p:[x+n[0]*0.85,y+n[1]*0.85,z+n[2]*0.65],n,part:0.0,side:0.0}
        });
    }
    out
}

fn indexed_mesh()->(Vec<f32>,Vec<u32>) {
    let mut vertices=Vec::new();let mut indices=Vec::new();
    let mut seen=std::collections::HashMap::<[u32;STRIDE],u32>::new();
    for vertex in mesh().chunks_exact(STRIDE) {
        let mut key=[0;STRIDE];for(i,v)in vertex.iter().enumerate(){key[i]=v.to_bits();}
        let index=*seen.entry(key).or_insert_with(||{
            let index=(vertices.len()/STRIDE) as u32;vertices.extend(vertex);index
        });
        indices.push(index);
    }
    (vertices,indices)
}

const AA_SIZE:i32=512;
fn fbo(id:i32)->Option<glow::Framebuffer> {std::num::NonZeroU32::new(id as u32).map(glow::NativeFramebuffer)}
fn texture(id:i32)->Option<glow::Texture> {std::num::NonZeroU32::new(id as u32).map(glow::NativeTexture)}

struct Antialias {
    multisample:glow::Framebuffer,resolve:glow::Framebuffer,
    color:glow::Renderbuffer,image:glow::Texture,composite:Prog,
}
impl Antialias {
    fn new(gl:&glow::Context)->Result<Self,String> {unsafe {
        let composite=Prog::from_parts(gl,include_str!("../assets/shaders/whale-composite.vert"),include_str!("../assets/shaders/whale-composite.frag"))?;
        let draw=gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
        let read=gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING);
        let old_color=gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);
        let old_image=gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
        let multisample=gl.create_framebuffer()?;let resolve=gl.create_framebuffer()?;
        let color=gl.create_renderbuffer()?;let image=gl.create_texture()?;
        let samples=gl.get_parameter_i32(glow::MAX_SAMPLES).clamp(1,4);
        gl.bind_framebuffer(glow::FRAMEBUFFER,Some(multisample));
        gl.bind_renderbuffer(glow::RENDERBUFFER,Some(color));
        gl.renderbuffer_storage_multisample(glow::RENDERBUFFER,samples,glow::RGBA8,AA_SIZE,AA_SIZE);
        gl.framebuffer_renderbuffer(glow::FRAMEBUFFER,glow::COLOR_ATTACHMENT0,glow::RENDERBUFFER,Some(color));
        let valid_ms=gl.check_framebuffer_status(glow::FRAMEBUFFER)==glow::FRAMEBUFFER_COMPLETE;
        gl.bind_texture(glow::TEXTURE_2D,Some(image));
        gl.tex_image_2d(glow::TEXTURE_2D,0,glow::RGBA8 as i32,AA_SIZE,AA_SIZE,0,glow::RGBA,glow::UNSIGNED_BYTE,None);
        for p in [glow::TEXTURE_MIN_FILTER,glow::TEXTURE_MAG_FILTER] {gl.tex_parameter_i32(glow::TEXTURE_2D,p,glow::LINEAR as i32);}
        for p in [glow::TEXTURE_WRAP_S,glow::TEXTURE_WRAP_T] {gl.tex_parameter_i32(glow::TEXTURE_2D,p,glow::CLAMP_TO_EDGE as i32);}
        gl.bind_framebuffer(glow::FRAMEBUFFER,Some(resolve));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER,glow::COLOR_ATTACHMENT0,glow::TEXTURE_2D,Some(image),0);
        let valid_resolve=gl.check_framebuffer_status(glow::FRAMEBUFFER)==glow::FRAMEBUFFER_COMPLETE;
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER,fbo(draw));gl.bind_framebuffer(glow::READ_FRAMEBUFFER,fbo(read));
        gl.bind_renderbuffer(glow::RENDERBUFFER,std::num::NonZeroU32::new(old_color as u32).map(glow::NativeRenderbuffer));
        gl.bind_texture(glow::TEXTURE_2D,texture(old_image));
        let target=Self{multisample,resolve,color,image,composite};
        if !valid_ms||!valid_resolve {target.destroy(gl);return Err("Bộ đệm khử răng cưa cá voi chưa hoàn chỉnh".into());}
        crate::log!("Cá voi: khử răng cưa MSAA {samples}x, bộ đệm cục bộ {AA_SIZE}×{AA_SIZE}");
        Ok(target)
    }}
    fn destroy(self,gl:&glow::Context) {unsafe {
        gl.delete_framebuffer(self.multisample);gl.delete_framebuffer(self.resolve);
        gl.delete_renderbuffer(self.color);gl.delete_texture(self.image);gl.delete_program(self.composite.prog);
    }}
}

pub struct Renderer { skin:Prog,water:Prog,vao:glow::VertexArray,vbo:glow::Buffer,ebo:glow::Buffer,count:i32,aa:Option<Antialias> }
impl Renderer {
    pub fn new(gl:&glow::Context)->Result<Self,String> {
        Self::with_legacy_aa(gl,std::env::args().any(|a|a=="--legacy-whale-aa"))
    }

    fn with_legacy_aa(gl:&glow::Context,legacy:bool)->Result<Self,String> {
        let skin=Prog::from_parts(gl,VERT,FRAG)?;
        let water=match Prog::from_parts(gl,WATER_VERT,WATER_FRAG) {
            Ok(p)=>p,Err(e)=>{unsafe{gl.delete_program(skin.prog)};return Err(e)}
        };
        let aa=if legacy {Some(Antialias::new(gl)?)}else{None};
        let (values,indices)=indexed_mesh();
        let mut bytes=Vec::with_capacity(values.len()*4);
        for v in &values {bytes.extend(v.to_ne_bytes());}
        let mut index_bytes=Vec::with_capacity(indices.len()*4);
        for v in &indices {index_bytes.extend(v.to_ne_bytes());}
        unsafe {
            let previous=gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING);
            let vao=gl.create_vertex_array()?;let vbo=gl.create_buffer()?;let ebo=gl.create_buffer()?;
            gl.bind_vertex_array(Some(vao));gl.bind_buffer(glow::ARRAY_BUFFER,Some(vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER,&bytes,glow::STATIC_DRAW);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER,Some(ebo));
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER,&index_bytes,glow::STATIC_DRAW);
            for (index,size,offset) in [(0,3,0),(1,3,12),(2,2,24)] {
                gl.enable_vertex_attrib_array(index);
                gl.vertex_attrib_pointer_f32(index,size,glow::FLOAT,false,(STRIDE*4) as i32,offset);
            }
            // GLuint handles from the same current context; preserve the theme's
            // empty VAO because the remaining shaders use gl_VertexID.
            let old=std::num::NonZeroU32::new(previous as u32).map(glow::NativeVertexArray);
            gl.bind_vertex_array(old);gl.bind_buffer(glow::ARRAY_BUFFER,None);
            if !legacy {crate::log!("Cá voi: vẽ trực tiếp, viền mềm, {} đỉnh dùng chung; không MSAA/chép bộ đệm",values.len()/STRIDE);}
            Ok(Self{skin,water,vao,vbo,ebo,count:indices.len() as i32,aa})
        }
    }

    pub fn draw(&mut self,gl:&glow::Context,res:[f32;2],whale:[f32;4],motion:Motion,water:f32) {
        if water<65.0 || motion.depth < -92.0 || motion.visibility<0.002 {return;}
        if self.aa.is_none() {self.draw_direct(gl,res,whale,motion);return;}
        let (pose,stroke,fx)=motion.uniforms();
        let aa=self.aa.as_mut().unwrap();
        unsafe {
            let previous=gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING);
            let draw=gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
            let read=gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING);
            let mut viewport=[0;4];gl.get_parameter_i32_slice(glow::VIEWPORT,&mut viewport);
            let mut scissor=[0;4];gl.get_parameter_i32_slice(glow::SCISSOR_BOX,&mut scissor);
            let scissoring=gl.is_enabled(glow::SCISSOR_TEST);
            let stencil=gl.is_enabled(glow::STENCIL_TEST);
            let multisampling=gl.is_enabled(glow::MULTISAMPLE);
            let mut clear=[0.0;4];gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE,&mut clear);
            let side=330.0*whale[3];
            let rect=[whale[0]-side*0.5,whale[1]-side*0.5,side,side];
            let physical=[viewport[0] as f32+rect[0]/res[0]*viewport[2] as f32,
                viewport[1] as f32+(1.0-(rect[1]+side)/res[1])*viewport[3] as f32,
                side/res[0]*viewport[2] as f32,side/res[1]*viewport[3] as f32];
            if scissoring && (physical[0]+physical[2]<=scissor[0] as f32 || physical[0]>= (scissor[0]+scissor[2]) as f32
                || physical[1]+physical[3]<=scissor[1] as f32 || physical[1]>= (scissor[1]+scissor[3]) as f32) {return;}
            let width=(physical[2].ceil() as i32).clamp(2,AA_SIZE);
            let height=(physical[3].ceil() as i32).clamp(2,AA_SIZE);
            let local=[side*0.5,side*0.5,whale[2],whale[3]];
            gl.bind_framebuffer(glow::FRAMEBUFFER,Some(aa.multisample));
            gl.viewport(0,0,width,height);gl.enable(glow::SCISSOR_TEST);gl.scissor(0,0,width,height);
            gl.disable(glow::STENCIL_TEST);gl.enable(glow::MULTISAMPLE);
            gl.clear_color(0.0,0.0,0.0,0.0);gl.clear(glow::COLOR_BUFFER_BIT);
            gl.enable(glow::BLEND);gl.blend_func(glow::ONE,glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(self.water.prog));
            self.water.f2(gl,"uRes",side,side);self.water.f4v(gl,"uWhale",&local);
            self.water.f4v(gl,"uPose",&pose);self.water.f4v(gl,"uEffect",&fx);
            gl.draw_arrays(glow::TRIANGLE_STRIP,0,4);
            gl.bind_vertex_array(Some(self.vao));gl.use_program(Some(self.skin.prog));
            self.skin.f2(gl,"uRes",side,side);self.skin.f4v(gl,"uWhale",&local);
            self.skin.f4v(gl,"uPose",&pose);self.skin.f4v(gl,"uStroke",&stroke);
            self.skin.f2(gl,"uDetail",motion.visibility,motion.juvenile);
            self.skin.f1(gl,"uEdgeAA",0.0);
            gl.enable(glow::CULL_FACE);gl.front_face(glow::CW);
            gl.draw_elements(glow::TRIANGLES,self.count,glow::UNSIGNED_INT,0);
            gl.front_face(glow::CCW);gl.disable(glow::CULL_FACE);
            gl.disable(glow::SCISSOR_TEST);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER,Some(aa.multisample));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER,Some(aa.resolve));
            gl.blit_framebuffer(0,0,width,height,0,0,width,height,glow::COLOR_BUFFER_BIT,glow::NEAREST);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER,fbo(draw));gl.bind_framebuffer(glow::READ_FRAMEBUFFER,fbo(read));
            gl.viewport(viewport[0],viewport[1],viewport[2],viewport[3]);
            gl.scissor(scissor[0],scissor[1],scissor[2],scissor[3]);
            if scissoring {gl.enable(glow::SCISSOR_TEST);}
            if stencil {gl.enable(glow::STENCIL_TEST);}
            if !multisampling {gl.disable(glow::MULTISAMPLE);}
            gl.clear_color(clear[0],clear[1],clear[2],clear[3]);
            let old=std::num::NonZeroU32::new(previous as u32).map(glow::NativeVertexArray);
            gl.bind_vertex_array(old);
            let active=gl.get_parameter_i32(glow::ACTIVE_TEXTURE) as u32;
            gl.active_texture(glow::TEXTURE7);let old_image=gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
            gl.bind_texture(glow::TEXTURE_2D,Some(aa.image));gl.use_program(Some(aa.composite.prog));
            aa.composite.f2(gl,"uRes",res[0],res[1]);aa.composite.f4v(gl,"uRect",&rect);
            aa.composite.f2(gl,"uUvSize",width as f32/AA_SIZE as f32,height as f32/AA_SIZE as f32);
            aa.composite.i1(gl,"uImage",7);gl.draw_arrays(glow::TRIANGLE_STRIP,0,4);
            gl.bind_texture(glow::TEXTURE_2D,texture(old_image));gl.active_texture(active);
            gl.disable(glow::BLEND);gl.use_program(None);
        }
    }

    fn draw_direct(&mut self,gl:&glow::Context,res:[f32;2],whale:[f32;4],motion:Motion) {
        let (pose,stroke,fx)=motion.uniforms();
        unsafe {
            let previous=gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING);
            gl.enable(glow::BLEND);gl.blend_func(glow::ONE,glow::ONE_MINUS_SRC_ALPHA);
            // Respect panorama stencil/scissor without copying either state or
            // changing render targets. All actors share the existing scene FBO.
            if motion.surface>0.001 || (0.0..1.0).contains(&motion.entry_age) {
                gl.use_program(Some(self.water.prog));
                self.water.f2(gl,"uRes",res[0],res[1]);self.water.f4v(gl,"uWhale",&whale);
                self.water.f4v(gl,"uPose",&pose);self.water.f4v(gl,"uEffect",&fx);
                gl.draw_arrays(glow::TRIANGLE_STRIP,0,4);
            }
            gl.bind_vertex_array(Some(self.vao));gl.use_program(Some(self.skin.prog));
            self.skin.f2(gl,"uRes",res[0],res[1]);self.skin.f4v(gl,"uWhale",&whale);
            self.skin.f4v(gl,"uPose",&pose);self.skin.f4v(gl,"uStroke",&stroke);
            self.skin.f2(gl,"uDetail",motion.visibility,motion.juvenile);self.skin.f1(gl,"uEdgeAA",1.0);
            gl.enable(glow::CULL_FACE);gl.front_face(glow::CW);
            gl.draw_elements(glow::TRIANGLES,self.count,glow::UNSIGNED_INT,0);
            gl.front_face(glow::CCW);gl.disable(glow::CULL_FACE);
            gl.bind_vertex_array(std::num::NonZeroU32::new(previous as u32).map(glow::NativeVertexArray));
            gl.disable(glow::BLEND);gl.use_program(None);
        }
    }

    pub fn destroy(self,gl:&glow::Context) {unsafe {
        gl.delete_program(self.skin.prog);gl.delete_program(self.water.prog);
        gl.delete_buffer(self.vbo);gl.delete_buffer(self.ebo);gl.delete_vertex_array(self.vao);
        if let Some(aa)=self.aa {aa.destroy(gl);}
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
            row.extend(pose);row.extend(stroke);row.extend(fx);
            let motion=whale.motion();row.extend([motion.visibility,motion.juvenile]);row
        }).collect::<Vec<_>>());
        pod.step(1.0/fps as f32,|x,y|field.sample(&values,x,y));
    }
    let detail:Vec<_>=(0..=36*fps).map(|frame|{
        let pose=Motion::at(frame as f32/fps as f32);
        let (p,s,f)=pose.uniforms();let mut row=vec![width*0.5,height*0.56,-0.75,2.5];
        row.extend(p);row.extend(s);row.extend(f);row.extend([pose.visibility,pose.juvenile]);row
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

/// Isolate the whale pass on the same GPU, resolution, poses and render target.
/// The legacy path is allocated only for this diagnostic or an explicit flag.
pub fn benchmark(gl:&glow::Context,path:&str)->Result<(),String> {unsafe {
    let old_draw=gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
    let old_read=gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING);
    let mut viewport=[0;4];gl.get_parameter_i32_slice(glow::VIEWPORT,&mut viewport);
    let stencil=gl.is_enabled(glow::STENCIL_TEST);let scissor=gl.is_enabled(glow::SCISSOR_TEST);
    gl.disable(glow::STENCIL_TEST);gl.disable(glow::SCISSOR_TEST);
    let width=2621;let height=1382;
    let target=gl.create_framebuffer()?;let image=gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D,Some(image));
    gl.tex_image_2d(glow::TEXTURE_2D,0,glow::RGBA8 as i32,width,height,0,glow::RGBA,glow::UNSIGNED_BYTE,None);
    gl.bind_framebuffer(glow::FRAMEBUFFER,Some(target));
    gl.framebuffer_texture_2d(glow::FRAMEBUFFER,glow::COLOR_ATTACHMENT0,glow::TEXTURE_2D,Some(image),0);
    if gl.check_framebuffer_status(glow::FRAMEBUFFER)!=glow::FRAMEBUFFER_COMPLETE {return Err("Whale benchmark FBO incomplete".into());}
    gl.viewport(0,0,width,height);
    let mut direct=Renderer::with_legacy_aa(gl,false)?;
    let mut legacy=Renderer::with_legacy_aa(gl,true)?;
    let actors=[([600.0,650.0,0.6,1.0],6.0),([1500.0,850.0,-0.8,1.12],16.0),
        ([2400.0,650.0,2.8,0.86],8.0),([1510.0,930.0,-0.8,0.48],5.0)];
    let mut draw=|mode:usize| {
        gl.bind_framebuffer(glow::FRAMEBUFFER,Some(target));gl.viewport(0,0,width,height);
        gl.clear_color(0.0,0.45,0.50,1.0);gl.clear(glow::COLOR_BUFFER_BIT);
        if mode!=0 {for (actor,age) in actors {
            let mut motion=Motion::at(age);if actor[3]<0.5 {motion.juvenile=1.0;}
            if mode==1 {direct.draw(gl,[3640.0,1920.0],actor,motion,999.0);}
            else {legacy.draw(gl,[3640.0,1920.0],actor,motion,999.0);}
        }}
    };
    let query=gl.create_query()?;
    let mut samples=[Vec::new(),Vec::new(),Vec::new()];
    for _ in 0..12 {draw(0);draw(1);draw(2);}gl.finish();
    for round in 0..8 {
        for mode in if round%2==0 {[0,1,2]}else{[2,1,0]} {
            gl.begin_query(glow::TIME_ELAPSED,query);
            for _ in 0..32 {draw(mode);}
            gl.end_query(glow::TIME_ELAPSED);
            samples[mode].push(gl.get_query_parameter_u32(query,glow::QUERY_RESULT) as f64/32.0/1_000_000.0);
        }
    }
    gl.delete_query(query);drop(draw);direct.destroy(gl);legacy.destroy(gl);
    let medians:Vec<_>=samples.iter_mut().map(|list|{list.sort_by(f64::total_cmp);(list[3]+list[4])*0.5}).collect();
    gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER,fbo(old_draw));gl.bind_framebuffer(glow::READ_FRAMEBUFFER,fbo(old_read));
    gl.viewport(viewport[0],viewport[1],viewport[2],viewport[3]);
    if stencil {gl.enable(glow::STENCIL_TEST);}if scissor {gl.enable(glow::SCISSOR_TEST);}
    gl.delete_framebuffer(target);gl.delete_texture(image);
    if gl.get_error()!=glow::NO_ERROR {return Err("OpenGL error in whale benchmark".into());}
    let report=serde_json::json!({"scope":"whale overlay only, four actors at fixed poses; excludes beach, postprocessing, DWM and other apps",
        "offMs":medians[0],"directMs":medians[1],"legacyMs":medians[2],"samplesMs":samples,
        "whaleReductionPercent":(1.0-(medians[1]-medians[0]).max(0.0)/(medians[2]-medians[0]).max(0.000001))*100.0});
    std::fs::write(path,serde_json::to_vec_pretty(&report).unwrap()).map_err(|e|e.to_string())?;
    crate::log!("GPU cá voi: không vẽ {:.4} ms, trực tiếp {:.4} ms, MSAA cũ {:.4} ms",medians[0],medians[1],medians[2]);
    Ok(())
}}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mesh_is_small_finite_and_has_outward_winding() {
        let data=mesh();assert!(data.len()/STRIDE<18000);assert_eq!(data.len()%(STRIDE*3),0);
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

    #[test]
    fn humpback_has_long_flippers_broad_flukes_and_a_rounded_head() {
        let data=mesh();
        let min_x=data.chunks_exact(STRIDE).map(|v|v[0]).fold(f32::INFINITY,f32::min);
        let max_x=data.chunks_exact(STRIDE).map(|v|v[0]).fold(f32::NEG_INFINITY,f32::max);
        let length=max_x-min_x;
        let fin_tip=data.chunks_exact(STRIDE).filter(|v|v[6]==1.0&&v[1]>0.0)
            .max_by(|a,b|a[1].total_cmp(&b[1])).unwrap();
        let fin_length=(fin_tip[0]-23.0).hypot(fin_tip[1]-13.5);
        assert!((0.25..0.33).contains(&(fin_length/length)));
        let span=data.chunks_exact(STRIDE).filter(|v|v[6]==2.0).map(|v|v[1].abs()).fold(0.0,f32::max)*2.0;
        assert!((0.32..0.40).contains(&(span/length)));
        assert!(profile(60.0).0>profile(-49.0).0*1.6,"head became a pointed fish snout");
        assert!(profile(8.0).0>profile(-64.0).0*5.0,"peduncle must be slender");
    }

    #[test]
    fn indexed_mesh_preserves_every_triangle_with_fewer_vertex_transforms() {
        let expanded=mesh();let(vertices,indices)=indexed_mesh();
        assert!(vertices.len()<expanded.len()/3);
        assert_eq!(indices.len()*STRIDE,expanded.len());
        for (i,index) in indices.iter().enumerate() {
            assert_eq!(&vertices[*index as usize*STRIDE..(*index as usize+1)*STRIDE],&expanded[i*STRIDE..(i+1)*STRIDE]);
        }
    }
}
