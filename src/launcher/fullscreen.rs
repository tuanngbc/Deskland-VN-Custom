//! Native fullscreen library. The input HWND owns its native editor, while
//! a passive companion can host system glass. The main process owns the
//! profile and the app catalog.
use super::{
    catalog,
    library::{Library, Preferences, TileKey},
    *,
};
use std::{
    collections::{HashMap, HashSet},
    ptr,
    sync::mpsc,
};
use windows_sys::Win32::Graphics::Dwm::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::Dialogs::*;
use windows::{core::Interface,Win32::{Foundation::{HMODULE,HWND as WinHwnd,RECT as WinRect,POINT as WinPoint},Graphics::{
    Direct3D::D3D_DRIVER_TYPE_HARDWARE,Direct3D11::*,DirectComposition::*,Dxgi::{IDXGIDevice,IDXGISurface,Common::*},
}}};

const CLASS: &str = "TLauncherSpotlightLibrary";
const EDIT: usize = 501;
const EDIT_KEY: u32 = WM_APP + 150;
const POLL: usize = 1;
const SEARCH: usize = 2;
const SHELL_REFRESH: usize = 3;
const HOVER_ANIMATION: usize = 4;
const GRID_ANIMATION:usize=5;
const TRANSITION_TIMER:usize=6;
const CATALOG_CHANGE: u32 = WM_APP + 151;
// WM_MOUSELEAVE is omitted by this windows-sys version's feature selection.
const WM_MOUSELEAVE: u32 = 0x02A3;

// Windows 11 24H2 added alpha for the redirection bitmap. Older versions
// cannot display our current native editor and premultiplied foreground on
// this pair of HWNDs, so use the explicitly named compatibility material.
const REDIRECTION_ALPHA:u32=39;
const MATERIAL_CLASS:&str="TLauncherSpotlightBackdrop";
#[derive(Clone,Copy,Debug)]
enum BackdropKind { Host, Acrylic }
type DesktopTarget=windows::UI::Composition::Desktop::DesktopWindowTarget;
type Compositor=windows::UI::Composition::Compositor;
type Sprite=windows::UI::Composition::SpriteVisual;
struct HostLayer {
    target:DesktopTarget,
    root:windows::UI::Composition::ContainerVisual,
    brush:windows::UI::Composition::CompositionColorBrush,
    sheet:Sprite,
    material:Sprite,
    backdrop_brush:windows::UI::Composition::CompositionBackdropBrush,
    compositor:Compositor,
}
// The owner log shows ~30 MiB private bytes of growth per repeat open of
// native glass. One WinRT compositor session and one DComp/D3D factory per
// UI thread avoid repeated driver/compositor session allocation. Only these
// lightweight owners remain warm; each overlay still closes its own visuals.
struct HostFactory(Compositor);
impl Drop for HostFactory{fn drop(&mut self){let _=self.0.Close();}}
thread_local!{static HOST_FACTORY:RefCell<Option<HostFactory>>=const{RefCell::new(None)};}
fn close_composition<T:Interface>(object:&T){
    if let Ok(close)=object.cast::<windows::Foundation::IClosable>(){let _=close.Close();}
}
impl Drop for HostLayer{
    fn drop(&mut self){
        let _=self.root.StopAnimation(&windows::core::HSTRING::from("Opacity"));
        if let Ok(children)=self.root.Children(){let _=children.RemoveAll();}
        close_composition(&self.target);
        close_composition(&self.material);
        close_composition(&self.sheet);
        close_composition(&self.backdrop_brush);
        close_composition(&self.brush);
        close_composition(&self.root);
    }
}
struct NativeBackdrop {
    hwnd:HWND,
    host:Option<HostLayer>,
}
struct Dispatcher {
    queue:windows::System::DispatcherQueueController,
}
impl Drop for Dispatcher{
    fn drop(&mut self){
        let _=self.queue.ShutdownQueueAsync();
        unsafe{windows::Win32::System::WinRT::RoUninitialize();}
    }
}
thread_local! {
    static DISPATCHER:RefCell<Option<Dispatcher>>=const{RefCell::new(None)};
}
unsafe extern "system" fn backdrop_proc(hwnd:HWND,msg:u32,w:WPARAM,l:LPARAM)->LRESULT{
    match msg {
        WM_NCHITTEST=>HTTRANSPARENT as isize,
        WM_MOUSEACTIVATE=>MA_NOACTIVATE as isize,
        WM_ERASEBKGND=>1,
        WM_PAINT=>{
            let mut ps:PAINTSTRUCT=std::mem::zeroed();
            let dc=BeginPaint(hwnd,&mut ps);
            if GetWindowLongPtrW(hwnd,GWLP_USERDATA)!=0 {
                FillRect(dc,&ps.rcPaint,GetStockObject(BLACK_BRUSH) as HBRUSH);
            }
            EndPaint(hwnd,&ps);0
        }
        _=>DefWindowProcW(hwnd,msg,w,l)
    }
}
impl HostLayer {
    unsafe fn new(hwnd:HWND)->windows::core::Result<Self>{
        use windows::{core::Interface,Win32::{Foundation::HWND as WinHwnd,System::WinRT::{
            Composition::ICompositorDesktopInterop,*,},},UI::Color};
        use windows_numerics::Vector2;
        use windows::System::DispatcherQueue;
        if DispatcherQueue::GetForCurrentThread().is_err() {
            RoInitialize(RO_INIT_SINGLETHREADED)?;
            let queue=match CreateDispatcherQueueController(DispatcherQueueOptions{
                dwSize:std::mem::size_of::<DispatcherQueueOptions>() as u32,
                threadType:DQTYPE_THREAD_CURRENT,apartmentType:DQTAT_COM_NONE,
            }){
                Ok(queue)=>queue,
                Err(e)=>{RoUninitialize();return Err(e);}
            };
            DISPATCHER.with(|s|s.replace(Some(Dispatcher{queue})));
        }
        let compositor=HOST_FACTORY.with(|slot|->windows::core::Result<Compositor>{
            let mut slot=slot.borrow_mut();
            if slot.is_none(){*slot=Some(HostFactory(Compositor::new()?));}
            Ok(slot.as_ref().unwrap().0.clone())
        })?;
        let api:ICompositorDesktopInterop=compositor.cast()?;
        let target=api.CreateDesktopWindowTarget(WinHwnd(hwnd),false)?;
        let root=compositor.CreateContainerVisual()?;
        let relative=Vector2{X:1.0,Y:1.0};
        root.SetRelativeSizeAdjustment(relative)?;
        let material=compositor.CreateSpriteVisual()?;
        material.SetRelativeSizeAdjustment(relative)?;
        let backdrop_brush=compositor.CreateHostBackdropBrush()?;
        material.SetBrush(&backdrop_brush)?;
        let sheet=compositor.CreateSpriteVisual()?;
        sheet.SetRelativeSizeAdjustment(relative)?;
        let brush=compositor.CreateColorBrushWithColor(Color{A:255,R:24,G:25,B:29})?;
        sheet.SetBrush(&brush)?;
        let children=root.Children()?;
        children.InsertAtTop(&material)?;
        children.InsertAtTop(&sheet)?;
        target.SetRoot(&root)?;
        Ok(Self{target,root,brush,sheet,material,backdrop_brush,compositor})
    }
}
impl NativeBackdrop {
    unsafe fn create(kind:BackdropKind)->Result<Self,String>{
        let class=wide(MATERIAL_CLASS);let instance=GetModuleHandleW(ptr::null());
        let wc=WNDCLASSW{lpfnWndProc:Some(backdrop_proc),hInstance:instance,lpszClassName:class.as_ptr(),..std::mem::zeroed()};
        RegisterClassW(&wc);
        let flags=WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_TRANSPARENT|WS_EX_TOPMOST
            |if matches!(kind,BackdropKind::Host){WS_EX_NOREDIRECTIONBITMAP}else{0};
        let hwnd=CreateWindowExW(flags,class.as_ptr(),wide("TLauncher Spotlight backdrop").as_ptr(),WS_POPUP,
            0,0,1,1,ptr::null_mut(),ptr::null_mut(),instance,ptr::null_mut());
        if hwnd.is_null(){return Err(format!("CreateWindowExW: {}",GetLastError()));}
        let mut out=Self{hwnd,host:None};
        let attr=|id:i32,val:i32|DwmSetWindowAttribute(hwnd,id as u32,(&val as *const i32).cast(),4);
        let _=attr(DWMWA_WINDOW_CORNER_PREFERENCE,DWMWCP_DONOTROUND);
        let _=attr(DWMWA_BORDER_COLOR,-2);
        match kind {
            BackdropKind::Host=>{
                let hr=attr(DWMWA_USE_HOSTBACKDROPBRUSH,1);
                if hr<0{return Err(format!("HostBackdrop opt-in: 0x{:08X}",hr as u32));}
                out.host=Some(HostLayer::new(hwnd).map_err(|e|e.to_string())?);
            }
            BackdropKind::Acrylic=>{
                use windows_sys::Win32::UI::Controls::MARGINS;
                let margins=MARGINS{cxLeftWidth:-1,cxRightWidth:-1,cyTopHeight:-1,cyBottomHeight:-1};
                let extend=DwmExtendFrameIntoClientArea(hwnd,&margins);
                let hr=attr(DWMWA_SYSTEMBACKDROP_TYPE,DWMSBT_TRANSIENTWINDOW);
                if extend<0||hr<0{return Err(format!("Desktop Acrylic: 0x{:08X}/0x{:08X}",extend as u32,hr as u32));}
                SetWindowLongPtrW(hwnd,GWLP_USERDATA,1);
            }
        }
        Ok(out)
    }
    unsafe fn material(&self,dark:bool,strength:i32){
        if let Some(host)=&self.host{
            // Ivory labels require contrast even against white windows.
            // Keep real HostBackdrop color/detail, with a neutral cool tint.
            let (r,g,b)=if dark{(13,17,25)}else{(28,40,48)};
            let _=host.brush.SetColor(windows::UI::Color{A:255,R:r,G:g,B:b});
            let base:f32=if dark{0.50}else{0.53};
            let _=host.sheet.SetOpacity((base+strength as f32*0.002).clamp(0.54,0.64));
        }else{
            let value=i32::from(dark);
            let _=DwmSetWindowAttribute(self.hwnd,DWMWA_USE_IMMERSIVE_DARK_MODE as u32,(&value as *const i32).cast(),4);
        }
    }
    unsafe fn show_behind(&self,foreground:HWND,r:RECT){
        SetWindowPos(self.hwnd,foreground,r.left,r.top,r.right-r.left,r.bottom-r.top,SWP_NOACTIVATE|SWP_SHOWWINDOW);
    }
    fn prepare_open(&self)->windows::core::Result<()>{
        if let Some(host)=&self.host{host.root.SetOpacity(0.0)?;}
        Ok(())
    }
    fn transition(&self,t:Transition)->windows::core::Result<()>{
        let Some(host)=&self.host else{return Ok(());};
        // Tint is configured independently; fade the entire glass visual.
        let animation=host.compositor.CreateScalarKeyFrameAnimation()?;
        animation.SetDuration(windows::Foundation::TimeSpan{Duration:i64::from(t.duration)*10_000})?;
        animation.InsertKeyFrame(0.0,t.from)?;
        animation.InsertKeyFrame(1.0,t.to)?;
        host.root.StartAnimation(&windows::core::HSTRING::from("Opacity"),&animation)
    }
    fn settle(&self){
        if let Some(host)=&self.host{
            let _=host.root.StopAnimation(&windows::core::HSTRING::from("Opacity"));
            let _=host.root.SetOpacity(1.0);
        }
    }
}
impl Drop for NativeBackdrop{
    fn drop(&mut self){unsafe{
        drop(self.host.take());
        DestroyWindow(self.hwnd);
    }}
}

// GDI shapes and DirectWrite glyphs only rasterize dirty foreground pixels.
// DirectComposition keeps the uploaded surface and performs the transitions;
// it does not request app paints or resize either HWND on animation frames.
struct Foreground {
    comp:IDCompositionDevice,
    ctx:ID3D11DeviceContext,
    target:IDCompositionTarget,
    visual:IDCompositionVisual,
    scale:IDCompositionScaleTransform,
    effect:IDCompositionEffectGroup,
    surface:Option<(i32,i32,IDCompositionSurface)>,
}
use crate::dock::motion::{self,Tween as Transition};

struct ForegroundFactory{comp:IDCompositionDevice,ctx:ID3D11DeviceContext}
thread_local!{static FOREGROUND_FACTORY:RefCell<Option<ForegroundFactory>>=const{RefCell::new(None)};}
impl ForegroundFactory{
    unsafe fn new()->windows::core::Result<Self>{
        let mut dev=None;let mut ctx=None;
        D3D11CreateDevice(None,D3D_DRIVER_TYPE_HARDWARE,HMODULE::default(),D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,D3D11_SDK_VERSION,Some(&mut dev),None,Some(&mut ctx))?;
        let dxgi:IDXGIDevice=dev.expect("D3D device").cast()?;
        let comp:IDCompositionDevice=DCompositionCreateDevice(&dxgi)?;
        Ok(Self{comp,ctx:ctx.expect("D3D immediate context")})
    }
}
pub(super) fn reset_graphics_factories(){
    HOST_FACTORY.with(|x|{x.borrow_mut().take();});
    FOREGROUND_FACTORY.with(|x|{x.borrow_mut().take();});
}
impl Foreground {
    unsafe fn new(hwnd:HWND)->windows::core::Result<Self>{
        let (comp,ctx)=FOREGROUND_FACTORY.with(|slot|->windows::core::Result<_>{
            let mut slot=slot.borrow_mut();
            if slot.is_none(){*slot=Some(ForegroundFactory::new()?);}
            let factory=slot.as_ref().unwrap();
            Ok((factory.comp.clone(),factory.ctx.clone()))
        })?;
        // The native EDIT remains interactive below this visual. Its GDI
        // output is supplied by draw_live_edit, so the child must not clip it.
        let target=comp.CreateTargetForHwnd(WinHwnd(hwnd),true)?;
        let visual=comp.CreateVisual()?;
        let scale=comp.CreateScaleTransform()?;
        let effect=comp.CreateEffectGroup()?;
        target.SetRoot(&visual)?;
        comp.Commit()?;
        Ok(Self{comp,ctx,target,visual,scale,effect,surface:None})
    }
    unsafe fn upload(&mut self,frame:&Frame,area:RECT)->windows::core::Result<()>{
        let (w,h)=(frame.w,frame.h);
        let create=!self.surface.as_ref().is_some_and(|(sw,sh,_)|*sw==w&&*sh==h);
        if create{
            let surface=self.comp.CreateSurface(w as u32,h as u32,DXGI_FORMAT_B8G8R8A8_UNORM,DXGI_ALPHA_MODE_PREMULTIPLIED)?;
            self.visual.SetContent(&surface)?;
            self.surface=Some((w,h,surface));
        }
        let clip=if create{rect(0,0,w,h)}else{rect(area.left.max(0),area.top.max(0),area.right.min(w),area.bottom.min(h))};
        if clip.left>=clip.right||clip.top>=clip.bottom{return Ok(());}
        let surface=&self.surface.as_ref().expect("composition surface").2;
        let bounds=WinRect{left:clip.left,top:clip.top,right:clip.right,bottom:clip.bottom};
        let mut offset=WinPoint::default();
        let drawing:IDXGISurface=surface.BeginDraw(Some(&bounds),&mut offset)?;
        let updated=(||->windows::core::Result<()>{
            let subresource:ID3D11Texture2D=drawing.cast()?;
            let bounds=D3D11_BOX{left:offset.x as u32,top:offset.y as u32,front:0,
                right:(offset.x+clip.right-clip.left) as u32,bottom:(offset.y+clip.bottom-clip.top) as u32,back:1};
            let source=frame.bits.add(((clip.top as usize*w as usize+clip.left as usize)*4) as usize);
            self.ctx.UpdateSubresource(&subresource,0,Some(&bounds),source.cast(),w as u32*4,0);
            Ok(())
        })();
        let result=surface.EndDraw();
        updated?;
        result?;
        self.comp.Commit()
    }
    unsafe fn prepare_open(&self)->windows::core::Result<()>{
        self.effect.SetOpacity2(0.0)?;
        self.visual.SetEffect(&self.effect)?;
        self.comp.Commit()
    }
    unsafe fn motion(&self,t:Transition,w:i32,anchor_y:i32)->windows::core::Result<()>{
        let curve=|from:f32,to:f32|->windows::core::Result<IDCompositionAnimation>{
            let seconds=t.duration as f32/1000.0;
            let d=to-from;
            let anim=self.comp.CreateAnimation()?;
            anim.AddCubic(0.0,from,3.0*d/seconds,-3.0*d/seconds.powi(2),d/seconds.powi(3))?;
            anim.End(seconds as f64,to)?;
            Ok(anim)
        };
        let zoom=|v:f32|motion::zoom(motion::FULL.scale,v);
        let a=curve(zoom(t.from),zoom(t.to))?;
        self.scale.SetCenterX2(w as f32*0.5)?;
        self.scale.SetCenterY2(anchor_y as f32)?;
        self.scale.SetScaleX(&a)?;
        self.scale.SetScaleY(&a)?;
        self.effect.SetOpacity(&curve(t.from,t.to)?)?;
        self.visual.SetTransform(&self.scale)?;
        self.visual.SetEffect(&self.effect)?;
        self.comp.Commit()
    }
    unsafe fn settle(&self)->windows::core::Result<()>{
        // Return to direct composition of the original pixels. Keeping the
        // completed opacity effect attached would also keep its intermediate surface.
        self.visual.SetTransform(None::<&IDCompositionTransform>)?;
        self.visual.SetEffect(None::<&IDCompositionEffect>)?;
        self.comp.Commit()
    }
}
impl Drop for Foreground{
    fn drop(&mut self){unsafe{
        let _=self.visual.SetContent(None::<&windows::core::IUnknown>);
        let _=self.visual.SetTransform(None::<&IDCompositionTransform>);
        let _=self.visual.SetEffect(None::<&IDCompositionEffect>);
        let _=self.target.SetRoot(None::<&IDCompositionVisual>);
        self.surface.take();
        let _=self.comp.Commit();self.ctx.ClearState();self.ctx.Flush();
    }}
}

