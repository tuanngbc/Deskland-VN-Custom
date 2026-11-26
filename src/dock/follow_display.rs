//! FluidWin dock.h::controllaBordo/spostaVerso: dwell on another display's
//! selected edge, fold the existing Dock, transfer it, then reveal it.
use super::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState,VK_LBUTTON,VK_RBUTTON,VK_MBUTTON,VK_XBUTTON1,VK_XBUTTON2};
pub(super) const TIMER:usize=159;
const DWELL:Duration=Duration::from_millis(200);
#[derive(Default)]
struct Follow {armed:bool,candidate:Option<(String,Instant)>,pending:Option<Pending>}
struct Pending {target:String,from:RECT,side:DockSide,started:Instant}
thread_local!{static FOLLOW:RefCell<Follow>=RefCell::new(Follow::default());}

fn at_edge(p:POINT,r:RECT,side:DockSide)->bool{
    p.x>=r.left&&p.x<r.right&&p.y>=r.top&&p.y<r.bottom&&match side{
        DockSide::Bottom=>p.y>=r.bottom-2,DockSide::Top=>p.y<=r.top+1,
        DockSide::Left=>p.x<=r.left+1,DockSide::Right=>p.x>=r.right-2,
    }
}
impl Follow {
    fn dwell(&mut self,target:Option<&str>,now:Instant)->Option<String>{
        let Some(target)=target else{self.candidate=None;return None;};
        match &self.candidate{
            Some((old,since)) if old==target&&now.saturating_duration_since(*since)>DWELL=>{
                self.candidate=None;Some(target.into())
            },
            Some((old,_)) if old==target=>None,
            _=>{self.candidate=Some((target.into(),now));None},
        }
    }
}
unsafe fn animate(bar:&mut Bar,collapsed:bool,reduced:bool){
    if bar.collapse(collapsed,reduced)&&SetTimer(bar.hwnd,MOTION_TIMER,16,None)==0{
        bar.motion.scheduled=false;bar.reveal=motion::Spring::rest(if collapsed{0.0}else{1.0});
    }
    InvalidateRect(bar.hwnd,ptr::null(),0);
}
unsafe fn cancel(state:&mut State,follow:&mut Follow){
    follow.candidate=None;
    if let Some(p)=follow.pending.take(){
        let reduced=state.prefs.appearance.motion_reduced();
        for bar in &mut state.bars{if same_rect(bar.bounds,p.from){let hidden=bar.auto_hide;animate(bar,hidden,reduced);}}
    }
}
pub(super) unsafe fn sync(state:&mut State){
    let wanted=state.prefs.enabled&&!state.suspended&&state.prefs.dock_display.mode==DisplayMode::Follow&&state.screens.len()>1;
    FOLLOW.with(|cell|{
        let mut follow=cell.borrow_mut();
        let invalid=follow.pending.as_ref().is_some_and(|p|p.side!=state.prefs.appearance.position
            ||!state.screens.iter().any(|d|d.id==p.target)||!state.bars.iter().any(|b|same_rect(b.bounds,p.from)));
        if !wanted||invalid{cancel(state,&mut follow);}
        if wanted&&!follow.armed{follow.armed=SetTimer(CONTROL.get(),TIMER,80,None)!=0;}
        else if !wanted&&follow.armed{KillTimer(CONTROL.get(),TIMER);follow.armed=false;}
    });
}
pub(super) unsafe fn poll(){
    let mut point:POINT=mem::zeroed();
    let buttons=[VK_LBUTTON,VK_RBUTTON,VK_MBUTTON,VK_XBUTTON1,VK_XBUTTON2].into_iter()
        .any(|key|GetAsyncKeyState(key as i32)<0);
    let blocked=buttons||GetCursorPos(&mut point)==0||window_effect::active()||window_geometry::active()||window_motion::active();
    sample(point,blocked,Instant::now());
}
unsafe fn sample(point:POINT,blocked:bool,now:Instant){
    let begin=STATE.with(|cell|{
        let mut state=cell.borrow_mut();
        FOLLOW.with(|cell|{
            let mut follow=cell.borrow_mut();
            if !follow.armed{return None;}
            if blocked||state.bars.iter().any(|b|b.pressed.is_some()||b.keyboard.is_some()){
                cancel(&mut state,&mut follow);return None;
            }
            if follow.pending.is_some(){return None;}
            let current=state.bars.first()?;
            let from=current.bounds;
            let target=state.screens.iter().find(|d|!same_rect(d.bounds,from)&&at_edge(point,d.bounds,state.prefs.appearance.position)
                &&!display_fullscreen(d.bounds,&state.windows)).map(|d|d.id.clone());
            let target=follow.dwell(target.as_deref(),now)?;
            follow.pending=Some(Pending{target,from,side:state.prefs.appearance.position,started:now});
            Some(current.hwnd)
        })
    });
    if let Some(owner)=begin{
        // Close transient surfaces before borrowing State again. Their close
        // paths can consult the Dock model, so never call them under its lock.
        utilities::close_panels();shell_menu::close();app_previews::close_for(ptr::null_mut());
        stage_peek::close(ptr::null_mut());crate::launcher::start::dismiss_for_dock_move();EndMenu();close_status_tip(owner);
        STATE.with(|cell|{let mut state=cell.borrow_mut();let reduced=state.prefs.appearance.motion_reduced();
            if let Some(bar)=state.bars.iter_mut().find(|b|b.hwnd==owner){
                KillTimer(owner,HIDE_TIMER);bar.pointer=None;bar.hovered=-1;let immediate=reduced||bar.hidden;animate(bar,true,immediate);
            }
        });
    }
    let mut moved=None;
    update_state(|state|{
        moved=FOLLOW.with(|cell|{
            let mut follow=cell.borrow_mut();let pending=follow.pending.as_ref()?;
            let target=state.screens.iter().find(|d|d.id==pending.target).cloned();
            if target.is_none()||pending.side!=state.prefs.appearance.position||now.saturating_duration_since(pending.started)>Duration::from_secs(4){
                cancel(state,&mut follow);return None;
            }
            let index=state.bars.iter().position(|b|same_rect(b.bounds,pending.from))?;
            if !state.bars[index].hidden&&state.bars[index].reveal.value>0.003{return None;}
            let target=target.unwrap();let id=target.id.clone();
            let reduced=state.prefs.appearance.motion_reduced();
            let widgets=dock_status_widgets(state.prefs.utility_profiles.get(&id));
            let layout=geometry::measure_cluster(&target,&state.prefs.appearance,0,None,0.0,&widgets,state.prefs.utility_cluster(&id));
            let auto=state.prefs.dock_auto_hide(&id)&&display_has_maximized(target.bounds,&state.windows);
            let bar=&mut state.bars[index];
            KillTimer(bar.hwnd,MOTION_TIMER);KillTimer(bar.hwnd,RAIL_FADE_TIMER);KillTimer(bar.hwnd,FEEDBACK_TIMER);
            SURFACES.with(|s|{s.borrow_mut().remove(&(bar.hwnd as usize));});
            bar.bounds=target.bounds;bar.layout=Some(layout);bar.items.clear();bar.pinned_widgets=widgets;
            bar.motion=Motion::default();bar.feedback=None;bar.hovered=-1;bar.keyboard=None;bar.pressed=None;bar.pointer=None;
            bar.previous_focus=0;bar.auto_hide=auto;bar.collapsed=true;bar.hidden=false;bar.reveal=motion::Spring::rest(0.0);
            animate(bar,false,reduced);if auto{SetTimer(bar.hwnd,HIDE_TIMER,650,None);}
            state.prefs.dock_display.selected=Some(id.clone());follow.pending=None;
            Some(id)
        })
    });
    if let Some(id)=moved{
        update_state(|s|{refresh(s,false);for b in &s.bars{if !b.hidden{ShowWindow(b.hwnd,SW_SHOWNOACTIVATE);}}});
        emit(Event::Edit(Edit::FollowDisplay(id)));
    }
}
pub(super) unsafe fn shutdown(){
    KillTimer(CONTROL.get(),TIMER);FOLLOW.with(|s|{*s.borrow_mut()=Follow::default();});
}
#[cfg(test)]pub(super) unsafe fn fixture_sample(point:POINT,blocked:bool,now:Instant){sample(point,blocked,now);}
#[cfg(test)]pub(super) fn fixture_armed()->bool{FOLLOW.with(|s|s.borrow().armed)}

