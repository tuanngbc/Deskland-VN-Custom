// FluidWin notch.h::scaffale, adapted to the existing native Control Center.
// References only. Shell owns outgoing data objects and file operations.
#include <windows.h>
#include <shellapi.h>
#include <shlobj.h>
#include <shobjidl.h>
#include <shlwapi.h>
#include <wrl/client.h>
#include <memory>
#include <vector>
#include <string>
#include <algorithm>
#include <atomic>
#include <new>
namespace {
using Microsoft::WRL::ComPtr;
constexpr size_t LIMIT=200;
struct Item{unsigned long long id;std::wstring path;};
struct Model{std::vector<Item> items;unsigned long long next=1;bool enabled=true,outgoing=false;};
using Handle=std::shared_ptr<Model>;
bool valid(const std::wstring& path){return !path.empty()&&path.size()<32768&&!PathIsRelativeW(path.c_str())&&
    (PathIsUNCW(path.c_str())||(path.size()>2&&path[1]==L':'&&(path[2]==L'\\'||path[2]==L'/')))&&path.find(L'\0')==std::wstring::npos;}
bool append(Model& model,const std::vector<std::wstring>& paths){
    if(!model.enabled||model.outgoing||paths.empty()||paths.size()>LIMIT)return false;
    std::vector<std::wstring> additions;
    for(const auto& path:paths){
        if(!valid(path))return false;
        if(std::any_of(model.items.begin(),model.items.end(),[&](const Item& i){return !_wcsicmp(i.path.c_str(),path.c_str());})||
           std::any_of(additions.begin(),additions.end(),[&](const std::wstring& p){return !_wcsicmp(p.c_str(),path.c_str());}))continue;
        additions.push_back(path);
    }
    if(model.items.size()+additions.size()>LIMIT)return false; // all or none
    auto staged=model.items;auto next=model.next;staged.reserve(staged.size()+additions.size());
    for(auto& path:additions)staged.push_back(Item{next++,std::move(path)});
    model.items.swap(staged);model.next=next;
    return true;
}
bool read_paths(IDataObject* object,std::vector<std::wstring>& paths){
    if(!object)return false;FORMATETC format{CF_HDROP,nullptr,DVASPECT_CONTENT,-1,TYMED_HGLOBAL};STGMEDIUM medium{};
    if(FAILED(object->GetData(&format,&medium)))return false;
    struct Release{STGMEDIUM* value;~Release(){ReleaseStgMedium(value);}}release{&medium};
    if(medium.tymed!=TYMED_HGLOBAL||!medium.hGlobal)return false;
    const auto drop=reinterpret_cast<HDROP>(medium.hGlobal);const auto count=DragQueryFileW(drop,0xFFFFFFFF,nullptr,0);
    if(!count||count>LIMIT)return false;
    for(UINT i=0;i<count;i++){
        const auto length=DragQueryFileW(drop,i,nullptr,0);if(!length||length>=32768)return false;
        std::wstring path(length+1,L'\0');if(DragQueryFileW(drop,i,path.data(),length+1)!=length)return false;
        path.resize(length);if(!valid(path))return false;paths.push_back(std::move(path));
    }return true;
}
HRESULT data_object(const std::vector<std::wstring>& paths,IDataObject** out){
    if(!out)return E_POINTER;*out=nullptr;if(paths.empty()||paths.size()>LIMIT)return E_INVALIDARG;
    struct Ids{std::vector<PIDLIST_ABSOLUTE> values;~Ids(){for(auto id:values)CoTaskMemFree(id);}}ids;
    for(const auto& path:paths){PIDLIST_ABSOLUTE id=nullptr;auto hr=SHParseDisplayName(path.c_str(),nullptr,&id,0,nullptr);if(FAILED(hr)||!id)return FAILED(hr)?hr:E_FAIL;ids.values.push_back(id);}
    std::vector<PCIDLIST_ABSOLUTE> view;for(auto id:ids.values)view.push_back(id);
    ComPtr<IShellItemArray> array;auto hr=SHCreateShellItemArrayFromIDLists(UINT(view.size()),view.data(),&array);
    return FAILED(hr)?hr:array->BindToHandler(nullptr,BHID_DataObject,IID_PPV_ARGS(out));
}
void finish(Model& model,const std::vector<unsigned long long>& ids,HRESULT result,DWORD effect){
    if(result!=DRAGDROP_S_DROP||!(effect&(DROPEFFECT_COPY|DROPEFFECT_MOVE|DROPEFFECT_LINK)))return;
    model.items.erase(std::remove_if(model.items.begin(),model.items.end(),[&](const Item& item){return std::find(ids.begin(),ids.end(),item.id)!=ids.end();}),model.items.end());
}
struct Target final:IDropTarget{
    std::atomic<ULONG> refs{1};Handle model;HWND window;UINT notice;bool accepted=false;
    Target(Handle m,HWND h,UINT message):model(std::move(m)),window(h),notice(message){}
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id,void** out)override{if(!out)return E_POINTER;*out=nullptr;if(id!=IID_IUnknown&&id!=IID_IDropTarget)return E_NOINTERFACE;*out=static_cast<IDropTarget*>(this);AddRef();return S_OK;}
    ULONG STDMETHODCALLTYPE AddRef()override{return ++refs;}
    ULONG STDMETHODCALLTYPE Release()override{auto n=--refs;if(!n)delete this;return n;}
    HRESULT STDMETHODCALLTYPE DragEnter(IDataObject* object,DWORD,POINTL point,DWORD* effect)override{
        FORMATETC format{CF_HDROP,nullptr,DVASPECT_CONTENT,-1,TYMED_HGLOBAL};accepted=object&&object->QueryGetData(&format)==S_OK;return DragOver(0,point,effect);
    }
    HRESULT STDMETHODCALLTYPE DragOver(DWORD,POINTL point,DWORD* effect)override{
        if(!effect)return E_POINTER;RECT area{};
        const bool inside=IsWindowVisible(window)&&GetWindowRect(window,&area)&&PtInRect(&area,POINT{point.x,point.y});
        *effect=accepted&&model->enabled&&!model->outgoing&&inside&&(*effect&DROPEFFECT_COPY)?DROPEFFECT_COPY:DROPEFFECT_NONE;return S_OK;
    }
    HRESULT STDMETHODCALLTYPE DragLeave()override{accepted=false;return S_OK;}
    HRESULT STDMETHODCALLTYPE Drop(IDataObject* object,DWORD,POINTL point,DWORD* effect)override{
        if(!effect)return E_POINTER;
        try{
            DragOver(0,point,effect);const bool take=*effect==DROPEFFECT_COPY;*effect=DROPEFFECT_NONE;accepted=false;
            std::vector<std::wstring> paths;if(take&&read_paths(object,paths)&&append(*model,paths)){*effect=DROPEFFECT_COPY;PostMessageW(window,notice,0,0);}
            return S_OK;
        }catch(...){*effect=DROPEFFECT_NONE;return E_OUTOFMEMORY;}
    }
};
struct Registration{HWND window=nullptr;Target* target=nullptr;bool apartment=false,registered=false;
    ~Registration(){if(registered)RevokeDragDrop(window);if(target){target->DragLeave();target->Release();}if(apartment)OleUninitialize();}
};
struct Source final:IDropSource{
    ULONG refs=1;Handle model;
    explicit Source(Handle m):model(std::move(m)){}
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id,void** out)override{if(!out)return E_POINTER;*out=nullptr;if(id!=IID_IUnknown&&id!=IID_IDropSource)return E_NOINTERFACE;*out=static_cast<IDropSource*>(this);AddRef();return S_OK;}
    ULONG STDMETHODCALLTYPE AddRef()override{return ++refs;}ULONG STDMETHODCALLTYPE Release()override{return --refs;}
    HRESULT STDMETHODCALLTYPE QueryContinueDrag(BOOL escape,DWORD keys)override{return escape||!model->enabled?DRAGDROP_S_CANCEL:keys&MK_LBUTTON?S_OK:DRAGDROP_S_DROP;}
    HRESULT STDMETHODCALLTYPE GiveFeedback(DWORD)override{return DRAGDROP_S_USEDEFAULTCURSORS;}
};
}
extern "C" {
void* tlauncher_shelf_create()noexcept{try{return new Handle(std::make_shared<Model>());}catch(...){return nullptr;}}
void tlauncher_shelf_free(void* raw)noexcept{if(!raw)return;auto h=static_cast<Handle*>(raw);(*h)->enabled=false;(*h)->items.clear();delete h;}
unsigned tlauncher_shelf_count(void* raw)noexcept{return raw?unsigned((*static_cast<Handle*>(raw))->items.size()):0;}
unsigned long long tlauncher_shelf_item(void* raw,unsigned index,wchar_t* path,unsigned capacity)noexcept{
    if(!raw||!path)return 0;const auto& model=**static_cast<Handle*>(raw);if(index>=model.items.size())return 0;const auto& item=model.items[index];
    if(item.path.size()+1>capacity)return 0;std::copy(item.path.begin(),item.path.end(),path);path[item.path.size()]=0;return item.id;
}
void tlauncher_shelf_remove(void* raw,unsigned long long id)noexcept{
    if(!raw)return;auto& model=**static_cast<Handle*>(raw);if(model.outgoing)return;
    model.items.erase(std::remove_if(model.items.begin(),model.items.end(),[&](const Item& item){return !id||item.id==id;}),model.items.end());
}
void* tlauncher_shelf_register(void* raw,HWND window,UINT notice)noexcept{
    try{
        if(!raw||!IsWindow(window))return nullptr;auto r=std::make_unique<Registration>();r->window=window;r->apartment=SUCCEEDED(OleInitialize(nullptr));
        if(!r->apartment)return nullptr;r->target=new(std::nothrow)Target(*static_cast<Handle*>(raw),window,notice);
        if(!r->target||FAILED(RegisterDragDrop(window,r->target)))return nullptr;r->registered=true;return r.release();
    }catch(...){return nullptr;}
}
void tlauncher_shelf_unregister(void* raw)noexcept{delete static_cast<Registration*>(raw);}
int tlauncher_shelf_add_dialog(void* raw,HWND owner,const wchar_t* title)noexcept{
    try{
        if(!raw)return -1;auto model=*static_cast<Handle*>(raw);if(!model->enabled||model->outgoing)return -1;
        ComPtr<IFileOpenDialog> dialog;auto hr=CoCreateInstance(CLSID_FileOpenDialog,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&dialog));if(FAILED(hr))return -2;
        DWORD options=0;if(FAILED(dialog->GetOptions(&options))||FAILED(dialog->SetOptions(options|FOS_ALLOWMULTISELECT|FOS_FILEMUSTEXIST|FOS_PATHMUSTEXIST|FOS_FORCEFILESYSTEM|FOS_NOCHANGEDIR)))return -3;
        if(title)dialog->SetTitle(title);hr=dialog->Show(owner);if(hr==HRESULT_FROM_WIN32(ERROR_CANCELLED))return 0;if(FAILED(hr))return -4;
        ComPtr<IShellItemArray> items;if(FAILED(dialog->GetResults(&items)))return -5;DWORD count=0;if(FAILED(items->GetCount(&count))||!count||count>LIMIT)return -6;
        std::vector<std::wstring> paths;for(DWORD i=0;i<count;i++){ComPtr<IShellItem> item;if(FAILED(items->GetItemAt(i,&item)))return -7;
            PWSTR path=nullptr;if(FAILED(item->GetDisplayName(SIGDN_FILESYSPATH,&path))||!path)return -8;
            std::unique_ptr<wchar_t,decltype(&CoTaskMemFree)> release(path,&CoTaskMemFree);paths.emplace_back(path);
        }return append(*model,paths)?1:-9;
    }catch(...){return -10;}
}
int tlauncher_shelf_drag(void* raw,unsigned long long id,bool readOnly)noexcept{
    try{
        if(!raw||readOnly)return -1;auto model=*static_cast<Handle*>(raw);if(!model->enabled||model->outgoing)return -2;
        std::vector<std::wstring> paths;std::vector<unsigned long long> ids;
        for(const auto& item:model->items)if(!id||item.id==id){paths.push_back(item.path);ids.push_back(item.id);}
        ComPtr<IDataObject> object;if(FAILED(data_object(paths,&object)))return -3;
        model->outgoing=true;struct Reset{Handle model;~Reset(){model->outgoing=false;}}reset{model};
        Source source(model);DWORD effect=DROPEFFECT_NONE;const auto result=DoDragDrop(object.Get(),&source,DROPEFFECT_COPY|DROPEFFECT_MOVE|DROPEFFECT_LINK,&effect);
        finish(*model,ids,result,effect);return result==DRAGDROP_S_DROP&&effect!=DROPEFFECT_NONE?1:0;
    }catch(...){return -4;}
}
int tlauncher_shelf_copy(void* raw,unsigned long long id,bool readOnly)noexcept{
    try{
        if(!raw||readOnly)return -1;auto model=*static_cast<Handle*>(raw);if(!model->enabled||model->outgoing)return -2;
        std::vector<std::wstring> paths;for(const auto& item:model->items)if(!id||item.id==id)paths.push_back(item.path);
        ComPtr<IDataObject> object;if(FAILED(data_object(paths,&object)))return -3;
        if(FAILED(OleSetClipboard(object.Get())))return -4;
        // Persist the Shell formats before the temporary panel/apartment closes.
        return SUCCEEDED(OleFlushClipboard())?1:-5;
    }catch(...){return -6;}
}
// Hosted only: real Shell IDataObject + IDropTarget methods; no DoDragDrop,
// file opening, moving, copying, deleting, or touching an owner's files.
int tlauncher_shelf_fixture(void* raw,void* registration,const wchar_t* path)noexcept{
    try{
        if(!raw||!registration||!path)return 1;auto& model=**static_cast<Handle*>(raw);auto& r=*static_cast<Registration*>(registration);
        ComPtr<IDataObject> object;if(FAILED(data_object({path},&object)))return 2;
        RECT rect{};GetWindowRect(r.window,&rect);POINTL point{rect.left+10,rect.top+10};DWORD effect=DROPEFFECT_COPY|DROPEFFECT_MOVE;
        if(FAILED(r.target->DragEnter(object.Get(),0,point,&effect))||effect!=DROPEFFECT_COPY)return 3;
        effect=DROPEFFECT_COPY;if(FAILED(r.target->Drop(object.Get(),0,point,&effect))||effect!=DROPEFFECT_COPY||model.items.size()!=1)return 4;
        if(!append(model,{path,path})||model.items.size()!=1)return 5;
        auto original=model.items;const auto id=original[0].id;
        finish(model,{id},DRAGDROP_S_CANCEL,DROPEFFECT_COPY);if(model.items.size()!=1)return 6;
        finish(model,{id},DRAGDROP_S_DROP,DROPEFFECT_NONE);if(model.items.size()!=1)return 7;
        finish(model,{id},DRAGDROP_S_DROP,DROPEFFECT_COPY);if(!model.items.empty())return 8;model.items=original;
        if(append(model,{L"relative.txt"})||model.items.size()!=1)return 9;
        std::vector<std::wstring> over;for(size_t i=0;i<LIMIT;i++)over.push_back(std::wstring(path)+std::to_wstring(i));
        if(append(model,over)||model.items.size()!=1)return 10;
        effect=DROPEFFECT_MOVE;r.target->DragEnter(object.Get(),0,point,&effect);if(effect!=DROPEFFECT_NONE)return 11;
        model.outgoing=true;effect=DROPEFFECT_COPY;r.target->DragEnter(object.Get(),0,point,&effect);model.outgoing=false;if(effect!=DROPEFFECT_NONE)return 12;
        model.enabled=false;effect=DROPEFFECT_COPY;r.target->DragEnter(object.Get(),0,point,&effect);model.enabled=true;if(effect!=DROPEFFECT_NONE)return 13;
        r.target->DragLeave();return 0;
    }catch(...){return 14;}
}
int tlauncher_shelf_disconnected_fixture(void* registration,const wchar_t* path)noexcept{
    try{
        if(!registration||!path)return 1;auto& r=*static_cast<Registration*>(registration);
        if(r.target->model->enabled||!r.target->model->items.empty())return 2;
        ComPtr<IDataObject> object;if(FAILED(data_object({path},&object)))return 3;
        RECT rect{};GetWindowRect(r.window,&rect);POINTL point{rect.left+10,rect.top+10};DWORD effect=DROPEFFECT_COPY;
        if(FAILED(r.target->DragEnter(object.Get(),0,point,&effect))||effect!=DROPEFFECT_NONE)return 4;
        effect=DROPEFFECT_COPY;if(FAILED(r.target->Drop(object.Get(),0,point,&effect))||effect!=DROPEFFECT_NONE)return 5;
        return r.target->model->items.empty()?0:6;
    }catch(...){return 7;}
}
}
