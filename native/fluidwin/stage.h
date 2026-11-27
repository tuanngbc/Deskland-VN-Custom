// Native Stage: Dock owns inventory and activation. No foreign HWND placement/focus
// changes during hover; DWM owns live thumbnails, stampa owns asynchronous snapshots.
#pragma once
#include "vetro.h"
#include "menu.h"
#include "stampa.h"
#include <windowsx.h>
#include <memory>
#include <unordered_map>

namespace stage {
struct Settings {
    bool enabled = false, perspective = true, autoHide = false, peek = true;
    bool peekLive = true, peekAnimation = true, clickActivate = true;
    int monitor = 0, right = 0, height = 126, spacing = 12, opacity = 95;
    int peekDelay = 450, peekSize = 640, edgeGap = 12;
    std::wstring display;
    bool operator==(const Settings&) const = default;
};
struct Window {
    HWND hwnd = nullptr;
    DWORD pid = 0, tid = 0;
    std::wstring identity, name, icon;
    uint64_t generation=0;
    bool valid() const {
        DWORD p = 0;
        const DWORD t = GetWindowThreadProcessId(hwnd, &p);
        return t && p == pid && t == tid && IsWindow(hwnd)&&(!generation||generation==stampa::generation(hwnd));
    }
};
inline RECT aspectFit(int width, int height, RECT box, bool enlarge = true) {
    if (width <= 0 || height <= 0 || IsRectEmpty(&box)) return {};
    double s = std::min(double(box.right-box.left)/width, double(box.bottom-box.top)/height);
    if (!enlarge) s = std::min(1.0, s);
    const LONG w = std::max(1L, LONG(std::lround(width*s))), h = std::max(1L, LONG(std::lround(height*s)));
    const LONG x = box.left + (box.right-box.left-w)/2, y = box.top + (box.bottom-box.top-h)/2;
    return {x,y,x+w,y+h};
}

class Manager {
    struct Card {
        std::vector<Window> windows;
        size_t current = 0;
        RECT rect{};
        HTHUMBNAIL thumbnail = nullptr;
        ID2D1Bitmap* bitmap = nullptr;
        ID2D1Bitmap* icon = nullptr;
        HWND registered = nullptr;
        bool minimized = false;
        unsigned generation = 0;
        Window& source() { return windows[current % windows.size()]; }
    };
    struct Rail {
        Manager* owner;
        HMONITOR monitor = nullptr;
        vetro::Finestra glass;
        IDWriteTextFormat* text = nullptr;
        std::vector<Card> cards;
        int hover = -1, page = 0, capacity = 1;
        bool hidden = false;
        ULONGLONG hoverAt = 0;
        explicit Rail(Manager* m): owner(m) {}
        ~Rail() { clear(); if (text) text->Release(); glass.chiudi(); }
        void clear() {
            for (auto& c : cards) { if (c.thumbnail) DwmUnregisterThumbnail(c.thumbnail); if(c.bitmap) c.bitmap->Release(); if(c.icon)c.icon->Release(); }
            cards.clear();
        }
        int hit(POINT p) const {
            for (size_t i=0; i<cards.size(); ++i) if (PtInRect(&cards[i].rect,p)) return int(i);
            return -1;
        }
        bool messaggio(HWND h, UINT m, WPARAM w, LPARAM l, LRESULT& r) {
            r=0;
            switch(m) {
                case WM_PAINT: owner->draw(*this); return true;
                case WM_MOUSEACTIVATE: r=MA_NOACTIVATE; return true;
                case WM_MOUSEMOVE: {
                    if(hidden) { hidden=false; owner->layout(*this); }
                    TRACKMOUSEEVENT t{sizeof(t),TME_LEAVE,h,0}; TrackMouseEvent(&t);
                    int i=hit({GET_X_LPARAM(l),GET_Y_LPARAM(l)});
                    if(i!=hover) { owner->closePeek(); hover=i; hoverAt=GetTickCount64(); if(i>=0)owner->requestCapture(cards[i].source(),true); KillTimer(h,2); if(i>=0&&owner->settings.peek) SetTimer(h,2,owner->settings.peekDelay,nullptr); owner->draw(*this); }
                    return true;
                }
                case WM_MOUSELEAVE:
                    KillTimer(h,2); hover=-1; owner->scheduleClosePeek(); owner->draw(*this);
                    if(owner->settings.autoHide) SetTimer(h,3,650,nullptr);
                    return true;
                case WM_TIMER:
                    KillTimer(h,(UINT_PTR)w);
                    if(w==2&&hover>=0&&hover<int(cards.size())) owner->openPeek(*this,cards[hover].source());
                    if(w==3&&hover<0&&menu::aperti==0) { hidden=true; owner->closePeek(); owner->layout(*this); }
                    return true;
                case WM_LBUTTONUP: {
                    const int i=hit({GET_X_LPARAM(l),GET_Y_LPARAM(l)});
                    if(i>=0&&owner->settings.clickActivate) owner->activate(cards[i].source());
                    return true;
                }
                case WM_MOUSEWHEEL:
                    if(hover>=0&&hover<int(cards.size())&&cards[hover].windows.size()>1) {
                        auto& c=cards[hover]; const auto n=c.windows.size();
                        c.current=(c.current+n+(GET_WHEEL_DELTA_WPARAM(w)>0?1:-1))%n;
                        owner->closePeek(); owner->preview(*this,c); owner->draw(*this);
                    } else { page=std::max(0,page+(GET_WHEEL_DELTA_WPARAM(w)>0?-1:1)); owner->rebuild(*this); }
                    return true;
                case WM_DISPLAYCHANGE: owner->topologyChanged=true; return true;
                case WM_CLOSE: return true;
            }
            return false;
        }
    };
    struct Peek {
        Manager* owner;
        vetro::Finestra glass;
        Window source;
        HTHUMBNAIL thumbnail = nullptr;
        ID2D1Bitmap* bitmap = nullptr;
        double start = 0;
        explicit Peek(Manager* m): owner(m) {}
        ~Peek() { close(); }
        void close() {
            if(thumbnail) DwmUnregisterThumbnail(thumbnail); thumbnail=nullptr;
            if(bitmap) bitmap->Release(); bitmap=nullptr;
            vetro::fermaBattito(glass.fin,1); glass.chiudi(); source={};
        }
        void draw() {
            if(!glass.inizia()) return;
            if(bitmap) {
                const auto sz=bitmap->GetPixelSize();
                RECT r=aspectFit(sz.width,sz.height,{8,8,glass.W-8,glass.H-8},false);
                glass.rt->DrawBitmap(bitmap,D2D1::RectF(float(r.left),float(r.top),float(r.right),float(r.bottom)));
            }
            glass.finisci();
        }
        bool messaggio(HWND h,UINT m,WPARAM w,LPARAM,LRESULT& r) {
            r=0;
            if(m==WM_MOUSEACTIVATE) {r=MA_NOACTIVATE;return true;}
            if(m==WM_PAINT) {draw();return true;}
            if(m==WM_LBUTTONUP) {if(owner->settings.clickActivate)owner->activate(source);return true;}
            if(m==WM_MOUSEMOVE){KillTimer(h,2);TRACKMOUSEEVENT t{sizeof(t),TME_LEAVE,h,0};TrackMouseEvent(&t);return true;}
            if(m==WM_MOUSELEAVE){owner->scheduleClosePeek();return true;}
            if(m==WM_TIMER&&w==2){KillTimer(h,2);if(!owner->pointerInside())owner->closePeek();return true;}
            if(m==WM_TIMER&&w==1) {
                double k=vetro::uscita((vetro::secondi()-start)/.16);
                glass.alfa(k); if(k>=1) vetro::fermaBattito(glass.fin,1); return true;
            }
            return false;
        }
    };
public:
    Settings settings;
    std::function<std::vector<Window>()> inventory;
    std::function<void(HWND)> activation;
    std::function<RECT(HMONITOR)> reserved;
    std::function<std::vector<RECT>(HMONITOR)> exclusions;
    std::function<bool(HMONITOR)> fullscreen;
    // Reuse the shared background icon provider; target-specific bitmap is owned by Stage.
    std::function<ID2D1Bitmap*(ID2D1RenderTarget*,const std::wstring&,int)> icon;
    bool topologyChanged=false;
    void configure(const Settings& s) {
        if(settings==s&&(!s.enabled||!rails.empty()))return;
        settings=s;stampa::retain(s.enabled);
        if(!s.enabled) { stop(); return; }
        rebuildAll();
    }
    void changed() {
        if(!settings.enabled) return;
        if(peek.glass.fin&&!peek.source.valid()) closePeek();
        if(topologyChanged) { rebuildAll(); return; }
        auto next=inventory?inventory():std::vector<Window>{};
        bool same=next.size()==windows.size();
        if(same) for(size_t i=0;i<next.size();++i) if(next[i].hwnd!=windows[i].hwnd||next[i].pid!=windows[i].pid||next[i].tid!=windows[i].tid||next[i].generation!=windows[i].generation||next[i].identity!=windows[i].identity||next[i].name!=windows[i].name||next[i].icon!=windows[i].icon) {same=false;break;}
        windows=std::move(next);
        for(auto& r:rails) {
            const bool hide=fullscreen&&fullscreen(r->monitor);
            if(hide) { closePeek(); ShowWindow(r->glass.fin,SW_HIDE); }
            else { if(!same) rebuild(*r); else for(auto& c:r->cards) preview(*r,c); ShowWindow(r->glass.fin,SW_SHOWNOACTIVATE); draw(*r); }
        }
        pollCapture();
    }
    void stop() {
        closePeek(); rails.clear(); windows.clear();stampa::retain(false);
        if(capture) stampa::abbandona(capture); capture=nullptr; capturing={};
    }
    size_t windowCount() const {return windows.size();}
    size_t railCount() const {return rails.size();}
    size_t cachedPreviewCount()const{size_t n=0;for(const auto&r:rails)for(const auto&c:r->cards)if(c.minimized&&c.bitmap)n++;return n;}
    size_t snapshotBytes() const {std::lock_guard<std::mutex> lock(stampa::savedMutex);size_t n=0;for(auto& s:stampa::saved)n+=s.second->pixels.size()*4;return n;}
    ~Manager() {stop();}
    Manager():peek(this) {}
private:
    std::vector<Window> windows;
    std::vector<std::unique_ptr<Rail>> rails;
    Peek peek;
    stampa::Lavoro* capture=nullptr;
    Window capturing;
    ULONGLONG captureAt=0;
    void activate(const Window& w) {Window copy=w;closePeek();if(copy.valid()&&activation)activation(copy.hwnd);}
    void closePeek() {peek.close();}
    bool pointerInside() const {
        POINT p{};GetCursorPos(&p);HWND h=GetAncestor(WindowFromPoint(p),GA_ROOT);
        if(peek.glass.fin&&h==peek.glass.fin)return true;
        for(const auto& r:rails)if(h==r->glass.fin)return true;return false;
    }
    void scheduleClosePeek(){if(peek.glass.fin)SetTimer(peek.glass.fin,2,180,nullptr);}
    void rebuildAll() {
        closePeek();rails.clear();topologyChanged=false;
        windows=inventory?inventory():std::vector<Window>{};
        std::vector<HMONITOR> monitors;
        EnumDisplayMonitors(nullptr,nullptr,[](HMONITOR m,HDC,LPRECT,LPARAM p)->BOOL {reinterpret_cast<std::vector<HMONITOR>*>(p)->push_back(m);return TRUE;},reinterpret_cast<LPARAM>(&monitors));
        HMONITOR chosen=MonitorFromRect(&vetro::schermoDock(),MONITOR_DEFAULTTOPRIMARY);
        if(settings.monitor==1) chosen=MonitorFromPoint({0,0},MONITOR_DEFAULTTOPRIMARY);
        if(settings.monitor==3) for(auto m:monitors) {MONITORINFOEXW mi{};mi.cbSize=sizeof(mi);GetMonitorInfoW(m,&mi);if(settings.display==mi.szDevice)chosen=m;}
        for(auto m:monitors) {
            if(settings.monitor!=2&&m!=chosen) continue;
            auto r=std::make_unique<Rail>(this);r->monitor=m;
            if(!r->glass.crea(L"TLauncher.Native.Stage",vetro::procedura<Rail>,r.get(),false)) continue;
            r->glass.dpi=vetro::dpiMonitor(m);r->text=r->glass.formato(11*r->glass.dpi);
            rebuild(*r);rails.push_back(std::move(r));
        }
    }
    RECT work(Rail& rail) const {
        MONITORINFO mi{};mi.cbSize=sizeof(mi);GetMonitorInfoW(rail.monitor,&mi);
        RECT area=menu::areaLibera(rail.monitor,mi.rcWork);
        // Reserve the actual Notch/Dock footprint without altering Windows work areas.
        if(reserved) {RECT r=reserved(rail.monitor),over{};if(IntersectRect(&over,&area,&r)) {
            if(r.top<=area.top&&r.bottom<area.bottom) area.top=r.bottom;
            else if(r.bottom>=area.bottom&&r.top>area.top)area.bottom=r.top;
        }}
        if(exclusions){
            const int width=int((settings.height*1.55+20)*rail.glass.dpi),gap=int(settings.edgeGap*rail.glass.dpi);
            RECT side{settings.right?area.right-width-gap:area.left+gap,area.top,settings.right?area.right-gap:area.left+width+gap,area.bottom};
            for(auto obstacle:exclusions(rail.monitor)){RECT overlap{};if(!IntersectRect(&overlap,&side,&obstacle))continue;
                if(overlap.top-area.top>area.bottom-overlap.bottom)area.bottom=overlap.top-gap;else area.top=overlap.bottom+gap;
                side.top=area.top;side.bottom=area.bottom;
            }
        }
        return area;
    }
    void layout(Rail& rail) {
        const RECT area=work(rail);const float d=rail.glass.dpi;
        const int pad=int(10*d),ch=int(settings.height*d),gap=int(settings.spacing*d),edge=int(settings.edgeGap*d);
        const int width=int(settings.height*1.55*d)+2*pad;
        const int h=std::max(1,std::min(int(area.bottom-area.top-2*edge),int(rail.cards.size())*(ch+gap)+2*pad));
        const int x=settings.right?area.right-width-edge:area.left+edge;
        rail.glass.posiziona(rail.hidden?(settings.right?area.right-int(5*d):area.left):x,area.top+std::max(edge,int((area.bottom-area.top-h)/2)),rail.hidden?int(5*d):width,std::max(1,h));
        rail.glass.alfa(settings.opacity/100.0);
        for(size_t i=0;i<rail.cards.size();++i) {
            auto& c=rail.cards[i];int inset=settings.perspective?int((i%3)*4*d):0;
            const int top=pad+int(i)*(ch+gap);
            c.rect={pad+inset,top,width-pad-inset,top+ch};
            preview(rail,c);
        }
        if(!(fullscreen&&fullscreen(rail.monitor)))ShowWindow(rail.glass.fin,SW_SHOWNOACTIVATE);
        draw(rail);
    }
    void rebuild(Rail& rail) {
        closePeek();rail.clear();rail.hover=-1;
        std::vector<Card> groups;
        for(const auto& w:windows) {
            if(!w.valid()||(settings.monitor==2&&MonitorFromWindow(w.hwnd,MONITOR_DEFAULTTONEAREST)!=rail.monitor))continue;
            auto it=std::find_if(groups.begin(),groups.end(),[&](Card& c){return c.source().identity==w.identity;});
            if(it==groups.end()) {Card c;c.windows.push_back(w);groups.push_back(std::move(c));}else it->windows.push_back(w);
        }
        const RECT a=work(rail);const float d=rail.glass.dpi;
        rail.capacity=std::max(1,int((a.bottom-a.top-2*(settings.edgeGap+10)*d)/((settings.height+settings.spacing)*d)));
        rail.page=std::clamp(rail.page,0,std::max(0,(int(groups.size())-1)/rail.capacity));
        int from=rail.page*rail.capacity,to=std::min(int(groups.size()),from+rail.capacity);
        for(int i=from;i<to;++i)rail.cards.push_back(std::move(groups[i]));
        layout(rail);
    }
    void requestCapture(const Window& w,bool refresh=false) {
        if(capture||!w.valid()||IsIconic(w.hwnd))return;
        if(!refresh&&stampa::lastImage(w.hwnd))return;
        RECT r{};GetWindowRect(w.hwnd,&r);
        // Bound the shared service input; reject enormous surfaces rather than allocating them.
        if((long long)(r.right-r.left)*(r.bottom-r.top)>12000000)return;
        capture=stampa::avvia(w.hwnd,r.right-r.left,r.bottom-r.top,THREAD_PRIORITY_BELOW_NORMAL);
        if(capture){capturing=w;captureAt=GetTickCount64();}
    }
    void pollCapture() {
        if(!capture)return;
        if(!stampa::pronta(capture)) {if(GetTickCount64()-captureAt>800){stampa::abbandona(capture);capture=nullptr;}return;}
        const int w=capture->w,h=capture->hh;std::vector<UINT> pixels;
        const bool ok=stampa::prendi(capture,pixels);capture=nullptr;
        if(!ok||!capturing.valid())return;
    }
    ID2D1Bitmap* snapshot(const Window& w,vetro::Finestra& v) {
        if(!w.valid())return nullptr;
        if(auto shared=stampa::lastImage(w.hwnd);shared&&shared->pid==w.pid&&shared->tid==w.tid&&(!w.generation||shared->generation==w.generation)){
            ID2D1Bitmap* b=nullptr;v.rt->CreateBitmap(D2D1::SizeU(shared->w,shared->h),shared->pixels.data(),shared->w*4,D2D1::BitmapProperties(D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM,D2D1_ALPHA_MODE_PREMULTIPLIED)),&b);return b;
        }
        return nullptr;
    }
    void preview(Rail& rail,Card& c) {
        const Window& w=c.source();
        if(c.registered==w.hwnd&&c.minimized==bool(IsIconic(w.hwnd))&&c.generation==rail.glass.giroRt&&!rail.hidden&&(c.thumbnail||c.bitmap)) {
            if(c.thumbnail){SIZE sz{};DwmQueryThumbnailSourceSize(c.thumbnail,&sz);RECT r=c.rect;r.bottom-=int(25*rail.glass.dpi);r=aspectFit(sz.cx,sz.cy,r);DWM_THUMBNAIL_PROPERTIES p{};p.dwFlags=DWM_TNP_RECTDESTINATION;p.rcDestination=r;DwmUpdateThumbnailProperties(c.thumbnail,&p);}return;
        }
        if(c.generation!=rail.glass.giroRt&&c.icon){c.icon->Release();c.icon=nullptr;}
        if(c.thumbnail)DwmUnregisterThumbnail(c.thumbnail);c.thumbnail=nullptr;
        if(c.bitmap)c.bitmap->Release();c.bitmap=nullptr;
        c.registered=w.hwnd;c.minimized=IsIconic(w.hwnd);c.generation=rail.glass.giroRt;
        if(!w.valid()||rail.hidden)return;
        if(IsIconic(w.hwnd)) c.bitmap=snapshot(w,rail.glass);
        else if(SUCCEEDED(DwmRegisterThumbnail(rail.glass.fin,w.hwnd,&c.thumbnail))) {
            SIZE sz{};DwmQueryThumbnailSourceSize(c.thumbnail,&sz);RECT r=c.rect;r.bottom-=int(25*rail.glass.dpi);r=aspectFit(sz.cx,sz.cy,r);
            DWM_THUMBNAIL_PROPERTIES p{};p.dwFlags=DWM_TNP_RECTDESTINATION|DWM_TNP_VISIBLE|DWM_TNP_OPACITY|DWM_TNP_SOURCECLIENTAREAONLY;p.rcDestination=r;p.fVisible=TRUE;p.opacity=255;p.fSourceClientAreaOnly=FALSE;DwmUpdateThumbnailProperties(c.thumbnail,&p);
            requestCapture(w);
        }
    }
    void draw(Rail& rail) {
        auto& v=rail.glass;if(!v.inizia())return;
        if(rail.hidden){v.finisci();return;}
        const float d=v.dpi;
        for(size_t i=0;i<rail.cards.size();++i) {
            auto& c=rail.cards[i];const auto& w=c.source();if(!w.valid())continue;
            auto r=c.rect;D2D1_RECT_F f{float(r.left),float(r.top),float(r.right),float(r.bottom)};
            v.pen->SetColor(v.inchiostro(i==size_t(rail.hover)?.20f:.07f));v.rt->FillRoundedRectangle({f,8*d,8*d},v.pen);
            if(GetAncestor(GetForegroundWindow(),GA_ROOT)==w.hwnd){v.pen->SetColor(D2D1::ColorF(.1f,.55f,1));v.rt->DrawRoundedRectangle({f,8*d,8*d},v.pen,2*d);}
            if(c.bitmap){auto size=c.bitmap->GetPixelSize();RECT dest=r;dest.bottom-=int(25*d);dest=aspectFit(size.width,size.height,dest,false);v.rt->DrawBitmap(c.bitmap,{float(dest.left),float(dest.top),float(dest.right),float(dest.bottom)});}
            if(!c.icon&&icon)c.icon=icon(v.rt,w.icon,96);
            if(c.icon)v.rt->DrawBitmap(c.icon,{f.left+4*d,f.bottom-23*d,f.left+24*d,f.bottom-3*d});
            std::wstring label=w.name;if(c.windows.size()>1)label+=L" ("+std::to_wstring(c.current+1)+L"/"+std::to_wstring(c.windows.size())+L")";
            v.testo(label,rail.text,{f.left+28*d,f.bottom-25*d,f.right-4*d,f.bottom},v.inchiostro());
        }
        v.finisci();
    }
    void openPeek(Rail& rail,const Window& w) {
        closePeek();if(!settings.peek||!w.valid())return;
        if(!peek.glass.crea(L"TLauncher.Native.StagePeek",vetro::procedura<Peek>,&peek,false))return;
        peek.source=w;const float d=rail.glass.dpi;RECT a=work(rail),rr{};GetWindowRect(rail.glass.fin,&rr);
        RECT wr{};GetWindowRect(w.hwnd,&wr);int sw=wr.right-wr.left,sh=wr.bottom-wr.top;
        bool cached=IsIconic(w.hwnd)||!settings.peekLive;
        if(cached){peek.bitmap=snapshot(w,peek.glass);if(!peek.bitmap){closePeek();return;}auto s=peek.bitmap->GetPixelSize();sw=s.width;sh=s.height;}
        const int available=int(a.right-a.left-(rr.right-rr.left)-32*d);
        if(available<80*d||a.bottom-a.top<96*d||sw<=0||sh<=0){closePeek();return;}
        peek.glass.dpi=d;
        RECT r=aspectFit(sw,sh,{0,0,std::min(int(settings.peekSize*d),available),int((a.bottom-a.top)*.8)},!cached);
        int width=r.right-r.left+16,height=r.bottom-r.top+16;
        int x=std::clamp(int(settings.right?rr.left-width-int(12*d):rr.right+int(12*d)),int(a.left),std::max(int(a.left),int(a.right)-width)),y=std::clamp(int(rr.top),int(a.top),std::max(int(a.top),int(a.bottom)-height));
        peek.glass.posiziona(x,y,width,height);peek.glass.alfa(settings.peekAnimation?0:1);
        if(!cached&&SUCCEEDED(DwmRegisterThumbnail(peek.glass.fin,w.hwnd,&peek.thumbnail))) {
            SIZE sz{};DwmQueryThumbnailSourceSize(peek.thumbnail,&sz);RECT dest=aspectFit(sz.cx,sz.cy,{8,8,width-8,height-8});
            DWM_THUMBNAIL_PROPERTIES p{};p.dwFlags=DWM_TNP_RECTDESTINATION|DWM_TNP_VISIBLE|DWM_TNP_OPACITY;p.rcDestination=dest;p.fVisible=TRUE;p.opacity=255;DwmUpdateThumbnailProperties(peek.thumbnail,&p);
        }
        ShowWindow(peek.glass.fin,SW_SHOWNOACTIVATE);peek.draw();
        if(settings.peekAnimation){peek.start=vetro::secondi();vetro::battito(peek.glass.fin,1);}
    }
};
} // namespace stage
