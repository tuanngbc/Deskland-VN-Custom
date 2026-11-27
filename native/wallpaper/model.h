// New native Phase 2 model. Legacy code is reference only; no desktop runtime ABI.
#pragma once
#include <windows.h>
#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <map>
#include <string>
#include <vector>

namespace wallpaper {
using std::sin;using std::cos;using std::exp;using std::sqrt;using std::log;
using std::floor;using std::ceil;using std::abs;using std::hypot;
struct Param { const char* key; const wchar_t* name; double min, max, step, initial; };
struct Scene { const char* id; const wchar_t* name; const wchar_t* shader; const wchar_t* thumbnail; std::vector<Param> params; };
const std::vector<Scene>& catalog();
inline int sceneIndex(const std::string& id) {
    const auto& c=catalog();for(size_t i=0;i<c.size();++i)if(id==c[i].id)return int(i);return -1;
}
enum class Layout { Independent, Synchronized, Panorama };
enum class WeatherMode { Clear, Rain, Snow, Cloudy, Live, Offline };
struct Monitor {
    std::wstring id; RECT rect{}; UINT dpi=96; bool primary=false;
    bool operator==(const Monitor& b) const {return id==b.id&&EqualRect(&rect,&b.rect)&&dpi==b.dpi&&primary==b.primary;}
};
inline std::vector<Monitor> topology() {
    std::vector<Monitor> monitors;
    EnumDisplayMonitors(nullptr,nullptr,[](HMONITOR h,HDC,LPRECT,LPARAM p)->BOOL {
        MONITORINFOEXW m{};m.cbSize=sizeof(m);if(!GetMonitorInfoW(h,&m))return TRUE;
        // Physical coordinates are authoritative. DPI is metadata, never a scale
        // applied again to the panorama's global origin or size.
        UINT x=96,y=96;using Dpi=HRESULT(WINAPI*)(HMONITOR,int,UINT*,UINT*);
        static HMODULE sh=LoadLibraryExW(L"shcore.dll",nullptr,LOAD_LIBRARY_SEARCH_SYSTEM32);
        static auto dpi=sh?reinterpret_cast<Dpi>(GetProcAddress(sh,"GetDpiForMonitor")):nullptr;
        if(dpi) dpi(h,0,&x,&y);
        auto& out=*reinterpret_cast<std::vector<Monitor>*>(p);
        if(out.size()<16)out.push_back({m.szDevice,m.rcMonitor,x,bool(m.dwFlags&MONITORINFOF_PRIMARY)});
        return TRUE;
    },reinterpret_cast<LPARAM>(&monitors));
    std::sort(monitors.begin(),monitors.end(),[](const Monitor&a,const Monitor&b){return a.id<b.id;});return monitors;
}
inline RECT bounds(const std::vector<Monitor>& monitors) {
    if(monitors.empty())return {};
    RECT r=monitors.front().rect;for(const auto&m:monitors){r.left=std::min(r.left,m.rect.left);r.top=std::min(r.top,m.rect.top);r.right=std::max(r.right,m.rect.right);r.bottom=std::max(r.bottom,m.rect.bottom);}return r;
}
struct Assignment { bool enabled=true; int scene=0; std::map<std::string,double> params; };
struct Config {
    bool enabled=false, paused=false, pauseCovered=true, solar=true;
    Layout layout=Layout::Synchronized; WeatherMode weather=WeatherMode::Clear;
    int fps=25, scene=0; float resolution=1.f, manualHour=12.f, wind=1.f;
    bool clockLighting=true; std::map<std::wstring,Assignment> displays;
    std::map<std::string,std::map<std::string,double>> sceneParams;
};
inline double parameter(const Config& c,int scene,const char* key) {
    if(scene<0||scene>=int(catalog().size()))return 0;
    const auto& s=catalog()[scene];
    for(const auto&p:s.params)if(std::string(key)==p.key){
        auto i=c.sceneParams.find(s.id);double value=p.initial;
        if(i!=c.sceneParams.end()){auto j=i->second.find(key);if(j!=i->second.end())value=j->second;}
        return std::isfinite(value)?std::clamp(value,p.min,p.max):p.initial;
    }return 0;
}
inline void validate(Config& c) {
    c.fps=std::clamp(c.fps,5,60);c.scene=std::clamp(c.scene,0,int(catalog().size())-1);
    if(int(c.layout)<0||int(c.layout)>2)c.layout=Layout::Synchronized;
    if(int(c.weather)<0||int(c.weather)>5)c.weather=WeatherMode::Clear;
    c.resolution=std::isfinite(c.resolution)?std::clamp(c.resolution,.5f,1.f):1.f;
    c.manualHour=std::isfinite(c.manualHour)?std::clamp(c.manualHour,0.f,24.f):12.f;
    c.wind=std::isfinite(c.wind)?std::clamp(c.wind,0.f,2.f):1.f;
    for(auto& [id,a]:c.displays){a.scene=std::clamp(a.scene,0,int(catalog().size())-1);}
}
struct Weather { bool available=false; double latitude=0,longitude=0; float rain=0,snow=0,cloud=0,wind=0; uint64_t updated=0; };
struct Sun {float east=0,north=0,up=0;};
// Audited solar mathematics from USNO and src/solar.rs; no runtime dependency.
inline Sun solar(double unixSeconds,double latitude,double longitude) {
    constexpr double rad=3.14159265358979323846/180;
    const double d=unixSeconds/86400+2440587.5-2451545.;
    const double g=(357.529+.98560028*d)*rad;
    const double l=(280.459+.98564736*d+1.915*sin(g)+.020*sin(2*g))*rad;
    const double e=(23.439-.00000036*d)*rad;
    const double ra=atan2(cos(e)*sin(l),cos(l)),dec=asin(sin(e)*sin(l));
    const double ha=(280.46061837+360.98564736629*d+longitude)*rad-ra,lat=latitude*rad;
    return {float(-cos(dec)*sin(ha)),float(sin(dec)*cos(lat)-cos(dec)*cos(ha)*sin(lat)),float(sin(dec)*sin(lat)+cos(dec)*cos(ha)*cos(lat))};
}
inline Weather fromWmo(int code,float wind,double lat,double lon) {
    Weather w;w.available=true;w.latitude=lat;w.longitude=lon;w.wind=std::clamp(wind/20.f,0.f,2.f);w.updated=GetTickCount64();
    w.snow=((code>=71&&code<=77)||(code>=85&&code<=86))?1.f:0;
    w.rain=((code>=51&&code<=67)||(code>=80&&code<=82)||code>=95)?1.f:0;
    w.cloud=code==0?0.f:code==1?.2f:code==2?.55f:1.f;return w;
}
struct Snapshot {
    Config config;std::vector<Monitor> monitors;Weather weather;
    std::vector<RECT> opaqueWindows,icons;
    POINT pointer{};bool desktopPointer=false,click=false,locked=false,displayOff=false;
};
struct View { float x=0,y=0,w=0,h=0,worldW=0,worldH=0; };
inline View viewport(const Monitor& monitor,const RECT& all,Layout layout) {
    const auto&r=monitor.rect;
    if(layout==Layout::Panorama)return {float(r.left-all.left),float(r.top-all.top),float(r.right-r.left),float(r.bottom-r.top),float(all.right-all.left),float(all.bottom-all.top)};
    return {0,0,float(r.right-r.left),float(r.bottom-r.top),float(r.right-r.left),float(r.bottom-r.top)};
}
inline bool covered(const RECT& r,const std::vector<RECT>& windows) {
    // Exact rectangle subtraction handles several opaque windows covering a monitor.
    std::vector<RECT> remaining{r};
    for(const auto&w:windows){std::vector<RECT> next;
        for(const auto&p:remaining){RECT q;if(!IntersectRect(&q,&p,&w)){next.push_back(p);continue;}
            for(RECT t:std::array<RECT,4>{{{p.left,p.top,p.right,q.top},{p.left,q.bottom,p.right,p.bottom},{p.left,q.top,q.left,q.bottom},{q.right,q.top,p.right,q.bottom}}})
                if(t.right>t.left&&t.bottom>t.top)next.push_back(t);
        }remaining=std::move(next);if(remaining.empty())return true;if(remaining.size()>128)return false;
    }return remaining.empty();
}
}
