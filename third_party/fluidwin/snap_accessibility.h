// Native MSAA surface for the original FluidWin layout picker. Only state
// snapshots cross COM boundaries; actions are validated on the owning UI thread.
#pragma once
#include <windows.h>
#include <oleacc.h>
#include <oleauto.h>
#include <memory>
#include <mutex>
#include <vector>
#include <string>
#include <atomic>
#include <new>
#pragma comment(lib,"oleacc.lib")
#pragma comment(lib,"oleaut32.lib")
namespace tlauncher_snap_access {
constexpr UINT SELECT=WM_APP+145,CHOOSE=WM_APP+146;
struct Snapshot{HWND window=nullptr;std::wstring title,action;std::vector<std::wstring> labels;std::vector<RECT> regions;int selected=-1;
    long role=ROLE_SYSTEM_PANE,childRole=ROLE_SYSTEM_PUSHBUTTON;std::vector<long> roles,states;std::vector<std::wstring> actions;bool menuFocus=false;};
struct Model{std::mutex mutex;Snapshot state;Snapshot read(){std::lock_guard<std::mutex> guard(mutex);return state;}};
class Accessible final:public IAccessible{
    std::atomic<ULONG> refs{1};std::shared_ptr<Model> model;
    static int child(VARIANT value,const Snapshot& s){return value.vt==VT_I4&&value.lVal>=0&&value.lVal<=(LONG)s.labels.size()?int(value.lVal):-1;}
    static HRESULT string(const std::wstring& text,BSTR* out){if(!out)return E_POINTER;*out=SysAllocStringLen(text.data(),UINT(text.size()));return *out?S_OK:E_OUTOFMEMORY;}
    HRESULT type(ITypeInfo** out){if(!out)return E_POINTER;*out=nullptr;ITypeLib* library=nullptr;
        HRESULT hr=LoadRegTypeLib(LIBID_Accessibility,1,1,LOCALE_SYSTEM_DEFAULT,&library);if(FAILED(hr))return hr;
        hr=library->GetTypeInfoOfGuid(IID_IAccessible,out);library->Release();return hr;}
    HRESULT post(VARIANT value,UINT message){auto s=model->read();int index=child(value,s);if(index<=0)return E_INVALIDARG;
        if(!s.window||!IsWindowVisible(s.window))return CO_E_OBJNOTCONNECTED;
        if(size_t(index)<=s.states.size()&&(s.states[index-1]&(STATE_SYSTEM_UNAVAILABLE|STATE_SYSTEM_INVISIBLE)))return E_ACCESSDENIED;
        return PostMessageW(s.window,message,index-1,reinterpret_cast<LPARAM>(model.get()))?S_OK:E_FAIL;}
public:
    explicit Accessible(std::shared_ptr<Model> value):model(std::move(value)){}
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id,void** out)override{if(!out)return E_POINTER;*out=nullptr;
        if(id!=IID_IUnknown&&id!=IID_IDispatch&&id!=IID_IAccessible)return E_NOINTERFACE;*out=static_cast<IAccessible*>(this);AddRef();return S_OK;}
    ULONG STDMETHODCALLTYPE AddRef()override{return ++refs;}
    ULONG STDMETHODCALLTYPE Release()override{auto n=--refs;if(!n)delete this;return n;}
    HRESULT STDMETHODCALLTYPE GetTypeInfoCount(UINT* out)override{if(!out)return E_POINTER;*out=1;return S_OK;}
    HRESULT STDMETHODCALLTYPE GetTypeInfo(UINT index,LCID,ITypeInfo** out)override{return index?DISP_E_BADINDEX:type(out);}
    HRESULT STDMETHODCALLTYPE GetIDsOfNames(REFIID,LPOLESTR* names,UINT count,LCID,DISPID* ids)override{ITypeInfo* info=nullptr;auto hr=type(&info);if(SUCCEEDED(hr)){hr=DispGetIDsOfNames(info,names,count,ids);info->Release();}return hr;}
    HRESULT STDMETHODCALLTYPE Invoke(DISPID id,REFIID,LCID,WORD flags,DISPPARAMS* args,VARIANT* result,EXCEPINFO* exception,UINT* error)override{
        ITypeInfo* info=nullptr;auto hr=type(&info);if(SUCCEEDED(hr)){hr=DispInvoke(static_cast<IAccessible*>(this),info,id,flags,args,result,exception,error);info->Release();}return hr;}
    HRESULT STDMETHODCALLTYPE get_accParent(IDispatch** out)override{if(!out)return E_POINTER;*out=nullptr;auto s=model->read();
        return s.window?AccessibleObjectFromWindow(s.window,OBJID_WINDOW,IID_IDispatch,reinterpret_cast<void**>(out)):CO_E_OBJNOTCONNECTED;}
    HRESULT STDMETHODCALLTYPE get_accChildCount(long* out)override{if(!out)return E_POINTER;auto s=model->read();*out=s.window?LONG(s.labels.size()):0;return S_OK;}
    HRESULT STDMETHODCALLTYPE get_accChild(VARIANT,IDispatch** out)override{if(!out)return E_POINTER;*out=nullptr;return S_FALSE;}
    HRESULT STDMETHODCALLTYPE get_accName(VARIANT value,BSTR* out)override{auto s=model->read();int i=child(value,s);return i<0?E_INVALIDARG:string(i?s.labels[i-1]:s.title,out);}
    HRESULT STDMETHODCALLTYPE get_accValue(VARIANT,BSTR* out)override{if(!out)return E_POINTER;*out=nullptr;return S_FALSE;}
    HRESULT STDMETHODCALLTYPE get_accDescription(VARIANT value,BSTR* out)override{return get_accName(value,out);}
    HRESULT STDMETHODCALLTYPE get_accRole(VARIANT value,VARIANT* out)override{if(!out)return E_POINTER;VariantInit(out);auto s=model->read();int i=child(value,s);if(i<0)return E_INVALIDARG;
        out->vt=VT_I4;out->lVal=i?(size_t(i)<=s.roles.size()?s.roles[i-1]:s.childRole):s.role;return S_OK;}
    HRESULT STDMETHODCALLTYPE get_accState(VARIANT value,VARIANT* out)override{if(!out)return E_POINTER;VariantInit(out);auto s=model->read();int i=child(value,s);if(i<0)return E_INVALIDARG;
        out->vt=VT_I4;out->lVal=i?(STATE_SYSTEM_FOCUSABLE|STATE_SYSTEM_SELECTABLE):0;
        if(i&&size_t(i)<=s.states.size())out->lVal|=s.states[i-1];
        if(!s.window||!IsWindowVisible(s.window))out->lVal|=STATE_SYSTEM_INVISIBLE|STATE_SYSTEM_UNAVAILABLE;
        if(i&&s.selected==i-1){out->lVal|=STATE_SYSTEM_SELECTED;if(s.menuFocus||GetForegroundWindow()==s.window)out->lVal|=STATE_SYSTEM_FOCUSED;}return S_OK;}
    HRESULT STDMETHODCALLTYPE get_accHelp(VARIANT,BSTR* out)override{if(!out)return E_POINTER;*out=nullptr;return S_FALSE;}
    HRESULT STDMETHODCALLTYPE get_accHelpTopic(BSTR* file,VARIANT,long* topic)override{if(!file||!topic)return E_POINTER;*file=nullptr;*topic=-1;return S_FALSE;}
    HRESULT STDMETHODCALLTYPE get_accKeyboardShortcut(VARIANT,BSTR* out)override{if(!out)return E_POINTER;*out=nullptr;return S_FALSE;}
    HRESULT STDMETHODCALLTYPE get_accFocus(VARIANT* out)override{if(!out)return E_POINTER;VariantInit(out);auto s=model->read();
        if(!s.window||(!s.menuFocus&&GetForegroundWindow()!=s.window)||s.selected<0)return S_OK;out->vt=VT_I4;out->lVal=s.selected+1;return S_OK;}
    HRESULT STDMETHODCALLTYPE get_accSelection(VARIANT* out)override{if(!out)return E_POINTER;VariantInit(out);auto s=model->read();if(s.selected>=0&&s.window){out->vt=VT_I4;out->lVal=s.selected+1;}return S_OK;}
    HRESULT STDMETHODCALLTYPE get_accDefaultAction(VARIANT value,BSTR* out)override{if(!out)return E_POINTER;*out=nullptr;auto s=model->read();int i=child(value,s);
        if(i<=0||size_t(i)<=s.states.size()&&(s.states[i-1]&(STATE_SYSTEM_UNAVAILABLE|STATE_SYSTEM_INVISIBLE)))return S_FALSE;
        return string(size_t(i)<=s.actions.size()?s.actions[i-1]:s.action,out);}
    HRESULT STDMETHODCALLTYPE accSelect(long flags,VARIANT value)override{return flags&(SELFLAG_TAKEFOCUS|SELFLAG_TAKESELECTION)?post(value,SELECT):E_INVALIDARG;}
    HRESULT STDMETHODCALLTYPE accLocation(long* x,long* y,long* w,long* h,VARIANT value)override{if(!x||!y||!w||!h)return E_POINTER;*x=*y=*w=*h=0;auto s=model->read();int i=child(value,s);if(i<0)return E_INVALIDARG;
        if(!s.window)return CO_E_OBJNOTCONNECTED;RECT r{};if(i){if(size_t(i)>s.regions.size())return E_INVALIDARG;r=s.regions[i-1];}else if(!GetWindowRect(s.window,&r))return E_FAIL;
        *x=r.left;*y=r.top;*w=r.right-r.left;*h=r.bottom-r.top;return S_OK;}
    HRESULT STDMETHODCALLTYPE accNavigate(long direction,VARIANT start,VARIANT* out)override{if(!out)return E_POINTER;VariantInit(out);auto s=model->read();int i=child(start,s);if(i<0)return E_INVALIDARG;
        int next=direction==NAVDIR_FIRSTCHILD&&i==0?1:direction==NAVDIR_LASTCHILD&&i==0?int(s.labels.size()):direction==NAVDIR_NEXT?i+1:direction==NAVDIR_PREVIOUS?i-1:-1;
        if(next<=0||size_t(next)>s.labels.size())return S_FALSE;out->vt=VT_I4;out->lVal=next;return S_OK;}
    HRESULT STDMETHODCALLTYPE accHitTest(long x,long y,VARIANT* out)override{if(!out)return E_POINTER;VariantInit(out);auto s=model->read();if(!s.window)return CO_E_OBJNOTCONNECTED;
        for(size_t i=0;i<s.regions.size();i++)if(PtInRect(&s.regions[i],POINT{x,y})){out->vt=VT_I4;out->lVal=LONG(i+1);return S_OK;}
        RECT r{};if(GetWindowRect(s.window,&r)&&PtInRect(&r,POINT{x,y})){out->vt=VT_I4;out->lVal=CHILDID_SELF;return S_OK;}return S_FALSE;}
    HRESULT STDMETHODCALLTYPE accDoDefaultAction(VARIANT value)override{return post(value,CHOOSE);}
    HRESULT STDMETHODCALLTYPE put_accName(VARIANT,BSTR)override{return E_NOTIMPL;}
    HRESULT STDMETHODCALLTYPE put_accValue(VARIANT,BSTR)override{return E_NOTIMPL;}
};
}
