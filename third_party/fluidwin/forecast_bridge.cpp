// Adapted centro.h::disegnaWidget weather card and simboloMeteo, v1.3.1.
// Draws into the existing Control Center bitmap; no window/device/worker/feed.
#include "upstream/vetro.h"
#include <wrl/client.h>
#include <memory>
namespace {
using Microsoft::WRL::ComPtr;
struct Day {int code;wchar_t label[32],temperature[32];};
struct Forecast {int code,count,ready,focused;wchar_t city[128],temperature[32],detail[256];Day days[5];};
struct Canvas {
    unsigned phase=0;
    ComPtr<ID2D1DCRenderTarget> dc;ComPtr<ID2D1SolidColorBrush> pen;
    ComPtr<ID2D1Bitmap> background;UINT width=0,height=0;
    ComPtr<IDWriteTextFormat> city,big,detail,day;float scale=0;
    bool prepare(float d){
        auto f=fabbriche::d2d();auto text=fabbriche::testo();if(!f||!text)return false;
        if(!dc){
            auto props=D2D1::RenderTargetProperties(D2D1_RENDER_TARGET_TYPE_SOFTWARE,D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM,D2D1_ALPHA_MODE_IGNORE),96,96);
            if(FAILED(f->CreateDCRenderTarget(&props,&dc)))return false;
        }
        if(!pen&&FAILED(dc->CreateSolidColorBrush(D2D1::ColorF(1,1,1,1),&pen)))return false;
        if(scale!=d){
            city.Reset();big.Reset();detail.Reset();day.Reset();
            auto format=[&](float size,DWRITE_FONT_WEIGHT weight,ComPtr<IDWriteTextFormat>& out){
                if(FAILED(text->CreateTextFormat(vetro::famiglia(text),nullptr,weight,DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,size*d,L"",&out)))return false;
                out->SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);return true;
            };
            if(!format(14,DWRITE_FONT_WEIGHT_MEDIUM,city)||!format(36,DWRITE_FONT_WEIGHT_LIGHT,big)||!format(11.5f,DWRITE_FONT_WEIGHT_NORMAL,detail)||!format(10.5f,DWRITE_FONT_WEIGHT_NORMAL,day))return false;
            day->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);scale=d;
        }
        return true;
    }
    void write(const wchar_t* text,IDWriteTextFormat* format,D2D1_RECT_F rect,float alpha=1){
        pen->SetColor(D2D1::ColorF(1,1,1,alpha));dc->DrawText(text,(UINT32)wcsnlen_s(text,256),format,rect,pen.Get(),D2D1_DRAW_TEXT_OPTIONS_CLIP);
    }
    void symbol(int c,float cx,float cy,float s){
        const float d=scale*s;
        auto sun=[&](float x,float y,float r){
            dc->FillEllipse({{x,y},r*d,r*d},pen.Get());
            for(int i=0;i<8;++i){const float a=i*3.14159265358979323846f/4;dc->DrawLine({x+std::cos(a)*(r+2.5f)*d,y+std::sin(a)*(r+2.5f)*d},{x+std::cos(a)*(r+5)*d,y+std::sin(a)*(r+5)*d},pen.Get(),1.6f*d);}
        };
        auto cloud=[&](float x,float y){
            dc->FillEllipse({{x-5*d,y+2*d},6*d,5*d},pen.Get());dc->FillEllipse({{x+d,y-2*d},7*d,7*d},pen.Get());dc->FillEllipse({{x+7*d,y+2*d},5.5f*d,5*d},pen.Get());
            dc->FillRoundedRectangle({{x-11*d,y+d,x+12.5f*d,y+7*d},3*d,3*d},pen.Get());
        };
        if(c==0){sun(cx,cy,6);return;}
        if(c<=2){sun(cx-4*d,cy-4*d,4.5f);cloud(cx+2*d,cy+3*d);return;}
        cloud(cx,cy-2*d);
        if(c==45||c==48){for(int i=0;i<2;++i)dc->DrawLine({cx-9*d,cy+(9+i*4)*d},{cx+9*d,cy+(9+i*4)*d},pen.Get(),1.6f*d);return;}
        if((c>=51&&c<=67)||(c>=80&&c<=82)){for(int i=-1;i<=1;++i)dc->DrawLine({cx+i*5*d,cy+8*d},{cx+i*5*d-2*d,cy+13*d},pen.Get(),1.6f*d);return;}
        if((c>=71&&c<=77)||c==85||c==86){for(int i=-1;i<=1;++i)dc->FillEllipse({{cx+i*5*d,cy+11*d},1.5f*d,1.5f*d},pen.Get());return;}
        if(c>=95){dc->DrawLine({cx+d,cy+6*d},{cx-2*d,cy+11*d},pen.Get(),1.8f*d);dc->DrawLine({cx-2*d,cy+11*d},{cx+2*d,cy+11*d},pen.Get(),1.8f*d);dc->DrawLine({cx+2*d,cy+11*d},{cx-d,cy+16*d},pen.Get(),1.8f*d);}
    }
};
}
extern "C" void* tlauncher_forecast_canvas_create()noexcept{try{return new Canvas;}catch(...){return nullptr;}}
extern "C" void tlauncher_forecast_canvas_free(void* raw)noexcept{delete static_cast<Canvas*>(raw);}
extern "C" unsigned tlauncher_forecast_canvas_phase(void* raw)noexcept{return raw?static_cast<Canvas*>(raw)->phase:0;}
extern "C" bool tlauncher_forecast_canvas_draw(void* raw,HDC target,const RECT* logical,float dpi,const Forecast* forecast)noexcept{
    if(!raw||!target||!logical||!forecast||forecast->count<0||forecast->count>5||!std::isfinite(dpi)||dpi<=0)return false;
    try{
        auto& c=*static_cast<Canvas*>(raw);c.phase=1;
        POINT points[2]={{logical->left,logical->top},{logical->right,logical->bottom}};
        if(!LPtoDP(target,points,2))return false;
        RECT rect={points[0].x,points[0].y,points[1].x,points[1].y};if(IsRectEmpty(&rect))return false;
        const float d=dpi*(rect.bottom-rect.top)/float(logical->bottom-logical->top);
        c.phase=2;DIBSECTION dib{};if(GetObjectW(GetCurrentObject(target,OBJ_BITMAP),sizeof(dib),&dib)!=sizeof(dib)||!dib.dsBm.bmBits||dib.dsBm.bmBitsPixel!=32||dib.dsBmih.biHeight>=0||rect.left<0||rect.top<0||rect.right>dib.dsBm.bmWidth||rect.bottom>dib.dsBm.bmHeight)return false;
        const int saved=SaveDC(target);if(!saved)return false;
        struct Restore{HDC dc;int saved;~Restore(){RestoreDC(dc,saved);}}restore{target,saved};
        SetMapMode(target,MM_TEXT);SetWindowOrgEx(target,0,0,nullptr);SetViewportOrgEx(target,0,0,nullptr);
        c.phase=3;if(!c.prepare(d))return false;
        c.phase=4;if(FAILED(c.dc->BindDC(target,&rect)))return false;
        const UINT width=UINT(rect.right-rect.left),height=UINT(rect.bottom-rect.top);
        c.phase=5;if(!c.background||c.width!=width||c.height!=height){c.background.Reset();if(FAILED(c.dc->CreateBitmap(D2D1::SizeU(width,height),nullptr,0,D2D1::BitmapProperties(D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM,D2D1_ALPHA_MODE_IGNORE)),&c.background)))return false;c.width=width;c.height=height;}
        GdiFlush();const auto pixels=static_cast<const BYTE*>(dib.dsBm.bmBits)+size_t(rect.top)*dib.dsBm.bmWidthBytes+size_t(rect.left)*4;
        c.phase=6;if(FAILED(c.background->CopyFromMemory(nullptr,pixels,UINT(dib.dsBm.bmWidthBytes))))return false;
        const float w=float(width),h=float(height);c.dc->BeginDraw();c.dc->DrawBitmap(c.background.Get(),{0,0,w,h});
        // The existing glass remains under the transparent rounded corners.
        ComPtr<ID2D1GradientStopCollection> stops;ComPtr<ID2D1LinearGradientBrush> gradient;
        const D2D1_GRADIENT_STOP gs[2]={{0,D2D1::ColorF(.24f,.52f,.92f,1)},{1,D2D1::ColorF(.42f,.70f,.98f,1)}};
        if(SUCCEEDED(c.dc->CreateGradientStopCollection(gs,2,&stops))&&SUCCEEDED(c.dc->CreateLinearGradientBrush({{0,0},{0,h}},stops.Get(),&gradient)))c.dc->FillRoundedRectangle({{0,0,w,h},14*d,14*d},gradient.Get());
        else c.dc->Clear(D2D1::ColorF(.24f,.52f,.92f,1));
        if(forecast->focused){c.pen->SetColor(D2D1::ColorF(1,1,1,.18f));c.dc->FillRoundedRectangle({{8*d,7*d,w-8*d,34*d},8*d,8*d},c.pen.Get());}
        c.write(forecast->city,c.city.Get(),{16*d,10*d,w-34*d,32*d});
        c.pen->SetColor(D2D1::ColorF(1,1,1,.8f));c.dc->DrawLine({w-25*d,19*d},{w-21.5f*d,22.5f*d},c.pen.Get(),1.6f*d);c.dc->DrawLine({w-21.5f*d,22.5f*d},{w-18*d,19*d},c.pen.Get(),1.6f*d);
        if(forecast->ready){
            c.write(forecast->temperature,c.big.Get(),{16*d,30*d,w-80*d,76*d});c.pen->SetColor(D2D1::ColorF(1,1,1,1));c.symbol(forecast->code,w-40*d,44*d,1.4f);
            c.write(forecast->detail,c.detail.Get(),{16*d,78*d,w-12*d,99*d},.8f);
            for(int i=0;i<forecast->count;++i){const float cw=(w-16*d)/5,cx=8*d+cw*(i+.5f),y=104*d;
                c.write(forecast->days[i].label,c.day.Get(),{cx-cw/2,y,cx+cw/2,y+15*d},.8f);c.pen->SetColor(D2D1::ColorF(1,1,1,1));c.symbol(forecast->days[i].code,cx,y+27*d,.8f);
                c.write(forecast->days[i].temperature,c.day.Get(),{cx-cw/2,y+40*d,cx+cw/2,y+57*d});
            }
        }else c.write(forecast->detail,c.detail.Get(),{16*d,44*d,w-16*d,h-12*d},.8f);
        c.phase=7;const auto result=c.dc->EndDraw();
        if(FAILED(result)){c.background.Reset();c.pen.Reset();c.dc.Reset();c.scale=0;}
        if(SUCCEEDED(result))c.phase=8;return SUCCEEDED(result);
    }catch(...){return false;}
}
