// FluidWin launcher.h::menuApp Shell binding/canonical-verb discovery adapted
// to both existing Spotlight modes. Windows draws this fallback menu because
// the imported single-threaded D2D menu factory belongs to the Dock STA.
#include <windows.h>
#include <shellapi.h>
#include <shlobj.h>
#include <shobjidl.h>
#include <commctrl.h>
#include <wrl/client.h>
#include <string>
#include <vector>
#pragma comment(lib,"comctl32.lib")
namespace {
using Microsoft::WRL::ComPtr;
thread_local bool busy=false;
struct MenuScope{
    HWND owner=nullptr;HMENU menu=nullptr;ComPtr<IContextMenu2> second;ComPtr<IContextMenu3> third;bool subclassed=false;
    ~MenuScope(){if(subclassed)RemoveWindowSubclass(owner,route,reinterpret_cast<UINT_PTR>(this));if(menu)DestroyMenu(menu);}
    static LRESULT CALLBACK route(HWND h,UINT m,WPARAM w,LPARAM l,UINT_PTR,DWORD_PTR raw){
        auto self=reinterpret_cast<MenuScope*>(raw);
        if(m==WM_INITMENUPOPUP||m==WM_DRAWITEM||m==WM_MEASUREITEM||m==WM_MENUCHAR||m==WM_MENUSELECT||m==WM_UNINITMENUPOPUP){
            LRESULT result=0;if(self->third&&self->third->HandleMenuMsg2(m,w,l,&result)==S_OK)return result;
            if(self->second&&self->second->HandleMenuMsg(m,w,l)==S_OK)return 0;
        }return DefSubclassProc(h,m,w,l);
    }
};
UINT uninstall(IContextMenu* context,HMENU menu){
    const int count=GetMenuItemCount(menu);for(int i=0;i<count;i++){
        const auto id=GetMenuItemID(menu,i);if(!id||id==UINT(-1)||id>=0x7F00)continue;
        wchar_t verb[128]{};if(SUCCEEDED(context->GetCommandString(id-1,GCS_VERBW,nullptr,reinterpret_cast<char*>(verb),128))&&!_wcsicmp(verb,L"Uninstall"))return id;
    }return 0;
}
}
extern "C" int tlauncher_spotlight_app_menu(HWND owner,POINT point,const wchar_t* path,const wchar_t* uninstallLabel,bool invoke)noexcept{
    try{
        if(busy||!IsWindow(owner)||!path||!*path||!uninstallLabel)return -1;
        busy=true;struct Reset{~Reset(){busy=false;}}reset;
        const auto initialized=CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED);if(FAILED(initialized))return -2;
        struct Apartment{~Apartment(){CoUninitialize();}}apartment;
        ComPtr<IShellItem> item;if(FAILED(SHCreateItemFromParsingName(path,nullptr,IID_PPV_ARGS(&item))))return -3;
        ComPtr<IContextMenu> context;if(FAILED(item->BindToHandler(nullptr,BHID_SFUIObject,IID_PPV_ARGS(&context))))return -4;
        MenuScope scope;scope.owner=owner;scope.menu=CreatePopupMenu();if(!scope.menu)return -5;
        if(FAILED(context->QueryContextMenu(scope.menu,0,1,0x7EFF,CMF_NORMAL)))return -6;
        if(const auto id=uninstall(context.Get(),scope.menu)){
            // The canonical Shell command is preserved, including its own
            // app identity/UAC/confirmation. Never parse uninstall strings.
            const auto state=GetMenuState(scope.menu,id,MF_BYCOMMAND);
            if(state!=UINT(-1)&&!(state&(MF_DISABLED|MF_GRAYED))){RemoveMenu(scope.menu,id,MF_BYCOMMAND);InsertMenuW(scope.menu,0,MF_BYPOSITION|MF_STRING,id,uninstallLabel);}
        }
        context.As(&scope.second);context.As(&scope.third);
        scope.subclassed=SetWindowSubclass(owner,MenuScope::route,reinterpret_cast<UINT_PTR>(&scope),reinterpret_cast<DWORD_PTR>(&scope))!=FALSE;
        if(!scope.subclassed)return -7;
        if(!invoke)return GetMenuItemCount(scope.menu)>=0?1:-8; // hosted read-only enumeration
        SetForegroundWindow(owner);
        const auto selected=TrackPopupMenuEx(scope.menu,TPM_RETURNCMD|TPM_RIGHTBUTTON,point.x,point.y,owner,nullptr);
        if(!selected)return 0;if(selected>=0x7F00)return -9;
        CMINVOKECOMMANDINFOEX command{};command.cbSize=sizeof(command);command.fMask=CMIC_MASK_UNICODE|CMIC_MASK_PTINVOKE;
        command.hwnd=owner;command.lpVerb=MAKEINTRESOURCEA(selected-1);command.lpVerbW=MAKEINTRESOURCEW(selected-1);command.nShow=SW_SHOWNORMAL;command.ptInvoke=point;
        return SUCCEEDED(context->InvokeCommand(reinterpret_cast<CMINVOKECOMMANDINFO*>(&command)))?1:-10;
    }catch(...){return -11;}
}
