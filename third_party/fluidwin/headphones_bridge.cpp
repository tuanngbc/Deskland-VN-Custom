// Adapted sensori.h::uscite/cuffie. Endpoint events replace the upstream poll.
// The subscription never enumerates devices in a callback; reads run on a
// bounded Windows pool callback, with no audio activation or setting writes.
#include <windows.h>
#include <mmdeviceapi.h>
#include <setupapi.h>
#include <devpropdef.h>
#include <propvarutil.h>
#include <wrl/client.h>
#include <atomic>
#include <string>
#include <vector>
#include <algorithm>
namespace {
using Microsoft::WRL::ComPtr;
constexpr unsigned LIMIT=64;
struct Row {wchar_t id[1025]{},name[257]{};GUID container{};int bluetooth=0,battery=-1;};
struct Result {unsigned count=0;Row rows[LIMIT]{};};
const PROPERTYKEY CONTAINER={{0x8c7ed206,0x3f8a,0x4827,{0xb3,0xab,0xae,0x9e,0x1f,0xae,0xfc,0x6c}},2};
const PROPERTYKEY NAME={{0xa45c254e,0xdf1c,0x4efd,{0x80,0x20,0x67,0xd1,0x46,0xa8,0x50,0xe0}},14};
const DEVPROPKEY DEVICE_CONTAINER={CONTAINER.fmtid,2};
const DEVPROPKEY BATTERY={{0x104ea319,0x6ee2,0x4701,{0xbd,0x47,0x8d,0xdb,0xf4,0x25,0xbb,0xe5}},2};
struct Variant {PROPVARIANT v{};~Variant(){PropVariantClear(&v);}};
struct Devices {HDEVINFO h=INVALID_HANDLE_VALUE;~Devices(){if(h!=INVALID_HANDLE_VALUE)SetupDiDestroyDeviceInfoList(h);}};
bool bluetoothId(const wchar_t* id){return !_wcsnicmp(id,L"BTHENUM",7)||!_wcsnicmp(id,L"BTHLE",5)||!_wcsnicmp(id,L"BTHHFENUM",9);}
std::wstring displayName(std::wstring name){
    const auto a=name.find(L'('),b=name.rfind(L')');
    if(a!=std::wstring::npos&&b!=std::wstring::npos&&b>a+1)name=name.substr(a+1,b-a-1);
    for(auto& c:name)if(c<32||c==127)c=L' ';
    if(name.size()>256)name.resize(256);
    return name;
}
// Kept alive explicitly until unregister completes, as required by MMDevice.
// https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nn-mmdeviceapi-immnotificationclient
class Watch final:public IMMNotificationClient {
    std::atomic<ULONG> refs{1};
public:
    std::atomic<HWND> target{nullptr};std::atomic<bool> queued{false};UINT message=0;UINT_PTR ticket=0;
    ComPtr<IMMDeviceEnumerator> enumerator;
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid,void** out)override{
        if(!out)return E_POINTER;*out=nullptr;
        if(iid!=IID_IUnknown&&iid!=__uuidof(IMMNotificationClient))return E_NOINTERFACE;
        *out=static_cast<IMMNotificationClient*>(this);AddRef();return S_OK;
    }
    ULONG STDMETHODCALLTYPE AddRef()override{return ++refs;}
    ULONG STDMETHODCALLTYPE Release()override{auto n=--refs;if(!n)delete this;return n;}
    HRESULT signal(){auto h=target.load();if(h&&!queued.exchange(true)&&!PostMessageW(h,message,ticket,0))queued=false;return S_OK;}
    HRESULT STDMETHODCALLTYPE OnDeviceStateChanged(LPCWSTR,DWORD)override{return signal();}
    HRESULT STDMETHODCALLTYPE OnDeviceAdded(LPCWSTR)override{return signal();}
    HRESULT STDMETHODCALLTYPE OnDeviceRemoved(LPCWSTR)override{return signal();}
    HRESULT STDMETHODCALLTYPE OnDefaultDeviceChanged(EDataFlow,ERole,LPCWSTR)override{return signal();}
    HRESULT STDMETHODCALLTYPE OnPropertyValueChanged(LPCWSTR,const PROPERTYKEY)override{return signal();}
};
thread_local Watch* watch=nullptr;
bool stop(){
    if(!watch)return true;
    watch->target=nullptr;
    // If Windows refuses unregister, retain this one inert object and refuse
    // another registration. Never free memory still callable by MMDevice.
    if(FAILED(watch->enumerator->UnregisterEndpointNotificationCallback(watch)))return false;
    auto old=watch;watch=nullptr;old->enumerator.Reset();old->Release();return true;
}
}
extern "C" bool tlauncher_headphones_watch_start(HWND target,UINT message,UINT_PTR ticket)noexcept{
    if(!target||!ticket||!stop())return false;
    try{
        auto next=new Watch;
        if(FAILED(CoCreateInstance(__uuidof(MMDeviceEnumerator),nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&next->enumerator)))){next->Release();return false;}
        next->target=target;next->message=message;next->ticket=ticket;
        if(FAILED(next->enumerator->RegisterEndpointNotificationCallback(next))){next->Release();return false;}
        watch=next;return true;
    }catch(...){return false;}
}
extern "C" bool tlauncher_headphones_watch_stop()noexcept{return stop();}
extern "C" void tlauncher_headphones_watch_ack(UINT_PTR ticket)noexcept{if(watch&&watch->ticket==ticket)watch->queued=false;}
extern "C" bool tlauncher_headphones_read(Result* out)noexcept{
    if(!out)return false;*out=Result{};
    try{
        ComPtr<IMMDeviceEnumerator> en;ComPtr<IMMDeviceCollection> collection;
        if(FAILED(CoCreateInstance(__uuidof(MMDeviceEnumerator),nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&en)))||FAILED(en->EnumAudioEndpoints(eRender,DEVICE_STATE_ACTIVE,&collection)))return false;
        UINT count=0;if(FAILED(collection->GetCount(&count))||count>LIMIT)return false;
        for(UINT i=0;i<count;++i){
            ComPtr<IMMDevice> device;if(FAILED(collection->Item(i,&device)))return false;
            LPWSTR id=nullptr;const auto hr=device->GetId(&id);
            if(FAILED(hr)||!id){CoTaskMemFree(id);return false;}
            auto& row=out->rows[i];const auto length=wcsnlen_s(id,1025);
            if(!length||length>1024){CoTaskMemFree(id);return false;}
            wmemcpy(row.id,id,length);CoTaskMemFree(id);
            ComPtr<IPropertyStore> props;
            if(SUCCEEDED(device->OpenPropertyStore(STGM_READ,&props))){
                Variant container,name;
                if(SUCCEEDED(props->GetValue(CONTAINER,&container.v))&&container.v.vt==VT_CLSID&&container.v.puuid)row.container=*container.v.puuid;
                if(SUCCEEDED(props->GetValue(NAME,&name.v))&&name.v.vt==VT_LPWSTR&&name.v.pwszVal){
                    auto label=displayName(std::wstring(name.v.pwszVal,wcsnlen_s(name.v.pwszVal,4096)));wmemcpy(row.name,label.c_str(),label.size());
                }
            }
        }
        // One read-only PnP snapshot for all endpoints, rather than a scan for
        // every endpoint as in the original. Missing properties stay unknown.
        Devices devices;devices.h=SetupDiGetClassDevsW(nullptr,nullptr,nullptr,DIGCF_ALLCLASSES|DIGCF_PRESENT);
        if(devices.h!=INVALID_HANDLE_VALUE){
            SP_DEVINFO_DATA di{};di.cbSize=sizeof(di);
            for(DWORD i=0;i<5000&&SetupDiEnumDeviceInfo(devices.h,i,&di);++i){
                DEVPROPTYPE type=0;GUID container{};
                if(!SetupDiGetDevicePropertyW(devices.h,&di,&DEVICE_CONTAINER,&type,reinterpret_cast<BYTE*>(&container),sizeof(container),nullptr,0)||type!=DEVPROP_TYPE_GUID)continue;
                wchar_t instance[512]{};const bool bt=SetupDiGetDeviceInstanceIdW(devices.h,&di,instance,512,nullptr)&&bluetoothId(instance);
                BYTE battery=0;const bool hasBattery=SetupDiGetDevicePropertyW(devices.h,&di,&BATTERY,&type,&battery,sizeof(battery),nullptr,0)&&type==DEVPROP_TYPE_BYTE&&battery<=100;
                for(UINT j=0;j<count;++j){auto& row=out->rows[j];if(row.container==GUID{}||row.container!=container)continue;if(bt)row.bluetooth=1;if(hasBattery)row.battery=battery;}
            }
        }
        out->count=count;return true;
    }catch(...){*out=Result{};return false;}
}
extern "C" bool tlauncher_headphones_fixture(HWND target,UINT message)noexcept{
    wchar_t hosted[8]{};if(GetEnvironmentVariableW(L"GITHUB_ACTIONS",hosted,8)!=4||wcscmp(hosted,L"true"))return false;
    if(!tlauncher_headphones_watch_start(target,message,901))return false;
    auto held=watch;held->AddRef();for(int i=0;i<50;++i)held->OnDeviceStateChanged(L"synthetic-only",DEVICE_STATE_ACTIVE);
    MSG msg{};unsigned first=0;while(PeekMessageW(&msg,target,message,message,PM_REMOVE))++first;
    tlauncher_headphones_watch_ack(901);held->OnDeviceAdded(L"synthetic-only");unsigned second=0;while(PeekMessageW(&msg,target,message,message,PM_REMOVE))++second;
    const bool stopped=stop();while(PeekMessageW(&msg,target,message,message,PM_REMOVE)){}
    held->OnDeviceRemoved(L"synthetic-only");const bool late=PeekMessageW(&msg,target,message,message,PM_REMOVE)!=0;held->Release();
    return first==1&&second==1&&stopped&&!late&&displayName(L"Headphones (Synthetic headset)")==L"Synthetic headset"&&bluetoothId(L"BTHLE\\synthetic")&&!bluetoothId(L"USB\\synthetic");
}
