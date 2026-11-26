// TLauncher adapter. Imported implementation and provenance are in upstream/.
#include <windows.h>
#include <shellapi.h>
#include <cstdio>
#include <cstdint>
#include <wrl/client.h>
#include "upstream/destro.h"
#include "upstream/jumplist.h"

namespace {
struct JumpModel{jumplist::Elenco list;};
std::wstring jump_identity(HWND hwnd){
    IPropertyStore* store=nullptr;std::wstring id;
    if(SUCCEEDED(SHGetPropertyStoreForWindow(hwnd,IID_PPV_ARGS(&store)))&&store){id=jumplist::dettaglio::testo(store,jumplist::dettaglio::CHIAVE_AUMID);store->Release();}return id;
}
void add_recent(JumpModel& model,const std::wstring& aumid){
    if(aumid.empty())return;
    IApplicationDocumentLists* docs=nullptr;
    if(FAILED(CoCreateInstance(CLSID_ApplicationDocumentLists,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&docs)))||!docs)return;
    IObjectArray* items=nullptr;
    if(SUCCEEDED(docs->SetAppID(aumid.c_str()))&&SUCCEEDED(docs->GetList(ADLT_RECENT,12,IID_PPV_ARGS(&items)))&&items){
        jumplist::Categoria category;category.nome=L"Recent";UINT count=0;items->GetCount(&count);
        for(UINT i=0;i<std::min(count,12u);++i){
            IShellItem* item=nullptr;if(FAILED(items->GetAt(i,IID_PPV_ARGS(&item)))||!item)continue;
            PWSTR name=nullptr;PIDLIST_ABSOLUTE pidl=nullptr;
            if(SUCCEEDED(item->GetDisplayName(SIGDN_NORMALDISPLAY,&name))&&name&&SUCCEEDED(SHGetIDListFromObject(item,&pidl))&&pidl){
                const auto size=ILGetSize(pidl);
                if(size&&size<65536){jumplist::Comando command;command.titolo=name;command.pidl.assign(reinterpret_cast<BYTE*>(pidl),reinterpret_cast<BYTE*>(pidl)+size);category.voci.push_back(std::move(command));}
            }
            CoTaskMemFree(name);CoTaskMemFree(pidl);item->Release();
        }
        items->Release();if(!category.voci.empty())model.list.categorie.push_back(std::move(category));
    }
    docs->Release();
}
thread_local bool fixtureTimeout=false;
LRESULT CALLBACK shell_host(HWND hwnd,UINT message,WPARAM w,LPARAM l){
    LRESULT result=0;
    if(menu::shellMessage&&menu::shellMessage(message,w,l,result))return result;
    if(message==WM_TIMER&&w==1){EndMenu();return 0;}
    return DefWindowProcW(hwnd,message,w,l);
}
UINT native_menu(HMENU items,POINT point,HWND owner){
    WNDCLASSW wc{};wc.lpfnWndProc=shell_host;wc.hInstance=GetModuleHandleW(nullptr);wc.lpszClassName=L"TLauncher.ShellMenuHost";
    RegisterClassW(&wc);
    HWND host=CreateWindowExW(WS_EX_TOOLWINDOW,wc.lpszClassName,L"",WS_POPUP,point.x,point.y,1,1,owner,nullptr,wc.hInstance,nullptr);
    if(!host)return 0;
    if(fixtureTimeout)SetTimer(host,1,200,nullptr);
    SetForegroundWindow(owner);
    const auto command=TrackPopupMenuEx(items,TPM_RETURNCMD|TPM_RIGHTBUTTON,point.x,point.y,host,nullptr);
    KillTimer(host,1);DestroyWindow(host);return command;
}
}

