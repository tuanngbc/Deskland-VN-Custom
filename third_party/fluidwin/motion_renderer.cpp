// TLauncher D3D11 adapter for FluidWin's MIT-licensed grid and shader.
#include "motion_upstream.h"
#include <d3d11.h>
#include <dxgi1_2.h>
#include <dxgi1_4.h>
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
    ComPtr<ID3D11BlendState> blend;
    ComPtr<ID3D11ShaderResourceView> chips[2][2];

    bool tint(uint32_t bgra, ComPtr<ID3D11ShaderResourceView>& view) {
        // A small constant texture keeps the upstream shader's rounded
        // corner mask and subpixel coverage in the same code path.
        constexpr UINT side = 32;
        uint32_t pixels[side * side];
        std::fill_n(pixels, side * side, bgra);
        D3D11_TEXTURE2D_DESC desc{};
        desc.Width = desc.Height = side;
        desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        desc.MipLevels = desc.ArraySize = desc.SampleDesc.Count = 1;
        desc.Usage = D3D11_USAGE_IMMUTABLE;
        desc.BindFlags = D3D11_BIND_SHADER_RESOURCE;
        const D3D11_SUBRESOURCE_DATA init{pixels, side * 4, 0};
        ComPtr<ID3D11Texture2D> texture;
        return SUCCEEDED(device->CreateTexture2D(&desc, &init, &texture)) &&
               SUCCEEDED(device->CreateShaderResourceView(texture.Get(), nullptr, &view));
    }

    bool chip_theme(bool dark) {
        const unsigned theme = dark ? 1 : 0;
        return (chips[theme][0] || tint(dark ? 0xff2b2b33 : 0xfff5f5f7, chips[theme][0])) &&
               (chips[theme][1] || tint(dark ? 0x6b6b6b6b : 0x42000000, chips[theme][1]));
    }

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
        // A Genie row can become very short while staying wide. Filter the
        // compressed axis without smearing the other one into color bands.
        s.Filter = D3D11_FILTER_ANISOTROPIC;
        s.MaxAnisotropy = 4;
        s.AddressU = s.AddressV = s.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
        s.ComparisonFunc = D3D11_COMPARISON_NEVER;
        s.MaxLOD = D3D11_FLOAT32_MAX;
        D3D11_RASTERIZER_DESC r{};
        r.FillMode = D3D11_FILL_SOLID;
        r.CullMode = D3D11_CULL_NONE;
        r.DepthClipEnable = TRUE;
        D3D11_BLEND_DESC b{};
        auto& blendDesc = b.RenderTarget[0];
        blendDesc.BlendEnable = TRUE;
        blendDesc.SrcBlend = blendDesc.SrcBlendAlpha = D3D11_BLEND_ONE;
        blendDesc.DestBlend = blendDesc.DestBlendAlpha = D3D11_BLEND_INV_SRC_ALPHA;
        blendDesc.BlendOp = blendDesc.BlendOpAlpha = D3D11_BLEND_OP_ADD;
        blendDesc.RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_ALL;
        return SUCCEEDED(device->CreateSamplerState(&s, &sampler)) &&
               SUCCEEDED(device->CreateRasterizerState(&r, &raster)) &&
               SUCCEEDED(device->CreateBlendState(&b, &blend));
    }
};

struct Layer {
    ComPtr<IDXGISwapChain1> chain;
    ComPtr<ID3D11RenderTargetView> target;
    ComPtr<ID3D11ShaderResourceView> texture;
    // Allocated only by the hosted evidence API. Copy before Present, whose
    // flip can rotate the back buffer observed by a later readback.
    ComPtr<ID3D11Texture2D> evidence;
    UINT w = 0, h = 0, tw = 0, th = 0;
    HANDLE frameReady=nullptr;
    ~Layer(){if(frameReady)CloseHandle(frameReady);}

