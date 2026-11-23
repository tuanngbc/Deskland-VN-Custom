//! Cooperative work-area reservations. The shell owns the work area; the
//! hidden registration HWND covers its full strip while the UI stays compact.
use super::{DockSide, Display, same_rect};
use std::{mem,ptr};
use windows_sys::Win32::{Foundation::*,UI::{Shell::*,WindowsAndMessaging::*}};

pub const CALLBACK:u32=WM_APP+54;

#[derive(Clone)]
pub struct Request {
    pub display:String,
    pub stage:bool,
    pub monitor:RECT,
    pub side:DockSide,
    pub depth:i32,
}
impl Request {
    fn edge(&self)->u32 { match self.side {DockSide::Bottom=>ABE_BOTTOM,DockSide::Left=>ABE_LEFT,DockSide::Right=>ABE_RIGHT} }
    fn same_slot(&self,other:&Self)->bool {
        self.display==other.display&&self.stage==other.stage&&self.side==other.side&&same_rect(self.monitor,other.monitor)
    }
    fn matches(&self,other:&Self)->bool {
        self.same_slot(other)&&self.depth==other.depth
    }
    fn accepts(&self,r:RECT)->bool {
        let m=self.monitor;
        r.left>=m.left&&r.top>=m.top&&r.right<=m.right&&r.bottom<=m.bottom
            &&r.right>r.left&&r.bottom>r.top&&match self.side{
                DockSide::Bottom=>r.bottom-r.top==self.depth,
                _=>r.right-r.left==self.depth,
            }
    }
}

fn trim(mut rect:RECT,side:DockSide,depth:i32)->Option<RECT>{
    if depth<=0||rect.right<=rect.left || rect.bottom<=rect.top{return None;}
    match side{
        DockSide::Bottom=>{if rect.bottom-rect.top<=depth{return None;}rect.top=rect.bottom-depth;}
        DockSide::Left=>{if rect.right-rect.left<=depth{return None;}rect.right=rect.left+depth;}
        DockSide::Right=>{if rect.right-rect.left<=depth{return None;}rect.left=rect.right-depth;}
    }
    Some(rect)
}

