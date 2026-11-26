// TLauncher adapter. Imported implementation and provenance are in upstream/.
#include <windows.h>
#include <cstdio>
#include <cstdint>
#include "upstream/destro.h"

extern "C" {
int tlauncher_fluidwin_shell_hit(int x,int y) {
    return destro::vistaShell(POINT{x,y}) ? 1 : 0;
}
int tlauncher_fluidwin_shell_menu(HWND owner,int x,int y) {
    // Shell COM, Direct2D and the share helper all belong to the Dock STA.
    // Never call this from the low-level hook's thread.
    try {
        testi::lingua().store(testi::INGLESE);
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
}
