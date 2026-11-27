// Audited spring, punch timing and arcade rules from src/themes/boxing_ring.rs.
// New native scene model only: no legacy runtime, renderer or input owner.
#pragma once
#include "catalog.h"
#include "spatial.h"
namespace wallpaper {
struct RigSpring {
    float x=0,v=0,k=125,damping=5.6f;
    RigSpring(float stiffness=125,float drag=5.6f):k(stiffness),damping(drag){}
    void step(float dt,float target=0,float damp=1){v+=((target-x)*k-v*damping*damp)*dt;x+=v*dt;}
};
struct Boxer {
    RigSpring headX{125,5.6f},headY{132,5.8f},rotation{118,5.2f},squash{138,4.9f},faceX{135,5.8f},faceY{135,5.8f};
    RigSpring shear{115,5.8f},torsoSquash{128,5.8f},dent{118,5.4f},lunge{62,6.4f};
    float dentX=10,dentY=0,extension=0,weave=.2f,blinkWait=3,blink=-1,wince=0,dizzy=0,flash=0,hp=1,hpLag=1,lagHold=0,comboWindow=0;
    int combo=0;
    void stepSprings(float dt,float bounce){const float damp=std::clamp(1.35f/sqrt(bounce),.60f,1.85f);
        for(auto* spring:{&headX,&headY,&rotation,&squash,&faceX,&faceY,&shear,&torsoSquash,&dent})spring->step(dt,0,damp);
        lunge.step(dt);lunge.x=std::clamp(lunge.x,-145.f,145.f);dent.x=std::clamp(dent.x,-.35f,1.35f);
    }
};
class BoxingModel {
public:
    using Quad=std::array<float,4>;
    enum Zone {Top,Cheek,Chin,Body,Bag};
    struct Punch {bool active=false,impacted=false;float age=0;int attacker=-1,target=-1,bag=0;Zone zone=Cheek;V2 origin,point;float direction=0,peak=.92f,multiplier=1;};
    struct Puff {bool active=false;V2 point;float age=0,seed=.5f;};
    struct Uniforms {std::array<Quad,2> pos{},head{},face{},torso{},expression{};std::array<Quad,4> gloves{},gloveMeta{};std::array<Quad,6> puffs{};Quad scene{},hud{},round{},props{};};
    std::array<Boxer,2> fighters{};std::array<Punch,4> punches{};std::array<Puff,6> puffs{};
    std::array<RigSpring,3> bags{{RigSpring{24,1.85f},RigSpring{21,1.75f},RigSpring{20,1.75f}}};RigSpring ring{240,14};
    float w=1080,h=700,scale=1,bounce=1.25f,power=1,gloveStyle=0,hudMode=0;
    float roundSeconds=60,bell=0,rope=0,comboGlow=0,tire=0,sparWait=4.35f;int roundNumber=1,autoSpar=0,impacts=0;
private:
    size_t nextPunch=0,nextPuff=0;uint32_t seed=0x62ec19u;bool configured=false;
    float random(){seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;return (seed&0xffffff)/float(0x1000000);}
    void puff(V2 p){puffs[nextPuff++%puffs.size()]={true,p,0,.08f+random()*.90f};}
    void impact(const Punch& p){impacts++;puff(p.point);const float strength=power*p.multiplier;
        if(p.zone==Bag){bags[p.bag].v+=(cos(p.direction)>=0?1.f:-1.f)*(p.bag==0?3.4f:3.2f)*strength;return;}
        auto& f=fighters[p.target];const float facing=p.target==0?1.f:-1.f,push=-facing;
        f.wince=.85f;f.flash=1;f.combo=f.comboWindow>0?f.combo+1:1;f.comboWindow=1.15f;
        switch(p.zone){
            case Top:f.headY.v+=340*strength;f.squash.v-=9.2f*bounce*sqrt(strength);f.torsoSquash.v-=4.8f*bounce*sqrt(strength);f.dentX=0;f.dentY=-18;f.faceY.v+=280*bounce;f.lunge.v+=push*75*strength;break;
            case Cheek:f.headX.v+=push*420*strength;f.rotation.v+=push*11.5f*strength;f.squash.v-=4.2f*bounce;f.dentX=facing*15;f.dentY=2;f.faceX.v+=push*320*bounce*sqrt(strength);f.shear.v-=push*5.2f*strength;f.lunge.v+=push*185*strength;break;
            case Chin:f.headX.v+=push*330*strength;f.headY.v-=240*strength;f.rotation.v+=push*9.5f*strength;f.squash.v+=6.5f*bounce*sqrt(strength);f.dentX=facing*14;f.dentY=8;f.faceX.v+=push*260*bounce;f.faceY.v-=160*bounce;f.lunge.v+=push*155*strength;break;
            case Body:f.dent.v+=14.5f*bounce*sqrt(strength);f.shear.v-=facing*8.5f*strength;f.torsoSquash.v-=4.5f*bounce;f.headY.v+=165*strength;f.rotation.v-=push*5.2f*strength;f.lunge.v+=push*220*strength;break;
            default:break;
        }
        f.hp=std::max(0.f,f.hp-std::clamp(.075f*strength,.03f,.22f));f.lagHold=.45f;
        if(f.combo>=4||f.hp<=.01f){f.dizzy=1.45f;f.lunge.v+=push*140;comboGlow=1;}
        rope=std::min(7.5f,rope+(p.zone==Body?5.2f:p.zone==Top?4.2f:3.2f)*strength);ring.v+=95*strength;
        if(abs(f.lunge.x)>95){rope=std::min(8.f,rope+2.8f);auto base=foot(p.target);puff({base.x+f.lunge.x*scale,base.y-58*scale});}
    }
public:
    BoxingModel(){fighters[1].weave=2.3f;fighters[0].blinkWait=3.2f;fighters[1].blinkWait=2.8f;}
    void configure(const Config& c,float width,float height,float s){
        w=width;h=std::max(280.f,height-48);scale=std::max(.01f,s);const int scene=sceneIndex("boxing-ring");
        auto get=[&](const char* key){return float(parameter(c,scene,key));};bounce=get("bounciness");power=get("punchPower");gloveStyle=get("gloveStyle");hudMode=get("hudMode");
        const int mode=int(get("autoSpar"));if(!configured||mode!=autoSpar)sparWait=mode==2?.35f:mode==0?4.35f:9999.f;autoSpar=mode;configured=true;
    }
    V2 foot(int i)const{float facing=i==0?1.f:-1.f;return {w*.5f-facing*68*scale+sin(fighters[i].weave*.85f)*7.5f*scale,h*(i==0?.758f:.766f)};}
    void nextRound(){roundNumber=roundNumber%12+1;roundSeconds=60;bell=1;rope=std::min(6.f,rope+2.4f);for(auto& f:fighters){f.hp=f.hpLag=1;f.dizzy=0;f.combo=0;}}
    void launch(int attacker,int target,Zone zone,V2 click,bool clicked,float multiplier=1){
        auto a=foot(attacker),b=foot(target);const float facing=attacker==0?1.f:-1.f;
        a.x+=fighters[attacker].lunge.x*scale;b.x+=fighters[target].lunge.x*scale;
        const float headX=b.x+fighters[target].headX.x*scale,headY=b.y-115*scale+fighters[target].headY.x*scale,torsoY=b.y-65*scale;
        V2 point,origin;
        if(zone==Top){point={clicked?std::clamp(click.x,headX-24*scale,headX+24*scale):headX,headY-18*scale};origin={point.x-facing*72*scale,point.y-148*scale};}
        else if(zone==Cheek){point={headX-facing*12*scale,clicked?std::clamp(click.y,headY-14*scale,headY+14*scale):headY-2*scale};origin={a.x-facing*38*scale,point.y+26*scale};}
        else if(zone==Chin){point={headX-facing*10*scale,headY+8*scale};origin={a.x+facing*8*scale,a.y-78*scale};}
        else{point={b.x-facing*14*scale,clicked?std::clamp(click.y,torsoY-16*scale,torsoY+20*scale):torsoY};origin={a.x-facing*24*scale,torsoY+34*scale};}
        auto& f=fighters[attacker];f.extension=1;f.lunge.v+=facing*135*multiplier;f.shear.v-=facing*3.8f*multiplier;
        Punch p;p.active=true;p.attacker=attacker;p.target=target;p.zone=zone;p.origin=origin;p.point=point;p.direction=atan2(point.y-origin.y,point.x-origin.x);p.peak=(.86f+.12f*std::min(power,1.6f))*sqrt(multiplier);p.multiplier=multiplier;punches[nextPunch++%punches.size()]=p;
    }
    void click(V2 point){const float cx=w*.5f,hs=scale*(hudMode==1?.78f:1.f);
        if(hudMode<2&&abs(point.x-cx)<54*hs&&abs(point.y-44*hs)<36*hs){nextRound();puff({cx,44*hs});return;}
        if(abs(point.x-(cx-420*scale))<78*scale&&abs(point.y-h*.845f)<54*scale){tire=.01f;ring.v+=55;puff({cx-420*scale,h*.845f-10*scale});return;}
        const std::array<V2,3> targets{{{cx-388*scale,h*.18f+82*scale},{cx-448*scale,h*.29f+86*scale},{cx+425*scale,h*.24f+88*scale}}};
        for(int i=0;i<3;++i)if(abs(point.x-targets[i].x)<40*scale&&abs(point.y-targets[i].y)<78*scale){
            Punch p;p.active=true;p.zone=Bag;p.bag=i;p.point=targets[i];p.origin={p.point.x-(i==2?-1.f:1.f)*96*scale,p.point.y+18*scale};p.direction=atan2(p.point.y-p.origin.y,p.point.x-p.origin.x);p.peak=.78f;punches[nextPunch++%punches.size()]=p;return;}
        const int target=point.x>=cx?1:0,attacker=1-target;auto base=foot(target);base.x+=fighters[target].lunge.x*scale;
        const float relX=(point.x-base.x)/scale,relY=(point.y-(base.y-115*scale))/scale;
        Zone zone=relY<-14?Top:relY<26?(abs(relX)>14?Cheek:Chin):Body;
        if(autoSpar==0)sparWait=std::max(sparWait,1.8f);launch(attacker,target,zone,point,true);
    }
    void step(float dt){
        bell=std::max(0.f,bell-dt*2.2f);comboGlow=std::max(0.f,comboGlow-dt*1.4f);rope=std::max(0.f,rope-dt*4.8f);if(tire>0){tire+=dt*1.85f;if(tire>=1)tire=0;}
        roundSeconds-=dt*.65f;if(roundSeconds<=0)nextRound();
        if(autoSpar!=1){sparWait-=dt;if(sparWait<=0){int attacker=random()<.52f?0:1;float zone=random();if(fighters[attacker].dizzy<=.05f)launch(attacker,1-attacker,zone<.34f?Chin:zone<.68f?Cheek:zone<.88f?Body:Top,{},false,autoSpar==2?.88f:.62f);sparWait=autoSpar==2?.26f+random()*.32f:2.6f+random()*2.1f;}}
        for(auto& f:fighters)f.stepSprings(dt,bounce);const float wind=sin(fighters[0].weave*.45f)*.018f;
        for(int i=0;i<3;++i)bags[i].step(dt,wind*(i==0?1.f:i==1?-.8f:.9f));ring.step(dt);
        for(int i=0;i<2;++i){auto& f=fighters[i];bool dizzy=f.dizzy>0;f.weave+=dt*(dizzy?7.5f:3.6f);f.extension=std::max(0.f,f.extension-dt*4.6f);f.wince=std::max(0.f,f.wince-dt);f.dizzy=std::max(0.f,f.dizzy-dt);f.flash=std::max(0.f,f.flash-dt*6.5f);f.comboWindow=std::max(0.f,f.comboWindow-dt);if(f.comboWindow<=0)f.combo=0;
            if(dizzy&&f.dizzy<=0&&f.hp<.25f){f.hp=f.hpLag=1;f.rotation.v+=i==0?6:-6;}else if(f.dizzy<=0&&f.comboWindow<=0&&f.hp<1)f.hp=std::min(1.f,f.hp+dt*.025f);
            if(f.lagHold>0)f.lagHold-=dt;else if(f.hpLag>f.hp)f.hpLag=std::max(f.hp,f.hpLag-dt*.55f);else f.hpLag=f.hp;
            if(f.blink>=0){f.blink+=dt*7.5f;if(f.blink>=1)f.blink=-1;}else{f.blinkWait-=dt;if(f.blinkWait<=0){f.blink=0;f.blinkWait=2.4f+random()*3.4f;}}
        }
        for(auto& p:punches)if(p.active){p.age+=dt;if(!p.impacted&&p.age>=.09f){p.impacted=true;impact(p);}if(p.age>=.68f)p.active=false;}
        for(auto& p:puffs)if(p.active){p.age+=dt*1.05f;if(p.age>=1)p.active=false;}
    }
    Uniforms uniforms(float time)const{Uniforms u;
        for(int i=0;i<2;++i){const auto& f=fighters[i];auto base=foot(i);float sway=f.dizzy>0?sin(time*10+i)*7.5f:0;
            u.pos[i]={base.x,base.y,abs(sin(f.weave*1.7f))*4.2f-2,f.lunge.x+sway};u.head[i]={f.headX.x+sway*.45f,f.headY.x,f.rotation.x*.085f+sway*.02f,std::clamp(f.squash.x,-.58f,.58f)};
            u.face[i]={f.dentX,f.dentY,f.faceX.x,f.faceY.x};u.torso[i]={std::clamp(f.shear.x,-.85f,.85f),std::clamp(f.torsoSquash.x,-.45f,.45f),std::clamp(f.dent.x,-.35f,1.25f),f.extension};u.expression[i]={f.blink>=0?sin(f.blink*3.14159f):0.f,std::clamp(f.wince*3.2f,0.f,1.f),std::min(1.f,f.dizzy),f.flash};
        }
        for(size_t i=0;i<punches.size();++i){const auto& p=punches[i];if(!p.active)continue;V2 point;float alpha,phase;
            if(p.age<.09f){float k=std::clamp(p.age/.09f,0.f,1.f),ease=1-std::pow(1-k,3);point=p.origin+(p.point-p.origin)*ease;alpha=p.peak*(.45f+.55f*ease);phase=k;}
            else if(p.age<.21f){float k=(p.age-.09f)/.12f,over=sin(k*3.14159f)*11;point=p.point+V2{cos(p.direction),sin(p.direction)}*over;alpha=p.peak*(1+.18f*sin(k*3.14159f));phase=1;}
            else{float k=std::clamp((p.age-.21f)/.47f,0.f,1.f);point=p.point+(p.origin-p.point)*(k*k*.78f);alpha=p.peak*(1-k);phase=1.1f+k;}
            u.gloves[i]={point.x,point.y,p.direction,alpha};u.gloveMeta[i]={p.origin.x,p.origin.y,phase,p.zone==Top?1.f:p.zone==Chin?2.f:0.f};
        }
        for(size_t i=0;i<puffs.size();++i){const auto& p=puffs[i];if(p.active)u.puffs[i]={p.point.x,p.point.y,p.age,p.seed};}
        u.scene={gloveStyle,hudMode,ring.x,rope};u.hud={fighters[0].hp,fighters[0].hpLag,fighters[1].hp,fighters[1].hpLag};u.round={float(roundNumber),roundSeconds,bell,comboGlow};u.props={bags[0].x,bags[1].x,bags[2].x,tire};return u;
    }
};
} // namespace wallpaper
