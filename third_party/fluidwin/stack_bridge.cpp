#include <windows.h>
#include <shellapi.h>
#define TLAUNCHER_SHARED_UI_TIMERS
#include "upstream/pila.h"
#include "upstream/jumplist.h"
#include "upstream/anteprime.h"
#include "upstream/comodita.h"
#include "upstream/panoramica.h"
#define TLAUNCHER_EMBEDDED_SNAP
#include "upstream/affianca.h"

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
struct Snap{
    affianca::Riquadro panel;RECT selected{};bool chosen=false;
    Snap(){panel.scelto=[this](HWND,const RECT& r,HMONITOR){selected=r;chosen=true;};}
};
panoramica::Panoramica* overview=nullptr;
double stackSpeed=0.70;
double speed(){return stackSpeed;}
}
extern "C"{
// fluidwin.cpp::suAffiancaTasto, geometry only. The caller owns all mutations.
bool tlauncher_snap_arrow(const RECT* visible,bool maximized,bool right,RECT* selected)noexcept{
    if(!visible||!selected)return false;using namespace affianca;
    auto monitor=MonitorFromRect(visible,MONITOR_DEFAULTTONEAREST);if(!monitor)return false;
    const Zona current=maximized?PIENO:zonaDi(*visible,monitor);Zona next=right?DESTRA:SINISTRA;
    switch(current){
        case SINISTRA:case DESTRA:
            if((current==SINISTRA)==right)next=CENTRO;
            else{MONITORINFO info{};info.cbSize=sizeof(info);if(!GetMonitorInfoW(monitor,&info))return false;
                const POINT beyond{right?info.rcMonitor.right+1:info.rcMonitor.left-2,(info.rcMonitor.top+info.rcMonitor.bottom)/2};
                monitor=MonitorFromPoint(beyond,MONITOR_DEFAULTTONULL);if(!monitor)return false;next=right?SINISTRA:DESTRA;}
            break;
        case ALTO_SX:case ALTO_DX:next=right?ALTO_DX:ALTO_SX;break;
        case BASSO_SX:case BASSO_DX:next=right?BASSO_DX:BASSO_SX;break;
        default:break;
    }
    if(next==current)return false;*selected=rettZona(monitor,next);return selected->right>selected->left&&selected->bottom>selected->top;
}
void* tlauncher_snap_open(HWND source,POINT anchor,const wchar_t* title,const wchar_t* action,const wchar_t* const* labels,unsigned count,double speed,bool reduced)noexcept{
    try{
        if(!IsWindow(source)||!title||!labels||count!=11)return nullptr;
        auto snap=std::make_unique<Snap>();snap->panel.motionSpeed=speed;snap->panel.reducedMotion=reduced;snap->panel.panelTitle=title;snap->panel.actionLabel=action?action:L"Choose";
        for(unsigned i=0;i<count;i++)snap->panel.labels.emplace_back(labels[i]?labels[i]:L"");
        RECT button{anchor.x-1,anchor.y-1,anchor.x+1,anchor.y+1};snap->panel.mostra(source,anchor,button,true);
        return snap->panel.aperta()?snap.release():nullptr;
    }catch(...){return nullptr;}
}
int tlauncher_snap_poll(void* raw,RECT* selected)noexcept{
    if(!raw||!selected)return -1;auto& snap=*static_cast<Snap*>(raw);snap.panel.poll();
    if(snap.panel.aperta())return 0;if(snap.chosen&&!snap.panel.abandoned){*selected=snap.selected;return 1;}return 2;
}
HWND tlauncher_snap_window(void* raw)noexcept{return raw?static_cast<Snap*>(raw)->panel.window():nullptr;}
void tlauncher_snap_key(void* raw,unsigned key)noexcept{if(raw)static_cast<Snap*>(raw)->panel.key(key);}
void tlauncher_snap_free(void* raw)noexcept{delete static_cast<Snap*>(raw);}
void* tlauncher_snap_accessible(void* raw)noexcept{
    if(!raw)return nullptr;IAccessible* accessible=nullptr;
    if(FAILED(AccessibleObjectFromWindow(static_cast<Snap*>(raw)->panel.window(),OBJID_CLIENT,IID_IAccessible,reinterpret_cast<void**>(&accessible))))return nullptr;return accessible;
}
int tlauncher_snap_accessible_probe(void* raw,int mode)noexcept{
    auto accessible=static_cast<IAccessible*>(raw);if(!accessible)return 1;
    VARIANT child;VariantInit(&child);child.vt=VT_I4;child.lVal=2;
    if(mode==3){long count=-1;if(FAILED(accessible->get_accChildCount(&count))||count!=0)return 2;
        return accessible->accDoDefaultAction(child)==CO_E_OBJNOTCONNECTED?0:3;}
    if(mode==1){child.lVal=11;return SUCCEEDED(accessible->accSelect(SELFLAG_TAKEFOCUS|SELFLAG_TAKESELECTION,child))?0:4;}
    if(mode==2)return SUCCEEDED(accessible->accDoDefaultAction(child))?0:5;
    long count=0;if(FAILED(accessible->get_accChildCount(&count))||count!=11)return 6;
    for(long i=1;i<=11;i++){
        child.lVal=i;BSTR name=nullptr;VARIANT role;VariantInit(&role);long x=0,y=0,w=0,h=0;
        if(FAILED(accessible->get_accName(child,&name))||!name||!SysStringLen(name)){SysFreeString(name);return 7;}SysFreeString(name);
        if(FAILED(accessible->get_accRole(child,&role))||role.vt!=VT_I4||role.lVal!=ROLE_SYSTEM_PUSHBUTTON)return 8;
        if(FAILED(accessible->accLocation(&x,&y,&w,&h,child))||w<=0||h<=0)return 9;
        VARIANT hit;VariantInit(&hit);if(FAILED(accessible->accHitTest(x+w/2,y+h/2,&hit))||hit.vt!=VT_I4||hit.lVal!=i)return 10;
        BSTR action=nullptr;if(FAILED(accessible->get_accDefaultAction(child,&action))||!action||!SysStringLen(action)){SysFreeString(action);return 11;}SysFreeString(action);
    }
    VARIANT root;VariantInit(&root);root.vt=VT_I4;root.lVal=0;VARIANT next;VariantInit(&next);
    if(FAILED(accessible->accNavigate(NAVDIR_LASTCHILD,root,&next))||next.vt!=VT_I4||next.lVal!=11)return 12;
    ITypeInfo* info=nullptr;if(FAILED(accessible->GetTypeInfo(0,LOCALE_SYSTEM_DEFAULT,&info))||!info)return 13;info->Release();return 0;
}
void tlauncher_snap_accessible_free(void* raw)noexcept{if(raw)static_cast<IAccessible*>(raw)->Release();}
HWND tlauncher_overview_show(bool(*valid)(HWND),const RECT* area,double speed,bool reduced,bool alt,int steps) noexcept{
    try{
        if(overview||!valid||!area)return nullptr;
        panoramica::Panoramica panel;panel.idonea=valid;panel.areaOverride=*area;
        panel.motionSpeed=speed;panel.reducedMotion=reduced;panel.externalActivation=true;
        struct Active{Active(panoramica::Panoramica* p){overview=p;}~Active(){overview=nullptr;}}active(&panel);
        panel.apri(alt,steps);return panel.selected();
    }catch(...){return nullptr;}
}
void tlauncher_overview_key(int key) noexcept{
    if(!overview)return;
    if(key==0)overview->scegliSelezionata();
    else if(key==99)overview->cancel();
    else{LRESULT result=0;overview->messaggio(overview->window(),WM_KEYDOWN,VK_TAB,key<0?1:0,result);}
}
HWND tlauncher_overview_window() noexcept{return overview?overview->window():nullptr;}
unsigned tlauncher_overview_thumbnails() noexcept{return overview?overview->thumbnails():0;}
bool tlauncher_comfort_command(int operation,uint64_t duration) noexcept{
    if(operation==1)return comodita::tieniSveglio(duration);
    if(operation==2){comodita::avviaTimer(duration);return true;}
    if(operation==3){comodita::fermaTimer();return true;}
    if(operation==4){comodita::aggiungiTimer(duration);return true;}
    return false;
}
unsigned tlauncher_comfort_state(uint64_t* timer,uint64_t* awake) noexcept{
    comodita::controlla();auto& s=comodita::stato();const auto now=GetTickCount64();
    *timer=comodita::timerResto();*awake=s.svegliaFino==comodita::SEMPRE?UINT64_MAX:s.svegliaFino>now?s.svegliaFino-now:0;
    return (s.sveglioApplicato?1:0)|(comodita::timerAttivo()?2:0)|(comodita::timerSuona()?4:0);
}
int tlauncher_comfort_fixture() noexcept{
    int failure=0;auto& s=comodita::stato();
    if(!comodita::tieniSveglio(1000)||!s.sveglioApplicato)failure|=1;
    s.svegliaFino=GetTickCount64();comodita::controlla();if(s.sveglioApplicato)failure|=2;
    comodita::avviaTimer(10000);const auto end=s.timerFino;comodita::aggiungiTimer(5000);
    if(s.timerFino!=end+5000||s.timerDurata!=15000)failure|=4;
    s.senzaSuono=true;s.timerFino=GetTickCount64();comodita::controlla();
    if(!comodita::timerSuona()||comodita::timerAttivo())failure|=8;
    s.suonaFino=GetTickCount64();comodita::controlla();if(comodita::timerSuona())failure|=16;
    comodita::fermaTimer();comodita::tieniSveglio(0);return failure;
}
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
