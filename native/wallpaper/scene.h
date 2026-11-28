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
#include "boxing.h"
#include <memory>
#include <chrono>
#include <cstring>

namespace wallpaper {
using Vec4=std::array<float,4>;
inline float wrap(float v,float size){return size>0?v-floor(v/size)*size:0;}
struct Spring {float x=0,v=0;void step(float dt,float stiffness=180,float damping=17){int n=std::max(1,int(ceil(dt/.012f)));for(int i=0;i<n;i++){float h=dt/n;v+=(-stiffness*x-damping*v)*h;x+=v*h;}}};
struct Cat {float x=0,y=0,tx=0,ty=0,phase=0,wait=0,happy=0;int pose=0,previousPose=0,coat=0;float poseBlend=1;bool chasing=false;bool right=true;Body body;};
struct ClickFx {Vec4 value{0,0,100,0};};
enum class FishMode {Swim,Approach,Nibble,Bite,Hooked,Caught,Returning,Flee};
struct NativeFish {float x=0,y=0,heading=0,phase=0,timer=0;FishMode mode=FishMode::Swim;Body body;};
class SceneRuntime {
    GL&g;int index;Program program;Field field;Clouds clouds;std::shared_ptr<ImageCache> imageCache;std::unique_ptr<Sprites> sprites;
    std::vector<RECT> previousIcons;float previousW=0,previousH=0,previousSigma=-1,previousWarp=-1;
    std::array<Cat,15> cats{};std::array<ClickFx,8> effects{};size_t effectNext=0;float age=0;
    Vec4 toy{0,0,0,0};float fishing=0,fishingAge=0;POINT fishingPoint{};
    std::array<NativeFish,8> fishActors{};int hookedFish=-1;float castY=0;V2 rodAnchor{},lineHook{};int caughtFish=0;
    int melonBites=0;float fanMultiplier=1;
    float ballX=.5f,ballY=.5f,targetX=.5f,targetY=.5f;int passes=0;float passAge=0;
    double lastTime=0;int lastCorgis=3;float goalSide=0,goalAge=100,userChase=0;
    static constexpr std::array<Vec4,4> corgiSlots{{{.415f,.508f,1,0},{.580f,.556f,1.06f,0},{.496f,.522f,.58f,1},{.462f,.592f,.88f,2}}};
    std::array<int,4> corgiPose{0,1,0,2},corgiNext{0,1,0,2};std::array<float,4> corgiHappy{},corgiRoll{-1,-1,-1,-1},corgiWait{18,26,22,30},corgiBreath{};Spring chime;
    float birdHop=0,melonRefill=0,fanPhase=0,fanOscillation=.4f;
    struct PorchBug{V2 position,velocity;float phase=0,ttl=-1;};std::vector<PorchBug> porchBugs;