struct LiveMaterial { backdrop:NativeBackdrop, foreground:Foreground }
fn set_redirection_alpha(hwnd:HWND,enabled:bool)->bool{
    let value=i32::from(enabled);
    unsafe{DwmSetWindowAttribute(hwnd,REDIRECTION_ALPHA,(&value as *const i32).cast(),4)>=0}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Category {
    All,
    Apps,
    Files,
    System,
}
impl Category {
    const ALL: [Self; 4] = [Self::All, Self::Apps, Self::Files, Self::System];
    const LABELS: [&str; 4] = ["Tất cả", "Ứng dụng", "Tệp tin", "Hệ thống"];
    fn accepts(self, e: &Entry) -> bool {
        match self {
            Self::All => true,
            Self::Apps => app_entry(e),
            Self::Files => matches!(e.kind, "File" | "Web shortcut"),
            Self::System => e.kind == "System",
        }
    }
}

#[derive(Clone)]
struct Tile {
    key: TileKey,
    entry: Entry,
    preview: Vec<PathBuf>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    Compact,
    Search,
    Backdrop,
    Edit,
    Add,
    Create,
    Hidden,
    Refresh,
    Settings,
    Back,
    Tab(Category),
    Page(usize),
    Tile(usize),
}

#[derive(Clone, Copy)]
struct Grid {
    area: RECT,
    columns: usize,
    rows: usize,
    dx: i32,
    dy: i32,
    icon: i32,
}
impl Grid {
    fn new(w: i32, h: i32, dpi: u32, cfg: &Preferences) -> Self {
        Self::with_toolbar(w,h,dpi,cfg,false)
    }
    fn with_toolbar(w:i32,h:i32,dpi:u32,cfg:&Preferences,toolbar:bool)->Self {
        let p=|n|px(n,dpi);
        let edge=p(34).min(w/10);
        let top=p(if toolbar{159}else{112}).min(h/2);
        let bottom=(h-p(72)).max(top+1);
        let aw=(w-2*edge).max(1);
        let ah=bottom-top;
        let icon=p(cfg.icon_size).min((ah-p(28)).max(12)).min(w/4).max(8);
        let want_x=(icon+p(cfg.spacing+36)).max(p(106));
        let want_y=(icon+p(cfg.spacing+40)).max(p(96));
        let columns=(aw/want_x).clamp(1,if w>h*2{11}else{9}) as usize;
        let rows=(ah/want_y).clamp(1,12) as usize;
        let dx=(aw/columns as i32).min(p(177)).max(1);
        let dy=(ah/rows as i32).min(p(145)).max(1);
        let left=edge+(aw-dx*columns as i32)/2;
        let top=top+((ah-dy*rows as i32)/3).min(p(26));
        let area=rect(left,top,left+dx*columns as i32,top+dy*rows as i32);
        Self {
            area,
            columns,
            rows,
            dx,
            dy,
            icon,
        }
    }
    fn capacity(self) -> usize {
        self.columns * self.rows
    }
    fn tiles(self, i: usize) -> RECT {
        let x = self.area.left + (i % self.columns) as i32 * self.dx;
        let y = self.area.top + (i / self.columns) as i32 * self.dy;
        let inset=(self.dx/15).clamp(1,10).min((self.dx-1)/2);
        let vertical=(self.dy/28).clamp(1,6).min((self.dy-1)/2);
        rect(x+inset,y+vertical,x+self.dx-inset,y+self.dy-vertical)
    }
}
fn px(n: i32, dpi: u32) -> i32 {
    (n * dpi as i32 + 48) / 96
}
fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
    RECT {
        left,
        top,
        right,
        bottom,
    }
}
fn hit(r: RECT, p: POINT) -> bool {
    p.x >= r.left && p.y >= r.top && p.x < r.right && p.y < r.bottom
}
fn coords(l: LPARAM) -> POINT {
    POINT {
        x: l as i16 as i32,
        y: (l >> 16) as i16 as i32,
    }
}

struct Panel {
    hwnd: HWND,
    edit: HWND,
    prior: HWND,
    point: POINT,
    width: i32,
    height: i32,
    dpi: u32,
    // In-memory, quarter-resolution snapshot; ordinary opaque GDI surface.
    backdrop: Vec<u8>,
    glass: std::rc::Rc<[u8]>,
    native: Option<LiveMaterial>,
    native_rejected: bool,
    transition: Option<Transition>,
    bounds: RECT,
    dark: bool,
    high_contrast:bool,
    font: HFONT,
    search_font:HFONT,
    apps: Vec<Entry>,
    catalog: Option<mpsc::Receiver<Vec<Entry>>>,
    watch: Option<catalog::ShellWatch>,
    input: String,
    category: Category,
    folder: u64,
    hidden: bool,
    generation: u64,
    search_rx: Option<mpsc::Receiver<SearchReply>>,
    accept_pending: bool,
    search_settled: bool,
    editing: bool,
    page: usize,
    selected: usize,
    tiles: Vec<Tile>,
    error: String,
    hover: Option<Hit>,
    hover_from: Option<Hit>,
    hover_started:Option<motion::Span>,
    grid_started:Option<motion::Span>,
    motion: bool,
    tracking: bool,
    keyboard: bool,
    wheel:i32,
    pressed: Option<(Hit, POINT)>,
    icon_tx: IconRequests,
    icon_rx: mpsc::Receiver<IconResult>,
    icons: HashMap<IconKey, Option<Arc<IconPixels>>>,
    pending: HashSet<IconKey>,
    icon_rev: u64,
}
thread_local! {
    static PANEL: RefCell<Option<Panel>> = const { RefCell::new(None) };
    static OPTIONS: RefCell<Preferences> = RefCell::new(Preferences::default());
    static LIBRARY: RefCell<Library> = RefCell::new(Library::default());
    static EDIT_PROC: Cell<WNDPROC> = const { Cell::new(None) };
    // The control retains its HWND, focus, IME, scrolling and accessibility.
    // Its visible appearance is owned by the search surface, not USER32.
    static EDIT_COMPOSITE: Cell<bool> = const { Cell::new(false) };
    static EDIT_COMPOSING: Cell<bool> = const { Cell::new(false) };
    static EDIT_PRINT: Cell<Option<(u32,u32)>> = const { Cell::new(None) };
    #[cfg(test)]
    static LIVE_SEARCH_FIXTURE: Cell<bool> = const { Cell::new(false) };
    static MODAL: Cell<bool> = const { Cell::new(false) };
    static FRAME: RefCell<Option<Frame>> = const { RefCell::new(None) };
}

// A persistent compatible bitmap, not an alpha-bearing DWM extended frame.
// GDI renders RGB into it; only the completed dirty region is presented.
struct Frame { dc: HDC, bitmap: HBITMAP, old: HGDIOBJ, bits:*mut u8, w: i32, h: i32 }
impl Frame {
    unsafe fn new(target: HDC, w: i32, h: i32) -> Option<Self> {
        if w <= 0 || h <= 0 { return None; }
        let dc = CreateCompatibleDC(target);
        if dc.is_null() { return None; }
        let mut info:BITMAPINFO=std::mem::zeroed();
        info.bmiHeader=BITMAPINFOHEADER{biSize:std::mem::size_of::<BITMAPINFOHEADER>() as u32,biWidth:w,biHeight:-h,
            biPlanes:1,biBitCount:32,biCompression:BI_RGB,..std::mem::zeroed()};
        let mut bits=ptr::null_mut();
        let bitmap=CreateDIBSection(dc,&info,DIB_RGB_COLORS,&mut bits,ptr::null_mut(),0);
        if bitmap.is_null()||bits.is_null() {if !bitmap.is_null(){DeleteObject(bitmap);}DeleteDC(dc);return None;}
        let old = SelectObject(dc, bitmap);
        if old.is_null()||old as isize == -1 {DeleteObject(bitmap);DeleteDC(dc);return None;}
        Some(Self { dc, bitmap, old, bits:bits.cast(), w, h })
    }
}
impl Drop for Frame {
    fn drop(&mut self) { unsafe { SelectObject(self.dc, self.old); DeleteObject(self.bitmap); DeleteDC(self.dc); } }
}

fn intersects(a: RECT, b: RECT) -> bool {
    a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
}

fn target_rect(p: &Panel, target: Hit) -> Option<RECT> {
    if let Hit::Tile(i) = target {
        let g = grid(p);
        let start = p.page * g.capacity();
        return (i >= start && i < start + g.capacity() && i < p.tiles.len()).then(|| g.tiles(i - start));
    }
    controls(p).into_iter().find_map(|(h, r, _)| (h == target).then_some(r))
}

fn hover_transition(p: &mut Panel, next: Option<Hit>) -> Vec<RECT> {
    if p.hover == next { return Vec::new(); }
    let previous = p.hover_from;
    let old = p.hover;
    p.hover = next;
    let animate=p.motion&&motion::allowed();
    p.hover_from = if animate { old } else { None };
    p.hover_started = animate.then(motion::Span::hover);
    previous.into_iter().chain(old).chain(next).filter_map(|h| target_rect(p, h)).collect()
}

pub(super) fn motion_allowed() -> bool {
    OPTIONS.with(|p|p.borrow().animations)&&motion::allowed()
}

fn hover_opacity(h:Hit,current:Option<Hit>,previous:Option<Hit>,progress:f32)->f32 {
    if current==Some(h) { progress } else if previous==Some(h) { 1.0-progress } else { 0.0 }
}

fn animate_hover(hwnd:HWND,regions:Vec<RECT>) {
    if !regions.is_empty() && PANEL.with(|p|p.borrow().as_ref().is_some_and(|p|p.hover_started.is_some())) {
        unsafe {SetTimer(hwnd,HOVER_ANIMATION,16,None);}
    }
    invalidate_regions(hwnd,regions);
}

fn grid_opacity(progress:f32)->u8 {
    // The new content is immediately interactive, then fades to full opacity.
    (150.0+105.0*motion::ease(progress)).round() as u8
}

fn start_grid_motion(hwnd:HWND) {
    if unsafe{IsWindowVisible(hwnd)}==0{return;}
    let active=PANEL.with(|p|{
        let mut p=p.borrow_mut();let Some(p)=p.as_mut() else{return false;};
        let base=if p.folder!=0||p.hidden{motion::FOLDER_MS}else{motion::PAGE_MS as u32};
        p.grid_started=(p.motion&&motion::allowed()).then(||motion::Span::new(base));p.grid_started.is_some()
    });
    if active {unsafe{SetTimer(hwnd,GRID_ANIMATION,16,None);}}
}

fn invalidate_regions(hwnd: HWND, regions: impl IntoIterator<Item=RECT>) {
    for r in regions { unsafe { InvalidateRect(hwnd, &r, 0); } }
}
pub(super) fn configure(prefs: Preferences) {
    let changed = OPTIONS.with(|o| {
        let mut previous=o.borrow().clone();
        previous.hotkey_fullscreen=prefs.hotkey_fullscreen;
        previous.dock_fullscreen=prefs.dock_fullscreen;
        previous.dock_follow_hotkey=prefs.dock_follow_hotkey;
        let changed=previous!=prefs;
        o.replace(prefs);
        changed
    });
    if changed && enabled() {
        if let Some(hwnd) = window() {
            unsafe {
                SendMessageW(hwnd, WM_THEMECHANGED, 0, 0);
            }
            rebuild(hwnd, false);
        }
    }
}
pub(super) fn initialize(library: Library) {
    LIBRARY.with(|s| s.replace(library));
}
pub(super) fn merge_manual(rows: &mut Vec<Entry>) {
    LIBRARY.with(|s| s.borrow().merge_manual(rows));
}
pub(super) fn window() -> Option<HWND> {
    PANEL.with(|p| p.borrow().as_ref().map(|s| s.hwnd))
}
pub(super) fn enabled() -> bool {
    OPTIONS.with(|s| s.borrow().enabled)
}
pub(super) fn fullscreen_for(dock: bool) -> bool {
    OPTIONS.with(|s| s.borrow().fullscreen_for(dock))
}

fn dark() -> bool {
    OPTIONS.with(|p| match p.borrow().theme.as_str() {
        "dark" => true,
        "light" => false,
        _ => !crate::web::light_theme(),
    })
}
#[derive(Clone,Copy)]
struct Palette { face:u32, ink:u32, secondary:u32, active:u32, fallback:u32 }
fn palette(dark:bool, high_contrast:bool)->Palette {
    if high_contrast { unsafe {
        return Palette{face:GetSysColor(COLOR_WINDOW),ink:GetSysColor(COLOR_WINDOWTEXT),secondary:GetSysColor(COLOR_WINDOWTEXT),active:GetSysColor(COLOR_HIGHLIGHT),fallback:GetSysColor(COLOR_WINDOW)};
    } }
    // COLORREF: 0x00BBGGRR. Ivory #F5F8FC in both non-HC themes.
    if dark { Palette{face:0x00493d34,ink:0x00fcf8f5,secondary:0x00e4ddd5,active:0x00938170,fallback:0x00352a22} }
    else { Palette{face:0x00776c62,ink:0x00fcf8f5,secondary:0x00eee7df,active:0x00b8a89a,fallback:0x00746a5b} }
}
fn grid(p: &Panel) -> Grid {
    OPTIONS.with(|o| Grid::with_toolbar(p.width,p.height,p.dpi,&o.borrow(),
        !p.input.trim().is_empty()||p.editing||p.folder!=0||p.hidden))
}
fn count_pages(p: &Panel) -> usize {
    p.tiles.len().div_ceil(grid(p).capacity()).max(1)
}
fn refresh(hwnd: HWND) {
    let _ = unsafe { InvalidateRect(hwnd, ptr::null(), 0) };
}

