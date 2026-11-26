// TLauncher recovery journal around FluidWin's bounded foreign-call/thread-pool
// pattern. Never changes write-only DWM transition attributes or system policy.
#include <windows.h>
#include <string>
#include <vector>
#include <memory>
#include <atomic>
#include <mutex>
#include <algorithm>
#include <cwchar>
#include <unordered_map>
#include "upstream/altrui.h"
#include "upstream/pool.h"

namespace {
struct Handle { HANDLE value=nullptr; ~Handle(){if(value) CloseHandle(value);} };
uint64_t birth(HANDLE p){FILETIME a{},b{},c{},d{};return GetProcessTimes(p,&a,&b,&c,&d)?(uint64_t(a.dwHighDateTime)<<32)|a.dwLowDateTime:0;}
uintptr_t get(HWND h,const std::wstring& key,const wchar_t* suffix){return reinterpret_cast<uintptr_t>(GetPropW(h,(key+suffix).c_str()));}
bool put(HWND h,const std::wstring& key,const wchar_t* suffix,uintptr_t value){return SetPropW(h,(key+suffix).c_str(),reinterpret_cast<HANDLE>(value))!=FALSE;}
const wchar_t* fields[]={L".pid",L".tid",L".birthLo",L".birthHi",L".ownerLo",L".ownerHi",L".state",L".pending",L".kind",L".ticket"};
std::atomic<uintptr_t> nextTicket{1};
bool orphan(HWND h,const std::wstring& key);
std::shared_ptr<std::mutex> operation_for(HWND hwnd,bool recovery=false){
    struct Operations{std::mutex guard;std::unordered_map<HWND,std::weak_ptr<std::mutex>> rows[2];};
    static auto* operations=new Operations;
    std::lock_guard<std::mutex> lock(operations->guard);auto& rows=operations->rows[recovery?1:0];
    for(auto it=rows.begin();it!=rows.end();)if(it->second.expired())it=rows.erase(it);else ++it;
    auto& weak=rows[hwnd];auto found=weak.lock();if(!found){found=std::make_shared<std::mutex>();weak=found;}return found;
}
std::atomic<HANDLE> fixtureReached{nullptr},fixtureResume{nullptr};
bool identity(HWND h,const std::wstring& key){
    DWORD pid=0;const DWORD tid=GetWindowThreadProcessId(h,&pid);
    if(!pid||get(h,key,L".pid")!=uintptr_t(pid)+1||get(h,key,L".tid")!=uintptr_t(tid)+1)return false;
    Handle p{OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,FALSE,pid)};
    const uint64_t created=p.value?birth(p.value):0;
    return created && get(h,key,L".birthLo")==uintptr_t(uint32_t(created))+1 && get(h,key,L".birthHi")==uintptr_t(uint32_t(created>>32))+1;
}
bool marked(HWND h,const std::wstring& key,uintptr_t ticket=0){return GetPropW(h,key.c_str())&&(!ticket||get(h,key,L".ticket")==ticket)&&identity(h,key);}
void erase(HWND h,const std::wstring& key){
    // Keep admission reserved until all fields are gone. Publishing a new
    // record while an old erase is still removing fields would corrupt it.
    for(auto suffix:fields)RemovePropW(h,(key+suffix).c_str());RemovePropW(h,key.c_str());
}
bool set_layered(HWND h,bool enabled){
    const LONG_PTR current=GetWindowLongPtrW(h,GWL_EXSTYLE);
    const LONG_PTR next=enabled?current|WS_EX_LAYERED:current&~LONG_PTR(WS_EX_LAYERED);
    SetLastError(0);const auto previous=SetWindowLongPtrW(h,GWL_EXSTYLE,next);
    return previous!=0||GetLastError()==0;
}
void reveal(HWND h,const std::wstring& key,uintptr_t ticket=0){
    if(!marked(h,key,ticket))return;
    put(h,key,L".state",4); // revoke the concealment before making it visible
    if(get(h,key,L".kind")!=2)SetLayeredWindowAttributes(h,0,255,LWA_ALPHA); // no foreign message wait
}
bool restore(HWND h,const std::wstring& key,uintptr_t ticket=0){
    if(!marked(h,key,ticket))return true;
    reveal(h,key,ticket);
    if(!identity(h,key))return true;
    // Style changes can block inside another process. Only call in a pool
    // worker; keep the marker until the change really completes.
    const bool topmost=get(h,key,L".kind")==2;
    if(topmost){
        if(!SetWindowPos(h,HWND_NOTOPMOST,0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE|SWP_NOOWNERZORDER))return false;
    }else if(!set_layered(h,false))return false;
    // A live owner's already-entered foreign call can finish after this
    // recovery pass. Keep its identity/journal reserved until that worker
    // acknowledges completion; otherwise a late write can outlive the marker
    // or be confused with a new lease using the same session key.
    if(get(h,key,L".pending")&&!orphan(h,key))return false;
    if(marked(h,key,ticket)&&!(GetWindowLongPtrW(h,GWL_EXSTYLE)&(topmost?WS_EX_TOPMOST:WS_EX_LAYERED))){
        erase(h,key);
        RedrawWindow(h,nullptr,nullptr,RDW_INVALIDATE|RDW_FRAME|RDW_ALLCHILDREN);
        return true;
    }
    return !IsWindow(h);
}
struct Lease {
    HWND hwnd=nullptr;std::wstring key;Handle guardian;
    BYTE alpha=0;bool topmost=false;uintptr_t ticket=0;
    std::atomic<int> state{0};std::atomic<bool> cancel{false},cleanup{false};
    std::mutex operation;
    std::shared_ptr<std::mutex> sourceOperation;
    bool alive(){return guardian.value&&WaitForSingleObject(guardian.value,0)==WAIT_TIMEOUT;}
};
using Ref=std::shared_ptr<Lease>;
DWORD WINAPI cleanup_worker(void* raw){
    std::unique_ptr<Ref> ref(static_cast<Ref*>(raw));auto& l=**ref;
    std::lock_guard<std::mutex> lock(l.operation);
    std::lock_guard<std::mutex> sourceLock(*l.sourceOperation);
    l.state.store(restore(l.hwnd,l.key,l.ticket)?2:-1);return 0;
}
void release(const Ref& ref){
    ref->cancel.store(true);reveal(ref->hwnd,ref->key,ref->ticket);
    if(!ref->cleanup.exchange(true)){
        auto work=new(std::nothrow) Ref(ref);
        if(!work||!pool::lancia(cleanup_worker,work,THREAD_PRIORITY_NORMAL,true)){delete work;ref->state.store(-1);}
    }
}
DWORD WINAPI begin_worker(void* raw){
    std::unique_ptr<Ref> ref(static_cast<Ref*>(raw));auto& l=**ref;
    std::lock_guard<std::mutex> lock(l.operation);
    // Visibility and topmost both affect GWL_EXSTYLE. Serialize our own
    // enable/cleanup operations across kinds so a style snapshot cannot
    // overwrite a simultaneous SetWindowPos topmost change.
    std::lock_guard<std::mutex> sourceLock(*l.sourceOperation);
    struct Pending{Lease& lease;~Pending(){if(marked(lease.hwnd,lease.key,lease.ticket))RemovePropW(lease.hwnd,(lease.key+L".pending").c_str());}}pending{l};
    if(l.cancel.load()||!l.alive()||!marked(l.hwnd,l.key,l.ticket)||get(l.hwnd,l.key,L".state")!=1||!risponde(l.hwnd,200)){l.state.store(-1);release(*ref);return 0;}
    if(const auto resume=fixtureResume.load()){
        if(const auto reached=fixtureReached.load())SetEvent(reached);
        WaitForSingleObject(resume,10000);
    }
    if(l.cancel.load()||!l.alive()||!marked(l.hwnd,l.key,l.ticket)||get(l.hwnd,l.key,L".state")!=1){l.state.store(-1);release(*ref);return 0;}
    if(l.topmost){
        // Synchronous foreign call stays on this existing bounded pool worker.
        // No posted enable may survive journal removal/owner termination.
        const BOOL changed=SetWindowPos(l.hwnd,HWND_TOPMOST,0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE|SWP_NOOWNERZORDER);
        if(!changed||l.cancel.load()||!l.alive()||!marked(l.hwnd,l.key,l.ticket)||get(l.hwnd,l.key,L".state")!=1||!(GetWindowLongPtrW(l.hwnd,GWL_EXSTYLE)&WS_EX_TOPMOST)){release(*ref);return 0;}
        l.state.store(1);return 0;
    }
    if(!set_layered(l.hwnd,true)){l.state.store(-1);release(*ref);return 0;}
    if(l.cancel.load()||!l.alive()||!marked(l.hwnd,l.key,l.ticket)||get(l.hwnd,l.key,L".state")!=1){release(*ref);return 0;}
    const BOOL hidden=SetLayeredWindowAttributes(l.hwnd,0,l.alpha,LWA_ALPHA);
    if(!hidden||l.cancel.load()||!l.alive()||get(l.hwnd,l.key,L".state")!=1){release(*ref);return 0;}
    l.state.store(1);return 0;
}
struct Recovery {HWND hwnd;std::wstring key;uintptr_t ticket;};
struct RecoveryWork{Recovery item;std::shared_ptr<std::atomic<unsigned>> pending;};
DWORD WINAPI recover_worker(void* raw){
    std::unique_ptr<RecoveryWork> work(static_cast<RecoveryWork*>(raw));
    // Recovery must not wait on the owner's possibly hung begin lock. The
    // Guardian has a separate per-HWND lock for its two restore operations.
    const auto operation=operation_for(work->item.hwnd,true);std::lock_guard<std::mutex> sourceLock(*operation);
    restore(work->item.hwnd,work->item.key,work->item.ticket);work->pending->fetch_sub(1);return 0;
}
struct Scan{std::wstring key;std::vector<Recovery> items;};
BOOL CALLBACK collect(HWND h,LPARAM raw){auto& scan=*reinterpret_cast<Scan*>(raw);if(marked(h,scan.key))scan.items.push_back({h,scan.key,get(h,scan.key,L".ticket")});return TRUE;}
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
    const std::wstring key(property);static const std::wstring prefix=L"Local\\TuanpcDesk_Taskbar_",suffix=L"_Motion",topSuffix=L"_Motion_Topmost";
    const auto ends=[&](const std::wstring& s){return key.size()>=prefix.size()+s.size()&&key.compare(key.size()-s.size(),s.size(),s)==0;};
    if(key.compare(0,prefix.size(),prefix)!=0||(!ends(suffix)&&!ends(topSuffix)))return TRUE;
    auto& items=*reinterpret_cast<std::vector<Recovery>*>(raw);
    if(items.size()<128&&orphan(h,key))items.push_back({h,key,get(h,key,L".ticket")});return TRUE;
}
BOOL CALLBACK collect_orphan(HWND h,LPARAM raw){EnumPropsExW(h,collect_orphan_property,raw);return TRUE;}
unsigned recover_items(const std::vector<Recovery>& items){
    for(const auto& item:items)reveal(item.hwnd,item.key,item.ticket);
    auto pending=std::make_shared<std::atomic<unsigned>>(0);
    for(const auto& item:items){
        pending->fetch_add(1);auto work=new(std::nothrow) RecoveryWork{item,pending};
        if(!work||!pool::lancia(recover_worker,work,THREAD_PRIORITY_NORMAL,true)){delete work;pending->fetch_sub(1);}
    }
    const auto started=GetTickCount64();
    while(pending->load()&&GetTickCount64()-started<3000)Sleep(5);
    unsigned unresolved=0;for(const auto& item:items)if(marked(item.hwnd,item.key,item.ticket))unresolved++;return unresolved;
}
}