    Body football;std::array<Body,22> players{};
    std::array<V2,22> formation{};int formationCount=0,ballOwner=-1,ballReceiver=-1,lastKicker=-1;
    std::array<int,2> footballScore{};float ballHold=.85f,ballPickup=0,ballDrag=2.05f,ballZ=0,ballVZ=0,footballTempo=1;
    float busX=-170,busVelocity=0,busDoors=0,busWheel=0,busWait=0,busHonk=-1;int busState=0;bool busStop=true;
    std::unique_ptr<BoxingModel> boxing;
    uint32_t rng=0x731f91u;
    Geometry geometry;bool geometryExplicit=false,placementDirty=true;int lastPalmCount=-1;
    PalmLayout palmLayout;Body boyBody;V2 meadowCat{};Spring meadowTail;bool catPlaced=false;float boyGait=0,boyTug=0,boyNext=7,boyPullNext=2.2f;MeadowKite meadowKite;
    std::array<Vec4,8> lanternLayout{};std::array<float,8> lanternSwing{};int lanternCount=0,lanternWant=-1;bool lanternDirty=true;
    enum class WaterMood {Cruise,Forage,Startle,Rest,Curious};
    struct SwimState {WaterMood mood=WaterMood::Cruise;float remaining=0,phase=0,wander=0;};
    std::array<SwimState,6> schoolState{};std::array<SwimState,4> turtleState{};
    V2 feedPoint{};float feedAge=100;
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
            auto artwork=nativeCatPoses(assetText(assetsRoot()/L"wallpaper/shaders/cat_hill.frag"));auto main=artwork.find("void main()");if(main==std::string::npos)throw std::runtime_error("Cat artwork contract missing");
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
        if(std::string(s.id)=="cat-hill")text=catHillDepthComposition(nativeCatPoses(std::move(text)));
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
        {CpuMeasure simulation{graphicsStats.simulationMs};
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
        }
        int visible=0;for(int i=0;i<n;i++){auto&b=list[i];if(!b.initialized)continue;
            float flap=bird?.68f+.32f*cos(b.phase):b.phase;
            float direction=!strcmp(uniform,"uBfly")||!strcmp(uniform,"uBugs")?cos(b.heading):b.heading;
            if(!strcmp(uniform,"uFly")){direction=b.phase;flap=.6f+.4f*sin(t*2+i);}if(!strcmp(uniform,"uPart"))direction=b.phase;
            values[visible]={b.position.x,b.position.y,direction,!strcmp(uniform,"uPart")?float(i%3):flap};motion[visible]={b.speed()/std::max(1.f,spec.speed),b.velocity.x,b.velocity.y,b.angularVelocity};visible++;
        }program.f(count,float(visible));array(uniform,values);array((std::string(uniform)+"Motion").c_str(),motion);
    }
    // Species share collision/acceleration mathematics, but have distinct behavior,
    // timing, propulsion and responses. Each fish entry is an eleven-fish school.
    template<size_t N>void swimmers(const char*uniform,const char*count,int n,float w,float h,bool turtle){
        if(program.loc(uniform)<0)return;CpuMeasure simulation{graphicsStats.simulationMs};
        auto& list=groups[uniform];n=std::clamp(n,0,int(N));list.resize(n);std::array<Vec4,N> values{};
        const float radius=turtle?20.f:38.f;
        for(int i=0;i<n;i++){auto& b=list[i];auto& state=turtle?turtleState[i]:schoolState[i];
            if(!b.initialized){for(int trial=0;trial<768;trial++){V2 q{w*(.05f+.9f*hash(i*773+trial,index,turtle?703:701)),h*(.25f+.6f*hash(i*773+trial,index,turtle?704:702))};
                if(!navigable(q,radius,true))continue;bool clear=true;for(int j=0;j<i;j++)if(list[j].initialized&&(list[j].position-q).length()<radius*3)clear=false;if(!clear)continue;
                b.position=b.target=q;b.heading=hash(i,index,705)*6.283185f;b.initialized=true;state.phase=hash(i,index,706)*6.283185f;state.wander=0;state.remaining=12+hash(i,index,707)*10;break;}}
        }
        for(int step=0;step<simulationSteps;step++){
            // Read one coherent flock state so update order cannot favor a leader.
            const auto before=list;
            for(int i=0;i<n;i++){auto& b=list[i];auto& state=turtle?turtleState[i]:schoolState[i];if(!b.initialized)continue;
                state.remaining-=fixedStep;state.wander-=fixedStep;
                if(state.remaining<=0){
                    if(turtle&&state.mood==WaterMood::Cruise){state.mood=WaterMood::Rest;state.remaining=2+hash(i,int(simulationTime),708)*4;}
                    else{state.mood=WaterMood::Cruise;state.remaining=(turtle?14.f:7.f)+hash(i,int(simulationTime),709)*9;state.wander=0;b.command=false;routes.erase({uniform,size_t(i)});}
                }
                if(state.wander<=0&&state.mood==WaterMood::Cruise){
                    const float turn=(hash(i,int(simulationTime/3),710)-.5f)*(turtle?1.2f:2.f);
                    V2 wanted=b.position+V2{cos(b.heading+turn),sin(b.heading+turn)}*(turtle?130.f:230.f);
                    b.target=destination(wanted,radius,true,b.position);state.wander=(turtle?9.f:4.f)+hash(i,int(simulationTime),711)*5;b.command=false;
                }
                const bool startled=state.mood==WaterMood::Startle,forage=state.mood==WaterMood::Forage;
                float speed=turtle?(state.mood==WaterMood::Rest?0.f:7.5f*(.85f+.15f*sin(state.phase))):
                    (startled?72.f:forage?34.f:19.f)*(.8f+.2f*sin(state.phase*.27f+i));
                MotionSpec spec=turtle?MotionSpec{speed,3.f,4.f,.55f,.8f,radius,false}:MotionSpec{speed,startled?65.f:24.f,36.f,2.f,4.f,radius,false};
                V2 social=separate(before[i],before.data(),before.size(),turtle?70.f:105.f);
                if(!turtle&&state.mood==WaterMood::Cruise){V2 center{},velocity{};int neighbors=0;
                    for(int j=0;j<n;j++)if(i!=j&&before[j].initialized&&(before[j].position-b.position).length()<300){center=center+before[j].position;velocity=velocity+before[j].velocity;neighbors++;}
                    if(neighbors){social=social+unit(center*(1.f/neighbors)-b.position)*.12f+unit(velocity)*.22f;}
                }
                V2 waypoint=b.target;auto route=routes.find({uniform,size_t(i)});const bool commanded=b.command;
                if(route!=routes.end()){auto& path=route->second;if(path.next<path.points.size()&&(path.points[path.next]-b.position).length()<radius)path.next++;
                    if(path.next<path.points.size()){waypoint=path.points[path.next];if(path.next+1<path.points.size())b.command=false;}}
                steer(b,spec,waypoint,fixedStep,[&](V2 q,float r){return navigable(q,r,true);},social);
                if(commanded&&(waypoint-b.target).length()>.01f)b.command=true;
                // Fins/tails retain a small idle beat; propulsion increases stroke rate.
                state.phase+=fixedStep*(turtle?(state.mood==WaterMood::Rest?.22f:1.1f+b.speed()*.08f):2.4f+b.speed()*.095f);
            }
        }
        int visible=0;for(int i=0;i<n;i++)if(list[i].initialized){const auto&b=list[i];const auto&state=turtle?turtleState[i]:schoolState[i];values[visible++]={b.position.x,b.position.y,b.heading,state.phase};}
        program.f(count,float(visible));array(uniform,values);
    }
    void interactSwimmers(const std::string&name,V2 click){
        auto& list=groups[name];const bool turtle=name=="uTurt";const float radius=turtle?20.f:38.f;
        for(size_t i=0;i<list.size();i++){auto&b=list[i];if(!b.initialized)continue;float distance=(b.position-click).length();if(distance>(turtle?260.f:520.f))continue;
            auto&state=turtle?turtleState[i]:schoolState[i];V2 wanted;
            if(distance<(turtle?40.f:70.f)){state.mood=WaterMood::Startle;state.remaining=turtle?3.f:2.2f;wanted=b.position+unit(b.position-click)*190;}
            else{state.mood=turtle?WaterMood::Curious:WaterMood::Forage;state.remaining=turtle?9.f:7.f;
                const float a=6.283185f*(float(i)/std::max(size_t(1),list.size()))+hash(int(i),index,712)*.5f;
                wanted=click+V2{cos(a),sin(a)}*(turtle?85.f:55.f+float(i%2)*45.f);}
            aimActor(name,i,wanted,radius,true);
        }
    }
    bool obstacle(float x,float y,const std::vector<RECT>&icons)const{for(const auto&r:icons)if(x>r.left-55&&x<r.right+55&&y>r.top-15&&y<r.bottom+65)return true;return false;}
    void aimActor(const std::string&name,size_t i,V2 target,float radius,bool water){auto&b=groups[name][i];auto valid=destination(target,radius,water,b.target);if(!navigable(valid,radius,water))return;
        if(water){if((valid-b.target).length()<24&&b.command)return;auto path=waterRoute(b.position,valid,previousW,previousH,radius,[&](V2 q,float r){return navigable(q,r,true);});if(path.empty())return;routes[{name,i}]={std::move(path),0};}b.aim(valid);
    }
public:
    SceneRuntime(GL&gl,int id,std::shared_ptr<ImageCache> cache={}):g(gl),index(id),program(gl,source(id)),field(gl),clouds(gl),imageCache(cache?std::move(cache):std::make_shared<ImageCache>(gl)){rng^=uint32_t(id*19001);}
    int scene()const{return index;}
    // Fork CPU state only when a previously compatible synchronized group splits.
    // Graphics resources stay uniquely owned on the render thread; physical
    // geometry must match, so this never rescales or reinterprets actor bodies.
    bool inheritSimulation(const SceneRuntime& other,const View& view,const Geometry& expected){
        if(index!=other.index||other.previousW!=view.worldW||other.previousH!=view.worldH||other.geometry!=expected)return false;
        cats=other.cats;effects=other.effects;effectNext=other.effectNext;age=other.age;toy=other.toy;
        fishing=other.fishing;fishingAge=other.fishingAge;fishingPoint=other.fishingPoint;fishActors=other.fishActors;hookedFish=other.hookedFish;
        castY=other.castY;rodAnchor=other.rodAnchor;lineHook=other.lineHook;caughtFish=other.caughtFish;melonBites=other.melonBites;
        fanMultiplier=other.fanMultiplier;ballX=other.ballX;ballY=other.ballY;targetX=other.targetX;targetY=other.targetY;
        passes=other.passes;passAge=other.passAge;lastTime=other.lastTime;lastCorgis=other.lastCorgis;goalSide=other.goalSide;
        goalAge=other.goalAge;userChase=other.userChase;corgiPose=other.corgiPose;corgiNext=other.corgiNext;corgiHappy=other.corgiHappy;
        corgiRoll=other.corgiRoll;corgiWait=other.corgiWait;corgiBreath=other.corgiBreath;chime=other.chime;birdHop=other.birdHop;
        melonRefill=other.melonRefill;fanPhase=other.fanPhase;fanOscillation=other.fanOscillation;porchBugs=other.porchBugs;football=other.football;
        players=other.players;formation=other.formation;formationCount=other.formationCount;ballOwner=other.ballOwner;ballReceiver=other.ballReceiver;
        lastKicker=other.lastKicker;footballScore=other.footballScore;ballHold=other.ballHold;ballPickup=other.ballPickup;ballDrag=other.ballDrag;
        ballZ=other.ballZ;ballVZ=other.ballVZ;footballTempo=other.footballTempo;busX=other.busX;busVelocity=other.busVelocity;
        busDoors=other.busDoors;busWheel=other.busWheel;busWait=other.busWait;busHonk=other.busHonk;busState=other.busState;
        busStop=other.busStop;rng=other.rng;geometry=other.geometry;geometryExplicit=other.geometryExplicit;lastPalmCount=other.lastPalmCount;
        palmLayout=other.palmLayout;boyBody=other.boyBody;meadowCat=other.meadowCat;meadowTail=other.meadowTail;catPlaced=other.catPlaced;
        boyGait=other.boyGait;boyTug=other.boyTug;boyNext=other.boyNext;boyPullNext=other.boyPullNext;meadowKite=other.meadowKite;lanternLayout=other.lanternLayout;lanternSwing=other.lanternSwing;
        lanternCount=other.lanternCount;lanternWant=other.lanternWant;schoolState=other.schoolState;turtleState=other.turtleState;feedPoint=other.feedPoint;
        feedAge=other.feedAge;groups=other.groups;remainder=other.remainder;simulationTime=other.simulationTime;routes=other.routes;
        pressedAt=other.pressedAt;pressed=other.pressed;dragged=other.dragged;capturedGroup=other.capturedGroup;capturedIndex=other.capturedIndex;
        previousW=other.previousW;previousH=other.previousH;previousIcons=other.previousIcons;
        boxing=other.boxing?std::make_unique<BoxingModel>(*other.boxing):nullptr;clouds.inheritSimulation(other.clouds);
        previousSigma=previousWarp=-1;placementDirty=lanternDirty=true;simulationSteps=0;return true;
    }

