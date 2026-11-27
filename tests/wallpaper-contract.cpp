// GitHub-hosted only; no desktop mutation or production data access.
#include "../native/wallpaper/store.h"
#include "../native/wallpaper/field.h"
#include "../native/wallpaper/scene.h"
#include "../native/wallpaper/session.h"
#include <iostream>
#include <limits>
#include <set>
#include <stdexcept>
static void require(bool b,const char*s){if(!b)throw std::runtime_error(s);}
int main(){try{
    using namespace wallpaper;require(catalog().size()==16,"Incomplete catalog");std::set<std::string> names;
    for(const auto&s:catalog()){require(names.insert(s.id).second,"Duplicate scene");for(const auto&p:s.params)require(p.min<=p.initial&&p.initial<=p.max&&p.step>0,"Invalid parameter contract");}
    std::vector<Monitor> monitors{{L"portrait",{-1080,-500,0,1420},144,false},{L"wide",{0,0,2560,1080},96,true}};
    RECT all=bounds(monitors);auto a=viewport(monitors[0],all,Layout::Panorama),b=viewport(monitors[1],all,Layout::Panorama);
    require(a.x==0&&a.y==0&&a.w==1080&&a.h==1920,"Portrait/negative topology");require(b.x==1080&&b.y==500&&a.worldW==3640&&a.worldH==1920,"Panorama physical coordinates");
    require(a.x+a.w==b.x,"Adjacent physical edges disagree");
    auto spatial=sceneGeometry(monitors,monitors[1],Layout::Panorama),single=sceneGeometry({monitors[1]},monitors[1],Layout::Synchronized);
    require(spatial.meadowBoyScale()==single.meadowBoyScale()&&spatial.meadowCatSize()==single.meadowCatSize()&&spatial.boxingScale()==single.boxingScale(),"Adding a portrait enlarged actors");
    require(!spatial.covers({1200,100,100,100})&&spatial.covers({1040,700,100,100}),"Physical union/seam coverage");
    for(auto&m:monitors)m.dpi=192;auto changedDpi=sceneGeometry(monitors,monitors[1],Layout::Panorama);require(changedDpi==spatial,"DPI rescaled physical scene");
    SpriteTransform transform{{140,160},84,78,.454f,.98f,true};auto anchor=transform.point(.454f,.98f);require(hypot(anchor.x-140,anchor.y-160)<.001f&&transform.hit(anchor),"Sprite foot/render/hit transform diverged");
    // Exercise real steering, not a reimplementation of its equations. Same
    // 120 Hz world steps are consumed at all supported presentation rates.
    std::vector<Body> final;
    for(int fps:{15,25,30,60}){Body actor;actor.position={120,150};actor.target=actor.position;actor.initialized=true;actor.heading=-.4f;
        MotionSpec spec{35,12,10,1.4f,2,30,false};double remainder=0;float travelled=0;actor.aim({300,250});
        for(int frame=0;frame<fps*14;frame++){if(frame==fps*2){auto position=actor.position,velocity=actor.velocity;actor.aim({420,180});require((actor.position-position).length()==0&&(actor.velocity-velocity).length()==0,"Repeated click reset physical state");}
            remainder+=1./fps;while(remainder+1e-8>=1./120){remainder-=1./120;auto before=actor;steer(actor,spec,actor.target,1.f/120,[](V2 p,float){return p.x>0&&p.y>0&&p.x<800&&p.y<600;});
                require((actor.position-before.position).length()<=spec.speed/120+.001f,"Discontinuous motion");require((actor.velocity-before.velocity).length()<=spec.accel/120+.001f,"Acceleration exceeded contract");require(abs(angleDelta(before.heading,actor.heading))<=spec.turn/120+.001f,"Instant heading reversal");travelled+=(actor.position-before.position).length();}}
        require(travelled>150&&actor.speed()<=spec.speed+.01f,"Actor failed to travel");final.push_back(actor);
    }
    for(auto&actor:final)require((actor.position-final[0].position).length()<.1f&&(actor.velocity-final[0].velocity).length()<.1f,"15/25/30/60 FPS simulation differs");
    {
        auto valid=[](V2 p,float r){return p.x>=r&&p.y>=r&&p.x<=800-r&&p.y<=600-r&&!(p.x>310-r&&p.x<490+r&&p.y>140-r&&p.y<460+r);};
        V2 start{120,300},goal{680,300};auto route=waterRoute(start,goal,800,600,18,valid);require(route.size()>1&&route.size()<=128,"Water route did not detour around land");
        V2 from=start;for(auto target:route){int steps=int(ceil((target-from).length()/2));for(int i=0;i<=steps;i++)require(valid(from+(target-from)*(float(i)/steps),18),"Route crossed shore/corner");from=target;}
        auto isolated=[](V2 p,float r){return p.x>r&&p.x<800-r&&p.y>r&&p.y<600-r&&!(p.x>310-r&&p.x<490+r);};
        require(waterRoute(start,goal,800,600,18,isolated).empty(),"Disconnected water unexpectedly navigable");std::cout<<"WATER_ROUTE_DETOUR_AND_DISCONNECTED_ISLANDS=PASS\n";
    }
    std::cout<<"PHYSICAL_UNION_SCALE_DPI_ANCHORS=PASS\nTEMPORAL_CONTINUITY_ACCELERATION_TURN_REPEATED_INPUT_FPS=PASS\n";
    require(covered({0,0,100,100},{{0,0,50,100},{50,0,100,100}}),"Multiwindow occlusion");
    require(!covered({0,0,100,100},{{0,0,49,100},{50,0,100,100}}),"Visible gap must render");
    Config c;c.fps=999;c.scene=-10;c.resolution=std::numeric_limits<float>::quiet_NaN();validate(c);require(c.fps==60&&c.scene==0&&c.resolution==1,"Unsafe configuration");
    auto sun=solar(1794718800.,10.94469,106.82432);require(sun.up>.8f,"Solar noon direction");
    auto snow=fromWmo(73,20,10,106);require(snow.snow==1&&snow.rain==0,"WMO snow mapping");
    auto rain=fromWmo(63,20,10,106);require(rain.rain==1&&rain.snow==0,"WMO rain mapping");
    c.weather=WeatherMode::Clear;require(resolvedWeather(c,rain,10).rain==0&&resolvedWeather(c,rain,10).cloud==0,"Manual clear must override shared rain");
    c.weather=WeatherMode::Snow;require(resolvedWeather(c,rain,10).snow==1&&resolvedWeather(c,rain,10).rain==0,"Manual snow contract");
    require(desktopShader("#version 300 es\nprecision highp float;\nuniform highp sampler2D x;\nvoid main(){}").find("#define gl_FragCoord wpFrag()")!=std::string::npos,"Tiled shader origin");
    const auto profile=std::filesystem::temp_directory_path()/L"tlauncher-native-trial-wallpaper-contract";
    std::filesystem::create_directories(profile);std::ofstream(profile/L".tlauncher-native-trial",std::ios::binary)<<"tlauncher-native isolated profile v1\n";
    SetEnvironmentVariableW(L"TLAUNCHER_NATIVE_TRIAL_DIR",profile.c_str());require(tlauncher::validateTrial(),"Invalid isolated contract profile");
    std::ofstream(profile/L"wallpaper-v1.ini")<<"schema=1\nenabled=1\nscene=lake\nunknown.future=retain\nfps=25\n";
    Store store(profile);c=store.load();require(c.enabled&&c.scene==sceneIndex("lake"),"Config read");c.fps=30;
    const auto connected=topology();require(!connected.empty(),"Hosted display missing");auto&assignment=c.displays[connected.front().id];assignment.scene=sceneIndex("aquarium");assignment.sceneParams["aquarium"]["plants"]=.85;
    require(store.save(c),"Atomic configuration save");Store reload(profile);auto roundtrip=reload.load();require(roundtrip.displays[connected.front().id].scene==sceneIndex("aquarium")&&abs(roundtrip.displays[connected.front().id].sceneParams["aquarium"]["plants"]-.85)<.0001,"Monitor parameter roundtrip");
    std::ifstream saved(profile/L"wallpaper-v1.ini");std::string text((std::istreambuf_iterator<char>(saved)),{});require(text.find("unknown.future=retain")!=std::string::npos,"Unknown field lost");require(std::filesystem::exists(profile/L"wallpaper-v1.ini.bak"),"Backup absent");
    saved.close();std::ofstream(profile/L"wallpaper-v1.ini",std::ios::app)<<"monitor.disconnected.param.aquarium.plants=0.31\n";Store unplugged(profile);auto retained=unplugged.load();require(unplugged.save(retained),"Disconnected save");std::ifstream preserved(profile/L"wallpaper-v1.ini");std::string disconnected((std::istreambuf_iterator<char>(preserved)),{});require(disconnected.find("monitor.disconnected.param.aquarium.plants=0.31")!=std::string::npos,"Disconnected monitor fields lost");preserved.close();
    std::ofstream(profile/L"wallpaper-v1.ini")<<"schema=900\nenabled=1\n";Store future(profile);c=future.load();require(!c.enabled&&future.readOnly()&&!future.save(c),"Future schema must remain intact");
    std::cout<<"CATALOG_16=PASS\nNEGATIVE_PORTRAIT_MIXED_DPI=PASS\nTILED_PANORAMA=PASS\nMULTIWINDOW_OCCLUSION=PASS\nSOLAR_WEATHER=PASS\nATOMIC_CONFIG_BACKUP_UNKNOWN_FIELDS=PASS\nFUTURE_SCHEMA_PROTECTION=PASS\nPHASE_2_COMPLETE=NO\n";return 0;
}catch(const std::exception&e){std::cerr<<e.what()<<'\n';return 1;}}
