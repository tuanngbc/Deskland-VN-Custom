// TLauncher D3D11 adapter for FluidWin's MIT-licensed grid and shader.
#include "motion_upstream.h"
#include <d3d11.h>
#include <dxgi1_2.h>
#include <d3dcompiler.h>
#include <wrl/client.h>
#include <new>
#include <cstring>
#include <cstdint>

namespace {
using Microsoft::WRL::ComPtr;
using namespace fluidwin_motion;

struct alignas(16) Constants {
    float origin[2], size[2], alpha, radius, texture[2];
    float shadow, lift, radiusY, pad, extent[2], feather[2];
};
static_assert(sizeof(Constants) == 64);

struct Pipeline {
    ComPtr<ID3D11DeviceContext> context;
    ComPtr<ID3D11Device> device;
    ComPtr<IDXGIFactory2> factory;
    ComPtr<ID3D11VertexShader> vs;
    ComPtr<ID3D11PixelShader> ps;
    ComPtr<ID3D11InputLayout> layout;
    ComPtr<ID3D11Buffer> vb, cb;
    ComPtr<ID3D11SamplerState> sampler;
    ComPtr<ID3D11RasterizerState> raster;

    bool init(ID3D11DeviceContext* ctx) {
        context = ctx;
        ctx->GetDevice(&device);
        ComPtr<IDXGIDevice> dxgi;
        ComPtr<IDXGIAdapter> adapter;
        if (FAILED(device.As(&dxgi)) || FAILED(dxgi->GetAdapter(&adapter)) ||
            FAILED(adapter->GetParent(IID_PPV_ARGS(&factory)))) return false;

        ComPtr<ID3DBlob> v, p;
        if (FAILED(D3DCompile(SHADER, std::strlen(SHADER), nullptr, nullptr, nullptr,
                              "vs", "vs_4_0", D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &v, nullptr)) ||
            FAILED(D3DCompile(SHADER, std::strlen(SHADER), nullptr, nullptr, nullptr,
                              "ps", "ps_4_0", D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &p, nullptr))) return false;
        const D3D11_INPUT_ELEMENT_DESC inputs[] = {
            {"POSITION", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 0, D3D11_INPUT_PER_VERTEX_DATA, 0},
            {"TEXCOORD", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 8, D3D11_INPUT_PER_VERTEX_DATA, 0},
        };
        if (FAILED(device->CreateVertexShader(v->GetBufferPointer(), v->GetBufferSize(), nullptr, &vs)) ||
            FAILED(device->CreatePixelShader(p->GetBufferPointer(), p->GetBufferSize(), nullptr, &ps)) ||
            FAILED(device->CreateInputLayout(inputs, 2, v->GetBufferPointer(), v->GetBufferSize(), &layout))) return false;
        D3D11_BUFFER_DESC buf{};
        buf.ByteWidth = sizeof(Vertice) * RIGHE * 2;
        buf.Usage = D3D11_USAGE_DYNAMIC;
        buf.BindFlags = D3D11_BIND_VERTEX_BUFFER;
        buf.CPUAccessFlags = D3D11_CPU_ACCESS_WRITE;
        if (FAILED(device->CreateBuffer(&buf, nullptr, &vb))) return false;
        buf.ByteWidth = sizeof(Constants);
        buf.Usage = D3D11_USAGE_DEFAULT;
        buf.BindFlags = D3D11_BIND_CONSTANT_BUFFER;
        buf.CPUAccessFlags = 0;
        if (FAILED(device->CreateBuffer(&buf, nullptr, &cb))) return false;
        D3D11_SAMPLER_DESC s{};
        s.Filter = D3D11_FILTER_MIN_MAG_MIP_LINEAR;
        s.AddressU = s.AddressV = s.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
        s.ComparisonFunc = D3D11_COMPARISON_NEVER;
        s.MaxLOD = D3D11_FLOAT32_MAX;
        D3D11_RASTERIZER_DESC r{};
        r.FillMode = D3D11_FILL_SOLID;
        r.CullMode = D3D11_CULL_NONE;
        r.DepthClipEnable = TRUE;
        return SUCCEEDED(device->CreateSamplerState(&s, &sampler)) &&
               SUCCEEDED(device->CreateRasterizerState(&r, &raster));
    }
};

struct Layer {
    ComPtr<IDXGISwapChain1> chain;
    ComPtr<ID3D11RenderTargetView> target;
    ComPtr<ID3D11ShaderResourceView> texture;
    UINT w = 0, h = 0, tw = 0, th = 0;