#[cfg(test)]mod tests{
    use super::*;
    #[test]
    #[ignore="native Dock transfer on injected two-output topology; hosted Windows only"]
    fn native_follow_transfers_one_hwnd_and_preserves_stage(){
        use std::sync::mpsc;
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(),Ok("true"));
        let display=displays().into_iter().find(|d|d.primary).unwrap();let mut left=display.clone();let mut right=display;
        left.bounds.right=(left.bounds.left+left.bounds.right)/2;left.work=left.bounds;left.id="fixture:left".into();
        right.bounds.left=left.bounds.right;right.work=right.bounds;right.id="fixture:right".into();right.primary=false;right.dpi=144;
        let mut options=Options{enabled:true,stage_enabled:true,..Default::default()};
        options.dock_display.mode=DisplayMode::Follow;options.appearance.reduced_motion=true;
        options.appearance.position=DockSide::Top;options.stage_display.mode=DisplayMode::Primary;
        let runtime=runtime::Runtime::new(options.clone(),0).unwrap();
        let sample=|screens,point,blocked,at|{
            let (tx,rx)=mpsc::channel();runtime.send(runtime::Command::FollowSample{screens,point,blocked,at,reply:tx});
            rx.recv_timeout(Duration::from_secs(5)).unwrap()
        };
        let now=Instant::now();let point=POINT{x:(right.bounds.left+right.bounds.right)/2,y:right.bounds.top};
        let (before,stages,armed)=sample(Some(vec![left.clone(),right.clone()]),point,false,now);
        assert!(armed);assert_eq!(before.len(),1);assert_eq!(stages.len(),1);assert!(same_rect(before[0].1,left.bounds));
        let (held,_,_)=sample(None,point,true,now+DWELL*2);assert!(same_rect(held[0].1,left.bounds));
        sample(None,point,false,now+DWELL*3);
        let (after,retained,_)=sample(None,point,false,now+DWELL*4+Duration::from_millis(1));
        assert_eq!(after[0].0,before[0].0,"transfer reuses the only Dock HWND");assert!(same_rect(after[0].1,right.bounds));assert_eq!(stages,retained);
        // Topology loss falls back without keeping a dwell timer alive. The
        // persisted disconnected selection is retained for reconnection.
        let (fallback,retained,armed)=sample(Some(vec![left.clone()]),point,false,now+DWELL*5);
        assert!(!armed);assert_eq!(fallback.len(),1);assert!(same_rect(fallback[0].1,left.bounds));assert_eq!(stages,retained);
        runtime.configure(Options{enabled:false,stage_enabled:true,..options});
        let (off,retained,armed)=sample(None,point,false,now+DWELL*6);assert!(off.is_empty());assert!(!armed);assert_eq!(stages,retained);
        drop(runtime);for hwnd in before.into_iter().map(|v|v.0).chain(after.into_iter().map(|v|v.0)).chain(stages){assert_eq!(unsafe{IsWindow(hwnd as HWND)},0);}
        println!("DOCK_FOLLOW two_output_topology=INJECTED edge_dwell=PASS held_button=REFUSED same_hwnd=PASS stage_unchanged=PASS disconnect=PASS timer_off=PASS physical_display_transfer=NOT_VERIFIED");
    }
    #[test]fn dwell_restarts_on_exit_buttons_or_another_output_and_all_edges_are_directional(){
        let now=Instant::now();let mut f=Follow::default();
        assert!(f.dwell(Some("portrait"),now).is_none());
        assert!(f.dwell(Some("portrait"),now+DWELL).is_none());
        assert!(f.dwell(None,now+DWELL).is_none());
        assert!(f.dwell(Some("portrait"),now+DWELL).is_none());
        assert!(f.dwell(Some("third"),now+DWELL*2).is_none());
        assert_eq!(f.dwell(Some("third"),now+DWELL*3+Duration::from_millis(1)),Some("third".into()));
        let r=RECT{left:-1080,top:-731,right:0,bottom:1189};
        for (side,p) in [(DockSide::Top,POINT{x:-500,y:-731}),(DockSide::Bottom,POINT{x:-500,y:1188}),
            (DockSide::Left,POINT{x:-1080,y:0}),(DockSide::Right,POINT{x:-1,y:0})]{
            assert!(at_edge(p,r,side));assert!(!at_edge(POINT{x:1,..p},r,side));
            assert!(!at_edge(POINT{x:-500,y:0},r,side));
        }
    }
}
