// Original FluidWin mixer owns endpoint/session COM references on one worker.
#include <windows.h>
#include <propsys.h>
#include <cstdint>
#include <cmath>
#include "upstream/mixer.h"
#include "upstream/sistema.h"

namespace {
struct Row { size_t output; int app; };
struct Model {
    mixer::Mixer audio;
    std::vector<sistema::Sistema::Uscita> outputs;
    std::vector<Row> rows;
    uint64_t generation=0;
    void read(){
        audio.leggi();outputs=sistema::Sistema::uscite();rows.clear();++generation;
        for(size_t u=0;u<audio.uscite.size()&&rows.size()<256;++u){
            rows.push_back({u,-1});
            for(size_t a=0;a<audio.uscite[u].app.size()&&rows.size()<256;++a)rows.push_back({u,int(a)});
        }
    }
};
void copy(const std::wstring& s,wchar_t* out,unsigned capacity){if(out&&capacity){wcsncpy_s(out,capacity,s.c_str(),_TRUNCATE);}}
}
extern "C" {
void* tlauncher_audio_create() noexcept {try{return new Model();}catch(...){return nullptr;}}
void tlauncher_audio_destroy(void* raw) noexcept {delete static_cast<Model*>(raw);}
uint64_t tlauncher_audio_read(void* raw) noexcept {try{auto& m=*static_cast<Model*>(raw);m.read();return m.generation;}catch(...){return 0;}}
unsigned tlauncher_audio_count(void* raw,bool outputs) noexcept {auto& m=*static_cast<Model*>(raw);return unsigned(outputs?m.outputs.size():m.rows.size());}
bool tlauncher_audio_row(void* raw,unsigned i,wchar_t* label,unsigned capacity,int* level,int* flags) noexcept {
    try{
        auto& m=*static_cast<Model*>(raw);if(i>=m.rows.size())return false;
        const auto r=m.rows[i];const auto& u=m.audio.uscite[r.output];
        const auto* a=r.app<0?nullptr:&u.app[r.app];
        copy(a?a->nome:u.nome,label,capacity);
        *level=int(std::round(std::clamp(a?a->volume:u.volume,0.0f,1.0f)*100));
        *flags=(a?0:1)|((a?a->muto:u.muto)?2:0)|(u.predefinita?4:0);return true;
    }catch(...){return false;}
}
bool tlauncher_audio_output(void* raw,unsigned i,wchar_t* label,unsigned capacity,bool* is_default) noexcept {
    try{auto& m=*static_cast<Model*>(raw);if(i>=m.outputs.size())return false;const auto& u=m.outputs[i];copy(u.nome,label,capacity);*is_default=u.predefinita;return true;}catch(...){return false;}
}
bool tlauncher_audio_key(void* raw,unsigned i,wchar_t* out,unsigned capacity) noexcept {
    try{
        auto& m=*static_cast<Model*>(raw);if(i>=m.rows.size())return false;
        const auto r=m.rows[i];const auto& u=m.audio.uscite[r.output];
        auto key=u.id+L"\n";
        if(r.app<0)key+=L"endpoint";
        else{const auto& a=u.app[r.app];key+=a.sistema?L"system":L"app\n"+a.exe+L"\n"+a.nome;}
        if(key.size()+1>capacity)return false;copy(key,out,capacity);return true;
    }catch(...){return false;}
}
// Generation is checked before resolving indices: hotplug/refresh cannot send
// a late slider gesture to an unrelated endpoint or a different application.
bool tlauncher_audio_change(void* raw,uint64_t generation,unsigned row,int operation,int value) noexcept {
    try{
        auto& m=*static_cast<Model*>(raw);if(!generation||generation!=m.generation)return false;
        if(operation==2){return row<m.outputs.size()&&sistema::Sistema::impostaUscita(m.outputs[row].id);}
        if(row>=m.rows.size())return false;const auto r=m.rows[row];
        if(operation==0){m.audio.volume(r.output,r.app,std::clamp(value,0,100)/100.0f);return true;}
        if(operation==1){m.audio.muto(r.output,r.app);return true;}return false;
    }catch(...){return false;}
}

// Synthetic COM sessions; never writes the runner's real audio state.
int tlauncher_audio_group_fixture() noexcept {
    struct Volume final:ISimpleAudioVolume {
        ULONG refs=1;float volume=0.6f;BOOL muted=FALSE;
        HRESULT STDMETHODCALLTYPE QueryInterface(REFIID,void**) override{return E_NOINTERFACE;}
        ULONG STDMETHODCALLTYPE AddRef() override{return ++refs;}
        ULONG STDMETHODCALLTYPE Release() override{return --refs;}
        HRESULT STDMETHODCALLTYPE SetMasterVolume(float value,LPCGUID) override{volume=value;return S_OK;}
        HRESULT STDMETHODCALLTYPE GetMasterVolume(float* value) override{*value=volume;return S_OK;}
        HRESULT STDMETHODCALLTYPE SetMute(BOOL value,LPCGUID) override{muted=value;return S_OK;}
        HRESULT STDMETHODCALLTYPE GetMute(BOOL* value) override{*value=muted;return S_OK;}
    } a,b;
    int result=0;
    {
        Model m;m.generation=1;m.audio.uscite.emplace_back();
        mixer::App app;app.nome=L"Synthetic grouped sessions";app.vol={&a,&b};a.AddRef();b.AddRef();
        m.audio.uscite[0].app.push_back(std::move(app));m.rows.push_back({0,0});
        if(!tlauncher_audio_change(&m,1,0,0,137)||a.volume!=1||b.volume!=1)result|=1;
        if(!tlauncher_audio_change(&m,1,0,1,0)||!a.muted||!b.muted)result|=2;
        if(tlauncher_audio_change(&m,2,0,0,5)||a.volume!=1)result|=4;
        if(!tlauncher_audio_change(&m,1,0,0,-9)||a.volume!=0||b.volume!=0)result|=8;
    }
    if(a.refs!=1||b.refs!=1)result|=16;
    return result;
}
}