fn system_items() -> Vec<Entry> {
    [
        ("Cài đặt Windows", "ms-settings:"),
        ("Task Manager", "Taskmgr.exe"),
        ("Bảng điều khiển · Control Panel", "control.exe"),
        ("File Explorer", "explorer.exe"),
    ]
    .into_iter()
    .map(|(name, path)| Entry {
        name: name.into(),
        path: path.into(),
        kind: "System",
        detail: "Windows".into(),
    })
    .collect()
}

struct SearchWork {
    source: Vec<Entry>,
    lib: Library,
    query: String,
    category: Category,
    folder: u64,
    hidden: bool,
    favorites: bool,
}
impl SearchWork {
    fn from_panel(p: &Panel) -> Self {
        let mut source = p.apps.clone();
        sync_pin_entries(&mut source);
        let lib = LIBRARY.with(|s| s.borrow().clone());
        lib.merge_manual(&mut source);
        Self {
            source,
            lib,
            query: p.input.trim().to_lowercase(),
            category: p.category,
            folder: p.folder,
            hidden: p.hidden,
            favorites: OPTIONS.with(|o| o.borrow().favorites_first),
        }
    }
    fn rows(self) -> Vec<Tile> {
        let Self {
            mut source,
            lib,
            query,
            category,
            folder,
            hidden,
            favorites,
        } = self;
        let browsing = query.is_empty();
        if !browsing || category == Category::System {
            source.extend(system_items());
        }
        let mut previews: HashMap<u64, Vec<PathBuf>> = HashMap::new();
        if browsing && folder == 0 && !hidden {
            for app in source.iter().filter(|app| app_entry(app)) {
                let id = catalog::identity(&app.path);
                let group = lib.folder(&id);
                if group == 0 || lib.hidden(&id) {
                    continue;
                }
                let icons = previews.entry(group).or_default();
                if icons.len() < 4 {
                    icons.push(app.path.clone());
                }
            }
        }
        let mut found: Vec<_> = source
            .into_iter()
            .filter_map(|mut e| {
                if !category.accepts(&e)
                    || (browsing && category == Category::All && !app_entry(&e))
                {
                    return None;
                }
                let id = catalog::identity(&e.path);
                if app_entry(&e) {
                    if lib.hidden(&id) != hidden
                        || (browsing && !hidden && lib.folder(&id) != folder)
                    {
                        return None;
                    }
                    e.name = lib.name(&id, &e.name).to_owned();
                } else if hidden || folder != 0 {
                    return None;
                }
                let rank = if browsing {
                    0
                } else {
                    catalog::search_rank(&e.name, &query)?
                };
                Some((rank, (TileKey::App(id), e)))
            })
            .collect();
        if browsing && folder == 0 && !hidden && matches!(category, Category::All | Category::Apps)
        {
            found.extend(lib.folders.iter().map(|f| {
                (
                    0,
                    (
                        TileKey::Folder(f.id),
                        Entry {
                            name: f.name.clone(),
                            path: PathBuf::new(),
                            kind: "Folder",
                            detail: String::new(),
                        },
                    ),
                )
            }));
        }
        let sorted = if browsing {
            let mut items = found.into_iter().map(|(_, t)| t).collect::<Vec<_>>();
            lib.ordered(&mut items, favorites);
            items
        } else {
            found.sort_by_key(|(rank, (_, e))| (*rank, !app_entry(e)));
            found.into_iter().take(100).map(|(_, t)| t).collect()
        };
        let mut seen = HashSet::new();
        sorted
            .into_iter()
            .filter(|(key,_)|seen.insert(match key { TileKey::App(id)=>format!("app:{id}"), TileKey::Folder(id)=>format!("folder:{id}") }))
            .map(|(key, entry)| {
                let preview = if let TileKey::Folder(id) = key {
                    previews.remove(&id).unwrap_or_default()
                } else {
                    Vec::new()
                };
                Tile {
                    key,
                    entry,
                    preview,
                }
            })
            .collect()
    }
}

type SearchReply = (u64, Vec<Tile>);
fn search_async(gen: u64, job: SearchWork) -> Option<mpsc::Receiver<SearchReply>> {
    type Task = (u64, SearchWork, mpsc::Sender<SearchReply>);
    static QUEUE: std::sync::OnceLock<Option<mpsc::SyncSender<Task>>> = std::sync::OnceLock::new();
    let queue = QUEUE
        .get_or_init(|| {
            let (tx, rx) = mpsc::sync_channel::<Task>(1);
            std::thread::Builder::new()
                .name("tlauncher-spotlight-search".into())
                .spawn(move || {
                    while let Ok((generation, job, reply)) = rx.recv() {
                        let _ = reply.send((generation, job.rows()));
                    }
                })
                .ok()?;
            Some(tx)
        })
        .as_ref()?;
    let (tx, rx) = mpsc::channel();
    queue.try_send((gen, job, tx)).ok()?;
    Some(rx)
}

fn apply_rows(hwnd: HWND, generation: u64, tiles: Vec<Tile>) {
    let launch = PANEL.with(|state| {
        let mut state = state.borrow_mut();
        let p = state.as_mut()?;
        if generation != p.generation {
            return None;
        }
        p.search_settled = true;
        p.tiles = tiles;
        p.hover=None;p.hover_from=None;p.hover_started=None;
        p.page = p.page.min(count_pages(p) - 1);
        p.selected = p.selected.min(p.tiles.len().saturating_sub(1));
        (std::mem::take(&mut p.accept_pending) && !p.tiles.is_empty()).then_some(p.selected)
    });
    if let Some(index) = launch {
        unsafe {
            action(hwnd, Hit::Tile(index));
        }
    }
    start_grid_motion(hwnd);
    refresh(hwnd);
}
fn rebuild(hwnd: HWND, reset_page: bool) {
    let Some((gen, job)) = PANEL.with(|state| {
        let mut state = state.borrow_mut();
        let p = state.as_mut()?;
        p.generation = p.generation.wrapping_add(1);
        p.search_rx = None;
        p.search_settled = false;
        if reset_page {
            p.page = 0;
            p.selected = 0;
        }
        Some((p.generation, SearchWork::from_panel(p)))
    }) else {
        return;
    };
    if job.query.is_empty() && job.source.len() < 1000 {
        apply_rows(hwnd, gen, job.rows());
        return;
    }
    let reply = search_async(gen, job);
    unsafe {
        if reply.is_none() {
            SetTimer(hwnd, SEARCH, 45, None);
            return;
        }
        SetTimer(hwnd, POLL, 25, None);
    }
    PANEL.with(|s| {
        if let Some(p) = s.borrow_mut().as_mut() {
            p.search_rx = reply;
        }
    });
    refresh(hwnd);
}

fn search_rect(w:i32,dpi:u32)->RECT {
    let u=|n|px(n,dpi);
    let width=u(570).min((w-u(32)).max(1));
    let x=(w-width)/2;
    rect(x,u(42),x+width,u(91))
}
fn edit_rect(w:i32,dpi:u32)->RECT {
    let r=search_rect(w,dpi);
    let u=|n|px(n,dpi);
    let x=(r.left+u(47)).min(r.right-1);
    let y=r.top+u(13);
    rect(x,y,(r.right-u(14)).max(x+1),y+u(24))
}
fn controls(p:&Panel)->Vec<(Hit,RECT,&'static str)>{
    let u=|n|px(n,p.dpi);
    let mid=p.width/2;
    let button=|x,y,w|rect(x,y,x+w,y+u(32));
    let mut out=Vec::new();
    if p.folder!=0||p.hidden {out.push((Hit::Back,button(u(26),u(106),u(108)),words("‹ Thư viện","‹ Library","‹ 应用库")));}
    if p.editing {
        out.push((Hit::Edit,button(p.width-u(100),u(106),u(74)),words("Xong","Done","完成")));
    }
    if !p.input.trim().is_empty(){
        let width=u(104).min((p.width-u(24)).max(4)/4).max(1);
        let labels=words_set(["Tất cả","Ứng dụng","Tệp tin","Hệ thống"],["All","Apps","Files","System"],["全部","应用","文件","系统"]);
        for (i,cat) in Category::ALL.into_iter().enumerate(){
            out.push((Hit::Tab(cat),button(mid-2*width+i as i32*width,u(103),(width-u(3)).max(1)),labels[i]));
        }
    }
    let pages=count_pages(p);
    if pages>1{
        // Keep every page directly clickable, including large libraries.
        let step=u(24).min(((p.width-u(32)).max(1)/pages as i32).max(1));
        let y=p.height-u(55);
        for i in 0..pages{
            out.push((Hit::Page(i),button(mid-(pages as i32*step)/2+i as i32*step,y,step),""));
        }
    }
    out
}

fn words<'a>(vi:&'a str,en:&'a str,zh:&'a str)->&'a str {
    LOCALE.with(|lang|match lang.get(){0=>vi,2=>zh,_=>en})
}
fn words_set<const N:usize>(vi:[&'static str;N],en:[&'static str;N],zh:[&'static str;N])->[&'static str;N]{
    LOCALE.with(|lang|match lang.get(){0=>vi,2=>zh,_=>en})
}
thread_local!{static LOCALE:Cell<u8>=const{Cell::new(0)};}
pub(super) fn locale(value:&str){LOCALE.set(match value{"vi"=>0,"zh-CN"=>2,_=>1});}

fn hit_at(p: &Panel, point: POINT) -> Option<Hit> {
    let point=if let Some(t)=p.transition{
        let scale=motion::zoom(motion::FULL.scale,t.openness());
        let (x,y)=(p.width as f32*0.5,px(66,p.dpi) as f32);
        POINT{x:motion::inverse(point.x as f32,x,scale).round() as i32,
            y:motion::inverse(point.y as f32,y,scale).round() as i32}
    }else{point};
    if hit(search_rect(p.width,p.dpi),point){return Some(Hit::Search);}
    if let Some((action, ..)) = controls(p).into_iter().find(|(_, r, _)| hit(*r, point)) {
        return Some(action);
    }
    let g = grid(p);
    if !hit(g.area, point) {
        return None;
    }
    let x = ((point.x - g.area.left) / g.dx) as usize;
    let y = ((point.y - g.area.top) / g.dy) as usize;
    if y >= g.rows || x >= g.columns {
        return None;
    }
    let i = p.page * g.capacity() + y * g.columns + x;
    (i < p.tiles.len()&&hit(g.tiles(i%g.capacity()),point)).then_some(Hit::Tile(i))
}

unsafe fn fill(dc: HDC, r: RECT, rgb: u32) {
    SetDCBrushColor(dc, rgb);
    FillRect(dc, &r, GetStockObject(DC_BRUSH) as HBRUSH);
}
fn rounded_ink(w: i32, h: i32, radius: i32, rgb: u32, alpha: u8) -> IconPixels {
    let mut pixels = vec![0; (w * h * 4) as usize];
    let r = (radius as f32 / 2.0).min(w.min(h) as f32 / 2.0);
    for y in 0..h {
        for x in 0..w {
            let (coverage, _) = super::panel_coverage(x as f32 + 0.5, y as f32 + 0.5, w as f32, h as f32, r);
            let a = (coverage * f32::from(alpha)).round() as u8;
            let at = ((y * w + x) * 4) as usize;
            for (i, shift) in [16, 8, 0].into_iter().enumerate() {
                pixels[at+i] = ((((rgb >> shift) & 255) * u32::from(a) + 127) / 255) as u8;
            }
            pixels[at+3] = a;
        }
    }
    IconPixels { width: w, height: h, pixels }
}
unsafe fn round(dc: HDC, r: RECT, rad: i32, rgb: u32, alpha: u8) {
    if r.right > r.left && r.bottom > r.top {
        let img = rounded_ink(r.right-r.left, r.bottom-r.top, rad, rgb, alpha);
        draw_icon(dc, &img, r.left, r.top);
    }
}
unsafe fn text(dc: HDC, s: &str, mut r: RECT, f: HFONT, ink: u32, center: bool) {
    let old = SelectObject(dc, f);
    SetTextColor(dc, ink);
    SetBkMode(dc, TRANSPARENT as i32);
    DrawTextW(
        dc,
        wide(s).as_ptr(),
        -1,
        &mut r,
        DT_SINGLELINE
            | DT_VCENTER
            | DT_END_ELLIPSIS
            | DT_NOPREFIX
            | if center { DT_CENTER } else { DT_LEFT },
    );
    SelectObject(dc, old);
}
unsafe fn text_alpha(dc:HDC,s:&str,r:RECT,f:HFONT,ink:u32,center:bool){
    if s.is_empty()||r.right<=r.left||r.bottom<=r.top||RectVisible(dc,&r)==0{return;}
    let (w,h)=(r.right-r.left,r.bottom-r.top);
    let Some(mut mask)=crate::dock::PopupPixels::new(w,h)else{return;};
    let area=rect(0,0,w,h);
    if !crate::native_text::draw(mask.dc(),area,s,f,[255;3],center,true) {
        text(mask.dc(),s,area,f,0x00ffffff,center);
    }
    GdiFlush();
    let coverage=mask.pixels().chunks_exact(4).map(|p|((u16::from(p[0])+u16::from(p[1])+u16::from(p[2])+1)/3) as u8).collect::<Vec<_>>();
    // A small separable Gaussian-like kernel, scaled with font DPI.
    // Shadow is independent of foreground glyph coverage.
    let radius=(h/18).clamp(1,3);
    let mut shadow=vec![0;w as usize*h as usize*4];
    let mut horizontal=vec![0u16;w as usize*h as usize];
    let weight=((radius+1)*(radius+1)) as u32;
    for y in 0..h{for x in 0..w{
        let sum=(-radius..=radius).filter_map(|dx|{
            let sx=x+dx;(0..w).contains(&sx).then(||u32::from(coverage[(y*w+sx) as usize])*(radius+1-dx.abs()) as u32)
        }).sum::<u32>();
        horizontal[(y*w+x) as usize]=sum as u16;
    }}
    for y in 0..h{for x in 0..w{
        let sum=(-radius..=radius).filter_map(|dy|{
            let sy=y+dy-radius;(0..h).contains(&sy).then(||u32::from(horizontal[(sy*w+x) as usize])*(radius+1-dy.abs()) as u32)
        }).sum::<u32>();
        let a=(sum*105/(weight*weight*255)) as u8;
        let i=((y*w+x)*4) as usize;shadow[i]=a/12;shadow[i+1]=a/24;shadow[i+3]=a;
    }}
    draw_icon(dc,&IconPixels{width:w,height:h,pixels:shadow},r.left,r.top);
    let mut pixels=vec![0;w as usize*h as usize*4];
    for (out,a) in pixels.chunks_exact_mut(4).zip(coverage){
        let a=u32::from(a);
        for (c,shift) in [16,8,0].into_iter().enumerate(){out[c]=((((ink>>shift)&255)*a+127)/255) as u8;}
        out[3]=a as u8;
    }
    draw_icon(dc,&IconPixels{width:w,height:h,pixels},r.left,r.top);
}
unsafe fn draw_icon(dc: HDC, icon: &IconPixels, x: i32, y: i32) {
    draw_icon_alpha(dc,icon,x,y,255);
}
unsafe fn draw_icon_alpha(dc:HDC,icon:&IconPixels,x:i32,y:i32,opacity:u8) {
    let mut info: BITMAPINFO = std::mem::zeroed();
    info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = icon.width;
    info.bmiHeader.biHeight = -icon.height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    let mut bits = ptr::null_mut();
    let bm = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0);
    if bm.is_null() {
        return;
    }
    let src = CreateCompatibleDC(dc);
    if !src.is_null() && !bits.is_null() {
        ptr::copy_nonoverlapping(icon.pixels.as_ptr(), bits.cast(), icon.pixels.len());
        let old = SelectObject(src, bm);
        AlphaBlend(
            dc,
            x,
            y,
            icon.width,
            icon.height,
            src,
            0,
            0,
            icon.width,
            icon.height,
            BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: opacity,
                AlphaFormat: AC_SRC_ALPHA as u8,
            },
        );
        SelectObject(src, old);
    }
    if !src.is_null() {
        DeleteDC(src);
    }
    DeleteObject(bm);
}