    bool surface(Pipeline& q, UINT width, UINT height) {
        if (!width || !height || width > 16384 || height > 16384 || uint64_t(width)*height > 33554432)
            return false;
        w = width; h = height;
        DXGI_SWAP_CHAIN_DESC1 sc{};
        sc.Width = w; sc.Height = h;
        sc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        sc.SampleDesc.Count = 1;
        sc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
        sc.BufferCount = 2;
        sc.Scaling = DXGI_SCALING_STRETCH;
        sc.AlphaMode = DXGI_ALPHA_MODE_PREMULTIPLIED;
        sc.SwapEffect = DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL;
        sc.Flags=DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT;
        if (FAILED(q.factory->CreateSwapChainForComposition(q.device.Get(), &sc, nullptr, &chain))) {
            sc.Flags=0; // old DXGI compatibility, reported by a missing clock
            if(FAILED(q.factory->CreateSwapChainForComposition(q.device.Get(), &sc, nullptr, &chain)))return false;
        } else {
            ComPtr<IDXGISwapChain2> paced;
            if(SUCCEEDED(chain.As(&paced))&&SUCCEEDED(paced->SetMaximumFrameLatency(1))) frameReady=paced->GetFrameLatencyWaitableObject();
        }
        ComPtr<ID3D11Texture2D> back;
        if (FAILED(chain->GetBuffer(0, IID_PPV_ARGS(&back))) ||
            FAILED(q.device->CreateRenderTargetView(back.Get(), nullptr, &target))) return false;
        return true;
    }

    bool init_gpu(Pipeline& q,UINT width,UINT height,ID3D11Texture2D* picture) {
        if (!picture) return false;
        ComPtr<ID3D11Device> device; picture->GetDevice(&device);
        if (device.Get()!=q.device.Get()) return false;
        D3D11_TEXTURE2D_DESC desc{};picture->GetDesc(&desc);
        if (desc.Format!=DXGI_FORMAT_B8G8R8A8_UNORM || !(desc.BindFlags&D3D11_BIND_SHADER_RESOURCE) ||
            desc.SampleDesc.Count!=1 || !surface(q,width,height)) return false;
        tw=desc.Width;th=desc.Height;
        if (FAILED(q.device->CreateShaderResourceView(picture,nullptr,&texture))) return false;
        if (desc.MiscFlags&D3D11_RESOURCE_MISC_GENERATE_MIPS) q.context->GenerateMips(texture.Get());
        return true;
    }

    bool init(Pipeline& q, UINT width, UINT height, const void* bgra, UINT sourceW, UINT sourceH) {
        if (!bgra || !sourceW || !sourceH || sourceW > 1024 || sourceH > 1024 || !surface(q,width,height)) return false;
        tw=sourceW;th=sourceH;
        D3D11_TEXTURE2D_DESC tx{};
        tx.Width = tw; tx.Height = th;
        // WGC returns premultiplied BGRA. Keep alpha at the window edge so the
        // unchanged FluidWin shader doesn't make transparent pixels opaque.
        tx.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        tx.MipLevels = tx.ArraySize = tx.SampleDesc.Count = 1;
        tx.Usage = D3D11_USAGE_IMMUTABLE;
        tx.BindFlags = D3D11_BIND_SHADER_RESOURCE;
        const D3D11_SUBRESOURCE_DATA pixels{bgra, tw * 4, 0};
        ComPtr<ID3D11Texture2D> picture;
        UINT support = 0;
        if (SUCCEEDED(q.device->CheckFormatSupport(tx.Format,&support)) && (support & D3D11_FORMAT_SUPPORT_MIP_AUTOGEN)) {
            auto mip = tx;
            mip.MipLevels = 0;
            mip.Usage = D3D11_USAGE_DEFAULT;
            mip.BindFlags |= D3D11_BIND_RENDER_TARGET;
            mip.MiscFlags = D3D11_RESOURCE_MISC_GENERATE_MIPS;
            if (SUCCEEDED(q.device->CreateTexture2D(&mip, nullptr, &picture)) &&
                SUCCEEDED(q.device->CreateShaderResourceView(picture.Get(), nullptr, &texture))) {
                q.context->UpdateSubresource(picture.Get(),0,nullptr,bgra,tw*4,0);
                q.context->GenerateMips(texture.Get());
                return true;
            }
            picture.Reset();
        }
        return SUCCEEDED(q.device->CreateTexture2D(&tx, &pixels, &picture)) &&
               SUCCEEDED(q.device->CreateShaderResourceView(picture.Get(), nullptr, &texture));
    }

