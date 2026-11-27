// New native presentation ownership. Standard WGL/D3D interop is an audited
// graphics algorithm, not a copied legacy renderer. One bridge per GL session.
#pragma once
#include "images.h"
#include <d3d11.h>
#include <dxgi1_2.h>
#include <dcomp.h>

namespace wallpaper {
inline void graphicsCheck(HRESULT result,const char*operation){if(FAILED(result))throw std::runtime_error(std::string(operation)+" HRESULT="+std::to_string(uint32_t(result)));}
class Bridge {
    using Open=HANDLE(WINAPI*)(void*);using Close=BOOL(WINAPI*)(HANDLE);
    using Register=HANDLE(WINAPI*)(HANDLE,void*,GLuint,GLenum,GLenum);using Remove=BOOL(WINAPI*)(HANDLE,HANDLE);
    using Access=BOOL(WINAPI*)(HANDLE,GLint,HANDLE*);
    template<class T>static T extension(const char*name){auto p=wglGetProcAddress(name);auto v=reinterpret_cast<intptr_t>(p);return !p||v==1||v==2||v==3||v==-1?nullptr:reinterpret_cast<T>(p);}
    Com<ID3D11Device> device;Com<ID3D11DeviceContext> context;Com<IDXGIFactory2> factory;Com<IDCompositionDevice> composition;
    HANDLE handle=nullptr;Close close=nullptr;Register registerObject=nullptr;Remove unregisterObject=nullptr;Access lockObjects=nullptr,unlockObjects=nullptr;
public:
    Bridge(){auto open=extension<Open>("wglDXOpenDeviceNV");close=extension<Close>("wglDXCloseDeviceNV");registerObject=extension<Register>("wglDXRegisterObjectNV");unregisterObject=extension<Remove>("wglDXUnregisterObjectNV");lockObjects=extension<Access>("wglDXLockObjectsNV");unlockObjects=extension<Access>("wglDXUnlockObjectsNV");
        if(!open||!close||!registerObject||!unregisterObject||!lockObjects||!unlockObjects)throw std::runtime_error("GPU interop unsupported; bounded DIB fallback");
        graphicsCheck(CreateDXGIFactory1(IID_PPV_ARGS(factory.put())),"DXGI factory");
        for(UINT i=0;i<16&&!handle;i++){Com<IDXGIAdapter1> adapter;if(factory->EnumAdapters1(i,adapter.put())!=S_OK)break;DXGI_ADAPTER_DESC1 desc{};if(FAILED(adapter->GetDesc1(&desc))||(desc.Flags&DXGI_ADAPTER_FLAG_SOFTWARE))continue;
            Com<ID3D11Device> candidate;Com<ID3D11DeviceContext> commands;
            if(FAILED(D3D11CreateDevice(adapter.get(),D3D_DRIVER_TYPE_UNKNOWN,nullptr,D3D11_CREATE_DEVICE_BGRA_SUPPORT,nullptr,0,D3D11_SDK_VERSION,candidate.put(),nullptr,commands.put())))continue;
            HANDLE shared=open(candidate.get());if(!shared)continue;device=std::move(candidate);context=std::move(commands);handle=shared;
        }
        if(!handle)throw std::runtime_error("No GL-compatible hardware adapter; bounded DIB fallback");
        try{Com<IDXGIDevice> dxgi;graphicsCheck(device->QueryInterface(IID_PPV_ARGS(dxgi.put())),"DXGI device");graphicsCheck(DCompositionCreateDevice(dxgi.get(),IID_PPV_ARGS(composition.put())),"Wallpaper composition");}
        catch(...){close(handle);handle=nullptr;throw;}
    }
    ~Bridge(){if(handle)close(handle);}Bridge(const Bridge&)=delete;
    friend class Presentation;
};
class Presentation {
    GL&g;Bridge*bridge;HWND hwnd;int w,h,physicalW,physicalH;GLuint framebuffer=0,renderbuffer=0;
    Com<ID3D11Texture2D> shared;Com<IDXGISwapChain1> swapchain;Com<IDCompositionTarget> target;Com<IDCompositionVisual> visual;Com<IDCompositionScaleTransform> scale;
    HANDLE object=nullptr;bool locked=false;Texture fallback;
    struct Readback {GLuint buffer=0;void*fence=nullptr;};std::array<Readback,3> queue{};size_t next=0;
    std::vector<unsigned char> pixels;bool cached=false;
    void unlock(){if(locked){if(!bridge->unlockObjects(bridge->handle,1,&object))throw std::runtime_error("GPU interop unlock failed");locked=false;}}
    void clear()noexcept{
        if(target.get())target->SetRoot(nullptr);if(bridge&&bridge->composition.get())bridge->composition->Commit();
        if(locked&&bridge){bridge->unlockObjects(bridge->handle,1,&object);locked=false;}
        if(object&&bridge){bridge->unregisterObject(bridge->handle,object);object=nullptr;}
        for(auto&r:queue){if(r.fence)g.DeleteSync(r.fence);if(r.buffer)g.DeleteBuffers(1,&r.buffer);r={};}
        if(framebuffer)g.DeleteFramebuffers(1,&framebuffer);if(renderbuffer)g.DeleteRenderbuffers(1,&renderbuffer);framebuffer=renderbuffer=0;
    }
public:
    Presentation(GL&gl,Bridge*gpu,HWND window,int width,int height,int pw,int ph):g(gl),bridge(gpu),hwnd(window),w(width),h(height),physicalW(pw),physicalH(ph){
        try{g.GenFramebuffers(1,&framebuffer);if(bridge){
            D3D11_TEXTURE2D_DESC description{};description.Width=w;description.Height=h;description.MipLevels=1;description.ArraySize=1;description.Format=DXGI_FORMAT_B8G8R8A8_UNORM;description.SampleDesc.Count=1;description.Usage=D3D11_USAGE_DEFAULT;description.BindFlags=D3D11_BIND_RENDER_TARGET|D3D11_BIND_SHADER_RESOURCE;
            graphicsCheck(bridge->device->CreateTexture2D(&description,nullptr,shared.put()),"Shared wallpaper surface");
            DXGI_SWAP_CHAIN_DESC1 chain{};chain.Width=w;chain.Height=h;chain.Format=description.Format;chain.SampleDesc.Count=1;chain.BufferUsage=DXGI_USAGE_RENDER_TARGET_OUTPUT;chain.BufferCount=2;chain.Scaling=DXGI_SCALING_STRETCH;chain.SwapEffect=DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL;chain.AlphaMode=DXGI_ALPHA_MODE_IGNORE;
            graphicsCheck(bridge->factory->CreateSwapChainForComposition(bridge->device.get(),&chain,nullptr,swapchain.put()),"Wallpaper swap chain");
            graphicsCheck(bridge->composition->CreateTargetForHwnd(hwnd,TRUE,target.put()),"Wallpaper composition target");graphicsCheck(bridge->composition->CreateVisual(visual.put()),"Wallpaper visual");graphicsCheck(bridge->composition->CreateScaleTransform(scale.put()),"Wallpaper scale");
            graphicsCheck(scale->SetScaleX(float(pw)/w),"Wallpaper scale X");graphicsCheck(scale->SetScaleY(float(ph)/h),"Wallpaper scale Y");graphicsCheck(visual->SetTransform(scale.get()),"Wallpaper scale transform");graphicsCheck(visual->SetContent(swapchain.get()),"Wallpaper content");graphicsCheck(target->SetRoot(visual.get()),"Wallpaper root");graphicsCheck(bridge->composition->Commit(),"Wallpaper composition commit");
            g.GenRenderbuffers(1,&renderbuffer);object=bridge->registerObject(bridge->handle,shared.get(),renderbuffer,0x8D41,0x0002);if(!object)throw std::runtime_error("Shared wallpaper registration failed");
            if(!bridge->lockObjects(bridge->handle,1,&object))throw std::runtime_error("Shared wallpaper lock failed");locked=true;g.BindFramebuffer(0x8D40,framebuffer);g.FramebufferRenderbuffer(0x8D40,0x8CE0,0x8D41,renderbuffer);
        }else{
            fallback.image(g,4,w,h,0x8058,GL_RGBA,GL_UNSIGNED_BYTE,nullptr);g.BindFramebuffer(0x8D40,framebuffer);g.FramebufferTexture2D(0x8D40,0x8CE0,GL_TEXTURE_2D,fallback.name(),0);
            for(auto&r:queue){g.GenBuffers(1,&r.buffer);g.BindBuffer(0x88EB,r.buffer);g.BufferData(0x88EB,ptrdiff_t(w)*h*4,nullptr,0x88E1);}g.BindBuffer(0x88EB,0);pixels.resize(size_t(w)*h*4);
        }
        if(g.CheckFramebufferStatus(0x8D40)!=0x8CD5)throw std::runtime_error("Presentation surface incomplete");g.BindFramebuffer(0x8D40,0);unlock();
        }catch(...){clear();throw;}
    }
    ~Presentation(){clear();}Presentation(const Presentation&)=delete;
    bool gpu()const{return bridge!=nullptr;}
    GLuint begin(){if(bridge){if(locked||!bridge->lockObjects(bridge->handle,1,&object))throw std::runtime_error("Wallpaper device lost during lock");locked=true;}return framebuffer;}
    bool present(){g.BindFramebuffer(0x8D40,0);
        if(bridge){unlock();Com<ID3D11Texture2D> back;graphicsCheck(swapchain->GetBuffer(0,IID_PPV_ARGS(back.put())),"Wallpaper back buffer");bridge->context->CopyResource(back.get(),shared.get());graphicsCheck(swapchain->Present(0,0),"Wallpaper Present");return true;}
        auto&slot=queue[next];
        if(slot.fence){GLenum ready=g.ClientWaitSync(slot.fence,0,0);if(ready!=0x911A&&ready!=0x911C)return false;g.DeleteSync(slot.fence);slot.fence=nullptr;g.BindBuffer(0x88EB,slot.buffer);g.GetBufferSubData(0x88EB,0,ptrdiff_t(pixels.size()),pixels.data());g.BindBuffer(0x88EB,0);cached=true;}
        g.BindFramebuffer(0x8D40,framebuffer);g.BindBuffer(0x88EB,slot.buffer);glReadPixels(0,0,w,h,0x80E1,GL_UNSIGNED_BYTE,nullptr);slot.fence=g.FenceSync(0x9117,0);glFlush();g.BindBuffer(0x88EB,0);g.BindFramebuffer(0x8D40,0);next=(next+1)%queue.size();
        if(cached){HDC dc=GetDC(hwnd);BITMAPINFO info{};info.bmiHeader.biSize=sizeof(BITMAPINFOHEADER);info.bmiHeader.biWidth=w;info.bmiHeader.biHeight=h;info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;SetStretchBltMode(dc,HALFTONE);SetBrushOrgEx(dc,0,0,nullptr);int result=StretchDIBits(dc,0,0,physicalW,physicalH,0,0,w,h,pixels.data(),&info,DIB_RGB_COLORS,SRCCOPY);ReleaseDC(hwnd,dc);if(result==GDI_ERROR)throw std::runtime_error("Wallpaper DIB fallback failed");}
        return cached;
    }
};
}
