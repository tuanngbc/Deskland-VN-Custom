// FluidWin dock.h Recycle Bin interaction adapted to the existing Dock.
// Delegate the actual move to Windows' Recycle Bin IDropTarget, preserving
// virtual items, confirmation and optimized-move completion semantics.
#include <windows.h>
#include <shlobj.h>
#include <wrl/client.h>
#include <new>
#include <atomic>
namespace {
using Microsoft::WRL::ComPtr;
using Hit=bool(*)(HWND,LONG,LONG);
struct Target final:IDropTarget{
    ULONG refs=1;HWND owner;Hit hit;ComPtr<IDataObject> data;ComPtr<IDropTarget> shell;
    Target(HWND h,Hit callback):owner(h),hit(callback){}
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id,void** out) override{
        if(!out)return E_POINTER;*out=nullptr;
        if(id!=IID_IUnknown&&id!=IID_IDropTarget)return E_NOINTERFACE;
        *out=static_cast<IDropTarget*>(this);AddRef();return S_OK;
    }
    ULONG STDMETHODCALLTYPE AddRef() override{return ++refs;}
    ULONG STDMETHODCALLTYPE Release() override{const auto n=--refs;if(!n)delete this;return n;}
    void leave(){if(shell)shell->DragLeave();shell.Reset();}
    HRESULT bind(){
        PIDLIST_ABSOLUTE id=nullptr;HRESULT hr=SHGetKnownFolderIDList(FOLDERID_RecycleBinFolder,0,nullptr,&id);
        if(FAILED(hr))return hr;ComPtr<IShellFolder> parent;PCUITEMID_CHILD child=nullptr;
        hr=SHBindToParent(id,IID_PPV_ARGS(&parent),&child);
        if(SUCCEEDED(hr))hr=parent->GetUIObjectOf(owner,1,&child,IID_IDropTarget,nullptr,reinterpret_cast<void**>(shell.GetAddressOf()));
        CoTaskMemFree(id);return hr;
    }
    HRESULT STDMETHODCALLTYPE DragEnter(IDataObject* object,DWORD keys,POINTL point,DWORD* effect) override{
        leave();data=object;return DragOver(keys,point,effect);
    }
    HRESULT STDMETHODCALLTYPE DragOver(DWORD keys,POINTL point,DWORD* effect) override{
        if(!effect)return E_POINTER;
        if(!data||!IsWindow(owner)||!hit||!hit(owner,point.x,point.y)||!(*effect&DROPEFFECT_MOVE)){
            leave();*effect=DROPEFFECT_NONE;return S_OK;
        }
        HRESULT hr=S_OK;
        if(!shell){hr=bind();if(SUCCEEDED(hr))hr=shell->DragEnter(data.Get(),keys,point,effect);}
        else hr=shell->DragOver(keys,point,effect);
        if(FAILED(hr)){leave();*effect=DROPEFFECT_NONE;}
        else *effect&=DROPEFFECT_MOVE;
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE DragLeave() override{leave();data.Reset();return S_OK;}
    HRESULT STDMETHODCALLTYPE Drop(IDataObject* object,DWORD keys,POINTL point,DWORD* effect) override{
        if(!effect)return E_POINTER;
        DragOver(keys,point,effect);
        HRESULT hr=S_OK;
        if(shell&&(*effect&DROPEFFECT_MOVE))hr=shell->Drop(object,keys,point,effect);
        else *effect=DROPEFFECT_NONE;
        shell.Reset();data.Reset();return hr;
    }
};
struct Registration{
    HWND owner=nullptr;Target* target=nullptr;bool apartment=false,registered=false;
    ~Registration(){
        if(registered)RevokeDragDrop(owner);
        if(target){target->DragLeave();target->Release();}
        if(apartment)OleUninitialize();
    }
};
}
extern "C"{
void* tlauncher_recycle_register(HWND owner,Hit hit) noexcept{
    auto r=new(std::nothrow) Registration;if(!r)return nullptr;
    r->owner=owner;r->apartment=SUCCEEDED(OleInitialize(nullptr));
    if(r->apartment)r->target=new(std::nothrow) Target(owner,hit);
    if(!r->target||FAILED(RegisterDragDrop(owner,r->target))){delete r;return nullptr;}
    r->registered=true;return r;
}
void tlauncher_recycle_unregister(void* raw) noexcept{delete static_cast<Registration*>(raw);}
int tlauncher_recycle_registration_fixture(HWND owner) noexcept{
    const auto hit=[](HWND,LONG,LONG)->bool{return false;};
    auto r=static_cast<Registration*>(tlauncher_recycle_register(owner,hit));if(!r)return 1;
    int result=0;DWORD effect=DROPEFFECT_MOVE;POINTL point{};
    if(FAILED(r->target->DragEnter(nullptr,0,point,&effect))||effect!=DROPEFFECT_NONE)result|=2;
    effect=DROPEFFECT_MOVE;if(FAILED(r->target->Drop(nullptr,0,point,&effect))||effect!=DROPEFFECT_NONE)result|=4;
    void* identity=nullptr;
    if(FAILED(r->target->QueryInterface(IID_IUnknown,&identity))||!identity)result|=8;
    if(identity)static_cast<IUnknown*>(identity)->Release();
    delete r;return result;
}
}
