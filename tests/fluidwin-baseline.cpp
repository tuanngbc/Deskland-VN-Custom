// Hosted-runner-only probe. The upstream headers are compiled unchanged.
// No FluidWin entry point, hooks, taskbar changes, startup or user registry writes.
#include <windows.h>
#include <dwmapi.h>
#include <fstream>
#include <filesystem>
#include <chrono>
#include <vector>
#include <iostream>
#include <cstring>
#include <thread>
#include "genio.h"
#include "viva.h"

static bool screenshot(const std::filesystem::path& path, RECT area,std::vector<UINT>* retained=nullptr) {
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
    if (ok) { out.write((const char*)&file,sizeof(file)); out.write((const char*)&info.bmiHeader,sizeof(info.bmiHeader)); out.write((const char*)pixels,w*h*4);
        if(retained)retained->assign(static_cast<UINT*>(pixels),static_cast<UINT*>(pixels)+size_t(w)*h); }
    SelectObject(dc,old); DeleteObject(bitmap); DeleteDC(dc); ReleaseDC(nullptr,screen);
    return ok && out.good();
}

static LRESULT CALLBACK source_proc(HWND h,UINT message,WPARAM w,LPARAM l){
    if(message==WM_PAINT||message==WM_PRINTCLIENT){
        PAINTSTRUCT paint{};HDC dc=message==WM_PAINT?BeginPaint(h,&paint):reinterpret_cast<HDC>(w);
        RECT area{};GetClientRect(h,&area);auto brush=CreateSolidBrush(COLORREF(GetWindowLongPtrW(h,GWLP_USERDATA)));FillRect(dc,&area,brush);DeleteObject(brush);
        if(message==WM_PAINT)EndPaint(h,&paint);return 0;
    }return DefWindowProcW(h,message,w,l);
}
static void pump(){MSG message{};while(PeekMessageW(&message,nullptr,0,0,PM_REMOVE)){TranslateMessage(&message);DispatchMessageW(&message);}DwmFlush();}
static bool gpu_pixels(const genio::Immagine& image,std::vector<UINT>& pixels){
    const auto& gpu=gpu::condivisa();D3D11_TEXTURE2D_DESC description{};image.tex->GetDesc(&description);
    description.Usage=D3D11_USAGE_STAGING;description.BindFlags=description.MiscFlags=0;description.CPUAccessFlags=D3D11_CPU_ACCESS_READ;description.MipLevels=1;
    ID3D11Texture2D* staging=nullptr;if(FAILED(gpu.dev->CreateTexture2D(&description,nullptr,&staging)))return false;
    gpu.ctx->CopySubresourceRegion(staging,0,0,0,0,image.tex,0,nullptr);D3D11_MAPPED_SUBRESOURCE mapped{};
    bool ok=SUCCEEDED(gpu.ctx->Map(staging,0,D3D11_MAP_READ,0,&mapped));
    if(ok){pixels.resize(size_t(description.Width)*description.Height);for(UINT y=0;y<description.Height;y++)std::memcpy(pixels.data()+size_t(y)*description.Width,static_cast<const BYTE*>(mapped.pData)+size_t(y)*mapped.RowPitch,size_t(description.Width)*4);gpu.ctx->Unmap(staging,0);}
    staging->Release();return ok;
}
static int original_live_capture(genio::Motore& engine,const std::filesystem::path& dir,std::ofstream& report){
    const auto instance=GetModuleHandleW(nullptr);WNDCLASSW type{};type.hInstance=instance;type.lpfnWndProc=source_proc;type.lpszClassName=L"FluidWin.Baseline.OwnedCapture";
    if(!RegisterClassW(&type))return 20;
    HWND source=CreateWindowExW(WS_EX_TOOLWINDOW,type.lpszClassName,L"Synthetic upstream WGC fixture",WS_POPUP,80,80,480,320,nullptr,nullptr,instance,nullptr);
    if(!source)return 21;struct Window{HWND value;~Window(){DestroyWindow(value);}}window{source};
    SetWindowLongPtrW(source,GWLP_USERDATA,RGB(255,255,255));ShowWindow(source,SW_SHOWNOACTIVATE);UpdateWindow(source);pump();
    const RECT area{80,80,560,400},expanded{64,64,576,416},icon{340,600,388,648};
    std::vector<UINT> before,during;
    if(!screenshot(dir/"wgc-source-before.bmp",expanded,&before))return 22;
    genio::Immagine image;struct Image{genio::Immagine& value;~Image(){value.libera();}}release{image};
    if(!engine.cattura(source,area,image)){report<<"capture=BLOCKED original initial PrintWindow capture unavailable\n";return 0;}
    viva::Cattura capture;
    if(!capture.avvia(gpu::condivisa().dev,source)){report<<"capture=BLOCKED original viva::Cattura startup denied or unsupported\n";return 0;}
    const auto start=std::chrono::steady_clock::now();bool copied=false;
    while(std::chrono::steady_clock::now()-start<std::chrono::seconds(4)){
        pump();copied=capture.copia(gpu::condivisa().ctx,image.tex,0,0,RECT{0,0,image.w,image.h});
        if(copied)break;std::this_thread::sleep_for(std::chrono::milliseconds(8));
    }
    if(!copied)return 23;
    // Observe the complete *screen* perimeter while WGC is active, separately
    // from texture/shader output. Changes here are evidence, not a border fix.
    for(int i=0;i<8;i++){pump();std::this_thread::sleep_for(std::chrono::milliseconds(12));}
    if(!screenshot(dir/"wgc-source-active-white.bmp",expanded,&during))return 24;
    size_t ring=0,changed=0,chromatic=0;const int width=expanded.right-expanded.left;
    for(int y=0;y<expanded.bottom-expanded.top;y++)for(int x=0;x<width;x++){
        const int sx=x+expanded.left,sy=y+expanded.top;
        if(sx>=area.left+3&&sx<area.right-3&&sy>=area.top+3&&sy<area.bottom-3)continue;
        const auto i=size_t(y)*width+x;const UINT b=before[i],d=during[i];ring++;if((b&0xffffff)!=(d&0xffffff))changed++;
        const int r=(d>>16)&255,g=(d>>8)&255,blue=d&255;if(std::max({r,g,blue})-std::min({r,g,blue})>32)chromatic++;
    }
    report<<"capture_screen_ring_pixels="<<ring<<" changed="<<changed<<" chromatic="<<chromatic<<" border_conclusion=NOT_INFERRED\n";
    SetWindowLongPtrW(source,GWLP_USERDATA,RGB(0,0,255));InvalidateRect(source,nullptr,FALSE);UpdateWindow(source);
    std::vector<UINT> pixels;const auto repaint=std::chrono::steady_clock::now();bool blue=false;
    while(std::chrono::steady_clock::now()-repaint<std::chrono::seconds(4)){
        pump();if(capture.copia(gpu::condivisa().ctx,image.tex,0,0,RECT{0,0,image.w,image.h})&&gpu_pixels(image,pixels)){
            const auto middle=pixels[size_t(image.h/2)*image.w+image.w/2];if((middle&0xffffff)==0x0000ff){blue=true;break;}
        }std::this_thread::sleep_for(std::chrono::milliseconds(8));
    }
    if(!blue)return 25;gpu::condivisa().ctx->GenerateMips(image.srv);
    if(!screenshot(dir/"wgc-source-active-blue.bmp",expanded))return 26;
    capture.ferma();pump();if(!screenshot(dir/"wgc-source-stopped.bmp",expanded))return 27;
    ShowWindow(source,SW_HIDE);pump();
    const double progress[]={0,.05,.25,.5,.85,.97,1,.85,.5,.05,0};
    for(unsigned i=0;i<11;i++){
        genio::Elemento element{&image,area,icon,progress[i],1};engine.mostra(true);engine.disegna({element});DwmFlush();
        if(!screenshot(dir/("wgc-genie-"+std::to_string(i)+".bmp"),RECT{60,60,600,680}))return 28;
    }
    engine.svuota();engine.mostra(false);
    report<<"capture=ORIGINAL_VIVA_EXECUTED GPU_to_GPU=PASS live_white_to_blue=PASS source=480x320\n"
          <<"capture_screen_images=4 actual_captured_genie_poses=11 display_fps=NOT_MEASURED\n";
    std::cout<<"FLUIDWIN_BASELINE_CAPTURE original_viva=PASS GPU_to_GPU=PASS live_repaint=PASS screen_perimeter=EXPORTED owner_border=UNRESOLVED\n";
    return 0;
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
    report << "tint_screenshots=66 complete_canvas_including_perimeter\n"
           << "drop_jelly_guardian_input_hooks=NOT_EXERCISED\n";
    const auto capture=original_live_capture(engine,dir,report);if(capture)return capture;
    std::cout << "FLUIDWIN_BASELINE renderer=EXECUTED screenshots=66 owner_quality=NOT_VERIFIED\n";
    return engine.gpuPersa()?5:0;
}
