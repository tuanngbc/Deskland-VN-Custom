// Adapted sensori.h::microfono/webcam/inUso, exact upstream recorded in ADAPTATIONS.
// Read-only, bounded snapshot. No retained COM singleton or recurring timer.
#include <windows.h>
#include <mmdeviceapi.h>
#include <audiopolicy.h>
#include <tlhelp32.h>
#include <wrl/client.h>
#include <string>
#include <vector>
#include <algorithm>
#include <cwctype>
namespace {
using Microsoft::WRL::ComPtr;
constexpr unsigned LIMIT=16,NAME=257;
struct Result{int micStatus=0,camStatus=0;unsigned micCount=0,camCount=0;wchar_t mic[LIMIT][NAME]{},cam[LIMIT][NAME]{};};
struct Key{HKEY value=nullptr;~Key(){if(value)RegCloseKey(value);}};
struct Processes{HANDLE value=INVALID_HANDLE_VALUE;~Processes(){if(value!=INVALID_HANDLE_VALUE)CloseHandle(value);}};
std::wstring clean(std::wstring name){
    name.erase(std::remove_if(name.begin(),name.end(),[](wchar_t c){return c<32||c==127;}),name.end());
    if(name.size()>NAME-1)name.resize(NAME-1);return name;
}
std::wstring pretty(std::wstring name){auto dot=name.rfind(L'.');if(dot!=std::wstring::npos&&dot>0)name.resize(dot);if(!name.empty())name[0]=wchar_t(towupper(name[0]));return clean(std::move(name));}
void add(wchar_t (&rows)[LIMIT][NAME],unsigned& count,int& status,const std::wstring& source){
    auto name=clean(source);if(name.empty()){status=2;return;}
    for(unsigned i=0;i<count;i++)if(!_wcsicmp(rows[i],name.c_str()))return;
    if(count==LIMIT){status=2;return;}std::copy(name.begin(),name.end(),rows[count]);rows[count++][name.size()]=0;
}
std::wstring process_name(DWORD pid,Processes& snapshot){
    if(snapshot.value==INVALID_HANDLE_VALUE)snapshot.value=CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0);
    if(snapshot.value==INVALID_HANDLE_VALUE)return L"";
    PROCESSENTRY32W entry{};entry.dwSize=sizeof(entry);unsigned count=0;
    for(BOOL ok=Process32FirstW(snapshot.value,&entry);ok&&count++<4096;ok=Process32NextW(snapshot.value,&entry))if(entry.th32ProcessID==pid)return pretty(entry.szExeFile);
    return L"";
}
void microphone(Result& out){
    ComPtr<IMMDeviceEnumerator> enumerator;
    if(FAILED(CoCreateInstance(__uuidof(MMDeviceEnumerator),nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&enumerator))))return;
    ComPtr<IMMDeviceCollection> devices;if(FAILED(enumerator->EnumAudioEndpoints(eCapture,DEVICE_STATE_ACTIVE,&devices)))return;
    UINT count=0;if(FAILED(devices->GetCount(&count)))return;out.micStatus=count>32?2:1;Processes processes;
    for(UINT i=0;i<std::min(count,UINT(32));i++){
        ComPtr<IMMDevice> device;ComPtr<IAudioSessionManager2> manager;ComPtr<IAudioSessionEnumerator> sessions;
        if(FAILED(devices->Item(i,&device))||FAILED(device->Activate(__uuidof(IAudioSessionManager2),CLSCTX_INPROC_SERVER,nullptr,reinterpret_cast<void**>(manager.GetAddressOf())))||FAILED(manager->GetSessionEnumerator(&sessions))){out.micStatus=2;continue;}
        int n=0;if(FAILED(sessions->GetCount(&n))||n<0){out.micStatus=2;continue;}if(n>256)out.micStatus=2;
        for(int j=0;j<std::min(n,256);j++){
            ComPtr<IAudioSessionControl> session;AudioSessionState state=AudioSessionStateInactive;
            if(FAILED(sessions->GetSession(j,&session))||FAILED(session->GetState(&state))){out.micStatus=2;continue;}
            if(state!=AudioSessionStateActive)continue;
            ComPtr<IAudioSessionControl2> details;DWORD pid=0;
            if(FAILED(session.As(&details))||FAILED(details->GetProcessId(&pid))){out.micStatus=2;continue;}
            if(!pid||pid==GetCurrentProcessId()||details->IsSystemSoundsSession()==S_OK)continue;
            auto name=process_name(pid,processes);if(name.empty()){name=L"PID "+std::to_wstring(pid);out.micStatus=2;}
            add(out.mic,out.micCount,out.micStatus,name);
        }
    }
}
std::wstring camera_name(std::wstring name,bool desktop){
    if(desktop){auto pos=name.find_last_of(L"#\\/");if(pos!=std::wstring::npos)name=name.substr(pos+1);return pretty(std::move(name));}
    auto underscore=name.find(L'_');if(underscore!=std::wstring::npos)name.resize(underscore);
    auto dot=name.rfind(L'.');if(dot!=std::wstring::npos)name=name.substr(dot+1);return clean(std::move(name));
}
bool camera_root(const wchar_t* path,bool desktop,Result& out){
    Key root;auto opened=RegOpenKeyExW(HKEY_CURRENT_USER,path,0,KEY_READ,&root.value);
    if(opened==ERROR_FILE_NOT_FOUND)return false;if(opened!=ERROR_SUCCESS){out.camStatus=2;return false;}
    for(DWORD i=0;i<500;i++){
        wchar_t name[512]{};DWORD length=512;auto result=RegEnumKeyExW(root.value,i,name,&length,nullptr,nullptr,nullptr,nullptr);
        if(result==ERROR_NO_MORE_ITEMS)return true;if(result!=ERROR_SUCCESS){out.camStatus=2;continue;}
        if(!desktop&&!_wcsicmp(name,L"NonPackaged"))continue;
        ULONGLONG start=0,stop=0;DWORD bytes=sizeof(start);
        result=RegGetValueW(root.value,name,L"LastUsedTimeStart",RRF_RT_REG_QWORD,nullptr,&start,&bytes);
        if(result==ERROR_FILE_NOT_FOUND)continue;if(result!=ERROR_SUCCESS){out.camStatus=2;continue;}
        bytes=sizeof(stop);result=RegGetValueW(root.value,name,L"LastUsedTimeStop",RRF_RT_REG_QWORD,nullptr,&stop,&bytes);
        if(result!=ERROR_SUCCESS&&result!=ERROR_FILE_NOT_FOUND){out.camStatus=2;continue;}
        if(start&&start>stop)add(out.cam,out.camCount,out.camStatus,camera_name(std::wstring(name,length),desktop));
    }
    out.camStatus=2;return true;
}
}
extern "C" {
bool tlauncher_sensors_read(Result* out)noexcept{
    if(!out)return false;*out=Result{};try{
        microphone(*out);out->camStatus=1;
        const auto exists=camera_root(L"Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\webcam",false,*out);
        camera_root(L"Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\webcam\\NonPackaged",true,*out);
        // Windows does not guarantee that this registry telemetry is present,
        // current or exhaustive. An empty report is never a privacy guarantee.
        if(!exists&&out->camCount==0)out->camStatus=0;
        return true;
    }catch(...){*out=Result{};return false;}
}
bool tlauncher_sensors_fixture()noexcept{
    try{
        Result out;out.micStatus=1;
        add(out.mic,out.micCount,out.micStatus,L"Meeting");add(out.mic,out.micCount,out.micStatus,L"meeting");if(out.micCount!=1)return false;
        for(unsigned i=0;i<20;i++)add(out.mic,out.micCount,out.micStatus,L"App "+std::to_wstring(i));if(out.micCount!=LIMIT||out.micStatus!=2)return false;
        if(camera_name(L"C:#Apps#Meeting.exe",true)!=L"Meeting"||camera_name(L"Vendor.Camera_123",false)!=L"Camera")return false;
        if(clean(L"A\r\nB\t")!=L"AB"||clean(std::wstring(600,L'x')).size()!=NAME-1)return false;
        return true;
    }catch(...){return false;}
}
}