unsafe fn search_glyph(dc:HDC,r:RECT,ink:u32,dpi:u32){
    let size=px(24,dpi);
    let Some(mut layer)=crate::dock::PopupPixels::new(size,size)else{return;};
    layer.pixels().fill(0);
    let k=dpi as f32/96.0;
    layer.shape_ink(rect(0,0,size,size),[(ink>>16) as u8,(ink>>8) as u8,ink as u8],|x,y|{
        let (x,y)=(x/k,y/k);
        let ring=((x-10.0).hypot(y-10.0)-4.8).abs();
        let t=(((x-13.6)+(y-13.6))/8.8).clamp(0.0,1.0);
        let handle=(x-13.6-4.4*t).hypot(y-13.6-4.4*t);
        ((1.35-ring.min(handle))*k+0.5).clamp(0.0,1.0)*255.0
    });
    let pixels=layer.pixels().to_vec();
    draw_icon(dc,&IconPixels{width:size,height:size,pixels},r.left+px(15,dpi),(r.top+r.bottom-size)/2);
}

unsafe fn draw_live_edit(dc:HDC,edit:HWND,area:RECT,ink:u32,font:HFONT,high_contrast:bool){
    let (w,h)=(area.right-area.left,area.bottom-area.top);
    let query=read_edit(edit,256);
    let placeholder=query.is_empty()&&!EDIT_COMPOSING.get();
    let Some(mut raster)=crate::dock::PopupPixels::new(w,h)else{return;};
    let edc=raster.dc();
    // A black/white matte pair recovers native text coverage without treating
    // antialias fringes or background pixels as opaque input rectangles.
    // Selection and IME decoration that are independent of the matte survive.
    let print=|target:HDC,bg:u32|{
        fill(target,rect(0,0,w,h),bg);
        EDIT_PRINT.set(Some((bg,ink)));
        CallWindowProcW(EDIT_PROC.get(),edit,WM_PRINTCLIENT,target as WPARAM,PRF_CLIENT as LPARAM);
        EDIT_PRINT.set(None);
        GdiFlush();
    };
    if placeholder{
        text_alpha(dc,words("Tìm kiếm…","Search…","搜索…"),area,font,
            if high_contrast{ink}else{0x00d8d4cf},false);
    }else if let Some(mut white)=crate::dock::PopupPixels::new(w,h){
        print(edc,0);
        print(white.dc(),0x00ffffff);
        // If a native theme ignored WM_CTLCOLOR, never copy its opaque
        // rectangle. The corner is outside the single-line text/selection.
        let black_bg=raster.pixels()[0..3].to_vec();
        let white_bg=white.pixels()[0..3].to_vec();
        let valid_matte=black_bg.iter().all(|&c|c<12)&&white_bg.iter().all(|&c|c>243);
        for (black,light) in raster.pixels().chunks_exact_mut(4).zip(white.pixels().chunks_exact(4)){
            if valid_matte{
                let difference=(0..3).map(|i|light[i].saturating_sub(black[i]) as u16).sum::<u16>();
                let alpha=255-((difference+1)/3) as u8;
                for c in &mut black[..3]{*c=(*c).min(alpha);}
                black[3]=alpha;
            }else{
                // Unthemed EDIT normally honors the matte. On an unusual
                // theme recover only glyph contrast, never a painted panel.
                let alpha=(0..3).map(|i|black[i].abs_diff(black_bg[i])).max().unwrap_or(0);
                for (i,shift) in [16,8,0].into_iter().enumerate(){
                    black[i]=((((ink>>shift)&255)*u32::from(alpha)+127)/255) as u8;
                }
                black[3]=alpha;
            }
        }
        AlphaBlend(dc,area.left,area.top,w,h,edc,0,0,w,h,BLENDFUNCTION{
            BlendOp:AC_SRC_OVER as u8,BlendFlags:0,SourceConstantAlpha:255,AlphaFormat:AC_SRC_ALPHA as u8});
    }
    if GetFocus()==edit{
        let sel=SendMessageW(edit,0x00b0,0,0) as u32; // EM_GETSEL
        let start=sel&0xffff;
        if start==sel>>16{
            let pos=SendMessageW(edit,0x00d6,start as WPARAM,0); // EM_POSFROMCHAR
            let x=if pos!=-1{(pos as i16) as i32}else{
                let mut size:SIZE=std::mem::zeroed();
                let value=wide(&read_edit(edit,256));
                let old=SelectObject(edc,font);
                GetTextExtentPoint32W(edc,value.as_ptr(),value.len().saturating_sub(1) as i32,&mut size);
                SelectObject(edc,old);
                size.cx+1
            };
            let x=x.clamp(0,w-1);
            // A separate premultiplied caret, kept at the native character
            // position. GDI FillRect would zero the composition alpha.
            round(dc,rect(area.left+x,area.top+2,area.left+x+1,area.bottom-2),0,ink,255);
        }
    }
}

unsafe fn draw_search_box(dc:HDC,edit:HWND,width:i32,dpi:u32,dark:bool,high_contrast:bool,live:bool,font:HFONT){
    let u=|n|px(n,dpi);
    let search=search_rect(width,dpi);
    let field=edit_rect(width,dpi);
    let Palette{face,ink,secondary,..}=palette(dark,high_contrast);
    if high_contrast{round(dc,search,u(32),face,255);}
    else{
        round(dc,rect(search.left-u(2),search.top,search.right+u(2),search.bottom+u(4)),u(36),0x00151008,48);
        round(dc,search,u(32),0x00fcf8f5,if GetFocus()==edit{100}else{62});
        let inner=rect(search.left+1,search.top+1,search.right-1,search.bottom-1);
        round(dc,inner,u(30),face,if live{140}else if EDIT_COMPOSITE.get(){207}else{255});
    }
    search_glyph(dc,search,secondary,dpi);
    if live||EDIT_COMPOSITE.get(){draw_live_edit(dc,edit,field,ink,font,high_contrast);}
}
type PaintTile = (
    Tile,
    RECT,
    Option<Arc<IconPixels>>,
    Vec<Option<Arc<IconPixels>>>,
    bool,
);
fn request_icon(p: &mut Panel, path: PathBuf, size: i32) -> IconKey {
    let key = (path, size.clamp(8, 192));
    if !p.icons.contains_key(&key) {
        if let Some((icon,fresh))=lookup_raster(&key,true){
            p.icons.insert(key.clone(),icon);
            if !fresh&&p.pending.len()<32&&p.pending.insert(key.clone())&&p.icon_tx.send(key.clone()).is_err(){
                p.pending.remove(&key);
            }
            return key;
        }
    }
    if !key.0.as_os_str().is_empty()
        && !p.icons.contains_key(&key)
        && p.pending.len() < 32
        && p.pending.insert(key.clone())
        && p.icon_tx.send(key.clone()).is_err()
    {
        p.pending.remove(&key);
    }
    key
}
unsafe fn paint_full(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let dc = BeginPaint(hwnd, &mut ps);
    let size = PANEL.with(|p| p.borrow().as_ref().map(|p| (p.width,p.height,p.native.is_some())));
    let mut failed=false;
    if let Some((w,h,live)) = size {
        failed=FRAME.with(|cell| {
            let mut slot = cell.borrow_mut();
            let new = !slot.as_ref().is_some_and(|f| f.w == w && f.h == h);
            if new { *slot = Frame::new(dc,w,h); }
            if let Some(f) = slot.as_ref() {
                let dirty = if new { rect(0,0,w,h) } else { ps.rcPaint };
                let saved = SaveDC(f.dc);
                IntersectClipRect(f.dc,dirty.left,dirty.top,dirty.right,dirty.bottom);
                if live{
                    GdiFlush();
                    let clipped=rect(dirty.left.max(0),dirty.top.max(0),dirty.right.min(w),dirty.bottom.min(h));
                    if clipped.left<clipped.right{
                        for y in clipped.top..clipped.bottom{
                            let at=(y as usize*w as usize+clipped.left as usize)*4;
                            ptr::write_bytes(f.bits.add(at),0,(clipped.right-clipped.left) as usize*4);
                        }
                    }
                }
                render_full(hwnd, f.dc, dirty);
                RestoreDC(f.dc,saved);
                if live{
                    GdiFlush();
                    PatBlt(dc,ps.rcPaint.left,ps.rcPaint.top,ps.rcPaint.right-ps.rcPaint.left,
                        ps.rcPaint.bottom-ps.rcPaint.top,BLACKNESS);
                    return PANEL.with(|p|p.borrow_mut().as_mut().and_then(|p|p.native.as_mut())
                        .is_none_or(|native| match native.foreground.upload(f,dirty){
                            Ok(())=>false,
                            Err(e)=>{crate::log!("Spotlight foreground present: {e}");true}
                        }));
                }
                BitBlt(dc, ps.rcPaint.left, ps.rcPaint.top, ps.rcPaint.right-ps.rcPaint.left,
                       ps.rcPaint.bottom-ps.rcPaint.top, f.dc, ps.rcPaint.left, ps.rcPaint.top, SRCCOPY);
            } else if live {return true;} else { render_full(hwnd,dc,ps.rcPaint); }
            false
        });
    }
    EndPaint(hwnd, &ps);
    if failed{fallback_material(hwnd);}
}
unsafe fn render_full(hwnd: HWND, dc: HDC, dirty: RECT) {
    let view = PANEL.with(|s| {
        let mut s = s.borrow_mut();
        let p = s.as_mut()?;
        if p.icon_rev != crate::app_icons::revision() {
            p.icons.clear();
            p.pending.clear();
            (p.icon_tx, p.icon_rx) = icon_worker();
            p.icon_rev = crate::app_icons::revision();
        }
        let grid = grid(p);
        let start = p.page * grid.capacity();
        let visible = p
            .tiles
            .iter()
            .skip(start)
            .take(grid.capacity())
            .cloned()
            .collect::<Vec<_>>();
        // Only the visible page owns decoded rasters. Previous pages use the
        // bounded shared RAM/disk caches, not an extra overlay-wide cache.
        let visible_keys=visible.iter().flat_map(|t|{
            std::iter::once((t.entry.path.clone(),grid.icon.clamp(8,192)))
                .chain(t.preview.iter().map(|path|(path.clone(),(grid.icon/3).clamp(8,192))))
        }).collect::<std::collections::HashSet<_>>();
        p.icons.retain(|key,_|visible_keys.contains(key));
        let tiles = visible
            .into_iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let r = grid.tiles(i);
                let key = request_icon(p, t.entry.path.clone(), grid.icon);
                let preview_keys: Vec<_> = t
                    .preview
                    .iter()
                    .map(|path| request_icon(p, path.clone(), grid.icon / 3))
                    .collect();
                if !intersects(r,dirty) { return None; }
                let icon = p.icons.get(&key).cloned().flatten();
                let preview=preview_keys.iter().map(|k|p.icons.get(k).cloned().flatten()).collect();
                Some((i + start,(t, r, icon, preview, p.keyboard && p.selected == i + start)))
            })
            .collect::<Vec<(usize,PaintTile)>>();
        if p.pending.is_empty()&&p.page+1<count_pages(p){
            let next=p.tiles.iter().skip(start+grid.capacity()).take(8).map(|t|t.entry.path.clone()).collect::<Vec<_>>();
            for path in next{request_icon(p,path,grid.icon);}
        }
        let heading = LIBRARY.with(|l| {
            l.borrow()
                .folders
                .iter()
                .find(|f| f.id == p.folder)
                .map(|f| f.name.clone())
        });
        Some((
            p.width,
            p.height,
            p.dpi,
            p.dark,
            p.high_contrast,
            p.glass.clone(),
            p.native.is_some(),
            p.edit,
            p.font,
            p.search_font,
            grid.icon,
            tiles,
            controls(p),
            p.hover,
            p.hover_from,
            p.hover_started.map_or(1.0,|t|motion::ease(t.progress())),
            p.grid_started.map_or(255,|t|grid_opacity(t.progress())),
            p.pressed.map(|(h,_)|h),
            p.category,
            p.page,
            !p.pending.is_empty(),
            p.error.clone(),
            heading,
            p.hidden,
            p.catalog.is_some(),
            p.input.is_empty(),
        ))
    });
    if let Some((
        w,
        h,
        dpi,
        dark,
        high_contrast,
        glass,
        live,
        edit,
        font,
        search_font,
        size,
        tiles,
        buttons,
        hover,
        hover_from,
        hover_progress,
        grid_alpha,
        pressed,
        category,
        page,
        busy,
        error,
        heading,
        hidden,
        loading,
        empty_query,
    )) = view
    {
        let u = |n| px(n, dpi);
        let Palette{face,ink,secondary,active,fallback}=palette(dark,high_contrast);
        if !live&&glass.is_empty() { fill(dc, rect(0,0,w,h),fallback); }
        else if !live {
            let (bw,bh) = ((w/4).max(1),(h/4).max(1));
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = bw; info.bmiHeader.biHeight = -bh;
            info.bmiHeader.biPlanes = 1; info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            SetStretchBltMode(dc, HALFTONE);
            StretchDIBits(dc,0,0,w,h,0,0,bw,bh,glass.as_ptr().cast(),&info,DIB_RGB_COLORS,SRCCOPY);
        }
        let text:unsafe fn(HDC,&str,RECT,HFONT,u32,bool)=if high_contrast{text}else{text_alpha};
        let header=crate::native_text::font_pixels(u(17),550);
        let search=search_rect(w,dpi);
        let halo=rect(search.left-u(5),search.top-u(5),search.right+u(5),search.bottom+u(7));
        if intersects(halo,dirty){
            draw_search_box(dc,edit,w,dpi,dark,high_contrast,live,search_font);
        }
        for (action, r, s) in buttons {
            if !intersects(r,dirty) { continue; }
            if let Hit::Page(i)=action{
                let center=(r.left+r.right)/2;
                let y=(r.top+r.bottom)/2;
                let radius=(u(if i==page{4}else{3})).min((r.right-r.left).max(1)/2).max(1);
                round(dc,rect(center-radius,y-radius,center+radius,y+radius),radius,ink,if i==page{245}else{105});
                continue;
            }
            let selected = matches!(action,Hit::Tab(c) if c==category);
            let lift=hover_opacity(action,hover,hover_from,hover_progress);
            round(
                dc,
                r,
                u(12),
                if selected {
                    active
                } else {
                    face
                },
                if pressed == Some(action) { 230 } else if selected { 104 } else { (150.0+lift*65.0) as u8 },
            );
            text(dc, s, r, font, ink, true);
        }
        if hidden||heading.is_some(){
            let title=if hidden{words("Ứng dụng đã ẩn","Hidden apps","隐藏的应用")}else{heading.as_deref().unwrap_or_default()};
            text(dc,title,rect(w/3,u(133),w*2/3,u(156)),header,ink,true);
        }
        let names = OPTIONS.with(|o| o.borrow().show_names);
        for (i, (t, r, icon, preview, sel)) in &tiles {
            let lift=hover_opacity(Hit::Tile(*i),hover,hover_from,hover_progress);
            if *sel || lift>0.0 || pressed==Some(Hit::Tile(*i)) {
                round(dc, *r, u(30), active, if pressed == Some(Hit::Tile(*i)) { 98 } else if *sel { 66 } else { (46.0*lift) as u8 });
            }
            let ix = (r.left + r.right) / 2;
            let iy = r.top + ((r.bottom - r.top - size - u(16)) / 2).max(0);
            if let Some(img) = icon {
                draw_icon_alpha(dc, img, ix - img.width / 2, iy + (size - img.height) / 2,grid_alpha);
            } else {
                let badge = rect(ix - size / 2, iy, ix + (size + 1) / 2, iy + size);
                round(
                    dc,
                    badge,
                    u(18),
                    if t.entry.kind == "Folder" {
                        active
                    } else {
                        face
                    }, 140,
                );
                let letter = if t.entry.kind == "Folder" {
                    "▦".into()
                } else {
                    t.entry.name.chars().take(1).collect::<String>()
                };
                if preview.iter().any(Option::is_some) {
                    for (i, icon) in preview.iter().enumerate() {
                        if let Some(icon) = icon {
                            let quarter = size / 4;
                            let x =
                                badge.left + quarter + (i % 2) as i32 * (size / 2) - icon.width / 2;
                            let y =
                                badge.top + quarter + (i / 2) as i32 * (size / 2) - icon.height / 2;
                            draw_icon_alpha(dc, icon, x, y,grid_alpha);
                        }
                    }
                } else {
                    text(dc, &letter, badge, header, ink, true);
                }
            }
            if names || !empty_query {
                let last = (iy + size + u(24)).min(r.bottom);
                text(
                    dc,
                    &t.entry.name,
                    rect(r.left + u(5), last - u(23), r.right - u(5), last),
                    font,
                    ink,
                    true,
                );
            }
        }
        if PANEL.with(|p|p.borrow().as_ref().is_some_and(|p|p.tiles.is_empty())) {
            let message = if loading {
                "Đang tải ứng dụng…"
            } else {
                "Không có mục phù hợp"
            };
            text(
                dc,
                message,
                rect(0, h / 2, w, h / 2 + u(38)),
                header,
                secondary,
                true,
            );
        }
        if !error.is_empty(){
            text(dc,&error,rect(u(12),h-u(27),w-u(12),h-u(3)),font,secondary,true);
        }
        if !header.is_null() {
            DeleteObject(header);
        }
        if busy {
            SetTimer(hwnd, POLL, 45, None);
        }
    }
}

