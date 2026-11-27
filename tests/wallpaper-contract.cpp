// GitHub-hosted only; no desktop mutation or production data access.
#include "../native/wallpaper/store.h"
#include "../native/wallpaper/field.h"
#include "../native/wallpaper/scene.h"
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
    require(covered({0,0,100,100},{{0,0,50,100},{50,0,100,100}}),"Multiwindow occlusion");
    require(!covered({0,0,100,100},{{0,0,49,100},{50,0,100,100}}),"Visible gap must render");
    Config c;c.fps=999;c.scene=-10;c.resolution=std::numeric_limits<float>::quiet_NaN();validate(c);require(c.fps==60&&c.scene==0&&c.resolution==1,"Unsafe configuration");
    auto sun=solar(1794718800.,10.94469,106.82432);require(sun.up>.8f,"Solar noon direction");
    auto snow=fromWmo(73,20,10,106);require(snow.snow==1&&snow.rain==0,"WMO snow mapping");
    auto rain=fromWmo(63,20,10,106);require(rain.rain==1&&rain.snow==0,"WMO rain mapping");
    require(desktopShader("#version 300 es\nprecision highp float;\nuniform highp sampler2D x;\nvoid main(){}").find("#define gl_FragCoord wpFrag()")!=std::string::npos,"Tiled shader origin");
    const auto profile=std::filesystem::temp_directory_path()/L"tlauncher-native-trial-wallpaper-contract";
    std::filesystem::create_directories(profile);std::ofstream(profile/L".tlauncher-native-trial",std::ios::binary)<<"tlauncher-native isolated profile v1\n";
    SetEnvironmentVariableW(L"TLAUNCHER_NATIVE_TRIAL_DIR",profile.c_str());require(tlauncher::validateTrial(),"Invalid isolated contract profile");
    std::ofstream(profile/L"wallpaper-v1.ini")<<"schema=1\nenabled=1\nscene=lake\nunknown.future=retain\nfps=25\n";
    Store store(profile);c=store.load();require(c.enabled&&c.scene==sceneIndex("lake"),"Config read");c.fps=30;require(store.save(c),"Atomic configuration save");
    std::ifstream saved(profile/L"wallpaper-v1.ini");std::string text((std::istreambuf_iterator<char>(saved)),{});require(text.find("unknown.future=retain")!=std::string::npos,"Unknown field lost");require(std::filesystem::exists(profile/L"wallpaper-v1.ini.bak"),"Backup absent");
    std::ofstream(profile/L"wallpaper-v1.ini")<<"schema=900\nenabled=1\n";Store future(profile);c=future.load();require(!c.enabled&&future.readOnly()&&!future.save(c),"Future schema must remain intact");
    std::cout<<"CATALOG_16=PASS\nNEGATIVE_PORTRAIT_MIXED_DPI=PASS\nTILED_PANORAMA=PASS\nMULTIWINDOW_OCCLUSION=PASS\nSOLAR_WEATHER=PASS\nATOMIC_CONFIG_BACKUP_UNKNOWN_FIELDS=PASS\nFUTURE_SCHEMA_PROTECTION=PASS\nPHASE_2_COMPLETE=NO\n";return 0;
}catch(const std::exception&e){std::cerr<<e.what()<<'\n';return 1;}}
