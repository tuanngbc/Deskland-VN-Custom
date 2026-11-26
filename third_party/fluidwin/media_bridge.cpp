// Original FluidWin media reader, without its background polling thread.
#include <windows.h>
#include <cstdint>
#include "upstream/musica.h"
namespace{
struct Media{musica::Lettore reader;uint64_t generation=0;musica::Brano snapshot;};
void text(const std::wstring& value,wchar_t* out,unsigned capacity){if(out&&capacity)wcsncpy_s(out,capacity,value.c_str(),_TRUNCATE);}
}
extern "C"{
void* tlauncher_media_create() noexcept{try{return new Media;}catch(...){return nullptr;}}
void tlauncher_media_free(void* raw) noexcept{delete static_cast<Media*>(raw);}
uint64_t tlauncher_media_read(void* raw) noexcept{
    try{auto& m=*static_cast<Media*>(raw);m.reader.leggiUnaVolta();m.snapshot=m.reader.stato();return ++m.generation;}catch(...){return 0;}
}
unsigned tlauncher_media_snapshot(void* raw,wchar_t* title,wchar_t* artist,wchar_t* identity,unsigned capacity,uint32_t* art,unsigned pixels) noexcept{
    try{
        const auto& s=static_cast<Media*>(raw)->snapshot;text(s.titolo,title,capacity);text(s.artista,artist,capacity);text(s.aumid,identity,capacity);
        const bool cover=s.copertina.size()==musica::LATO*musica::LATO&&pixels>=s.copertina.size()&&art;
        if(cover)std::copy(s.copertina.begin(),s.copertina.end(),art);
        return (s.c?1:0)|(s.suona?2:0)|(cover?4:0);
    }catch(...){return 0;}
}
bool tlauncher_media_command(void* raw,uint64_t generation,const wchar_t* identity,int command) noexcept{
    try{auto& m=*static_cast<Media*>(raw);
        if(!identity||!generation||generation!=m.generation||!m.snapshot.c||m.snapshot.aumid!=identity)return false;
        return m.reader.comandoUnaVolta(command,m.snapshot.aumid);
    }catch(...){return false;}
}
}