extern "C" {
void tlauncher_fluidwin_menu_policy(int language,double speed,bool reduced) noexcept {
    menu::motionSpeed=std::isfinite(speed)?std::clamp(speed,0.25,2.0):0.70;
    menu::reducedMotion=reduced;
    menu::uiLocale=language==0?L"vi-VN":language==2?L"zh-CN":L"en-US";
    destro::adminTitle=language==0?L"Chạy với quyền quản trị":language==2?L"以管理员身份运行":L"Run as administrator";
}
bool tlauncher_fluidwin_menu_policy_fixture(int language,double speed,bool reduced) noexcept {
    tlauncher_fluidwin_menu_policy(language,speed,reduced);
    const auto locale=language==0?L"vi-VN":language==2?L"zh-CN":L"en-US";
    return menu::uiLocale==locale && menu::reducedMotion==reduced &&
        std::abs(menu::duration(150.0)*menu::motionSpeed-150.0)<0.001 && !destro::adminTitle.empty();
}
void* tlauncher_jump_prepare(const wchar_t* path,HWND hwnd,DWORD expectedPid,DWORD expectedTid,const wchar_t* otherIds) noexcept {
    try{
        auto model=std::make_unique<JumpModel>();std::vector<std::wstring> ids,others;
        std::wstring exe=path?path:L"";
        constexpr wchar_t prefix[]=L"shell:AppsFolder\\";
        if(exe.rfind(prefix,0)==0){ids.push_back(exe.substr(wcslen(prefix)));exe.clear();}
        if(hwnd){DWORD pid=0;const auto tid=GetWindowThreadProcessId(hwnd,&pid);if(pid==expectedPid&&tid==expectedTid){auto id=jump_identity(hwnd);if(!id.empty())ids.push_back(std::move(id));}}
        if(exe.size()>=4&&_wcsicmp(exe.c_str()+exe.size()-4,L".lnk")==0&&jumplist::locale(exe)){
            IShellLinkW* link=nullptr;
            if(SUCCEEDED(CoCreateInstance(CLSID_ShellLink,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&link)))&&link){
                IPersistFile* file=nullptr;
                if(SUCCEEDED(link->QueryInterface(IID_PPV_ARGS(&file)))&&file){
                    if(SUCCEEDED(file->Load(exe.c_str(),STGM_READ))){
                        IPropertyStore* props=nullptr;
                        if(SUCCEEDED(link->QueryInterface(IID_PPV_ARGS(&props)))&&props){auto id=jumplist::dettaglio::testo(props,jumplist::dettaglio::CHIAVE_AUMID);if(!id.empty())ids.push_back(std::move(id));props->Release();}
                        wchar_t target[32768]{};if(SUCCEEDED(link->GetPath(target,32768,nullptr,SLGP_RAWPATH)))exe=target;
                    }file->Release();
                }link->Release();
            }
        }
        if(otherIds)for(const wchar_t* at=otherIds;*at;at+=wcslen(at)+1)others.emplace_back(at);
        jumplist::Indice index;index.leggiUnaVolta();model->list=index.per(ids,jumplist::minuscolo(exe),others);
        if(!ids.empty())add_recent(*model,ids.front());
        return model.release();
    }catch(...){return nullptr;}
}
void tlauncher_jump_free(void* raw) noexcept{delete static_cast<JumpModel*>(raw);}
unsigned tlauncher_jump_show(void* raw,HWND owner,int x,int y,const wchar_t* tasks,const wchar_t* recent,const wchar_t* empty) noexcept{
    try{
        auto& model=*static_cast<JumpModel*>(raw);std::vector<menu::Voce> rows;unsigned id=0;
        for(const auto& category:model.list.categorie){
            auto heading=menu::voce(category.compiti?tasks:(category.nome==L"Recent"?recent:category.nome.c_str()),0);heading.titolo=true;heading.disattivata=true;rows.push_back(std::move(heading));
            for(const auto& entry:category.voci){
                ++id;auto row=menu::voce(entry.titolo,id);row.separatore=entry.separatore;
                if(!entry.pixel.empty()){row.pixel=entry.pixel.data();row.latoPixel=jumplist::LATO_ICONA;}
                rows.push_back(std::move(row));
            }
        }
        if(rows.empty()){auto row=menu::voce(empty,0);row.disattivata=true;rows.push_back(std::move(row));}
        return menu::mostra(rows,POINT{x,y},owner,true);
    }catch(...){return 0;}
}
bool tlauncher_jump_launch(void* raw,unsigned selected) noexcept{
    try{unsigned id=0;for(const auto& category:static_cast<JumpModel*>(raw)->list.categorie)for(const auto& command:category.voci){if(++id==selected)return !command.separatore&&jumplist::avvia(command);}return false;}catch(...){return false;}
}
int tlauncher_jump_parser_fixture(const wchar_t* path) noexcept{
    using Microsoft::WRL::ComPtr;
    try{
        ComPtr<IShellLinkW> link;ComPtr<IPersistStream> persist;ComPtr<IPropertyStore> props;ComPtr<IStream> stream;
        if(FAILED(CoCreateInstance(CLSID_ShellLink,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&link)))||FAILED(link.As(&persist))||FAILED(link.As(&props))||FAILED(CreateStreamOnHGlobal(nullptr,TRUE,&stream)))return 1;
        const std::wstring arguments=std::wstring(5000,L'x')+L" --fixture \"a b\"";
        link->SetPath(L"C:\\Synthetic\\not-executed.exe");link->SetArguments(arguments.c_str());link->SetWorkingDirectory(L"C:\\Synthetic");
        PROPVARIANT title;PropVariantInit(&title);InitPropVariantFromString(L"Fixture task",&title);props->SetValue(PKEY_Title,title);PropVariantClear(&title);props->Commit();
        if(FAILED(persist->Save(stream.Get(),TRUE)))return 2;
        STATSTG stat{};stream->Stat(&stat,STATFLAG_NONAME);if(stat.cbSize.QuadPart>65536)return 3;
        LARGE_INTEGER zero{};stream->Seek(zero,STREAM_SEEK_SET,nullptr);
        std::vector<BYTE> data;const auto append=[&](const void* value,size_t size){auto p=static_cast<const BYTE*>(value);data.insert(data.end(),p,p+size);};
        const DWORD header[]={2,1,0,2,1};append(header,sizeof(header));append(&CLSID_ShellLink,sizeof(CLSID_ShellLink));
        const auto at=data.size();data.resize(at+size_t(stat.cbSize.QuadPart));ULONG read=0;
        if(FAILED(stream->Read(data.data()+at,ULONG(stat.cbSize.QuadPart),&read))||read!=stat.cbSize.QuadPart)return 4;
        const DWORD end=jumplist::dettaglio::FINE_CATEGORIA;append(&end,sizeof(end));
        const auto write=[&](const BYTE* bytes,DWORD size){HANDLE h=CreateFileW(path,GENERIC_WRITE,0,nullptr,CREATE_ALWAYS,FILE_ATTRIBUTE_NORMAL,nullptr);if(h==INVALID_HANDLE_VALUE)return false;DWORD done=0;const bool ok=WriteFile(h,bytes,size,&done,nullptr)&&done==size;CloseHandle(h);return ok;};
        if(!write(data.data(),DWORD(data.size())))return 5;
        auto list=jumplist::leggiFile(path);
        if(list.categorie.size()!=1||!list.categorie[0].compiti||list.categorie[0].voci.size()!=1)return 6;
        const auto& task=list.categorie[0].voci[0];if(task.titolo!=L"Fixture task"||task.argomenti!=arguments||task.cartella!=L"C:\\Synthetic")return 7;
        // Truncation must not publish a partially read command/category.
        if(!write(data.data(),12)||!jumplist::leggiFile(path).vuoto())return 8;
        if(!write(data.data(),DWORD(data.size())))return 9;
        return 0;
    }catch(...){return 10;}
}
unsigned tlauncher_jump_menu_fixture(const wchar_t* path,HWND owner) noexcept{
    try{JumpModel model;model.list=jumplist::leggiFile(path);return tlauncher_jump_show(&model,owner,150,400,L"Tasks",L"Recent",L"Empty");}catch(...){return 0;}
}
int tlauncher_fluidwin_shell_hit(int x,int y) {
    return destro::vistaShell(POINT{x,y}) ? 1 : 0;
}
int tlauncher_fluidwin_shell_menu(HWND owner,int x,int y) {
    // Shell COM, Direct2D and the share helper all belong to the Dock STA.
    // Never call this from the low-level hook's thread.
    try {
        testi::lingua().store(testi::INGLESE);
        menu::nativeShellMenu=native_menu;
        return destro::mostra(POINT{x,y},owner,nullptr) ? 1 : 0;
    } catch (...) { return 0; }
}
void tlauncher_fluidwin_shell_menu_close() { menu::chiudi(); }
void tlauncher_fluidwin_shell_menu_shutdown() { menu::chiudi(); condividi::ferma(0); }

