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
    std::filesystem::create_directories(tlauncher::dockPins());
    auto pin=tlauncher::dockPins()/L"fixture.lnk";
    if(!tlauncher::ownsDockPin(pin.wstring())||tlauncher::ownsDockPin(L"C:\\Windows\\fixture.lnk"))return 82;
    report<<"DOCK_PIN_WRITE_BOUNDARY=PASS\n";
    auto& s=impostazioni::valori();s.stage.height=174;s.stage.peekDelay=750;s.stage.peekLive=false;s.dockVisibility=2;s.dockBarCloseDelay=900;s.spotlightMode=2;impostazioni::salva();
    s.stage.height=72;s.dockVisibility=0;impostazioni::carica();
    if(s.stage.height!=174||s.stage.peekDelay!=750||s.stage.peekLive||s.dockVisibility!=2||s.dockBarCloseDelay!=900||s.spotlightMode!=2)return 61;
    appcatalog::Metadata meta;meta.name=L"Ứng dụng thử nghiệm";meta.favorite=true;meta.folder=L"Công việc";meta.manual=true;meta.icon=L"custom.ico";
    appcatalog::save(L"C:\\fixture\\tool.exe",meta);appcatalog::folder(L"",meta.folder);appcatalog::loaded=false;appcatalog::metadata.clear();
    auto restored=appcatalog::get(L"C:\\fixture\\tool.exe");if(restored.name!=meta.name||!restored.favorite||restored.folder!=meta.folder||!restored.manual||restored.icon!=meta.icon)return 62;
    appcatalog::folder(meta.folder,L"Đã đổi tên");if(appcatalog::get(L"C:\\fixture\\tool.exe").folder!=L"Đã đổi tên")return 63;
    appcatalog::folder(L"Đã đổi tên",L"");if(appcatalog::get(L"C:\\fixture\\tool.exe").manual!=true||!appcatalog::get(L"C:\\fixture\\tool.exe").folder.empty())return 64;
    const std::wstring alias=L"C:\\fixture\\tool-alias.lnk",canonical=L"exe:c:\\fixture\\tool.exe";
    appcatalog::bind(L"C:\\fixture\\tool.exe",canonical);appcatalog::bind(alias,canonical);
    auto shared=appcatalog::get(alias);if(shared.name!=meta.name||shared.icon!=meta.icon||!shared.favorite)return 98;
    shared.name=L"Tên mới dùng chung";shared.folder=L"Dự án";shared.icon=L"new-custom.png";shared.hidden=true;appcatalog::save(alias,shared);
    auto original=appcatalog::get(L"C:\\fixture\\tool.exe");
    if(original.name!=shared.name||original.icon!=shared.icon||!original.hidden||!original.manual)return 99;
    appcatalog::folder(L"Dự án",L"Đã gộp");if(appcatalog::get(alias).folder!=L"Đã gộp"||appcatalog::get(alias).name!=shared.name)return 100;
    appcatalog::loaded=false;appcatalog::metadata.clear();appcatalog::aliases.clear();appcatalog::owners.clear();appcatalog::load();
    appcatalog::bind(L"C:\\fixture\\tool.exe",canonical);appcatalog::bind(alias,canonical);
    if(appcatalog::get(L"C:\\fixture\\tool.exe").name!=shared.name||appcatalog::iconSource(alias)!=shared.icon)return 101;
    auto shown=appcatalog::get(alias);shown.hidden=false;appcatalog::save(alias,shown);
    wchar_t actualExe[32768]{};GetModuleFileNameW(nullptr,actualExe,32768);
    IShellLinkW* link=nullptr;IPersistFile* persist=nullptr;
    if(FAILED(CoCreateInstance(CLSID_ShellLink,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&link))))return 102;
    const auto shortcut=evidence/L"canonical-fixture.lnk";link->SetPath(actualExe);
    if(FAILED(link->QueryInterface(IID_PPV_ARGS(&persist)))||FAILED(persist->Save(shortcut.c_str(),TRUE)))return 103;
    const auto exeIdentity=appcatalog::identity(actualExe),linkIdentity=appcatalog::identity(shortcut.wstring());
    if(exeIdentity!=linkIdentity)return 104;
    link->SetArguments(L"--separate-fixture");if(FAILED(persist->Save(shortcut.c_str(),TRUE)))return 105;
    persist->Release();link->Release();
    if(appcatalog::identity(shortcut.wstring())==exeIdentity)return 106;
    report<<"ACTUAL_EXE_LNK_IDENTITY_AND_ARGUMENT_DISTINCTION=PASS\n";
    report<<"CANONICAL_METADATA_ALIAS_PROPAGATION=PASS\nCANONICAL_METADATA_RELOAD_AND_FOLDER_MIGRATION=PASS\n";
    report<<"PHASE3_CUSTOM_SETTINGS_PERSIST=PASS\nVIETNAMESE_ACCENT_SEARCH=PASS\nCATALOG_FOLDER_RENAME_DELETE_PRESERVE_APPS=PASS\n";
    HWND source=CreateWindowW(L"STATIC",L"Stage actual source window",WS_OVERLAPPEDWINDOW|WS_VISIBLE,280,160,640,420,nullptr,nullptr,GetModuleHandleW(nullptr),nullptr);
    if(!source)return 65;ciPump(100);DWORD pid=0;DWORD tid=GetWindowThreadProcessId(source,&pid);
    stage::Manager manager;manager.inventory=[&]{return IsWindow(source)?std::vector<stage::Window>{{source,pid,tid,L"fixture",L"Cửa sổ thử nghiệm",L""}}:std::vector<stage::Window>{};};
    bool activated=false;manager.activation=[&](HWND h){activated=h==source;};stage::Settings st;st.enabled=true;st.peekDelay=100;manager.configure(st);ciPump(180);manager.changed();ciPump(60);
    HWND rail=FindWindowW(L"TLauncher.Native.Stage",nullptr);if(!rail||manager.windowCount()!=1)return 66;
    WINDOWPLACEMENT before{sizeof(before)},after{sizeof(after)};GetWindowPlacement(source,&before);HWND focus=GetForegroundWindow();
    POINT previousPointer{};GetCursorPos(&previousPointer);POINT hoverPoint{45,45};ClientToScreen(rail,&hoverPoint);
    if(!SetCursorPos(hoverPoint.x,hoverPoint.y))return 79;ciPump(180);
    HWND peek=FindWindowW(L"TLauncher.Native.StagePeek",nullptr);if(!peek)return 67;
    GetWindowPlacement(source,&after);if(memcmp(&before,&after,sizeof(before))||focus!=GetForegroundWindow())return 68;
    if(!phase3Snapshot(rail,evidence/L"phase3-stage.bmp")||!phase3Snapshot(peek,evidence/L"phase3-peek.bmp"))return 69;
    // Crossing rail -> Peek must keep the preview clickable; leaving both dismisses it.
    POINT previewPoint{12,12};ClientToScreen(peek,&previewPoint);SetCursorPos(previewPoint.x,previewPoint.y);
    ciPump(220);peek=FindWindowW(L"TLauncher.Native.StagePeek",nullptr);if(!peek)return 70;
    SendMessageW(peek,WM_LBUTTONUP,0,MAKELPARAM(12,12));if(!activated)return 71;activated=false;
    report<<"STAGE_PEEK_CLICK_ACTIVATION=PASS\n";
    SendMessageW(rail,WM_LBUTTONUP,0,MAKELPARAM(45,45));if(!activated)return 87;
    ShowWindow(source,SW_MINIMIZE);ciPump(100);manager.changed();ciPump(80);
    if(!IsIconic(source)||!manager.snapshotBytes()||!manager.cachedPreviewCount())return 80;
    if(!phase3Snapshot(rail,evidence/L"phase3-stage-minimized.bmp"))return 81;
    report<<"STAGE_MINIMIZED_SHARED_SNAPSHOT_NO_RESTORE=PASS\n";
    st.perspective=true;manager.configure(st);ciPump(100);manager.changed();rail=FindWindowW(L"TLauncher.Native.Stage",nullptr);
    if(!rail||!IsIconic(source)||!manager.cachedPreviewCount()||!phase3Snapshot(rail,evidence/L"phase3-stage-perspective.bmp"))return 93;
    report<<"STAGE_NATIVE_PROJECTIVE_CARD_RENDER=PASS\n";
    SetCursorPos(previousPointer.x,previousPointer.y);
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
    gRicerca.elencoApp=[]{std::vector<std::pair<std::wstring,std::wstring>> result;for(int i=0;i<20;i++)result.push_back({L"Tool "+std::to_wstring(i),L"C:\\fixture\\tool"+std::to_wstring(i)+L".exe"});return result;};
    gRicerca.fullscreen=true;gRicerca.apri();ciPump(450);search=FindWindowW(L"TLauncher.Native.Ricerca",nullptr);
    SendMessageW(search,WM_CHAR,'t',0);SendMessageW(search,WM_CHAR,'o',0);ciPump(50);
    for(int i=0;i<8;i++)SendMessageW(search,WM_KEYDOWN,VK_RIGHT,0);const auto selected=gRicerca.ciSelection();
    SendMessageW(search,WM_KEYDOWN,VK_F6,0);if(gRicerca.ciSelection()!=selected)return 88;
    SendMessageW(search,WM_KEYDOWN,VK_F6,0);if(gRicerca.ciSelection()!=selected)return 89;
    wchar_t executable[32768]{};GetModuleFileNameW(nullptr,executable,32768);std::wstring dropPath=executable;
    HGLOBAL drop=GlobalAlloc(GMEM_MOVEABLE|GMEM_ZEROINIT,sizeof(DROPFILES)+(dropPath.size()+2)*sizeof(wchar_t));
    if(!drop)return 90;auto data=static_cast<DROPFILES*>(GlobalLock(drop));data->pFiles=sizeof(DROPFILES);data->fWide=TRUE;
    memcpy(reinterpret_cast<BYTE*>(data)+sizeof(DROPFILES),dropPath.c_str(),(dropPath.size()+1)*sizeof(wchar_t));GlobalUnlock(drop);
    SendMessageW(search,WM_DROPFILES,reinterpret_cast<WPARAM>(drop),0);if(!appcatalog::get(dropPath).manual)return 91;
    SendMessageW(search,WM_KEYDOWN,VK_ESCAPE,0);ciPump(220);if(gRicerca.aperta())return 92;
    report<<"SPOTLIGHT_F6_LOGICAL_SELECTION_PRESERVED=PASS\nSPOTLIGHT_NATIVE_EXE_DROP_METADATA=PASS\nSPOTLIGHT_ESCAPE_DISMISS=PASS\n";
    // Closing invalidates outstanding file/icon work. A fresh opening starts
    // with no stale query; disabling releases HWND, timers, fonts and bitmaps.
    gRicerca.ferma();
    if(FindWindowW(L"TLauncher.Native.Ricerca",nullptr)||gRicerca.ciBitmapCount()||gRicerca.ciPendingWork())return 94;
    gRicerca.fullscreen=false;gRicerca.apri();ciPump(80);search=FindWindowW(L"TLauncher.Native.Ricerca",nullptr);
    if(!search||!gRicerca.ciQuery().empty())return 95;
    SendMessageW(search,WM_CHAR,'t',0);SendMessageW(search,WM_CHAR,'o',0);ciPump(190);gRicerca.ferma();
    for(int i=0;i<400&&!gRicerca.ciWorkerIdle();++i)ciPump(10);
    if(!gRicerca.ciWorkerIdle()||gRicerca.ciPendingWork()||gRicerca.ciBitmapCount()||FindWindowW(L"TLauncher.Native.Ricerca",nullptr))return 96;
    for(int i=0;i<3;++i){gRicerca.apri();ciPump(40);gRicerca.ferma();}
    if(FindWindowW(L"TLauncher.Native.Ricerca",nullptr)||gRicerca.ciPendingWork())return 97;
    report<<"SPOTLIGHT_DISABLE_NATIVE_RESOURCES_RELEASED=PASS\nSPOTLIGHT_WORKER_DRAINS_AND_EXITS=PASS\nSPOTLIGHT_REOPEN_AFTER_CANCELLATION=PASS\n";
    report<<"SPOTLIGHT_NATIVE_COMPACT_FULLSCREEN=PASS\nF6_QUERY_PRESERVED=PASS\nOWNER_VISUAL_ACCEPTANCE=PENDING\n";
    return 0;
}
