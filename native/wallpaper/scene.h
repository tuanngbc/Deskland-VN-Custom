// Scene behavior is newly implemented against audited GLSL uniform contracts.
// No Rust scene object, legacy scheduler, or old desktop owner is reused.
#pragma once
#include "catalog.h"
#include "field.h"
#include "images.h"
#include "palms.h"
#include "motion.h"
#include "navigation.h"
#include "layers.h"
#include <memory>
#include <chrono>
#include <cstring>

namespace wallpaper {
using Vec4=std::array<float,4>;
inline float wrap(float v,float size){return size>0?v-floor(v/size)*size:0;}
struct Spring {float x=0,v=0;void step(float dt,float stiffness=180,float damping=17){int n=std::max(1,int(ceil(dt/.012f)));for(int i=0;i<n;i++){float h=dt/n;v+=(-stiffness*x-damping*v)*h;x+=v*h;}}};
struct Cat {float x=0,y=0,tx=0,ty=0,phase=0,wait=0,happy=0;int pose=0,coat=0;bool right=true;Body body;};
struct ClickFx {Vec4 value{0,0,100,0};};
enum class FishMode {Swim,Approach,Nibble,Bite,Hooked,Caught,Returning,Flee};
struct NativeFish {float x=0,y=0,heading=0,phase=0,timer=0;FishMode mode=FishMode::Swim;Body body;};
class SceneRuntime {
    GL&g;int index;Program program;Field field;Clouds clouds;std::unique_ptr<Sprites> sprites;
    std::vector<RECT> previousIcons;float previousW=0,previousH=0,previousSigma=-1,previousWarp=-1;
    std::array<Cat,15> cats{};std::array<ClickFx,8> effects{};size_t effectNext=0;float age=0;
    Vec4 toy{0,0,0,0};float fishing=0,fishingAge=0;POINT fishingPoint{};
    std::array<NativeFish,8> fishActors{};int hookedFish=-1;float castY=0;
    std::array<Spring,2> head{},torso{};Spring ring,bag;
    std::array<float,2> hp{1,1},hpLag{1,1};int hitActor=-1;float hitAge=100,hitX=0,hitY=0,autoPunch=0;int melonBites=0;float fanMultiplier=1;
    float ballX=.5f,ballY=.5f,targetX=.5f,targetY=.5f;int passes=0;float passAge=0;
    double lastTime=0;float lastHud=0,lastPower=1;int lastCorgis=3;float goalSide=0,goalAge=100,userChase=0;
    static constexpr std::array<Vec4,4> corgiSlots{{{.415f,.508f,1,0},{.580f,.556f,1.06f,0},{.496f,.522f,.58f,1},{.462f,.592f,.88f,2}}};
    std::array<int,4> corgiPose{0,1,0,2},corgiNext{0,1,0,2};std::array<float,4> corgiHappy{},corgiRoll{-1,-1,-1,-1},corgiWait{18,26,22,30},corgiBreath{};Spring chime;
    float birdHop=100,melonRefill=0,fanPhase=0,tireAge=100;int roundNumber=1,bagIndex=0,hitZone=0;
    float roundAge=0,roundBell=0;std::array<Spring,3> bags{};
    Body football;std::array<Body,22> players{};
    float busX=-170,busVelocity=0,busDoors=0,busWheel=0,busWait=0,busHonk=-1;int busState=0;bool busStop=true;
    uint32_t rng=0x731f91u;
    Geometry geometry;bool geometryExplicit=false,placementDirty=true;int lastPalmCount=-1;
    PalmLayout palmLayout;Body boyBody;V2 meadowCat{};bool catPlaced=false;float boyGait=0,boyTug=0,boyNext=7;
    std::array<Vec4,8> lanternLayout{};std::array<float,8> lanternSwing{};int lanternCount=0,lanternWant=-1;bool lanternDirty=true;
    std::map<std::string,std::vector<Body>> groups;double remainder=0,simulationTime=0;int simulationSteps=0;
    struct Route {std::vector<V2> points;size_t next=0;};std::map<std::pair<std::string,size_t>,Route> routes;
    static constexpr float fixedStep=1.f/120.f;
    float aquariumSand(float x)const{float seed=7341%997*.37f;return previousH-48-(46+18*sin(x*.0042f+seed)+10*sin(x*.0113f+seed*1.7f+1.3f));}
    bool fishWater(V2 q,float radius,bool aquarium)const{
        if(!navigable(q,radius,!aquarium))return false;
        if(!aquarium)return true;
        // Same surface/bed and signed icon sand field as aquarium.frag.
        for(float dx:{-radius,0.f,radius})if(q.y-radius<previousH*.12f+3.6f||q.y+radius>aquariumSand(q.x+dx)||field.sample(q.x+dx,q.y+radius,lastTime)<radius)return false;
        return true;
    }
    V2 pressedAt;bool pressed=false,dragged=false;std::string capturedGroup;size_t capturedIndex=0;
#ifdef WALLPAPER_HOSTED_PROBE
    V2 probeCloudCenter{};float probeCloudRadius=-1;bool probePalmOverride=false;std::vector<Palm> probePalmTrees;
#endif
    float random(){rng^=rng<<13;rng^=rng>>17;rng^=rng<<5;return float(double(rng)/4294967296.);}
    static std::string source(int id){const auto&s=catalog().at(id);
        if(std::string(s.id)=="cats"){
            auto artwork=assetText(assetsRoot()/L"wallpaper/shaders/cat_hill.frag");auto main=artwork.find("void main()");if(main==std::string::npos)throw std::runtime_error("Cat artwork contract missing");
            artwork.resize(main);artwork+=assetText(assetsRoot()/L"wallpaper/shaders/cats-native.frag");return desktopShader(std::move(artwork));
        }auto text=assetText(assetsRoot()/s.shader);
        // Preserve background composition; actor artwork scale comes from one
        // real reference display, not the panorama bounding rectangle.
        const std::pair<const char*,const char*> replacements[]={
            {"min(W / 1080.0, availH / 640.0)","wpActorScale"},
            {"min(W / 1536.0, H / 864.0)","wpActorScale"},
            {"min(W, H) / 900.0","wpActorScale"}};
        for(auto [a,b]:replacements){size_t at;if((at=text.find(a))!=std::string::npos)text.replace(at,strlen(a),b);}
        auto replaceAll=[&](const std::string&a,const std::string&b){size_t at=0;while((at=text.find(a,at))!=std::string::npos){text.replace(at,a.size(),b);at+=b.size();}};
        // Wake opacity follows simulated speed; heading already follows body.
        if(std::string(s.id)=="ocean"){
            replaceAll("wk * 0.45","wk * 0.45 * uBoatMotion[i].x");
            replaceAll("max(edgeWake * 0.65, centerFoam * 0.42) * fade","max(edgeWake * 0.65, centerFoam * 0.42) * fade * uYachtMotion[i].x");
        }
        if(std::string(s.id)=="meadow")text=meadowComposition(std::move(text));
        if(std::string(s.id)=="cat-hill")text=catHillDepthComposition(std::move(text));
        if(std::string(s.id)=="beach"||std::string(s.id)=="beach-turquoise")text=beachAirComposition(std::move(text));
        auto line=text.find('\n');text.insert(line+1,"uniform float wpActorScale;\nuniform vec4 uBoatMotion[6],uYachtMotion[4],uKayMotion[4];\n");return desktopShader(std::move(text));
    }
    template<size_t N>void array(const char*name,const std::array<Vec4,N>&a){program.v4(name,a.front().data(),int(N));}
    float p(const Config&c,const char*key,float fallback=0)const{for(const auto&d:catalog()[index].params)if(std::string(key)==d.key)return float(parameter(c,index,key));return fallback;}
    bool shorelineScene()const{auto id=std::string(catalog()[index].id);return id=="beach"||id=="beach-turquoise"||id=="lake";}
    bool navigable(V2 p,float radius,bool water)const{
        if(!geometry.contains(p,radius)||!iconClear({p.x-radius,p.y-radius,2*radius,2*radius},previousIcons,4))return false;
        if(water&&shorelineScene())for(V2 d:std::array<V2,5>{{{0,0},{1,0},{-1,0},{0,1},{0,-1}}})if(field.sample(p.x+d.x*radius,p.y+d.y*radius,lastTime)<12)return false;
        return true;
    }
    V2 destination(V2 wanted,float radius,bool water,V2 fallback)const {
        if(navigable(wanted,radius,water))return wanted;
        for(float ring:{24.f,48.f,80.f,128.f,192.f})for(int j=0;j<16;j++){float a=j*.392699f;V2 q=wanted+V2{cos(a),sin(a)}*ring;if(navigable(q,radius,water))return q;}
        return fallback;
    }
    template<size_t N>void flyers(const char*uniform,const char*count,int n,float w,float h,float t,float speed,float y0=.1f,float y1=.85f){
        if(program.loc(uniform)<0)return;
        bool craft=!strcmp(uniform,"uKay")||!strcmp(uniform,"uBoat")||!strcmp(uniform,"uYacht"),swimmer=!strcmp(uniform,"uFish")||!strcmp(uniform,"uTurt"),water=craft||swimmer;
        bool bird=!strcmp(uniform,"uBird")||!strcmp(uniform,"uSwallow");
        MotionSpec spec{speed,craft?12.f:bird?40.f:16.f,craft?10.f:bird?45.f:18.f,craft?1.4f:bird?1.8f:2.4f,craft?2.f:5.f,craft?30.f:swimmer?20.f:bird?19.f:8.f,!water};
        int salt=0;for(auto ch:std::string(uniform))salt=(salt*33+ch)%10000;
        auto&list=groups[uniform];n=std::clamp(n,0,int(N));list.resize(n);std::array<Vec4,N> values{},motion{};
        for(int i=0;i<n;i++){auto&b=list[i];if(!b.initialized){
                for(int trial=0;trial<768;trial++){V2 q{w*(.04f+.92f*hash(i*773+trial,index,210+salt)),h*(y0+(y1-y0)*hash(i*773+trial,index,211+salt))};
                    if(!navigable(q,spec.radius,water))continue;bool clear=true;for(int j=0;j<i;j++)if(list[j].initialized&&(list[j].position-q).length()<spec.radius*3)clear=false;if(!clear)continue;
                    b.position=b.target=q;b.heading=hash(i,index,201+salt)*6.283185f;b.phase=float(i);b.initialized=true;b.idle=10;break;}
            }
        }
        for(int step=0;step<simulationSteps;step++)for(int i=0;i<n;i++){auto&b=list[i];if(!b.initialized)continue;
            if((!b.command&&(b.idle>6||((b.target-b.position).length()<40&&b.idle>=0)))||(b.command&&spec.airborne&&b.idle>3)){
                float tick=float(simulationTime-(simulationSteps-1-step)*fixedStep);
                V2 wanted=b.position+V2{cos(b.heading+.6f+hash(i,int(tick/6),80+salt)*1.1f),sin(b.heading+.6f+hash(i,int(tick/6),80+salt)*1.1f)}*std::max(120.f,speed*8);
                b.target=destination(wanted,spec.radius,water,geometry.home(b.position));b.command=false;b.idle=0;
            }
            V2 waypoint=b.target;bool arriving=b.command;auto route=routes.find({uniform,size_t(i)});
            if(route!=routes.end()){auto&path=route->second;if(path.next<path.points.size()&&(path.points[path.next]-b.position).length()<std::max(12.f,spec.speed*.8f))path.next++;
                if(path.next<path.points.size()){waypoint=path.points[path.next];if(path.next+1<path.points.size())b.command=false;}}
            steer(b,spec,waypoint,fixedStep,[&](V2 p,float r){return navigable(p,r,water);},separate(b,list.data(),list.size(),spec.radius*3));
            if(arriving&&(waypoint-b.target).length()>.001f)b.command=true;
        }
        int visible=0;for(int i=0;i<n;i++){auto&b=list[i];if(!b.initialized)continue;
            float flap=bird?.68f+.32f*cos(b.phase):b.phase;
            float direction=!strcmp(uniform,"uBfly")||!strcmp(uniform,"uBugs")?cos(b.heading):b.heading;
            if(!strcmp(uniform,"uFly")){direction=b.phase;flap=.6f+.4f*sin(t*2+i);}if(!strcmp(uniform,"uPart"))direction=b.phase;
            values[visible]={b.position.x,b.position.y,direction,!strcmp(uniform,"uPart")?float(i%3):flap};motion[visible]={b.speed()/std::max(1.f,spec.speed),b.velocity.x,b.velocity.y,b.angularVelocity};visible++;
        }program.f(count,float(visible));array(uniform,values);array((std::string(uniform)+"Motion").c_str(),motion);
    }
    bool obstacle(float x,float y,const std::vector<RECT>&icons)const{for(const auto&r:icons)if(x>r.left-55&&x<r.right+55&&y>r.top-15&&y<r.bottom+65)return true;return false;}
    void aimActor(const std::string&name,size_t i,V2 target,float radius,bool water){auto&b=groups[name][i];auto valid=destination(target,radius,water,b.target);if(!navigable(valid,radius,water))return;
        if(water){if((valid-b.target).length()<24&&b.command)return;auto path=waterRoute(b.position,valid,previousW,previousH,radius,[&](V2 q,float r){return navigable(q,r,true);});if(path.empty())return;routes[{name,i}]={std::move(path),0};}b.aim(valid);
    }
public:
    SceneRuntime(GL&gl,int id):g(gl),index(id),program(gl,source(id)),field(gl),clouds(gl){rng^=uint32_t(id*19001);}
    int scene()const{return index;}
    void setGeometry(Geometry next){if(next!=geometry){geometry=std::move(next);placementDirty=true;}geometryExplicit=true;}
    const Geometry& spatial()const{return geometry;}
    const PalmLayout& palms()const{return palmLayout;}
    const std::map<std::string,std::vector<Body>>& actors()const{return groups;}
    const Body& meadowBoy()const{return boyBody;}
    const std::array<NativeFish,8>& fish()const{return fishActors;}
#ifdef WALLPAPER_HOSTED_PROBE
    // Synthetic initial conditions exist only in the hosted renderer executable.
    // Product builds have no runtime override or owner-facing probe controls.
    void probeCloud(V2 p,float radius){probeCloudCenter=p;probeCloudRadius=radius;}
    void probeActor(const char*name,size_t i,V2 p){auto&list=groups[name];if(i>=list.size())list.resize(i+1);auto&b=list[i];b.position=b.target=p;b.velocity={};b.heading=0;b.phase=0;b.initialized=true;}
    void probePalms(std::vector<Palm> trees){probePalmOverride=true;probePalmTrees=std::move(trees);}
    void probeCat(size_t i,V2 p){auto&cat=cats.at(i);cat.x=p.x;cat.y=p.y;cat.pose=0;cat.body.position=cat.body.target=p;cat.body.velocity={};cat.body.initialized=true;}
    bool probeFishWater(V2 p,float r)const{return fishWater(p,r,true);}
#endif
    float shore(V2 p)const{return field.sample(p.x,p.y,lastTime);}
    std::vector<Box> diagnosticBounds()const{std::vector<Box> result;for(auto&p:palmLayout.trees)result.push_back(p.transform().bounds());
        if(boyBody.initialized)result.push_back(boyBounds());if(catPlaced){float s=geometry.meadowCatSize();result.push_back({meadowCat.x-s*.5f,meadowCat.y-s*.5f,s,s});}
        for(auto&[name,list]:groups)for(auto&b:list)if(b.initialized){float r=name=="uKay"||name=="uBoat"||name=="uYacht"?30.f:20.f;result.push_back({b.position.x-r,b.position.y-r,2*r,2*r});}return result;
    }
    Box boyBounds()const{float s=geometry.meadowBoyScale();return {boyBody.position.x-76*s,boyBody.position.y-95*s,152*s,180*s};}
    void pointer(PointerAction action,V2 point){
        if(action==PointerAction::Cancel){pressed=dragged=false;capturedGroup.clear();return;}
        if(action==PointerAction::Down){pressed=true;dragged=false;pressedAt=point;capturedGroup.clear();
            for(auto&[name,list]:groups)for(size_t i=0;i<list.size();i++){auto&b=list[i];if(!b.initialized)continue;V2 d=point-b.position;float u=cos(b.heading)*d.x+sin(b.heading)*d.y,v=-sin(b.heading)*d.x+cos(b.heading)*d.y;
                float rx=name=="uKay"?24.f:name=="uYacht"?28.f:name=="uBoat"?26.f:18.f,ry=name=="uBird"?18.f:12.f;
                if(abs(u)<=rx&&abs(v)<=ry){capturedGroup=name;capturedIndex=i;return;}}
            return;
        }
        if(!pressed)return;dragged=dragged||(point-pressedAt).length()>5;
        if(dragged&&!capturedGroup.empty()){auto&list=groups[capturedGroup];if(capturedIndex<list.size()){bool water=capturedGroup=="uKay"||capturedGroup=="uBoat"||capturedGroup=="uYacht"||capturedGroup=="uFish"||capturedGroup=="uTurt";float radius=water?30.f:19.f;aimActor(capturedGroup,capturedIndex,point,radius,water);}}
        if(action==PointerAction::Up){if(!dragged)click(point.x,point.y);pressed=false;capturedGroup.clear();}
    }
    void click(float x,float y){if(previousW<=0||!geometry.contains({x,y})||!iconClear({x,y,.01f,.01f},previousIcons,0))return;
        auto&fx=effects[effectNext++%effects.size()];fx.value={x,y,0,.1f+random()*.9f};
        for(auto&[name,list]:groups){bool water=name=="uKay"||name=="uBoat"||name=="uYacht"||name=="uFish"||name=="uTurt";float r=water?(name=="uFish"||name=="uTurt"?20.f:30.f):19.f;
            for(size_t i=0;i<list.size();i++){auto&b=list[i];if(!b.initialized)continue;V2 target{x,y};
                if(!water){if((b.position-target).length()>440)continue;target=target+V2{float(int(i%3)-1)*30.f,float(i/3)*22.f};}
                else if(i)target=target+V2{float(i)*72,float(i%2)*70};
                aimActor(name,i,target,r,water);
            }}
        const std::string id=catalog()[index].id;
        if(id=="meadow"){if(boyBounds().contains({x,y}))boyTug=1;else if(navigable({x,y},70,false)){boyBody.aim({x,y});boyNext=age+18;}}
        if(id=="sky"||id=="meadow"||id=="ocean"||id=="lake")clouds.burst(x,y,lastTime);
        if(id=="cat-hill"||id=="cats"){bool pet=false;for(auto&cat:cats)if(cat.body.initialized&&Box{cat.x-42*geometry.catScale(),cat.y-65*geometry.catScale(),84*geometry.catScale(),85*geometry.catScale()}.contains({x,y})){cat.happy=2.5f;cat.wait=2;cat.pose=(cat.pose+1)%3;cat.body.target=cat.body.position;pet=true;}if(!pet)toy={x,y,0,10};}
        if(id=="lake"||id=="aquarium")clickFishing(x,y,fx);
        if(id=="boxing-ring")clickBoxing(x,y);
        if(id=="stadium"&&previousW>0&&previousH>0){targetX=std::clamp(x/previousW,.08f,.92f);targetY=std::clamp(y/previousH,.2f,.88f);passAge=0;passes++;userChase=4;goalSide=0;
            if(abs(y/previousH-.54f)<.13f&&(x<previousW*.12f||x>previousW*.88f)){goalSide=x<previousW*.5f?-1.f:1.f;targetX=goalSide<0?.10f:.90f;targetY=.54f;goalAge=-1;}}
        if(id=="summer-porch")clickPorch(x,y,fx);
        if(id=="tet-sakura")for(int i=lanternCount-1;i>=0;i--)if(hypot(x-lanternLayout[i][0],y-lanternLayout[i][1])<20){lanternSwing[i]=1;break;}
        if(id=="commute"){float road=(previousH-48)*.79f;if(abs(x-previousW*.53f)<24&&y>road-80&&y<road+12){busStop=true;if(busState==0)busWait=0;}else if(busState&&abs(x-busX)<126&&y>road-70&&y<road+20)busHonk=0;}
    }
    void render(const Config&c,const View&view,const std::vector<RECT>&icons,const Weather&weather,double time,float dt,const std::atomic<bool>&stop,float scaleX=1,float scaleY=1){
        const float w=view.worldW,h=view.worldH,t=float(time);lastHud=p(c,"hudMode",0);lastPower=p(c,"punchPower",1);lastCorgis=int(p(c,"corgis",3));lastTime=time;age+=dt;hitAge+=dt;toy[2]+=dt;toy[3]=std::max(0.f,toy[3]-dt);
        if(!geometryExplicit&&(w!=previousW||h!=previousH)){geometry={w,h,{{0,0,w,h}}};placementDirty=true;}
        remainder+=std::clamp(double(dt),0.,.25);simulationSteps=int(floor((remainder+1e-7)/fixedStep));remainder-=simulationSteps*fixedStep;simulationTime+=simulationSteps*fixedStep;
        for(auto&e:effects)e.value[2]+=dt;
        for(auto&s:torso)s.step(dt);ring.step(dt);bag.step(dt);chime.step(dt,18,2.2f);for(auto&s:bags)s.step(dt,15,1.6f);
        const float sigma=p(c,"sigma",72),warp=p(c,"warp",34);
        bool iconsChanged=icons.size()!=previousIcons.size();if(!iconsChanged)for(size_t i=0;i<icons.size();i++)if(!EqualRect(&icons[i],&previousIcons[i])){iconsChanged=true;break;}
        if(w!=previousW||h!=previousH||sigma!=previousSigma||warp!=previousWarp||iconsChanged){
            placementDirty=true;lanternDirty=true;
            field.generate(w,h,icons,sigma,warp,time,stop);if(stop)return;
            if(previousW==0||previousH==0){for(size_t i=0;i<cats.size();i++){auto&cat=cats[i];cat.x=w*(.12f+random()*.76f);cat.y=h*(.38f+random()*.5f);cat.tx=cat.x;cat.ty=cat.y;cat.coat=int(i%8);cat.wait=random()*8;}
                for(size_t i=0;i<fishActors.size();i++){auto&fish=fishActors[i];fish.x=w*(.15f+hash(int(i),index,71)*.7f);fish.y=h*(.3f+hash(int(i),index,72)*.5f);fish.phase=float(i);}}
            previousW=w;previousH=h;previousSigma=sigma;previousWarp=warp;previousIcons=icons;
        }
        program.use();program.f("wpCompositionPass",0);program.f("wpAirPass",0);program.v2("uRes",w,h);program.v2("wpOrigin",view.x,view.worldH-view.y-view.h);program.v2("wpPixelScale",scaleX,scaleY);
        program.f("uScale",1);program.f("uSeed",7341%997*.37f);program.f("uTime",t);program.f("uDebug",0);program.i("uNoise",0);field.bind(program,time);
        program.f("wpActorScale",std::string(catalog()[index].id)=="summer-porch"?geometry.porchScale():std::string(catalog()[index].id)=="boxing-ring"?geometry.boxingScale():std::string(catalog()[index].id)=="stadium"?std::min(geometry.referenceW,geometry.referenceH)/900.f:geometry.catScale());
        const std::string id=catalog()[index].id;
        if(id=="meadow"||id=="sky"||id=="ocean"||id=="lake"){clouds.update(w,h,id=="lake"?p(c,"clouds",4)/8:p(c,"cover",.25f),p(c,"flow",1),time);clouds.bind(program);}
#ifdef WALLPAPER_HOSTED_PROBE
        if(probeCloudRadius>=0){clouds.probeDisc(w,h,probeCloudCenter.x,probeCloudCenter.y,probeCloudRadius);clouds.bind(program);}
#endif
        program.f("uEdge",10);program.f("uSpeed",id=="beach-turquoise"?21:22);program.f("uRunup",120);program.f("uShallow",p(c,"shallow",260));program.f("uShoal",p(c,"shoal",110));program.f("uGrain",1);program.f("uWave",1.6f);program.f("uSigma",sigma);program.f("uSpacing",70);program.f("uWaves",1);program.f("uDepth",p(c,"depth",1));program.f("uFlow",p(c,"flow",1));
        program.f("uWind",p(c,"wind",c.wind));program.f("uRain",p(c,"rain",1));program.f("uMist",p(c,"mist",.4f));program.f("uBloom",p(c,"bloom",1));program.f("uPetals",p(c,"petals",1));
        SYSTEMTIME clock{};GetLocalTime(&clock);float hour=c.clockLighting?clock.wHour+clock.wMinute/60.f:c.manualHour;
        Sun sun;if(c.clockLighting&&weather.available){FILETIME ft;GetSystemTimeAsFileTime(&ft);ULARGE_INTEGER u;u.LowPart=ft.dwLowDateTime;u.HighPart=ft.dwHighDateTime;sun=solar(double(u.QuadPart)/1e7-11644473600.,weather.latitude,weather.longitude);}else{float a=(hour/24-.25f)*6.283185f;sun={cos(a),.35f,sin(a)};}
        const auto localWeather=resolvedWeather(c,weather,time);
        float shade=c.solar&&sun.up>0?std::clamp(sun.up/.15f,0.f,1.f)*.34f*(1-localWeather.cloud*.85f):0;
        const float shear=std::clamp(-sun.east/std::max(sun.up,.18f),-1.15f,1.15f);
        program.v2("LIGHT",c.solar?sun.east:-.6f,c.solar?-sun.north:-.8f);program.v2("SHADOW_OFF",c.solar?shear*150:115,c.solar?std::clamp(sun.north/std::max(sun.up,.18f),-.5f,1.5f)*100:156);
        program.v2("uShadowOffset",std::clamp(-sun.east/std::max(sun.up,.18f),-1.15f,1.15f)*18,std::clamp(sun.north/std::max(sun.up,.18f)*.45f,-.2f,.65f)*18);program.f("uShadowAlpha",shade);
        std::array<Vec4,6> fx{};for(size_t i=0;i<fx.size();i++){const auto&v=effects[(effectNext+effects.size()-1-i)%effects.size()].value;if(v[2]<5)fx[i]=v;}
        array("uRip",fx);array("uBurst",fx);auto allFx=effectsAsFx();program.v4("uFx",allFx.front().data(),id=="aquarium"?8:6);program.f("uShake",fx[0][3]?exp(-fx[0][2]*3):0);program.f("uTreeShake",fx[0][3]?exp(-fx[0][2]*3):0);
        std::array<Vec4,4> sand{};for(size_t i=0;i<sand.size();i++)sand[i]=fx[i];array("uSand",sand);
        flyers<8>("uBird","uBirdN",int(p(c,"birds",3)),w,h,t,id=="sky"?44.f:id=="rainforest"?28.f:id=="mocchau"?26.f:35.f);
        flyers<4>("uKay","uKayN",int(p(c,"kayaks",0)),w,h,t,13,.25f,.8f);
        flyers<6>("uFish","uFishN",int(p(c,"fishSchools",id=="lake"?3:0)),w,h,t,19,.3f,.85f);
        flyers<4>("uTurt","uTurtN",int(p(c,"turtles",0)),w,h,t,8,.3f,.8f);
        flyers<6>("uBoat","uBoatN",int(p(c,"boats",2)),w,h,t,15,.2f,.8f);
        flyers<4>("uYacht","uYachtN",int(p(c,"yachts",1)),w,h,t,22,.3f,.9f);
        flyers<8>("uSwallow","uSwallowN",int(p(c,"swallows",3)),w,h,t,45);
        if(id=="tet-sakura"){
            int want=std::clamp(int(p(c,"lanterns",4)),0,8);if(want!=lanternWant){lanternWant=want;lanternDirty=true;}
            if(lanternDirty&&field.settled(time)){lanternCount=0;
                for(int n=0;n<want;n++){float best=-1e9f;V2 selected{};
                    for(int k=0;k<160;k++){V2 q{40+hash(n*160+k,index,700)*std::max(1.f,w-80),40+hash(n*160+k,index,701)*std::max(1.f,h-128)};float s=field.sample(q.x,q.y,time);
                        if(s<14||(!icons.empty()&&s>95)||field.sample(q.x,q.y-13,time)<14||!geometry.covers({q.x-24,q.y-24,48,48})||!iconClear({q.x-24,q.y-24,48,48},icons))continue;
                        float distance=260;for(int j=0;j<lanternCount;j++)distance=std::min(distance,float(hypot(q.x-lanternLayout[j][0],q.y-lanternLayout[j][1])));
                        float score=distance-(!icons.empty()?abs(s-38)*1.2f:0);if(distance>75&&score>best){selected=q;best=score;}}
                    if(best>-1e8f){lanternLayout[lanternCount++]={selected.x,selected.y,hash(n,index,710)*6.283f,n%2?-1.f:0.f};}}
                lanternDirty=false;
            }
            auto lanterns=lanternLayout;for(int i=0;i<lanternCount;i++){lanternSwing[i]=std::max(0.f,lanternSwing[i]-dt*.65f);lanterns[i][3]+=(i%2?-1.f:1.f)*lanternSwing[i];}program.f("uLanternN",float(lanternCount));array("uLantern",lanterns);
        }
        flyers<15>("uFly","uFlyN",int(p(c,"fireflies",8)),w,h,t,6,.2f,.8f);
        if(id=="sky"){float setting=p(c,"time",0);float phase=setting==0?(hour<6||hour>19?3:hour>16?2:1):setting;program.f("uPhase",phase);program.f("uMoon",p(c,"moon",1));program.f("uStars",p(c,"stars",.6f));program.v4("uPlane",{wrap(t*(p(c,"plane",1)==2?85:52),w+500)-200,h*.18f,1,p(c,"plane",1)>0?220.f:0.f});std::array<Vec4,4> meteors{};if(phase>2.5f)for(int i=0;i<4;i++)if(fx[i][3]>0&&fx[i][2]<1.4f)meteors[i]={fx[i][0]-35,fx[i][1]-25,fx[i][2],fx[i][0]>w*.55f?2.55f:.60f};array("uMeteor",meteors);}
        if(id=="cat-hill"||id=="cats")renderCats(c,icons,w,h,t,dt);
        if(id=="aquarium"||id=="lake")renderFishing(c,id,w,h,t,dt);
        if(id=="commute")renderCommute(c,w,h,t,hour,fx);
        if(id=="summer-porch")renderPorch(c,w,h,t,dt);
        if(id=="stadium")renderStadium(c,w,h,t,dt);
        if(id=="boxing-ring")renderBoxing(c,w,h,t,dt);
        glDrawArrays(GL_TRIANGLES,0,3);
        if(id=="beach"||id=="beach-turquoise"||id=="meadow"){
            if(!sprites)sprites=std::make_unique<Sprites>(g);
            if(id=="meadow"){
                renderMeadow(c,view,t,dt,shade,shear,scaleX,scaleY);
            }else{
                static const wchar_t*paths[]={L"wallpaper/assets/beach-coconut-palm-v2.png",L"wallpaper/assets/beach-coconut-palm-curved.png",L"wallpaper/assets/beach-coconut-palm-airy.png",L"wallpaper/assets/beach-coconut-palm-windswept.png",L"wallpaper/assets/beach-coconut-palm-arch.png"};
                int n=std::clamp(int(p(c,"palms",4)),0,10);if(n!=lastPalmCount){placementDirty=true;lastPalmCount=n;}
                if(placementDirty&&field.settled(time)){palmLayout.build(field,geometry,icons,w,h,n,time);placementDirty=false;}
#ifdef WALLPAPER_HOSTED_PROBE
                if(probePalmOverride)palmLayout.trees=probePalmTrees;
#endif
                // All ground shadows precede all canopies: a later tree must
                // never project its ground shadow onto an earlier canopy.
                for(bool shadow:{true,false})for(auto&p:palmLayout.trees){auto transform=p.transform();if(!geometry.covers(transform.bounds())||!iconClear(transform.bounds(),icons)||field.sample(p.foot.x,p.foot.y,time)>-6)continue;
                    sprites->anchored(paths[p.variant],view,transform,t,scaleX,scaleY,shadow,shade,shear);
                }
            }
        }
        if(id=="meadow"){
            program.use();glEnable(GL_BLEND);
            program.f("wpCompositionPass",1);glBlendFunc(GL_ZERO,GL_SRC_COLOR);glDrawArrays(GL_TRIANGLES,0,3);
            program.f("wpCompositionPass",2);glBlendFunc(GL_SRC_ALPHA,GL_ONE_MINUS_SRC_ALPHA);glDrawArrays(GL_TRIANGLES,0,3);glDisable(GL_BLEND);program.f("wpCompositionPass",0);
        }
        if(id=="beach"||id=="beach-turquoise"){
            program.use();program.f("wpAirPass",1);glEnable(GL_BLEND);glBlendFunc(GL_SRC_ALPHA,GL_ONE_MINUS_SRC_ALPHA);glDrawArrays(GL_TRIANGLES,0,3);glDisable(GL_BLEND);program.f("wpAirPass",0);
        }
    }
private:
    void renderMeadow(const Config&c,const View&view,float t,float dt,float shade,float shear,float sx,float sy){
        float s=geometry.meadowBoyScale(),catSize=geometry.meadowCatSize();
        auto safeBoy=[&](V2 p,float){Box composition{p.x-96*s,p.y-116*s,708*s,726*s};return geometry.covers(composition)&&iconClear(composition,previousIcons)&&(!catPlaced||(p-meadowCat).length()>catSize*.58f+96*s+14);};
        if(!boyBody.initialized){for(auto o:geometry.outputs){for(float fy:{.38f,.48f,.28f,.58f})for(float fx:{.5f,.62f,.38f,.28f,.7f}){V2 p{o.x+o.w*fx,o.y+(o.h-48)*fy};if(!safeBoy(p,0))continue;boyBody.position=boyBody.target=p;boyBody.initialized=true;break;}if(boyBody.initialized)break;}}
        if(!catPlaced){for(int i=0;i<337;i++){V2 p{view.worldW*(.05f+.9f*hash(i,index,25)),view.worldH*(.08f+.84f*hash(i,index,26))};Box bounds{p.x-catSize*.5f,p.y-catSize*.5f,catSize,catSize};
                // Reserve both the next diagonal run and the kite composition.
                V2 a=boyBody.position,b=a+V2{-165,-254};V2 d=b-a;float k=std::clamp(((p-a).x*d.x+(p-a).y*d.y)/(d.x*d.x+d.y*d.y),0.f,1.f);
                if(geometry.covers(bounds)&&iconClear(bounds,previousIcons,18)&&(p-(a+d*k)).length()>catSize*.58f+96*s+14){meadowCat=p;catPlaced=true;break;}}
        }
        for(int i=0;i<simulationSteps;i++){boyTug=std::max(0.f,boyTug-fixedStep);
            if(boyBody.initialized){if(age>boyNext&&!boyBody.command){V2 q=boyBody.position+V2{-165,-254};if(!safeBoy(q,0))q=geometry.home(boyBody.position);if(safeBoy(q,0))boyBody.aim(q);boyNext=age+18;}
                steer(boyBody,{105,150,170,2.5f,7,30,false},boyBody.target,fixedStep,safeBoy);
                boyGait+=std::clamp(boyBody.speed()/105-boyGait,-fixedStep*8,fixedStep*8);
            }}
        if(p(c,"showBoy",1)>0&&boyBody.initialized&&safeBoy(boyBody.position,0))sprites->boy(view,boyBody.position,s,boyBody.phase,boyGait,t,boyTug,shade,sx,sy);
        if(p(c,"showCat",1)>0&&catPlaced){SpriteTransform cat{meadowCat,catSize,catSize,.5f,.5f,false};if(geometry.covers(cat.bounds())&&iconClear(cat.bounds(),previousIcons)){
            sprites->anchored(L"wallpaper/assets/meadow-black-kitten.png",view,cat,t,sx,sy,true,shade,shear);sprites->anchored(L"wallpaper/assets/meadow-black-kitten.png",view,cat,t,sx,sy);
        }}
    }
    void clickFishing(float x,float y,ClickFx&fx){
        const bool aquarium=std::string(catalog()[index].id)=="aquarium";
        if(fishing==4||(fishing==1&&fishingAge<.22f))return;
        if(!aquarium&&!navigable({x,y},8,true))return;
        if(fishing==3){fishing=4;fishingAge=0;if(hookedFish>=0)fishActors[hookedFish].mode=FishMode::Hooked;fx.value[3]=1;return;}
        if(hookedFish>=0){fishActors[hookedFish].mode=FishMode::Flee;fishActors[hookedFish].timer=1.5f;hookedFish=-1;}
        if(aquarium&&(!fishWater({x,y},44,true)||obstacle(x,y,previousIcons))){
            fishing=fishing?5:0;fishingAge=0;fx.value[3]=2;
            for(auto&fish:fishActors)if(hypot(fish.x-x,fish.y-y)<160){fish.mode=FishMode::Flee;fish.timer=1.5f;fish.body.aim(destination(fish.body.position+unit(fish.body.position-V2{x,y})*180,12,false,fish.body.position));}return;
        }
        if(fishing==2&&hypot(x-fishingPoint.x,y-fishingPoint.y)<42){fishing=5;fishingAge=0;return;}
        fishing=1;fishingAge=0;fishingPoint={LONG(x),LONG(y)};castY=aquarium?previousH*.12f:previousH;fx.value[3]=0;
    }
    float sceneScale()const{return geometry.boxingScale();}
    void clickBoxing(float x,float y){
        const float s=sceneScale(),cx=previousW*.5f,h=previousH-48;
        if(lastHud<2&&abs(x-cx)<54*s&&abs(y-44*s)<36*s){roundNumber++;roundAge=0;roundBell=1;hp={1,1};hpLag=hp;return;}
        if(abs(x-(cx-420*s))<78*s&&abs(y-h*.845f)<54*s){tireAge=0;ring.v+=25;return;}
        std::array<Vec4,3> targets{{{cx-388*s,h*.18f+82*s,0,0},{cx-448*s,h*.29f+86*s,0,0},{cx+425*s,h*.24f+88*s,0,0}}};
        for(int i=0;i<3;i++)if(abs(x-targets[i][0])<40*s&&abs(y-targets[i][1])<78*s){bags[i].v+=3;bagIndex=i;hitActor=-1;hitAge=0;hitX=targets[i][0];hitY=targets[i][1];return;}
        hitActor=x<cx?0:1;hitAge=0;hitX=x;hitY=y;autoPunch=2.2f;
        const float foot=h*(hitActor?.766f:.758f),rel=(y-(foot-115*s))/std::max(s,.01f);
        hitZone=rel<-14?0:rel<26?(abs(x-(cx+(hitActor?68:-68)*s))>14*s?1:2):3;
        const float power=lastPower,side=hitActor?1.f:-1.f;
        if(hitZone==3)torso[hitActor].v+=32*power;else head[hitActor].v+=side*(hitZone==0?70:55)*power;
        ring.v+=5*power;hp[hitActor]=std::max(0.f,hp[hitActor]-(hitZone==3?.035f:.05f)*power);
    }
    void clickPorch(float x,float y,ClickFx&fx){
        float w=previousW,h=previousH,s=geometry.porchScale();int n=lastCorgis;
        fx.value[3]=4;
        std::array<int,4> order{0,1,2,3};std::stable_sort(order.begin(),order.begin()+n,[](int a,int b){return corgiSlots[a][1]>corgiSlots[b][1];});
        for(int j=0;j<n;j++){int i=order[j];auto slot=corgiSlots[i];float sc=slot[2]*s*1.72f;
            if(Box{w*slot[0]-50*sc,h*slot[1]-65*sc,100*sc,95*sc}.contains({x,y})){corgiNext[i]=(corgiPose[i]+1)%3;corgiRoll[i]=0;corgiHappy[i]=1;corgiWait[i]=22;fx.value[3]=0;return;}}
        if(hypot(x-w*.704f,y-(h*.518f-58*s))<68*s){fanMultiplier=fanMultiplier==1?1.8f:fanMultiplier>1.4f?.4f:fanMultiplier>.1f?0:1;fx.value[3]=1;return;}
        if(hypot(x-w*.346f,y-(h*.095f+58*s))<52*s){chime.v=chime.x>=0?-4.2f:4.2f;birdHop=0;fx.value[3]=2;return;}
        if(hypot(x-(w*.702f+32*s),y-(h*.592f-6*s))<52*s){melonBites=(melonBites+1)%4;melonRefill=6.5f;fx.value[3]=3;return;}
        if(hypot(x-w*.848f,y-h*.676f)<36*s){birdHop=0;fx.value[3]=0;}
    }
    std::array<Vec4,8> effectsAsFx(){std::array<Vec4,8> values{};for(size_t i=0;i<values.size();i++){values[i]=effects[(effectNext+effects.size()-1-i)%effects.size()].value;if(values[i][2]>5)values[i]={0,0,-1,0};else if(std::string(catalog()[index].id)!="summer-porch")values[i][3]=fishing==4?1.f:0.f;}return values;}
    void renderCats(const Config&c,const std::vector<RECT>&icons,float w,float h,float t,float dt){
        int n=std::clamp(int(p(c,"cats",5)),0,15);std::array<Vec4,15> positions{},meta{};
        for(int step=0;step<simulationSteps;step++)for(int i=0;i<n;i++){auto&cat=cats[i];auto&b=cat.body;
            if(!b.initialized){for(int trial=0;trial<337;trial++){V2 q{w*(.08f+.84f*hash(i*337+trial,index,83)),h*(.37f+.51f*hash(i*337+trial,index,84))};if(navigable(q,42*geometry.catScale(),false)){b.position=b.target=q;b.initialized=true;cat.wait=hash(i,0,2)*8;break;}}}
            if(!b.initialized)continue;cat.wait-=fixedStep;cat.happy=std::max(0.f,cat.happy-fixedStep);
            if(cat.wait<=0){cat.pose=random()<p(c,"naps",.5f)?int(random()*3):3;V2 wanted=toy[3]>0?V2{toy[0]+(i%3-1)*60.f,toy[1]+(i%4)*35.f}:V2{w*(.08f+random()*.84f),h*(.37f+random()*.51f)};
                if(cat.pose==3)b.aim(destination(wanted,42*geometry.catScale(),false,b.position));else b.target=b.position;cat.wait=4+random()*10;}
            V2 separation{};for(int j=0;j<n;j++)if(j!=i&&cats[j].body.initialized){V2 d=b.position-cats[j].body.position;float distance=d.length(),spacing=75*geometry.catScale();if(distance<spacing&&distance>.01f)separation=separation+unit(d)*((spacing-distance)/spacing*3);}
            auto valid=[&](V2 q,float r){return navigable(q,r,false)&&q.y>h*.34f;};
            steer(b,{42*p(c,"speed",1),65,85,2.2f,5,42*geometry.catScale(),false},b.target,fixedStep,valid,separation);
            cat.x=b.position.x;cat.y=b.position.y;cat.phase=b.phase;cat.right=cos(b.heading)>=0;
        }
        for(int i=0;i<n;i++){auto&cat=cats[i];if(!cat.body.initialized){positions[i]={-10000,-10000,1,0};continue;}
            positions[i]={cat.x,cat.y,geometry.catScale()*(cat.right?1:-1),cat.phase};meta[i]={cat.body.speed()>2?3.f:float(cat.pose==3?2:cat.pose),float(cat.coat),cat.happy,0};}        // Painter order remains stable for actor overlap.
        for(int i=0;i<n;i++)for(int j=i+1;j<n;j++)if(positions[j][1]<positions[i][1]){std::swap(positions[i],positions[j]);std::swap(meta[i],meta[j]);}
        program.f("uCatN",float(n));array("uCatPos",positions);array("uCatMeta",meta);program.v4("uToy",toy.data());
        flyers<6>("uBfly","uBflyN",int(p(c,"butterflies",4)),w,h,t,15,.12f,.7f);
        std::array<Vec4,28> leaves{};int nleaf=std::clamp(int(p(c,"leaves",.45f)*28),0,28);for(int i=0;i<nleaf;i++)leaves[i]={wrap(hash(i,1,80)*w+t*15*p(c,"wind",1),w),wrap(hash(i,2,80)*h+t*18,h),t+i,1};program.f("uLeafN",float(nleaf));array("uLeaf",leaves);
        program.f("wpMound",p(c,"mound",1));program.f("wpFlowers",p(c,"flowers",1));program.f("wpBushes",p(c,"bushes",4));program.f("wpSun",p(c,"sun",1));
        std::array<Vec4,12> trees{};int nt=std::clamp(int(p(c,"bushes",4)),0,12);
        for(int i=0;i<nt;i++)trees[i]={w*(.06f+hash(i,index,308)*.88f),h*(.4f+hash(i,index,310)*.47f),.7f+hash(i,index,311)*.3f,float(i)};
        std::stable_sort(trees.begin(),trees.begin()+nt,[](auto&a,auto&b){return a[1]<b[1];});program.f("wpTreeN",float(nt));array("wpTrees",trees);
    }
    void renderFishing(const Config&c,const std::string&id,float w,float h,float t,float dt){
        const bool aquarium=id=="aquarium";int n=aquarium?std::clamp(int(p(c,"fish",4)),1,8):4;
        if(fishing){fishingAge+=dt;
            if(fishing==1&&fishingAge>.6f){fishing=2;fishingAge=0;}
            else if(fishing==2&&fishingAge>(aquarium?1.4f/std::max(.4f,p(c,"eager",1)):3.f)&&hookedFish<0){
                float nearest=1e9;for(int i=0;i<n;i++){float d=hypot(fishActors[i].x-fishingPoint.x,fishActors[i].y-fishingPoint.y);if(d<nearest&&fishActors[i].mode==FishMode::Swim){nearest=d;hookedFish=i;}}
                if(hookedFish>=0){fishActors[hookedFish].mode=FishMode::Approach;fishActors[hookedFish].timer=0;}}
            else if(fishing==3&&fishingAge>2.2f){fishing=5;fishingAge=0;if(hookedFish>=0){fishActors[hookedFish].mode=FishMode::Flee;fishActors[hookedFish].timer=1.5f;hookedFish=-1;}}
            else if(fishing>=4&&fishingAge>1.2f){if(fishing==4&&hookedFish>=0){auto&fish=fishActors[hookedFish];fish.mode=FishMode::Caught;fish.timer=0;}fishing=0;fishingAge=0;hookedFish=-1;}}
        float fx=float(fishingPoint.x),fy=float(fishingPoint.y);float k=std::clamp(fishingAge/.6f,0.f,1.f);
        std::array<Vec4,2> rod{{{fx,h,fx,fy},{fishing,k,fx,fy}}};if(fishing==1)rod[0]={fx,h,fx,h+(fy-h)*k-sin(k*3.14159f)*42};if(fishing>=4){k=std::clamp(fishingAge/1.2f,0.f,1.f);rod[0][3]=fy+(h-fy)*k-sin(k*3.14159f)*54;rod[1][1]=k;}array("uRod",rod);
        std::array<Vec4,8> fish{},body{};
        for(int i=0;i<n;i++){auto&b=fishActors[i].body;
            auto valid=[&](V2 q,float radius){return fishWater(q,radius,aquarium);};
            if(!b.initialized){for(int trial=0;trial<768;trial++){V2 q{w*(.08f+.84f*hash(i*773+trial,index,71)),h*(.26f+.60f*hash(i*773+trial,index,72))};if(valid(q,aquarium?44.f:12.f)){b.position=b.target=q;b.heading=hash(i,3,4)*6.28f;b.initialized=true;b.idle=10;fishActors[i].x=q.x;fishActors[i].y=q.y;fishActors[i].heading=b.heading;break;}}}
        }
        for(int step=0;step<simulationSteps;step++)for(int i=0;i<n;i++){
            auto&a=fishActors[i];auto&b=a.body;
            auto valid=[&](V2 q,float radius){return fishWater(q,radius,aquarium);};

            if(!b.initialized)continue;
            float speed=22;V2 target=b.target;
            if(a.mode==FishMode::Approach||a.mode==FishMode::Nibble||a.mode==FishMode::Bite){target={fx+12,fy};speed=70*p(c,"eager",1);a.timer+=fixedStep;b.command=true;
                if(a.mode==FishMode::Approach&&(b.position-target).length()<20){a.mode=FishMode::Nibble;a.timer=0;}
                if(a.mode==FishMode::Nibble&&a.timer>.8f){a.mode=FishMode::Bite;a.timer=0;fishing=3;fishingAge=0;}}
            else if(a.mode==FishMode::Hooked){target={fx,fy+(h*.12f-fy)*std::clamp(fishingAge/1.2f,0.f,1.f)};speed=220;b.command=true;}
            else if(a.mode==FishMode::Caught){a.timer+=fixedStep;target={fx,std::max(16.f,h*.12f-80)};speed=180;b.command=true;if(a.timer>2.4f){a.mode=FishMode::Returning;target={fx,fy};b.aim(target);}}
            else if(a.mode==FishMode::Returning){target={fx,fy};speed=100;b.command=true;if(valid(b.position,34)){a.mode=FishMode::Flee;a.timer=2;}}
            else if(a.mode==FishMode::Flee){speed=130;a.timer-=fixedStep;target=b.target;if(a.timer<=0){a.mode=FishMode::Swim;b.command=false;b.idle=10;}}
            else if(b.idle>7||(b.target-b.position).length()<20){V2 q=b.position+V2{cos(b.heading+.8f+sin(float(simulationTime)*.2f+i)),sin(b.heading+.8f+sin(float(simulationTime)*.2f+i))}*180;target=destination(q,12,!aquarium,geometry.home(b.position));b.idle=0;b.command=false;}
            b.target=target;V2 separation{};for(int j=0;j<n;j++)if(j!=i&&fishActors[j].body.initialized){V2 d=b.position-fishActors[j].body.position;float distance=d.length();if(distance<48&&distance>.01f)separation=separation+unit(d)*((48-distance)/24);}
            bool lifted=a.mode==FishMode::Hooked||a.mode==FishMode::Caught||a.mode==FishMode::Returning;
            steer(b,{speed,90,100,2.3f,5,aquarium?44.f:12.f,false},target,fixedStep,[&](V2 q,float r){return lifted?geometry.contains(q,r):valid(q,r);},lifted?V2{}:separation);
            a.x=b.position.x;a.y=b.position.y;a.heading=b.heading;a.phase=b.phase;
        }
        for(int i=0;i<n;i++){auto&a=fishActors[i];bool hidden=a.mode==FishMode::Returning||(!aquarium&&(a.mode==FishMode::Hooked||a.mode==FishMode::Caught));fish[i]={hidden?-10000.f:a.x,hidden?-10000.f:a.y,a.heading,a.phase+(aquarium?0:100)};body[i]={.65f+hash(i,2,80)*.6f,hash(i,3,70),std::clamp(a.body.speed()/130,.15f,.9f),a.body.initialized&&!hidden?(a.mode==FishMode::Caught?std::clamp(1-a.timer/1.8f,0.f,1.f):1.f):0.f};}
        // One lifted fish, attached to the physical fish body; no duplicate
        // underwater body and no second shader-generated flight trajectory.
        if(!aquarium&&fishing==4&&hookedFish>=0){rod[0][2]=fishActors[hookedFish].x;rod[0][3]=fishActors[hookedFish].y;array("uRod",rod);}
        if(aquarium){
            program.f("uFishN",float(n));array("uFish",fish);array("uFishB",body);
            int np=std::clamp(int(p(c,"plants",.6f)*32),0,32);std::array<Vec4,32> plants{};for(int i=0;i<np;i++){float x=w*(i+.5f)/std::max(np,1),bed=aquariumSand(x);while(bed>h*.12f+12&&field.sample(x,bed-4,lastTime)<0)bed-=4;plants[i]={x,bed+2,70+hash(i,1,20)*120,hash(i,2,30)*100};}program.f("uPlantN",float(np));array("uPlant",plants);program.f("uLight",p(c,"light",1));program.f("uBubbles",p(c,"bubbles",1));program.f("uSurf",h*.12f);
            float hookY=fishing==1?castY+(fy-castY)*k:fishing>=4?fy+(h*.12f-fy)*std::clamp(fishingAge/1.2f,0.f,1.f):fy;
            program.v4("uLine",{fx,fx,hookY,fishing?1.f:0.f});program.v4("uLineB",{fishing==3?11.f:hookedFish>=0&&fishActors[hookedFish].mode==FishMode::Nibble?2+abs(sin(t*7))*2:0.f,fishing==2||fishing==3?1.f:0.f,fishing==3?1.f:0.f,fishing==4?1.f:0.f});
        }else{program.f("uFishN",float(n));program.v4("uFish",fish.front().data(),4);}
    }
    void renderPorch(const Config&c,float w,float h,float t,float dt){
        birdHop+=dt;melonRefill-=dt;if(melonRefill<=0)melonBites=0;fanPhase+=dt*p(c,"fanSpeed",1)*fanMultiplier*14;
        int n=std::clamp(int(p(c,"corgis",3)),1,4);std::array<Vec4,4> a{},b{};
        for(int step=0;step<simulationSteps;step++)for(int i=0;i<n;i++){corgiHappy[i]=std::max(0.f,corgiHappy[i]-fixedStep*.75f);corgiBreath[i]+=fixedStep*1.85f;
            if(corgiRoll[i]>=0){float previous=corgiRoll[i];corgiRoll[i]+=fixedStep*1.85f;if(previous<.5f&&corgiRoll[i]>=.5f)corgiPose[i]=corgiNext[i];if(corgiRoll[i]>=1)corgiRoll[i]=-1;}
            else {corgiWait[i]-=fixedStep;if(corgiWait[i]<=0){corgiNext[i]=(corgiPose[i]+1+int(random()>.55f))%3;corgiRoll[i]=0;corgiWait[i]=16+random()*18;}}}
        for(int i=0;i<n;i++){auto slot=corgiSlots[i];a[i]={w*slot[0],h*slot[1],float(corgiPose[i]),std::max(0.f,corgiRoll[i])};b[i]={slot[2],slot[3],corgiBreath[i]+i,corgiHappy[i]};}
        for(int i=0;i<n;i++)for(int j=i+1;j<n;j++)if(a[j][1]<a[i][1]){std::swap(a[i],a[j]);std::swap(b[i],b[j]);}program.f("uCorgiN",float(n));array("uCorgiA",a);array("uCorgiB",b);
        float day=p(c,"timeOfDay",0);if(day==3){SYSTEMTIME st{};GetLocalTime(&st);int minutes=st.wHour*60+st.wMinute;day=minutes>=360&&minutes<990?0:minutes>=990&&minutes<1140?1:2;}
        program.f("uDayMode",day);program.f("uSunbeams",p(c,"sunbeams",1));program.f("uFanSpeed",p(c,"fanSpeed",1)*fanMultiplier);program.v4("uFanState",{w*.704f,h*.518f,sin(t*.6f)*.85f,fanPhase});program.v4("uProps",{chime.x,abs(chime.v)>.1f?wrap(age,1.f):-1.f,float(melonBites),birdHop<.7f?birdHop/.7f:0});flyers<10>("uBugs","uBugN",8,w,h,t,6);
    }
    void renderCommute(const Config&c,float w,float h,float t,float hour,const std::array<Vec4,6>&fx){
        // Audited Drive -> braking -> Stop/doors -> depart -> Off lifecycle.
        for(int i=0;i<simulationSteps;i++){
            if(busHonk>=0){busHonk+=fixedStep;if(busHonk>.8f)busHonk=-1;}
            if(busState==0){busWait-=fixedStep;if(busWait<=0){busState=1;busX=-170;busVelocity=0;busStop=true;}continue;}
            float distance=w*.53f-busX,want=95;
            if(busState==1&&busStop&&distance<180){want=std::min(want,sqrt(std::max(0.f,2*24*(distance-1))));if(distance<2&&busVelocity<2){busState=2;busWait=3.6f;}}
            if(busState==2){want=0;busWait-=fixedStep;busDoors=std::clamp(busDoors+fixedStep*(busWait>1?2.f:-2.f),0.f,1.f);if(busWait<=0){busState=1;busStop=false;busHonk=0;}}
            busVelocity+=std::clamp(want-busVelocity,-24*fixedStep,30*fixedStep);busX+=busVelocity*fixedStep;busWheel-=busVelocity*fixedStep/13;
            if(busX>w+200){busState=0;busWait=(22+random()*26)/std::max(.2f,p(c,"busFreq",1));}
        }
        program.f("uTod",p(c,"time",0)==0?hour/24.f:p(c,"time",0)==1?.5f:p(c,"time",0)==2?.76f:.95f);program.f("uReeds",p(c,"reeds",1));program.f("uLights",p(c,"lights",1));
        program.v4("uBus",{busX,1,busWheel,busState?1.f:0.f});program.v4("uBusB",{busDoors,busHonk,busStop&&w*.53f-busX<180?1.f:0.f,0});program.v4("uStop",{w*.53f,1,0,0});program.v4("uGust",fx[0].data());flyers<24>("uPart","uPartN",18,w,h,t,8);program.v4("uStar",{0,0,-1,0});
    }
    void renderStadium(const Config&c,float w,float h,float t,float dt){
        int n=std::clamp(int(p(c,"players",6))*2,6,22);float scale=std::min(geometry.referenceW,geometry.referenceH)/900.f;
        auto pitch=[&](V2 q,float r){return geometry.contains(q,r)&&q.x>w*.08f+r&&q.x<w*.92f-r&&q.y>h*.2f+r&&q.y<h*.88f-r&&iconClear({q.x-r,q.y-r,2*r,2*r},previousIcons);};
        if(!football.initialized){football.position=destination({w*.5f,h*.54f},8,false,geometry.home({w*.5f,h*.54f}));football.target=football.position;football.initialized=true;}
        for(int i=0;i<n;i++)if(!players[i].initialized){float x=i<n/2?.2f+float(i%3)*.1f:.8f-float(i%3)*.1f,y=.3f+float(i%5)*.11f;players[i].position=destination({x*w,y*h},12*scale,false,football.position);players[i].target=players[i].position;players[i].initialized=true;}
        for(int step=0;step<simulationSteps;step++){
            passAge+=fixedStep;userChase=std::max(0.f,userChase-fixedStep);
            if(goalSide!=0){if(goalAge<0&&(football.position-V2{targetX*w,targetY*h}).length()<10)goalAge=0;else if(goalAge>=0)goalAge+=fixedStep;
                if(goalAge>2.2f){goalSide=0;targetX=.5f;targetY=.54f;football.aim({targetX*w,targetY*h});}}
            if(goalSide==0&&userChase==0&&passAge>std::max(.5f,4-p(c,"passFreq",1))){int receiver=(passes+1)%n;V2 lead=players[receiver].position+players[receiver].velocity*.28f;targetX=lead.x/w;targetY=lead.y/h;passAge=0;passes++;}
            V2 ballTarget{targetX*w,targetY*h};football.command=true;
            steer(football,{220*p(c,"speed",1),420,230,7,20,6,false},ballTarget,fixedStep,pitch);
            ballX=football.position.x/w;ballY=football.position.y/h;
            for(int i=0;i<n;i++){auto&b=players[i];float baseX=i<n/2?.2f+float(i%3)*.1f:.8f-float(i%3)*.1f,baseY=.3f+float(i%5)*.11f;
                bool chase=i%3==passes%3;V2 target=chase?football.position:V2{baseX*w,baseY*h}+(football.position-V2{baseX*w,baseY*h})*.16f;
                b.command=true;steer(b,{(chase?118.f:55.f)*p(c,"speed",1),170,200,4,10,12*scale,false},target,fixedStep,pitch,separate(b,players.data(),n,34*scale));
            }
        }
        std::array<Vec4,22> pos{},meta{};for(int i=0;i<n;i++){auto&b=players[i];pos[i]={b.position.x,b.position.y,b.heading,b.phase};meta[i]={float(i==0?2:i==n/2?3:i<n/2?0:1),std::clamp(b.speed()/118.f,0.f,1.f),i==passes%n?1.f:0.f,hash(i,1,101)};}
        program.v4("uPitch",{w*.1f,h*.2f,w*.9f,h*.88f});program.f("uStripes",p(c,"stripes",.4f));program.f("uLighting",p(c,"lighting",0));program.f("uPlayerN",float(n));array("uPlPos",pos);array("uPlMeta",meta);
        program.v4("uBall",{football.position.x,football.position.y,sin(std::min(passAge,1.f)*3.14159f)*16,football.phase});program.v4("uBallTrail",{football.position.x-football.velocity.x*.06f,football.position.y-football.velocity.y*.06f,std::clamp(football.speed()/220,0.f,1.f),passAge});program.v4("uTarget",{targetX*w,targetY*h,passAge,userChase>0?1.f:0.f});program.v4("uGoalFx",{targetX*w,h*.54f,goalAge,goalAge>=0?goalSide:0});
    }
    void renderBoxing(const Config&c,float w,float h,float t,float dt){
        roundAge+=dt;roundBell=std::max(0.f,roundBell-dt);tireAge+=dt;if(roundAge>=60){roundAge=0;roundNumber++;roundBell=1;hp={1,1};}
        for(auto&s:head)s.step(dt,180/std::max(.4f,p(c,"bounciness",1.25f)),8/std::max(.4f,p(c,"bounciness",1.25f)));
        float spar=p(c,"autoSpar",0);autoPunch-=dt;
        if(spar!=1&&autoPunch<=0){hitActor=int(random()*2);hitAge=0;float side=hitActor?1.f:-1.f;head[hitActor].v=side*55*p(c,"punchPower",1);torso[hitActor].v=14;ring.v=4;hp[hitActor]=std::max(0.f,hp[hitActor]-.025f);autoPunch=spar==2?.6f:1.6f+random()*2;}
        if(hp[0]<=0||hp[1]<=0){roundNumber++;roundAge=0;roundBell=1;hp={1,1};}for(int i=0;i<2;i++)hpLag[i]+=(hp[i]-hpLag[i])*std::min(1.f,dt*2);
        float s=geometry.boxingScale();std::array<Vec4,2> pos{},heads{},torsos{},expr{},faces{};
        for(int i=0;i<2;i++){float facing=i?-1.f:1.f;pos[i]={w*.5f-facing*68*s,(h-48)*(i?.766f:.758f),abs(sin(t*1.7f+i))*4.2f-2,sin(t*.85f+i)*7.5f};heads[i]={head[i].x,0,head[i].x*.006f,std::clamp(head[i].x*.025f,-.58f,.58f)};torsos[i]={std::clamp(torso[i].x*.08f,-.85f,.85f),0,std::clamp(torso[i].x*.04f,0.f,1.25f),hitAge<.25f&&i!=hitActor?sin(hitAge/.25f*3.14159f):0};expr[i]={wrap(t+i,4)>.0f&&wrap(t+i,4)<.12f?1.f:0.f,hitAge<.4f&&i==hitActor?1.f:0.f,hp[i]<.25f?1.f:0.f,hitAge<.1f&&i==hitActor?1.f:0.f};}
        std::array<Vec4,4> gloves{},gloveMeta{};std::array<Vec4,6> puffs{};
        if(hitActor>=0&&hitAge<.68f){int j=hitActor;float facing=j?1.f:-1.f,tx=pos[j][0]+(hitZone==1?facing*14*s:0),ty=pos[j][1]-(hitZone==3?65:hitZone==0?135:115)*s,k=std::clamp(hitAge/.09f,0.f,1.f),tail=std::clamp((hitAge-.21f)/.47f,0.f,1.f);gloves[0]={tx-facing*(1-k)*100*s,ty-(hitZone==0?(1-k)*70*s:0), j?0.f:3.14159f,(1-tail)*p(c,"punchPower",1)};gloveMeta[0]={tx-facing*100*s,ty,k,hitZone==0?1.f:hitZone==2?2.f:0.f};puffs[0]={tx,ty,hitAge,.6f};}
        if(hitActor<0&&hitAge<.68f){float k=std::min(1.f,hitAge/.09f);gloves[0]={hitX-(1-k)*80*s,hitY,0,1-std::clamp((hitAge-.21f)/.47f,0.f,1.f)};gloveMeta[0]={hitX-80*s,hitY,k,0};puffs[0]={hitX,hitY,hitAge,.6f};}
        program.v4("uScene",{p(c,"gloveStyle",0),p(c,"hudMode",0),ring.x,ring.x*.2f});program.v4("uHud",{hp[0],hpLag[0],hp[1],hpLag[1]});program.v4("uRound",{float(roundNumber),60-roundAge,roundBell,0});program.v4("uGymProps",{bags[0].x,bags[1].x,bags[2].x,tireAge<.8f?sin(tireAge/.8f*3.14159f):0});array("uFPos",pos);array("uFHead",heads);array("uFFace",faces);array("uFTorso",torsos);array("uFExpr",expr);array("uGloves",gloves);array("uGloveMeta",gloveMeta);array("uPuffs",puffs);
    }
};
}