    void setGeometry(Geometry next){if(next!=geometry){geometry=std::move(next);placementDirty=true;}geometryExplicit=true;}
    const Geometry& spatial()const{return geometry;}
    const PalmLayout& palms()const{return palmLayout;}
    const std::map<std::string,std::vector<Body>>& actors()const{return groups;}
    const Body& meadowBoy()const{return boyBody;}
    const std::array<Cat,15>& catActors()const{return cats;}
    const std::array<NativeFish,8>& fish()const{return fishActors;}
    struct FishingStatus {int phase,actor,caught;V2 hook;};
    FishingStatus fishingStatus()const{return {int(fishing),hookedFish,caughtFish,lineHook};}
    const BoxingModel* boxingState()const{return boxing.get();}
    const Body& stadiumBall()const{return football;}
    const std::array<Body,22>& stadiumPlayers()const{return players;}
    const std::array<int,2>& stadiumScore()const{return footballScore;}
#ifdef WALLPAPER_HOSTED_PROBE
    // Synthetic initial conditions exist only in the hosted renderer executable.
    // Product builds have no runtime override or owner-facing probe controls.
    V2 probeMeadowKitten()const{return meadowCat;}
    float probeMeadowTail()const{return meadowTail.x;}
    const MeadowKite& probeMeadowKite()const{return meadowKite;}
    float probeMeadowGait()const{return boyGait;}
    const auto& probePorchBugs()const{return porchBugs;}
    struct PorchStatus {float fanYaw,fanBlade,chimeAngle,chimeVelocity,bird;int bites;std::array<int,4> poses;std::array<float,4> rolls,happy;};
    PorchStatus probePorch()const{return {sin(fanOscillation)*.85f,fanPhase,chime.x,chime.v,birdHop,melonBites,corgiPose,corgiRoll,corgiHappy};}
    void probeCloud(V2 p,float radius){probeCloudCenter=p;probeCloudRadius=radius;}
    void probeActor(const char*name,size_t i,V2 p){auto&list=groups[name];if(i>=list.size())list.resize(i+1);auto&b=list[i];b.position=b.target=p;b.velocity={};b.heading=0;b.phase=0;b.initialized=true;}
    void probePalms(std::vector<Palm> trees){probePalmOverride=true;probePalmTrees=std::move(trees);}
    void probeCat(size_t i,V2 p){auto&cat=cats.at(i);cat.x=p.x;cat.y=p.y;cat.pose=0;cat.body.position=cat.body.target=p;cat.body.velocity={};cat.body.initialized=true;}
    void probeStadiumBall(V2 point){football.position=football.target=point;football.velocity={};football.initialized=true;ballOwner=ballReceiver=-1;ballZ=ballVZ=0;goalSide=0;goalAge=100;}
    void probeCatState(size_t i,int pose,float seconds){auto&cat=cats.at(i);cat.pose=cat.previousPose=pose;cat.poseBlend=1;cat.wait=seconds;cat.body.aim(cat.body.position);}
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
        feedPoint={x,y};feedAge=0;
        for(auto&[name,list]:groups){if(name=="uFish"||name=="uTurt"){interactSwimmers(name,{x,y});continue;}bool water=name=="uKay"||name=="uBoat"||name=="uYacht"||name=="uFish"||name=="uTurt";float r=water?(name=="uFish"||name=="uTurt"?20.f:30.f):19.f;
            for(size_t i=0;i<list.size();i++){auto&b=list[i];if(!b.initialized)continue;V2 target{x,y};
                if(!water){if((b.position-target).length()>440)continue;target=target+V2{float(int(i%3)-1)*30.f,float(i/3)*22.f};}
                else if(i)target=target+V2{float(i)*72,float(i%2)*70};
                aimActor(name,i,target,r,water);
            }}
        const std::string id=catalog()[index].id;
        if(id=="meadow"){float radius=geometry.meadowCatSize()*.5f;if(catPlaced&&Box{meadowCat.x-radius,meadowCat.y-radius,2*radius,2*radius}.contains({x,y}))meadowTail.v+=.9f;else if(boyBounds().contains({x,y})){boyTug=1.12f;boyPullNext=age+7.8f;}else if(navigable({x,y},70,false)){boyBody.aim({x,y});boyNext=age+18;}}
        if(id=="sky"||id=="meadow"||id=="ocean"||id=="lake")clouds.burst(x,y,lastTime);
        if(id=="cat-hill"||id=="cats"){bool pet=false;for(auto&cat:cats)if(cat.body.initialized&&Box{cat.x-42*geometry.catScale(),cat.y-65*geometry.catScale(),84*geometry.catScale(),85*geometry.catScale()}.contains({x,y})){cat.happy=2.5f;cat.wait=3;catPose(cat,4);cat.chasing=false;cat.body.aim(cat.body.position);pet=true;}if(!pet)toy={x,y,0,10};}
        if(id=="lake"||id=="aquarium")clickFishing(x,y,fx);
        if(id=="boxing-ring")clickBoxing(x,y);
        if(id=="stadium"&&football.initialized){
            const float scale=std::min(geometry.referenceW,geometry.referenceH)/900.f;
            targetX=std::clamp(x/previousW,.08f,.92f);targetY=std::clamp(y/previousH,.2f,.88f);
            const bool goal=abs(y/previousH-.54f)<.13f&&(x<previousW*.12f||x>previousW*.88f);
            V2 target{targetX*previousW,targetY*previousH};if(goal)target.x=x<previousW*.5f?previousW*.1f-18*scale:previousW*.9f+18*scale;
            kickFootball(target,goal?1.55f:2.05f,(goal?680.f:560.f)*scale*sqrt(footballTempo),goal?.16f:.10f);
            userChase=4;ballReceiver=-1;goalSide=0;goalAge=100;
        }
        if(id=="summer-porch")clickPorch(x,y,fx);
        if(id=="tet-sakura")for(int i=lanternCount-1;i>=0;i--)if(hypot(x-lanternLayout[i][0],y-lanternLayout[i][1])<20){lanternSwing[i]=1;break;}
        if(id=="commute"){float road=(previousH-48)*.79f;if(abs(x-previousW*.53f)<24&&y>road-80&&y<road+12){busStop=true;if(busState==0)busWait=0;}else if(busState&&abs(x-busX)<126&&y>road-70&&y<road+20)busHonk=0;}
    }
    void render(const Config&c,const View&view,const std::vector<RECT>&icons,const Weather&weather,double time,float dt,const std::atomic<bool>&stop,float scaleX=1,float scaleY=1){
        const float w=view.worldW,h=view.worldH,t=float(time);lastCorgis=int(p(c,"corgis",3));lastTime=time;age+=dt;feedAge+=dt;toy[2]+=dt;toy[3]=std::max(0.f,toy[3]-dt);
        if(!geometryExplicit&&(w!=previousW||h!=previousH)){geometry={w,h,{{0,0,w,h}}};placementDirty=true;}
        remainder+=std::clamp(double(dt),0.,.25);simulationSteps=int(floor((remainder+1e-7)/fixedStep));remainder-=simulationSteps*fixedStep;simulationTime+=simulationSteps*fixedStep;
        for(auto&e:effects)e.value[2]+=dt;
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
        if(id!="lake"&&id!="aquarium"){
            swimmers<6>("uFish","uFishN",int(p(c,"fishSchools",0)),w,h,false);
            swimmers<4>("uTurt","uTurtN",int(p(c,"turtles",0)),w,h,true);
        }
        program.v4("uBait",{feedPoint.x,feedPoint.y,feedAge,feedAge<7?std::max(0.f,1-feedAge/7):0.f});
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
            if(!sprites)sprites=std::make_unique<Sprites>(g,imageCache);
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
                    sprites->anchored(paths[p.variant],view,transform,t,scaleX,scaleY,shadow,shadow?shade:1.f,shear);
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
        for(int i=0;i<simulationSteps;i++){meadowTail.step(fixedStep,18,3.8f);
            const float stepTime=t-(simulationSteps-i-1)*fixedStep;
            const float waveTime=fmod(stepTime,9.f);
            if(boyGait<.05f&&age>=boyPullNext&&boyNext-age>3.6f&&(waveTime<2.8f||waveTime>5.f)){
                boyTug=1.12f;boyPullNext=age+7.8f;
            }
            boyTug=std::max(0.f,boyTug-fixedStep);
            if(boyBody.initialized){if(age>boyNext&&!boyBody.command){V2 q=boyBody.position+V2{-165,-254};if(!safeBoy(q,0))q=geometry.home(boyBody.position);if(safeBoy(q,0))boyBody.aim(q);boyNext=age+18;}
                if(boyBody.command)steer(boyBody,{105,150,170,2.5f,7,30,false},boyBody.target,fixedStep,safeBoy);
                else{ // A land character rests after arrival; generic cruising
                    // steering would accelerate away from its reached target.
                    const float speed=boyBody.speed();
                    if(speed>0)boyBody.velocity=boyBody.velocity*(std::max(0.f,speed-170*fixedStep)/speed);
                    const V2 next=boyBody.position+boyBody.velocity*fixedStep;
                    if(safeBoy(next,0))boyBody.position=next;
                }
                boyGait+=std::clamp(boyBody.speed()/105-boyGait,-fixedStep*8,fixedStep*8);
                const auto pose=meadowPose(stepTime,boyTug>0?1.12f-boyTug:100.f);
                meadowKite.step(stepTime,fixedStep,boyBody.position,s,boyGait,boyGait>.05f?0:pose.linePull);
            }}
        const auto pose=meadowPose(t,boyTug>0?1.12f-boyTug:100.f);
        meadowKite.step(t,0,boyBody.position,s,boyGait,0);
        if(p(c,"showBoy",1)>0&&boyBody.initialized&&safeBoy(boyBody.position,0))sprites->boy(view,boyBody.position,s,boyBody.phase,boyGait,t,pose,meadowKite.pose(t,boyBody.position,s,boyGait,boyGait>.05f?0:pose.linePull),shade,sx,sy);
        if(p(c,"showCat",1)>0&&catPlaced){SpriteTransform cat{meadowCat,catSize,catSize,.5f,.5f,false};if(geometry.covers(cat.bounds())&&iconClear(cat.bounds(),previousIcons)){
            const float tail=std::clamp(meadowKittenTailAngle(t)+meadowTail.x,-.45f,.45f);
            sprites->anchored(L"wallpaper/assets/meadow-black-kitten.png",view,cat,t,sx,sy,true,shade,shear,tail);sprites->anchored(L"wallpaper/assets/meadow-black-kitten.png",view,cat,t,sx,sy,false,1,.3f,tail);
        }}
    }
    void fleeFish(NativeFish&fish,V2 origin,bool aquarium){
        fish.mode=FishMode::Flee;fish.timer=1.5f;V2 away=unit(fish.body.position-origin);if(away.length()<.1f)away={cos(fish.body.heading),sin(fish.body.heading)};
        const float radius=aquarium?44.f:12.f;V2 wanted=fish.body.position+away*210;
        for(float distance:{210.f,140.f,80.f,40.f}){V2 q=fish.body.position+away*distance;if(fishWater(q,radius,aquarium)){wanted=q;break;}}
        if(!fishWater(wanted,radius,aquarium))wanted=fish.body.position;fish.body.aim(wanted);
    }
    void clickFishing(float x,float y,ClickFx&fx){
        const bool aquarium=std::string(catalog()[index].id)=="aquarium";
        if(fishing==4||(fishing==1&&fishingAge<.22f))return;
        if(fishing==3){fishing=4;fishingAge=0;if(hookedFish>=0)fishActors[hookedFish].mode=FishMode::Hooked;fx.value[3]=1;return;}
        if(!aquarium&&!navigable({x,y},8,true))return;
        if(hookedFish>=0){fleeFish(fishActors[hookedFish],{float(fishingPoint.x),float(fishingPoint.y)},aquarium);hookedFish=-1;}
        if(aquarium&&(!fishWater({x,y},44,true)||obstacle(x,y,previousIcons))){
            fishing=fishing?5:0;fishingAge=0;fx.value[3]=2;
            for(auto&fish:fishActors)if(hypot(fish.x-x,fish.y-y)<160)fleeFish(fish,{x,y},true);return;
        }
        if(fishing==2&&hypot(x-fishingPoint.x,y-fishingPoint.y)<42){fishing=5;fishingAge=0;return;}
        fishing=1;fishingAge=0;fishingPoint={LONG(x),LONG(y)};castY=aquarium?previousH*.12f:previousH;rodAnchor={x,castY};
        if(!aquarium){float nearest=1e9f;
            // Audited original shore-anchor search; use a physical in-bounds
            // edge fallback so a lifted actor never targets outside its world.
            for(int j=0;j<24;j++){float a=j*6.283185f/24;
                for(float distance=24;distance<=460;distance+=16){V2 q{x+cos(a)*distance,y+sin(a)*distance};
                    if(geometry.contains(q,18)&&iconClear({q.x-10,q.y-10,20,20},previousIcons)&&field.sample(q.x,q.y,lastTime)<=-3){if(distance<nearest){nearest=distance;rodAnchor=q;}break;}}}
            if(nearest==1e9f)for(auto o:geometry.outputs)if(o.contains({x,y})){rodAnchor={std::clamp(x+38,o.x+20,o.x+o.w-20),o.y+o.h-20};break;}
        }
        fx.value[3]=0;
    }
    void clickBoxing(float x,float y){if(boxing)boxing->click({x,y});}
    void clickPorch(float x,float y,ClickFx&fx){
        float w=previousW,h=previousH,s=geometry.porchScale();int n=lastCorgis;
        fx.value[3]=4;
        std::array<int,4> order{0,1,2,3};std::stable_sort(order.begin(),order.begin()+n,[](int a,int b){return corgiSlots[a][1]>corgiSlots[b][1];});
        for(int j=0;j<n;j++){int i=order[j];auto slot=corgiSlots[i];float sc=slot[2]*s*1.72f;
            if(Box{w*slot[0]-50*sc,h*slot[1]-65*sc,100*sc,95*sc}.contains({x,y})){corgiNext[i]=(corgiPose[i]+1)%3;corgiRoll[i]=0;corgiHappy[i]=1;corgiWait[i]=18;fx.value[3]=0;return;}}
        if(hypot(x-w*.704f,y-(h*.518f-58*s))<68*s){fanMultiplier=fanMultiplier==1?1.8f:fanMultiplier>1.4f?.4f:fanMultiplier>.1f?0:1;fx.value[3]=1;return;}
        if(hypot(x-w*.346f,y-(h*.095f+58*s))<52*s){chime.v=chime.x>=0?-4.2f:4.2f;birdHop=.01f;fx.value[3]=2;return;}
        if(hypot(x-(w*.702f+32*s),y-(h*.592f-6*s))<52*s){melonBites=(melonBites+1)%4;melonRefill=6.5f;fx.value[3]=3;return;}
        if(hypot(x-w*.848f,y-h*.676f)<36*s){birdHop=.01f;fx.value[3]=0;return;}
        if(porchBugs.size()<10){float angle=random()*6.283185f;porchBugs.push_back({{x,y},{cos(angle)*34,-18+sin(angle)*14},random()*6.283185f,9});}
    }
    std::array<Vec4,8> effectsAsFx(){std::array<Vec4,8> values{};for(size_t i=0;i<values.size();i++){values[i]=effects[(effectNext+effects.size()-1-i)%effects.size()].value;if(values[i][2]>5)values[i]={0,0,-1,0};else if(std::string(catalog()[index].id)!="summer-porch")values[i][3]=fishing==4?1.f:0.f;}return values;}
    static void catPose(Cat&cat,int pose){if(cat.pose==pose)return;cat.previousPose=cat.pose;cat.pose=pose;cat.poseBlend=0;}
    void renderCats(const Config&c,const std::vector<RECT>&icons,float w,float h,float t,float dt){
        int n=std::clamp(int(p(c,"cats",5)),0,15);std::array<Vec4,15> positions{},meta{};const float scale=geometry.catScale();
        std::array<V2,12> trunks{};const int trunkCount=std::string(catalog()[index].id)=="cats"?std::clamp(int(p(c,"bushes",4)),0,12):0;
        for(int j=0;j<trunkCount;j++)trunks[j]={w*(.06f+hash(j,index,308)*.88f),h*(.4f+hash(j,index,310)*.47f)};
        {CpuMeasure simulation{graphicsStats.simulationMs};
        for(int step=0;step<simulationSteps;step++)for(int i=0;i<n;i++){auto&cat=cats[i];auto&b=cat.body;
            if(!b.initialized){for(int trial=0;trial<337;trial++){V2 q{w*(.08f+.84f*hash(i*337+trial,index,83)),h*(.37f+.51f*hash(i*337+trial,index,84))};if(navigable(q,42*scale,false)){b.position=b.target=q;b.initialized=true;cat.wait=hash(i,0,2)*8;break;}}}
            if(!b.initialized)continue;cat.wait-=fixedStep;cat.happy=std::max(0.f,cat.happy-fixedStep);cat.poseBlend=std::min(1.f,cat.poseBlend+fixedStep/.65f);
            V2 toyPosition{toy[0],toy[1]};const bool interested=toy[3]>0&&cat.happy<=0&&(b.position-toyPosition).length()<560&&i<3;
            if(interested){V2 slot=toyPosition+V2{i==0?-40.f:i==1?40.f:-16.f,i==2?-20.f:-3.f}*scale;
                if(!cat.chasing){b.aim(destination(slot,42*scale,false,b.position));cat.chasing=true;catPose(cat,3);cat.wait=10;}
                if((b.target-b.position).length()<12*scale&&b.speed()<8){catPose(cat,6);b.aim(b.position);cat.wait=1.3f;}
            }else if(cat.chasing){cat.chasing=false;cat.wait=0;}
            if(!interested&&cat.wait<=0){
                if(cat.pose==0||cat.pose==1){catPose(cat,5);cat.wait=1.8f;b.aim(b.position);}
                else if(cat.pose==3){int rest=random()<p(c,"naps",.5f)?int(random()*2):random()<.45f?2:4;catPose(cat,rest);cat.wait=rest<2?10+random()*16:3+random()*6;b.aim(b.position);}
                else{V2 wanted{w*(.08f+random()*.84f),h*(.37f+random()*.51f)};
                    if(n>1&&random()<.25f){auto&friendCat=cats[(i+1)%n];if(friendCat.body.initialized)wanted=friendCat.body.position+V2{95*scale,0};}
                    catPose(cat,3);b.aim(destination(wanted,42*scale,false,b.position));cat.wait=12+random()*8;}
            }
            if(cat.pose==3&&!interested&&(b.target-b.position).length()<8*scale&&b.speed()<4)cat.wait=0;
            V2 separation{};for(int j=0;j<n;j++)if(j!=i&&cats[j].body.initialized){V2 d=b.position-cats[j].body.position;float distance=d.length(),spacing=75*scale;if(distance<spacing&&distance>.01f)separation=separation+unit(d)*((spacing-distance)/spacing*3);}
            auto valid=[&](V2 q,float r){if(!navigable(q,r,false)||q.y<=h*.34f)return false;
                for(int j=0;j<trunkCount;j++){float next=(q-trunks[j]).length(),current=(b.position-trunks[j]).length();if(next<28*scale&&(current>=28*scale||next<=current))return false;}return true;};
            const float speed=(cat.pose==3?42.f*(cat.chasing?2.2f:1.f):0.f)*p(c,"speed",1);b.command=true;
            steer(b,{speed,65,85,2.2f,5,42*scale,false},b.target,fixedStep,valid,separation);
            cat.x=b.position.x;cat.y=b.position.y;cat.phase+=fixedStep*(b.speed()>2?b.speed()/(12*std::max(.1f,scale)):.9f);if(b.speed()>2&&abs(cos(b.heading))>.15f)cat.right=cos(b.heading)>=0;
        }}
        for(int i=0;i<n;i++){auto&cat=cats[i];if(!cat.body.initialized){positions[i]={-10000,-10000,1,0};continue;}
            positions[i]={cat.x,cat.y,geometry.catScale()*(cat.right?1:-1),cat.phase};meta[i]={cat.body.speed()>5?3.f:float(cat.pose),float(cat.coat),cat.happy,float(cat.previousPose*2)+cat.poseBlend};}        // Painter order remains stable for actor overlap.
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
        if(fishing&&dt>0){fishingAge+=dt;
            if(fishing==1&&fishingAge>.6f){fishing=2;fishingAge=0;}
            else if(fishing==2&&fishingAge>(aquarium?1.4f/std::max(.4f,p(c,"eager",1)):3.f)&&hookedFish<0){
                float nearest=1e9;for(int i=0;i<n;i++){float d=hypot(fishActors[i].x-fishingPoint.x,fishActors[i].y-fishingPoint.y);if(d<nearest&&fishActors[i].mode==FishMode::Swim){nearest=d;hookedFish=i;}}
                if(hookedFish>=0){auto&fish=fishActors[hookedFish];auto path=waterRoute(fish.body.position,{float(fishingPoint.x)+12,float(fishingPoint.y)},w,h,aquarium?44.f:12.f,[&](V2 q,float r){return fishWater(q,r,aquarium);});
                    if(path.empty()){hookedFish=-1;fishing=5;fishingAge=0;}else{fish.mode=FishMode::Approach;fish.timer=0;routes[{"bait",size_t(hookedFish)}]={std::move(path),0};}}}
            else if(fishing==3&&fishingAge>(aquarium?1.35f:3.2f)){fishing=5;fishingAge=0;if(hookedFish>=0){fleeFish(fishActors[hookedFish],{float(fishingPoint.x),float(fishingPoint.y)},aquarium);hookedFish=-1;}}
            else if(fishing==4&&hookedFish>=0){auto& fish=fishActors[hookedFish];V2 landing=aquarium?V2{float(fishingPoint.x),h*.12f+18}:rodAnchor;
                if((fish.body.position-landing).length()<22&&fish.body.speed()<42){fish.mode=FishMode::Caught;fish.timer=0;caughtFish++;fishing=0;fishingAge=0;hookedFish=-1;}
                else if(fishingAge>18){fleeFish(fish,{float(fishingPoint.x),float(fishingPoint.y)},aquarium);fishing=5;fishingAge=0;hookedFish=-1;}}
            else if(fishing==5&&fishingAge>1.2f){fishing=0;fishingAge=0;hookedFish=-1;}}
        float fx=float(fishingPoint.x),fy=float(fishingPoint.y);float k=std::clamp(fishingAge/.6f,0.f,1.f);
        std::array<Vec4,2> rod{{{rodAnchor.x,rodAnchor.y,fx,fy},{fishing,k,fx,fy}}};
        if(fishing==1){float ease=1-(1-k)*(1-k);rod[0]={rodAnchor.x,rodAnchor.y,rodAnchor.x+(fx-rodAnchor.x)*ease,rodAnchor.y+(fy-rodAnchor.y)*ease-sin(k*3.14159f)*42};}
        if(fishing>=4){k=std::clamp(fishingAge/1.2f,0.f,1.f);rod[0][2]=fx+(rodAnchor.x-fx)*k;rod[0][3]=fy+(rodAnchor.y-fy)*k-sin(k*3.14159f)*18;rod[1][1]=k;}array("uRod",rod);
        std::array<Vec4,8> fish{},body{};
        for(int i=0;i<n;i++){auto&b=fishActors[i].body;
            auto valid=[&](V2 q,float radius){return fishWater(q,radius,aquarium);};
            if(!b.initialized){for(int trial=0;trial<768;trial++){V2 q{w*(.08f+.84f*hash(i*773+trial,index,71)),h*(.26f+.60f*hash(i*773+trial,index,72))};if(valid(q,aquarium?44.f:12.f)){b.position=b.target=q;b.heading=hash(i,3,4)*6.28f;b.initialized=true;b.idle=10;fishActors[i].x=q.x;fishActors[i].y=q.y;fishActors[i].heading=b.heading;break;}}}
        }
        {CpuMeasure simulation{graphicsStats.simulationMs};
        for(int step=0;step<simulationSteps;step++)for(int i=0;i<n;i++){
            auto&a=fishActors[i];auto&b=a.body;
            auto valid=[&](V2 q,float radius){return fishWater(q,radius,aquarium);};

            if(!b.initialized)continue;
            float speed=22;V2 target=b.target;
            if(a.mode==FishMode::Approach||a.mode==FishMode::Nibble||a.mode==FishMode::Bite){target={fx+12,fy};speed=70*p(c,"eager",1);a.timer+=fixedStep;b.command=true;
                if(a.mode==FishMode::Approach&&a.timer>18){fleeFish(a,{fx,fy},aquarium);hookedFish=-1;fishing=5;fishingAge=0;target=b.target;}
                else if(a.mode==FishMode::Approach&&(b.position-target).length()<20){a.mode=FishMode::Nibble;a.timer=0;}
                if(a.mode==FishMode::Nibble&&a.timer>.8f){a.mode=FishMode::Bite;a.timer=0;fishing=3;fishingAge=0;}}
            else if(a.mode==FishMode::Hooked){target=aquarium?V2{fx,h*.12f+18}:rodAnchor;speed=220;b.command=true;}
            else if(a.mode==FishMode::Caught){a.timer+=fixedStep;target={fx,std::max(16.f,h*.12f-80)};speed=180;b.command=true;if(a.timer>2.4f){a.mode=FishMode::Returning;target={fx,fy};b.aim(target);}}
            else if(a.mode==FishMode::Returning){target={fx,fy};speed=100;b.command=true;if(valid(b.position,aquarium?44.f:12.f)){fleeFish(a,{fx,fy},aquarium);target=b.target;}}
            else if(a.mode==FishMode::Flee){speed=130;a.timer-=fixedStep;target=b.target;if(a.timer<=0){a.mode=FishMode::Swim;b.command=false;b.idle=10;}}
            else if(b.idle>7||(b.target-b.position).length()<20){V2 q=b.position+V2{cos(b.heading+.8f+sin(float(simulationTime)*.2f+i)),sin(b.heading+.8f+sin(float(simulationTime)*.2f+i))}*180;target=destination(q,12,!aquarium,geometry.home(b.position));b.idle=0;b.command=false;}
            b.target=target;const V2 finalTarget=target;
            if(a.mode==FishMode::Approach){auto route=routes.find({"bait",size_t(i)});if(route!=routes.end()){auto&path=route->second;
                if(path.next<path.points.size()&&(path.points[path.next]-b.position).length()<20)path.next++;
                if(path.next<path.points.size()){target=path.points[path.next];if(path.next+1<path.points.size())b.command=false;}}}
            V2 separation{};for(int j=0;j<n;j++)if(j!=i&&fishActors[j].body.initialized){V2 d=b.position-fishActors[j].body.position;float distance=d.length();if(distance<48&&distance>.01f)separation=separation+unit(d)*((48-distance)/24);}
            bool lifted=a.mode==FishMode::Hooked||a.mode==FishMode::Caught||a.mode==FishMode::Returning;
            steer(b,{speed,a.mode==FishMode::Hooked?200.f:90.f,a.mode==FishMode::Hooked?200.f:100.f,2.3f,5,aquarium?44.f:12.f,false},target,fixedStep,[&](V2 q,float r){return lifted?geometry.contains(q,r):valid(q,r);},lifted?V2{}:separation);
            b.target=finalTarget;if(a.mode==FishMode::Approach)b.command=true;
            a.phase+=fixedStep*(2.1f+b.speed()*.08f);
            a.x=b.position.x;a.y=b.position.y;a.heading=b.heading;
        }
        }
        for(int i=0;i<n;i++){auto&a=fishActors[i];bool hidden=a.mode==FishMode::Returning||(!aquarium&&(a.mode==FishMode::Hooked||a.mode==FishMode::Caught));fish[i]={hidden?-10000.f:a.x,hidden?-10000.f:a.y,a.heading,a.phase+(aquarium?0:100)};body[i]={.65f+hash(i,2,80)*.6f,hash(i,3,70),std::clamp(a.body.speed()/130,.15f,.9f),a.body.initialized&&!hidden?(a.mode==FishMode::Caught?std::clamp(1-a.timer/1.8f,0.f,1.f):1.f):0.f};}
        // One lifted fish, attached to the physical fish body; no duplicate
        // underwater body and no second shader-generated flight trajectory.
        if(!aquarium&&fishing==4&&hookedFish>=0){rod[0][2]=fishActors[hookedFish].x;rod[0][3]=fishActors[hookedFish].y;array("uRod",rod);}
        lineHook={rod[0][2],rod[0][3]};
        if(aquarium){
            program.f("uFishN",float(n));array("uFish",fish);array("uFishB",body);
            int want=std::clamp(int(p(c,"plants",.6f)*32),0,32),np=0;std::array<Vec4,32> plants{};
            for(int i=0;i<want;i++){float x=w*(i+.5f)/std::max(want,1),bed=aquariumSand(x),height=std::min(70+hash(i,1,20)*120,bed-h*.12f-16);
                // Bottom plants need open water above their root. Do not lift
                // roots through an icon sand island or across the waterline.
                if(height<24||field.sample(x,bed-26,lastTime)<16)continue;
                plants[np++]={x,bed+3,height,hash(i,2,30)*100};}
            program.f("uPlantN",float(np));array("uPlant",plants);program.f("uLight",p(c,"light",1));program.f("uBubbles",p(c,"bubbles",1));program.f("uSurf",h*.12f);
            float hookY=fishing==1?castY+(fy-castY)*k:fishing>=4?fy+(h*.12f-fy)*std::clamp(fishingAge/1.2f,0.f,1.f):fy;
            lineHook={fx,hookY};
            if(fishing==4&&hookedFish>=0){const auto& a=fishActors[hookedFish];float size=.65f+hash(hookedFish,2,80)*.6f;lineHook=a.body.position+V2{cos(a.heading),sin(a.heading)}*(14*size);}
            program.v4("uLine",{fx,lineHook.x,lineHook.y,fishing?1.f:0.f});program.v4("uLineB",{fishing==3?11.f:hookedFish>=0&&fishActors[hookedFish].mode==FishMode::Nibble?2+abs(sin(t*7))*2:0.f,fishing==2||fishing==3?1.f:0.f,fishing==3?1.f:0.f,fishing==4?1.f:0.f});
        }else{program.f("uFishN",float(n));program.v4("uFish",fish.front().data(),4);}
    }
    void renderPorch(const Config&c,float w,float h,float t,float dt){
        const float fanSpeed=std::clamp(p(c,"fanSpeed",1)*fanMultiplier,0.f,2.4f);
        int n=std::clamp(int(p(c,"corgis",3)),1,4);std::array<Vec4,4> a{},b{};
        const int permanentWanted=std::clamp(int(std::lround(p(c,"sunbeams",1)*5)),1,6);
        auto permanent=[&]{return std::count_if(porchBugs.begin(),porchBugs.end(),[](const PorchBug& bug){return bug.ttl<0;});};
        while(permanent()>permanentWanted){auto i=std::find_if(porchBugs.begin(),porchBugs.end(),[](const PorchBug& bug){return bug.ttl<0;});porchBugs.erase(i);}
        while(permanent()<permanentWanted){
            if(porchBugs.size()>=10){auto i=std::find_if(porchBugs.begin(),porchBugs.end(),[](const PorchBug& bug){return bug.ttl>=0;});if(i==porchBugs.end())break;porchBugs.erase(i);}
            float angle=random()*6.283185f;porchBugs.push_back({{random()*w*.65f,h*(.2f+random()*.68f)},{cos(angle)*24,sin(angle)*14},random()*6.283185f,-1});
        }
        // Audited fan yaw/blade phases stop together; the chime follows a
        // wind target through its own damped spring, including click inertia.
        for(int step=0;step<simulationSteps;step++){
            if(fanSpeed>.02f){fanOscillation=wrap(fanOscillation+fixedStep*(.55f+.25f*fanSpeed),6.283185f);fanPhase=wrap(fanPhase+fixedStep*(12+16*fanSpeed),6.283185f);}
            const float windTarget=sin(fanOscillation*.7f)*(.05f+.04f*fanSpeed);
            chime.v+=((windTarget-chime.x)*14-chime.v*2.6f)*fixedStep;chime.x=std::clamp(chime.x+chime.v*fixedStep,-.65f,.65f);
            if(birdHop>0){birdHop+=fixedStep*2.4f;if(birdHop>=1)birdHop=0;}
            if(melonBites>0){melonRefill-=fixedStep;if(melonRefill<=0)melonBites=0;}
            for(auto& bug:porchBugs){bug.phase=wrap(bug.phase+fixedStep*11.5f,6.283185f);
                bug.velocity.x=std::clamp(bug.velocity.x+(random()-.5f)*90*fixedStep,-52.f,52.f);bug.velocity.y=std::clamp(bug.velocity.y+(random()-.5f)*70*fixedStep,-36.f,36.f);
                bug.position=bug.position+bug.velocity*fixedStep;
                if(bug.position.x<16)bug.velocity.x=abs(bug.velocity.x);else if(bug.position.x>w*.72f)bug.velocity.x=-abs(bug.velocity.x);
                if(bug.position.y<h*.14f)bug.velocity.y=abs(bug.velocity.y);else if(bug.position.y>h-72)bug.velocity.y=-abs(bug.velocity.y);
                if(bug.ttl>=0)bug.ttl=std::max(0.f,bug.ttl-fixedStep);
            }
            std::erase_if(porchBugs,[](const PorchBug& bug){return bug.ttl==0;});
            for(int i=0;i<n;i++){corgiHappy[i]=std::max(0.f,corgiHappy[i]-fixedStep*.75f);corgiBreath[i]+=fixedStep*1.85f;
            if(corgiRoll[i]>=0){float previous=corgiRoll[i];corgiRoll[i]+=fixedStep*1.85f;if(previous<.5f&&corgiRoll[i]>=.5f)corgiPose[i]=corgiNext[i];if(corgiRoll[i]>=1)corgiRoll[i]=-1;}
            else {corgiWait[i]-=fixedStep;if(corgiWait[i]<=0){corgiNext[i]=(corgiPose[i]+1+int(random()>.55f))%3;corgiRoll[i]=0;corgiWait[i]=16+random()*18;}}}}
        for(int i=0;i<n;i++){auto slot=corgiSlots[i];a[i]={w*slot[0],h*slot[1],float(corgiPose[i]),std::max(0.f,corgiRoll[i])};b[i]={slot[2],slot[3],corgiBreath[i]+i,corgiHappy[i]};}
        for(int i=0;i<n;i++)for(int j=i+1;j<n;j++)if(a[j][1]<a[i][1]){std::swap(a[i],a[j]);std::swap(b[i],b[j]);}program.f("uCorgiN",float(n));array("uCorgiA",a);array("uCorgiB",b);
        float day=p(c,"timeOfDay",0);if(day==3){SYSTEMTIME st{};GetLocalTime(&st);int minutes=st.wHour*60+st.wMinute;day=minutes>=360&&minutes<990?0:minutes>=990&&minutes<1140?1:2;}
        program.f("uDayMode",day);program.f("uSunbeams",p(c,"sunbeams",1));program.f("uFanSpeed",fanSpeed);program.v4("uFanState",{w*.704f,h*.518f,sin(fanOscillation)*.85f,fanPhase});program.v4("uProps",{chime.x,-1.f,float(melonBites),birdHop});
        std::array<Vec4,10> bugs{};for(size_t i=0;i<porchBugs.size();++i){const auto& bug=porchBugs[i];bugs[i]={bug.position.x,bug.position.y,bug.velocity.x>=0?1.f:-1.f,bug.phase};}
        array("uBugs",bugs);program.f("uBugN",float(porchBugs.size()));
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
    void kickFootball(V2 target,float drag,float maximum,float loft){
        V2 delta=target-football.position;float distance=std::max(1.f,delta.length()),speed=distance*drag;
        if(speed>maximum){speed=maximum;drag=std::max(.95f,speed/distance);}football.velocity=unit(delta)*speed;
        ballDrag=drag;ballVZ=std::clamp(distance*loft,0.f,105.f);ballOwner=-1;ballPickup=.26f;passAge=0;passes++;
        targetX=target.x/std::max(1.f,previousW);targetY=target.y/std::max(1.f,previousH);
    }
    void renderStadium(const Config&c,float w,float h,float t,float dt){
        const int perTeam=std::clamp(int(p(c,"players",6)),3,11),n=perTeam*2;const float scale=std::min(geometry.referenceW,geometry.referenceH)/900.f;
        const float x0=w*.1f,x1=w*.9f,y0=h*.2f,y1=h*.88f,pw=x1-x0,ph=y1-y0,cy=(y0+y1)*.5f,goalHalf=ph*.11f;
        footballTempo=p(c,"speed",1);const float tempo=footballTempo,frequency=std::max(.4f,p(c,"passFreq",1));
        auto pitch=[&](V2 q,float r){return geometry.contains(q,r)&&q.x>x0+r&&q.x<x1-r&&q.y>y0+r&&q.y<y1-r&&iconClear({q.x-r,q.y-r,2*r,2*r},previousIcons);};
        if(!football.initialized){football.position=destination({w*.5f,cy},8,false,geometry.home({w*.5f,cy}));football.target=football.position;football.initialized=true;ballHold=.85f;}
        if(formationCount!=n){formationCount=n;const std::array<std::array<int,3>,9> split{{{1,0,1},{1,1,1},{2,1,1},{2,2,1},{2,2,2},{3,2,2},{3,3,2},{4,3,2},{4,3,3}}};
            for(int team=0;team<2;team++){int slot=team*perTeam;formation[slot++]={team?1.f-.042f:.042f,.5f};const auto lines=split[perTeam-3];
                for(int line=0;line<3;line++)for(int j=0;j<lines[line];j++){const float base=line==0?.2f:line==1?.36f:.47f,spread=line==0?.62f:line==1?.66f:.54f;
                    formation[slot++]={team?1-base:base,lines[line]==1?.5f:.5f-spread*.5f+spread*j/(lines[line]-1)};}}
            for(int i=0;i<n;i++)if(!players[i].initialized){auto&b=players[i];V2 spot{x0+formation[i].x*pw,y0+formation[i].y*ph};b.position=b.target=destination(spot,12*scale,false,football.position);b.heading=i<perTeam?0:3.14159f;b.initialized=true;}
            if(ballOwner>=n)ballOwner=-1;
        }
        {CpuMeasure simulation{graphicsStats.simulationMs};
        for(int step=0;step<simulationSteps;step++){
            passAge+=fixedStep;userChase=std::max(0.f,userChase-fixedStep);ballPickup=std::max(0.f,ballPickup-fixedStep);
            if(goalSide!=0){goalAge+=fixedStep;
                if(goalAge<2.2f){football.velocity=football.velocity*exp(-5*fixedStep);football.position=football.position+football.velocity*fixedStep;ballZ=std::max(0.f,ballZ+ballVZ*fixedStep);ballVZ=ballZ>0?ballVZ-295*fixedStep:0;continue;}
                goalSide=0;goalAge=100;kickFootball({w*.5f,cy},2.05f,400*scale,0);userChase=1;
            }
            std::array<int,2> closest{-1,-1};std::array<float,2> nearest{1e9f,1e9f};
            for(int i=0;i<n;i++)if(i%perTeam){int team=i/perTeam;float distance=(players[i].position-football.position).length();if(distance<nearest[team]){nearest[team]=distance;closest[team]=i;}}
            const auto before=players;
            for(int i=0;i<n;i++){auto&b=players[i];const int team=i/perTeam;const bool keeper=i%perTeam==0;
                V2 home{x0+formation[i].x*pw,y0+formation[i].y*ph},target=home;float speed=118*scale*tempo*.88f;
                if(keeper){target={team?x1-18*scale:x0+18*scale,cy+std::clamp(football.position.y-cy,-goalHalf*.88f,goalHalf*.88f)};speed=118*scale*tempo*1.15f;}
                else if(i==ballOwner){target=b.position+V2{team?-90.f:90.f,sin(float(simulationTime)+i)*16}*scale;speed=72*scale*tempo;}
                else if(i==ballReceiver||i==closest[team]){target=football.position+football.velocity*(i==ballReceiver?.18f:0.f);speed=118*scale*tempo*(userChase>0?1.45f:1.22f);}
                else{target=home+V2{(football.position.x-(x0+x1)*.5f)*.36f,(football.position.y-cy)*.22f};
                    if(ballOwner>=0&&ballOwner/perTeam==team)target.x+=(team?-1.f:1.f)*pw*.12f;
                    target=target+V2{cos(float(simulationTime)*1.3f+i)*20,sin(float(simulationTime)*1.6f+i*1.3f)*18}*scale;}
                target.x=std::clamp(target.x,x0+14*scale,x1-14*scale);target.y=std::clamp(target.y,y0+14*scale,y1-14*scale);b.command=true;
                steer(b,{speed,170*scale,200*scale,4,10,12*scale,false},target,fixedStep,pitch,separate(before[i],before.data(),n,26*scale));
            }
            if(ballOwner>=0){auto&owner=players[ballOwner];const int team=ballOwner/perTeam;V2 foot=owner.position+V2{team?-10.6f:10.6f,0}*scale;
                // Possession attracts the same physical ball toward the foot;
                // acquiring or losing possession never teleports it.
                football.command=true;steer(football,{160*scale*tempo,600*scale,650*scale,9,28,5*scale,false},foot,fixedStep,pitch);
                ballHold-=fixedStep*frequency;
                if(ballHold<=0){int kicker=ballOwner;lastKicker=kicker;const float goalX=team?x0:x1;
                    if(kicker%perTeam!=0&&abs(goalX-owner.position.x)<pw*.25f&&abs(owner.position.y-cy)<ph*.3f&&random()<.58f){
                        kickFootball({goalX+(team?-18.f:18.f)*scale,cy+(random()-.5f)*goalHalf*1.44f},1.55f,680*scale*sqrt(tempo),.16f);ballReceiver=-1;
                    }else{float best=-1e9f;int receiver=-1;
                        for(int j=team*perTeam;j<(team+1)*perTeam;j++)if(j!=kicker){V2 to=players[j].position;float distance=(to-owner.position).length();if(distance<42*scale||distance>pw*.68f)continue;
                            float open=1e9f;for(int k=(1-team)*perTeam;k<(2-team)*perTeam;k++)open=std::min(open,(players[k].position-to).length());
                            float score=(to.x-owner.position.x)*(team?-.55f:.55f)+std::min(open,140*scale)*.65f-abs(distance-180*scale)*.18f+(random()-.5f)*95*scale;
                            if(j%perTeam==0)score-=120*scale;if(score>best){best=score;receiver=j;}}
                        if(receiver>=0){V2 lead=players[receiver].position+players[receiver].velocity*.28f;kickFootball(lead,2.05f,560*scale*sqrt(tempo),.06f);ballReceiver=receiver;}else ballHold=.45f;
                    }
                }
            }else{
                V2 next=football.position+football.velocity*fixedStep;const float speed=football.speed();
                if(iconClear({next.x-6*scale,next.y-6*scale,12*scale,12*scale},previousIcons)&&geometry.contains(next,6*scale))football.position=next;
                else football.velocity=football.velocity*(-.45f);
                football.velocity=football.velocity*exp(-ballDrag*fixedStep);football.phase+=speed*fixedStep/std::max(1.f,6.8f*scale);
                if(football.speed()<4)football.velocity={};ballZ+=ballVZ*fixedStep;ballVZ-=295*fixedStep;
                if(ballZ<=0){ballZ=0;ballVZ=ballVZ<-22?-ballVZ*.42f:0;}
                const bool mouth=abs(football.position.y-cy)<goalHalf*.96f;
                if(mouth&&(football.position.x<x0-3.5f*scale||football.position.x>x1+3.5f*scale)){
                    goalSide=football.position.x<x0?-1.f:1.f;goalAge=0;footballScore[goalSide>0?0:1]++;ballOwner=ballReceiver=-1;userChase=0;continue;}
                if(football.position.x<x0-22*scale)football.velocity.x=abs(football.velocity.x)*.45f;
                if(football.position.x>x1+22*scale)football.velocity.x=-abs(football.velocity.x)*.45f;
                if(football.position.y<y0-18*scale)football.velocity.y=abs(football.velocity.y)*.45f;
                if(football.position.y>y1+18*scale)football.velocity.y=-abs(football.velocity.y)*.45f;
                if(ballZ<18*scale){int catcher=-1;float distance=1e9f;
                    for(int i=0;i<n;i++){bool keeper=i%perTeam==0;if(i==lastKicker&&ballPickup>0)continue;if(!keeper&&(ballPickup>0||(userChase>0&&football.speed()>115*scale)))continue;
                        float d=(players[i].position-football.position).length();if(d<(keeper?17.5f:13.5f)*scale&&d<distance){catcher=i;distance=d;}}
                    if(catcher>=0){ballOwner=catcher;ballReceiver=-1;ballHold=catcher%perTeam==0?.55f:(.65f+random()*.75f)/frequency;ballVZ=ballZ=0;userChase=0;}
                }
            }
            ballX=football.position.x/w;ballY=football.position.y/h;
        }}
        std::array<Vec4,22> pos{},meta{};for(int i=0;i<n;i++){auto&b=players[i];pos[i]={b.position.x,b.position.y,b.heading,b.phase};meta[i]={float(i==0?2:i==perTeam?3:i<perTeam?0:1),std::clamp(b.speed()/(118*scale),0.f,1.f),i==ballOwner?1.f:0.f,hash(i,1,101)};}
        program.v4("uPitch",{x0,y0,x1,y1});program.f("uStripes",p(c,"stripes",.4f));program.f("uLighting",p(c,"lighting",0));program.f("uPlayerN",float(n));array("uPlPos",pos);array("uPlMeta",meta);
        program.v4("uBall",{football.position.x,football.position.y,ballZ,football.phase});program.v4("uBallTrail",{football.position.x-football.velocity.x*.06f,football.position.y-football.velocity.y*.06f,std::clamp(football.speed()/220,0.f,1.f),passAge});
        program.v4("uTarget",{targetX*w,targetY*h,passAge,userChase>0?1.f:0.f});program.v4("uGoalFx",{goalSide<0?x0:x1,cy,goalAge,goalSide});
    }
    void renderBoxing(const Config&c,float w,float h,float t,float){
        if(!boxing)boxing=std::make_unique<BoxingModel>();boxing->configure(c,w,h,geometry.boxingScale());
        {CpuMeasure measure{graphicsStats.simulationMs};for(int i=0;i<simulationSteps;++i)boxing->step(fixedStep);}
        const auto u=boxing->uniforms(t);program.v4("uScene",u.scene.data());program.v4("uHud",u.hud.data());program.v4("uRound",u.round.data());program.v4("uGymProps",u.props.data());
        array("uFPos",u.pos);array("uFHead",u.head);array("uFFace",u.face);array("uFTorso",u.torso);array("uFExpr",u.expression);array("uGloves",u.gloves);array("uGloveMeta",u.gloveMeta);array("uPuffs",u.puffs);
    }
};
}