    bool element(Pipeline& q, const RECT& canvas, const Vertice* vertices, UINT count,
                 ID3D11ShaderResourceView* tex, UINT imageW, UINT imageH, float radiusX,
                 float radiusY, float extentV, float opacity,float shadow=0,float lift=0) {
        auto ctx = q.context.Get();
        D3D11_MAPPED_SUBRESOURCE map{};
        if (FAILED(ctx->Map(q.vb.Get(), 0, D3D11_MAP_WRITE_DISCARD, 0, &map))) return false;
        std::memcpy(map.pData, vertices, count * sizeof(Vertice));
        ctx->Unmap(q.vb.Get(), 0);
        const Constants constants{{float(canvas.left), float(canvas.top)}, {float(w), float(h)},
            opacity, radiusX, {float(imageW), float(imageH)}, shadow, lift, radiusY, 0, {1, extentV}, {0, 0}};
        ctx->UpdateSubresource(q.cb.Get(), 0, nullptr, &constants, 0, 0);
        ctx->PSSetShaderResources(0, 1, &tex);
        ctx->Draw(count, 0);
        return true;
    }

    int draw(Pipeline& q, const RECT& window, const RECT& icon, const RECT& canvas,
             int side, double progress, float alpha, int effect, bool dark, double scale,
             const Vertice* mesh=nullptr,int columns=0,int rows=0,float lift=0) {
        const bool drop=effect==1;
        if (drop && !q.chip_theme(dark)) return 0;
        auto ctx = q.context.Get();
        const D3D11_VIEWPORT viewport{0, 0, float(w), float(h), 0, 1};
        const float zero[4]{};
        auto rtv = target.Get();
        ctx->OMSetRenderTargets(1, &rtv, nullptr);
        ctx->OMSetDepthStencilState(nullptr,0);
        ctx->OMSetBlendState(q.blend.Get(), nullptr, 0xffffffff);
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
        ctx->GSSetShader(nullptr,nullptr,0);
        ctx->HSSetShader(nullptr,nullptr,0);
        ctx->DSSetShader(nullptr,nullptr,0);
        ctx->SetPredication(nullptr,FALSE);
        auto cb = q.cb.Get();
        ctx->VSSetConstantBuffers(0, 1, &cb);
        ctx->PSSetConstantBuffers(0, 1, &cb);
        auto s = q.sampler.Get();
        ctx->PSSetSamplers(0, 1, &s);
        bool painted = true;
        if(effect==3){
            if(!mesh||columns!=14||rows!=11)return 0;
            q.context->GenerateMips(texture.Get());
            // Original fluidwin.cpp GEL_C/GEL_R mesh and expanded shadow ring.
            Vertice shadow[16*13];const float margin=float(80*scale);
            for(int r=0;r<13;r++)for(int c=0;c<16;c++){
                const int ci=std::clamp(c-1,0,13),ri=std::clamp(r-1,0,10);
                const float sx=c==0?-1.f:c==15?1.f:0.f,sy=r==0?-1.f:r==12?1.f:0.f;
                const auto& v=mesh[ri*14+ci];shadow[r*16+c]={v.x+sx*margin,v.y+sy*margin,v.u+sx*margin/tw,v.v+sy*margin/th};
            }
            const auto grid=[&](const Vertice* vertices,int cols,int rs,bool isShadow){
                bool ok=true;Vertice strip[26];
                for(int c=0;c<cols-1;c++){
                    for(int r=0;r<rs;r++){strip[2*r]=vertices[r*cols+c];strip[2*r+1]=vertices[r*cols+c+1];}
                    ok=element(q,canvas,strip,UINT(rs*2),texture.Get(),tw,th,float(8*scale),float(8*scale),1,alpha,isShadow?1.f:0.f,lift)&&ok;
                }return ok;
            };
            painted=grid(shadow,16,13,true)&&grid(mesh,14,11,false);
        }else if (effect==2) {
            const auto mix=[&](LONG a,LONG b){return float(a+(b-a)*std::clamp(progress,0.0,1.0));};
            const float l=mix(window.left,icon.left),t=mix(window.top,icon.top),r=mix(window.right,icon.right),b=mix(window.bottom,icon.bottom);
            const Vertice quad[]={{l,t,0,0},{r,t,1,0},{l,b,0,1},{r,b,1,1}};
            painted=element(q,canvas,quad,4,texture.Get(),tw,th,0,0,1,alpha);
        } else if (drop && progress > 0) {
            const auto chip = elementoCaduta(window, icon, side, progress, scale);
            const double radius = std::min(8.0 * scale, 0.22 * std::min(chip.r-chip.l,chip.b-chip.t));
            const double border = 2.0 * scale;
            const auto quad = [&](double pad, ID3D11ShaderResourceView* tex, UINT iw, UINT ih,
                                  double opacity) {
                const double l=chip.l-pad, t=chip.t-pad, r=chip.r+pad, b=chip.b+pad;
                const double visible=chip.piccolo ? std::min(b,chip.taglio) : b;
                if (opacity <= 0.002 || visible <= t) return true;
                const float v=float((visible-t)/(b-t));
                const double xs[]{l,r,l,r}, ys[]{t,t,visible,visible};
                const float uvs[]{0,1,0,1}, vvs[]{0,0,v,v};
                Vertice vertices[4];
                for (int k=0;k<4;k++) {
                    const auto point=dalCanonico(side,xs[k],ys[k]);
                    float u=uvs[k], vv=vvs[k];
                    if (v>=1.f) uvDalCanonico(side,u,vv,u,vv);
                    vertices[k]={float(point.x),float(point.y),u,vv};
                }
                const bool rotate = side >= 2 && v >= 1.f;
                return element(q,canvas,vertices,4,tex,iw,ih,
                    float((radius+pad)*iw/std::max(1.0,rotate?b-t:r-l)),
                    float((radius+pad)*ih/std::max(1.0,rotate?r-l:b-t)),v,float(opacity)*alpha);
            };
            const auto& colors=q.chips[dark ? 1 : 0];
            if (!chip.piccolo || chip.t < chip.taglio) {
                painted = quad(border,colors[1].Get(),32,32,1.0-chip.contenuto) &&
                    quad(0,colors[0].Get(),32,32,chip.contenuto<0.999 ? 1.0 : 0.0) &&
                    quad(0,texture.Get(),tw,th,chip.contenuto);
            }
        } else {
            Vertice grid[RIGHE * 2];
            griglia(Elemento{window, icon, progress, side}, grid);
            const float rx = std::min(float(8.0 * scale) * tw / std::max(1L, window.right - window.left), float(tw) * 0.5f);
            const float ry = std::min(float(8.0 * scale) * th / std::max(1L, window.bottom - window.top), float(th) * 0.5f);
            painted = element(q,canvas,grid,RIGHE*2,texture.Get(),tw,th,rx,ry,1,alpha);
        }
        ID3D11ShaderResourceView* tex = nullptr;
        ctx->PSSetShaderResources(0, 1, &tex);
        ctx->OMSetRenderTargets(0, nullptr, nullptr);
        if (!painted) return 0;
        if(evidence){ComPtr<ID3D11Resource> rendered;target->GetResource(&rendered);ctx->CopyResource(evidence.Get(),rendered.Get());}
        const HRESULT hr = chain->Present(1, DXGI_PRESENT_DO_NOT_WAIT);
        if (hr == DXGI_ERROR_WAS_STILL_DRAWING) return 2;
        return hr == S_OK ? 1 : static_cast<int>(hr);
    }
};
}

