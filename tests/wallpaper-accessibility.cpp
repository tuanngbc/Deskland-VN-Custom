// Real out-of-process MSAA contract for the full hosted native executable.
#include <windows.h>
#include <oleacc.h>
#include <iostream>
#include <string>
int main(){
    wchar_t hosted[16]{};GetEnvironmentVariableW(L"GITHUB_ACTIONS",hosted,16);
    if(wcscmp(hosted,L"true"))return 2;
    CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED);
    HWND h=FindWindowW(L"TLauncher.Native.Impostazioni",nullptr);IAccessible*page=nullptr;
    HRESULT hr=AccessibleObjectFromWindow(h,OBJID_CLIENT,IID_IAccessible,reinterpret_cast<void**>(&page));
    if(FAILED(hr)||!page){std::cerr<<"MSAA page unavailable "<<std::hex<<hr;return 1;}
    LONG count=0;page->get_accChildCount(&count);bool toggle=false,gallery=false,weather=false;
    for(LONG i=1;i<=count;i++){
        VARIANT child{};child.vt=VT_I4;child.lVal=i;BSTR name=nullptr;
        if(SUCCEEDED(page->get_accName(child,&name))&&name){std::wstring text(name,SysStringLen(name));
            if(text.find(L"Live Wallpaper")!=std::wstring::npos)toggle=true;
            if(text==L"Bãi biển")gallery=true;
            if(text==L"Rain")weather=true;
            SysFreeString(name);
        }
    }
    page->Release();CoUninitialize();
    if(count<30||!toggle||!gallery||!weather){std::cerr<<"MSAA controls incomplete: "<<count;return 1;}
    std::cout<<"NATIVE_WALLPAPER_MSAA=PASS\nACCESSIBLE_CHILDREN="<<count<<'\n';return 0;
}