fn show_error(hwnd: HWND, message: &str) {
    PANEL.with(|p| {
        if let Some(p) = p.borrow_mut().as_mut() {
            p.error = message.into();
        }
    });
    refresh(hwnd);
}
fn change(hwnd: HWND, apply: impl FnOnce(&mut Library) -> Result<(), &'static str>) {
    let mut next = LIBRARY.with(|l| l.borrow().clone());
    if let Err(message) = apply(&mut next) {
        show_error(hwnd, message);
        return;
    }
    if !cfg!(test) {
        if let Err(error) = crate::config_store::save(
            &crate::rust_settings_path(),
            serde_json::json!({"spotlightLibrary":&next}),
        ) {
            show_error(hwnd, "Không lưu được thay đổi vào hồ sơ TLauncher.");
            crate::log!(
                "Spotlight library: settings write failed ({:?})",
                error.kind()
            );
            return;
        }
    }
    initialize(next);
    rebuild(hwnd, false);
}
pub(super) fn reset(hwnd: HWND) {
    unsafe {
        MODAL.set(true);
        let result = MessageBoxW(
            hwnd,
            wide(
                "Đặt lại thứ tự và thư mục? Ứng dụng tự thêm, tên, icon và mục đã ẩn vẫn được giữ.",
            )
            .as_ptr(),
            wide("TLauncher Spotlight").as_ptr(),
            MB_OKCANCEL | MB_ICONQUESTION,
        );
        MODAL.set(false);
        if result == IDOK {
            change(hwnd, |l| {
                l.reset_layout();
                Ok(())
            });
        }
    }
}

pub(super) fn refresh_catalog(hwnd: HWND) {
    PANEL.with(|s| {
        if let Some(p) = s.borrow_mut().as_mut() {
            p.catalog = catalog::request(true, true);
        }
    });
    unsafe {
        SetTimer(hwnd, POLL, 40, None);
    }
}
pub(super) fn update_pins() {
    if let Some(hwnd) = window() {
        rebuild(hwnd, false);
    }
}

unsafe fn surface_style(hwnd: HWND, dark: bool) {
    // No extended client glass: GDI does not supply a reliable alpha channel
    // for DWM glass. Use the same captured/frosted material as Compact.
    let attr = |id: DWMWINDOWATTRIBUTE, value: i32| {
        DwmSetWindowAttribute(hwnd, id as u32, (&value as *const i32).cast(), 4)
    };
    let _ = attr(DWMWA_USE_IMMERSIVE_DARK_MODE, i32::from(dark));
    let _ = attr(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND);
    let _ = attr(DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_NONE);
}

pub(super) fn high_contrast()->bool {
    if cfg!(test){return false;}
    use windows_sys::Win32::UI::Accessibility::{HIGHCONTRASTW,HCF_HIGHCONTRASTON};
    unsafe {
        let mut hc: HIGHCONTRASTW = std::mem::zeroed();
        hc.cbSize = std::mem::size_of::<HIGHCONTRASTW>() as u32;
        SystemParametersInfoW(SPI_GETHIGHCONTRAST,hc.cbSize,(&mut hc as *mut HIGHCONTRASTW).cast(),0)!=0
            && hc.dwFlags&HCF_HIGHCONTRASTON!=0
    }
}
fn transparency_allowed() -> bool {
    if cfg!(test) { return true; }
    use windows_sys::Win32::System::Registry::HKEY_CURRENT_USER;
    if high_contrast(){return false;}
    crate::lock::reg_get_dword(HKEY_CURRENT_USER,r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize","EnableTransparency") != Some(0)
}

fn frosted(raw: &[u8], w:i32,h:i32,dpi:u32,dark:bool, cfg:&Preferences) -> Vec<u8> {
    if raw.is_empty() { return Vec::new(); }
    let mut glass=super::frost(raw,w,h,dpi as f32/96.0,&Appearance {
        theme: "dark".into(),
        transparency: if dark{0.40}else{0.62}+cfg.glass_strength.clamp(20,48) as f32*0.001,
        blur: cfg.glass_blur.clamp(24,64) as f32,
    });
    // Preserve hue while bounding the extreme brightness of a desktop sample.
    // Small labels must remain readable even on pure-white/black wallpaper.
    for pixel in glass.chunks_exact_mut(4) {
        if dark {
            let max=*pixel[..3].iter().max().unwrap() as u32;
            if max>94 {for c in &mut pixel[..3]{*c=(*c as u32*94/max) as u8;}}
        } else {
            let max=*pixel[..3].iter().max().unwrap() as u32;
            if max>117 {for c in &mut pixel[..3]{*c=(*c as u32*117/max) as u8;}}
        }
    }
    glass
}

unsafe fn sample_backdrop(r:RECT)->Vec<u8> {
    let w=r.right-r.left;
    let h=r.bottom-r.top;
    if cfg!(test) {
        let (bw,bh)=((w/4).max(1),(h/4).max(1));
        let mut raw=Vec::with_capacity((bw*bh*4) as usize);
        for y in 0..bh { for x in 0..bw {
            raw.extend_from_slice(&[ (60+170*x/bw) as u8,(110+110*y/bh) as u8,(220-180*x/bw) as u8,255 ]);
        } }
        return raw;
    }
    capture_glass(r.left,r.top,w,h)
}

fn refresh_material(hwnd:HWND) {
    let allowed=transparency_allowed();
    PANEL.with(|cell| {
        if let Some(p)=cell.borrow_mut().as_mut() {
            if let Some(native)=&p.native {
                unsafe{native.backdrop.material(p.dark,OPTIONS.with(|o|o.borrow().glass_strength));}
                p.glass=Vec::<u8>::new().into();
                return;
            }
            p.glass=if allowed { OPTIONS.with(|o| frosted(&p.backdrop,p.width,p.height,p.dpi,p.dark,&o.borrow())).into() }
                else { Vec::<u8>::new().into() };
        }
    });
    refresh(hwnd);
}

fn ensure_live(hwnd:HWND)->bool{
    #[cfg(test)]
    if !LIVE_SEARCH_FIXTURE.get(){return false;}
    if !transparency_allowed(){return false;}
    let flags=PANEL.with(|p|p.borrow().as_ref().map(|p|(p.native.is_some(),p.native_rejected)));
    match flags{
        Some((true,_))=>return true,
        Some((false,false))=>{},
        _=>return false,
    }
    let result=unsafe{
        if !set_redirection_alpha(hwnd,true){Err("redirection alpha is unavailable".to_owned())}
        else{(move || {
            let backdrop=NativeBackdrop::create(BackdropKind::Host)?;
            let foreground=Foreground::new(hwnd).map_err(|e|e.to_string())?;
            Ok(LiveMaterial{backdrop,foreground})
        })()}
    };
    match result{
        Ok(native)=>{
            PANEL.with(|p|if let Some(panel)=p.borrow_mut().as_mut(){panel.native=Some(native);});
            crate::log!("Spotlight material: HostBackdrop configured (visual output depends on DWM)");
            true
        }
        Err(e)=>{
            reset_graphics_factories();
            set_redirection_alpha(hwnd,false);
            PANEL.with(|p|if let Some(panel)=p.borrow_mut().as_mut(){panel.native_rejected=true;});
            crate::log!("Spotlight material: compatibility fallback ({e})");
            false
        }
    }
}
fn fallback_material(hwnd:HWND){
    let old=PANEL.with(|p|{
        let mut panel=p.borrow_mut();let panel=panel.as_mut()?;
        panel.native_rejected=true;
        panel.transition=None;
        // Force a one-shot compatibility capture without sampling ourselves.
        panel.bounds=rect(0,0,0,0);
        Some((panel.point,panel.native.take()))
    });
    if let Some((point,native))=old{
        drop(native);
        reset_graphics_factories();
        set_redirection_alpha(hwnd,false);
        unsafe{KillTimer(hwnd,TRANSITION_TIMER);place(hwnd,point);}
    }
}

fn estimate(point: POINT) -> Option<(RECT, u32)> {
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    unsafe {
        let mon = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut mi) == 0 {
            return None;
        }
        let mut x = 96;
        let mut y = 96;
        let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut x, &mut y);
        Some((mi.rcMonitor, x.clamp(72, 288)))
    }
}
unsafe fn place(hwnd: HWND, point: POINT) {
    let Some((r, dpi)) = estimate(point) else {
        return;
    };
    let (width, height) = (r.right - r.left, r.bottom - r.top);
    if width <= 0 || height <= 0 {
        return;
    }
    let changed = PANEL.with(|p| p.borrow().as_ref().is_some_and(|p| {
        (p.bounds.left,p.bounds.top,p.bounds.right,p.bounds.bottom) != (r.left,r.top,r.right,r.bottom)
    }));
    let mut raw = None;
    let visible = IsWindowVisible(hwnd)!=0;
    let live=ensure_live(hwnd);
    if changed&&!live {
        // Hide only our own overlay before capture. Never capture it back into
        // itself on monitor change. Take one low-resolution sample, no polling.
        let was_modal = MODAL.replace(true);
        if visible { ShowWindow(hwnd,SW_HIDE); }
        raw=Some(if transparency_allowed() { sample_backdrop(r) } else { Vec::new() });
        MODAL.set(was_modal);
    }
    let Some((edit,old_font,old_search_font)) = PANEL.with(|p| {
        let mut p = p.borrow_mut();
        let p = p.as_mut()?;
        let old = p.font;
        p.point = point;
        p.dpi = dpi;
        p.width = width;
        p.height = height;
        p.bounds = r;
        if let Some(raw)=raw { p.backdrop=raw; }
        p.font=crate::native_text::font_pixels(px(15,dpi),500);
        let old_search=p.search_font;
        p.search_font=crate::native_text::font_pixels(px(16,dpi),500);
        Some((p.edit,old,old_search))
    }) else {
        return;
    };
    let field=edit_rect(width,dpi);
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        r.left,
        r.top,
        width,
        height,
        SWP_NOACTIVATE,
    );
    let backdrop=PANEL.with(|p|p.borrow().as_ref().and_then(|p|p.native.as_ref().map(|n|n.backdrop.hwnd)));
    if let Some(backdrop)=backdrop{
        SetWindowPos(backdrop,hwnd,r.left,r.top,width,height,SWP_NOACTIVATE|if visible{SWP_SHOWWINDOW}else{0});
    }
    SetWindowPos(
        edit,
        ptr::null_mut(),
        field.left,field.top,field.right-field.left,field.bottom-field.top,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
    if let Some(font) = PANEL.with(|p| p.borrow().as_ref().map(|s| s.search_font)) {
        SendMessageW(edit, WM_SETFONT, font as WPARAM, 0);
    }
    if !old_font.is_null() {
        DeleteObject(old_font);
    }
    if !old_search_font.is_null(){DeleteObject(old_search_font);}
    refresh_material(hwnd);
    rebuild(hwnd, false);
    if changed && visible&&!live { ShowWindow(hwnd,SW_SHOW); SetForegroundWindow(hwnd); SetFocus(edit); }
}

pub(super) fn dismiss(restore: bool) {
    let Some(mut p) = PANEL.with(|s| s.borrow_mut().take()) else {
        return;
    };
    unsafe {
        if GetCapture() == p.hwnd {
            ReleaseCapture();
        }
        KillTimer(p.hwnd, POLL);
        KillTimer(p.hwnd, SEARCH);
        KillTimer(p.hwnd, SHELL_REFRESH);
        KillTimer(p.hwnd,HOVER_ANIMATION);
        KillTimer(p.hwnd,GRID_ANIMATION);
        KillTimer(p.hwnd,TRANSITION_TIMER);
        drop(p.watch.take());
        drop(p.native.take());
        DestroyWindow(p.hwnd);
        EDIT_COMPOSITE.set(false);
        EDIT_COMPOSING.set(false);
        FRAME.with(|frame| { frame.borrow_mut().take(); });
        crate::native_text::release();
        if !p.font.is_null() {
            DeleteObject(p.font);
        }
        if !p.search_font.is_null(){DeleteObject(p.search_font);}
        if restore && !p.prior.is_null() && IsWindow(p.prior) != 0 {
            SetForegroundWindow(p.prior);
        }
    }
    if crate::trial::active() {
        let (ram_hits,disk_hits,extractions,ram_bytes,disk_bytes)=crate::app_icons::cache_metrics();
        crate::log!("Spotlight icon cache: source RAM hits={ram_hits}, disk hits={disk_hits}, shell extractions={extractions}, source RAM bytes={ram_bytes}, disk bytes={disk_bytes}");
    }
}

fn start_transition(hwnd:HWND,open:bool)->bool{
    let transition=PANEL.with(|cell|{
        let mut panel=cell.borrow_mut();let p=panel.as_mut()?;
        if !p.motion||!motion::allowed(){return None;}
        let native=p.native.as_ref()?;
        let t=Transition::new(open,motion::FULL,p.transition);
        if unsafe{native.foreground.motion(t,p.width,px(66,p.dpi))}.is_err(){return None;}
        let _=native.backdrop.transition(t);
        let t=t.restart_clock();
        p.transition=Some(t);
        Some(t)
    });
    if let Some(t)=transition{
        if unsafe{SetTimer(hwnd,TRANSITION_TIMER,t.duration+20,None)}==0{settle_transition(hwnd);}
        true
    }else{false}
}
fn request_close(){
    let Some(hwnd)=window()else{return;};
    let closing=PANEL.with(|p|p.borrow().as_ref().and_then(|p|p.transition).is_some_and(|t|t.to==0.0));
    if !closing&&!start_transition(hwnd,false){dismiss(true);}
}
fn settle_transition(hwnd:HWND){
    let closing=PANEL.with(|s|{
        let mut p=s.borrow_mut();
        let Some(p)=p.as_mut()else{return false;};
        let Some(t)=p.transition.take()else{return false;};
        if t.to!=0.0{
            if let Some(n)=&p.native{
                let _=unsafe{n.foreground.settle()};
                n.backdrop.settle();
            }
        }
        t.to==0.0
    });
    unsafe{KillTimer(hwnd,TRANSITION_TIMER);}
    if closing{dismiss(true);}
}
pub(super) fn toggle_visible()->bool{
    let Some(hwnd)=window()else{return false;};
    let closing=PANEL.with(|p|p.borrow().as_ref().and_then(|p|p.transition).is_some_and(|t|t.to==0.0));
    if closing {
        if !start_transition(hwnd,true){dismiss(true);}
    }else{request_close();}
    true
}

