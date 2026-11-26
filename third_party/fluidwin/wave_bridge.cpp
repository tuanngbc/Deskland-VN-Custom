// Original onda.h shader/constants, scoped to TLauncher's shared GPU/UI owner.
// Onda's permanent thread/device implementation is never instantiated.
#include "upstream/onda.h"
#include <dxgi1_3.h>
#include <wrl/client.h>
#include <memory>
#include <cstdint>
namespace {
using Microsoft::WRL::ComPtr;
struct Wave {
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> deferred;
    ComPtr<IDXGISwapChain1> chain;
    ComPtr<ID3D11RenderTargetView> target;
    ComPtr<IDCompositionDevice> composition;
    ComPtr<IDCompositionTarget> window;
    ComPtr<IDCompositionVisual> visual;
    ComPtr<ID3D11VertexShader> vs;
    ComPtr<ID3D11PixelShader> ps;
    ComPtr<ID3D11Buffer> constants;
    ComPtr<ID3D11SamplerState> sampler;
    ComPtr<ID3D11RasterizerState> raster;
    ComPtr<ID3D11ShaderResourceView> photograph;
    ComPtr<ID3D11Texture2D> evidence;
    UINT width=0,height=0;HANDLE ready=nullptr;
    ~Wave(){if(visual)visual->SetContent(nullptr);if(window)window->SetRoot(nullptr);if(composition)composition->Commit();if(ready)CloseHandle(ready);}
    bool init(ID3D11Device* d,HWND hwnd,UINT w,UINT h,const UINT* pixels){
        device=d;width=w;height=h;
        if(!device||!IsWindow(hwnd)||!pixels||!w||!h||w>16384||h>16384||uint64_t(w)*h>33554432)return false;
        if(FAILED(device->CreateDeferredContext(0,&deferred)))return false;
        ComPtr<ID3DBlob> vertex,pixel;
        if(FAILED(D3DCompile(onda::SHADER,strlen(onda::SHADER),nullptr,nullptr,nullptr,"vs","vs_4_0",D3DCOMPILE_OPTIMIZATION_LEVEL3,0,&vertex,nullptr))||
           FAILED(D3DCompile(onda::SHADER,strlen(onda::SHADER),nullptr,nullptr,nullptr,"ps","ps_4_0",D3DCOMPILE_OPTIMIZATION_LEVEL3,0,&pixel,nullptr)))return false;
        if(FAILED(device->CreateVertexShader(vertex->GetBufferPointer(),vertex->GetBufferSize(),nullptr,&vs))||
           FAILED(device->CreatePixelShader(pixel->GetBufferPointer(),pixel->GetBufferSize(),nullptr,&ps)))return false;
        static_assert(sizeof(onda::Costanti)==48);
        D3D11_BUFFER_DESC cb{};cb.ByteWidth=sizeof(onda::Costanti);cb.Usage=D3D11_USAGE_DEFAULT;cb.BindFlags=D3D11_BIND_CONSTANT_BUFFER;
        if(FAILED(device->CreateBuffer(&cb,nullptr,&constants)))return false;
        D3D11_TEXTURE2D_DESC td{};td.Width=w;td.Height=h;td.MipLevels=td.ArraySize=td.SampleDesc.Count=1;
        td.Format=DXGI_FORMAT_B8G8R8A8_UNORM;td.Usage=D3D11_USAGE_IMMUTABLE;td.BindFlags=D3D11_BIND_SHADER_RESOURCE;
        const D3D11_SUBRESOURCE_DATA data{pixels,w*4,0};ComPtr<ID3D11Texture2D> texture;
        if(FAILED(device->CreateTexture2D(&td,&data,&texture))||FAILED(device->CreateShaderResourceView(texture.Get(),nullptr,&photograph)))return false;
        D3D11_SAMPLER_DESC sm{};sm.Filter=D3D11_FILTER_MIN_MAG_MIP_LINEAR;sm.AddressU=sm.AddressV=sm.AddressW=D3D11_TEXTURE_ADDRESS_CLAMP;sm.MaxLOD=D3D11_FLOAT32_MAX;
        D3D11_RASTERIZER_DESC rs{};rs.FillMode=D3D11_FILL_SOLID;rs.CullMode=D3D11_CULL_NONE;rs.DepthClipEnable=TRUE;
        if(FAILED(device->CreateSamplerState(&sm,&sampler))||FAILED(device->CreateRasterizerState(&rs,&raster)))return false;
        ComPtr<IDXGIDevice> dxgi;ComPtr<IDXGIAdapter> adapter;ComPtr<IDXGIFactory2> factory;
        if(FAILED(device.As(&dxgi))||FAILED(dxgi->GetAdapter(&adapter))||FAILED(adapter->GetParent(IID_PPV_ARGS(&factory))))return false;
        DXGI_SWAP_CHAIN_DESC1 sc{};sc.Width=w;sc.Height=h;sc.Format=DXGI_FORMAT_B8G8R8A8_UNORM;sc.SampleDesc.Count=1;
        sc.BufferUsage=DXGI_USAGE_RENDER_TARGET_OUTPUT;sc.BufferCount=2;sc.SwapEffect=DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL;
        sc.AlphaMode=DXGI_ALPHA_MODE_PREMULTIPLIED;sc.Flags=DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT;
        if(FAILED(factory->CreateSwapChainForComposition(device.Get(),&sc,nullptr,&chain)))return false;
        ComPtr<IDXGISwapChain2> queue;if(FAILED(chain.As(&queue))||FAILED(queue->SetMaximumFrameLatency(1)))return false;
        ready=queue->GetFrameLatencyWaitableObject();if(!ready)return false;
        ComPtr<ID3D11Texture2D> back;if(FAILED(chain->GetBuffer(0,IID_PPV_ARGS(&back)))||FAILED(device->CreateRenderTargetView(back.Get(),nullptr,&target)))return false;
        if(FAILED(DCompositionCreateDevice(dxgi.Get(),IID_PPV_ARGS(&composition)))||FAILED(composition->CreateTargetForHwnd(hwnd,TRUE,&window))||
           FAILED(composition->CreateVisual(&visual))||FAILED(visual->SetContent(chain.Get()))||FAILED(window->SetRoot(visual.Get()))||FAILED(composition->Commit()))return false;
        return true;
    }
    bool draw(ID3D11DeviceContext* immediate,const onda::Costanti& k){
        // Deferred commands avoid changing the shared immediate-context state.
        // The caller holds ShellGpu::context for ExecuteCommandList only.
        if(!immediate||!std::isfinite(k.t)||!std::isfinite(k.spegni)||!std::isfinite(k.dpi)||k.dpi<=0)return false;
        deferred->ClearState();deferred->UpdateSubresource(constants.Get(),0,nullptr,&k,0,0);
        ID3D11RenderTargetView* rt=target.Get();deferred->OMSetRenderTargets(1,&rt,nullptr);
        D3D11_VIEWPORT viewport{0,0,float(width),float(height),0,1};deferred->RSSetViewports(1,&viewport);deferred->RSSetState(raster.Get());
        deferred->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);deferred->VSSetShader(vs.Get(),nullptr,0);deferred->PSSetShader(ps.Get(),nullptr,0);
        auto* cb=constants.Get();deferred->PSSetConstantBuffers(0,1,&cb);auto* s=sampler.Get();deferred->PSSetSamplers(0,1,&s);
        auto* image=k.foto>0.5f?photograph.Get():nullptr;deferred->PSSetShaderResources(0,1,&image);deferred->Draw(3,0);
        deferred->OMSetRenderTargets(0,nullptr,nullptr);image=nullptr;deferred->PSSetShaderResources(0,1,&image);
        if(evidence){ComPtr<ID3D11Resource> source;target->GetResource(&source);deferred->CopyResource(evidence.Get(),source.Get());}
        ComPtr<ID3D11CommandList> commands;if(FAILED(deferred->FinishCommandList(FALSE,&commands)))return false;
        immediate->ExecuteCommandList(commands.Get(),TRUE);return true;
    }
};
}
extern "C" {
void* tlauncher_wave_create(ID3D11Device* device,HWND hwnd,UINT w,UINT h,const UINT* pixels) try{
    auto wave=std::make_unique<Wave>();return wave->init(device,hwnd,w,h,pixels)?wave.release():nullptr;
}catch(...){return nullptr;}
void tlauncher_wave_free(void* raw){delete static_cast<Wave*>(raw);}
HANDLE tlauncher_wave_ready(void* raw){return raw?static_cast<Wave*>(raw)->ready:nullptr;}
int tlauncher_wave_draw(void* raw,ID3D11DeviceContext* ctx,float time,float x,float y,float dpi,float fade,bool photo){
    if(!raw)return 0;auto& wave=*static_cast<Wave*>(raw);
    const onda::Costanti k{{float(wave.width),float(wave.height)},time,1,{x,y},fade,dpi,photo?1.0f:0.0f,{0,0,0}};
    return wave.draw(ctx,k)?1:0;
}
int tlauncher_wave_present(void* raw,bool settled){
    if(!raw)return 0;UINT interval=1;
    if(settled){DWM_TIMING_INFO timing{};timing.cbSize=sizeof(timing);
        if(SUCCEEDED(DwmGetCompositionTimingInfo(nullptr,&timing))&&timing.rateRefresh.uiDenominator)
            interval=std::clamp(int(std::lround(double(timing.rateRefresh.uiNumerator)/timing.rateRefresh.uiDenominator/60.0)),1,4);
    }
    const HRESULT hr=static_cast<Wave*>(raw)->chain->Present(interval,DXGI_PRESENT_DO_NOT_WAIT);
    return hr==S_OK?1:hr==DXGI_ERROR_WAS_STILL_DRAWING?2:int(hr);
}
void tlauncher_wave_release_photo(void* raw){if(raw)static_cast<Wave*>(raw)->photograph.Reset();}
bool tlauncher_wave_evidence(void* raw){
    if(!raw)return false;auto& wave=*static_cast<Wave*>(raw);ComPtr<ID3D11Resource> resource;wave.target->GetResource(&resource);
    ComPtr<ID3D11Texture2D> texture;if(FAILED(resource.As(&texture)))return false;D3D11_TEXTURE2D_DESC d{};texture->GetDesc(&d);
    d.Usage=D3D11_USAGE_STAGING;d.BindFlags=d.MiscFlags=0;d.CPUAccessFlags=D3D11_CPU_ACCESS_READ;
    return SUCCEEDED(wave.device->CreateTexture2D(&d,nullptr,&wave.evidence));
}
bool tlauncher_wave_pixels(void* raw,ID3D11DeviceContext* ctx,void* pixels,UINT stride){
    if(!raw||!ctx||!pixels)return false;auto& wave=*static_cast<Wave*>(raw);if(!wave.evidence||stride<wave.width*4)return false;
    D3D11_MAPPED_SUBRESOURCE map{};if(FAILED(ctx->Map(wave.evidence.Get(),0,D3D11_MAP_READ,0,&map)))return false;
    for(UINT y=0;y<wave.height;y++)memcpy(static_cast<char*>(pixels)+size_t(y)*stride,static_cast<char*>(map.pData)+size_t(y)*map.RowPitch,wave.width*4);
    ctx->Unmap(wave.evidence.Get(),0);return true;
}
}
