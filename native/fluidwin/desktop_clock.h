#pragma once
#include "tlauncher_identity.h"
#include "testi.h"
#include <algorithm>
#include <array>
#include <cmath>
#include <shellscalingapi.h>
#include <string>
#include <vector>
#include <windowsx.h>

namespace desktopclock {
struct Settings {
    bool enabled=false, locked=true, clickThrough=true, date=true, seconds=false;
    int style=0, scale=100, opacity=72;
    COLORREF color=RGB(185,231,245);
    std::wstring font, display;
    int x=-1,y=-1;
    bool operator==(const Settings&)const=default;
};
struct Monitor {
    HMONITOR handle=nullptr;
    RECT area{};
    std::wstring name;
    bool primary=false;
};
inline std::vector<Monitor> monitors() {
    std::vector<Monitor> result;
    EnumDisplayMonitors(nullptr,nullptr,[](HMONITOR h,HDC,LPRECT,LPARAM data)->BOOL {
        MONITORINFOEXW info{};info.cbSize=sizeof(info);
        if(GetMonitorInfoW(h,&info))
            reinterpret_cast<std::vector<Monitor>*>(data)->push_back({h,info.rcWork,info.szDevice,bool(info.dwFlags&MONITORINFOF_PRIMARY)});
        return TRUE;
    },reinterpret_cast<LPARAM>(&result));
    return result;
}
inline void registerFonts() {
    static bool loaded=false;
    if(loaded)return;
    loaded=true;
    wchar_t executable[32768]{};
    const DWORD length=GetModuleFileNameW(nullptr,executable,DWORD(std::size(executable)));
    if(!length||length>=std::size(executable))return;
    const auto base=std::filesystem::path(executable).parent_path()/L"assets"/L"widget";
    for(const auto* file:{L"steelfish rg.ttf",L"steelfish rg it.ttf",L"la Compagnie des Ombres.ttf",L"Gravity-Light.ttf"})
        AddFontResourceExW((base/file).c_str(),FR_PRIVATE,nullptr);
}
inline HWND desktopHost() {
    HWND result=nullptr;
    EnumWindows([](HWND h,LPARAM state)->BOOL{
        if(IsWindowVisible(h)&&FindWindowExW(h,nullptr,L"SHELLDLL_DefView",nullptr)){
            *reinterpret_cast<HWND*>(state)=h;
            return FALSE;
        }
        return TRUE;
    },reinterpret_cast<LPARAM>(&result));
    return result;
}

class Clock {
public:
    Clock()=default;
    Clock(const Clock&)=delete;
    Clock& operator=(const Clock&)=delete;
    ~Clock(){stop();}
    std::function<void(const Settings&)> moved;

    void configure(const Settings& next,bool second,bool refresh=false) {
        if(!refresh&&next==config&&hwnd&&language==testi::attuale())return;
        config=next;
        secondClock=second;
        language=testi::attuale();
        if(!config.enabled){stop();return;}
        auto list=monitors();
        auto selected=std::find_if(list.begin(),list.end(),[&](const Monitor& m){return !config.display.empty()?m.name==config.display:m.primary;});
        if(selected==list.end()){stop();return;}
        if(second&&config.display.empty()&&list.size()>1){
            auto alternate=std::find_if(list.begin(),list.end(),[](const Monitor& m){return !m.primary;});
            if(alternate!=list.end())selected=alternate;
        }
        screen=*selected;
        registerFonts();
        if(!hwnd){
            WNDCLASSW cls{};cls.hInstance=GetModuleHandleW(nullptr);cls.lpszClassName=L"TLauncher.Native.DesktopClock";
            cls.lpfnWndProc=procedure;cls.hCursor=LoadCursorW(nullptr,IDC_ARROW);RegisterClassW(&cls);
            hwnd=CreateWindowExW(WS_EX_LAYERED|WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE,cls.lpszClassName,L"TLauncher Desktop Clock",
                WS_POPUP,0,0,1,1,nullptr,nullptr,cls.hInstance,this);
            if(!hwnd)return;
        }
        LONG_PTR flags=GetWindowLongPtrW(hwnd,GWL_EXSTYLE);
        if(config.clickThrough)flags|=WS_EX_TRANSPARENT;
        else flags&=~WS_EX_TRANSPARENT;
        SetWindowLongPtrW(hwnd,GWL_EXSTYLE,flags);
        render();
        ShowWindow(hwnd,SW_SHOWNOACTIVATE);
        order();
    }

