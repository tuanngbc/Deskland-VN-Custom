// TLauncher recovery journal around FluidWin's bounded foreign-call/thread-pool
// pattern. Never changes write-only DWM transition attributes or system policy.
#include <windows.h>
#include <string>
#include <vector>
#include <memory>
#include <atomic>
#include <mutex>
#include <algorithm>
#include "upstream/altrui.h"
#include "upstream/pool.h"

namespace {
struct Handle { HANDLE value=nullptr; ~Handle(){if(value) CloseHandle(value);} };
uint64_t birth(HANDLE p){FILETIME a{},b{},c{},d{};return GetProcessTimes(p,&a,&b,&c,&d)?(uint64_t(a.dwHighDateTime)<<32)|a.dwLowDateTime:0;}
uintptr_t get(HWND h,const std::wstring& key,const wchar_t* suffix){return reinterpret_cast<uintptr_t>(GetPropW(h,(key+suffix).c_str()));}
bool put(HWND h,const std::wstring& key,const wchar_t* suffix,uintptr_t value){return SetPropW(h,(key+suffix).c_str(),reinterpret_cast<HANDLE>(value))!=FALSE;}
const wchar_t* fields[]={L".pid",L".tid",L".birthLo",L".birthHi",L".ownerLo",L".ownerHi",L".state"};
bool identity(HWND h,const std::wstring& key){
    DWORD pid=0;const DWORD tid=GetWindowThreadProcessId(h,&pid);
    if(!pid||get(h,key,L".pid")!=uintptr_t(pid)+1||get(h,key,L".tid")!=uintptr_t(tid)+1)return false;
    Handle p{OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,FALSE,pid)};
    const uint64_t created=p.value?birth(p.value):0;
    return created && get(h,key,L".birthLo")==uintptr_t(uint32_t(created))+1 && get(h,key,L".birthHi")==uintptr_t(uint32_t(created>>32))+1;
}
bool marked(HWND h,const std::wstring& key){return GetPropW(h,key.c_str())&&identity(h,key);}
void erase(HWND h,const std::wstring& key){
    RemovePropW(h,key.c_str());for(auto suffix:fields)RemovePropW(h,(key+suffix).c_str());
}
bool set_layered(HWND h,bool enabled){
    const LONG_PTR current=GetWindowLongPtrW(h,GWL_EXSTYLE);
    const LONG_PTR next=enabled?current|WS_EX_LAYERED:current&~LONG_PTR(WS_EX_LAYERED);
    SetLastError(0);const auto previous=SetWindowLongPtrW(h,GWL_EXSTYLE,next);
    return previous!=0||GetLastError()==0;
}
void reveal(HWND h,const std::wstring& key){
    if(!marked(h,key))return;
    put(h,key,L".state",4); // revoke the concealment before making it visible
    SetLayeredWindowAttributes(h,0,255,LWA_ALPHA); // no foreign message wait
}
bool restore(HWND h,const std::wstring& key){
    if(!marked(h,key))return true;
    reveal(h,key);
    if(!identity(h,key))return true;
    // Style changes can block inside another process. Only call in a pool
    // worker; keep the marker until the change really completes.
    if(!set_layered(h,false))return false;
    if(marked(h,key)&&!(GetWindowLongPtrW(h,GWL_EXSTYLE)&WS_EX_LAYERED)){
        erase(h,key);
        RedrawWindow(h,nullptr,nullptr,RDW_INVALIDATE|RDW_FRAME|RDW_ALLCHILDREN);
        return true;
    }
    return !IsWindow(h);
}
struct Lease {
    HWND hwnd=nullptr;std::wstring key;Handle guardian;
    std::atomic<int> state{0};std::atomic<bool> cancel{false},cleanup{false};
    std::mutex operation;
    bool alive(){return guardian.value&&WaitForSingleObject(guardian.value,0)==WAIT_TIMEOUT;}
};
using Ref=std::shared_ptr<Lease>;
DWORD WINAPI cleanup_worker(void* raw){
    std::unique_ptr<Ref> ref(static_cast<Ref*>(raw));auto& l=**ref;
    std::lock_guard<std::mutex> lock(l.operation);
    l.state.store(restore(l.hwnd,l.key)?2:-1);return 0;
}
void release(const Ref& ref){
    ref->cancel.store(true);reveal(ref->hwnd,ref->key);
    if(!ref->cleanup.exchange(true)){
        auto work=new(std::nothrow) Ref(ref);
        if(!work||!pool::lancia(cleanup_worker,work,THREAD_PRIORITY_NORMAL,true)){delete work;ref->state.store(-1);}
    }
}
DWORD WINAPI begin_worker(void* raw){
    std::unique_ptr<Ref> ref(static_cast<Ref*>(raw));auto& l=**ref;
    std::lock_guard<std::mutex> lock(l.operation);
    if(l.cancel.load()||!l.alive()||!marked(l.hwnd,l.key)||!risponde(l.hwnd,200)){l.state.store(-1);release(*ref);return 0;}
    if(!set_layered(l.hwnd,true)){l.state.store(-1);release(*ref);return 0;}
    if(l.cancel.load()||!l.alive()||!marked(l.hwnd,l.key)||get(l.hwnd,l.key,L".state")!=1){release(*ref);return 0;}
    const BOOL hidden=SetLayeredWindowAttributes(l.hwnd,0,0,LWA_ALPHA);
    if(!hidden||l.cancel.load()||!l.alive()||get(l.hwnd,l.key,L".state")!=1){release(*ref);return 0;}
    l.state.store(1);return 0;
}
struct Recovery {HWND hwnd;std::wstring key;};
struct RecoveryWork{Recovery item;std::shared_ptr<std::atomic<unsigned>> pending;};
DWORD WINAPI recover_worker(void* raw){
    std::unique_ptr<RecoveryWork> work(static_cast<RecoveryWork*>(raw));
    restore(work->item.hwnd,work->item.key);work->pending->fetch_sub(1);return 0;
}
struct Scan{std::wstring key;std::vector<Recovery> items;};
BOOL CALLBACK collect(HWND h,LPARAM raw){auto& scan=*reinterpret_cast<Scan*>(raw);if(marked(h,scan.key))scan.items.push_back({h,scan.key});return TRUE;}
bool orphan(HWND h,const std::wstring& key){
    if(!marked(h,key))return false;
    const auto encoded=reinterpret_cast<uintptr_t>(GetPropW(h,key.c_str()));
    if(encoded<=1||encoded>uint64_t(MAXDWORD)+1)return false;
    const DWORD pid=DWORD(encoded-1);
    Handle owner{OpenProcess(SYNCHRONIZE|PROCESS_QUERY_LIMITED_INFORMATION,FALSE,pid)};
    if(!owner.value)return GetLastError()==ERROR_INVALID_PARAMETER;
    if(WaitForSingleObject(owner.value,0)==WAIT_OBJECT_0)return true;
    const auto stamp=birth(owner.value);if(!stamp)return false;
    return get(h,key,L".ownerLo")!=uintptr_t(uint32_t(stamp))+1||get(h,key,L".ownerHi")!=uintptr_t(uint32_t(stamp>>32))+1;
}
int CALLBACK collect_orphan_property(HWND h,LPWSTR property,HANDLE,ULONG_PTR raw){
    if(reinterpret_cast<uintptr_t>(property)<=0xffff)return TRUE;
    const std::wstring key(property);static const std::wstring prefix=L"Local\\TuanpcDesk_Taskbar_",suffix=L"_Motion";
    if(key.size()<prefix.size()+suffix.size()||key.compare(0,prefix.size(),prefix)!=0||key.compare(key.size()-suffix.size(),suffix.size(),suffix)!=0)return TRUE;
    auto& items=*reinterpret_cast<std::vector<Recovery>*>(raw);
    if(items.size()<128&&orphan(h,key))items.push_back({h,key});return TRUE;
}
BOOL CALLBACK collect_orphan(HWND h,LPARAM raw){EnumPropsExW(h,collect_orphan_property,raw);return TRUE;}
unsigned recover_items(const std::vector<Recovery>& items){
    for(const auto& item:items)reveal(item.hwnd,item.key);
    auto pending=std::make_shared<std::atomic<unsigned>>(0);
    for(const auto& item:items){
        pending->fetch_add(1);auto work=new(std::nothrow) RecoveryWork{item,pending};
        if(!work||!pool::lancia(recover_worker,work,THREAD_PRIORITY_NORMAL,true)){delete work;pending->fetch_sub(1);}
    }
    const auto started=GetTickCount64();
    while(pending->load()&&GetTickCount64()-started<3000)Sleep(5);
    unsigned unresolved=0;for(const auto& item:items)if(marked(item.hwnd,item.key))unresolved++;return unresolved;
}
}

