// FluidWin widget.h RSS decoding and publication time conversion, MIT.
// The existing Rust network owner supplies bounded HTTPS bytes on demand.
#include <cwctype>
#include <utility>
#include "upstream/widget.h"
#include <new>
namespace widget {
struct TLauncherWidgetBridge {
    static std::vector<Notizia> parse(const std::string& xml) {
        Istantanea snapshot;Dati::notizieDaXml(snapshot,xml);
        for(auto& n:snapshot.notizie){
            if(n.titolo.size()>2048)n.titolo.resize(2048);
            if(n.link.size()>2048)n.link.clear();
        }
        return std::move(snapshot.notizie);
    }
};
}
using News=std::vector<widget::Notizia>;
extern "C" void* tlauncher_news_parse(const char* bytes,unsigned size) noexcept {
    if(!bytes||size>1024*1024)return nullptr;
    try{return new News(widget::TLauncherWidgetBridge::parse(std::string(bytes,size)));}catch(...){return nullptr;}
}
extern "C" void tlauncher_news_free(void* raw) noexcept {delete static_cast<News*>(raw);}
extern "C" unsigned tlauncher_news_count(void* raw) noexcept {return raw?static_cast<unsigned>(static_cast<News*>(raw)->size()):0;}
static void copy(const std::wstring& value,wchar_t* out,unsigned size) noexcept {
    if(!out||size==0)return;const size_t n=std::min(value.size(),static_cast<size_t>(size-1));
    std::copy_n(value.data(),n,out);out[n]=0;
}
extern "C" bool tlauncher_news_item(void* raw,unsigned index,wchar_t* title,wchar_t* link,wchar_t* time,unsigned size) noexcept {
    if(!raw||index>=static_cast<News*>(raw)->size()||size<2)return false;
    const auto& n=(*static_cast<News*>(raw))[index];copy(n.titolo,title,size);copy(n.link,link,size);copy(n.quando,time,size);return true;
}