    void stop() {
        if(!hwnd)return;
        KillTimer(hwnd,1);
        if(GetCapture()==hwnd)ReleaseCapture();
        DestroyWindow(hwnd);hwnd=nullptr;
    }

    void order() {
        if(!hwnd)return;
        HWND desktop=desktopHost(),above=desktop?GetWindow(desktop,GW_HWNDPREV):nullptr;
        if(above==hwnd)return;
        const UINT flags=SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE|SWP_NOOWNERZORDER;
        if(desktop&&above)SetWindowPos(hwnd,above,0,0,0,0,flags);
        else SetWindowPos(hwnd,HWND_BOTTOM,0,0,0,0,flags);
    }

private:
    Settings config;
    Monitor screen;
    HWND hwnd=nullptr;
    int language=-1;
    bool dragging=false,secondClock=false;
    POINT anchor{},origin{};

    static LRESULT CALLBACK procedure(HWND h,UINT msg,WPARAM w,LPARAM l) {
        if(msg==WM_NCCREATE)SetWindowLongPtrW(h,GWLP_USERDATA,reinterpret_cast<LONG_PTR>(reinterpret_cast<CREATESTRUCTW*>(l)->lpCreateParams));
        auto* self=reinterpret_cast<Clock*>(GetWindowLongPtrW(h,GWLP_USERDATA));
        if(self)switch(msg){
            case WM_MOUSEACTIVATE:return MA_NOACTIVATE;
            case WM_NCHITTEST:if(self->config.clickThrough)return HTTRANSPARENT;break;
            case WM_TIMECHANGE:
            case WM_TIMER:self->render();return 0;
            case WM_LBUTTONDOWN:
                if(!self->config.locked&&!self->config.clickThrough) {
                    RECT bounds{};GetWindowRect(h,&bounds);
                    self->origin={bounds.left,bounds.top};
                    GetCursorPos(&self->anchor);
                    self->dragging=true;SetCapture(h);
                }
                return 0;
            case WM_MOUSEMOVE:
                if(self->dragging){
                    POINT point{};GetCursorPos(&point);
                    SetWindowPos(h,nullptr,self->origin.x+point.x-self->anchor.x,self->origin.y+point.y-self->anchor.y,0,0,SWP_NOSIZE|SWP_NOZORDER|SWP_NOACTIVATE);
                }
                return 0;
            case WM_LBUTTONUP:
                if(self->dragging){self->dragging=false;ReleaseCapture();self->savePosition();}
                return 0;
            case WM_CAPTURECHANGED:self->dragging=false;return 0;
        }
        return DefWindowProcW(h,msg,w,l);
    }

    void savePosition() {
        RECT bounds{};GetWindowRect(hwnd,&bounds);
        const auto current=MonitorFromWindow(hwnd,MONITOR_DEFAULTTONEAREST);
        for(const auto& monitor:monitors())if(monitor.handle==current){screen=monitor;break;}
        config.display=screen.name;
        config.x=bounds.left-screen.area.left;
        config.y=bounds.top-screen.area.top;
        if(moved)moved(config);
        render();
    }

