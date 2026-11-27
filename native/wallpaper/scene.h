// Scene behavior is newly implemented against audited GLSL uniform contracts.
// No Rust scene object, legacy scheduler, or old desktop owner is reused.
#pragma once
#include "catalog.h"
#include "field.h"
#include "images.h"
#include <memory>
#include <chrono>

namespace wallpaper {
using Vec4=std::array<float,4>;
inline float wrap(float v,float size){return size>0?v-floor(v/size)*size:0;}
struct Spring {float x=0,v=0;void step(float dt,float stiffness=180,float damping=17){int n=std::max(1,int(ceil(dt/.012f)));for(int i=0;i<n;i++){float h=dt/n;v+=(-stiffness*x-damping*v)*h;x+=v*h;}}};
struct Cat {float x=0,y=0,tx=0,ty=0,phase=0,wait=0,happy=0;int pose=0,coat=0;bool right=true;};
struct ClickFx {Vec4 value{0,0,100,0};};
class SceneRuntime {
    GL&g;int index;Program program;Field field;Clouds clouds;std::unique_ptr<Sprites> sprites;
    std::vector<RECT> previousIcons;float previousW=0,previousH=0,previousSigma=-1,previousWarp=-1;
    std::array<Cat,15> cats{};std::array<ClickFx,8> effects{};size_t effectNext=0;float age=0;
    Vec4 toy{0,0,0,0};float fishing=0,fishingAge=0;POINT fishingPoint{};
    std::array<Spring,2> head{},torso{};Spring ring,bag;
    std::array<float,2> hp{1,1},hpLag{1,1};int hitActor=-1;float hitAge=100,hitX=0,hitY=0,autoPunch=0;int melonBites=0;float fanMultiplier=1;
    float ballX=.5f,ballY=.5f,targetX=.5f,targetY=.5f;int passes=0;float passAge=0;
    Config lastConfig;double lastTime=0;float goalSide=0,goalAge=100,userChase=0;
    std::array<int,4> corgiPose{};std::array<float,4> corgiHappy{};Spring chime;
    float birdHop=100,melonRefill=0,fanPhase=0,tireAge=100;int roundNumber=1,bagIndex=0,hitZone=0;
    float roundAge=0,roundBell=0;std::array<Spring,3> bags{};
    uint32_t rng=0x731f91u;
    float random(){rng^=rng<<13;rng^=rng>>17;rng^=rng<<5;return float(double(rng)/4294967296.);}
    static std::string source(int id){const auto&s=catalog().at(id);
        if(std::string(s.id)=="cats"){
            auto artwork=assetText(assetsRoot()/L"wallpaper/shaders/cat_hill.frag");auto main=artwork.find("void main()");if(main==std::string::npos)throw std::runtime_error("Cat artwork contract missing");
            artwork.resize(main);artwork+=assetText(assetsRoot()/L"wallpaper/shaders/cats-native.frag");return desktopShader(std::move(artwork));
        }return desktopShader(assetText(assetsRoot()/s.shader));
    }
    template<size_t N>void array(const char*name,const std::array<Vec4,N>&a){program.v4(name,a.front().data(),int(N));}
    float p(const Config&c,const char*key,float fallback=0)const{for(const auto&d:catalog()[index].params)if(std::string(key)==d.key)return float(parameter(c,index,key));return fallback;}
    template<size_t N>void flyers(const char*uniform,const char*count,int n,float w,float h,float t,float speed,float y0=.1f,float y1=.85f){
        std::array<Vec4,N> values{};n=std::clamp(n,0,int(N));for(int i=0;i<n;i++){
            float phase=hash(i,index,201)*6.283185f,dir=i%2?1.f:-1.f;
            float x=wrap(hash(i,index,210)*(w+160)+t*speed*dir,w+160)-80;
            float y=h*(y0+(y1-y0)*hash(i,index,211))+sin(t*.3f+phase)*std::min(h*.08f,55.f);
            values[i]={x,y,dir>0?.08f:3.05f,.68f+.32f*cos(t*7+phase)};
        }program.f(count,float(n));array(uniform,values);
    }
    bool obstacle(float x,float y,const std::vector<RECT>&icons)const{for(const auto&r:icons)if(x>r.left-55&&x<r.right+55&&y>r.top-15&&y<r.bottom+65)return true;return false;}
public:
    SceneRuntime(GL&gl,int id):g(gl),index(id),program(gl,source(id)),field(gl),clouds(gl){rng^=uint32_t(id*19001);}
    int scene()const{return index;}
    void click(float x,float y){auto&fx=effects[effectNext++%effects.size()];fx.value={x,y,0,.1f+random()*.9f};
        const std::string id=catalog()[index].id;
        if(id=="sky"||id=="meadow"||id=="ocean"||id=="lake")clouds.burst(x,y,lastTime);
        if(id=="cat-hill"||id=="cats"){bool pet=false;for(auto&cat:cats)if(hypot(cat.x-x,cat.y-y)<80){cat.happy=2.5f;cat.wait=2;cat.pose=(cat.pose+1)%3;pet=true;}if(!pet)toy={x,y,0,10};}
        if(id=="lake"||id=="aquarium"){if(fishing==0){fishing=1;fishingAge=0;fishingPoint={LONG(x),LONG(y)};}else if(fishing==3){fishing=4;fishingAge=0;}else{fishing=5;fishingAge=0;}}
        if(id=="boxing-ring")clickBoxing(x,y);
        if(id=="stadium"&&previousW>0&&previousH>0){targetX=std::clamp(x/previousW,.08f,.92f);targetY=std::clamp(y/previousH,.2f,.88f);passAge=0;passes++;userChase=4;goalSide=0;
            if(abs(y/previousH-.54f)<.13f&&(x<previousW*.12f||x>previousW*.88f)){goalSide=x<previousW*.5f?-1.f:1.f;targetX=goalSide<0?.10f:.90f;targetY=.54f;goalAge=-1;}}
        if(id=="summer-porch")clickPorch(x,y,fx);
    }
    void render(const Config&c,const View&view,const std::vector<RECT>&icons,const Weather&weather,double time,float dt,const std::atomic<bool>&stop,float scaleX=1,float scaleY=1){
        const float w=view.worldW,h=view.worldH,t=float(time);lastConfig=c;lastTime=time;age+=dt;hitAge+=dt;toy[2]+=dt;toy[3]=std::max(0.f,toy[3]-dt);
        for(auto&e:effects)e.value[2]+=dt;
        for(auto&s:torso)s.step(dt);ring.step(dt);bag.step(dt);chime.step(dt,18,2.2f);for(auto&s:bags)s.step(dt,15,1.6f);
        const float sigma=p(c,"sigma",72),warp=p(c,"warp",34);
        bool iconsChanged=icons.size()!=previousIcons.size();if(!iconsChanged)for(size_t i=0;i<icons.size();i++)if(!EqualRect(&icons[i],&previousIcons[i])){iconsChanged=true;break;}
        if(w!=previousW||h!=previousH||sigma!=previousSigma||warp!=previousWarp||iconsChanged){
            field.generate(w,h,icons,sigma,warp,time,stop);if(stop)return;
            if(previousW==0||previousH==0){for(size_t i=0;i<cats.size();i++){auto&cat=cats[i];cat.x=w*(.12f+random()*.76f);cat.y=h*(.38f+random()*.5f);cat.tx=cat.x;cat.ty=cat.y;cat.coat=int(i%8);cat.wait=random()*8;}}
            previousW=w;previousH=h;previousSigma=sigma;previousWarp=warp;previousIcons=icons;
        }
        program.use();program.v2("uRes",w,h);program.v2("wpOrigin",view.x,view.worldH-view.y-view.h);program.v2("wpPixelScale",scaleX,scaleY);
        program.f("uScale",1);program.f("uSeed",7341%997*.37f);program.f("uTime",t);program.f("uDebug",0);program.i("uNoise",0);field.bind(program,time);
        const std::string id=catalog()[index].id;
        if(id=="meadow"||id=="sky"||id=="ocean"||id=="lake"){clouds.update(w,h,id=="lake"?p(c,"clouds",4)/8:p(c,"cover",.25f),p(c,"flow",1),time);clouds.bind(program);}
        program.f("uEdge",10);program.f("uSpeed",id=="beach-turquoise"?21:22);program.f("uRunup",120);program.f("uShallow",p(c,"shallow",260));program.f("uShoal",p(c,"shoal",110));program.f("uGrain",1);program.f("uWave",1.6f);program.f("uSigma",sigma);program.f("uSpacing",70);program.f("uWaves",1);program.f("uDepth",p(c,"depth",1));program.f("uFlow",p(c,"flow",1));
        program.f("uWind",p(c,"wind",c.wind));program.f("uRain",p(c,"rain",1));program.f("uMist",p(c,"mist",.4f));program.f("uBloom",p(c,"bloom",1));program.f("uPetals",p(c,"petals",1));
        SYSTEMTIME clock{};GetLocalTime(&clock);float hour=c.clockLighting?clock.wHour+clock.wMinute/60.f:c.manualHour;
        Sun sun;if(c.clockLighting&&weather.available){FILETIME ft;GetSystemTimeAsFileTime(&ft);ULARGE_INTEGER u;u.LowPart=ft.dwLowDateTime;u.HighPart=ft.dwHighDateTime;sun=solar(double(u.QuadPart)/1e7-11644473600.,weather.latitude,weather.longitude);}else{float a=(hour/24-.25f)*6.283185f;sun={cos(a),.35f,sin(a)};}
        float shade=c.solar&&sun.up>0?std::clamp(sun.up/.15f,0.f,1.f)*.34f*(1-weather.cloud*.85f):0;
        program.v2("uShadowOffset",std::clamp(-sun.east/std::max(sun.up,.18f),-1.15f,1.15f)*18,std::clamp(sun.north/std::max(sun.up,.18f)*.45f,-.2f,.65f)*18);program.f("uShadowAlpha",shade);
        std::array<Vec4,6> fx{};for(size_t i=0;i<fx.size();i++){const auto&v=effects[(effectNext+effects.size()-1-i)%effects.size()].value;if(v[2]<5)fx[i]=v;}
        array("uRip",fx);array("uBurst",fx);auto allFx=effectsAsFx();program.v4("uFx",allFx.front().data(),id=="aquarium"?8:6);program.f("uShake",fx[0][3]?exp(-fx[0][2]*3):0);program.f("uTreeShake",fx[0][3]?exp(-fx[0][2]*3):0);
        std::array<Vec4,4> sand{};for(size_t i=0;i<sand.size();i++)sand[i]=fx[i];array("uSand",sand);
        flyers<8>("uBird","uBirdN",int(p(c,"birds",3)),w,h,t,35);
        flyers<4>("uKay","uKayN",int(p(c,"kayaks",0)),w,h,t,13,.25f,.8f);
        flyers<6>("uFish","uFishN",int(p(c,"fishSchools",id=="lake"?3:0)),w,h,t,19,.3f,.85f);
        flyers<4>("uTurt","uTurtN",int(p(c,"turtles",0)),w,h,t,8,.3f,.8f);
        flyers<6>("uBoat","uBoatN",int(p(c,"boats",2)),w,h,t,15,.2f,.8f);
        flyers<4>("uYacht","uYachtN",int(p(c,"yachts",1)),w,h,t,11,.3f,.9f);
        flyers<8>("uSwallow","uSwallowN",int(p(c,"swallows",3)),w,h,t,45);
        std::array<Vec4,8> lanterns{};int nl=std::clamp(int(p(c,"lanterns",4)),0,8);for(int i=0;i<nl;i++)lanterns[i]={w*(.12f+i*.11f),h*(.12f+hash(i,index,61)*.15f),t*.8f+i,sin(t+i)*p(c,"wind",1)*.1f};program.f("uLanternN",float(nl));array("uLantern",lanterns);
        std::array<Vec4,15> flies{};int nf=std::clamp(int(p(c,"fireflies",8)),0,15);for(int i=0;i<nf;i++)flies[i]={w*hash(i,index,60)+sin(t*.3f+i)*30,h*(.2f+.6f*hash(i,index,61))+cos(t*.5f+i)*25,t+i,.5f+.5f*sin(t*2+i)};program.f("uFlyN",float(nf));array("uFly",flies);
        if(id=="sky"){float setting=p(c,"time",0);float phase=setting==0?(hour<6||hour>19?3:hour>16?2:1):setting;program.f("uPhase",phase);program.f("uMoon",p(c,"moon",1));program.f("uStars",p(c,"stars",.6f));program.v4("uPlane",{wrap(t*(p(c,"plane",1)==2?85:52),w+500)-200,h*.18f,1,p(c,"plane",1)>0?220.f:0.f});std::array<Vec4,4> meteors{};if(phase>2.5f)for(int i=0;i<4;i++)if(fx[i][3]>0&&fx[i][2]<1.4f)meteors[i]={fx[i][0]-35,fx[i][1]-25,fx[i][2],fx[i][0]>w*.55f?2.55f:.60f};array("uMeteor",meteors);}
        if(id=="cat-hill"||id=="cats")renderCats(c,icons,w,h,t,dt);
        if(id=="aquarium"||id=="lake")renderFishing(c,id,w,h,t,dt);
        if(id=="commute"){float period=std::max(8.f,45.f/std::max(.2f,p(c,"busFreq",1)));float cycle=wrap(t,period),x=cycle<period*.45f?w*(cycle/(period*.45f))*.53f:cycle<period*.58f?w*.53f:w*(.53f+(cycle-period*.58f)/(period*.42f)*.65f);program.f("uTod",p(c,"time",0)==0?hour/24.f:p(c,"time",0)==1?.5f:p(c,"time",0)==2?.76f:.95f);program.f("uReeds",p(c,"reeds",1));program.f("uLights",p(c,"lights",1));program.v4("uBus",{x,1,t*2,1});program.v4("uBusB",{cycle>period*.45f&&cycle<period*.58f?1.f:0.f,fx[0][3]?fx[0][2]:-1,0,0});program.v4("uStop",{w*.53f,1,0,0});program.v4("uGust",fx[0].data());flyers<24>("uPart","uPartN",18,w,h,t,8);program.v4("uStar",{0,0,-1,0});}
        if(id=="summer-porch")renderPorch(c,w,h,t,dt);
        if(id=="stadium")renderStadium(c,w,h,t,dt);
        if(id=="boxing-ring")renderBoxing(c,w,h,t,dt);
        glDrawArrays(GL_TRIANGLES,0,3);
        if(id=="beach"||id=="beach-turquoise"||id=="meadow"){
            if(!sprites)sprites=std::make_unique<Sprites>(g);
            if(id=="meadow"){
                float actor=std::min(w/1080.f,h/700.f),x=w*.70f+sin(t*.08f)*w*.10f,y=h*.58f;
                if(p(c,"showBoy",1)>0){int frame=int(t*5)%4;sprites->draw(L"wallpaper/assets/meadow-kite-boy-diagonal-run.png",view,x,y,actor*210,actor*240,t,scaleX,scaleY,false,true,shade,.3f,frame);sprites->draw(L"wallpaper/assets/meadow-kite-boy-diagonal-run.png",view,x,y,actor*210,actor*240,t,scaleX,scaleY,false,false,1,.3f,frame);}
                if(p(c,"showCat",1)>0){float cx=w*.53f+sin(t*.11f)*w*.08f,cy=h*.72f; sprites->draw(L"wallpaper/assets/meadow-black-kitten.png",view,cx,cy,actor*155,actor*155,t,scaleX,scaleY,false,true,shade);sprites->draw(L"wallpaper/assets/meadow-black-kitten.png",view,cx,cy,actor*155,actor*155,t,scaleX,scaleY,cos(t*.11f)<0);}
            }else{
                static const wchar_t*paths[]={L"wallpaper/assets/beach-coconut-palm-v2.png",L"wallpaper/assets/beach-coconut-palm-curved.png",L"wallpaper/assets/beach-coconut-palm-airy.png",L"wallpaper/assets/beach-coconut-palm-windswept.png",L"wallpaper/assets/beach-coconut-palm-arch.png"};
                int n=std::clamp(int(p(c,"palms",4)),0,10);float size=std::clamp(std::min(w/1080,h/700),.65f,1.6f)*220;
                for(int i=0;i<n;i++){float x=w*(.08f+hash(i,1,99)*.80f),y=h*(.45f+hash(i,2,99)*.35f);if(!icons.empty()){const auto&r=icons[size_t(i)%icons.size()];x=float(r.right)+15+(i%2)*35;y=float(r.bottom)+25;}
                    x=std::clamp(x,0.f,std::max(0.f,w-size));y=std::clamp(y-size*.8f,0.f,std::max(0.f,h-size));sprites->draw(paths[i%5],view,x,y,size,size,t,scaleX,scaleY,i%2,true,shade,std::clamp(-sun.east/std::max(sun.up,.18f),-1.15f,1.15f));sprites->draw(paths[i%5],view,x,y,size,size,t,scaleX,scaleY,i%2);}
            }
        }
    }
private:
    float sceneScale()const{return std::min(previousW/1080.f,std::max(280.f,previousH-48)/640.f);}
    void clickBoxing(float x,float y){
        const float s=sceneScale(),cx=previousW*.5f,h=previousH-48;
        if(p(lastConfig,"hudMode",0)<2&&abs(x-cx)<54*s&&abs(y-44*s)<36*s){roundNumber++;roundAge=0;roundBell=1;hp={1,1};hpLag=hp;return;}
        if(abs(x-(cx-420*s))<78*s&&abs(y-h*.845f)<54*s){tireAge=0;ring.v+=25;return;}
        std::array<Vec4,3> targets{{{cx-388*s,h*.18f+82*s,0,0},{cx-448*s,h*.29f+86*s,0,0},{cx+425*s,h*.24f+88*s,0,0}}};
        for(int i=0;i<3;i++)if(abs(x-targets[i][0])<40*s&&abs(y-targets[i][1])<78*s){bags[i].v+=3;bagIndex=i;hitActor=-1;hitAge=0;hitX=targets[i][0];hitY=targets[i][1];return;}
        hitActor=x<cx?0:1;hitAge=0;hitX=x;hitY=y;autoPunch=2.2f;
        const float foot=h*(hitActor?.766f:.758f),rel=(y-(foot-115*s))/std::max(s,.01f);
        hitZone=rel<-14?0:rel<26?(abs(x-(cx+(hitActor?68:-68)*s))>14*s?1:2):3;
        const float power=p(lastConfig,"punchPower",1),side=hitActor?1.f:-1.f;
        if(hitZone==3)torso[hitActor].v+=32*power;else head[hitActor].v+=side*(hitZone==0?70:55)*power;
        ring.v+=5*power;hp[hitActor]=std::max(0.f,hp[hitActor]-(hitZone==3?.035f:.05f)*power);
    }
    void clickPorch(float x,float y,ClickFx&fx){
        float w=previousW,h=previousH,s=std::min(w/1080.f,std::max(1.f,h-48)/640.f);int n=int(p(lastConfig,"corgis",3));
        fx.value[3]=4;
        for(int i=0;i<n;i++)if(hypot(x-w*(.22f+i*.18f),y-h*(.73f+.06f*(i%2)))<78*s){corgiPose[i]=(corgiPose[i]+1)%3;corgiHappy[i]=2.6f;fx.value[3]=0;return;}
        if(hypot(x-w*.704f,y-(h*.518f-58*s))<68*s){fanMultiplier=fanMultiplier==1?1.8f:fanMultiplier>1.4f?.4f:fanMultiplier>.1f?0:1;fx.value[3]=1;return;}
        if(hypot(x-w*.346f,y-(h*.095f+58*s))<52*s){chime.v=chime.x>=0?-4.2f:4.2f;birdHop=0;fx.value[3]=2;return;}
        if(hypot(x-(w*.702f+32*s),y-(h*.592f-6*s))<52*s){melonBites=(melonBites+1)%4;melonRefill=6.5f;fx.value[3]=3;return;}
        if(hypot(x-w*.848f,y-h*.676f)<36*s){birdHop=0;fx.value[3]=0;}
    }
    std::array<Vec4,8> effectsAsFx(){std::array<Vec4,8> values{};for(size_t i=0;i<values.size();i++){values[i]=effects[(effectNext+effects.size()-1-i)%effects.size()].value;if(values[i][2]>5)values[i]={0,0,-1,0};else if(std::string(catalog()[index].id)!="summer-porch")values[i][3]=fishing==4?1.f:0.f;}return values;}
    void renderCats(const Config&c,const std::vector<RECT>&icons,float w,float h,float t,float dt){
        int n=std::clamp(int(p(c,"cats",5)),0,15);std::array<Vec4,15> positions{},meta{};
        for(int i=0;i<n;i++){auto&cat=cats[i];cat.wait-=dt;cat.happy=std::max(0.f,cat.happy-dt);cat.phase+=dt*5;
            if(cat.wait<=0){cat.pose=random()<p(c,"naps",.5f)?int(random()*3):3;cat.tx=toy[3]>0?toy[0]+(i%3-1)*40:w*(.08f+random()*.84f);cat.ty=toy[3]>0?toy[1]+(i%4)*20:h*(.37f+random()*.51f);cat.wait=4+random()*10;}
            if(cat.pose==3){float dx=cat.tx-cat.x,dy=cat.ty-cat.y,len=hypot(dx,dy),step=std::min(len,dt*42*p(c,"speed",1));if(len>2){float x=cat.x+dx/len*step,y=cat.y+dy/len*step;if(!obstacle(x,y,icons)){cat.x=x;cat.y=y;cat.right=dx>0;}else{cat.wait=0;cat.pose=2;}}else{cat.pose=2;cat.wait=2;}}
            float scale=std::clamp(std::min(w/1080,h/700),.6f,1.8f);positions[i]={cat.x,cat.y,scale*(cat.right?1:-1),cat.phase};meta[i]={float(cat.pose),float(cat.coat),cat.happy,0};
        }
        // Painter order remains stable for actor overlap.
        for(int i=0;i<n;i++)for(int j=i+1;j<n;j++)if(positions[j][1]<positions[i][1]){std::swap(positions[i],positions[j]);std::swap(meta[i],meta[j]);}
        program.f("uCatN",float(n));array("uCatPos",positions);array("uCatMeta",meta);program.v4("uToy",toy.data());
        flyers<6>("uBfly","uBflyN",int(p(c,"butterflies",4)),w,h,t,15,.12f,.7f);
        std::array<Vec4,28> leaves{};int nleaf=std::clamp(int(p(c,"leaves",.45f)*28),0,28);for(int i=0;i<nleaf;i++)leaves[i]={wrap(hash(i,1,80)*w+t*15*p(c,"wind",1),w),wrap(hash(i,2,80)*h+t*18,h),t+i,1};program.f("uLeafN",float(nleaf));array("uLeaf",leaves);
        program.f("wpMound",p(c,"mound",1));program.f("wpFlowers",p(c,"flowers",1));program.f("wpBushes",p(c,"bushes",4));program.f("wpSun",p(c,"sun",1));
    }
    void renderFishing(const Config&c,const std::string&id,float w,float h,float t,float dt){
        if(fishing){fishingAge+=dt;if(fishing==1&&fishingAge>.6f){fishing=2;fishingAge=0;}else if(fishing==2&&fishingAge>3){fishing=3;fishingAge=0;}else if((fishing==3&&fishingAge>2.2f)||(fishing>=4&&fishingAge>1.2f)){fishing=0;fishingAge=0;}}
        float fx=float(fishingPoint.x),fy=float(fishingPoint.y);float k=std::clamp(fishingAge/.6f,0.f,1.f);
        std::array<Vec4,2> rod{{{fx,h,fx,fy},{fishing,k,fx,fy}}};if(fishing==1)rod[0]={fx,h,fx,h+(fy-h)*k-sin(k*3.14159f)*42};if(fishing>=4){k=std::clamp(fishingAge/1.2f,0.f,1.f);rod[0][3]=fy+(h-fy)*k-sin(k*3.14159f)*54;rod[1][1]=k;}array("uRod",rod);
        if(id=="aquarium"){
            int n=std::clamp(int(p(c,"fish",4)),0,8);std::array<Vec4,8> fish{},body{};
            for(int i=0;i<n;i++){float phase=t*(.5f+hash(i,1,90)*.2f)+i;float x=w*(.5f+.32f*sin(phase*.22f)),y=h*(.5f+.2f*cos(phase*.18f));if(fishing){float attract=std::clamp(p(c,"eager",1)*.25f,0.f,.9f);x+=(fx-x)*attract;y+=(fy-y)*attract;}fish[i]={x,y,cos(phase*.22f)>0?0.f:3.14159f,phase*3};body[i]={.65f+hash(i,2,80)*.6f,hash(i,3,70),.4f,1};}
            program.f("uFishN",float(n));array("uFish",fish);array("uFishB",body);
            int np=std::clamp(int(p(c,"plants",.6f)*32),0,32);std::array<Vec4,32> plants{};for(int i=0;i<np;i++)plants[i]={w*(i+.5f)/std::max(np,1),h-30,70+hash(i,1,20)*120,hash(i,2,30)*100};program.f("uPlantN",float(np));array("uPlant",plants);program.f("uLight",p(c,"light",1));program.f("uBubbles",p(c,"bubbles",1));program.f("uSurf",h*.12f);
            program.v4("uLine",{fx,fx,fy,fishing?1.f:0.f});program.v4("uLineB",{fishing==3?6.f:0.f,fishing==2||fishing==3?1.f:0.f,fishing==3?1.f:0.f,fishing==4?1.f:0.f});
        }
    }
    void renderPorch(const Config&c,float w,float h,float t,float dt){
        birdHop+=dt;melonRefill-=dt;if(melonRefill<=0)melonBites=0;fanPhase+=dt*p(c,"fanSpeed",1)*fanMultiplier*14;
        int n=std::clamp(int(p(c,"corgis",3)),1,4);std::array<Vec4,4> a{},b{};for(int i=0;i<n;i++){corgiHappy[i]=std::max(0.f,corgiHappy[i]-dt);a[i]={w*(.22f+i*.18f),h*(.73f+.06f*(i%2)),float(corgiPose[i]),std::clamp(corgiHappy[i],0.f,1.f)};b[i]={.9f,float(corgiPose[i]),t*1.8f+i,corgiHappy[i]>0?1.f:0};}program.f("uCorgiN",float(n));array("uCorgiA",a);array("uCorgiB",b);
        float day=p(c,"timeOfDay",0);if(day==0){SYSTEMTIME st{};GetLocalTime(&st);day=st.wHour<6||st.wHour>=19?3:st.wHour>=16?2:1;}
        program.f("uDayMode",day);program.f("uSunbeams",p(c,"sunbeams",1));program.f("uFanSpeed",p(c,"fanSpeed",1)*fanMultiplier);program.v4("uFanState",{w*.704f,h*.518f,sin(t*.6f)*.85f,fanPhase});program.v4("uProps",{chime.x,abs(chime.v)>.1f?wrap(age,1.f):-1.f,float(melonBites),birdHop<.7f?birdHop/.7f:0});flyers<10>("uBugs","uBugN",8,w,h,t,6);
    }
    void renderStadium(const Config&c,float w,float h,float t,float dt){
        passAge+=dt;userChase=std::max(0.f,userChase-dt);float speed=dt*std::max(.2f,p(c,"speed",1))*(userChase>0?5.f:1.4f);ballX+=(targetX-ballX)*std::min(1.f,speed);ballY+=(targetY-ballY)*std::min(1.f,speed);
        if(goalSide!=0){if(goalAge<0&&hypot(ballX-targetX,ballY-targetY)<.015f)goalAge=0;else if(goalAge>=0)goalAge+=dt;if(goalAge>2.2f){goalSide=0;ballX=targetX=.5f;ballY=targetY=.54f;}}
        if(passAge>std::max(.5f,4-p(c,"passFreq",1))){targetX=.16f+random()*.68f;targetY=.26f+random()*.52f;passAge=0;passes++;}
        std::array<Vec4,22> pos{},meta{};int n=std::clamp(int(p(c,"players",6))*2,6,22);
        for(int i=0;i<n;i++){float baseX=i<n/2?.2f+float(i%3)*.1f:.8f-float(i%3)*.1f,baseY=.3f+float(i%5)*.11f;float chase=userChase>0&&i%3==passes%3?.72f:.16f;float x=baseX+(ballX-baseX)*chase+sin(t*.6f+i)*.015f,y=baseY+(ballY-baseY)*chase;pos[i]={x*w,y*h,atan2(ballY-y,ballX-x),t*(chase>.5f?11:6)+i};meta[i]={float(i==0?2:i==n/2?3:i<n/2?0:1),.6f,i==passes%n?1.f:0.f,hash(i,1,101)};}
        program.v4("uPitch",{w*.1f,h*.2f,w*.9f,h*.88f});program.f("uStripes",p(c,"stripes",.4f));program.f("uLighting",p(c,"lighting",0));program.f("uPlayerN",float(n));array("uPlPos",pos);array("uPlMeta",meta);program.v4("uBall",{ballX*w,ballY*h,sin(std::min(passAge,1.f)*3.14159f)*16,t*5});program.v4("uBallTrail",{ballX*w-10,ballY*h-8,.4f,passAge});program.v4("uTarget",{targetX*w,targetY*h,passAge,userChase>0?1.f:0.f});program.v4("uGoalFx",{targetX*w,h*.54f,goalAge,goalAge>=0?goalSide:0});
    }
    void renderBoxing(const Config&c,float w,float h,float t,float dt){
        roundAge+=dt;roundBell=std::max(0.f,roundBell-dt);tireAge+=dt;if(roundAge>=60){roundAge=0;roundNumber++;roundBell=1;hp={1,1};}
        for(auto&s:head)s.step(dt,180/std::max(.4f,p(c,"bounciness",1.25f)),8/std::max(.4f,p(c,"bounciness",1.25f)));
        float spar=p(c,"autoSpar",0);autoPunch-=dt;
        if(spar!=1&&autoPunch<=0){hitActor=int(random()*2);hitAge=0;float side=hitActor?1.f:-1.f;head[hitActor].v=side*55*p(c,"punchPower",1);torso[hitActor].v=14;ring.v=4;hp[hitActor]=std::max(0.f,hp[hitActor]-.025f);autoPunch=spar==2?.6f:1.6f+random()*2;}
        if(hp[0]<=0||hp[1]<=0){roundNumber++;roundAge=0;roundBell=1;hp={1,1};}for(int i=0;i<2;i++)hpLag[i]+=(hp[i]-hpLag[i])*std::min(1.f,dt*2);
        float s=std::min(w/1080,std::max(280.f,h-48)/640);std::array<Vec4,2> pos{},heads{},torsos{},expr{},faces{};
        for(int i=0;i<2;i++){float facing=i?-1.f:1.f;pos[i]={w*.5f-facing*68*s,(h-48)*(i?.766f:.758f),abs(sin(t*1.7f+i))*4.2f-2,sin(t*.85f+i)*7.5f};heads[i]={head[i].x,0,head[i].x*.006f,std::clamp(head[i].x*.025f,-.58f,.58f)};torsos[i]={std::clamp(torso[i].x*.08f,-.85f,.85f),0,std::clamp(torso[i].x*.04f,0.f,1.25f),hitAge<.25f&&i!=hitActor?sin(hitAge/.25f*3.14159f):0};expr[i]={wrap(t+i,4)>.0f&&wrap(t+i,4)<.12f?1.f:0.f,hitAge<.4f&&i==hitActor?1.f:0.f,hp[i]<.25f?1.f:0.f,hitAge<.1f&&i==hitActor?1.f:0.f};}
        std::array<Vec4,4> gloves{},gloveMeta{};std::array<Vec4,6> puffs{};
        if(hitActor>=0&&hitAge<.68f){int j=hitActor;float facing=j?1.f:-1.f,tx=pos[j][0],ty=pos[j][1]-115*s,k=std::clamp(hitAge/.09f,0.f,1.f),tail=std::clamp((hitAge-.21f)/.47f,0.f,1.f);gloves[0]={tx-facing*(1-k)*100*s,ty, j?0.f:3.14159f,(1-tail)*p(c,"punchPower",1)};gloveMeta[0]={tx-facing*100*s,ty,k,hitY>pos[j][1]-80*s?2.f:0.f};puffs[0]={tx,ty,hitAge,.6f};}
        if(hitActor<0&&hitAge<.68f){float k=std::min(1.f,hitAge/.09f);gloves[0]={hitX-(1-k)*80*s,hitY,0,1-std::clamp((hitAge-.21f)/.47f,0.f,1.f)};gloveMeta[0]={hitX-80*s,hitY,k,0};puffs[0]={hitX,hitY,hitAge,.6f};}
        program.v4("uScene",{p(c,"gloveStyle",0),p(c,"hudMode",0),ring.x,ring.x*.2f});program.v4("uHud",{hp[0],hpLag[0],hp[1],hpLag[1]});program.v4("uRound",{float(roundNumber),60-roundAge,roundBell,0});program.v4("uGymProps",{bags[0].x,bags[1].x,bags[2].x,tireAge<.8f?sin(tireAge/.8f*3.14159f):0});array("uFPos",pos);array("uFHead",heads);array("uFFace",faces);array("uFTorso",torsos);array("uFExpr",expr);array("uGloves",gloves);array("uGloveMeta",gloveMeta);array("uPuffs",puffs);
    }
};
}
