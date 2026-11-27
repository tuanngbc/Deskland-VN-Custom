// Native shortcut validation and gap-free replacement. No input hooks.
#pragma once
#include "testi.h"
#include <windows.h>
#include <string>
#include <cwctype>
#include <algorithm>
#include <cstdlib>

namespace shortcuts {
struct Key {
    UINT mods=MOD_ALT,key=VK_SPACE;
    bool operator==(const Key&)const=default;
};
inline bool valid(Key k){
    constexpr UINT allowed=MOD_ALT|MOD_CONTROL|MOD_SHIFT|MOD_WIN;
    const bool key=(k.key>='0'&&k.key<='9')||(k.key>='A'&&k.key<='Z')||(k.key>=VK_F1&&k.key<=VK_F24)||
        (k.key>=VK_NUMPAD0&&k.key<=VK_NUMPAD9)||k.key==VK_SPACE||k.key==VK_TAB||k.key==VK_RETURN||k.key==VK_BACK||
        (k.key>=VK_PRIOR&&k.key<=VK_DOWN)||k.key==VK_INSERT||k.key==VK_DELETE;
    return (k.mods&allowed)&&!(k.mods&~allowed)&&key&&k.key!=VK_F12;
}
inline std::wstring label(Key k){
    std::wstring s;if(k.mods&MOD_CONTROL)s+=L"Ctrl+";if(k.mods&MOD_ALT)s+=L"Alt+";if(k.mods&MOD_SHIFT)s+=L"Shift+";if(k.mods&MOD_WIN)s+=L"Win+";
    if(k.key>='0'&&k.key<='Z')return s+wchar_t(k.key);
    if(k.key>=VK_F1&&k.key<=VK_F24)return s+L"F"+std::to_wstring(k.key-VK_F1+1);
    if(k.key>=VK_NUMPAD0&&k.key<=VK_NUMPAD9)return s+L"Num"+std::to_wstring(k.key-VK_NUMPAD0);
    switch(k.key){case VK_SPACE:return s+L"Space";case VK_TAB:return s+L"Tab";case VK_RETURN:return s+L"Enter";case VK_BACK:return s+L"Backspace";
        case VK_PRIOR:return s+L"PageUp";case VK_NEXT:return s+L"PageDown";case VK_HOME:return s+L"Home";case VK_END:return s+L"End";
        case VK_LEFT:return s+L"Left";case VK_RIGHT:return s+L"Right";case VK_UP:return s+L"Up";case VK_DOWN:return s+L"Down";
        case VK_INSERT:return s+L"Insert";case VK_DELETE:return s+L"Delete";default:return s+L"?";}
}
inline bool parse(const std::wstring& text,Key& result){
    Key k{0,0};size_t begin=0;
    while(begin<text.size()){
        auto end=text.find(L'+',begin);std::wstring token=text.substr(begin,end==std::wstring::npos?end:end-begin);
        token.erase(std::remove_if(token.begin(),token.end(),[](wchar_t c){return iswspace(c);}),token.end());for(auto& c:token)c=(wchar_t)towupper(c);
        if(token==L"CTRL"||token==L"CONTROL")k.mods|=MOD_CONTROL;else if(token==L"ALT")k.mods|=MOD_ALT;
        else if(token==L"SHIFT")k.mods|=MOD_SHIFT;else if(token==L"WIN")k.mods|=MOD_WIN;
        else{
            if(k.key)return false;
            if(token.size()==1&&((token[0]>=L'A'&&token[0]<=L'Z')||(token[0]>=L'0'&&token[0]<=L'9')))k.key=token[0];
            else if(token.size()>=2&&token[0]==L'F'){
                wchar_t* tail=nullptr;auto n=wcstoul(token.c_str()+1,&tail,10);if(!tail||*tail||n<1||n>24)return false;k.key=VK_F1+UINT(n)-1;
            }else if(token.size()==4&&token.rfind(L"NUM",0)==0&&token[3]>=L'0'&&token[3]<=L'9')k.key=VK_NUMPAD0+token[3]-L'0';
            else if(token==L"SPACE")k.key=VK_SPACE;else if(token==L"TAB")k.key=VK_TAB;else if(token==L"ENTER")k.key=VK_RETURN;
            else if(token==L"BACKSPACE")k.key=VK_BACK;else if(token==L"PAGEUP")k.key=VK_PRIOR;else if(token==L"PAGEDOWN")k.key=VK_NEXT;
            else if(token==L"HOME")k.key=VK_HOME;else if(token==L"END")k.key=VK_END;else if(token==L"LEFT")k.key=VK_LEFT;
            else if(token==L"RIGHT")k.key=VK_RIGHT;else if(token==L"UP")k.key=VK_UP;else if(token==L"DOWN")k.key=VK_DOWN;
            else if(token==L"INSERT")k.key=VK_INSERT;else if(token==L"DELETE")k.key=VK_DELETE;else return false;
        }
        if(end==std::wstring::npos)break;begin=end+1;if(begin==text.size())return false;
    }
    if(!valid(k))return false;result=k;return true;
}
class Binding {
    HWND window=nullptr;int base,spare,slot=0;Key current{};bool registered=false;
public:
    Binding(int first,int second):base(first),spare(second){}
    ~Binding(){stop();}
    Binding(const Binding&)=delete;Binding& operator=(const Binding&)=delete;
    bool matches(WPARAM id)const{return registered&&id==WPARAM(slot?spare:base);}
    bool active()const{return registered;}
    Key key()const{return current;}
    void stop(){if(registered)UnregisterHotKey(window,slot?spare:base);registered=false;}
    bool apply(HWND h,Key wanted,bool enabled){
        if(!enabled){stop();return true;}if(!h||!valid(wanted))return false;
        if(registered&&window==h&&current==wanted)return true;
        const int next=registered?1-slot:slot;
        // Reserve the new combination while the old one is still registered.
        // A rejected replacement cannot release the owner's working shortcut.
        if(!RegisterHotKey(h,next?spare:base,wanted.mods|MOD_NOREPEAT,wanted.key))return false;
        if(registered)UnregisterHotKey(window,slot?spare:base);
        window=h;slot=next;current=wanted;registered=true;return true;
    }
};
} // namespace shortcuts
