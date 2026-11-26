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
#include <utility>
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
template<class T>struct Interface{T* p=nullptr;~Interface(){if(p)p->Release();}};
static bool image_file(const std::filesystem::path& path,const std::vector<UINT>& pixels,int w,int h){
    if(pixels.size()!=size_t(w)*h)return false;BITMAPFILEHEADER file{};file.bfType=0x4d42;
    BITMAPINFOHEADER header{};header.biSize=sizeof(header);header.biWidth=w;header.biHeight=-h;header.biPlanes=1;header.biBitCount=32;
    file.bfOffBits=sizeof(file)+sizeof(header);file.bfSize=file.bfOffBits+DWORD(pixels.size()*4);
    std::ofstream out(path,std::ios::binary);out.write(reinterpret_cast<const char*>(&file),sizeof(file));out.write(reinterpret_cast<const char*>(&header),sizeof(header));out.write(reinterpret_cast<const char*>(pixels.data()),pixels.size()*4);return out.good();
}
static int original_static_capture(genio::Motore& engine,const std::filesystem::path& dir,std::ofstream& report){
    const auto instance=GetModuleHandleW(nullptr);WNDCLASSW type{};type.hInstance=instance;type.lpfnWndProc=source_proc;type.lpszClassName=L"FluidWin.Baseline.StaticCapture";
    if(!RegisterClassW(&type))return 30;
    HWND source=CreateWindowExW(0,type.lpszClassName,L"Synthetic original static capture",WS_POPUP,80,80,480,320,nullptr,nullptr,instance,nullptr);
    if(!source)return 31;struct Window{HWND h;~Window(){DestroyWindow(h);}}window{source};
    ShowWindow(source,SW_SHOWNOACTIVATE);
    const RECT area{80,80,560,400},expanded{64,64,576,416},icon{340,600,388,648};
    const COLORREF colors[]={RGB(255,255,255),RGB(255,0,0),RGB(0,255,0),RGB(0,0,255),RGB(0,0,0)};
    const char* names[]={"white","red","green","blue","black"};
    const double progress[]={0,.05,.25,.5,.85,.97,1,.85,.5,.05,0};
    for(unsigned c=0;c<5;c++){
        SetWindowLongPtrW(source,GWLP_USERDATA,colors[c]);InvalidateRect(source,nullptr,FALSE);UpdateWindow(source);pump();
        if(!screenshot(dir/(std::string("static-source-")+names[c]+".bmp"),expanded))return 32;
        genio::Immagine image;engine.fotoDaSchermo=false;
        const auto start=std::chrono::steady_clock::now();const bool accepted=engine.cattura(source,area,image,false);
        report<<"static_capture="<<names[c]<<" accepted="<<accepted<<" source_backend="<<(engine.fotoDaSchermo?"BitBlt-fallback":"PrintWindow")
              <<" capture_us="<<std::chrono::duration_cast<std::chrono::microseconds>(std::chrono::steady_clock::now()-start).count()<<" WGC_session=NOT_STARTED\n";
        if(!accepted){image.libera();if(c==4){report<<"static_black=ORIGINAL_CARICA_REJECTS_NEAR_BLACK\n";continue;}return 33;}
        std::vector<UINT> pixels;if(!gpu_pixels(image,pixels)){image.libera();return 34;}
        const UINT expected=(c==0?0x00ffffff:c==1?0x00ff0000:c==2?0x0000ff00:c==3?0x000000ff:0);
        for(int y=0;y<image.h;y++)for(int x=0;x<image.w;x++){
            const UINT pixel=pixels[size_t(y)*image.w+x];if(pixel!=0xff000000u+expected){image.libera();return 35;}
        }
        if(!image_file(dir/(std::string("static-texture-")+names[c]+".bmp"),pixels,image.w,image.h)){image.libera();return 36;}
        for(unsigned f=0;f<11;f++){
            genio::Elemento element{&image,area,icon,progress[f],1};engine.mostra(true);engine.disegna({element});DwmFlush();
            if(!screenshot(dir/(std::string("static-genie-")+names[c]+"-"+std::to_string(f)+".bmp"),RECT{60,60,600,680})){image.libera();return 37;}
        }
        engine.svuota();engine.mostra(false);image.libera();
    }
    SetWindowPos(source,nullptr,20,20,2110,980,SWP_NOACTIVATE|SWP_NOZORDER);SetWindowLongPtrW(source,GWLP_USERDATA,RGB(255,255,255));InvalidateRect(source,nullptr,FALSE);UpdateWindow(source);pump();
    RECT large{};GetWindowRect(source,&large);genio::Immagine image;
    if(!engine.cattura(source,large,image,false)){report<<"static_large=BLOCKED original snapshot unavailable\n";return 0;}
    std::vector<UINT> pixels;const bool largeOK=image.w==2110&&image.h==980&&gpu_pixels(image,pixels)&&image_file(dir/"static-large-native-texture.bmp",pixels,image.w,image.h);image.libera();if(!largeOK)return 38;
    report<<"static_capture=ORIGINAL_GENIO_CATTURA_EXECUTED source=PrintWindow_or_documented_screen_fallback texture_perimeter=VERIFIED large_native=2110x980 WGC_session=NOT_STARTED CPU_snapshot_GPU_upload=YES\n";
    std::cout<<"FLUIDWIN_BASELINE_STATIC original_genio_cattura=PASS texture_perimeters=PASS large_native=2110x980 WGC_session=NOT_STARTED owner_border=UNRESOLVED\n";return 0;
}
static void diagnose_capture(HWND source,std::ofstream& report){
    // Run the same documented API sequence only to identify a startup blocker;
    // no upstream implementation or permission/policy setting is patched.
    auto check=[&](const char* name,HRESULT result){report<<"capture_startup_step="<<name<<" hr=0x"<<std::hex<<static_cast<unsigned long>(result)<<std::dec<<"\n";return SUCCEEDED(result);};
    Interface<viva::WGC::IGraphicsCaptureSessionStatics> support;
    if(!check("session_factory",viva::fabbrica(L"Windows.Graphics.Capture.GraphicsCaptureSession",__uuidof(viva::WGC::IGraphicsCaptureSessionStatics),reinterpret_cast<void**>(&support.p))))return;
    boolean available=false;if(!check("IsSupported",support.p->IsSupported(&available)))return;report<<"capture_supported="<<int(available)<<"\n";if(!available)return;
    Interface<IDXGIDevice> dxgi;if(!check("DXGI_device",gpu::condivisa().dev->QueryInterface(__uuidof(IDXGIDevice),reinterpret_cast<void**>(&dxgi.p))))return;
    Interface<IInspectable> inspect;if(!check("CreateDirect3D11DeviceFromDXGIDevice",CreateDirect3D11DeviceFromDXGIDevice(dxgi.p,&inspect.p)))return;
    Interface<viva::WD3D::IDirect3DDevice> device;if(!check("IDirect3DDevice",inspect.p->QueryInterface(__uuidof(viva::WD3D::IDirect3DDevice),reinterpret_cast<void**>(&device.p))))return;
    Interface<IGraphicsCaptureItemInterop> interop;if(!check("item_factory",viva::fabbrica(L"Windows.Graphics.Capture.GraphicsCaptureItem",__uuidof(IGraphicsCaptureItemInterop),reinterpret_cast<void**>(&interop.p))))return;
    Interface<viva::WGC::IGraphicsCaptureItem> item;if(!check("CreateForWindow",interop.p->CreateForWindow(source,__uuidof(viva::WGC::IGraphicsCaptureItem),reinterpret_cast<void**>(&item.p))))return;
    ABI::Windows::Graphics::SizeInt32 size{};if(!check("item_Size",item.p->get_Size(&size)))return;
    Interface<viva::WGC::IDirect3D11CaptureFramePoolStatics2> factory;
    if(!check("pool_factory",viva::fabbrica(L"Windows.Graphics.Capture.Direct3D11CaptureFramePool",__uuidof(viva::WGC::IDirect3D11CaptureFramePoolStatics2),reinterpret_cast<void**>(&factory.p))))return;
    Interface<viva::WGC::IDirect3D11CaptureFramePool> pool;
    if(!check("CreateFreeThreaded",factory.p->CreateFreeThreaded(device.p,ABI::Windows::Graphics::DirectX::DirectXPixelFormat_B8G8R8A8UIntNormalized,2,size,&pool.p)))return;
    struct ClosePool{IUnknown* p;~ClosePool(){viva::chiudi(p);}}closePool{pool.p};
    Interface<viva::WGC::IGraphicsCaptureSession> session;if(!check("CreateCaptureSession",pool.p->CreateCaptureSession(item.p,&session.p)))return;
    struct CloseSession{IUnknown* p;~CloseSession(){viva::chiudi(p);}}closeSession{session.p};
    Interface<viva::WGC::IGraphicsCaptureSession2> cursor;if(check("cursor_interface",session.p->QueryInterface(__uuidof(viva::WGC::IGraphicsCaptureSession2),reinterpret_cast<void**>(&cursor.p))))check("cursor_off",cursor.p->put_IsCursorCaptureEnabled(false));
    Interface<viva::WGC::IGraphicsCaptureSession3> border;if(check("border_interface",session.p->QueryInterface(__uuidof(viva::WGC::IGraphicsCaptureSession3),reinterpret_cast<void**>(&border.p))))check("border_off",border.p->put_IsBorderRequired(false));
    if(!check("StartCapture",session.p->StartCapture()))return;
    for(const auto& pair:{std::pair<const char*,IUnknown*>{"pool_agile",pool.p},{"session_agile",session.p},{"device_agile",device.p}}){
        Interface<IAgileObject> agile;if(!check(pair.first,pair.second->QueryInterface(__uuidof(IAgileObject),reinterpret_cast<void**>(&agile.p))))return;
    }
}
static int original_live_capture(genio::Motore& engine,const std::filesystem::path& dir,std::ofstream& report){
    const auto instance=GetModuleHandleW(nullptr);WNDCLASSW type{};type.hInstance=instance;type.lpfnWndProc=source_proc;type.lpszClassName=L"FluidWin.Baseline.OwnedCapture";
    if(!RegisterClassW(&type))return 20;
    HWND source=CreateWindowExW(0,type.lpszClassName,L"Synthetic upstream WGC fixture",WS_POPUP,80,80,480,320,nullptr,nullptr,instance,nullptr);
    if(!source)return 21;struct Window{HWND value;~Window(){DestroyWindow(value);}}window{source};
    SetWindowLongPtrW(source,GWLP_USERDATA,RGB(255,255,255));ShowWindow(source,SW_SHOWNOACTIVATE);UpdateWindow(source);pump();
    report<<"capture_source_valid="<<IsWindow(source)<<" visible="<<IsWindowVisible(source)<<" root="<<(GetAncestor(source,GA_ROOT)==source)
          <<" style=0x"<<std::hex<<GetWindowLongPtrW(source,GWL_STYLE)<<" exstyle=0x"<<GetWindowLongPtrW(source,GWL_EXSTYLE)<<std::dec<<"\n";
    const RECT area{80,80,560,400},expanded{64,64,576,416},icon{340,600,388,648};
    std::vector<UINT> before,during;
    if(!screenshot(dir/"wgc-source-before.bmp",expanded,&before))return 22;
    genio::Immagine image;struct Image{genio::Immagine& value;~Image(){value.libera();}}release{image};
    if(!engine.cattura(source,area,image)){report<<"capture=BLOCKED original initial PrintWindow capture unavailable\n";return 0;}
    viva::Cattura capture;
    if(!capture.avvia(gpu::condivisa().dev,source)){report<<"capture=BLOCKED original viva::Cattura startup denied or unsupported\n";diagnose_capture(source,report);return 0;}
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
    const auto apartment=RoInitialize(RO_INIT_MULTITHREADED);
    report << "reference=2f773a8733b84c7dbee88fff6628d14e9ac802b9\n"
           << "original_application=COMPILED_NOT_STARTED\nowner_quality=NOT_VERIFIED\n";
    report<<"RoInitialize_hr=0x"<<std::hex<<static_cast<unsigned long>(apartment)<<std::dec<<"\n";
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
    const auto photo=original_static_capture(engine,dir,report);if(photo)return photo;
    const auto capture=original_live_capture(engine,dir,report);if(capture)return capture;
    std::cout << "FLUIDWIN_BASELINE renderer=EXECUTED screenshots=66 owner_quality=NOT_VERIFIED\n";
    return engine.gpuPersa()?5:0;
}