extern "C" {
static void* create_lease(HWND hwnd,HANDLE guardian,const wchar_t* marker,BYTE alpha,bool topmost) try {
    if(alpha>1)return nullptr;
    if(!hwnd||!guardian||!marker||!IsWindow(hwnd)||WaitForSingleObject(guardian,0)!=WAIT_TIMEOUT)return nullptr;
    // Existing per-pixel/scalar layered windows stay on the native path until
    // their full alpha/color-key restoration has independent evidence.
    if(GetWindowLongPtrW(hwnd,GWL_EXSTYLE)&(topmost?WS_EX_TOPMOST:WS_EX_LAYERED))return nullptr;
    DWORD pid=0;const auto tid=GetWindowThreadProcessId(hwnd,&pid);
    Handle source{OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,FALSE,pid)};
    const auto created=source.value?birth(source.value):0;const auto owner=birth(GetCurrentProcess());
    if(!created||!owner||!tid)return nullptr;
    auto l=std::make_shared<Lease>();l->hwnd=hwnd;l->key=marker;l->alpha=alpha;l->topmost=topmost;l->sourceOperation=operation_for(hwnd);
    l->ticket=nextTicket.fetch_add(1);if(!l->ticket)return nullptr;
    if(GetPropW(hwnd,marker))return nullptr;
    if(!DuplicateHandle(GetCurrentProcess(),guardian,GetCurrentProcess(),&l->guardian.value,SYNCHRONIZE,FALSE,0))return nullptr;
    if(!put(hwnd,l->key,L".pid",uintptr_t(pid)+1)||!put(hwnd,l->key,L".tid",uintptr_t(tid)+1)||
       !put(hwnd,l->key,L".birthLo",uintptr_t(uint32_t(created))+1)||!put(hwnd,l->key,L".birthHi",uintptr_t(uint32_t(created>>32))+1)||
       !put(hwnd,l->key,L".ownerLo",uintptr_t(uint32_t(owner))+1)||!put(hwnd,l->key,L".ownerHi",uintptr_t(uint32_t(owner>>32))+1)||
       !put(hwnd,l->key,L".state",1)||!put(hwnd,l->key,L".pending",1)||!put(hwnd,l->key,L".kind",topmost?2:1)||!put(hwnd,l->key,L".ticket",l->ticket)||!SetPropW(hwnd,marker,reinterpret_cast<HANDLE>(uintptr_t(GetCurrentProcessId())+1))){erase(hwnd,l->key);return nullptr;}
    auto handle=new(std::nothrow) Ref(l);auto work=new(std::nothrow) Ref(l);
    if(!handle||!work||!pool::lancia(begin_worker,work,THREAD_PRIORITY_NORMAL,true)){delete work;delete handle;erase(hwnd,l->key);return nullptr;}
    return handle;
} catch(...){return nullptr;}
void* tlauncher_window_lease_topmost(HWND hwnd,HANDLE guardian,const wchar_t* marker){return create_lease(hwnd,guardian,marker,0,true);}
void* tlauncher_window_lease_alpha(HWND hwnd,HANDLE guardian,const wchar_t* marker,BYTE alpha){return create_lease(hwnd,guardian,marker,alpha,false);}
void* tlauncher_window_lease(HWND hwnd,HANDLE guardian,const wchar_t* marker){return tlauncher_window_lease_alpha(hwnd,guardian,marker,0);}
int tlauncher_window_lease_poll(void* raw){
    if(!raw)return -1;const auto& ref=*static_cast<Ref*>(raw);
    const auto state=ref->state.load();
    if(state==1&&!ref->cancel.load()&&(!ref->alive()||!marked(ref->hwnd,ref->key,ref->ticket)||get(ref->hwnd,ref->key,L".state")!=1)){
        release(ref);return -1;
    }
    return state;
}
void tlauncher_window_lease_restore(void* raw){if(raw)release(*static_cast<Ref*>(raw));}
void tlauncher_window_lease_free(void* raw){if(raw){release(*static_cast<Ref*>(raw));delete static_cast<Ref*>(raw);}}
unsigned tlauncher_window_restore_session(const wchar_t* marker){
    if(!marker)return 0;
    Scan scan{marker,{}};EnumWindows(collect,reinterpret_cast<LPARAM>(&scan));
    Scan top{std::wstring(marker)+L"_Topmost",{}};EnumWindows(collect,reinterpret_cast<LPARAM>(&top));
    scan.items.insert(scan.items.end(),top.items.begin(),top.items.end());
    return recover_items(scan.items);
}
unsigned tlauncher_window_restore_orphans(){
    std::vector<Recovery> items;EnumWindows(collect_orphan,reinterpret_cast<LPARAM>(&items));
    return recover_items(items);
}
void tlauncher_window_lease_fixture_pause(HANDLE reached,HANDLE resume){
    wchar_t hosted[8]{};if(GetEnvironmentVariableW(L"GITHUB_ACTIONS",hosted,8)!=4||wcscmp(hosted,L"true"))return;
    fixtureReached.store(reached);fixtureResume.store(resume);
}
}
