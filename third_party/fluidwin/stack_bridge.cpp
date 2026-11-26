#include <windows.h>
#include <shellapi.h>
#define TLAUNCHER_SHARED_UI_TIMERS
#include "upstream/pila.h"
#include "upstream/jumplist.h"

namespace{
struct Apartment{bool active=false;~Apartment(){if(active)OleUninitialize();}};
struct Stack{Apartment apartment;pila::Pila panel;std::wstring folder;};
double stackSpeed=0.70;
double speed(){return stackSpeed;}
}
extern "C"{
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
