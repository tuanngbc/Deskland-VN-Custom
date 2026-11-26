// Original FluidWin osd.h rendering, scoped to confirmed TLauncher volume UI
// commands. Does not read/write audio or intercept any keyboard input.
#define TLAUNCHER_SHARED_UI_TIMERS
#include "upstream/osd.h"
#include <memory>
namespace {
struct Host {osd::Indicatore osd;UINT_PTR ticket=0;RECT target{};~Host(){osd.chiudi();}};
thread_local std::unique_ptr<Host> active;
}
extern "C" HWND tlauncher_volume_osd_show(HWND controller,UINT notice,UINT_PTR ticket,const RECT* target,
    int percent,bool muted,double speed,bool animate,const wchar_t* mutedLabel,const wchar_t* accessibleName)noexcept{
    if(!controller||!target||IsRectEmpty(target)||!ticket||percent<0||percent>100)return nullptr;
    try{
        if(active&&(!EqualRect(&active->target,target)||!active->osd.noticeText.empty()))active.reset();
        if(!active){active=std::make_unique<Host>();if(!active->osd.inizializza()){active.reset();return nullptr;}
            HWND h=active->osd.window();SetWindowLongPtrW(h,GWL_EXSTYLE,GetWindowLongPtrW(h,GWL_EXSTYLE)|WS_EX_TRANSPARENT);
        }
        active->ticket=ticket;active->target=*target;auto& value=active->osd;value.speed=std::isfinite(speed)?std::clamp(speed,.25,2.):.70;value.animate=animate;
        value.mutedLabel=mutedLabel?std::wstring(mutedLabel,wcsnlen_s(mutedLabel,64)):L"Muted";
        value.onHidden=[controller,notice,ticket]{PostMessageW(controller,notice,ticket,0);};
        value.mostra(float(percent)/100,muted,target);
        HWND h=value.window();SetWindowTextW(h,accessibleName?accessibleName:L"Volume");
        NotifyWinEvent(EVENT_SYSTEM_ALERT,h,OBJID_WINDOW,0);return h;
    }catch(...){active.reset();return nullptr;}
}
extern "C" void tlauncher_volume_osd_close(UINT_PTR ticket)noexcept{
    if(active&&(!ticket||active->ticket==ticket))active.reset();
}
extern "C" HWND tlauncher_battery_osd_show(HWND controller,UINT notice,UINT_PTR ticket,const RECT* target,
    int percent,bool charging,double speed,bool animate,const wchar_t* text)noexcept{
    if(!controller||!target||IsRectEmpty(target)||!ticket||percent<0||percent>100||!text||!text[0])return nullptr;
    try{
        active.reset();active=std::make_unique<Host>();if(!active->osd.inizializza()){active.reset();return nullptr;}
        active->ticket=ticket;active->target=*target;auto& value=active->osd;
        value.speed=std::isfinite(speed)?std::clamp(speed,.25,2.):.70;value.animate=animate;value.noticeText=std::wstring(text,wcsnlen_s(text,160));
        value.noticePercent=percent;value.noticeCharging=charging;
        value.onHidden=[controller,notice,ticket]{PostMessageW(controller,notice,ticket,0);};
        HWND h=value.window();SetWindowLongPtrW(h,GWL_EXSTYLE,GetWindowLongPtrW(h,GWL_EXSTYLE)|WS_EX_TRANSPARENT);
        SetWindowTextW(h,value.noticeText.c_str());value.mostra(float(percent)/100,false,target);NotifyWinEvent(EVENT_SYSTEM_ALERT,h,OBJID_WINDOW,0);return h;
    }catch(...){active.reset();return nullptr;}
}
