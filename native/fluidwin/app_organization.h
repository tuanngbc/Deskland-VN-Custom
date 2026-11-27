// User metadata overlays the single Dock catalog; it is never another discovery index.
#pragma once
#include "app_icons.h"
#include "testi.h"
#include <commdlg.h>
#include <shellapi.h>
#include <shobjidl.h>
#include <shlobj.h>
#include <propkey.h>
#include <unordered_set>

namespace appcatalog {
inline std::wstring normalize(std::wstring value) {
    for(auto& c:value){if(c==L'/')c=L'\\';c=(wchar_t)towlower(c);}return value;
}
inline std::wstring fold(const std::wstring& value) {
    int n=NormalizeString(NormalizationD,value.c_str(),int(value.size()),nullptr,0);
    std::wstring decomposed(n>0?n:0,L'\0');if(n>0)n=NormalizeString(NormalizationD,value.c_str(),int(value.size()),decomposed.data(),n);
    if(n<=0)return normalize(value);decomposed.resize(n);
    std::wstring result;for(wchar_t c:decomposed){if(c>=0x300&&c<=0x36f)continue;if(c==L'đ'||c==L'Đ')c=L'd';result+=(wchar_t)towlower(c);}return result;
}
inline std::wstring identity(const std::wstring& command) {
    std::wstring target=command,id;
    IShellItem2* item=nullptr;
    if(SUCCEEDED(SHCreateItemFromParsingName(command.c_str(),nullptr,IID_PPV_ARGS(&item)))) {
        LPWSTR s=nullptr;
        if(SUCCEEDED(item->GetString(PKEY_AppUserModel_ID,&s))&&s){id=s;CoTaskMemFree(s);}
        s=nullptr;if(SUCCEEDED(item->GetString(PKEY_Link_TargetParsingPath,&s))&&s){target=s;CoTaskMemFree(s);}item->Release();
    }
    // Target path takes precedence for Win32 entries from AppsFolder and LNK.
    if(GetFileAttributesW(target.c_str())!=INVALID_FILE_ATTRIBUTES&&normalize(target).ends_with(L".exe"))return L"exe:"+normalize(target);
    return !id.empty()?L"aumid:"+normalize(id):normalize(command);
}
inline std::wstring root(){return std::wstring(tlauncher::registry())+L"\\Catalog";}
inline std::wstring key(const std::wstring& command){const auto s=normalize(command);return root()+L"\\Apps\\"+std::to_wstring(appicons::hash(s.data(),s.size()*sizeof(wchar_t)));}
inline std::wstring read(const std::wstring& path,const wchar_t* name) {
    DWORD bytes=0;if(RegGetValueW(HKEY_CURRENT_USER,path.c_str(),name,RRF_RT_REG_SZ,nullptr,nullptr,&bytes)!=ERROR_SUCCESS||bytes>65536)return {};
    std::wstring value(bytes/sizeof(wchar_t),L'\0');if(RegGetValueW(HKEY_CURRENT_USER,path.c_str(),name,RRF_RT_REG_SZ,nullptr,value.data(),&bytes)!=ERROR_SUCCESS)return {};
    while(!value.empty()&&value.back()==0)value.pop_back();return value;
}
inline void write(const std::wstring& path,const wchar_t* name,const std::wstring& value) {
    HKEY h=nullptr;if(RegCreateKeyExW(HKEY_CURRENT_USER,path.c_str(),0,nullptr,0,KEY_SET_VALUE,nullptr,&h,nullptr)!=ERROR_SUCCESS)return;
    RegSetValueExW(h,name,0,REG_SZ,(const BYTE*)value.c_str(),DWORD((value.size()+1)*sizeof(wchar_t)));RegCloseKey(h);
}
struct Metadata {std::wstring name,icon,folder;bool favorite=false,hidden=false,manual=false;int order=0;};
inline std::unordered_map<std::wstring,Metadata> metadata;
inline bool loaded=false;
inline void load() {
    if(loaded)return;loaded=true;HKEY h=nullptr;const auto path=root()+L"\\Apps";
    if(RegOpenKeyExW(HKEY_CURRENT_USER,path.c_str(),0,KEY_READ,&h)!=ERROR_SUCCESS)return;
    for(DWORD i=0;;++i){wchar_t sub[128];DWORD n=128;if(RegEnumKeyExW(h,i,sub,&n,nullptr,nullptr,nullptr,nullptr)!=ERROR_SUCCESS)break;
        auto k=path+L"\\"+sub;auto cmd=read(k,L"Command");if(cmd.empty())continue;
        Metadata m;m.name=read(k,L"Name");m.icon=read(k,L"Icon");m.folder=read(k,L"Folder");m.favorite=read(k,L"Favorite")==L"1";m.hidden=read(k,L"Hidden")==L"1";m.manual=read(k,L"Manual")==L"1";
        const auto order=read(k,L"Order");if(!order.empty())m.order=_wtoi(order.c_str());metadata[cmd]=std::move(m);
    }RegCloseKey(h);
}
inline Metadata get(const std::wstring& cmd){load();auto it=metadata.find(cmd);return it==metadata.end()?Metadata{}:it->second;}
inline void save(const std::wstring& cmd,const Metadata& m) {
    load();metadata[cmd]=m;auto k=key(cmd);write(k,L"Command",cmd);write(k,L"Name",m.name);write(k,L"Icon",m.icon);write(k,L"Folder",m.folder);
    write(k,L"Favorite",m.favorite?L"1":L"0");write(k,L"Hidden",m.hidden?L"1":L"0");write(k,L"Manual",m.manual?L"1":L"0");write(k,L"Order",std::to_wstring(m.order));
}
inline std::wstring iconSource(const std::wstring& cmd){const auto m=get(cmd);return m.icon.empty()?cmd:m.icon;}
inline std::vector<std::wstring> folders() {
    load();std::vector<std::wstring> out;
    HKEY h=nullptr;auto k=root()+L"\\Folders";
    if(RegOpenKeyExW(HKEY_CURRENT_USER,k.c_str(),0,KEY_READ,&h)==ERROR_SUCCESS){for(DWORD i=0;;++i){wchar_t name[512];DWORD n=512;if(RegEnumValueW(h,i,name,&n,nullptr,nullptr,nullptr,nullptr)!=ERROR_SUCCESS)break;out.emplace_back(name,n);}RegCloseKey(h);}
    for(const auto& e:metadata)if(!e.second.folder.empty())out.push_back(e.second.folder);
    std::sort(out.begin(),out.end());out.erase(std::unique(out.begin(),out.end()),out.end());return out;
}
inline void folder(const std::wstring& oldName,const std::wstring& newName) {
    if(!newName.empty())write(root()+L"\\Folders",newName.c_str(),L"1");
    if(!oldName.empty()&&oldName!=newName){HKEY h=nullptr;if(RegOpenKeyExW(HKEY_CURRENT_USER,(root()+L"\\Folders").c_str(),0,KEY_SET_VALUE,&h)==ERROR_SUCCESS){RegDeleteValueW(h,oldName.c_str());RegCloseKey(h);}}
    for(auto& e:metadata)if(e.second.folder==oldName&&!oldName.empty()){auto m=e.second;m.folder=newName;save(e.first,m);}
}
// Small OS-native input dialog. Windows owns edit behavior, IME and accessibility.
inline bool prompt(HWND owner,const wchar_t* title,std::wstring& value) {
    struct State{std::wstring value;const wchar_t* title;};State state{value,title};
    struct Template{DLGTEMPLATE dialog;WORD menu=0,cls=0,title=0;} t{};
    t.dialog.style=WS_POPUP|WS_CAPTION|WS_SYSMENU|DS_MODALFRAME|DS_CENTER;t.dialog.cx=260;t.dialog.cy=76;
    INT_PTR result=DialogBoxIndirectParamW(GetModuleHandleW(nullptr),&t.dialog,owner,[](HWND h,UINT m,WPARAM w,LPARAM l)->INT_PTR {
        if(m==WM_INITDIALOG){auto s=(State*)l;SetWindowLongPtrW(h,DWLP_USER,l);SetWindowTextW(h,s->title);
            RECT r{};GetClientRect(h,&r);int gap=12,height=26;UINT dpi=GetDpiForWindow(h);gap=MulDiv(gap,dpi,96);height=MulDiv(height,dpi,96);
            HWND edit=CreateWindowExW(WS_EX_CLIENTEDGE,L"EDIT",s->value.c_str(),WS_CHILD|WS_VISIBLE|WS_TABSTOP|ES_AUTOHSCROLL,gap,gap,r.right-2*gap,height,h,(HMENU)100,GetModuleHandleW(nullptr),nullptr);
            SendMessageW(edit,EM_SETLIMITTEXT,240,0);SendMessageW(edit,EM_SETSEL,0,-1);
            CreateWindowW(L"BUTTON",testi::t(L"Salva",L"Save"),WS_CHILD|WS_VISIBLE|WS_TABSTOP|BS_DEFPUSHBUTTON,r.right-2*gap-160,gap*2+height,76,height,h,(HMENU)IDOK,GetModuleHandleW(nullptr),nullptr);
            CreateWindowW(L"BUTTON",testi::t(L"Annulla",L"Cancel"),WS_CHILD|WS_VISIBLE|WS_TABSTOP,r.right-gap-76,gap*2+height,76,height,h,(HMENU)IDCANCEL,GetModuleHandleW(nullptr),nullptr);
            SetFocus(edit);return FALSE;
        }
        if(m==WM_COMMAND&&(LOWORD(w)==IDOK||LOWORD(w)==IDCANCEL)){if(LOWORD(w)==IDOK){wchar_t text[241]{};GetDlgItemTextW(h,100,text,241);((State*)GetWindowLongPtrW(h,DWLP_USER))->value=text;}EndDialog(h,LOWORD(w));return TRUE;}
        return FALSE;
    },(LPARAM)&state);
    if(result!=IDOK)return false;value=std::move(state.value);return true;
}
inline std::wstring choose(HWND owner,bool image) {
    wchar_t file[32768]{};OPENFILENAMEW o{};o.lStructSize=sizeof(o);o.hwndOwner=owner;o.lpstrFile=file;o.nMaxFile=32768;o.Flags=OFN_FILEMUSTEXIST|OFN_PATHMUSTEXIST|OFN_NOCHANGEDIR;
    o.lpstrFilter=image?L"Images\0*.ico;*.png;*.jpg;*.bmp\0\0":L"Applications\0*.exe;*.lnk\0\0";
    return GetOpenFileNameW(&o)?std::wstring(file):std::wstring();
}
} // namespace appcatalog
