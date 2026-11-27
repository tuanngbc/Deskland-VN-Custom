// MSAA provider for native wallpaper controls. Standard Windows accessibility,
// immutable geometry snapshot, and posted UI actions; no cross-thread UI calls.
#pragma once
#include <windows.h>
#include <oleacc.h>
#include <vector>
#include <string>
#include <atomic>

namespace wallpaper {
inline constexpr UINT ACCESS_ACTION=WM_APP+91;
struct AccessibleControl {int action=0;std::wstring name;RECT rect{};bool selected=false;};
class AccessiblePage final:public IAccessible {
    std::atomic<ULONG> refs{1};HWND window;std::vector<AccessibleControl> controls;int focus;
    static HRESULT string(const std::wstring&text,BSTR*out){if(!out)return E_POINTER;*out=SysAllocStringLen(text.data(),UINT(text.size()));return *out?S_OK:E_OUTOFMEMORY;}
    int child(const VARIANT&v)const{return v.vt==VT_I4&&v.lVal>=0&&v.lVal<=LONG(controls.size())?v.lVal:-1;}
public:
    AccessiblePage(HWND h,std::vector<AccessibleControl> c,int selected):window(h),controls(std::move(c)),focus(selected){}
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid,void**out)override{if(!out)return E_POINTER;*out=nullptr;if(iid==IID_IUnknown||iid==IID_IDispatch||iid==IID_IAccessible){*out=static_cast<IAccessible*>(this);AddRef();return S_OK;}return E_NOINTERFACE;}
    ULONG STDMETHODCALLTYPE AddRef()override{return ++refs;}ULONG STDMETHODCALLTYPE Release()override{ULONG n=--refs;if(!n)delete this;return n;}
    HRESULT STDMETHODCALLTYPE GetTypeInfoCount(UINT*n)override{if(!n)return E_POINTER;*n=0;return S_OK;}
    HRESULT STDMETHODCALLTYPE GetTypeInfo(UINT,LCID,ITypeInfo**)override{return E_NOTIMPL;}
    HRESULT STDMETHODCALLTYPE GetIDsOfNames(REFIID,LPOLESTR*,UINT,LCID,DISPID*)override{return DISP_E_UNKNOWNNAME;}
    HRESULT STDMETHODCALLTYPE Invoke(DISPID,REFIID,LCID,WORD,DISPPARAMS*,VARIANT*,EXCEPINFO*,UINT*)override{return DISP_E_MEMBERNOTFOUND;}
    HRESULT STDMETHODCALLTYPE get_accParent(IDispatch**out)override{if(!out)return E_POINTER;*out=nullptr;return CreateStdAccessibleObject(window,OBJID_WINDOW,IID_IDispatch,reinterpret_cast<void**>(out));}
    HRESULT STDMETHODCALLTYPE get_accChildCount(LONG*n)override{if(!n)return E_POINTER;*n=LONG(controls.size());return S_OK;}
    HRESULT STDMETHODCALLTYPE get_accChild(VARIANT,IDispatch**out)override{if(!out)return E_POINTER;*out=nullptr;return S_FALSE;}
    HRESULT STDMETHODCALLTYPE get_accName(VARIANT v,BSTR*out)override{int n=child(v);if(n<0)return E_INVALIDARG;return string(n?controls[n-1].name:L"TLauncher Live Wallpaper",out);}
    HRESULT STDMETHODCALLTYPE get_accValue(VARIANT,BSTR*out)override{if(!out)return E_POINTER;*out=nullptr;return S_FALSE;}
    HRESULT STDMETHODCALLTYPE get_accDescription(VARIANT v,BSTR*out)override{return get_accName(v,out);}
    HRESULT STDMETHODCALLTYPE get_accRole(VARIANT v,VARIANT*out)override{if(!out)return E_POINTER;int n=child(v);if(n<0)return E_INVALIDARG;VariantInit(out);out->vt=VT_I4;out->lVal=n?ROLE_SYSTEM_PUSHBUTTON:ROLE_SYSTEM_PANE;return S_OK;}
    HRESULT STDMETHODCALLTYPE get_accState(VARIANT v,VARIANT*out)override{if(!out)return E_POINTER;int n=child(v);if(n<0)return E_INVALIDARG;VariantInit(out);out->vt=VT_I4;out->lVal=STATE_SYSTEM_FOCUSABLE;
        if(!IsWindowVisible(window))out->lVal|=STATE_SYSTEM_INVISIBLE;
        if(n){RECT client{};GetClientRect(window,&client);const auto&c=controls[n-1];RECT visible;if(!IntersectRect(&visible,&client,&c.rect))out->lVal|=STATE_SYSTEM_OFFSCREEN;if(c.selected)out->lVal|=STATE_SYSTEM_PRESSED;if(n==focus+1)out->lVal|=STATE_SYSTEM_FOCUSED;}return S_OK;}
    HRESULT STDMETHODCALLTYPE get_accHelp(VARIANT,BSTR*out)override{if(!out)return E_POINTER;*out=nullptr;return S_FALSE;}
    HRESULT STDMETHODCALLTYPE get_accHelpTopic(BSTR*out,VARIANT,LONG*n)override{if(out)*out=nullptr;if(n)*n=0;return S_FALSE;}
    HRESULT STDMETHODCALLTYPE get_accKeyboardShortcut(VARIANT,BSTR*out)override{return string(L"Tab; Enter or Space",out);}
    HRESULT STDMETHODCALLTYPE get_accFocus(VARIANT*out)override{if(!out)return E_POINTER;VariantInit(out);out->vt=VT_I4;out->lVal=focus+1;return S_OK;}
    HRESULT STDMETHODCALLTYPE get_accSelection(VARIANT*out)override{if(!out)return E_POINTER;VariantInit(out);return S_FALSE;}
    HRESULT STDMETHODCALLTYPE get_accDefaultAction(VARIANT v,BSTR*out)override{if(child(v)<=0)return E_INVALIDARG;return string(L"Activate",out);}
    HRESULT STDMETHODCALLTYPE accSelect(LONG flags,VARIANT v)override{int n=child(v);if(n<=0||!(flags&SELFLAG_TAKEFOCUS))return E_INVALIDARG;return PostMessageW(window,ACCESS_ACTION+1,WPARAM(controls[n-1].action),0)?S_OK:E_FAIL;}
    HRESULT STDMETHODCALLTYPE accLocation(LONG*x,LONG*y,LONG*w,LONG*h,VARIANT v)override{if(!x||!y||!w||!h)return E_POINTER;int n=child(v);if(n<0)return E_INVALIDARG;RECT r{};if(n)r=controls[n-1].rect;else GetClientRect(window,&r);MapWindowPoints(window,nullptr,reinterpret_cast<POINT*>(&r),2);*x=r.left;*y=r.top;*w=r.right-r.left;*h=r.bottom-r.top;return S_OK;}
    HRESULT STDMETHODCALLTYPE accNavigate(LONG direction,VARIANT from,VARIANT*out)override{if(!out)return E_POINTER;int n=child(from);if(n<0)return E_INVALIDARG;int next=direction==NAVDIR_FIRSTCHILD?1:direction==NAVDIR_LASTCHILD?int(controls.size()):direction==NAVDIR_NEXT?n+1:direction==NAVDIR_PREVIOUS?n-1:0;VariantInit(out);if(next<1||next>int(controls.size()))return S_FALSE;out->vt=VT_I4;out->lVal=next;return S_OK;}
    HRESULT STDMETHODCALLTYPE accHitTest(LONG x,LONG y,VARIANT*out)override{if(!out)return E_POINTER;POINT p{x,y};ScreenToClient(window,&p);VariantInit(out);out->vt=VT_I4;out->lVal=CHILDID_SELF;for(size_t i=0;i<controls.size();i++)if(PtInRect(&controls[i].rect,p)){out->lVal=LONG(i+1);break;}return S_OK;}
    HRESULT STDMETHODCALLTYPE accDoDefaultAction(VARIANT v)override{int n=child(v);if(n<=0)return E_INVALIDARG;return PostMessageW(window,ACCESS_ACTION,WPARAM(controls[n-1].action),0)?S_OK:E_FAIL;}
    HRESULT STDMETHODCALLTYPE put_accName(VARIANT,BSTR)override{return E_NOTIMPL;}
    HRESULT STDMETHODCALLTYPE put_accValue(VARIANT,BSTR)override{return E_NOTIMPL;}
};
}
