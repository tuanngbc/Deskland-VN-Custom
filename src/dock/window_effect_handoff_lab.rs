//! Hosted diagnostic of the real minimize/restore transaction. Screens are
//! compositor captures; mesh buffers and the source are exported separately.
//! This evidence does not infer the owner's border cause or display cadence.
use super::*;

unsafe fn save(dir:&std::path::Path,name:&str,pixels:&[u8],w:u32,h:u32){
    assert_eq!(pixels.len(),w as usize*h as usize*4);
    let rgba=pixels.chunks_exact(4).flat_map(|p|[p[2],p[1],p[0],p[3]]).collect::<Vec<_>>();
    image::save_buffer(dir.join(format!("handoff-{name}.png")),&rgba,w,h,image::ColorType::Rgba8).unwrap();
}
unsafe fn screen(dir:&std::path::Path,name:&str,area:RECT){
    let (w,h)=dims(area);assert!(w>0&&h>0);
    let mut layer=super::super::glass_probe::Layer::new(w,h).unwrap();
    let _=DwmFlush();let dc=GetDC(ptr::null_mut());assert!(!dc.is_null());
    let copied=BitBlt(layer.dc(),0,0,w,h,dc,area.left,area.top,SRCCOPY|CAPTUREBLT);
    ReleaseDC(ptr::null_mut(),dc);assert_ne!(copied,0);GdiFlush();
    let mut pixels=layer.pixels().to_vec();for p in pixels.chunks_exact_mut(4){p[3]=255;}
    save(dir,name,&pixels,w as u32,h as u32);
}
unsafe fn snapshot(dir:&std::path::Path,name:&str,proxy:&Proxy,t:&Transition){
    let (pixels,w,h,canvas)=proxy.evidence().expect("actual mesh before Present rotation");
    assert!(pixels.chunks_exact(4).all(|p|p[..3].iter().all(|c|*c<=p[3])),"complete canvas must remain premultiplied: {name}");
    if !name.ends_with("-terminal"){assert!(pixels.chunks_exact(4).any(|p|p[3]>0),"empty mesh cannot prove handoff: {name}");}
    save(dir,&format!("{name}-mesh"),&pixels,w,h);
    let _=proxy.comp.WaitForCommitCompletion();screen(dir,&format!("{name}-screen"),canvas);
    let source=t.req.source.hwnd as HWND;let mut alpha=0;let mut color=0;let mut flags=0;
    let layered=GetLayeredWindowAttributes(source,&mut color,&mut alpha,&mut flags)!=0;
    println!("FLUIDWIN_HANDOFF_SNAPSHOT name={name} mesh_progress={} frames={} iconic={} source_layered={layered} alpha={alpha} lease={:?} canvas=[{},{},{},{}] full_canvas_premultiplied=PASS",
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
        for action in [Action::Minimize,Action::Restore]{
            let label=format!("{role}-{}-{}",effect.name(),action.name());
            assert!(begin(bar,path,id,action),"real transaction was refused: {label}");
            let mut stages=[false;4];let mut terminal=false;let mut capture=false;let started=Instant::now();
            while active(){
                pump();timer();
                ACTIVE.with(|a|{
                    let a=a.borrow();let Some(t)=a.as_ref()else{return;};
                    if !capture{
                        if let Some(Phase::WaitingMinimize{frame,..})=&t.phase{
                            let stage_snapshot::MotionFrame::Gpu(frame)=frame else{panic!("full-size motion source expected");};
                            let pixels=frame.fixture_pixels().unwrap();save(&dir,&format!("{label}-source-texture"),&pixels,frame.width as u32,frame.height as u32);
                            capture=true;
                        }
                    }
                    if let Some(Phase::Animate{proxy,span})=&t.phase{
                        let progress=span.progress();
                        for (i,threshold) in [0.0,0.12,0.42,0.78].into_iter().enumerate(){
                            if !stages[i]&&progress>=threshold{
                                snapshot(&dir,&format!("{label}-{}",["first","early","mid","near-end"][i]),proxy,t);stages[i]=true;
                                break;
                            }
                        }
                    }
                    if !terminal{if let Some(Phase::WaitingRelease{proxy,..})=&t.phase{snapshot(&dir,&format!("{label}-terminal"),proxy,t);terminal=true;}}
                });
                assert!(started.elapsed()<Duration::from_secs(6),"real handoff did not settle: {label}");
                std::thread::sleep(Duration::from_millis(8));
            }
            let result=LAST.lock().unwrap().clone().unwrap();
            assert_eq!(result["actualEffect"],effect.name(),"real handoff fallback: {result}");
            assert!(stages.into_iter().all(|v|v),"missing actual animation stage: {label} {stages:?}");
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
            println!("FLUIDWIN_HANDOFF action={label} actual_transaction=PASS first_early_mid_near_terminal=EXPORTED source_texture=EXPORTED full_compositor_canvas=EXPORTED journal=CLEARED original_style=RESTORED owner_border=UNRESOLVED display_fps=NOT_MEASURED");
        }
    }
}