pub(super) fn compact_query() -> Option<(POINT, String, HWND)> {
    PANEL.with(|p| {
        p.borrow()
            .as_ref()
            .map(|p| (p.point, p.input.clone(), p.prior))
    })
}

fn safe_launch(entry: &Entry) -> bool {
    if entry.kind == "Folder" {
        return false;
    }
    let value = entry.path.to_string_lossy();
    if crate::dock::command_pin::decode(&value).is_some() {
        return true;
    }
    if apps_folder_parsing_name(&value).is_some() {
        return app_entry(entry);
    }
    entry.path.is_absolute()
        || matches!(
            value.as_ref(),
            "mspaint.exe"
                | "notepad.exe"
                | "calc.exe"
                | "explorer.exe"
                | "ms-settings:"
                | "Taskmgr.exe"
                | "control.exe"
        )
}

unsafe fn open_entry(hwnd: HWND, entry: &Entry, elevated: bool) {
    if !safe_launch(entry) {
        show_error(hwnd, "Mục này không có đường dẫn mở hợp lệ.");
        return;
    }
    MODAL.set(true);
    ShowWindow(hwnd, SW_HIDE);
    let result = if elevated {
        #[cfg(test)]
        {
            33
        }
        #[cfg(not(test))]
        {
            ShellExecuteW(
                hwnd,
                wide("runas").as_ptr(),
                wide(&entry.path.to_string_lossy()).as_ptr(),
                ptr::null(),
                ptr::null(),
                SW_SHOWNORMAL,
            ) as isize
        }
    } else {
        execute(hwnd, &entry.path)
    };
    MODAL.set(false);
    if result > 32 {
        dismiss(false);
    } else {
        ShowWindow(hwnd, SW_SHOW);
        if let Some(edit) = PANEL.with(|p| p.borrow().as_ref().map(|p| p.edit)) {
            SetFocus(edit);
        }
        show_error(hwnd, &format!("Windows không mở được mục này ({result})."));
    }
}

unsafe fn menu(hwnd: HWND, point: POINT, labels: &[&str]) -> usize {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return 0;
    }
    for (i, label) in labels.iter().enumerate() {
        AppendMenuW(menu, MF_STRING, i + 1, wide(label).as_ptr());
    }
    let mut at = point;
    ClientToScreen(hwnd, &mut at);
    let was_modal=MODAL.replace(true);
    let dark=PANEL.with(|p|p.borrow().as_ref().is_some_and(|p|p.dark));
    let result=crate::dock::glass_menu::track(menu,hwnd,at,TPM_RETURNCMD|TPM_RIGHTBUTTON|TPM_NONOTIFY,dark,40) as usize;
    MODAL.set(was_modal);
    DestroyMenu(menu);
    result
}

unsafe fn pick(hwnd: HWND, filter: &str, title: &str) -> Option<PathBuf> {
    let mut path = [0u16; 32768];
    let filter = wide(filter);
    let title = wide(title);
    let mut dialog: OPENFILENAMEW = std::mem::zeroed();
    dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner = hwnd;
    dialog.lpstrFilter = filter.as_ptr();
    dialog.lpstrTitle = title.as_ptr();
    dialog.lpstrFile = path.as_mut_ptr();
    dialog.nMaxFile = path.len() as u32;
    dialog.Flags =
        OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_HIDEREADONLY | OFN_EXPLORER;
    MODAL.set(true);
    let ok = GetOpenFileNameW(&mut dialog) != 0;
    MODAL.set(false);
    ok.then(|| {
        PathBuf::from(String::from_utf16_lossy(
            &path[..path.iter().position(|&c| c == 0).unwrap_or(path.len())],
        ))
    })
}

struct Prompt<'a> {
    title: &'a str,
    fields: &'a [(&'a str, &'a str)],
    values: Vec<String>,
}
unsafe extern "system" fn prompt_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> isize {
    const FIRST: i32 = 610;
    match msg {
        WM_INITDIALOG => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, l);
            let data = &*(l as *const Prompt<'_>);
            SetWindowTextW(hwnd, wide(data.title).as_ptr());
            let mut r: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut r);
            let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd).clamp(72, 288);
            let n = |n| px(n, dpi);
            let h = GetModuleHandleW(ptr::null());
            for (i, (name, value)) in data.fields.iter().enumerate() {
                let y = n(14 + i as i32 * 58);
                let label = CreateWindowExW(
                    0,
                    wide("STATIC").as_ptr(),
                    wide(name).as_ptr(),
                    WS_CHILD | WS_VISIBLE,
                    n(14),
                    y,
                    r.right - n(28),
                    n(18),
                    hwnd,
                    ptr::null_mut(),
                    h,
                    ptr::null_mut(),
                );
                let edit = CreateWindowExW(
                    WS_EX_CLIENTEDGE,
                    wide("EDIT").as_ptr(),
                    wide(value).as_ptr(),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32,
                    n(14),
                    y + n(19),
                    r.right - n(28),
                    n(26),
                    hwnd,
                    (FIRST + i as i32) as HMENU,
                    h,
                    ptr::null_mut(),
                );
                for child in [label, edit] {
                    SendMessageW(
                        child,
                        WM_SETFONT,
                        GetStockObject(DEFAULT_GUI_FONT) as WPARAM,
                        1,
                    );
                }
                SendMessageW(edit, 0x00c5, 4096, 0);
            }
            let y = n(14 + data.fields.len() as i32 * 58);
            for (id, name, x, style) in [
                (IDOK, "Lưu", r.right - n(194), BS_DEFPUSHBUTTON),
                (IDCANCEL, "Hủy", r.right - n(100), BS_PUSHBUTTON),
            ] {
                let button = CreateWindowExW(
                    0,
                    wide("BUTTON").as_ptr(),
                    wide(name).as_ptr(),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP | style as u32,
                    x,
                    y,
                    n(86),
                    n(28),
                    hwnd,
                    id as HMENU,
                    h,
                    ptr::null_mut(),
                );
                SendMessageW(
                    button,
                    WM_SETFONT,
                    GetStockObject(DEFAULT_GUI_FONT) as WPARAM,
                    1,
                );
            }
            let edit = GetDlgItem(hwnd, FIRST);
            SetFocus(edit);
            SendMessageW(edit, 0x00b1, 0, -1);
            0
        }
        WM_COMMAND if w & 0xffff == IDOK as usize => {
            let value = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            if value != 0 {
                let data = &mut *(value as *mut Prompt<'_>);
                data.values = (0..data.fields.len())
                    .map(|i| read_edit(GetDlgItem(hwnd, FIRST + i as i32), 4096))
                    .collect();
            }
            EndDialog(hwnd, 1);
            1
        }
        WM_CLOSE | WM_COMMAND if msg == WM_CLOSE || w & 0xffff == IDCANCEL as usize => {
            EndDialog(hwnd, 0);
            1
        }
        _ => 0,
    }
}
unsafe fn prompt(hwnd: HWND, title: &str, fields: &[(&str, &str)]) -> Option<Vec<String>> {
    // Standard empty dialog template; children use native Edit controls and
    // Windows tab/IME handling. Keep the dialog owned by the library window.
    #[repr(align(4))]
    struct Template([u16; 12]);
    let style = WS_POPUP | WS_CAPTION | WS_SYSMENU | DS_MODALFRAME as u32 | DS_CENTER as u32;
    let h = 42 + fields.len() as u16 * 37;
    let template = Template([
        style as u16,
        (style >> 16) as u16,
        0,
        0,
        0,
        0,
        0,
        300,
        h,
        0,
        0,
        0,
    ]);
    let mut ctx = Prompt {
        title,
        fields,
        values: Vec::new(),
    };
    MODAL.set(true);
    let result = DialogBoxIndirectParamW(
        GetModuleHandleW(ptr::null()),
        template.0.as_ptr().cast(),
        hwnd,
        Some(prompt_proc),
        (&mut ctx as *mut Prompt<'_>) as LPARAM,
    );
    MODAL.set(false);
    (result == 1).then_some(ctx.values)
}
unsafe fn read_edit(hwnd: HWND, limit: usize) -> String {
    let n = (GetWindowTextLengthW(hwnd).max(0) as usize).min(limit);
    let mut value = vec![0u16; n + 1];
    let got = GetWindowTextW(hwnd, value.as_mut_ptr(), value.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&value[..got])
}
unsafe fn add_app(hwnd: HWND) {
    let Some(path) = pick(
        hwnd,
        "Ứng dụng hoặc shortcut\0*.exe;*.lnk\0\0",
        "Thêm ứng dụng vào Spotlight",
    ) else {
        return;
    };
    let name = path.file_stem().and_then(|v| v.to_str()).unwrap_or("");
    let Some(values) = prompt(
        hwnd,
        "Thêm ứng dụng",
        &[
            ("Tên hiển thị", name),
            ("Đối số EXE (tùy chọn)", ""),
            ("Thư mục làm việc đầy đủ (tùy chọn)", ""),
        ],
    ) else {
        return;
    };
    change(hwnd, |lib| {
        lib.add(&path, &values[0], &values[1], &values[2])
    });
}

fn rewrite_state(hwnd: HWND, f: impl FnOnce(&mut Panel)) {
    PANEL.with(|s| {
        if let Some(p) = s.borrow_mut().as_mut() {
            f(p);
            p.error.clear();
        }
    });
    rebuild(hwnd, true);
}
fn internal_view(hwnd: HWND, folder: u64, hidden: bool) {
    rewrite_state(hwnd, |p| {
        p.folder = folder;
        p.hidden = hidden;
        p.category = Category::All;
    });
    start_grid_motion(hwnd);
}
unsafe fn choose_folder(hwnd: HWND, id: &str, at: POINT) {
    let folders = LIBRARY.with(|l| l.borrow().folders.clone());
    let names = std::iter::once("Thư viện chính")
        .chain(folders.iter().map(|f| f.name.as_str()))
        .collect::<Vec<_>>();
    let i = menu(hwnd, at, &names);
    if i > 0 {
        change(hwnd, |l| {
            l.move_to(id, if i == 1 { 0 } else { folders[i - 2].id });
            Ok(())
        });
    }
}

unsafe fn library_menu(hwnd:HWND,point:POINT){
    let Some((editing,hidden,nested))=PANEL.with(|p|p.borrow().as_ref().map(|p|(p.editing,p.hidden,p.folder!=0||p.hidden)))else{return;};
    let mut rows=vec![
        (Hit::Add,words("Thêm ứng dụng…","Add application…","添加应用…")),
        (Hit::Edit,if editing{words("Xong","Done","完成")}else{words("Chỉnh sửa bố cục","Edit layout","编辑布局")}),
        (Hit::Create,words("Tạo thư mục…","New folder…","新建文件夹…")),
        (Hit::Refresh,words("Làm mới thư viện","Refresh library","刷新应用库")),
    ];
    if !hidden{rows.push((Hit::Hidden,words("Quản lý ứng dụng đã ẩn","Manage hidden apps","管理隐藏应用")));}
    if nested{rows.push((Hit::Back,words("Về thư viện","Back to library","返回应用库")));}
    rows.push((Hit::Settings,words("Cài đặt Spotlight","Spotlight settings","Spotlight 设置")));
    rows.push((Hit::Compact,words("Thu gọn Spotlight","Show compact Spotlight","切换到精简 Spotlight")));
    let labels=rows.iter().map(|(_,label)|*label).collect::<Vec<_>>();
    let id=menu(hwnd,point,&labels);
    if let Some((action_id,_))=id.checked_sub(1).and_then(|i|rows.get(i)){
        action(hwnd,*action_id);
    }
}

unsafe fn tile_menu(hwnd: HWND, i: usize, point: POINT) {
    let Some((tile, hidden)) = PANEL.with(|s| {
        let s = s.borrow();
        let p = s.as_ref()?;
        Some((p.tiles.get(i)?.clone(), p.hidden))
    }) else {
        return;
    };
    if let TileKey::Folder(folder) = tile.key {
        match menu(
            hwnd,
            point,
            &[
                "Mở thư mục",
                "Đổi tên thư mục",
                "Xóa thư mục (giữ ứng dụng)",
            ],
        ) {
            1 => internal_view(hwnd, folder, false),
            2 => {
                if let Some(v) = prompt(
                    hwnd,
                    "Đổi tên thư mục",
                    &[("Tên thư mục", &tile.entry.name)],
                ) {
                    change(hwnd, |l| {
                        l.rename_folder(folder, &v[0]);
                        Ok(())
                    });
                }
            }
            3 => change(hwnd, |l| {
                l.delete_folder(folder);
                Ok(())
            }),
            _ => {}
        }
        return;
    }
    let TileKey::App(id) = tile.key else {
        return;
    };
    let entry = tile.entry;
    if !app_entry(&entry) {
        if menu(hwnd, point, &["Mở"]) == 1 {
            open_entry(hwnd, &entry, false);
        }
        return;
    }
    let (manual, favorite) = LIBRARY.with(|lib| {
        let l = lib.borrow();
        (l.is_manual(&id), l.favorite(&id))
    });
    let mut options = vec![(1, "Mở ứng dụng")];
    let at_file = entry.path.is_absolute();
    if at_file
        && entry
            .path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("exe"))
    {
        options.push((2, "Chạy với quyền quản trị…"));
    }
    let pin = pin_change(&entry);
    options.push((
        3,
        if matches!(&pin, Some(crate::dock::Edit::Unpin(_))) {
            "Bỏ ghim khỏi Dock"
        } else {
            "Ghim vào Dock"
        },
    ));
    options.push((
        4,
        if favorite {
            "Bỏ yêu thích"
        } else {
            "Thêm vào yêu thích"
        },
    ));
    options.push((5, "Chuyển vào thư mục…"));
    options.push((6, "Đổi tên hiển thị…"));
    options.push((7, "Đổi biểu tượng…"));
    if at_file || crate::dock::command_pin::decode(&entry.path.to_string_lossy()).is_some() {
        options.push((8, "Mở vị trí tệp"));
    }
    options.push((
        9,
        if hidden {
            "Khôi phục vào Spotlight"
        } else {
            "Ẩn khỏi Spotlight"
        },
    ));
    if manual {
        options.push((10, "Bỏ mục tự thêm (giữ tệp gốc)"));
    }
    let labels = options.iter().map(|(_, l)| *l).collect::<Vec<_>>();
    let action = menu(hwnd, point, &labels)
        .checked_sub(1)
        .and_then(|n| options.get(n))
        .map_or(0, |o| o.0);
    match action {
        1 => open_entry(hwnd, &entry, false),
        2 => open_entry(hwnd, &entry, true),
        3 => {
            if let Some(pin) = pin {
                PIN_EDIT.with(|c| c.replace(Some(pin)));
            }
        }
        4 => change(hwnd, |l| {
            l.toggle_favorite(&id);
            Ok(())
        }),
        5 => choose_folder(hwnd, &id, point),
        6 => {
            if let Some(v) = prompt(hwnd, "Đổi tên hiển thị", &[("Tên ứng dụng", &entry.name)])
            {
                change(hwnd, |l| {
                    l.rename(&id, &v[0]);
                    Ok(())
                });
            }
        }
        7 => choose_icon(hwnd, &entry.path),
        8 => {
            let target = crate::dock::command_pin::decode(&entry.path.to_string_lossy())
                .map_or_else(|| entry.path.clone(), |c| PathBuf::from(c.exe));
            if let Some(dir) = target.parent() {
                if dir.is_absolute() {
                    let _ = execute(hwnd, dir);
                }
            }
        }
        9 => change(hwnd, |l| {
            l.hide(&id, !hidden);
            Ok(())
        }),
        10 => change(hwnd, |l| {
            l.remove_manual(&id);
            Ok(())
        }),
        _ => {}
    }
}
unsafe fn choose_icon(hwnd: HWND, path: &Path) {
    let Some(key) = crate::app_icons::key(&path.to_string_lossy()) else {
        return;
    };
    let Some(file) = pick(
        hwnd,
        "Ảnh\0*.png;*.jpg;*.jpeg;*.ico;*.webp;*.bmp\0\0",
        "Đổi biểu tượng ứng dụng",
    ) else {
        return;
    };
    match crate::app_icons::import(&file) {
        Ok(file) => {
            PIN_EDIT.with(|e| {
                e.replace(Some(crate::dock::Edit::AppIcon {
                    key,
                    file: Some(file),
                }))
            });
        }
        Err(e) => show_error(hwnd, &e),
    }
}

