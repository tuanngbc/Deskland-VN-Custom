// FluidWin v1.3.1 genio::Motore::cattura / scopertaSulloSchermo adaptation.
// MIT, Copyright (c) 2026 Clooudy. Original stampa.h is reused unchanged.
// Static snapshots only; live Jelly remains on the shared WGC worker.
#include <windows.h>
#include <dwmapi.h>
#include <atomic>
#include <memory>
#include <vector>
#include "upstream/stampa.h"

namespace {
// Unlike upstream's per-HWND admission, one hung PrintWindow globally blocks
// further print jobs. The callback retains its DIB and permit until it returns.
std::atomic<bool> printBusy{false};
struct Identity { HWND h; DWORD pid,tid; RECT outer,visible; LONG_PTR style,exstyle; };
struct Photo { std::vector<UINT> pixels; int w=0,h=0,backend=0; };
bool same(const RECT& a,const RECT& b){return EqualRect(&a,&b)!=FALSE;}
bool permitted(const Identity& id){
    DWORD pid=0,affinity=~0u,cloaked=1;RECT r{},v{};
    return IsWindow(id.h)&&GetAncestor(id.h,GA_ROOT)==id.h&&
        GetWindowThreadProcessId(id.h,&pid)==id.tid&&pid==id.pid&&
        IsWindowVisible(id.h)&&!IsIconic(id.h)&&!IsHungAppWindow(id.h)&&
        GetWindowDisplayAffinity(id.h,&affinity)&&affinity==WDA_NONE&&
        SUCCEEDED(DwmGetWindowAttribute(id.h,DWMWA_CLOAKED,&cloaked,sizeof(cloaked)))&&!cloaked&&
        GetWindowRect(id.h,&r)&&same(r,id.outer)&&
        SUCCEEDED(DwmGetWindowAttribute(id.h,DWMWA_EXTENDED_FRAME_BOUNDS,&v,sizeof(v)))&&same(v,id.visible)&&
        GetWindowLongPtrW(id.h,GWL_STYLE)==id.style&&GetWindowLongPtrW(id.h,GWL_EXSTYLE)==id.exstyle;
}
bool rectangular(const Identity& id){
    // GDI photos have no trustworthy per-pixel alpha. Keep WGC for layered,
    // shaped and potentially rounded sources instead of flattening corners.
    if(id.exstyle&WS_EX_LAYERED)return false;
    HRGN region=CreateRectRgn(0,0,0,0);if(!region)return false;
    const int type=GetWindowRgn(id.h,region);bool shape=false;
    if(type==ERROR)shape=true;
    else if(type==SIMPLEREGION){
        HRGN full=CreateRectRgn(0,0,id.outer.right-id.outer.left,id.outer.bottom-id.outer.top);
        shape=full&&EqualRgn(region,full);if(full)DeleteObject(full);
    }
    DeleteObject(region);if(!shape)return false;
    DWM_WINDOW_CORNER_PREFERENCE corner=DWMWCP_DEFAULT;
    const bool known=SUCCEEDED(DwmGetWindowAttribute(id.h,DWMWA_WINDOW_CORNER_PREFERENCE,&corner,sizeof(corner)));
    if(known&&(corner==DWMWCP_ROUND||corner==DWMWCP_ROUNDSMALL))return false;
    if(IsZoomed(id.h)||!(id.style&(WS_CAPTION|WS_THICKFRAME)))return true;
    return known&&corner==DWMWCP_DONOTROUND;
}
bool uncovered(const Identity& id){
    if(!permitted(id))return false;
    struct Coverage {RECT area;long long sum=0;} coverage{id.visible};
    if(!EnumDisplayMonitors(nullptr,nullptr,[](HMONITOR,HDC,LPRECT r,LPARAM p)->BOOL{
        auto& c=*reinterpret_cast<Coverage*>(p);RECT x{};
        if(IntersectRect(&x,&c.area,r))c.sum+=static_cast<long long>(x.right-x.left)*(x.bottom-x.top);return TRUE;
    },reinterpret_cast<LPARAM>(&coverage)))return false;
    if(coverage.sum!=static_cast<long long>(id.visible.right-id.visible.left)*(id.visible.bottom-id.visible.top))return false;
    unsigned count=0;
    for(HWND w=GetWindow(id.h,GW_HWNDPREV);w;w=GetWindow(w,GW_HWNDPREV)){
        if(++count>4096)return false;
        if(!IsWindowVisible(w))continue;
        DWORD cloaked=0;RECT r{},region{},x{};
        if(FAILED(DwmGetWindowAttribute(w,DWMWA_CLOAKED,&cloaked,sizeof(cloaked))))return false;
        if(cloaked)continue;if(!GetWindowRect(w,&r))return false;
        const int type=GetWindowRgnBox(w,&region);if(type==NULLREGION)continue;
        if(type==SIMPLEREGION||type==COMPLEXREGION){OffsetRect(&region,r.left,r.top);r=region;}
        // No own-overlay exemption: screen pixels must belong to the source.
        if(IntersectRect(&x,&r,&id.visible))return false;
    }
    return permitted(id);
}
struct Dib {
    HDC screen=nullptr,dc=nullptr;HBITMAP bitmap=nullptr;HGDIOBJ old=nullptr;void* bits=nullptr;
    Dib(int w,int h){
        BITMAPINFO info{};info.bmiHeader={sizeof(BITMAPINFOHEADER),w,-h,1,32,BI_RGB,0,0,0,0,0};
        screen=GetDC(nullptr);if(!screen)return;dc=CreateCompatibleDC(screen);if(!dc)return;
        bitmap=CreateDIBSection(screen,&info,DIB_RGB_COLORS,&bits,nullptr,0);if(bitmap)old=SelectObject(dc,bitmap);
    }
    ~Dib(){if(old&&old!=HGDI_ERROR)SelectObject(dc,old);if(bitmap)DeleteObject(bitmap);if(dc)DeleteDC(dc);if(screen)ReleaseDC(nullptr,screen);}
    bool ready()const{return bits&&old&&old!=HGDI_ERROR;}
};
bool screen_photo(const Identity& id,Photo& photo){
    if(!uncovered(id))return false;
    // Only the visible frame is copied; invisible resize margins and shadows
    // outside it are not application content. No color repair or edge rewrite.
    const int w=id.visible.right-id.visible.left,h=id.visible.bottom-id.visible.top;
    Dib dib(w,h);if(!dib.ready()||!BitBlt(dib.dc,0,0,w,h,dib.screen,id.visible.left,id.visible.top,SRCCOPY))return false;
    GdiFlush();if(!uncovered(id)||!rectangular(id))return false;
    const auto* bits=static_cast<const UINT*>(dib.bits);photo.pixels.assign(bits,bits+size_t(w)*h);
    for(auto& pixel:photo.pixels)pixel|=0xff000000u;
    photo.w=w;photo.h=h;photo.backend=1;return true;
}
struct Print {stampa::Lavoro* work;Identity id;};
void CALLBACK print_callback(PTP_CALLBACK_INSTANCE instance,void* context){
    CallbackMayRunLong(instance);std::unique_ptr<Print> print(static_cast<Print*>(context));
    auto* work=print->work;
    if(permitted(print->id))stampa::thread(work);
    else{stampa::libera(work->h);SetEvent(work->fatto);stampa::lascia(work);}
    printBusy.store(false,std::memory_order_release);
}
// 0=ready, 1=unsupported shape (WGC allowed), 2=policy/identity denied,
// 3=capture failed, 4=print busy. Failed supported photos use native fallback.
int capture(const Identity& id,Photo& photo,bool allowPrint){
    if(!permitted(id))return 2;if(!rectangular(id))return 1;
    if(screen_photo(id,photo))return 0;
    if(!allowPrint)return 3;
    if(!permitted(id))return 2;
    bool expected=false;if(!printBusy.compare_exchange_strong(expected,true))return 4;
    struct Permit{bool queued=false;~Permit(){if(!queued)printBusy.store(false);}}permit;
    const int w=id.outer.right-id.outer.left,h=id.outer.bottom-id.outer.top;
    if(!stampa::prenota(id.h)){printBusy.store(false);return 4;}
    auto* work=new(std::nothrow) stampa::Lavoro{id.h,w,h,{},FALSE,CreateEventW(nullptr,TRUE,FALSE,nullptr),2};
    auto* print=work?new(std::nothrow) Print{work,id}:nullptr;
    if(!work||!work->fatto||!print||!TrySubmitThreadpoolCallback(print_callback,print,nullptr)){
        delete print;if(work){if(work->fatto)CloseHandle(work->fatto);delete work;}
        stampa::libera(id.h);printBusy.store(false);return 3;
    }
    permit.queued=true;
    // Original refcount/event ownership: timeout drops only the caller's ref;
    // the callback eventually frees its own DIB, pixels and process-wide permit.
    WaitForSingleObject(work->fatto,300);std::vector<UINT> pixels;
    if(!stampa::prendi(work,pixels))return 3;
    if(!permitted(id)||!rectangular(id))return 2;
    const int sx=id.visible.left-id.outer.left,sy=id.visible.top-id.outer.top;
    const int vw=id.visible.right-id.visible.left,vh=id.visible.bottom-id.visible.top;
    photo.pixels.resize(size_t(vw)*vh);size_t colored=0;
    for(int y=0;y<vh;y++)for(int x=0;x<vw;x++){
        const UINT pixel=pixels[size_t(y+sy)*w+x+sx];photo.pixels[size_t(y)*vw+x]=pixel|0xff000000u;colored+=(pixel&0xffffff)!=0;
    }
    if(colored*200<photo.pixels.size())return 3; // Original blank PrintWindow heuristic.
    photo.w=vw;photo.h=vh;photo.backend=2;return 0;
}
}
extern "C" int tlauncher_fluidwin_static_capture(HWND h,DWORD pid,DWORD tid,const RECT* outer,const RECT* visible,
    unsigned long long maxPixels,int allowPrint,void** out)noexcept{
    if(!out)return 2;*out=nullptr;if(!outer||!visible)return 2;
    const auto w=static_cast<long long>(outer->right)-outer->left,hh=static_cast<long long>(outer->bottom)-outer->top;
    MEMORYSTATUSEX memory{};memory.dwLength=sizeof(memory);
    // Budget is a refusal, never a downsample. Three native CPU buffers at most
    // (DIB, original print vector, cropped output), within available RAM/32.
    if(w<=0||hh<=0||w>16384||hh>16384||static_cast<unsigned long long>(w*hh)>maxPixels||
        !GlobalMemoryStatusEx(&memory)||static_cast<unsigned long long>(w*hh)*12>memory.ullAvailPhys/32||
        IsRectEmpty(visible)||visible->left<outer->left||visible->top<outer->top||visible->right>outer->right||visible->bottom>outer->bottom)return 3;
    try{
        Identity id{h,pid,tid,*outer,*visible,GetWindowLongPtrW(h,GWL_STYLE),GetWindowLongPtrW(h,GWL_EXSTYLE)};
        auto photo=std::make_unique<Photo>();const int status=capture(id,*photo,allowPrint!=0);
        if(status)return status;if(!permitted(id))return 2;*out=photo.release();return 0;
    }catch(...){return 3;}
}
extern "C" const UINT* tlauncher_fluidwin_static_pixels(void* value,int* w,int* h,int* backend)noexcept{
    if(!value||!w||!h||!backend)return nullptr;const auto& photo=*static_cast<Photo*>(value);
    *w=photo.w;*h=photo.h;*backend=photo.backend;return photo.pixels.data();
}
extern "C" void tlauncher_fluidwin_static_free(void* value)noexcept{delete static_cast<Photo*>(value);}
