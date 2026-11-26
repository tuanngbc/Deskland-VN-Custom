// TLauncher adapter. Imported implementation and provenance are in upstream/.
#include <windows.h>
#include <shellapi.h>
#include <cstdio>
#include <cstdint>
#include "upstream/destro.h"

namespace {
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
unsigned tlauncher_fluidwin_shell_menu_fixture(HWND owner) {
    try {
        testi::lingua().store(testi::INGLESE);
        HMENU child=CreatePopupMenu();
        AppendMenuW(child,MF_STRING,73,L"Fixture command");
        auto disabled=menu::voce(L"Disabled",11); disabled.disattivata=true;
        auto nested=menu::voce(L"Submenu",0); nested.sotto=child;
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
