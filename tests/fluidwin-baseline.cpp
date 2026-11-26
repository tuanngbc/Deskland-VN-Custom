// Hosted-runner-only probe. The upstream headers are compiled unchanged.
// No FluidWin entry point, hooks, taskbar changes, startup or user registry writes.
#include <windows.h>
#include <dwmapi.h>
#include <fstream>
#include <filesystem>
#include <chrono>
#include <vector>
#include <iostream>
#include "genio.h"
#include "viva.h"

static bool screenshot(const std::filesystem::path& path, RECT area) {
    const int w=area.right-area.left, h=area.bottom-area.top;
    HDC screen=GetDC(nullptr), dc=CreateCompatibleDC(screen);
    BITMAPINFO info{}; info.bmiHeader.biSize=sizeof(BITMAPINFOHEADER);
    info.bmiHeader.biWidth=w; info.bmiHeader.biHeight=-h;
    info.bmiHeader.biPlanes=1; info.bmiHeader.biBitCount=32;
    void* pixels=nullptr;
    HBITMAP bitmap=CreateDIBSection(screen,&info,DIB_RGB_COLORS,&pixels,nullptr,0);
    if (!bitmap) { DeleteDC(dc); ReleaseDC(nullptr,screen); return false; }
    HGDIOBJ old=SelectObject(dc,bitmap);
    const bool ok=BitBlt(dc,0,0,w,h,screen,area.left,area.top,SRCCOPY|CAPTUREBLT)!=0;
    BITMAPFILEHEADER file{}; file.bfType=0x4d42;
    file.bfOffBits=sizeof(file)+sizeof(BITMAPINFOHEADER); file.bfSize=file.bfOffBits+w*h*4;
    std::ofstream out(path,std::ios::binary);
    if (ok) { out.write((const char*)&file,sizeof(file)); out.write((const char*)&info.bmiHeader,sizeof(info.bmiHeader)); out.write((const char*)pixels,w*h*4); }
    SelectObject(dc,old); DeleteObject(bitmap); DeleteDC(dc); ReleaseDC(nullptr,screen);
    return ok && out.good();
}

int main(int argc,char** argv) {
    if (!getenv("GITHUB_ACTIONS")) return 2;
    const std::filesystem::path dir=argc>1?argv[1]:"baseline-evidence";
    std::ofstream report(dir/"baseline.txt");
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    RoInitialize(RO_INIT_MULTITHREADED);
    report << "reference=2f773a8733b84c7dbee88fff6628d14e9ac802b9\n"
           << "original_application=COMPILED_NOT_STARTED\nowner_quality=NOT_VERIFIED\n";
    genio::Motore engine;
    if (!engine.inizializza(GetModuleHandleW(nullptr))) {
        report << "renderer=BLOCKED hardware D3D11 or composition unavailable\n";
        std::cout << "FLUIDWIN_BASELINE renderer=BLOCKED\n"; return 0;
    }
    report << "renderer=IMPLEMENTATION_EXECUTED\npresent=upstream Present(1,0)\n";
    const UINT colors[]={0xffffffff,0xff000000,0xffff0000,0xff00ff00,0xff0000ff,0x80800000};
    const RECT window{80,80,560,400},icon{340,600,388,648};
    const double progress[]={0,.05,.25,.5,.85,.97,1,.85,.5,.05,0};
    const char* names[]={"white","black","red","green","blue","alpha"};
    for (unsigned c=0;c<6;c++) {
        genio::Immagine image;
        const UINT color=colors[c];
        if (!engine.tinta(float((color>>16)&255)/255,float((color>>8)&255)/255,float(color&255)/255,float(color>>24)/255,image)) return 3;
        for (unsigned f=0;f<11;f++) {
            const auto start=std::chrono::steady_clock::now();
            genio::Elemento element{&image,window,icon,progress[f],1};
            engine.mostra(true); engine.disegna({element});
            DwmFlush();
            const auto us=std::chrono::duration_cast<std::chrono::microseconds>(std::chrono::steady_clock::now()-start).count();
            report << "fixture="<<names[c]<<" frame="<<f<<" progress="<<progress[f]<<" submit_and_flush_us="<<us<<" display_fps=NOT_MEASURED\n";
            if (!screenshot(dir/(std::string(names[c])+"-"+std::to_string(f)+".bmp"),RECT{60,60,600,680})) return 4;
        }
        image.libera();
    }
    engine.svuota(); engine.mostra(false);
    report << "screenshots=66 complete_canvas_including_perimeter\n"
           << "capture=NOT_EXERCISED (tint fixtures isolate original renderer)\n"
           << "drop_jelly_guardian_input_hooks=NOT_EXERCISED\n";
    std::cout << "FLUIDWIN_BASELINE renderer=EXECUTED screenshots=66 owner_quality=NOT_VERIFIED\n";
    return engine.gpuPersa()?5:0;
}
