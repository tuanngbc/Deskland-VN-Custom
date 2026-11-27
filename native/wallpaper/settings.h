// Native wallpaper page hosted inside the accepted FluidWin Settings window.
#pragma once
#include "controller.h"
#include "images.h"
#include "../fluidwin/vetro.h"
#include <functional>

namespace wallpaper {
class SettingsPage {
    struct Gallery {std::atomic<bool> started{false},ready{false};std::array<Pixels,16> images;std::string error;};
    std::shared_ptr<Gallery> gallery=std::make_shared<Gallery>();
    std::array<ID2D1Bitmap*,16> thumbs{};ID2D1RenderTarget*target=nullptr;int selectedMonitor=0,focus=-1;std::vector<int> focusable;
    std::vector<Monitor> monitors;
    void loadGallery(HWND notify){if(gallery->started.exchange(true))return;auto*g=new std::pair<std::shared_ptr<Gallery>,HWND>(gallery,notify);
        if(!pool::lancia([](void*raw)->DWORD {auto job=std::unique_ptr<std::pair<std::shared_ptr<Gallery>,HWND>>(static_cast<std::pair<std::shared_ptr<Gallery>,HWND>*>(raw));HRESULT com=CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED);
            for(size_t i=0;i<catalog().size();i++){try{job->first->images[i]=decode(assetsRoot()/catalog()[i].thumbnail,180,102);}catch(const std::exception&e){job->first->error=e.what();}}
            if(SUCCEEDED(com))CoUninitialize();job->first->ready=true;PostMessageW(job->second,WM_PAINT,0,0);return 0;
        },g,THREAD_PRIORITY_BELOW_NORMAL)){delete g;gallery->error="Wallpaper gallery could not load";gallery->ready=true;}}
    void release(){for(auto&b:thumbs)if(b){b->Release();b=nullptr;}target=nullptr;}
    int scene(const Config&c)const{if(c.layout==Layout::Independent&&!monitors.empty()){auto i=c.displays.find(monitors[selectedMonitor].id);if(i!=c.displays.end())return i->second.scene;}return c.scene;}
public:
    std::function<Config()> read;
    std::function<void(Config)> changed;
    std::function<Metrics()> metrics;
    ~SettingsPage(){release();}
    bool key(WPARAM key){if(focusable.empty())return false;if(key==VK_TAB){int step=GetKeyState(VK_SHIFT)<0?-1:1;focus=(focus+step+int(focusable.size()))%int(focusable.size());return true;}if((key==VK_SPACE||key==VK_RETURN)&&focus>=0){click(focusable[size_t(focus)]);return true;}return false;}
    bool click(int action){if(action<5000||!read||!changed)return false;Config c=read();
        if(action==5000)c.enabled=!c.enabled;else if(action==5001)c.paused=!c.paused;
        else if(action>=5010&&action<=5012)c.layout=Layout(action-5010);
        else if(action>=5020&&action<5036){selectedMonitor=std::clamp(action-5020,0,std::max(0,int(monitors.size())-1));return true;}
        else if(action==5040&&!monitors.empty()){auto&a=c.displays[monitors[selectedMonitor].id];a.enabled=!a.enabled;}
        else if(action>=5100&&action<5116){if(c.layout==Layout::Independent&&!monitors.empty())c.displays[monitors[selectedMonitor].id].scene=action-5100;else c.scene=action-5100;}
        else if(action>=5200&&action<5400){int n=(action-5200)/2;int id=scene(c);const auto&params=catalog()[id].params;if(n>=int(params.size()))return true;const auto&p=params[n];double value=parameter(c,id,p.key);value+=((action-5200)%2?1:-1)*p.step;c.sceneParams[catalog()[id].id][p.key]=std::clamp(value,p.min,p.max);}
        else if(action>=5400&&action<=5405)c.weather=WeatherMode(action-5400);
        else if(action==5410)c.solar=!c.solar;else if(action==5411)c.clockLighting=!c.clockLighting;
        else if(action==5420)c.manualHour=std::max(0.f,c.manualHour-1);else if(action==5421)c.manualHour=std::min(24.f,c.manualHour+1);
        else if(action==5430)c.fps=std::max(5,c.fps-5);else if(action==5431)c.fps=std::min(60,c.fps+5);
        else if(action==5440)c.pauseCovered=!c.pauseCovered;else if(action==5441)c.resolution=c.resolution<.99f?1.f:.75f;
        else if(action==5450)c.wind=std::max(0.f,c.wind-.1f);else if(action==5451)c.wind=std::min(2.f,c.wind+.1f);
        else return false;
        changed(std::move(c));return true;
    }
    void draw(vetro::Finestra&v,float left,float right,float&y,IDWriteTextFormat*font,IDWriteTextFormat*small,IDWriteTextFormat*center,D2D1_COLOR_F ink,D2D1_COLOR_F muted,D2D1_COLOR_F accent,float alpha,const std::function<void(D2D1_RECT_F,int)>&zone,int hovered){
        if(!read)return;Config c=read();monitors=topology();selectedMonitor=std::clamp(selectedMonitor,0,std::max(0,int(monitors.size())-1));const float d=v.dpi;loadGallery(v.fin);
        if(target!=v.rt){release();target=v.rt;}
        if(gallery->ready){for(size_t i=0;i<thumbs.size();i++)if(!thumbs[i]&&!gallery->images[i].rgba.empty()){const auto&p=gallery->images[i];v.rt->CreateBitmap(D2D1::SizeU(p.width,p.height),p.rgba.data(),p.width*4,D2D1::BitmapProperties(D2D1::PixelFormat(DXGI_FORMAT_R8G8B8A8_UNORM,D2D1_ALPHA_MODE_IGNORE)),&thumbs[i]);}}
        focusable.clear();
        auto button=[&](D2D1_RECT_F r,int id,const std::wstring&label,bool selected=false){zone(r,id);focusable.push_back(id);bool focused=focus>=0&&focus<int(focusable.size())&&focusable[size_t(focus)]==id;
            v.pen->SetColor(selected?D2D1::ColorF(accent.r,accent.g,accent.b,.22f*alpha):D2D1::ColorF(v.scuro?1.f:0.f,v.scuro?1.f:0.f,v.scuro?1.f:0.f,(hovered==id?.12f:.06f)*alpha));v.rt->FillRoundedRectangle({r,7*d,7*d},v.pen);if(focused||selected){v.pen->SetColor(accent);v.rt->DrawRoundedRectangle({r,7*d,7*d},v.pen,1.3f*d);}v.testo(label,center,r,ink);};
        auto row=[&](const std::wstring&name,const std::wstring&detail,int id,bool on){D2D1_RECT_F r{left,y,right,y+58*d};v.pen->SetColor(D2D1::ColorF(v.scuro?1.f:0.f,v.scuro?1.f:0.f,v.scuro?1.f:0.f,.045f*alpha));v.rt->FillRoundedRectangle({r,9*d,9*d},v.pen);v.testo(name,font,{left+12*d,y+4*d,right-90*d,y+28*d},ink);v.testo(detail,small,{left+12*d,y+28*d,right-90*d,y+52*d},muted);button({right-78*d,y+12*d,right-10*d,y+46*d},id,on?L"On":L"Off",on);y+=64*d;};
        auto title=[&](const std::wstring&t){v.testo(t,font,{left,y,right,y+28*d},ink);y+=34*d;};
        auto choices=[&](const std::vector<std::wstring>&labels,int base,int selected){float width=(right-left-6*d*(labels.size()-1))/float(labels.size());for(size_t i=0;i<labels.size();i++)button({left+float(i)*(width+6*d),y,left+float(i)*(width+6*d)+width,y+36*d},base+int(i),labels[i],int(i)==selected);y+=44*d;};
        auto adjust=[&](const std::wstring&name,const std::wstring&value,int base){v.testo(name,font,{left+10*d,y,right-150*d,y+36*d},ink);button({right-134*d,y,right-99*d,y+32*d},base,L"−");v.testo(value,center,{right-98*d,y,right-46*d,y+32*d},ink);button({right-44*d,y,right-9*d,y+32*d},base+1,L"+");y+=42*d;};
        row(L"Live Wallpaper",L"Native renderer · isolated TEST configuration",5000,c.enabled);
        row(L"Pause wallpaper",L"Keeps the last frame; core animations remain available",5001,c.paused);
        title(L"Display layout");choices({L"Independent",L"Synchronized",L"Panorama"},5010,int(c.layout));
        for(size_t i=0;i<monitors.size();i++){const auto&m=monitors[i];std::wstring text=m.id+L" · "+std::to_wstring(m.rect.right-m.rect.left)+L"×"+std::to_wstring(m.rect.bottom-m.rect.top)+L" · "+std::to_wstring(m.dpi*100/96)+L"%";button({left,y,right,y+32*d},5020+int(i),text,int(i)==selectedMonitor);y+=38*d;}
        if(!monitors.empty()){auto i=c.displays.find(monitors[selectedMonitor].id);row(L"Wallpaper on selected display",L"Display coordinates and orientation follow Windows",5040,i==c.displays.end()||i->second.enabled);}
        title(L"Wallpaper gallery · select a scene to preview on Desktop");int selected=scene(c);float gap=8*d,cardW=(right-left-gap*2)/3,cardH=94*d;
        for(size_t i=0;i<catalog().size();i++){int col=int(i%3);float x=left+col*(cardW+gap);D2D1_RECT_F card{x,y,x+cardW,y+cardH};button(card,5100+int(i),L"",int(i)==selected);if(thumbs[i])v.rt->DrawBitmap(thumbs[i],{x+4*d,y+4*d,x+cardW-4*d,y+65*d},alpha,D2D1_BITMAP_INTERPOLATION_MODE_LINEAR);v.testo(catalog()[i].name,small,{x+5*d,y+66*d,x+cardW-5*d,y+cardH},ink);if(col==2||i+1==catalog().size())y+=cardH+gap;}
        if(!gallery->ready){v.testo(L"Loading scene thumbnails…",small,{left,y,right,y+26*d},muted);y+=30*d;}
        title(L"Scene customization");const auto&s=catalog()[selected];for(size_t i=0;i<s.params.size();i++){const auto&p=s.params[i];wchar_t value[48];swprintf(value,48,L"%.2g",parameter(c,selected,p.key));adjust(p.name,value,5200+int(i)*2);}
        title(L"Weather");choices({L"Clear",L"Rain",L"Snow"},5400,int(c.weather));choices({L"Clouds",L"Live",L"Offline"},5403,int(c.weather)-3);
        v.testo(L"Live uses the city selected in Control Center → Weather",small,{left,y,right,y+28*d},muted);y+=32*d;
        adjust(L"Wind",std::to_wstring(int(c.wind*100))+L"%",5450);
        row(L"Solar lighting and shadows",L"Uses shared weather coordinates when available",5410,c.solar);
        row(L"Use current time",L"Disable to select the lighting hour below",5411,c.clockLighting);adjust(L"Lighting hour",std::to_wstring(int(c.manualHour)),5420);
        title(L"Performance");adjust(L"Maximum frame rate",std::to_wstring(c.fps)+L" FPS",5430);
        row(L"Pause covered displays",L"Also suspends on session lock and display power off",5440,c.pauseCovered);
        row(L"Full resolution",L"Off renders at 75%; layout stays in physical coordinates",5441,c.resolution>.99f);
        if(metrics){auto m=metrics();std::wstring state=m.running?(m.paused?L"Paused":L"Rendering"):c.enabled?L"Starting / recovery":L"Disabled · graphics resources released";v.testo(state,small,{left,y,right,y+26*d},muted);y+=30*d;if(!m.error.empty()){int n=MultiByteToWideChar(CP_UTF8,0,m.error.data(),int(m.error.size()),nullptr,0);std::wstring error(n,L'\0');MultiByteToWideChar(CP_UTF8,0,m.error.data(),int(m.error.size()),error.data(),n);v.testo(error,small,{left,y,right,y+52*d},ink);y+=56*d;}}
    }
};
}
