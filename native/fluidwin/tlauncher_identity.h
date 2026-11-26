// TLauncher Phase 1 namespace/profile boundary. Rendering is owned by FluidWin.
#pragma once
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <string>
#include <cwctype>
#include <fstream>
#include <filesystem>

namespace tlauncher {
inline std::wstring environment(const wchar_t* name) {
    DWORD n=GetEnvironmentVariableW(name,nullptr,0);
    if(!n || n>32768) return {};
    std::wstring value(n,L'\0');
    const DWORD got=GetEnvironmentVariableW(name,value.data(),n);
    if(!got || got>=n) return {};
    value.resize(got);return value;
}
inline const std::wstring& profile() {
    static const std::wstring value=environment(L"TLAUNCHER_NATIVE_TRIAL_DIR");
    return value;
}
inline std::wstring profileName() {
    const auto& path=profile();const auto i=path.find_last_of(L"\\/");
    return i==std::wstring::npos?std::wstring():path.substr(i+1);
}
inline const wchar_t* registry() {
    static const std::wstring key=L"Software\\TLauncher\\NativeBase\\Profiles\\"+profileName();
    return key.c_str();
}
inline const wchar_t* settingsRegistry() {
    static const std::wstring key=std::wstring(registry())+L"\\Impostazioni";return key.c_str();
}
inline const wchar_t* bannerRegistry() {
    static const std::wstring key=std::wstring(registry())+L"\\Banner";return key.c_str();
}
// Phase 1 is exclusively an isolated UAT binary. Production migration is gated.
inline bool trial(){return true;}
inline bool validateTrial() {
    const auto& path=profile();const auto leaf=profileName();
    if(path.size()<4 || path.size()>220 || path[1]!=L':' || path[2]!=L'\\' ||
       path.find(L"..")!=std::wstring::npos || leaf.rfind(L"tlauncher-native-trial-",0)!=0) return false;
    for(wchar_t c:leaf) if(!((c>=L'a'&&c<=L'z')||(c>=L'A'&&c<=L'Z')||(c>=L'0'&&c<=L'9')||c==L'-')) return false;
    wchar_t full[32768];
    DWORD n=GetFullPathNameW(path.c_str(),32768,full,nullptr);
    if(!n || n>=32768 || _wcsicmp(full,path.c_str())) return false;
    for(std::filesystem::path p(path);!p.empty();) {
        const DWORD attributes=GetFileAttributesW(p.c_str());
        if(attributes==INVALID_FILE_ATTRIBUTES || (attributes&FILE_ATTRIBUTE_REPARSE_POINT)) return false;
        auto parent=p.parent_path();if(parent==p)break;p=parent;
    }
    std::ifstream marker(std::filesystem::path(path)/L".tlauncher-native-trial",std::ios::binary);
    const std::string text((std::istreambuf_iterator<char>(marker)),std::istreambuf_iterator<char>());
    return text=="tlauncher-native isolated profile v1\n";
}
}
