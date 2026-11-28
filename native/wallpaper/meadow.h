// Audited pose and elastic kite response from prototype/meadow-boy.js.
// CPU state belongs to the existing SceneRuntime/fixed-step scheduler only.
#pragma once
#include "spatial.h"
namespace wallpaper {
inline float meadowEase(float x){x=std::clamp(x,0.f,1.f);return x*x*(3-2*x);}
struct MeadowPose {
    V2 body,legs,tugOffset,hand;
    float reel=-.18f,nod=0,linePull=0;
    bool tugActive=false;
};
inline MeadowPose meadowPose(float time,float tugAge){
    MeadowPose p;p.tugActive=tugAge>=0&&tugAge<1.12f;
    float reach=0,pull=0;
    if(p.tugActive){reach=meadowEase(tugAge/.16f)*(1-meadowEase((tugAge-.16f)/.20f));
        pull=meadowEase((tugAge-.16f)/.20f)*(1-meadowEase((tugAge-.50f)/.62f));}
    const float wave=std::clamp((float(fmod(time,9))-4)/.6f,0.f,1.f);
    const float wavePull=p.tugActive?0:pow(sin(3.14159265f*wave),2);
    p.linePull=std::max(pull,wavePull);
    p.body={sin(time*.85f)*1.7f-pull*1.8f,sin(time*1.7f)*.65f-pull*.8f};
    const float shift=sin(time*1.05f);p.legs={std::max(0.f,shift)*3.8f,std::max(0.f,-shift)*3.2f};
    p.tugOffset={reach*4-pull*12,reach*3-pull*8};
    p.reel=-.18f+(p.tugActive?-pull*.22f:wavePull*1.14f);p.nod=sin(time*.9f+.4f)*1.5f;
    // Invert the actual shader's arm/body warp at the artwork grip. Merely
    // offsetting the old hand point detaches the string while the elbow bends.
    const float a=p.reel+.18f,c=cos(a),s=sin(a);
    const V2 q{-21+c*(-49+21)-s*(-45.5f+9)-p.tugOffset.x*.35f,
        -9+s*(-49+21)+c*(-45.5f+9)-p.tugOffset.y*.35f};
    const float y=(q.y+p.body.y*78/170)/(1+p.body.y/170);
    p.hand={q.x+p.body.x*(78-y)/170,y};return p;
}
struct MeadowKitePose {V2 offset;float rock=0,tension=0,wind=0;};
struct MeadowKite {
    V2 followed,reaction,velocity;float angle=0,spin=0,lastPull=0,lastScale=0;bool initialized=false;
    static V2 base(float time,float wind){return {215+sin(time*.72f)*21+sin(time*1.23f)*5+(sin(time*4.8f)*60+sin(time*7.1f)*8)*wind,
        205+sin(time*.49f+.6f)*10};}
    void step(float time,float dt,V2 position,float scale,float wind,float pull){
        const V2 desired=position+base(time,wind)*scale;
        if(!initialized||lastScale!=scale){followed=desired;lastScale=scale;initialized=true;}
        if(dt<=0)return;
        const float impulse=std::max(0.f,pull-lastPull);lastPull=pull;
        velocity.x-=100*impulse;velocity.y-=120*impulse;spin+=2.8f*impulse;
        velocity.x+=(-18*reaction.x-3.4f*velocity.x)*dt;
        velocity.y+=(-22*reaction.y-4*velocity.y)*dt;
        spin+=(-24*angle-3.8f*spin)*dt;
        reaction=reaction+velocity*dt;angle+=spin*dt;
        followed=followed+(desired-followed)*(1-exp(-dt/.23f));
    }
    MeadowKitePose pose(float time,V2 position,float scale,float wind,float pull)const{
        const V2 b=base(time,wind),desired=position+b*scale;
        const V2 lag=initialized?(followed-desired)*(1/std::max(.001f,scale)):V2{};
        return {{b.x+std::clamp(lag.x,-30.f,32.f)+reaction.x-pull*10,
            b.y+std::clamp(lag.y,-30.f,32.f)+reaction.y-pull*12},angle+pull*.10f,pull,wind};
    }
};
} // namespace wallpaper
