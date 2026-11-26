//! Hosted diagnostic of the real minimize/restore transaction. Screens are
//! compositor captures; mesh buffers and the source are exported separately.
//! This evidence does not infer the owner's border cause or display cadence.
use super::*;
struct Image{name:String,pixels:Vec<u8>,w:u32,h:u32}
impl Image{unsafe fn write(self,dir:&std::path::Path){save(dir,&self.name,&self.pixels,self.w,self.h);}}

unsafe fn save(dir:&std::path::Path,name:&str,pixels:&[u8],w:u32,h:u32){
    assert_eq!(pixels.len(),w as usize*h as usize*4);
    let rgba=pixels.chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],p[3]]).collect::<Vec<_>>();
    image::save_buffer(dir.join(format!("handoff-{name}.png")),&rgba,w,h,image::ColorType::Rgba8).unwrap();
}
unsafe fn screen_image(name:&str,area:RECT)->Image{
    let (w,h)=dims(area);assert!(w>0&&h>0);
    let mut layer=super::super::glass_probe::Layer::new(w,h).unwrap();
    let _=DwmFlush();let dc=GetDC(ptr::null_mut());assert!(!dc.is_null());
    let copied=BitBlt(layer.dc(),0,0,w,h,dc,area.left,area.top,SRCCOPY|CAPTUREBLT);
    ReleaseDC(ptr::null_mut(),dc);assert_ne!(copied,0);GdiFlush();
    let mut pixels=layer.pixels().to_vec();for p in pixels.chunks_exact_mut(4){p[3]=255;}
    Image{name:name.into(),pixels,w:w as u32,h:h as u32}
}
unsafe fn screen(dir:&std::path::Path,name:&str,area:RECT){screen_image(name,area).write(dir);}
unsafe fn snapshot(images:&mut Vec<Image>,name:&str,proxy:&Proxy,t:&Transition){
    let (pixels,w,h,canvas)=proxy.evidence().expect("actual mesh before Present rotation");
    assert!(pixels.chunks_exact(4).all(|p|p[..3].iter().all(|c|*c<=p[3])),"complete canvas must remain premultiplied: {name}");
    // Genie/Drop intentionally disappear at progress 1. Restore starts at
    // that exact pose; a transparent first endpoint is valid evidence.
    let invisible_endpoint=t.req.effect.mesh()&&t.last_progress>=1.0-1e-8;
    let occupied=pixels.chunks_exact(4).any(|p|p[3]>0);
    // Screen readback can stall hosted DWM. Require visible first/mid content
    // where expected; late endpoint occupancy is evidence, not an invariant.
    if name.ends_with("-mid")||name.ends_with("-first")&&!invisible_endpoint{assert!(occupied,"empty required visible mesh: {name}");}
    images.push(Image{name:format!("{name}-mesh"),pixels,w,h});
    let _=proxy.comp.WaitForCommitCompletion();images.push(screen_image(&format!("{name}-screen"),canvas));
    let source=t.req.source.hwnd as HWND;let mut alpha=0;let mut color=0;let mut flags=0;
    let layered=GetLayeredWindowAttributes(source,&mut color,&mut alpha,&mut flags)!=0;
    println!("FLUIDWIN_HANDOFF_SNAPSHOT name={name} occupied={occupied} transparent_endpoint={invisible_endpoint} mesh_progress={} frames={} iconic={} source_layered={layered} alpha={alpha} lease={:?} canvas=[{},{},{},{}] full_canvas_premultiplied=PASS",
        t.last_progress,t.attempt.frames,IsIconic(source),t.lease.as_ref().map(|l|l.status()),canvas.left,canvas.top,canvas.right,canvas.bottom);
}