    void render() {
        if(!hwnd)return;
        SYSTEMTIME time{};GetLocalTime(&time);
        const int interval=config.seconds?1000-time.wMilliseconds:60000-time.wSecond*1000-time.wMilliseconds;
        SetTimer(hwnd,1,UINT(std::max(20,interval)),nullptr);
        if(dragging)return;

        UINT dpi=96,yDpi=96;
        if(FAILED(GetDpiForMonitor(screen.handle,MDT_EFFECTIVE_DPI,&dpi,&yDpi)))dpi=96;
        const float scale=config.scale*dpi/9600.f;
        const int width=std::max(1,std::min(int(screen.area.right-screen.area.left),int(std::ceil((config.style==0?500:320)*scale))));
        const int height=std::max(1,std::min(int(screen.area.bottom-screen.area.top),int(std::ceil((config.date?(config.style==0?230:135):105)*scale))));
        HDC screenDC=GetDC(nullptr);
        if(!screenDC)return;
        HDC dc=CreateCompatibleDC(screenDC);
        BITMAPINFO info{};
        info.bmiHeader={sizeof(BITMAPINFOHEADER),width,-height,1,32,BI_RGB,0,0,0,0};
        void* data=nullptr;
        HBITMAP bitmap=dc?CreateDIBSection(screenDC,&info,DIB_RGB_COLORS,&data,nullptr,0):nullptr;
        if(!bitmap||!data){
            if(bitmap)DeleteObject(bitmap);
            if(dc)DeleteDC(dc);
            ReleaseDC(nullptr,screenDC);
            return;
        }
        auto old=SelectObject(dc,bitmap);
        auto* pixels=static_cast<unsigned*>(data);
        std::fill_n(pixels,size_t(width)*height,0u);
        SetBkMode(dc,TRANSPARENT);SetTextColor(dc,RGB(255,255,255));
        wchar_t hour[40]{},weekday[128]{},date[128]{};
        GetTimeFormatEx(testi::locale(),0,&time,config.seconds?L"HH:mm:ss":L"HH:mm",hour,40);
        GetDateFormatEx(testi::locale(),0,&time,L"dddd",weekday,128,nullptr);
        if(config.style==0&&testi::attuale()==testi::INGLESE){
            wchar_t month[48]{};GetDateFormatEx(L"en-US",0,&time,L"MMMM",month,48,nullptr);
            const auto day=time.wDay;
            const wchar_t* suffix=day>=11&&day<=13?L"th":day%10==1?L"st":day%10==2?L"nd":day%10==3?L"rd":L"th";
            swprintf(date,128,L"%u%ls %ls %u",unsigned(day),suffix,month,unsigned(time.wYear));
        }else GetDateFormatEx(testi::locale(),0,&time,L"d MMMM yyyy",date,128,nullptr);
        if(config.style==0)CharUpperBuffW(date,DWORD(wcslen(date)));
        const wchar_t* straight=config.font.empty()?(config.style==0?L"Steelfish Rg":L"Segoe UI Variable Text"):config.font.c_str();
        const wchar_t* script=config.font.empty()&&config.style==0?L"la Compagnie des Ombres":straight;
        const auto draw=[&](const wchar_t* value,const wchar_t* face,int size,int top,int bottom){
            RECT rect{0,LONG(top*scale),LONG(width),std::min(LONG(height),LONG(bottom*scale))};
            HFONT font=CreateFontW((config.style==0?1:-1)*std::max(1,int(size*scale)),0,0,0,FW_NORMAL,FALSE,FALSE,FALSE,DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,CLIP_DEFAULT_PRECIS,ANTIALIASED_QUALITY,DEFAULT_PITCH,face);
            auto previous=font?SelectObject(dc,font):nullptr;
            DrawTextW(dc,value,-1,&rect,DT_CENTER|DT_SINGLELINE|DT_VCENTER|DT_NOPREFIX|DT_NOCLIP);
            if(font){SelectObject(dc,previous);DeleteObject(font);}
        };
        draw(hour,straight,config.style==0?100:70,0,config.style==0?108:100);
        if(config.date){
            if(config.style==0){draw(date,straight,35,150,193);draw(weekday,script,85,70,163);}
            else draw(date,straight,27,86,128);
        }
        GdiFlush();
        for(size_t i=0;i<size_t(width)*height;++i){
            const unsigned alpha=pixels[i]&255;
            const unsigned r=GetRValue(config.color)*alpha/255,g=GetGValue(config.color)*alpha/255,b=GetBValue(config.color)*alpha/255;
            pixels[i]=(alpha<<24)|(r<<16)|(g<<8)|b;
        }
        const auto& area=screen.area;
        POINT pos{area.left+(config.x==-1?(area.right-area.left-width)/2:config.x),
                  area.top+(config.y==-1?int((secondClock&&screen.primary?300:70)*scale):config.y)};
        pos.x=std::clamp(pos.x,area.left,area.right-width);
        pos.y=std::clamp(pos.y,area.top,area.bottom-height);
        SIZE size{width,height};POINT source{};
        BLENDFUNCTION blend{AC_SRC_OVER,0,BYTE(config.opacity*255/100),AC_SRC_ALPHA};
        UpdateLayeredWindow(hwnd,screenDC,&pos,&size,dc,&source,0,&blend,ULW_ALPHA);
        SelectObject(dc,old);DeleteObject(bitmap);DeleteDC(dc);ReleaseDC(nullptr,screenDC);
    }
};
} // namespace desktopclock