extern "C" {
void* tlauncher_window_lease(HWND hwnd,HANDLE guardian,const wchar_t* marker) try {
    if(!hwnd||!guardian||!marker||!IsWindow(hwnd)||WaitForSingleObject(guardian,0)!=WAIT_TIMEOUT)return nullptr;
    // Existing per-pixel/scalar layered windows stay on the native path until
    // their full alpha/color-key restoration has independent evidence.
    if(GetWindowLongPtrW(hwnd,GWL_EXSTYLE)&WS_EX_LAYERED)return nullptr;
    DWORD pid=0;const auto tid=GetWindowThreadProcessId(hwnd,&pid);
    Handle source{OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,FALSE,pid)};
    const auto created=source.value?birth(source.value):0;const auto owner=birth(GetCurrentProcess());
    if(!created||!owner||!tid)return nullptr;
    auto l=std::make_shared<Lease>();l->hwnd=hwnd;l->key=marker;
    if(GetPropW(hwnd,marker))return nullptr;
    if(!DuplicateHandle(GetCurrentProcess(),guardian,GetCurrentProcess(),&l->guardian.value,SYNCHRONIZE,FALSE,0))return nullptr;
    if(!put(hwnd,l->key,L".pid",uintptr_t(pid)+1)||!put(hwnd,l->key,L".tid",uintptr_t(tid)+1)||
       !put(hwnd,l->key,L".birthLo",uintptr_t(uint32_t(created))+1)||!put(hwnd,l->key,L".birthHi",uintptr_t(uint32_t(created>>32))+1)||
       !put(hwnd,l->key,L".ownerLo",uintptr_t(uint32_t(owner))+1)||!put(hwnd,l->key,L".ownerHi",uintptr_t(uint32_t(owner>>32))+1)||
       !put(hwnd,l->key,L".state",1)||!SetPropW(hwnd,marker,reinterpret_cast<HANDLE>(uintptr_t(GetCurrentProcessId())+1))){erase(hwnd,l->key);return nullptr;}
    auto handle=new(std::nothrow) Ref(l);auto work=new(std::nothrow) Ref(l);
    if(!handle||!work||!pool::lancia(begin_worker,work,THREAD_PRIORITY_NORMAL,true)){delete work;delete handle;erase(hwnd,l->key);return nullptr;}
    return handle;
} catch(...){return nullptr;}
int tlauncher_window_lease_poll(void* raw){
    if(!raw)return -1;const auto& ref=*static_cast<Ref*>(raw);
    const auto state=ref->state.load();
    if(state==1&&!ref->cancel.load()&&(!ref->alive()||!marked(ref->hwnd,ref->key)||get(ref->hwnd,ref->key,L".state")!=1)){
        release(ref);return -1;
    }
    return state;
}
void tlauncher_window_lease_restore(void* raw){if(raw)release(*static_cast<Ref*>(raw));}
void tlauncher_window_lease_free(void* raw){if(raw){release(*static_cast<Ref*>(raw));delete static_cast<Ref*>(raw);}}
unsigned tlauncher_window_restore_session(const wchar_t* marker){
    if(!marker)return 0;
    Scan scan{marker,{}};EnumWindows(collect,reinterpret_cast<LPARAM>(&scan));
    return recover_items(scan.items);
}
unsigned tlauncher_window_restore_orphans(){
    std::vector<Recovery> items;EnumWindows(collect_orphan,reinterpret_cast<LPARAM>(&items));
    return recover_items(items);
}
}