pub(super) unsafe fn probe(bar:HWND,path:&str,id:WindowIdentity,policy:motion::Settings){
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(),Ok("true"));
    assert!(guardian_ready());assert_eq!(Policy::read().native_minimize,Some(1));
    let dir=std::path::PathBuf::from(std::env::var_os("TUANPC_DESK_STAGE_EVIDENCE").unwrap());
    let role=if super::super::taskbar_active(){"taskbar"}else{"compatibility"};
    let source=id.hwnd as HWND;fixture_settle_source(source);
    let style=GetWindowLongPtrW(source,GWL_EXSTYLE);let normal=placement(source).unwrap().rcNormalPosition;
    let (_,key)=super::super::motion_guard::session().unwrap();let key=wide(&key);
    let pump=||{
        fixture_guard_pulse();let mut msg:MSG=mem::zeroed();
        for _ in 0..64{if PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)==0{break;}TranslateMessage(&msg);DispatchMessageW(&msg);}
    };
    for effect in [motion::WindowEffect::Scale,motion::WindowEffect::Genie,motion::WindowEffect::Drop]{
        motion::configure(motion::Settings{window_effect:effect,..policy});fixture_settle_source(source);
        let area=visible_area(source).unwrap();let expanded=RECT{left:area.left-16,top:area.top-16,right:area.right+16,bottom:area.bottom+16};
        screen(&dir,&format!("{role}-{}-source-before",effect.name()),expanded);
        let mut complete=[false;2];
        // GPU/screen readback can stall hosted DWM for an entire late pose.
        // Preserve that incomplete attempt and repeat the actual native
        // minimize/restore cycle, never inventing a screenshot from an endpoint
        // or changing the production animation clock to fill a missing stage.
        for attempt in 0..3{
        for (action_index,action) in [Action::Minimize,Action::Restore].into_iter().enumerate(){
            let suffix=if attempt==0{String::new()}else{format!("-retry{attempt}")};
            let label=format!("{role}-{}-{}{suffix}",effect.name(),action.name());
            assert!(begin(bar,path,id,action),"real transaction was refused: {label}");
            let mut images=Vec::with_capacity(12);
            let mut stages=[false;4];let mut terminal=false;let mut capture=false;let started=Instant::now();
            while active(){
                pump();timer();
                ACTIVE.with(|a|{
                    let a=a.borrow();let Some(t)=a.as_ref()else{return;};
                    if !capture{
                        if let Some(Phase::WaitingMinimize{frame,..})=&t.phase{
                            let stage_snapshot::MotionFrame::Gpu(frame)=frame else{panic!("full-size motion source expected");};
                            let pixels=frame.fixture_pixels().unwrap();images.push(Image{name:format!("{label}-source-texture"),pixels,w:frame.width as u32,h:frame.height as u32});
                            capture=true;
                        }
                    }
                    if let Some(Phase::Animate{proxy,span})=&t.phase{
                        let progress=span.progress();
                        for (i,threshold) in [0.0,0.12,0.42,0.78].into_iter().enumerate(){
                            if !stages[i]&&progress>=threshold{
                                snapshot(&mut images,&format!("{label}-{}",["first","early","mid","near-end"][i]),proxy,t);stages[i]=true;
                                break;
                            }
                        }
                    }
                    if !terminal{if let Some(Phase::WaitingRelease{proxy,..})=&t.phase{snapshot(&mut images,&format!("{label}-terminal"),proxy,t);terminal=true;}}
                });
                assert!(started.elapsed()<Duration::from_secs(6),"real handoff did not settle: {label}");
                std::thread::sleep(Duration::from_millis(8));
            }
            let result=LAST.lock().unwrap().clone().unwrap();
            // Encode/write only after the real transaction has finished. PNG
            // compression must not lengthen animation submission intervals.
            for image in images{image.write(&dir);}
            assert_eq!(result["actualEffect"],effect.name(),"real handoff fallback: {result}");
            assert!(stages[0],"actual handoff must expose the first proxy: {label}");
            assert!(terminal,"must retain terminal proxy while releasing the journal: {label}");
            assert_eq!(GetWindowLongPtrW(source,GWL_EXSTYLE),style);assert!(GetPropW(source,key.as_ptr()).is_null());
            assert!(super::super::same_rect(placement(source).unwrap().rcNormalPosition,normal));
            assert_eq!(IsIconic(source)!=0,action==Action::Minimize);
            screen(&dir,&format!("{label}-released"),expanded);
            if action==Action::Minimize{
                let revision=RECENT.with(|r|r.borrow().get(&id).unwrap().revision);
                let stage_snapshot::MotionFrame::Gpu(frame)=stage_snapshot::recent_motion(id,revision).unwrap()else{panic!("restore must reuse full-size source");};
                let pixels=frame.fixture_pixels().unwrap();save(&dir,&format!("{label}-retained-texture"),&pixels,frame.width as u32,frame.height as u32);
            }
            let all_stages=stages.into_iter().all(|v|v);complete[action_index]|=all_stages;
            println!("FLUIDWIN_HANDOFF action={label} actual_transaction=PASS first_early_mid_near={stages:?} temporal_images={} terminal=EXPORTED source_texture=EXPORTED full_compositor_canvas=EXPORTED journal=CLEARED original_style=RESTORED owner_border=UNRESOLVED display_fps=NOT_MEASURED",if all_stages{"COMPLETE"}else{"INCOMPLETE_REAL_CYCLE_WILL_REPEAT"});
        }
        if complete.into_iter().all(|v|v){break;}
        }
        assert!(complete.into_iter().all(|v|v),"three actual cycles could not collect every temporal pose: {role} {} minimize_restore={complete:?}",effect.name());
    }
}