extern "C" {
uint64_t tlauncher_fluidwin_capture_budget(ID3D11Device* device) {
    // A native-resolution texture budget from actual adapter headroom. Never
    // scale down a full-window capture to fit a Stage thumbnail limit.
    ComPtr<IDXGIDevice> dxgi;ComPtr<IDXGIAdapter> adapter;ComPtr<IDXGIAdapter3> memory;
    if (device && SUCCEEDED(device->QueryInterface(IID_PPV_ARGS(&dxgi))) &&
        SUCCEEDED(dxgi->GetAdapter(&adapter)) && SUCCEEDED(adapter.As(&memory))) {
        DXGI_QUERY_VIDEO_MEMORY_INFO info{};
        if (SUCCEEDED(memory->QueryVideoMemoryInfo(0,DXGI_MEMORY_SEGMENT_GROUP_LOCAL,&info)))
            return std::min<uint64_t>(128ull<<20,info.Budget>info.CurrentUsage?(info.Budget-info.CurrentUsage)/8:0);
    }
    return 32ull<<20; // explicit native fallback if budget cannot hold the frame
}
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
void* tlauncher_fluidwin_gpu_layer(void* p,UINT w,UINT h,ID3D11Texture2D* texture) {
    auto l=new(std::nothrow) Layer;
    if(l&&p&&l->init_gpu(*static_cast<Pipeline*>(p),w,h,texture)) return l;
    delete l;return nullptr;
}

// Borrowed COM pointer; DirectComposition AddRefs it when visual content is set.
IDXGISwapChain1* tlauncher_fluidwin_content(void* p) { return static_cast<Layer*>(p)->chain.Get(); }
HANDLE tlauncher_fluidwin_frame_ready(void* p) { return static_cast<Layer*>(p)->frameReady; }

UINT tlauncher_fluidwin_source_mips(void* p) {
    ComPtr<ID3D11Resource> resource;
    ComPtr<ID3D11Texture2D> texture;
    static_cast<Layer*>(p)->texture->GetResource(&resource);
    if (FAILED(resource.As(&texture))) return 0;
    D3D11_TEXTURE2D_DESC desc{};
    texture->GetDesc(&desc);
    ComPtr<ID3D11Device> device;
    texture->GetDevice(&device);
    UINT support=0;
    if (desc.MipLevels==1 && std::max(desc.Width,desc.Height)>1 &&
        SUCCEEDED(device->CheckFormatSupport(desc.Format,&support)) && (support & D3D11_FORMAT_SUPPORT_MIP_AUTOGEN)) return 0;
    return desc.Format == DXGI_FORMAT_B8G8R8A8_UNORM ? desc.MipLevels : 0;
}

// Used by explicit hosted visual fixtures: read the entire rendered target,
// including every border texel, after real shader/raster/blend execution.
int tlauncher_fluidwin_enable_evidence(void* p,void* layer) {
    auto& q=*static_cast<Pipeline*>(p);auto& l=*static_cast<Layer*>(layer);
    ComPtr<ID3D11Resource> resource;l.target->GetResource(&resource);
    ComPtr<ID3D11Texture2D> rendered;if(FAILED(resource.As(&rendered)))return 0;
    D3D11_TEXTURE2D_DESC desc{};rendered->GetDesc(&desc);
    desc.Usage=D3D11_USAGE_STAGING;desc.BindFlags=0;desc.MiscFlags=0;desc.CPUAccessFlags=D3D11_CPU_ACCESS_READ;
    return SUCCEEDED(q.device->CreateTexture2D(&desc,nullptr,&l.evidence));
}
int tlauncher_fluidwin_rendered_pixels(void* p,void* layer,void* pixels,UINT stride) {
    auto& q=*static_cast<Pipeline*>(p);auto& l=*static_cast<Layer*>(layer);
    if(!pixels||stride<l.w*4||!l.evidence) return 0;
    D3D11_MAPPED_SUBRESOURCE map{};
    if(FAILED(q.context->Map(l.evidence.Get(),0,D3D11_MAP_READ,0,&map))) return 0;
    for(UINT y=0;y<l.h;y++) std::memcpy(static_cast<unsigned char*>(pixels)+y*stride,static_cast<const unsigned char*>(map.pData)+y*map.RowPitch,l.w*4);
    q.context->Unmap(l.evidence.Get(),0);return 1;
}

int tlauncher_fluidwin_draw(void* p, void* layer, const RECT* window, const RECT* icon,
                          const RECT* canvas, int side, double progress, float opacity,
                          int drop, int dark, double scale) {
    return static_cast<Layer*>(layer)->draw(*static_cast<Pipeline*>(p), *window, *icon, *canvas, side,
                                          progress, opacity, drop, dark != 0, scale);
}
int tlauncher_fluidwin_custom(void* p,void* layer,const RECT* rect,const RECT* canvas,const Vertice* vertices,unsigned count,float lift,float alpha,double scale){
    if(!p||!layer||!rect||!canvas||(count!=0&&count!=154))return 0;
    return static_cast<Layer*>(layer)->draw(*static_cast<Pipeline*>(p),*rect,*rect,*canvas,0,0,alpha,count?3:2,false,scale,vertices,14,11,lift);
}
double tlauncher_fluidwin_progress(double t, int restore, int drop) {
    if (drop) return restore ? cadutaApre(t) : cadutaChiude(t);
    return restore ? std::pow(1.0 - std::clamp(t, 0.0, 1.0), 3) : curvaGenio(t);
}
double tlauncher_fluidwin_invert(double p, int restore, int drop) {
    if (drop) {
        // FluidWin invertiTempo, used at direction changes.
        double lo = 0, hi = 1;
        for (int i = 0; i < 40; i++) {
            const double m = (lo + hi) / 2;
            if (((restore ? cadutaApre(m) : cadutaChiude(m)) < p) == !restore) lo = m;
            else hi = m;
        }
        return (lo + hi) / 2;
    }
    return restore ? 1.0 - std::cbrt(std::clamp(p, 0.0, 1.0)) : inversaCurvaGenio(p);
}
}