struct Bar{window:HWND,request:Request,position:Option<RECT>,proposal:Option<RECT>,registered:bool}
impl Bar {
    unsafe fn create(request:Request)->Option<Self>{
        let m=request.monitor;
        let window=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,
            super::wide(super::CLASS).as_ptr(),ptr::null(),WS_POPUP,
            m.left,m.top,1,1,ptr::null_mut(),ptr::null_mut(),super::GetModuleHandleW(ptr::null()),ptr::null());
        if window.is_null(){return None;}
        let mut bar=Self{window,request,position:None,proposal:None,registered:false};
        let mut data=bar.data();
        data.uCallbackMessage=CALLBACK;
        if SHAppBarMessage(ABM_NEW,&mut data)==0{return None;}
        bar.registered=true;
        Some(bar)
    }
    fn data(&self)->APPBARDATA {
        APPBARDATA{cbSize:mem::size_of::<APPBARDATA>() as u32,hWnd:self.window,
            uCallbackMessage:0,uEdge:self.request.edge(),rc:self.request.monitor,lParam:0}
    }
    unsafe fn arrange(&mut self)->Option<bool> {
        let mut data=self.data();
        if SHAppBarMessage(ABM_QUERYPOS,&mut data)==0{return None;}
        let rect=trim(data.rc,self.request.side,self.request.depth)?;
        if self.proposal.is_some_and(|old|same_rect(old,rect)){return Some(false);}
        data.rc=rect;
        if SHAppBarMessage(ABM_SETPOS,&mut data)==0{return None;}
        let r=data.rc;
        if !self.request.accepts(r){return None;}
        if SetWindowPos(self.window,ptr::null_mut(),r.left,r.top,r.right-r.left,r.bottom-r.top,SWP_NOACTIVATE|SWP_NOZORDER)==0{return None;}
        SHAppBarMessage(ABM_WINDOWPOSCHANGED,&mut data);
        self.proposal=Some(rect);
        self.position=Some(r);
        Some(true)
    }
}
impl Drop for Bar {
    fn drop(&mut self){unsafe{let mut data=self.data();if self.registered{SHAppBarMessage(ABM_REMOVE,&mut data);}DestroyWindow(self.window);}}
}
#[derive(Default)]
pub struct Bars{
    bars:Vec<Bar>,
    requested:Vec<Request>,
    shell:(usize,u32),
    healthy:bool,
}
#[derive(Clone,Copy)]
pub struct SyncResult{pub healthy:bool,pub work_changed:bool}
fn shell_owner()->(usize,u32){
    unsafe{
        let hwnd=FindWindowW(super::wide("Shell_TrayWnd").as_ptr(),ptr::null());
        let mut pid=0;
        if !hwnd.is_null(){GetWindowThreadProcessId(hwnd,&mut pid);}
        (hwnd as usize,pid)
    }
}
impl Bars {
    pub fn clear(&mut self){*self=Self::default();}
    pub fn contains(&self,id:&str,stage:bool)->bool{
        self.bars.iter().any(|bar|bar.request.stage==stage&&bar.request.display==id)
    }
    /// Resize surviving strips in place and leave unrelated outputs alone.
    /// TaskbarCreated, or a changed shell owner on the recovery tick, forces
    /// fresh ABM_NEW messages. Never write the desktop work area directly.
    pub unsafe fn sync(&mut self,requests:Vec<Request>,notify:bool,restart:bool)->SyncResult {
        let changed=requests.len()!=self.requested.len()||!requests.iter().all(|a|self.requested.iter().any(|b|a.matches(b)));
        if !changed&&!notify&&!restart {
            return SyncResult{healthy:self.healthy||requests.is_empty(),work_changed:false};
        }
        let mut work_changed=false;
        if !requests.is_empty()||!self.bars.is_empty(){
            let shell=shell_owner();
            if restart||shell!=self.shell{
                work_changed=!self.bars.is_empty();
                self.bars.clear();
                self.shell=shell;
            }
        }
        let before=self.bars.len();
        self.bars.retain(|bar|requests.iter().any(|r|r.same_slot(&bar.request)));
        work_changed|=before!=self.bars.len();
        self.healthy=true;
        for request in &requests {
            if let Some(i)=self.bars.iter().position(|bar|bar.request.same_slot(request)){
                let bar=&mut self.bars[i];
                if bar.request.depth!=request.depth{
                    bar.request.depth=request.depth;bar.proposal=None;
                }
                match bar.arrange(){
                    Some(moved)=>work_changed|=moved,
                    None=>{self.bars.remove(i);work_changed=true;self.healthy=false;}
                }
            }else if self.shell.0!=0{
                if let Some(mut bar)=Bar::create(request.clone()){
                    if bar.arrange().is_some(){self.bars.push(bar);work_changed=true;}
                    else{work_changed=true;self.healthy=false;}
                }else{self.healthy=false;}
            }else{self.healthy=false;}
        }
        self.requested=requests;
        SyncResult{healthy:self.healthy,work_changed}
    }
    pub fn placement(&self,display:&Display,stage:bool)->(Display,Option<i32>){
        let mut result=display.clone();
        let Some(bar)=self.bars.iter().find(|b|b.request.display==display.id&&b.request.stage==stage) else{return (result,None);};
        let Some(r)=bar.position else{return (result,None);};
        // Use the shell strip cross axis and the final work area along its
        // length. This avoids centering against our own reserved work area
        // and prevents Stage and Dock intersecting if they share an edge.
        match bar.request.side {
            DockSide::Bottom=>{result.work.bottom=r.bottom;}
            DockSide::Left=>{result.work.left=r.left;}
            DockSide::Right=>{result.work.right=r.right;}
        }
        (result,Some(bar.request.depth))
    }
    pub fn dock_placement(&self, display:&Display, side:DockSide)->Display{
        let (mut dock,_)=self.placement(display,false);
        if side==DockSide::Bottom{
            // Release only the Stage strip along the Dock's length. Keep
            // the cross-axis position and any work area reserved by Windows.
            for bar in self.bars.iter().filter(|b|b.request.display==display.id&&b.request.stage){
                if let Some(r)=bar.position{
                    match bar.request.side{
                        DockSide::Left if r.right==dock.work.left=>dock.work.left=r.left.max(display.bounds.left),
                        DockSide::Right if r.left==dock.work.right=>dock.work.right=r.right.min(display.bounds.right),
                        _=>{}
                    }
                }
            }
        }
        dock
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    #[ignore = "real work area on the disposable Windows Actions desktop"]
    fn native_keeps_dock_when_stage_changes_edges_and_rebinds(){
        use std::{thread, time::{Duration,Instant}};
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(),Ok("true"),"isolated runner only");
        let shell=shell_owner();
        if shell.0==0{
            eprintln!("SKIP live AppBar edge lifecycle: Explorer taskbar missing on runner");
            return;
        }
        let primary=||super::super::displays().into_iter().find(|d|d.primary).expect("primary display");
        unsafe {
            let class=super::super::wide(super::super::CLASS);
            let mut wc:WNDCLASSW=mem::zeroed();
            wc.lpfnWndProc=Some(DefWindowProcW);
            wc.hInstance=super::super::GetModuleHandleW(ptr::null());
            wc.lpszClassName=class.as_ptr();
            assert_ne!(RegisterClassW(&wc),0);
            let before=primary();
            let request=|stage,side,depth|Request{display:before.id.clone(),stage,side,depth,monitor:before.bounds};
            let mut bars=Bars::default();
            let expect=|name:&str, left:i32, right:i32, bottom:i32|{
                let expected=RECT{left:before.work.left+left,right:before.work.right-right,bottom:before.work.bottom-bottom,..before.work};
                let until=Instant::now()+Duration::from_secs(4);
                loop{
                    let mut msg:MSG=mem::zeroed();
                    while PeekMessageW(&mut msg,ptr::null_mut(),0,0,PM_REMOVE)!=0{DispatchMessageW(&msg);}
                    let d=primary();
                    if d.id==before.id&&same_rect(d.work,expected){break;}
                    let coordinates=|r:RECT|[r.left,r.top,r.right,r.bottom];
                    assert!(Instant::now()<until,"{name}: expected {:?}, observed {:?}",coordinates(expected),coordinates(d.work));
                    thread::sleep(Duration::from_millis(25));
                }
                assert_eq!(shell_owner(),shell,"the test must preserve the Windows taskbar owner");
            };
            let dock=request(false,DockSide::Bottom,70);
            let left=request(true,DockSide::Left,68);
            let right=request(true,DockSide::Right,91);
            assert!(bars.sync(vec![dock.clone(),left],false,false).healthy);
            expect("Dock + Stage left",68,0,70);
            let dock_hwnd=bars.bars.iter().find(|b|!b.request.stage).unwrap().window;
            assert!(bars.sync(vec![dock.clone(),right.clone()],false,false).healthy);
            expect("Dock + Stage right",0,91,70);
            assert_eq!(bars.bars.iter().find(|b|!b.request.stage).unwrap().window,dock_hwnd);
            assert!(bars.sync(vec![dock.clone(),right.clone()],true,true).healthy);
            expect("explicit shell recovery signal",0,91,70);
            assert_eq!(IsWindow(dock_hwnd),0,"recovery must destroy old reservation HWNDs");
            assert!(bars.sync(vec![right],false,false).healthy);
            expect("Dock OFF",0,91,0);
            bars.clear();
            expect("all reservations removed",0,0,0);
            eprintln!("Live AppBar left/right, independent removal and registration replay verified (no Explorer restart)");
        }
    }