unsafe fn action(hwnd: HWND, which: Hit) {
    match which {
        Hit::Search => {
            if let Some(edit)=PANEL.with(|p|p.borrow().as_ref().map(|p|p.edit)){SetFocus(edit);}
        }
        Hit::Backdrop => {
            if MODAL.get(){return;}
            let nested=PANEL.with(|s|s.borrow().as_ref().map(|p|(p.editing,p.folder!=0||p.hidden)));
            match nested{Some((true,_))=>{},Some((false,true))=>internal_view(hwnd,0,false),Some((false,false))=>request_close(),None=>{}}
        }
        Hit::Compact => {
            if let Some((point, query, prior)) = compact_query() {
                dismiss(false);
                super::toggle_at(Some(point), Some(query), Some(prior));
            }
        }
        Hit::Add => add_app(hwnd),
        Hit::Refresh => refresh_catalog(hwnd),
        Hit::Settings => {
            dismiss(false);
            let owner=FindWindowW(wide(crate::MSG_CLASS).as_ptr(),ptr::null());
            if !owner.is_null(){PostMessageW(owner,crate::WM_OPEN_SETTINGS,0,0);}
        }
        Hit::Edit => rewrite_state(hwnd, |p| p.editing = !p.editing),
        Hit::Hidden => show_hidden(hwnd),
        Hit::Back => internal_view(hwnd, 0, false),
        Hit::Create => {
            if let Some(v) = prompt(hwnd, "Thư mục mới", &[("Tên thư mục", "")]) {
                change(hwnd, |lib| {
                    lib.create_folder(&v[0])
                        .map(|_| ())
                        .ok_or("Nhập tên thư mục hợp lệ.")
                });
            }
        }
        Hit::Tab(category) => rewrite_state(hwnd, |p| {
            p.category = category;
            p.folder = 0;
            p.hidden = false;
        }),
        Hit::Page(page) => set_page(hwnd, page),
        Hit::Tile(i) => {
            let Some((tile, editing)) = PANEL.with(|s| {
                let s = s.borrow();
                let p = s.as_ref()?;
                if !p.search_settled { return None; }
                Some((p.tiles.get(i)?.clone(), p.editing))
            }) else {
                return;
            };
            match tile.key {
                TileKey::Folder(id) => internal_view(hwnd, id, false),
                TileKey::App(_) if !editing => open_entry(hwnd, &tile.entry, false),
                _ => {}
            }
        }
    }
}
fn set_page(hwnd: HWND, page: usize) {
    let changed=PANEL.with(|s| {
        if let Some(p) = s.borrow_mut().as_mut() {
            let next=page.min(count_pages(p)-1);
            if next==p.page{return false;}
            p.page = next;
            p.selected = p.page * grid(p).capacity();
            p.hover = None;
            p.hover_from=None;p.hover_started=None;
            return true;
        }
        false
    });
    if changed{
        start_grid_motion(hwnd);
        let areas=PANEL.with(|p|p.borrow().as_ref().map(|p|
            [grid(p).area,rect(0,p.height-px(59,p.dpi),p.width,p.height-px(22,p.dpi))]));
        if let Some(areas)=areas{invalidate_regions(hwnd,areas);}
    }
}
fn page(hwnd: HWND, delta: isize) {
    if let Some(value) = PANEL.with(|s| {
        s.borrow()
            .as_ref()
            .map(|p| p.page.saturating_add_signed(delta))
    }) {
        set_page(hwnd, value);
    }
}
fn step(hwnd: HWND, offset: isize) {
    PANEL.with(|s| {
        if let Some(p) = s.borrow_mut().as_mut() {
            p.keyboard=true;
            p.selected = p
                .selected
                .saturating_add_signed(offset)
                .min(p.tiles.len().saturating_sub(1));
            p.page = p.selected / grid(p).capacity();
        }
    });
    refresh(hwnd);
}
pub(super) fn show_hidden(hwnd: HWND) {
    let edit = PANEL.with(|p| p.borrow().as_ref().map(|p| p.edit));
    if let Some(edit) = edit {
        unsafe {
            SetWindowTextW(edit, wide("").as_ptr());
        }
    }
    internal_view(hwnd, 0, true);
}

unsafe fn drop_tile(hwnd: HWND, from: usize, to: usize) {
    let Some((keys, src, dst, folder)) = PANEL.with(|s| {
        let s = s.borrow();
        let p = s.as_ref()?;
        let src = p.tiles.get(from)?.key.clone();
        let dst = p.tiles.get(to)?.key.clone();
        Some((
            p.tiles.iter().map(|t| t.key.clone()).collect::<Vec<_>>(),
            src,
            dst,
            p.folder,
        ))
    }) else {
        return;
    };
    change(hwnd, |lib| {
        if let (TileKey::App(id), TileKey::Folder(target)) = (&src, &dst) {
            lib.move_to(id, *target);
        } else if from != to {
            lib.reorder(&keys, &src, &dst);
        }
        if folder != 0 && matches!(&src, TileKey::Folder(_)) {
            return Err("Không hỗ trợ thư mục lồng nhau.");
        }
        Ok(())
    });
}

