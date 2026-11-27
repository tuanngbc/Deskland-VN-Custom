// Compiled/executed exclusively on the hosted Windows runner.
#pragma once
static bool phase3Snapshot(HWND h,const std::filesystem::path& file) {
    RECT r{};GetWindowRect(h,&r);int w=r.right-r.left,hh=r.bottom-r.top;if(w<=0||hh<=0)return false;
    HDC screen=GetDC(nullptr),dc=CreateCompatibleDC(screen);void* pixels=nullptr;
    BITMAPINFO info{};info.bmiHeader={sizeof(BITMAPINFOHEADER),w,-hh,1,32,BI_RGB,0,0,0,0,0};
    HBITMAP bitmap=CreateDIBSection(screen,&info,DIB_RGB_COLORS,&pixels,nullptr,0);bool ok=false;
    if(bitmap&&pixels&&dc){auto old=SelectObject(dc,bitmap);ok=BitBlt(dc,0,0,w,hh,screen,r.left,r.top,SRCCOPY|CAPTUREBLT);GdiFlush();
        BITMAPFILEHEADER head{};head.bfType=0x4d42;head.bfOffBits=sizeof(head)+sizeof(info.bmiHeader);head.bfSize=head.bfOffBits+w*hh*4;
        std::ofstream out(file,std::ios::binary);out.write((char*)&head,sizeof(head));out.write((char*)&info.bmiHeader,sizeof(info.bmiHeader));out.write((char*)pixels,size_t(w)*hh*4);ok=ok&&bool(out);SelectObject(dc,old);}
    if(bitmap)DeleteObject(bitmap);if(dc)DeleteDC(dc);ReleaseDC(nullptr,screen);return ok;
}
static int phase3Contracts(std::ostream& report,const std::filesystem::path& evidence) {
    if(appcatalog::fold(L"Tiếng Việt Đặng")!=L"tieng viet dang")return 60;
    auto& s=impostazioni::valori();s.stage.height=174;s.stage.peekDelay=750;s.stage.peekLive=false;s.dockVisibility=2;s.dockBarCloseDelay=900;s.spotlightMode=2;impostazioni::salva();
    s.stage.height=72;s.dockVisibility=0;impostazioni::carica();
    if(s.stage.height!=174||s.stage.peekDelay!=750||s.stage.peekLive||s.dockVisibility!=2||s.dockBarCloseDelay!=900||s.spotlightMode!=2)return 61;
    appcatalog::Metadata meta;meta.name=L"Ứng dụng thử nghiệm";meta.favorite=true;meta.folder=L"Công việc";meta.manual=true;meta.icon=L"custom.ico";
    appcatalog::save(L"C:\\fixture\\tool.exe",meta);appcatalog::folder(L"",meta.folder);appcatalog::loaded=false;appcatalog::metadata.clear();
    auto restored=appcatalog::get(L"C:\\fixture\\tool.exe");if(restored.name!=meta.name||!restored.favorite||restored.folder!=meta.folder||!restored.manual||restored.icon!=meta.icon)return 62;
    appcatalog::folder(meta.folder,L"Đã đổi tên");if(appcatalog::get(L"C:\\fixture\\tool.exe").folder!=L"Đã đổi tên")return 63;
    appcatalog::folder(L"Đã đổi tên",L"");if(appcatalog::get(L"C:\\fixture\\tool.exe").manual!=true||!appcatalog::get(L"C:\\fixture\\tool.exe").folder.empty())return 64;
    report<<"PHASE3_CUSTOM_SETTINGS_PERSIST=PASS\nVIETNAMESE_ACCENT_SEARCH=PASS\nCATALOG_FOLDER_RENAME_DELETE_PRESERVE_APPS=PASS\n";
    HWND source=CreateWindowW(L"STATIC",L"Stage actual source window",WS_OVERLAPPEDWINDOW|WS_VISIBLE,280,160,640,420,nullptr,nullptr,GetModuleHandleW(nullptr),nullptr);
    if(!source)return 65;ciPump(100);DWORD pid=0;DWORD tid=GetWindowThreadProcessId(source,&pid);
    stage::Manager manager;manager.inventory=[&]{return IsWindow(source)?std::vector<stage::Window>{{source,pid,tid,L"fixture",L"Cửa sổ thử nghiệm",L""}}:std::vector<stage::Window>{};};
    bool activated=false;manager.activation=[&](HWND h){activated=h==source;};stage::Settings st;st.enabled=true;st.peekDelay=100;manager.configure(st);ciPump(180);manager.changed();ciPump(60);
    HWND rail=FindWindowW(L"TLauncher.Native.Stage",nullptr);if(!rail||manager.windowCount()!=1)return 66;
    WINDOWPLACEMENT before{sizeof(before)},after{sizeof(after)};GetWindowPlacement(source,&before);HWND focus=GetForegroundWindow();
    SendMessageW(rail,WM_MOUSEMOVE,0,MAKELPARAM(45,45));ciPump(180);
    HWND peek=FindWindowW(L"TLauncher.Native.StagePeek",nullptr);if(!peek)return 67;
    GetWindowPlacement(source,&after);if(memcmp(&before,&after,sizeof(before))||focus!=GetForegroundWindow())return 68;
    if(!phase3Snapshot(rail,evidence/L"phase3-stage.bmp")||!phase3Snapshot(peek,evidence/L"phase3-peek.bmp"))return 69;
    SendMessageW(rail,WM_MOUSELEAVE,0,0);if(FindWindowW(L"TLauncher.Native.StagePeek",nullptr))return 70;
    SendMessageW(rail,WM_LBUTTONUP,0,MAKELPARAM(45,45));if(!activated)return 71;
    ShowWindow(source,SW_MINIMIZE);ciPump(60);manager.changed();
    DestroyWindow(source);manager.changed();st.enabled=false;manager.configure(st);
    if(manager.railCount()||manager.snapshotBytes()||FindWindowW(L"TLauncher.Native.Stage",nullptr)||FindWindowW(L"TLauncher.Native.StagePeek",nullptr))return 72;
    report<<"STAGE_ACTUAL_NATIVE_WINDOW=PASS\nSTAGE_PEEK_FOCUS_PLACEMENT_UNCHANGED=PASS\nSTAGE_SHARED_ACTIVATION_CALLBACK=PASS\nSTAGE_DESTROY_AND_DISABLE_CLEANUP=PASS\n";
    gRicerca.elencoApp=[]{return std::vector<std::pair<std::wstring,std::wstring>>{{L"Notepad",L"notepad.exe"},{L"Ứng dụng",L"C:\\fixture\\tool.exe"}};};
    gRicerca.conOnda=[]{return false;};gRicerca.fullscreen=false;gRicerca.apri();ciPump(500);
    HWND search=FindWindowW(L"TLauncher.Native.Ricerca",nullptr);if(!search)return 73;
    SendMessageW(search,WM_CHAR,'n',0);SendMessageW(search,WM_CHAR,'o',0);ciPump(50);auto query=gRicerca.ciQuery();
    if(!phase3Snapshot(search,evidence/L"phase3-compact.bmp"))return 74;
    SendMessageW(search,WM_KEYDOWN,VK_F6,0);ciPump(80);if(!gRicerca.fullscreen||gRicerca.ciQuery()!=query)return 75;
    RECT full{};GetWindowRect(search,&full);RECT screen=vetro::areaPrincipale();if(!EqualRect(&full,&screen))return 76;
    if(!phase3Snapshot(search,evidence/L"phase3-fullscreen.bmp"))return 77;
    SendMessageW(search,WM_KEYDOWN,VK_F6,0);if(gRicerca.fullscreen||gRicerca.ciQuery()!=query)return 78;
    gRicerca.chiudi();ciPump(250);
    report<<"SPOTLIGHT_NATIVE_COMPACT_FULLSCREEN=PASS\nF6_QUERY_PRESERVED=PASS\nOWNER_VISUAL_ACCEPTANCE=PENDING\n";
    return 0;
}
