#include <windows.h>
#include <shellapi.h>
#define TLAUNCHER_SHARED_UI_TIMERS
#include "upstream/pila.h"
#include "upstream/jumplist.h"
#include "upstream/anteprime.h"

namespace{
struct Apartment{bool active=false;~Apartment(){if(active)OleUninitialize();}};
struct Stack{Apartment apartment;pila::Pila panel;std::wstring folder;};
struct Preview{
    anteprime::Anteprime panel;HWND target=nullptr;int action=0;
    Preview(){
        panel.scegli=[this](HWND h){target=h;action=1;};
        panel.chiudiFinestra=[this](HWND h){target=h;action=2;};
    }
};
double stackSpeed=0.70;
double speed(){return stackSpeed;}
}
extern "C"{
void* tlauncher_previews_open(const HWND* sources,unsigned count,const RECT* icon,const RECT* area,int side,HWND owner,
    double speed,bool reduced,const wchar_t* minimized,const wchar_t* unavailable) noexcept{
    try{
        if(!sources||!count||count>8||!icon||!area||!IsWindow(owner))return nullptr;
        auto preview=std::make_unique<Preview>();
        preview->panel.motionSpeed=speed;preview->panel.reducedMotion=reduced;
        preview->panel.minimizedLabel=minimized;preview->panel.unavailableLabel=unavailable;
        const LONG inner=side==2?icon->right:side==3?icon->left:icon->top;
        preview->panel.apri(std::vector<HWND>(sources,sources+count),*icon,inner,*area,0,owner,side);
        return preview->panel.visible()?preview.release():nullptr;
    }catch(...){return nullptr;}
}
int tlauncher_previews_poll(void* raw,HWND* target) noexcept{
    if(!raw||!target)return 0;auto& p=*static_cast<Preview*>(raw);
    p.panel.riconcilia();*target=p.target;const int action=p.action;p.target=nullptr;p.action=0;return action;
}
bool tlauncher_previews_hover(void* raw) noexcept{return raw&&static_cast<Preview*>(raw)->panel.mouseSopra();}
bool tlauncher_previews_visible(void* raw) noexcept{return raw&&static_cast<Preview*>(raw)->panel.visible();}
void tlauncher_previews_close(void* raw) noexcept{if(raw)static_cast<Preview*>(raw)->panel.chiudi();}
void tlauncher_previews_free(void* raw) noexcept{delete static_cast<Preview*>(raw);}
HWND tlauncher_previews_card(void* raw,unsigned index) noexcept{return raw?static_cast<Preview*>(raw)->panel.card(index):nullptr;}
unsigned tlauncher_previews_thumbnails(void* raw) noexcept{return raw?static_cast<Preview*>(raw)->panel.thumbnails():0;}
void* tlauncher_stack_prepare(const wchar_t* fixtureFolder) noexcept{
    try{
        auto stack=std::make_unique<Stack>();
        if(fixtureFolder)stack->folder=fixtureFolder;
        else{PWSTR folder=nullptr;if(FAILED(SHGetKnownFolderPath(FOLDERID_Downloads,0,nullptr,&folder))||!folder)return nullptr;stack->folder=folder;CoTaskMemFree(folder);}
        // Folder redirection must not block the UI or retain a live Shell
        // thumbnail service. Resolve on this worker and refuse remote paths.
        if(!jumplist::locale(stack->folder))return nullptr;
        HANDLE dir=CreateFileW(stack->folder.c_str(),FILE_READ_ATTRIBUTES,FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE,nullptr,OPEN_EXISTING,FILE_FLAG_BACKUP_SEMANTICS,nullptr);
        if(dir==INVALID_HANDLE_VALUE)return nullptr;
        wchar_t resolved[32768]{};const auto size=GetFinalPathNameByHandleW(dir,resolved,32768,FILE_NAME_NORMALIZED);CloseHandle(dir);
        if(!size||size>=32768||!jumplist::locale(resolved))return nullptr;
        stack->panel.prepara(stack->folder);return stack.release();
    }catch(...){return nullptr;}
}
bool tlauncher_stack_open(void* raw,const RECT* icon,int side,const wchar_t* title,const wchar_t* empty,const wchar_t* explorer,double factor,bool reduced) noexcept{
    try{
        auto& s=*static_cast<Stack*>(raw);stackSpeed=reduced?10000.0:factor;vetro::motionSpeed=speed;
        if(!s.apartment.active){if(FAILED(OleInitialize(nullptr)))return false;s.apartment.active=true;}
        s.panel.emptyLabel=empty;s.panel.openLabel=explorer;s.panel.apri(*icon,s.folder,title,side);
        if(s.panel.finestra())SetWindowTextW(s.panel.finestra(),title);
        return s.panel.finestra()!=nullptr;
    }catch(...){return false;}
}
HWND tlauncher_stack_window(void* raw) noexcept{return raw?static_cast<Stack*>(raw)->panel.finestra():nullptr;}
unsigned tlauncher_stack_count(void* raw) noexcept{return raw?unsigned(static_cast<Stack*>(raw)->panel.quantiFile()):0;}
bool tlauncher_stack_thumbnails_idle() noexcept{return !pila::thumbnailActive.load();}
bool tlauncher_stack_active(void* raw) noexcept{return raw&&static_cast<Stack*>(raw)->panel.aperta();}
void tlauncher_stack_free(void* raw) noexcept{delete static_cast<Stack*>(raw);}
}
