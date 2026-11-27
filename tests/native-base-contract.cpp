// Hosted only. Exercise the actual migrated Settings/Notch source, not mocks.
#define TLAUNCHER_NATIVE_CI
#define wWinMain tlauncher_application_entry
#include "../native/fluidwin/fluidwin.cpp"
#undef wWinMain
#include <fstream>
#include <filesystem>
static void ciPump(int ms) {
    const ULONGLONG until=GetTickCount64()+ms;
    do {MSG m;while(PeekMessageW(&m,nullptr,0,0,PM_REMOVE)){TranslateMessage(&m);DispatchMessageW(&m);}Sleep(5);}while(GetTickCount64()<until);
}
#include "native-phase3-contract.h"
int main(int argc,char** argv) {
    if((argc!=2&&argc!=3)||!tlauncher::validateTrial())return 10;
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    OleInitialize(nullptr);
    std::ofstream report(std::filesystem::path(argv[1])/"native-contract.txt");
    if(wcsstr(tlauncher::registry(),L"FluidWin")||!wcsstr(tlauncher::registry(),L"TLauncher\\NativeBase\\Profiles"))return 11;
    impostazioni::carica();
    if(argc==3&&!strcmp(argv[2],"--verify-migrated")){
        const auto&s=impostazioni::valori();wallpaper::Store store;const auto c=store.load();
        if(testi::attuale()!=testi::VIETNAMITA||s.notchEnabled||s.dockVisibility!=2||s.stage.height!=174||s.stage.peekDelay!=750||s.spotlightMode!=2)return 83;
        if(s.spotlightShortcut!=shortcuts::Key{MOD_CONTROL|MOD_ALT,VK_F22}||s.stageShortcut!=shortcuts::Key{MOD_ALT|MOD_SHIFT,'S'}||s.stageShortcutEnabled)return 117;
        if(!c.enabled||c.scene!=wallpaper::sceneIndex("lake")||c.fps!=29||c.layout!=wallpaper::Layout::Panorama)return 84;
        gImpostazioni.apri();ciPump(350);if(!FindWindowW(L"TLauncher.Native.Impostazioni",nullptr))return 85;
        if(!phase3Snapshot(FindWindowW(L"TLauncher.Native.Impostazioni",nullptr),std::filesystem::path(argv[1])/L"phase3-migrated-vietnamese-settings.bmp"))return 86;
        report<<"MIGRATED_CUSTOM_NATIVE_SETTINGS_READ=PASS\nMIGRATED_WALLPAPER_READ=PASS\nVIETNAMESE_NATIVE_SETTINGS_RENDER=PASS\n";report.close();ExitProcess(0);
    }
    impostazioni::valori().notchEnabled=false;impostazioni::valori().notch=false;impostazioni::salva();
    impostazioni::valori().notchEnabled=true;impostazioni::carica();
    if(impostazioni::valori().notchEnabled||impostazioni::valori().notch)return 12;
    impostazioni::impostaAvvio(true);
    if(impostazioni::avvioAttivo())return 13;
    report<<"PROFILE_NAMESPACE=PASS\nNOTCH_OFF_PERSISTENCE=PASS\nSTARTUP_WRITE_BLOCK=PASS\n";
    {
        notch::Notch n;
        if(!n.crea(GetModuleHandleW(nullptr)))return 14;
        n.posto({0,0,2560,1080},1);n.testi(L"12:34",L"Thu 26");n.mostra(true);ciPump(100);
        n.abilita(false);ciPump(100);
        n.volume(0.5f,false);ciPump(80);
        n.enable(false);
        if(!n.ciReleased()||FindWindowW(L"TLauncher.Native.Notch",nullptr))return 15;
        const DWORD gdi=GetGuiResources(GetCurrentProcess(),GR_GDIOBJECTS);
        for(int i=0;i<4;i++) {
            n.enable(true);n.abilita(i%2==0);ciPump(40);
            if(!FindWindowW(L"TLauncher.Native.Notch",nullptr))return 16;
            n.enable(false);n.posto({-1080,0,0,1920},1.25f);n.testi(L"12:35",L"Fri 27");
            if(!n.ciReleased())return 17;
        }
        if(GetGuiResources(GetCurrentProcess(),GR_GDIOBJECTS)>gdi+2)return 18;
        n.mostra(false);
        report<<"NOTCH_TRUE_DISABLE=PASS\nNOTCH_LIVE_REENABLE=PASS\nNOTCH_FIXED_AND_ACTIVITY=PASS\nNOTCH_GDI_RELEASE=PASS\n";
    }
    if(int result=phase3Contracts(report,std::filesystem::path(argv[1])))return result;
    gImpostazioni.apri();ciPump(350);
    if(!FindWindowW(L"TLauncher.Native.Impostazioni",nullptr))return 19;
    report<<"NATIVE_SETTINGS_WINDOW=PASS\nOWNER_VISUAL_ACCEPTANCE=PENDING\n";
    report.close();
    // Upstream exits without static teardown after workers; retain that policy.
    ExitProcess(0);
}