    #[test]
    fn shell_strip_keeps_negative_coordinates_and_the_taskbar_inset(){
        let available=RECT{left:-1080,top:-731,right:0,bottom:1141};
        assert!(same_rect(trim(available,DockSide::Bottom,80).unwrap(),RECT{top:1061,..available}));
        assert!(same_rect(trim(available,DockSide::Left,132).unwrap(),RECT{right:-948,..available}));
        assert!(same_rect(trim(available,DockSide::Right,132).unwrap(),RECT{left:-132,..available}));
        assert!(trim(available,DockSide::Left,1080).is_none());
        assert!(trim(available,DockSide::Left,0).is_none());
    }
    #[test]
    fn request_rejects_other_monitor_and_invalid_thickness(){
        let monitor=RECT{left:-1080,top:-731,right:0,bottom:1189};
        let request=Request{display:"portrait".into(),stage:false,monitor,side:DockSide::Bottom,depth:101};
        let strip=RECT{top:1040,bottom:1141,..monitor};
        assert!(request.accepts(strip));
        assert!(!request.accepts(RECT{left:0,right:2560,..strip}));
        assert!(!request.accepts(RECT{bottom:1189,..strip}));
        let resized=Request{depth:82,..request.clone()};
        assert!(request.same_slot(&resized)&&!request.matches(&resized));
        assert!(!request.same_slot(&Request{stage:true,..request.clone()}));
    }
}
