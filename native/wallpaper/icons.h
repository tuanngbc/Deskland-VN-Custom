// Read-only Shell COM geometry. One coalesced job on the accepted Windows pool;
// no remote allocation, process-memory writes, or blocking render/UI queries.
#pragma once
#include "images.h"
#include "model.h"
#include "../fluidwin/pool.h"
#include <shlobj.h>
#include <shobjidl.h>
#include <exdisp.h>
#include <shlguid.h>
#include <mutex>
#include <atomic>

namespace wallpaper {
inline HWND desktopList(){
    HWND list=nullptr;
    EnumWindows([](HWND h,LPARAM p)->BOOL {
        HWND view=FindWindowExW(h,nullptr,L"SHELLDLL_DefView",nullptr);
        if(!view)return TRUE;
        *reinterpret_cast<HWND*>(p)=FindWindowExW(view,nullptr,L"SysListView32",nullptr);
        return FALSE;
    },reinterpret_cast<LPARAM>(&list));
    return list;
}
class IconGeometry {
    struct State {
        std::mutex mutex;std::vector<RECT> snapshot;
        std::atomic<bool> stop{false};bool active=false;DWORD worker=0;
        uint64_t started=0;bool cancelled=false;
    };
    std::shared_ptr<State> state=std::make_shared<State>();
    std::vector<RECT> snapshot;uint64_t next=0;

    static bool query(const std::shared_ptr<State>&s,std::vector<RECT>&result){
        Com<IShellWindows> windows;Com<IDispatch> dispatch;Com<IServiceProvider> services;
        Com<IShellBrowser> browser;Com<IShellView> view;Com<IFolderView> folder;
        if(s->stop||FAILED(CoCreateInstance(CLSID_ShellWindows,nullptr,CLSCTX_LOCAL_SERVER,IID_PPV_ARGS(windows.put()))))return false;
        VARIANT desktop{},reserved{};desktop.vt=VT_I4;desktop.lVal=CSIDL_DESKTOP;long hwnd=0;
        if(s->stop||FAILED(windows->FindWindowSW(&desktop,&reserved,SWC_DESKTOP,&hwnd,SWFO_NEEDDISPATCH,dispatch.put()))||!dispatch.get())return false;
        if(s->stop||FAILED(dispatch->QueryInterface(IID_PPV_ARGS(services.put())))||FAILED(services->QueryService(SID_STopLevelBrowser,IID_PPV_ARGS(browser.put())))||FAILED(browser->QueryActiveShellView(view.put()))||FAILED(view->QueryInterface(IID_PPV_ARGS(folder.put()))))return false;
        HWND list=desktopList();POINT spacing{};int count=0;
        if(!list||s->stop||FAILED(folder->GetSpacing(&spacing))||FAILED(folder->ItemCount(SVGIO_ALLVIEW,&count))||spacing.x<=0||spacing.y<=0)return false;
        spacing.x=std::clamp<LONG>(spacing.x,16,1024);spacing.y=std::clamp<LONG>(spacing.y,16,1024);
        for(int i=0;i<std::min(count,512);i++){
            if(s->stop||GetTickCount64()-s->started>150)return false;
            PITEMID_CHILD pidl=nullptr;POINT position{};
            HRESULT item=folder->Item(i,&pidl);
            if(FAILED(item)||!pidl)continue;
            HRESULT located=folder->GetItemPosition(pidl,&position);CoTaskMemFree(pidl);
            if(FAILED(located))continue;
            RECT r{position.x,position.y,position.x+spacing.x,position.y+spacing.y};
            MapWindowPoints(list,nullptr,reinterpret_cast<POINT*>(&r),2);result.push_back(r);
        }
        return !s->stop;
    }
public:
    ~IconGeometry(){auto s=state;s->stop=true;std::lock_guard<std::mutex> lock(s->mutex);if(s->worker)CoCancelCall(s->worker,0);}
    const std::vector<RECT>&poll(const std::atomic<bool>&stop){
        auto s=state;std::lock_guard<std::mutex> lock(s->mutex);snapshot=s->snapshot;
        if(stop){s->stop=true;return snapshot;}
        const auto now=GetTickCount64();
        if(s->active){if(s->worker&&!s->cancelled&&now-s->started>150){s->cancelled=true;CoCancelCall(s->worker,0);}return snapshot;}
        if(now<next)return snapshot;
        next=now+1000;s->active=true;s->started=now;s->cancelled=false;
        auto*job=new std::shared_ptr<State>(s);
        if(!pool::lancia([](void*raw)->DWORD {
            auto owner=std::unique_ptr<std::shared_ptr<State>>(static_cast<std::shared_ptr<State>*>(raw));auto s=*owner;
            HRESULT com=CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED);
            if(SUCCEEDED(com)){
                HRESULT cancellation=CoEnableCallCancellation(nullptr);
                {std::lock_guard<std::mutex> lock(s->mutex);s->worker=GetCurrentThreadId();}
                std::vector<RECT> fresh;bool ok=false;
                try{ok=query(s,fresh);}catch(...){}
                {std::lock_guard<std::mutex> lock(s->mutex);if(ok&&!s->cancelled&&!s->stop)s->snapshot=std::move(fresh);s->worker=0;}
                if(SUCCEEDED(cancellation))CoDisableCallCancellation(nullptr);
                CoUninitialize();
            }
            {std::lock_guard<std::mutex> lock(s->mutex);s->active=false;}
            return 0;
        },job,THREAD_PRIORITY_BELOW_NORMAL,true)){delete job;s->active=false;}
        return snapshot;
    }
};
}
