#pragma once
#include "gl.h"
#include <wincodec.h>
#include <memory>

namespace wallpaper {
template<class T>class Com {
    T*p=nullptr;
public:
    ~Com(){if(p)p->Release();}Com()=default;Com(const Com&)=delete;
    T**put(){if(p){p->Release();p=nullptr;}return &p;}T*get()const{return p;}T*operator->()const{return p;}
};
struct Pixels {UINT width=0,height=0;std::vector<unsigned char> rgba;};
inline Pixels decode(const std::filesystem::path&path,UINT maxWidth=2048,UINT maxHeight=2048){
    Com<IWICImagingFactory> factory;Com<IWICBitmapDecoder> decoder;Com<IWICBitmapFrameDecode> frame;Com<IWICBitmapScaler> scaler;Com<IWICFormatConverter> converter;
    if(FAILED(CoCreateInstance(CLSID_WICImagingFactory,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(factory.put())))||FAILED(factory->CreateDecoderFromFilename(path.c_str(),nullptr,GENERIC_READ,WICDecodeMetadataCacheOnLoad,decoder.put()))||FAILED(decoder->GetFrame(0,frame.put())))throw std::runtime_error("Wallpaper image decode failed");
    Pixels pixels;UINT w=0,h=0;frame->GetSize(&w,&h);if(!w||!h||w>16384||h>16384||uint64_t(w)*h>64*1024*1024)throw std::runtime_error("Wallpaper image exceeds budget");
    double scale=std::min({1.,double(maxWidth)/w,double(maxHeight)/h});pixels.width=std::max(1u,UINT(w*scale));pixels.height=std::max(1u,UINT(h*scale));
    if(FAILED(factory->CreateBitmapScaler(scaler.put()))||FAILED(scaler->Initialize(frame.get(),pixels.width,pixels.height,WICBitmapInterpolationModeFant))||FAILED(factory->CreateFormatConverter(converter.put()))||FAILED(converter->Initialize(scaler.get(),GUID_WICPixelFormat32bppRGBA,WICBitmapDitherTypeNone,nullptr,0,WICBitmapPaletteTypeCustom)))throw std::runtime_error("Wallpaper image conversion failed");
    pixels.rgba.resize(size_t(pixels.width)*pixels.height*4);if(FAILED(converter->CopyPixels(nullptr,pixels.width*4,UINT(pixels.rgba.size()),pixels.rgba.data())))throw std::runtime_error("Wallpaper image pixels unavailable");return pixels;
}
class Sprites {
    GL&g;Program program;std::map<std::wstring,std::unique_ptr<Texture>> textures;
    Texture&image(const wchar_t*path){auto i=textures.find(path);if(i!=textures.end())return *i->second;auto pixels=decode(assetsRoot()/path);auto texture=std::make_unique<Texture>();texture->image(g,5,pixels.width,pixels.height,0x8058,GL_RGBA,GL_UNSIGNED_BYTE,pixels.rgba.data());auto&ref=*texture;textures.emplace(path,std::move(texture));return ref;}
public:
    explicit Sprites(GL&gl):g(gl),program(gl,R"(#version 330 core
uniform sampler2D sprite;uniform vec2 origin,world,pixelScale;uniform vec4 rect;uniform float time,mirror,shadow,alpha,shear;
out vec4 o;
void main(){vec2 p=vec2(gl_FragCoord.x/pixelScale.x,world.y-gl_FragCoord.y/pixelScale.y)+origin;vec2 uv=(p-rect.xy)/rect.zw;if(shadow>.5){uv.x-=shear*(1.-uv.y);uv.y=1.-(1.-uv.y)*2.8;}else uv.x+=sin(time*1.4+uv.y*2.)*.012*pow(1.-uv.y,2.);if(mirror>.5)uv.x=1.-uv.x;if(any(lessThan(uv,vec2(0)))||any(greaterThan(uv,vec2(1))))discard;vec4 c=texture(sprite,uv);o=vec4(shadow>.5?vec3(.12,.20,.18):c.rgb,c.a*alpha);})"){}
    void draw(const wchar_t*path,const View&view,float x,float y,float w,float h,float time,float scaleX,float scaleY,bool mirror=false,bool shadow=false,float alpha=1,float shear=.3f){
        auto&texture=image(path);texture.bind(g,5);program.use();program.i("sprite",5);program.v2("world",view.worldW,view.worldH);program.v2("origin",view.x,-(view.worldH-view.y-view.h));program.v2("pixelScale",scaleX,scaleY);program.v4("rect",{x,y,w,h});program.f("time",time);program.f("mirror",mirror?1.f:0.f);program.f("shadow",shadow?1.f:0.f);program.f("alpha",alpha);program.f("shear",shear);
        const float margin=shadow?w:0;int left=int((x-view.x-margin)*scaleX),right=int((x+w-view.x+margin)*scaleX),bottom=int((view.y+view.h-y-h)*scaleY),top=int((view.y+view.h-y)*scaleY);
        left=std::clamp(left,0,int(view.w*scaleX));right=std::clamp(right,0,int(view.w*scaleX));bottom=std::clamp(bottom,0,int(view.h*scaleY));top=std::clamp(top,0,int(view.h*scaleY));
        if(right<=left||top<=bottom)return;
        glEnable(GL_SCISSOR_TEST);glScissor(left,bottom,right-left,top-bottom);glEnable(GL_BLEND);glBlendFunc(GL_SRC_ALPHA,GL_ONE_MINUS_SRC_ALPHA);glDrawArrays(GL_TRIANGLES,0,3);glDisable(GL_BLEND);glDisable(GL_SCISSOR_TEST);
    }
};
}