// Native fixture for the actual imported menu, not a JSON/settings-only test.
unsigned tlauncher_fluidwin_shell_menu_fixture(HWND owner,int language) {
    try {
        testi::lingua().store(testi::INGLESE);
        HMENU child=CreatePopupMenu();
        AppendMenuW(child,MF_STRING,73,destro::adminTitle.c_str());
        auto disabled=menu::voce(language==0?L"Không khả dụng":language==2?L"不可用":L"Disabled",11); disabled.disattivata=true;
        auto nested=menu::voce(language==0?L"Lệnh của ứng dụng":language==2?L"应用命令":L"Application commands",0); nested.sotto=child;
        const unsigned result=menu::mostra({disabled,nested},POINT{120,120},owner);
        DestroyMenu(child);
        return result;
    } catch (...) { return 0; }
}
unsigned tlauncher_fluidwin_owner_draw_fixture(HWND owner){
    unsigned measured=0,drawn=0;
    menu::ShellMessageScope scope([&](UINT message,WPARAM,LPARAM l,LRESULT& result){
        if(message==WM_MEASUREITEM){auto item=reinterpret_cast<MEASUREITEMSTRUCT*>(l);item->itemWidth=160;item->itemHeight=32;measured++;result=TRUE;return true;}
        if(message==WM_DRAWITEM){auto item=reinterpret_cast<DRAWITEMSTRUCT*>(l);FillRect(item->hDC,&item->rcItem,reinterpret_cast<HBRUSH>(GetStockObject(WHITE_BRUSH)));drawn++;result=TRUE;return true;}
        return false;
    });
    HMENU popup=CreatePopupMenu();AppendMenuW(popup,MF_OWNERDRAW,91,L"Synthetic Shell extension");
    if(!menu::requiresNative(popup)){DestroyMenu(popup);return 0;}
    fixtureTimeout=true;native_menu(popup,POINT{150,150},owner);fixtureTimeout=false;
    DestroyMenu(popup);
    return (measured?1u:0u)|(drawn?2u:0u);
}
}
