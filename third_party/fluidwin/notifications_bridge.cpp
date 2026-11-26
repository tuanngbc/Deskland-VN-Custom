// Original FluidWin notification enumeration, scoped to an on-demand job.
#include "upstream/notifiche.h"
#include <memory>
#include <cstdint>
namespace {
struct Reader { notifiche::Lettore reader; std::vector<notifiche::Notifica> rows; int status=0; };
bool same(const notifiche::Notifica& item,UINT32 id,INT64 time,const wchar_t* aumid) {
    return aumid && *aumid && time>0 && item.id==id && item.quando==time && item.aumid==aumid;
}
void copy(const std::wstring& value,wchar_t* out,unsigned size) {
    if(out&&size)wcsncpy_s(out,size,value.c_str(),_TRUNCATE);
}
}
extern "C" {
void* tlauncher_notifications_create() noexcept { try{return new Reader;}catch(...){return nullptr;} }
void tlauncher_notifications_free(void* raw) noexcept { delete static_cast<Reader*>(raw); }
int tlauncher_notifications_read(void* raw) noexcept {
    if(!raw)return 3;
    try {
        auto& model=*static_cast<Reader*>(raw);model.rows.clear();
        if(!model.reader.permesso())return model.status=0;
        bool complete=false;
        if(!model.reader.leggi(model.rows,&complete))return model.status=model.reader.permesso()?3:0;
        if(!model.reader.permesso()){model.rows.clear();return model.status=0;}
        if(model.rows.size()>64){model.rows.resize(64);complete=false;}
        return model.status=complete?1:2;
    }catch(...){static_cast<Reader*>(raw)->rows.clear();return 3;}
}
unsigned tlauncher_notifications_count(void* raw) noexcept {
    return raw?unsigned(static_cast<Reader*>(raw)->rows.size()):0;
}
bool tlauncher_notifications_item(void* raw,unsigned index,UINT32* id,INT64* time,
                                wchar_t* app,wchar_t* aumid,wchar_t* title,wchar_t* body,unsigned size) noexcept {
    if(!raw||!id||!time)return false;
    try {
        const auto& rows=static_cast<Reader*>(raw)->rows;if(index>=rows.size())return false;
        const auto& row=rows[index];*id=row.id;*time=row.quando;
        copy(row.app,app,size);copy(row.aumid,aumid,size);copy(row.titolo,title,size);copy(row.testo,body,size);return true;
    }catch(...){return false;}
}
bool tlauncher_notifications_remove(void* raw,UINT32 id,INT64 time,const wchar_t* aumid,bool readOnly) noexcept {
    // Revalidate the actual notification identity immediately before the user's
    // single explicit dismissal. Never use ClearNotifications or delete by ID alone.
    if(readOnly||!raw||!aumid||!*aumid||time<=0)return false;
    try {
        auto& model=*static_cast<Reader*>(raw);std::vector<notifiche::Notifica> current;
        if(!model.reader.leggi(current))return false;
        const auto found=std::find_if(current.begin(),current.end(),[&](const auto& item){return same(item,id,time,aumid);});
        return found!=current.end()&&model.reader.rimuovi(id);
    }catch(...){return false;}
}
bool tlauncher_notifications_identity_fixture() noexcept {
    notifiche::Notifica item;item.id=7;item.quando=123;item.aumid=L"Synthetic.App";
    return same(item,7,123,L"Synthetic.App")&&!same(item,7,124,L"Synthetic.App")&&
        !same(item,7,123,L"Reused.App")&&!same(item,8,123,L"Synthetic.App")&&
        !same(item,7,123,L"")&&!tlauncher_notifications_remove(nullptr,7,123,L"Synthetic.App",true);
}
}