unsafe extern "system" fn edit_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let live=EDIT_COMPOSITE.get()||PANEL.with(|p|p.try_borrow().ok().is_some_and(|p|p.as_ref().is_some_and(|p|p.edit==hwnd&&p.native.is_some())));
    if msg==WM_IME_STARTCOMPOSITION{EDIT_COMPOSING.set(true);}
    if msg==WM_IME_ENDCOMPOSITION{EDIT_COMPOSING.set(false);}
    if live&&msg==WM_ERASEBKGND{return 1;}
    if live&&msg==WM_PAINT{
        let mut ps:PAINTSTRUCT=std::mem::zeroed();
        BeginPaint(hwnd,&mut ps);
        EndPaint(hwnd,&ps);
        return 0;
    }
    if msg == WM_KEYDOWN {
        let empty = PANEL.with(|s| s.borrow().as_ref().is_some_and(|p| p.input.is_empty()));
        if matches!(
            w as u16,
            VK_UP | VK_DOWN | VK_RETURN | VK_ESCAPE | VK_PRIOR | VK_NEXT | VK_APPS | VK_F6
        ) || (empty && matches!(w as u16, VK_LEFT | VK_RIGHT))
        {
            PostMessageW(GetParent(hwnd), EDIT_KEY, w, 0);
            return 0;
        }
    }
    if msg == WM_CHAR && matches!(w, 13 | 27) {
        return 0;
    }
    let result=CallWindowProcW(EDIT_PROC.get(), hwnd, msg, w, l);
    if live&&matches!(msg,WM_SETFOCUS|WM_KILLFOCUS|WM_KEYDOWN|WM_CHAR|WM_LBUTTONDOWN|WM_LBUTTONUP|WM_MOUSEMOVE|
        WM_SETTEXT|WM_PASTE|WM_CUT|WM_CLEAR|WM_UNDO|WM_IME_STARTCOMPOSITION|WM_IME_COMPOSITION|WM_IME_ENDCOMPOSITION|0x00b1){
        if msg==WM_SETFOCUS{HideCaret(hwnd);}
        if msg!=WM_MOUSEMOVE||w&1!=0{
            let area=PANEL.with(|p|p.try_borrow().ok().and_then(|p|p.as_ref().map(|p|{
                search_rect(p.width,p.dpi)
            })));
            if let Some(area)=area{InvalidateRect(GetParent(hwnd),&area,0);}
        }
    }
    result
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint_full(hwnd);
            0
        }
        #[cfg(test)]
        WM_PRINTCLIENT => {
            let area = PANEL.with(|p| p.borrow().as_ref().map(|p|rect(0,0,p.width,p.height)));
            if let Some(area)=area { render_full(hwnd,w as HDC,area); }
            0
        }
        WM_CTLCOLOREDIT|WM_CTLCOLORSTATIC => {
            let (dark,high)=PANEL.with(|p|p.borrow().as_ref().map_or((false,false),|s|(s.dark,s.high_contrast)));
            let palette=palette(dark,high);
            let (bg,ink)=EDIT_PRINT.get().unwrap_or((palette.face,palette.ink));
            SetBkMode(w as HDC,OPAQUE as i32);
            SetTextColor(w as HDC, ink);
            SetBkColor(w as HDC, bg);
            SetDCBrushColor(w as HDC, bg);
            GetStockObject(DC_BRUSH) as LRESULT
        }
        WM_COMMAND if w & 0xffff == EDIT && (w >> 16) & 0xffff == EN_CHANGE as usize => {
            let text = read_edit(l as HWND, 256);
            let dirty = PANEL.with(|s| {
                let mut s = s.borrow_mut();
                let Some(p) = s.as_mut() else {
                    return false;
                };
                if p.input == text {
                    return false;
                }
                let cleared=!p.input.trim().is_empty()&&text.trim().is_empty();
                p.input = text;
                if cleared{p.category=Category::All;p.folder=0;p.hidden=false;}
                p.page = 0;
                p.selected = 0;
                p.hover = None;
                p.hover_from=None;p.hover_started=None;
                p.error.clear();
                p.generation = p.generation.wrapping_add(1);
                p.search_rx = None;
                p.accept_pending = false;
                p.search_settled = false;
                true
            });
            if dirty {
                SetTimer(hwnd, SEARCH, 90, None);
                refresh(hwnd);
            }
            0
        }
        CATALOG_CHANGE => {
            // Do not read the shared notification payload; any scoped change
            // invalidates the snapshot. Merge bursts of shortcut writes.
            catalog::invalidate();
            SetTimer(hwnd, SHELL_REFRESH, 450, None);
            0
        }
        WM_TIMER if w == SHELL_REFRESH => {
            KillTimer(hwnd, SHELL_REFRESH);
            if PANEL.with(|p| p.borrow().as_ref().is_some_and(|p| p.catalog.is_some())) {
                SetTimer(hwnd, SHELL_REFRESH, 450, None);
            } else {
                refresh_catalog(hwnd);
            }
            0
        }
        WM_TIMER if w == SEARCH => {
            KillTimer(hwnd, SEARCH);
            rebuild(hwnd, true);
            0
        }
        WM_TIMER if w == HOVER_ANIMATION => {
            let (regions,active)=PANEL.with(|s| {
                let mut s=s.borrow_mut();let Some(p)=s.as_mut() else {return (Vec::new(),false);};
                let Some(start)=p.hover_started else {return(Vec::new(),false);};
                let regions=p.hover_from.into_iter().chain(p.hover).filter_map(|h|target_rect(p,h)).collect::<Vec<_>>();
                let active=motion::allowed()&&start.progress()<1.0;
                if !active {p.hover_started=None;p.hover_from=None;}
                (regions,active)
            });
            if !active {KillTimer(hwnd,HOVER_ANIMATION);}
            invalidate_regions(hwnd,regions);0
        }
        WM_TIMER if w == GRID_ANIMATION => {
            let (region,active)=PANEL.with(|s|{
                let mut s=s.borrow_mut();let Some(p)=s.as_mut()else{return(None,false);};
                let Some(start)=p.grid_started else{return(None,false);};
                let active=motion::allowed()&&start.progress()<1.0;
                if !active {p.grid_started=None;}
                (Some(grid(p).area),active)
            });
            if !active{KillTimer(hwnd,GRID_ANIMATION);}
            invalidate_regions(hwnd,region);0
        }
        WM_TIMER if w == TRANSITION_TIMER => {
            settle_transition(hwnd);
            0
        }
        WM_TIMER if w == POLL => {
            let (icons, apps, result, busy) = PANEL.with(|s| {
                let mut s = s.borrow_mut();
                let Some(p) = s.as_mut() else {
                    return (Vec::new(), false, None, false);
                };
                let mut icons = Vec::new();
                while let Ok((key, image)) = p.icon_rx.try_recv() {
                    p.pending.remove(&key);
                    let g=grid(p);
                    let mut visible=false;
                    for (i,t) in p.tiles.iter().skip(p.page*g.capacity()).take(g.capacity()).enumerate() {
                        if (t.entry.path==key.0 && key.1==g.icon.clamp(8,192))
                            || (t.preview.contains(&key.0) && key.1==(g.icon/3).clamp(8,192)){
                            icons.push(g.tiles(i));visible=true;
                        }
                    }
                    if visible{p.icons.insert(key,image);}
                }
                let mut apps = false;
                if let Some(rx) = &p.catalog {
                    match rx.try_recv() {
                        Ok(rows) => {
                            p.apps = rows;
                            p.catalog = None;
                            apps = true;
                        }
                        Err(mpsc::TryRecvError::Disconnected) => p.catalog = None,
                        Err(mpsc::TryRecvError::Empty) => {}
                    }
                }
                let mut result = None;
                if let Some(rx) = &p.search_rx {
                    match rx.try_recv() {
                        Ok(rows) => {
                            result = Some(rows);
                            p.search_rx = None;
                        }
                        Err(mpsc::TryRecvError::Disconnected) => p.search_rx = None,
                        Err(mpsc::TryRecvError::Empty) => {}
                    }
                }
                (
                    icons,
                    apps,
                    result,
                    p.catalog.is_some() || !p.pending.is_empty() || p.search_rx.is_some(),
                )
            });
            if !busy {
                KillTimer(hwnd, POLL);
            }
            if apps {
                rebuild(hwnd, false);
            } else if let Some((gen, tiles)) = result {
                apply_rows(hwnd, gen, tiles);
            } else if !icons.is_empty() {
                invalidate_regions(hwnd,icons);
            }
            0
        }
        WM_ACTIVATE if w & 0xffff == WA_INACTIVE as usize => {
            if !MODAL.get() {
                dismiss(false);
            }
            0
        }
        WM_CLOSE => {
            request_close();
            0
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED | WM_DWMCOMPOSITIONCHANGED => {
            if msg==WM_SETTINGCHANGE{motion::refresh_system();}
            let dark = dark();
            surface_style(hwnd, dark);
            let motion=motion_allowed();
            PANEL.with(|s| {
                if let Some(p) = s.borrow_mut().as_mut() {
                    p.dark = dark;
                    p.high_contrast=high_contrast();
                    p.motion=motion;
                }
            });
            if !motion&&PANEL.with(|s|s.borrow().as_ref().is_some_and(|p|p.transition.is_some())){
                settle_transition(hwnd);
                if window().is_none(){return 0;}
            }
            if !transparency_allowed()&&PANEL.with(|s|s.borrow().as_ref().is_some_and(|p|p.native.is_some())){
                fallback_material(hwnd);
            }
            refresh_material(hwnd);
            if let Some(edit)=PANEL.with(|p|p.borrow().as_ref().map(|p|p.edit)){ InvalidateRect(edit,ptr::null(),0); }
            0
        }
        WM_DPICHANGED | WM_DISPLAYCHANGE => {
            let point = PANEL.with(|p| p.borrow().as_ref().map(|p| p.point));
            if let Some(point) = point {
                place(hwnd, point);
            }
            0
        }
        WM_KEYDOWN | EDIT_KEY => {
            let (n, escape, selected) = PANEL.with(|s| {
                s.borrow().as_ref().map_or((1, false, 0), |p| {
                    (
                        grid(p).columns as isize,
                        p.hidden || p.folder != 0,
                        p.selected,
                    )
                })
            });
            match w as u16 {
                VK_ESCAPE if escape => internal_view(hwnd, 0, false),
                VK_ESCAPE => request_close(),
                VK_RETURN => {
                    let waiting = PANEL.with(|p| {
                        let mut s = p.borrow_mut();
                        let Some(p) = s.as_mut() else {
                            return false;
                        };
                        let waiting = !p.search_settled;
                        p.accept_pending = waiting;
                        waiting
                    });
                    if waiting {
                        KillTimer(hwnd, SEARCH);
                        rebuild(hwnd, true);
                    } else {
                        action(hwnd, Hit::Tile(selected));
                    }
                }
                VK_UP => step(hwnd, -n),
                VK_DOWN => step(hwnd, n),
                VK_LEFT => step(hwnd, -1),
                VK_RIGHT => step(hwnd, 1),
                VK_PRIOR => page(hwnd, -1),
                VK_NEXT => page(hwnd, 1),
                VK_F6 => action(hwnd, Hit::Compact),
                VK_APPS => tile_menu(hwnd, selected, POINT { x: 28, y: 180 }),
                _ => {}
            }
            0
        }
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let direction=PANEL.with(|p|{
                let mut p=p.borrow_mut();let Some(p)=p.as_mut() else{return 0;};
                p.wheel=(p.wheel+(w>>16) as i16 as i32).clamp(-240,240);
                if p.wheel.abs()<120 {0}else{let sign=p.wheel.signum();p.wheel=0;-sign}
            });
            if direction!=0 {page(hwnd,direction as isize);}
            0
        }
        WM_MOUSEMOVE => {
            let (regions, track) = PANEL.with(|s| {
                let mut s = s.borrow_mut();
                let Some(p) = s.as_mut() else {
                    return (Vec::new(),false);
                };
                let next = hit_at(p, coords(l));
                let mut regions = hover_transition(p,next);
                if p.keyboard { if let Some(r)=target_rect(p,Hit::Tile(p.selected)){regions.push(r);} p.keyboard=false; }
                let track = !p.tracking;
                p.tracking = true;
                (regions,track)
            });
            if track { TrackMouseEvent(&mut TRACKMOUSEEVENT { cbSize:std::mem::size_of::<TRACKMOUSEEVENT>() as u32,dwFlags:TME_LEAVE,hwndTrack:hwnd,dwHoverTime:0 }); }
            animate_hover(hwnd,regions);
            0
        }
        WM_MOUSELEAVE => {
            let regions=PANEL.with(|s| s.borrow_mut().as_mut().map_or_else(Vec::new,|p|{p.tracking=false;hover_transition(p,None)}));
            animate_hover(hwnd,regions);
            0
        }
        WM_LBUTTONDOWN => {
            let point = coords(l);
            let pressed = PANEL.with(|s| {
                let mut s = s.borrow_mut();
                let Some(p) = s.as_mut() else {
                    return false;
                };
                p.pressed = Some((hit_at(p,point).unwrap_or(Hit::Backdrop),point));
                true
            });
            if pressed {
                SetCapture(hwnd);
                if let Some(r)=PANEL.with(|s|s.borrow().as_ref().and_then(|p|p.pressed.and_then(|(h,_)|target_rect(p,h)))){ invalidate_regions(hwnd,[r]); }
            }
            0
        }
        WM_CAPTURECHANGED => {
            let region=PANEL.with(|s|{let mut s=s.borrow_mut();let p=s.as_mut()?;let (h,_)=p.pressed.take()?;target_rect(p,h)});
            invalidate_regions(hwnd,region);
            0
        }
        WM_LBUTTONUP => {
            let point = coords(l);
            let mut changed=None;
            let targets = PANEL.with(|s| {
                let mut s = s.borrow_mut();
                let p = s.as_mut()?;
                let (start, origin) = p.pressed.take()?;
                changed=target_rect(p,start);
                let end = hit_at(p, point).unwrap_or(Hit::Backdrop);
                let far = (point.x - origin.x).abs().max((point.y - origin.y).abs()) > px(7, p.dpi);
                Some((start, end, far, p.editing, p.folder))
            });
            if GetCapture() == hwnd {
                ReleaseCapture();
            }
            invalidate_regions(hwnd,changed);
            if let Some((start, end, far, editing, folder)) = targets {
                match (start, end, far) {
                    (Hit::Tile(a), Hit::Tile(b), true) if editing => drop_tile(hwnd, a, b),
                    (Hit::Tile(a), Hit::Page(dest), true) if editing => {
                        if let Some(dest) = PANEL.with(|s| {
                            let s = s.borrow();
                            let p = s.as_ref()?;
                            (dest != p.page).then_some((dest,dest * grid(p).capacity()))
                        }) {
                            drop_tile(hwnd, a, dest.1);
                            set_page(hwnd, dest.0);
                        }
                    }
                    (Hit::Tile(a), Hit::Back, true) if editing && folder != 0 => {
                        let tile = PANEL
                            .with(|s| s.borrow().as_ref().and_then(|p| p.tiles.get(a).cloned()));
                        if let Some(Tile {
                            key: TileKey::App(id),
                            ..
                        }) = tile
                        {
                            change(hwnd, |l| {
                                l.move_to(&id, 0);
                                Ok(())
                            });
                        }
                    }
                    (a, b, false) if a == b => action(hwnd, b),
                    _ => {}
                }
            }
            0
        }
        WM_RBUTTONUP => {
            let point = coords(l);
            let target=PANEL.with(|s|s.borrow().as_ref().and_then(|p|hit_at(p,point)));
            match target{
                Some(Hit::Tile(i))=>tile_menu(hwnd,i,point),
                None=>library_menu(hwnd,point),
                _=>{}
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

pub(super) fn open(point: POINT, query: String, prior: HWND, manage: bool) {
    open_native(point, query, prior, manage, true);
}

fn open_native(point: POINT, query: String, prior: HWND, manage: bool, activate: bool) {
    let started=std::time::Instant::now();
    if !enabled() {
        return;
    }
    unsafe {
        if let Some(hwnd) = window() {
            let same = PANEL.with(|s| {
                s.borrow().as_ref().is_some_and(|p| {
                    MonitorFromPoint(p.point, MONITOR_DEFAULTTONEAREST)
                        == MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST)
                })
            });
            if manage {
                show_hidden(hwnd);
                return;
            }
            if same {
                toggle_visible();
                return;
            }
            place(hwnd, point);
            return;
        }
        let Some((area, _)) = estimate(point) else {
            return;
        };
        let instance = GetModuleHandleW(ptr::null());
        let c = wide(CLASS);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance,
            lpszClassName: c.as_ptr(),
            hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let input = InputHandoff::capture();
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            c.as_ptr(),
            wide("TLauncher Spotlight · Thư viện ứng dụng").as_ptr(),
            WS_POPUP,
            area.left,
            area.top,
            1,
            1,
            ptr::null_mut(),
            ptr::null_mut(),
            instance,
            ptr::null_mut(),
        );
        if hwnd.is_null() {
            return;
        }
        EDIT_COMPOSITE.set(false);
        EDIT_COMPOSING.set(false);
        let edit = CreateWindowExW(
            0,
            wide("EDIT").as_ptr(),
            wide("").as_ptr(),
            WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL as u32,
            0,
            0,
            1,
            1,
            hwnd,
            EDIT as HMENU,
            instance,
            ptr::null_mut(),
        );
        if edit.is_null() {
            DestroyWindow(hwnd);
            return;
        }
        let old = SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as *const () as isize);
        if old == 0 {
            DestroyWindow(hwnd);
            return;
        }
        EDIT_PROC.set(Some(std::mem::transmute::<
            isize,
            unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
        >(old)));
        // Do not let native theme/cue painting leak opaque gray into the
        // composition. Near-zero alpha keeps normal HWND hit testing and IME
        // geometry intact; alpha zero would make mouse input pass through.
        let empty=wide("");
        windows_sys::Win32::UI::Controls::SetWindowTheme(edit,empty.as_ptr(),empty.as_ptr());
        let ex=GetWindowLongPtrW(edit,GWL_EXSTYLE);
        SetWindowLongPtrW(edit,GWL_EXSTYLE,ex|WS_EX_LAYERED as isize);
        let composited=SetLayeredWindowAttributes(edit,0,1,LWA_ALPHA)!=0;
        EDIT_COMPOSITE.set(composited);
        if !composited{
            SetWindowLongPtrW(edit,GWL_EXSTYLE,ex);
            // Downlevel/session fallback stays a fully usable native EDIT.
            SetWindowLongPtrW(hwnd,GWL_STYLE,GetWindowLongPtrW(hwnd,GWL_STYLE)|WS_CLIPCHILDREN as isize);
        }
        SendMessageW(edit, 0x00c5, 256, 0);
        SendMessageW(
            edit,
            0x1501,
            0,
            wide(words("Tìm kiếm…","Search…","搜索…")).as_ptr() as LPARAM,
        );
        let d = dark();
        surface_style(hwnd, d);
        let (icon_tx, icon_rx) = icon_worker();
        let apps = catalog::snapshot(true);
        PANEL.with(|p| {
            p.replace(Some(Panel {
                hwnd,
                edit,
                prior,
                point,
                width: 0,
                height: 0,
                dpi: 96,
                backdrop:Vec::new(),
                glass:Vec::<u8>::new().into(),
                native:None,
                native_rejected:false,
                transition:None,
                bounds:std::mem::zeroed(),
                dark: d,
                high_contrast:high_contrast(),
                font: ptr::null_mut(),
                search_font:ptr::null_mut(),
                apps,
                catalog: catalog::request(true, false),
                watch: None,
                input: query.clone(),
                category: Category::All,
                folder: 0,
                hidden: manage,
                generation: 0,
                search_rx: None,
                accept_pending: false,
                search_settled: false,
                editing: manage,
                page: 0,
                selected: 0,
                tiles: Vec::new(),
                error: String::new(),
                hover: None,
                hover_from:None,
                hover_started:None,
                grid_started:None,
                motion:motion_allowed(),
                tracking:false,
                keyboard:false,
                wheel:0,
                pressed: None,
                icon_tx,
                icon_rx,
                icons: HashMap::new(),
                pending: HashSet::new(),
                icon_rev: crate::app_icons::revision(),
            }))
        });
        SetWindowTextW(edit, wide(&query).as_ptr());
        place(hwnd, point);
        // The input HWND and passive glass companion have not been shown yet.
        // Present the foreground while the composition root is transparent.
        // Do not spend the opening clock on HWND/layout/icon initialization.
        let (prepared,backdrop)=PANEL.with(|s|{
            let b=s.borrow();let Some(p)=b.as_ref()else{return (false,None);};
            let Some(n)=&p.native else{return (false,None);};
            let ready=activate&&p.motion&&motion::allowed()
                &&n.foreground.prepare_open().is_ok()&&n.backdrop.prepare_open().is_ok();
            if !ready{let _=n.foreground.settle();n.backdrop.settle();}
            (ready,Some((n.backdrop.hwnd,p.bounds)))
        });
        if let Some((bg,r))=backdrop{
            SetWindowPos(bg,hwnd,r.left,r.top,r.right-r.left,r.bottom-r.top,SWP_NOACTIVATE|SWP_SHOWWINDOW);
        }
        if activate {
            ShowWindow(hwnd, SW_SHOW);
            UpdateWindow(hwnd);
            SetForegroundWindow(hwnd);
            SetFocus(edit);
            input.finish(hwnd, "Spotlight library");
        } else {
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            UpdateWindow(hwnd);
        }
        if prepared&&!start_transition(hwnd,true){
            PANEL.with(|s|if let Some(n)=s.borrow().as_ref().and_then(|p|p.native.as_ref()){
                let _=n.foreground.settle();n.backdrop.settle();
            });
        }
        SendMessageW(edit, 0x00b1, usize::MAX, -1);
        if crate::trial::active(){
            let (ready,total,live)=PANEL.with(|p|p.borrow().as_ref().map_or((0,0,false),|p|{
                let g=grid(p);
                let paths=p.tiles.iter().take(g.capacity()).filter(|t|!t.entry.path.as_os_str().is_empty()).collect::<Vec<_>>();
                let ready=paths.iter().filter(|t|p.icons.get(&(t.entry.path.clone(),g.icon.clamp(8,192))).is_some_and(Option::is_some)).count();
                (ready,paths.len(),p.native.is_some())
            }));
            crate::log!("Spotlight first frame prepared in {:.1} ms, icons ready={ready}/{total}, live host initialized={live}",started.elapsed().as_secs_f64()*1000.0);
        }
        SetTimer(hwnd, POLL, 40, None);
        let watch = catalog::ShellWatch::new(hwnd, CATALOG_CHANGE);
        PANEL.with(|p| {
            if let Some(p) = p.borrow_mut().as_mut() {
                p.watch = Some(watch);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rounded_material_has_premultiplied_alpha_and_no_white_corner() {
        let ink=rounded_ink(40,30,24,0x00a05020,100);
        assert_eq!(&ink.pixels[..4],&[0,0,0,0]);
        assert!(ink.pixels.chunks_exact(4).all(|p|p[..3].iter().all(|c|*c<=p[3])));
        let i=(15*40+20)*4;
        assert_eq!(&ink.pixels[i..i+4],&[63,31,13,100]);
        assert_eq!(hover_opacity(Hit::Edit,Some(Hit::Edit),None,0.5),0.5);
        assert_eq!(hover_opacity(Hit::Edit,None,Some(Hit::Edit),0.5),0.5);
        assert_eq!(grid_opacity(0.0),150);
        assert_eq!(grid_opacity(1.0),255);
    }
    #[test]
    fn extreme_backdrops_keep_readable_theme_brightness() {
        let prefs=Preferences{glass_strength:48,..Default::default()};
        let white=vec![255;64*4];let black=vec![0;64*4];
        let dark=frosted(&white,32,32,96,true,&prefs);
        let light=frosted(&black,32,32,96,false,&prefs);
        assert!(dark.chunks_exact(4).all(|p|p[3]==255 && p[..3].iter().all(|c|*c<=94)));
        assert!(light.chunks_exact(4).all(|p|p[3]==255 && p[..3].iter().all(|c|*c<=117)));
    }
    #[test]
    fn layout_keeps_tiles_inside_landscape_portrait_and_high_dpi() {
        let portrait=Grid::new(1080,1920,96,&Preferences::default());
        let landscape=Grid::new(2560,1080,96,&Preferences::default());
        assert!(portrait.rows>landscape.rows&&portrait.columns<landscape.columns);
        assert!(portrait.area.top<160&&portrait.area.bottom>1680);
        for (w, h) in [
            (2560, 1080),
            (1080, 1920),
            (3840, 2160),
            (3440, 1440),
            (1024, 768),
        ] {
            for dpi in [96, 120, 144, 192] {
                let g = Grid::new(w, h, dpi, &Preferences::default());
                assert!(g.capacity() >= 2);
                let a = g.tiles(0);
                let b = g.tiles(g.capacity() - 1);
                assert!(a.left >= 0 && a.top >= 0 && b.right <= w && b.bottom <= h);
                assert!(a.right - a.left >= g.icon && a.bottom - a.top >= g.icon);
                assert_eq!((g.capacity() + 1).div_ceil(g.capacity()), 2);
            }
        }
    }
    #[test]
    fn categories_filter_launchable_apps_files_and_system_actions() {
        let entry = |kind| Entry {
            name: String::new(),
            path: PathBuf::new(),
            kind,
            detail: String::new(),
        };
        assert!(Category::Apps.accepts(&entry("Pinned")));
        assert!(!Category::Apps.accepts(&entry("File")));
        assert!(Category::Files.accepts(&entry("Web shortcut")));
        assert!(Category::System.accepts(&entry("System")));
        assert!(!Category::System.accepts(&entry("Application")));
    }
}

#[cfg(test)]
#[path = "fullscreen_tests.rs"]
pub(super) mod native_tests;
