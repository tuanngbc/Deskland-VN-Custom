// Wallpaper-only coordinates. One world/logical unit is one physical pixel;
// DPI describes the display, never multiplies input or render coordinates.
#pragma once
#include "model.h"
namespace wallpaper {
struct V2 {
    float x=0,y=0;
    V2 operator+(V2 b)const{return {x+b.x,y+b.y};}
    V2 operator-(V2 b)const{return {x-b.x,y-b.y};}
    V2 operator*(float k)const{return {x*k,y*k};}
    float length()const{return hypot(x,y);}
};
inline V2 unit(V2 p){float n=p.length();return n>.0001f?p*(1/n):V2{1,0};}
inline float angleDelta(float a,float b){return atan2(sin(b-a),cos(b-a));}
struct Box {
    float x=0,y=0,w=0,h=0;
    bool contains(V2 p,float pad=0)const{return p.x>=x+pad&&p.y>=y+pad&&p.x<=x+w-pad&&p.y<=y+h-pad;}
    bool operator==(const Box&)const=default;
};
inline bool intersects(Box a,Box b,float pad=0){return a.x<b.x+b.w+pad&&a.x+a.w+pad>b.x&&a.y<b.y+b.h+pad&&a.y+a.h+pad>b.y;}
struct Geometry {
    float referenceW=1080,referenceH=700;
    std::vector<Box> outputs;
    bool operator==(const Geometry&)const=default;
    bool contains(V2 p,float radius=0)const{return covers({p.x-radius,p.y-radius,radius*2+.001f,radius*2+.001f});}
    bool covers(Box b)const {
        std::vector<Box> remaining{b};
        for(auto o:outputs){std::vector<Box> next;
            for(auto r:remaining){float l=std::max(r.x,o.x),t=std::max(r.y,o.y),e=std::min(r.x+r.w,o.x+o.w),d=std::min(r.y+r.h,o.y+o.h);
                if(l>=e||t>=d){next.push_back(r);continue;}
                for(auto q:std::array<Box,4>{{{r.x,r.y,r.w,t-r.y},{r.x,d,r.w,r.y+r.h-d},{r.x,t,l-r.x,d-t},{e,t,r.x+r.w-e,d-t}}})if(q.w>.001f&&q.h>.001f)next.push_back(q);
            }remaining=std::move(next);if(remaining.empty())return true;
        }return remaining.empty();
    }
    V2 home(V2 p)const {for(auto o:outputs)if(o.contains(p))return {o.x+o.w*.5f,o.y+o.h*.5f};return outputs.empty()?V2{}:V2{outputs[0].x+outputs[0].w*.5f,outputs[0].y+outputs[0].h*.5f};}
    float meadowBoyScale()const{return std::clamp(std::min(referenceW/1379.f,std::max(1.f,referenceH-48)/1141.f),.24f,1.25f)*.35f;}
    float meadowCatSize()const{return std::clamp(std::min(referenceW/1379.f,std::max(1.f,referenceH-48)/1141.f)*78,28.f,90.f)*.578f;}
    float catScale()const{return std::clamp(std::min(referenceW/1080.f,referenceH/700.f),.6f,1.8f);}
    float boxingScale()const{return std::min(referenceW/1080.f,std::max(280.f,referenceH-48)/640.f);}
    float porchScale()const{return std::min(referenceW/1536.f,referenceH/864.f);}
};
inline Geometry sceneGeometry(const std::vector<Monitor>&monitors,const Monitor&current,Layout layout){
    Geometry g;const Monitor*ref=&current;
    if(layout==Layout::Panorama)for(auto&m:monitors)if(m.primary){ref=&m;break;}
    g.referenceW=float(ref->rect.right-ref->rect.left);g.referenceH=float(ref->rect.bottom-ref->rect.top);
    RECT all=bounds(monitors);
    if(layout==Layout::Panorama){auto add=[&](const Monitor&m){g.outputs.push_back({float(m.rect.left-all.left),float(m.rect.top-all.top),float(m.rect.right-m.rect.left),float(m.rect.bottom-m.rect.top)});};add(*ref);for(auto&m:monitors)if(m.id!=ref->id)add(m);}
    else g.outputs.push_back({0,0,float(current.rect.right-current.rect.left),float(current.rect.bottom-current.rect.top)});
    return g;
}
inline bool iconClear(Box b,const std::vector<RECT>&icons,float pad=7){for(auto r:icons)if(intersects(b,{float(r.left),float(r.top),float(r.right-r.left),float(r.bottom-r.top)},pad))return false;return true;}
// The exact same anchored quad is used for drawing, bounds and hit testing.
struct SpriteTransform {
    V2 foot;float width=0,height=0,anchorX=.5f,anchorY=1;bool mirror=false;
    Box bounds()const{return {foot.x-width*(mirror?1-anchorX:anchorX),foot.y-height*anchorY,width,height};}
    V2 point(float u,float v)const {auto r=bounds();return {r.x+(mirror?1-u:u)*width,r.y+v*height};}
    bool hit(V2 p)const{return bounds().contains(p);}
};
}