    bool init(Pipeline& q, UINT width, UINT height, const void* bgra, UINT sourceW, UINT sourceH) {
        if (!bgra || !width || !height || width > 8192 || height > 8192 ||
            uint64_t(width) * height > 8400000 || !sourceW || !sourceH || sourceW > 768 || sourceH > 768)
            return false;
        w = width; h = height; tw = sourceW; th = sourceH;
        DXGI_SWAP_CHAIN_DESC1 sc{};
        sc.Width = w; sc.Height = h;
        sc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        sc.SampleDesc.Count = 1;
        sc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
        sc.BufferCount = 2;
        sc.Scaling = DXGI_SCALING_STRETCH;
        sc.AlphaMode = DXGI_ALPHA_MODE_PREMULTIPLIED;
        sc.SwapEffect = DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL;
        if (FAILED(q.factory->CreateSwapChainForComposition(q.device.Get(), &sc, nullptr, &chain))) return false;
        ComPtr<ID3D11Texture2D> back;
        if (FAILED(chain->GetBuffer(0, IID_PPV_ARGS(&back))) ||
            FAILED(q.device->CreateRenderTargetView(back.Get(), nullptr, &target))) return false;
        D3D11_TEXTURE2D_DESC tx{};
        tx.Width = tw; tx.Height = th;
        tx.Format = DXGI_FORMAT_B8G8R8X8_UNORM;
        tx.MipLevels = tx.ArraySize = tx.SampleDesc.Count = 1;
        tx.Usage = D3D11_USAGE_IMMUTABLE;
        tx.BindFlags = D3D11_BIND_SHADER_RESOURCE;
        const D3D11_SUBRESOURCE_DATA pixels{bgra, tw * 4, 0};
        ComPtr<ID3D11Texture2D> picture;
        return SUCCEEDED(q.device->CreateTexture2D(&tx, &pixels, &picture)) &&
               SUCCEEDED(q.device->CreateShaderResourceView(picture.Get(), nullptr, &texture));
    }

    int draw(Pipeline& q, const RECT& window, const RECT& icon, const RECT& canvas,
             int side, double progress, float alpha) {
        auto ctx = q.context.Get();
        D3D11_MAPPED_SUBRESOURCE map{};
        if (FAILED(ctx->Map(q.vb.Get(), 0, D3D11_MAP_WRITE_DISCARD, 0, &map))) return 0;
        griglia(Elemento{window, icon, progress, side}, static_cast<Vertice*>(map.pData));
        ctx->Unmap(q.vb.Get(), 0);
        const float rx = std::min(8.f * tw / std::max(1L, window.right - window.left), float(tw) * 0.5f);
        const float ry = std::min(8.f * th / std::max(1L, window.bottom - window.top), float(th) * 0.5f);
        const Constants constants{{float(canvas.left), float(canvas.top)}, {float(w), float(h)},
            alpha, rx, {float(tw), float(th)}, 0, 0, ry, 0, {1, 1}, {0, 0}};
        ctx->UpdateSubresource(q.cb.Get(), 0, nullptr, &constants, 0, 0);
        const D3D11_VIEWPORT viewport{0, 0, float(w), float(h), 0, 1};
        const float zero[4]{};
        auto rtv = target.Get();
        ctx->OMSetRenderTargets(1, &rtv, nullptr);
        ctx->ClearRenderTargetView(rtv, zero);
        ctx->RSSetViewports(1, &viewport);
        ctx->RSSetState(q.raster.Get());
        ctx->IASetInputLayout(q.layout.Get());
        const UINT stride = sizeof(Vertice), offset = 0;
        auto vb = q.vb.Get();
        ctx->IASetVertexBuffers(0, 1, &vb, &stride, &offset);
        ctx->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
        ctx->VSSetShader(q.vs.Get(), nullptr, 0);
        ctx->PSSetShader(q.ps.Get(), nullptr, 0);
        auto cb = q.cb.Get();
        ctx->VSSetConstantBuffers(0, 1, &cb);
        ctx->PSSetConstantBuffers(0, 1, &cb);
        auto s = q.sampler.Get();
        ctx->PSSetSamplers(0, 1, &s);
        auto tex = texture.Get();
        ctx->PSSetShaderResources(0, 1, &tex);
        ctx->Draw(RIGHE * 2, 0);
        tex = nullptr;
        ctx->PSSetShaderResources(0, 1, &tex);
        ctx->OMSetRenderTargets(0, nullptr, nullptr);
        const HRESULT hr = chain->Present(1, DXGI_PRESENT_DO_NOT_WAIT);
        if (hr == DXGI_ERROR_WAS_STILL_DRAWING) return 2;
        return hr == S_OK ? 1 : 0;
    }
};
}

extern "C" {
void* tlauncher_fluidwin_pipeline(ID3D11DeviceContext* context) {
    auto p = new (std::nothrow) Pipeline;
    if (p && context && p->init(context)) return p;
    delete p;
    return nullptr;
}
void tlauncher_fluidwin_release_pipeline(void* p) { delete static_cast<Pipeline*>(p); }

void* tlauncher_fluidwin_layer(void* p, UINT w, UINT h, const void* bgra, UINT tw, UINT th) {
    auto l = new (std::nothrow) Layer;
    if (l && l->init(*static_cast<Pipeline*>(p), w, h, bgra, tw, th)) return l;
    delete l;
    return nullptr;
}
void tlauncher_fluidwin_release_layer(void* p) { delete static_cast<Layer*>(p); }

// Borrowed COM pointer; DirectComposition AddRefs it when visual content is set.
IDXGISwapChain1* tlauncher_fluidwin_content(void* p) { return static_cast<Layer*>(p)->chain.Get(); }

int tlauncher_fluidwin_draw(void* p, void* layer, const RECT* window, const RECT* icon,
                          const RECT* canvas, int side, double progress, float opacity) {
    return static_cast<Layer*>(layer)->draw(*static_cast<Pipeline*>(p), *window, *icon, *canvas, side, progress, opacity);
}
double tlauncher_fluidwin_progress(double t, int restore) {
    return restore ? std::pow(1.0 - std::clamp(t, 0.0, 1.0), 3) : curvaGenio(t);
}
}
