#pragma once
#include "gl.h"
#include "spatial.h"
#include <wincodec.h>
#include <memory>
#include <string_view>

namespace wallpaper {
template<class T>class Com {
    T*p=nullptr;
public:
    ~Com(){if(p)p->Release();}Com()=default;Com(const Com&)=delete;
    Com(Com&&other)noexcept:p(other.p){other.p=nullptr;}Com&operator=(Com&&other)noexcept{if(this!=&other){if(p)p->Release();p=other.p;other.p=nullptr;}return *this;}
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
// Immutable package images share one GPU upload within the session/context.
// Weak entries never retain a scene's last texture after its owners release it.
class ImageCache {
    GL& g;std::map<std::wstring,std::weak_ptr<Texture>> images;
public:
    explicit ImageCache(GL& gl):g(gl){}
    std::shared_ptr<Texture> image(const wchar_t* path){
        auto& entry=images[path];if(auto current=entry.lock())return current;
        auto pixels=decode(assetsRoot()/path);auto texture=std::make_shared<Texture>();
        texture->image(g,5,pixels.width,pixels.height,0x8058,GL_RGBA,GL_UNSIGNED_BYTE,pixels.rgba.data());entry=texture;return texture;
    }
};
// Audited from prototype/meadow-cat.js: long rests, then a short tail flick.
inline float meadowKittenTailAngle(float time,int seed=7341){
    const float period=10+abs(seed%4),age=float(fmod(std::max(0.f,time)+abs(seed%1300)/1000.f,period))-3;
    if(age<0||age>=1.6f)return 0;
    const float phase=age/1.6f,envelope=sin(phase*3.14159265f);
    return sin(phase*12.5663706f)*envelope*envelope*.34f;
}
class Sprites {
    GL&g;Program program;std::unique_ptr<Program> kiteProgram,boyProgram;std::shared_ptr<ImageCache> imageCache;std::map<std::wstring,std::shared_ptr<Texture>> textures;
    Texture&image(const wchar_t*path){auto& texture=textures[path];if(!texture)texture=imageCache->image(path);return *texture;}
public:
    void boy(const View&view,V2 position,float s,float phase,float gait,float time,float tug,float shade,float scaleX,float scaleY){
        if(!boyProgram)boyProgram=std::make_unique<Program>(g,desktopShader(assetText(assetsRoot()/L"wallpaper/shaders/meadow-boy-native.frag")));
        auto&p=*boyProgram;p.use();auto&front=image(L"wallpaper/assets/meadow-kite-boy-level-face.png");auto&run=image(L"wallpaper/assets/meadow-kite-boy-diagonal-run.png");front.bind(g,5);run.bind(g,6);p.i("uCharacter",5);p.i("uRunSprite",6);
        p.v2("uRes",view.worldW,view.worldH);p.v2("wpOrigin",view.x,view.worldH-view.y-view.h);p.v2("wpPixelScale",scaleX,scaleY);p.v2("uOffset",0,0);p.f("uScale",1);p.f("uTime",time);p.v4("uPlace",{position.x,position.y,s,0});
        float bob=-(1-cos(phase*2))*.65f*gait;bool running=gait>.05f;
        p.v4("uMotion",{3,running?1.f:0.f,1,bob});p.v4("uRig",{gait,-sin(phase)*.42f*gait,(sin(phase)*.26f+sin(phase*2)*.035f)*gait,0});
        // Two-bone inverse kinematics audited from meadowBoyRunningRig. Stride
        // phase is integrated from distance, so planted feet match velocity.
        struct Leg {V2 hip,knee,ankle;float angle,lift;bool contact;};
        auto solve=[&](V2 hip,V2 knee,V2 ankle,float offset,V2 front,V2 back,float lift,float kick){
            float u=phase/6.283185f+offset;u-=floor(u);bool swing=u>=.5f;float t=swing?(u-.5f)*2:u*2,recovery=std::max(0.f,(t-.18f)/.82f),step=swing?recovery*recovery*(3-2*recovery):t,fold=swing?pow(sin(t*3.14159f),2):0;
            V2 a=swing?back:front,b=swing?front:back;float x=a.x+(b.x-a.x)*step+kick*fold,y=a.y+(b.y-a.y)*step,height=swing?std::min(fold*lift,std::max(0.f,y-5))*gait:0;
            V2 foot{ankle.x+(hip.x+x-ankle.x)*gait,ankle.y+(hip.y+y-ankle.y)*gait-bob-height};
            float upper=(knee-hip).length(),lower=(ankle-knee).length(),distance=std::clamp((foot-hip).length(),abs(upper-lower)+.001f,upper+lower-.001f);V2 axis=unit(foot-hip);foot=hip+axis*distance;
            float along=(upper*upper-lower*lower+distance*distance)/(2*distance),bend=sqrt(std::max(0.f,upper*upper-along*along));
            return Leg{hip,hip+axis*along+V2{-axis.y,axis.x}*bend,foot,-fold*1.05f*gait,height,!swing};
        };
        auto left=solve({-12,37},{-14,47},{-15,57},0,{-7.5f,8.5f},{1.4f,19},19,9),right=solve({14,35},{18,44},{21,51},.5f,{.5f,7.2f},{7,15.8f},16,5);
        p.v4("uLeftHipKnee",{left.hip.x,left.hip.y,left.knee.x,left.knee.y});p.v4("uRightHipKnee",{right.hip.x,right.hip.y,right.knee.x,right.knee.y});p.v2("uLeftAnkle",left.ankle.x,left.ankle.y);p.v2("uRightAnkle",right.ankle.x,right.ankle.y);p.v2("uFootAngles",left.angle,right.angle);
        p.v2("uBody",sin(time*.85f)*1.7f,sin(time*1.7f)*.65f);p.v2("uLegs",std::max(0.f,sin(time*1.05f))*3.8f,std::max(0.f,-sin(time*1.05f))*3.2f);
        float arm=(sin(phase)*.26f+sin(phase*2)*.035f)*gait;V2 hand=running?V2{22+cos(arm)*15+sin(arm)*16,-7+sin(arm)*15-cos(arm)*16-10+bob}:V2{-49-tug*12,-45.5f-tug*8};
        p.v2("uHand",hand.x,hand.y);p.v2("uReelHand",hand.x,hand.y);p.v2("uTug",-tug*12,-tug*8);p.v4("uPose",{0,sin(time*.9f)*1.5f,-.18f-tug*.22f,0});
        p.v2("uKite",215+sin(time*.72f)*21+sin(time*1.23f)*5,205+sin(time*.49f+.6f)*10);p.f("uRunWind",gait);p.f("uTension",tug);p.f("uKiteRock",tug*.1f);
        V2 soleL=running?left.ankle+V2{-sin(left.angle)*15.94f-.52f*cos(left.angle),-.52f*sin(left.angle)+15.94f*cos(left.angle)+bob-10}:V2{-22.5f,72};
        V2 soleR=running?right.ankle+V2{-sin(right.angle)*15.63f-3.42f*cos(right.angle),-3.42f*sin(right.angle)+15.63f*cos(right.angle)+bob-10}:V2{37.8f,63.3f};
        float slope=running?-.19f:(soleR.y-soleL.y)/(soleR.x-soleL.x),intercept=running?(left.contact?soleL.y-slope*soleL.x:soleR.y-slope*soleR.x):(soleL.y+soleR.y-slope*(soleL.x+soleR.x))*.5f;
        p.v2("uGround",slope,intercept);p.v4("uContactL",{soleL.x,soleL.y-.5f,running&&!left.contact?.22f:1.f,0});p.v4("uContactR",{soleR.x,soleR.y-.5f,running&&!right.contact?.22f:1.f,0});p.f("wpShadowAlpha",shade);
        glEnable(GL_BLEND);glBlendFunc(GL_SRC_ALPHA,GL_ONE_MINUS_SRC_ALPHA);
        for(float shadow:{1.f,0.f}){p.f("uShadow",shadow);glDrawArrays(GL_TRIANGLES,0,3);}glDisable(GL_BLEND);
    }
    void anchored(const wchar_t*path,const View&view,const SpriteTransform&transform,float time,float sx,float sy,bool shadow=false,float alpha=1,float shear=.3f,float tailAngle=0){auto b=transform.bounds();draw(path,view,b.x,b.y,b.w,b.h,time,sx,sy,transform.mirror,shadow,alpha,shear,-1,transform.anchorY,tailAngle);}
    void kite(const View&view,float handX,float handY,float x,float y,float scale,float time,float reaction,float scaleX,float scaleY){
        if(!kiteProgram)kiteProgram=std::make_unique<Program>(g,assetText(assetsRoot()/L"wallpaper/shaders/meadow-kite-native.frag"));
        auto&p=*kiteProgram;p.use();p.v2("world",view.worldW,view.worldH);p.v2("origin",view.x,-(view.worldH-view.y-view.h));p.v2("pixelScale",scaleX,scaleY);p.v2("hand",handX,handY);p.v2("kite",x,y);p.f("scale",scale);p.f("time",time);p.f("reaction",reaction);
        glEnable(GL_BLEND);glBlendFunc(GL_SRC_ALPHA,GL_ONE_MINUS_SRC_ALPHA);glDrawArrays(GL_TRIANGLES,0,3);glDisable(GL_BLEND);
    }
    explicit Sprites(GL&gl,std::shared_ptr<ImageCache> cache={}):g(gl),program(gl,R"(#version 330 core
uniform sampler2D sprite;uniform vec2 origin,world,pixelScale;uniform vec4 rect,uvRect;uniform float time,mirror,shadow,alpha,shear,anchorY,kitten,tailAngle;
out vec4 o;
float tailMask(vec2 uv){return smoothstep(.635,.665,uv.x)*smoothstep(.715,.75,uv.y);}
vec4 kittenColor(vec2 uv){vec2 pivot=vec2(.68,.73),delta=uv-pivot;float a=-tailAngle*smoothstep(.025,.16,length(delta));float c=cos(a),s=sin(a);vec2 source=pivot+vec2(c*delta.x-s*delta.y,s*delta.x+c*delta.y);vec4 body=texture(sprite,uv),tail=texture(sprite,source);body.a*=1.-tailMask(uv);tail.a*=tailMask(source);float opacity=body.a+tail.a;return vec4((body.rgb*body.a+tail.rgb*tail.a)/max(opacity,.001),min(opacity,1.));}
void main(){vec2 p=vec2(gl_FragCoord.x/pixelScale.x,world.y-gl_FragCoord.y/pixelScale.y)+origin;vec2 uv=(p-rect.xy)/rect.zw;if(shadow>.5){float height=(uv.y-anchorY)*2.8;uv.x-=shear*height*rect.w/rect.z;uv.y=anchorY-height;}else if(kitten<.5)uv.x+=sin(time*1.4+uv.y*2.)*.006*pow(1.-uv.y,2.);if(mirror>.5)uv.x=1.-uv.x;if(any(lessThan(uv,vec2(0)))||any(greaterThan(uv,vec2(1))))discard;vec4 c=kitten>.5?kittenColor(clamp(uv,.001,.999)):texture(sprite,uvRect.xy+clamp(uv,.001,.999)*uvRect.zw);o=vec4(shadow>.5?vec3(.12,.20,.18):c.rgb,c.a*alpha);})"),imageCache(cache?std::move(cache):std::make_shared<ImageCache>(gl)){}
    void draw(const wchar_t*path,const View&view,float x,float y,float w,float h,float time,float scaleX,float scaleY,bool mirror=false,bool shadow=false,float alpha=1,float shear=.3f,int atlasFrame=-1,float anchorY=1,float tailAngle=0){
        auto&texture=image(path);texture.bind(g,5);program.use();program.i("sprite",5);program.v2("world",view.worldW,view.worldH);program.v2("origin",view.x,-(view.worldH-view.y-view.h));program.v2("pixelScale",scaleX,scaleY);program.v4("rect",{x,y,w,h});program.f("time",time);program.f("mirror",mirror?1.f:0.f);program.f("shadow",shadow?1.f:0.f);program.f("alpha",alpha);program.f("shear",shear);
        program.f("kitten",std::wstring_view(path)==L"wallpaper/assets/meadow-black-kitten.png"?1.f:0.f);program.f("tailAngle",tailAngle);
        program.f("anchorY",anchorY);if(atlasFrame<0)program.v4("uvRect",{0,0,1,1});else program.v4("uvRect",{float(atlasFrame%2)*.5f,float(atlasFrame/2)*.5f,.5f,.5f});
        const float margin=shadow?w:1;int left=int((x-view.x-margin)*scaleX),right=int((x+w-view.x+margin)*scaleX),bottom=int((view.y+view.h-y-h*(shadow?1.5f:1.f)-1)*scaleY),top=int((view.y+view.h-y+1)*scaleY);
        left=std::clamp(left,0,int(view.w*scaleX));right=std::clamp(right,0,int(view.w*scaleX));bottom=std::clamp(bottom,0,int(view.h*scaleY));top=std::clamp(top,0,int(view.h*scaleY));
        if(right<=left||top<=bottom)return;
        glEnable(GL_SCISSOR_TEST);glScissor(left,bottom,right-left,top-bottom);glEnable(GL_BLEND);glBlendFunc(GL_SRC_ALPHA,GL_ONE_MINUS_SRC_ALPHA);glDrawArrays(GL_TRIANGLES,0,3);glDisable(GL_BLEND);glDisable(GL_SCISSOR_TEST);
    }
};
}
