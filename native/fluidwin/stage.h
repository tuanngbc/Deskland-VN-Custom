// Native Stage: Dock owns inventory and activation. No foreign HWND placement/focus
// changes during hover; DWM owns live thumbnails, stampa owns asynchronous snapshots.
#pragma once
#include "vetro.h"
#include "menu.h"
#include "stampa.h"
#include <windowsx.h>
#include <memory>
#include <unordered_map>
#include <utility>

namespace stage {
struct Settings {
    bool enabled = false, perspective = false, autoHide = false, peek = true;
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
        unsigned generation = 0;ULONGLONG snapshotAt=0;
        double opacity=0, fromOpacity=0, targetOpacity=0, transitionAt=0;
        Card()=default;
        Card(const Card&)=delete;
        Card& operator=(const Card&)=delete;
        Card(Card&& other) noexcept { swap(other); }
        Card& operator=(Card&& other) noexcept { if(this!=&other){release();swap(other);}return *this; }
        ~Card(){release();}
        void release(){
            if(thumbnail)DwmUnregisterThumbnail(thumbnail);thumbnail=nullptr;
            if(bitmap)bitmap->Release();bitmap=nullptr;
            if(icon)icon->Release();icon=nullptr;
        }
        void swap(Card& o) noexcept {
            windows.swap(o.windows);std::swap(current,o.current);std::swap(rect,o.rect);
            std::swap(thumbnail,o.thumbnail);std::swap(bitmap,o.bitmap);std::swap(icon,o.icon);
            std::swap(registered,o.registered);std::swap(minimized,o.minimized);
            std::swap(generation,o.generation);std::swap(snapshotAt,o.snapshotAt);
            std::swap(opacity,o.opacity);std::swap(fromOpacity,o.fromOpacity);
            std::swap(targetOpacity,o.targetOpacity);std::swap(transitionAt,o.transitionAt);
        }
        void sample(double now){opacity=fromOpacity+(targetOpacity-fromOpacity)*vetro::uscita((now-transitionAt)/.18);}
        void fade(double to,double now){sample(now);fromOpacity=opacity;targetOpacity=to;transitionAt=now;}
        bool moving(double now)const{return fromOpacity!=targetOpacity&&now-transitionAt<.18;}
        Window& source() { return windows[current % windows.size()]; }
    };
    struct Rail {
        Manager* owner;
        HMONITOR monitor = nullptr;
        vetro::Finestra glass;
        IDWriteTextFormat* text = nullptr;
        std::vector<Card> cards, departing;
        bool animating=false;unsigned animationFrames=0,renderFrames=0;
        HWND foreground=nullptr;int language=-1;
        int hover = -1, page = 0, capacity = 1;
        bool hidden = false,blocked=false,suppressed=false;RECT available{};
        ULONGLONG hoverAt = 0;
        explicit Rail(Manager* m): owner(m) {}
        ~Rail() { clear(); if (text) text->Release(); glass.chiudi(); }
        void clear() {
            vetro::fermaBattito(glass.fin,4);animating=false;
            cards.clear();departing.clear();
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
                    if(w==4){owner->animate(*this);return true;}
                    KillTimer(h,(UINT_PTR)w);
                    if(w==2&&hover>=0&&hover<int(cards.size())) owner->openPeek(*this,cards[hover].source());
                    if(w==3&&hover<0){if(menu::aperti||owner->pointerInside())SetTimer(h,3,350,nullptr);else{hidden=true;owner->closePeek();owner->layout(*this);}}
                    return true;
                case WM_LBUTTONUP: {
                    const int i=hit({GET_X_LPARAM(l),GET_Y_LPARAM(l)});
                    if(i>=0&&owner->settings.clickActivate) owner->activate(cards[i].source());
                    return true;
                }
                case WM_RBUTTONUP: {
                    int i=hit({GET_X_LPARAM(l),GET_Y_LPARAM(l)});POINT p{GET_X_LPARAM(l),GET_Y_LPARAM(l)};ClientToScreen(h,&p);
                    if(i>=0)owner->groupMenu(*this,i,p);return true;
                }
                case WM_MOUSEWHEEL:
                    if(hover>=0&&hover<int(cards.size())&&cards[hover].windows.size()>1) {
                        auto& c=cards[hover]; const auto n=c.windows.size();
                        c.current=(c.current+n+(GET_WHEEL_DELTA_WPARAM(w)>0?1:-1))%n;
                        owner->closePeek(); owner->preview(*this,c); owner->draw(*this);
                    } else { page=std::max(0,page+(GET_WHEEL_DELTA_WPARAM(w)>0?-1:1)); owner->rebuild(*this); }
                    return true;
                case WM_DISPLAYCHANGE: case WM_DPICHANGED: owner->topologyChanged=true; return true;
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
        double start = 0;unsigned generation=0;
        explicit Peek(Manager* m): owner(m) {}
        ~Peek() { close(); }
        void close() {
            if(thumbnail) DwmUnregisterThumbnail(thumbnail); thumbnail=nullptr;
            if(bitmap) bitmap->Release(); bitmap=nullptr;
            vetro::fermaBattito(glass.fin,1); glass.chiudi(); source={};
        }
        void draw() {
            if(!glass.inizia()) return;
            if(generation!=glass.giroRt){generation=glass.giroRt;if(bitmap){bitmap->Release();bitmap=owner->snapshot(source,glass);}}
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
        // Dock may still retain a destroyed/reused HWND until its queued
        // inventory refresh. A stale record must not suppress the exit fade.
        std::erase_if(next,[](const Window& w){return !w.valid();});
        bool same=next.size()==windows.size();
        if(same) for(size_t i=0;i<next.size();++i) if(next[i].hwnd!=windows[i].hwnd||next[i].pid!=windows[i].pid||next[i].tid!=windows[i].tid||next[i].generation!=windows[i].generation||next[i].identity!=windows[i].identity||next[i].name!=windows[i].name||next[i].icon!=windows[i].icon) {same=false;break;}
        windows=std::move(next);
        for(auto& r:rails) {
            const bool hide=fullscreen&&fullscreen(r->monitor);
            const bool resumed=r->suppressed&&!hide;r->suppressed=hide;
            const RECT area=work(*r);
            if(hide) { settle(*r);closePeek();for(auto& c:r->cards)preview(*r,c);ShowWindow(r->glass.fin,SW_HIDE); }
            else {
                if(!same||resumed||!EqualRect(&area,&r->available))rebuild(*r);
                else if(!r->blocked){
                    bool repaint=r->foreground!=GetAncestor(GetForegroundWindow(),GA_ROOT)||r->language!=testi::attuale();
                    for(auto& c:r->cards){
                        const auto priorSnapshot=c.snapshotAt;const auto priorGeneration=c.generation;
                        const auto priorWindow=c.registered;const bool priorMinimized=c.minimized;
                        const bool priorImage=c.bitmap||c.thumbnail;
                        preview(*r,c);
                        repaint|=priorSnapshot!=c.snapshotAt||priorGeneration!=c.generation||priorWindow!=c.registered||priorMinimized!=c.minimized||priorImage!=bool(c.bitmap||c.thumbnail);
                        if(!r->hidden&&!c.icon&&icon){c.icon=icon(r->glass.rt,c.source().icon,96);repaint|=c.icon!=nullptr;}
                    }
                    if(!r->cards.empty()||!r->departing.empty()){
                        if(!IsWindowVisible(r->glass.fin))ShowWindow(r->glass.fin,SW_SHOWNOACTIVATE);
                        if(repaint&&!r->hidden)draw(*r);
                    }
                }
            }

        }
        pollCapture();
    }
    void stop() {
        closePeek(); rails.clear(); windows.clear();stampa::retain(false);
        if(capture) stampa::abbandona(capture); capture=nullptr; capturing={};
    }
    size_t windowCount() const {return windows.size();}
    size_t railCount() const {return rails.size();}
#ifdef TLAUNCHER_NATIVE_CI
    size_t ciThumbnailCount()const{size_t n=0;for(const auto&r:rails)for(const auto&c:r->cards)if(c.thumbnail)n++;return n;}
    bool ciAnimating()const{for(const auto&r:rails)if(r->animating)return true;return false;}
    size_t ciDeparting()const{size_t n=0;for(const auto&r:rails)n+=r->departing.size();return n;}
    HTHUMBNAIL ciThumbnail()const{return rails.empty()||rails[0]->cards.empty()?nullptr:rails[0]->cards[0].thumbnail;}
    double ciOpacity()const{return rails.empty()||rails[0]->cards.empty()?0:rails[0]->cards[0].opacity;}
    unsigned ciRenderFrames()const{unsigned n=0;for(const auto&r:rails)n+=r->renderFrames;return n;}
    unsigned ciAnimationFrames()const{unsigned n=0;for(const auto&r:rails)n+=r->animationFrames;return n;}
    HWND ciSelected()const{return rails.empty()||rails[0]->cards.empty()?nullptr:rails[0]->cards[0].source().hwnd;}
#endif
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
        if(reserved){RECT r=reserved(rail.monitor),over{};if(IntersectRect(&over,&area,&r)){
            if(r.right-r.left>=r.bottom-r.top){if(r.bottom>=mi.rcMonitor.bottom-1)area.bottom=std::min(area.bottom,r.top);else if(r.top<=mi.rcMonitor.top+1)area.top=std::max(area.top,r.bottom);}
            else{if(r.left<=mi.rcMonitor.left+1)area.left=std::max(area.left,r.right);else if(r.right>=mi.rcMonitor.right-1)area.right=std::min(area.right,r.left);}
        }}
        if(exclusions){
            const int width=int((settings.height*1.55+20)*rail.glass.dpi),gap=int(settings.edgeGap*rail.glass.dpi);
            RECT side{settings.right?area.right-width-gap:area.left+gap,area.top,settings.right?area.right-gap:area.left+width+gap,area.bottom};
            // Subtract all intersecting intervals before choosing the largest
            // free section. The result cannot depend on popup enumeration order.
            std::vector<std::pair<LONG,LONG>> free{{area.top,area.bottom}};
            for(auto obstacle:exclusions(rail.monitor)){RECT overlap{};if(!IntersectRect(&overlap,&side,&obstacle))continue;
                std::vector<std::pair<LONG,LONG>> next;for(auto [top,bottom]:free){
                    if(overlap.bottom+gap<=top||overlap.top-gap>=bottom)next.push_back({top,bottom});
                    else{if(overlap.top-gap>top)next.push_back({top,std::min(bottom,overlap.top-gap)});if(overlap.bottom+gap<bottom)next.push_back({std::max(top,overlap.bottom+gap),bottom});}
                }free=std::move(next);
            }
            auto best=std::max_element(free.begin(),free.end(),[](auto a,auto b){return a.second-a.first<b.second-b.first;});
            if(best==free.end())area.bottom=area.top;else{area.top=best->first;area.bottom=best->second;}
        }
        return area;
    }
    void settle(Rail& rail){
        vetro::fermaBattito(rail.glass.fin,4);rail.animating=false;rail.departing.clear();
        for(auto& c:rail.cards)c.opacity=c.fromOpacity=c.targetOpacity=1;
    }
    void animate(Rail& rail){
        if(!rail.animating)return;
        if(rail.hidden||rail.blocked||rail.suppressed){settle(rail);return;}
        const double now=vetro::secondi();bool moving=false;
        for(auto& c:rail.cards){c.sample(now);moving|=c.moving(now);preview(rail,c);}
        for(auto& c:rail.departing){c.sample(now);moving|=c.moving(now);}
        rail.animationFrames++;
        const bool removed=std::erase_if(rail.departing,[&](const Card& c){return !c.moving(now);})!=0;
        if(!moving){vetro::fermaBattito(rail.glass.fin,4);rail.animating=false;}
        if(removed)layout(rail);else draw(rail);
    }
    void layout(Rail& rail) {
        const RECT area=work(rail);const float d=rail.glass.dpi;
        const int pad=int(10*d),ch=int(settings.height*d),gap=int(settings.spacing*d),edge=int(settings.edgeGap*d);
        const int width=int(settings.height*1.55*d)+2*pad;rail.available=area;
        rail.blocked=area.right-area.left<width+2*edge||area.bottom-area.top<(rail.cards.empty()?2*pad:ch+2*pad)+2*edge;
        if(rail.blocked){settle(rail);closePeek();for(auto& c:rail.cards)preview(rail,c);ShowWindow(rail.glass.fin,SW_HIDE);return;}
        const int h=std::max(1,std::min(int(area.bottom-area.top-2*edge),int(rail.cards.size())*(ch+gap)+2*pad));
        const int x=settings.right?area.right-width-edge:area.left+edge;
        const int y=area.top+std::max(edge,int((area.bottom-area.top-h)/2));
        RECT previous{};GetWindowRect(rail.glass.fin,&previous);
        int top=y,bottom=y+h;
        // Keep the old viewport until outgoing cards finish fading. Both old
        // and new rectangles remain in the allowed work region.
        if(!rail.departing.empty()&&!rail.hidden){top=std::max(int(area.top),std::min(y,int(previous.top)));bottom=std::min(int(area.bottom),std::max(y+h,int(previous.bottom)));}
        for(auto& c:rail.departing)OffsetRect(&c.rect,previous.left-x,previous.top-top);
        if(rail.hidden||rail.suppressed)settle(rail);
        rail.glass.posiziona(rail.hidden?(settings.right?area.right-int(5*d):area.left):x,top,rail.hidden?int(5*d):width,std::max(1,bottom-top));
        rail.glass.alfa(settings.opacity/100.0);
        for(size_t i=0;i<rail.cards.size();++i) {
            auto& c=rail.cards[i];int inset=0;
            const int cardTop=y-top+pad+int(i)*(ch+gap);
            c.rect={pad+inset,cardTop,width-pad-inset,cardTop+ch};
            preview(rail,c);
        }
        if(!rail.suppressed&&!(fullscreen&&fullscreen(rail.monitor)))ShowWindow(rail.glass.fin,SW_SHOWNOACTIVATE);
        const double now=vetro::secondi();rail.animating=!rail.departing.empty();
        for(auto& c:rail.cards){c.sample(now);rail.animating|=c.moving(now);preview(rail,c);}
        if(rail.cards.empty()&&rail.departing.empty())ShowWindow(rail.glass.fin,SW_HIDE);
        if(rail.animating)vetro::battito(rail.glass.fin,4);else vetro::fermaBattito(rail.glass.fin,4);
        draw(rail);
    }
    void groupMenu(Rail&rail,int index,POINT at){
        closePeek();const auto choices=rail.cards[index].windows;const HWND selected=rail.cards[index].source().hwnd;
        std::vector<menu::Voce> items{menu::titolo(testi::t(L"Finestre dell'app",L"Application windows"))};
        for(size_t i=0;i<choices.size();i++){auto item=menu::voce(choices[i].name,UINT(i+1));item.spuntata=choices[i].hwnd==selected;item.disattivata=!choices[i].valid();items.push_back(std::move(item));}
        const HWND destination=rail.glass.fin;
        UINT choice=menu::mostra(items,at,destination);
        // The shared menu pumps core events: topology/inventory may rebuild or
        // destroy this rail while it is open. Reacquire it before selecting.
        if(!settings.enabled||!choice||choice>choices.size()||!choices[choice-1].valid())return;
        const auto chosen=choices[choice-1];
        for(auto& currentRail:rails)if(currentRail->glass.fin==destination){
            for(auto& card:currentRail->cards)for(size_t i=0;i<card.windows.size();++i)
                if(card.windows[i].hwnd==chosen.hwnd&&card.windows[i].valid()){
                    card.current=i;preview(*currentRail,card);draw(*currentRail);
                }
        }
        activate(chosen);
    }
    void rebuild(Rail& rail) {
        std::unordered_map<std::wstring,HWND> selection;for(auto&c:rail.cards)selection[c.source().identity]=c.source().hwnd;
        closePeek();rail.hover=-1;
        std::vector<Card> previous=std::move(rail.cards);
        std::vector<Card> groups;
        for(const auto& w:windows) {
            if(!w.valid()||(settings.monitor==2&&MonitorFromWindow(w.hwnd,MONITOR_DEFAULTTONEAREST)!=rail.monitor))continue;
            auto it=std::find_if(groups.begin(),groups.end(),[&](Card& c){return c.source().identity==w.identity;});
            if(it==groups.end()) {Card c;c.windows.push_back(w);groups.push_back(std::move(c));}else it->windows.push_back(w);
        }
        for(auto&c:groups){auto old=selection.find(c.source().identity);if(old!=selection.end())for(size_t i=0;i<c.windows.size();i++)if(c.windows[i].hwnd==old->second){c.current=i;break;}}
        const RECT a=work(rail);const float d=rail.glass.dpi;
        rail.capacity=std::max(1,int((a.bottom-a.top-2*(settings.edgeGap+10)*d)/((settings.height+settings.spacing)*d)));
        rail.page=std::clamp(rail.page,0,std::max(0,(int(groups.size())-1)/rail.capacity));
        int from=rail.page*rail.capacity,to=std::min(int(groups.size()),from+rail.capacity);
        const double now=vetro::secondi();
        for(int i=from;i<to;++i){
            auto& next=groups[i];
            auto found=std::find_if(previous.begin(),previous.end(),[&](Card& c){return !c.windows.empty()&&c.source().identity==next.source().identity;});
            if(found!=previous.end()){
                if(found->source().hwnd!=next.source().hwnd||found->source().pid!=next.source().pid||found->source().tid!=next.source().tid||found->source().generation!=next.source().generation)found->release();
                else if(found->source().icon!=next.source().icon&&found->icon){found->icon->Release();found->icon=nullptr;}
                found->windows=std::move(next.windows);found->current=next.current;
                rail.cards.push_back(std::move(*found));
            }else{next.fade(1,now);rail.cards.push_back(std::move(next));}
        }
        for(auto& old:previous)if(!old.windows.empty()){
            // Never keep a live DWM registration or capture request for a
            // removed source. Only the existing validated snapshot/icon fades.
            if(!old.bitmap&&old.source().valid())old.bitmap=snapshot(old.source(),rail.glass);
            if(old.thumbnail)DwmUnregisterThumbnail(old.thumbnail);old.thumbnail=nullptr;
            old.fade(0,now);rail.departing.push_back(std::move(old));
        }
        // Rapid topology/inventory changes cannot accumulate outgoing surfaces.
        while(rail.departing.size()>size_t(rail.capacity)*2)rail.departing.erase(rail.departing.begin());
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
        const Window& w=c.source();auto cached=stampa::lastImage(w.hwnd);const ULONGLONG at=cached?cached->at:0;
        if(c.registered==w.hwnd&&c.minimized==bool(IsIconic(w.hwnd))&&c.generation==rail.glass.giroRt&&c.snapshotAt==at&&!rail.hidden&&!rail.blocked&&!rail.suppressed&&(c.thumbnail||c.bitmap)) {
            if(c.thumbnail){SIZE sz{};DwmQueryThumbnailSourceSize(c.thumbnail,&sz);RECT r=c.rect;r.bottom-=int(25*rail.glass.dpi);r=aspectFit(sz.cx,sz.cy,r);DWM_THUMBNAIL_PROPERTIES p{};p.dwFlags=DWM_TNP_RECTDESTINATION|DWM_TNP_OPACITY;p.rcDestination=r;p.opacity=BYTE(std::lround(c.opacity*255));DwmUpdateThumbnailProperties(c.thumbnail,&p);}return;
        }
        if(c.generation!=rail.glass.giroRt&&c.icon){c.icon->Release();c.icon=nullptr;}
        if(c.thumbnail)DwmUnregisterThumbnail(c.thumbnail);c.thumbnail=nullptr;
        if(c.bitmap)c.bitmap->Release();c.bitmap=nullptr;
        c.registered=w.hwnd;c.minimized=IsIconic(w.hwnd);c.generation=rail.glass.giroRt;c.snapshotAt=at;
        if(!w.valid()||rail.hidden||rail.blocked||rail.suppressed)return;
        if(IsIconic(w.hwnd)||settings.perspective){c.bitmap=snapshot(w,rail.glass);if(!IsIconic(w.hwnd))requestCapture(w);}
        else if(SUCCEEDED(DwmRegisterThumbnail(rail.glass.fin,w.hwnd,&c.thumbnail))) {
            SIZE sz{};DwmQueryThumbnailSourceSize(c.thumbnail,&sz);RECT r=c.rect;r.bottom-=int(25*rail.glass.dpi);r=aspectFit(sz.cx,sz.cy,r);
            DWM_THUMBNAIL_PROPERTIES p{};p.dwFlags=DWM_TNP_RECTDESTINATION|DWM_TNP_VISIBLE|DWM_TNP_OPACITY|DWM_TNP_SOURCECLIENTAREAONLY;p.rcDestination=r;p.fVisible=TRUE;p.opacity=BYTE(std::lround(c.opacity*255));p.fSourceClientAreaOnly=FALSE;DwmUpdateThumbnailProperties(c.thumbnail,&p);
            requestCapture(w);
        }
    }
    void perspectiveBitmap(vetro::Finestra&v,ID2D1Bitmap*bitmap,RECT dest,float opacity)const{
        const auto size=bitmap->GetPixelSize();const float width=float(dest.right-dest.left),height=float(dest.bottom-dest.top);
        if(width<=0||height<=0)return;
        const float cx=(dest.left+dest.right)*.5f,cy=(dest.top+dest.bottom)*.5f;
        // Perspective divide of a plane rotated about Y, sampled in narrow
        // texture strips. HwndRenderTarget has no projective bitmap transform;
        // this retains the accepted native target/device and correct aspect.
        const float angle=settings.right?-.30f:.30f,distance=width*2.8f;
        auto project=[&](float u){float x=(u-.5f)*width,k=distance/(distance+sin(angle)*x);return std::pair<float,float>{cx+cos(angle)*x*k,k};};
        for(int strip=0;strip<64;strip++){const float u0=strip/64.f,u1=(strip+1)/64.f;auto a=project(u0),b=project(u1);float k=project((u0+u1)*.5f).second;
            D2D1_RECT_F source{u0*size.width,0,u1*size.width,float(size.height)};
            v.rt->DrawBitmap(bitmap,{a.first,cy-height*.5f*k,b.first+.12f,cy+height*.5f*k},opacity,D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,&source);
        }
    }
    void draw(Rail& rail) {
        auto& v=rail.glass;if(!v.inizia())return;
        rail.renderFrames++;rail.foreground=GetAncestor(GetForegroundWindow(),GA_ROOT);rail.language=testi::attuale();
        testi::aggiornaFormato(v.fdw,rail.text);
        if(rail.hidden){v.finisci();return;}
        const float d=v.dpi;
        auto drawCard=[&](Card& c,bool outgoing,bool hovered){
            if(c.generation!=v.giroRt){if(outgoing)c.release();else preview(rail,c);}
            const auto& w=c.source();if((!outgoing&&!w.valid())||c.opacity<=0)return;
            const float alpha=float(c.opacity);
            auto r=c.rect;D2D1_RECT_F f{float(r.left),float(r.top),float(r.right),float(r.bottom)};
            v.pen->SetColor(v.inchiostro((hovered?.20f:.07f)*alpha));v.rt->FillRoundedRectangle({f,8*d,8*d},v.pen);
            if(!outgoing&&GetAncestor(GetForegroundWindow(),GA_ROOT)==w.hwnd){v.pen->SetColor(D2D1::ColorF(.1f,.55f,1,alpha));v.rt->DrawRoundedRectangle({f,8*d,8*d},v.pen,2*d);}
            if(c.bitmap){auto size=c.bitmap->GetPixelSize();RECT dest=r;dest.bottom-=int(25*d);dest=aspectFit(size.width,size.height,dest,false);if(settings.perspective)perspectiveBitmap(v,c.bitmap,dest,alpha);else v.rt->DrawBitmap(c.bitmap,{float(dest.left),float(dest.top),float(dest.right),float(dest.bottom)},alpha);}
            if(!outgoing&&!c.icon&&icon)c.icon=icon(v.rt,w.icon,96);
            if(c.icon)v.rt->DrawBitmap(c.icon,{f.left+4*d,f.bottom-23*d,f.left+24*d,f.bottom-3*d},alpha);
            std::wstring label=w.name;if(c.windows.size()>1)label+=L" ("+std::to_wstring(c.current+1)+L"/"+std::to_wstring(c.windows.size())+L")";
            v.testo(label,rail.text,{f.left+28*d,f.bottom-25*d,f.right-4*d,f.bottom},v.inchiostro(alpha));
        };
        for(auto& c:rail.departing)drawCard(c,true,false);
        for(size_t i=0;i<rail.cards.size();++i)drawCard(rail.cards[i],false,i==size_t(rail.hover));
        v.finisci();
    }
    void openPeek(Rail& rail,const Window& w) {
        closePeek();if(!settings.peek||!w.valid()||rail.blocked||rail.suppressed)return;
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
