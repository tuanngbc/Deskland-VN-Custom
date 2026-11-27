// Shared pixel cache. Extraction only runs in the existing background workers.
// UI callers use peek(); D2D bitmaps remain owned by their render target.
#pragma once
#include "tlauncher_identity.h"
#include <vector>
#include <unordered_map>
#include <mutex>
#include <fstream>
#include <memory>
#include <algorithm>
#include <cstdint>

namespace appicons {
struct Pixels {int w=0,h=0;std::vector<UINT> bgra;};
struct Entry {std::shared_ptr<const Pixels> pixels;ULONGLONG used=0;};
inline std::mutex mutex;
inline std::unordered_map<std::wstring,Entry> memory;
inline constexpr size_t budget=24*1024*1024;
inline std::wstring key(const std::wstring& source,int size) {
    std::wstring s=source;for(auto& c:s)c=(wchar_t)towlower(c);
    WIN32_FILE_ATTRIBUTE_DATA a{};
    if(GetFileAttributesExW(source.c_str(),GetFileExInfoStandard,&a))
        s+=L"|"+std::to_wstring(a.ftLastWriteTime.dwHighDateTime)+L":"+std::to_wstring(a.ftLastWriteTime.dwLowDateTime)+L":"+std::to_wstring(a.nFileSizeLow);
    return s+L"|v1|"+std::to_wstring(size);
}
inline uint64_t hash(const void* data,size_t n) {
    uint64_t h=14695981039346656037ull;auto b=(const unsigned char*)data;
    for(size_t i=0;i<n;++i){h^=b[i];h*=1099511628211ull;}return h;
}
inline std::filesystem::path path(const std::wstring& k) {
    return std::filesystem::path(tlauncher::profile())/L"icon-cache"/(std::to_wstring(hash(k.data(),k.size()*sizeof(wchar_t)))+L".bgra");
}
inline void remember(const std::wstring& k,std::shared_ptr<const Pixels> p) {
    std::lock_guard<std::mutex> lock(mutex);memory[k]={std::move(p),GetTickCount64()};
    size_t bytes=0;for(auto& e:memory)bytes+=e.second.pixels->bgra.size()*4;
    while(bytes>budget&&!memory.empty()) {auto old=std::min_element(memory.begin(),memory.end(),[](auto& a,auto& b){return a.second.used<b.second.used;});bytes-=old->second.pixels->bgra.size()*4;memory.erase(old);}
}
inline std::shared_ptr<const Pixels> peek(const std::wstring& source,int size) {
    const auto k=key(source,size);std::unique_lock<std::mutex> lock(mutex,std::try_to_lock);
    if(!lock.owns_lock())return {};
    auto i=memory.find(k);if(i==memory.end())return {};i->second.used=GetTickCount64();return i->second.pixels;
}
struct Header {uint32_t magic=0x31494354,w=0,h=0,reserved=0;uint64_t checksum=0;};
template<class Extract> bool load(const std::wstring& source,int size,int& w,int& h,std::vector<UINT>& pixels,Extract extract) {
    if(auto p=peek(source,size)){w=p->w;h=p->h;pixels=p->bgra;return true;}
    if(size<16||size>512)return false;
    const auto k=key(source,size);auto p=std::make_shared<Pixels>();
    Header head;bool valid=false;
    {std::ifstream in(path(k),std::ios::binary);if(in.read((char*)&head,sizeof(head))&&head.magic==0x31494354&&head.w>0&&head.h>0&&head.w<=512&&head.h<=512){p->w=head.w;p->h=head.h;p->bgra.resize(size_t(head.w)*head.h);valid=bool(in.read((char*)p->bgra.data(),p->bgra.size()*4))&&hash(p->bgra.data(),p->bgra.size()*4)==head.checksum;}}
    if(!valid) {
        p->bgra.clear();if(!extract(source,size,p->w,p->h,p->bgra)||p->w<=0||p->h<=0||p->w>512||p->h>512||p->bgra.size()!=size_t(p->w)*p->h)return false;
        std::error_code ec;const auto dest=path(k);std::filesystem::create_directories(dest.parent_path(),ec);
        // Bound persistent storage too; stale entries are expendable cache, not user assets.
        size_t total=0;std::vector<std::filesystem::directory_entry> files;
        for(std::filesystem::directory_iterator it(dest.parent_path(),ec),end;!ec&&it!=end;it.increment(ec))if(it->is_regular_file(ec)&&it->path().extension()==L".bgra"){total+=it->file_size(ec);files.push_back(*it);}
        if(total>64*1024*1024){std::sort(files.begin(),files.end(),[&](auto& a,auto& b){return a.last_write_time(ec)<b.last_write_time(ec);});for(auto& f:files){if(total<=48*1024*1024)break;auto n=f.file_size(ec);if(std::filesystem::remove(f.path(),ec))total-=n;}}
        auto temp=dest;temp+=L"."+std::to_wstring(GetCurrentThreadId())+L".tmp";
        head.w=p->w;head.h=p->h;head.checksum=hash(p->bgra.data(),p->bgra.size()*4);
        {std::ofstream out(temp,std::ios::binary);out.write((char*)&head,sizeof(head));out.write((char*)p->bgra.data(),p->bgra.size()*4);out.flush();if(!out){out.close();std::filesystem::remove(temp,ec);}}
        MoveFileExW(temp.c_str(),dest.c_str(),MOVEFILE_REPLACE_EXISTING|MOVEFILE_WRITE_THROUGH);
    }
    w=p->w;h=p->h;pixels=p->bgra;remember(k,p);return true;
}
} // namespace appicons
