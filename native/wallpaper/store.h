// Versioned Phase 2 configuration, isolated profile only. Unknown keys survive.
#pragma once
#include "catalog.h"
#include "../fluidwin/tlauncher_identity.h"
#include <filesystem>
#include <fstream>
#include <sstream>
#include <locale>

namespace wallpaper {
class Store {
    using Fields=std::map<std::string,std::string>;Fields fields;
    std::filesystem::path file;bool future=false;
    static bool regularPath(const std::filesystem::path&p){const DWORD a=GetFileAttributesW(p.c_str());return a==INVALID_FILE_ATTRIBUTES||!(a&(FILE_ATTRIBUTE_REPARSE_POINT|FILE_ATTRIBUTE_DIRECTORY));}
    static Fields parse(const std::string&text){
        Fields out;std::istringstream in(text);std::string line;
        while(std::getline(in,line)){if(!line.empty()&&line.back()=='\r')line.pop_back();if(line.empty()||line[0]=='#')continue;auto p=line.find('=');if(p==std::string::npos||p==0||p>200||line.size()>4096||out.size()>=4096)continue;out[line.substr(0,p)]=line.substr(p+1);}return out;
    }
    static std::string utf8(const std::wstring&w){if(w.empty())return {};int n=WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,w.data(),int(w.size()),nullptr,0,nullptr,nullptr);std::string s(n,'\0');if(n)WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,w.data(),int(w.size()),s.data(),n,nullptr,nullptr);return s;}
    static std::string read(const std::filesystem::path&p){std::ifstream f(p,std::ios::binary);if(!f)return {};f.seekg(0,std::ios::end);auto n=f.tellg();if(n<0||n>512*1024)return {};std::string s(size_t(n),'\0');f.seekg(0);if(n&&!f.read(s.data(),n))return {};return s;}
    static bool write(const std::filesystem::path&p,const std::string&s){
        HANDLE h=CreateFileW(p.c_str(),GENERIC_WRITE,0,nullptr,CREATE_NEW,FILE_ATTRIBUTE_NORMAL|FILE_FLAG_WRITE_THROUGH,nullptr);if(h==INVALID_HANDLE_VALUE)return false;
        DWORD n=0;bool ok=WriteFile(h,s.data(),DWORD(s.size()),&n,nullptr)&&n==s.size()&&FlushFileBuffers(h);CloseHandle(h);return ok;
    }
    double number(const std::string&key,double fallback)const{auto i=fields.find(key);if(i==fields.end())return fallback;try{size_t n;double d=std::stod(i->second,&n);return n==i->second.size()&&std::isfinite(d)?d:fallback;}catch(...){return fallback;}}
public:
    explicit Store(std::filesystem::path profile=tlauncher::profile()):file(profile/L"wallpaper-v1.ini"){}
    Config load(){Config c;if(!regularPath(file)||!regularPath(file.wstring()+L".bak")){future=true;return c;}fields=parse(read(file));if(!fields.count("schema")){auto backup=parse(read(file.wstring()+L".bak"));if(backup.count("schema"))fields=std::move(backup);}
        future=number("schema",1)>1;if(future)return c;
        c.enabled=number("enabled",0)!=0;c.paused=number("paused",0)!=0;c.fps=int(number("fps",25));c.scene=sceneIndex(fields["scene"]);c.pauseCovered=number("pauseCovered",1)!=0;
        c.layout=Layout(int(number("layout",1)));c.weather=WeatherMode(int(number("weather",0)));c.solar=number("solar",1)!=0;c.clockLighting=number("clockLighting",1)!=0;
        c.resolution=float(number("resolution",1));c.manualHour=float(number("hour",12));c.wind=float(number("wind",1));
        for(const auto&s:catalog())for(const auto&p:s.params)c.sceneParams[s.id][p.key]=number(std::string("param.")+s.id+"."+p.key,p.initial);
        for(const auto&m:topology()){const std::string key="monitor."+utf8(m.id)+".";Assignment a;a.enabled=number(key+"enabled",1)!=0;auto i=fields.find(key+"scene");a.scene=i==fields.end()?c.scene:sceneIndex(i->second);
            for(const auto&s:catalog())for(const auto&p:s.params){auto field=key+"param."+s.id+"."+p.key;if(fields.count(field))a.sceneParams[s.id][p.key]=number(field,p.initial);}
            c.displays[m.id]=a;}
        validate(c);return c;
    }
    bool save(Config c){if(future||!tlauncher::validateTrial()||!regularPath(file)||!regularPath(file.wstring()+L".bak"))return false;validate(c);
        auto set=[&](const std::string&k,double v){std::ostringstream s;s.imbue(std::locale::classic());s.precision(12);s<<v;fields[k]=s.str();};
        set("schema",1);set("enabled",c.enabled);set("paused",c.paused);set("fps",c.fps);fields["scene"]=catalog()[c.scene].id;set("pauseCovered",c.pauseCovered);set("layout",int(c.layout));set("weather",int(c.weather));set("solar",c.solar);set("clockLighting",c.clockLighting);set("resolution",c.resolution);set("hour",c.manualHour);set("wind",c.wind);
        for(size_t i=0;i<catalog().size();++i){const auto&s=catalog()[i];for(const auto&p:s.params)set(std::string("param.")+s.id+"."+p.key,parameter(c,int(i),p.key));}
        for(const auto&[id,a]:c.displays){const std::string key="monitor."+utf8(id)+".";set(key+"enabled",a.enabled);fields[key+"scene"]=catalog()[a.scene].id;
            for(const auto&[scene,params]:a.sceneParams){int index=sceneIndex(scene);if(index<0)continue;Config monitorConfig=c;monitorConfig.sceneParams[scene]=params;for(const auto&p:catalog()[index].params)set(key+"param."+scene+"."+p.key,parameter(monitorConfig,index,p.key));}}
        std::string text="# TLauncher native wallpaper; unknown keys are retained\n";for(const auto&[k,v]:fields)text+=k+"="+v+"\n";
        auto tmp=std::filesystem::path(file.wstring()+L".tmp-"+std::to_wstring(GetCurrentProcessId())+L"-"+std::to_wstring(GetTickCount64()));
        if(!write(tmp,text)){DeleteFileW(tmp.c_str());return false;}
        bool ok;if(GetFileAttributesW(file.c_str())!=INVALID_FILE_ATTRIBUTES)ok=ReplaceFileW(file.c_str(),tmp.c_str(),(file.wstring()+L".bak").c_str(),0,nullptr,nullptr)!=FALSE;
        else ok=MoveFileExW(tmp.c_str(),file.c_str(),MOVEFILE_WRITE_THROUGH)!=FALSE;
        if(!ok)DeleteFileW(tmp.c_str());return ok;
    }
    bool readOnly()const{return future;}
};
}
