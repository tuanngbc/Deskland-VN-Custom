// Reuse the same snapshot-only MSAA implementation as original menu/Snap.
// The native Control Center validates an opaque layout revision on its UI.
#include "snap_accessibility.h"
namespace {
struct Entry{const wchar_t* name;RECT rect;LONG role,state;const wchar_t* action;};
struct Access{
    std::shared_ptr<tlauncher_snap_access::Model> model=std::make_shared<tlauncher_snap_access::Model>();
    tlauncher_snap_access::Accessible* provider=nullptr;
    ~Access(){disconnect();if(provider)provider->Release();}
    void disconnect(){std::lock_guard<std::mutex> guard(model->mutex);model->state=tlauncher_snap_access::Snapshot{};}
};
}
extern "C" {
void* tlauncher_control_access_create()noexcept{try{return new Access();}catch(...){return nullptr;}}
void tlauncher_control_access_free(void* raw)noexcept{delete static_cast<Access*>(raw);}
void tlauncher_control_access_close(void* raw)noexcept{if(raw)static_cast<Access*>(raw)->disconnect();}
void tlauncher_control_access_update(void* raw,HWND window,const wchar_t* title,const Entry* rows,unsigned count,int selected,LPARAM token)noexcept{
    try{
        if(!raw||!window||!rows||count>64)return;
        tlauncher_snap_access::Snapshot value;value.window=window;value.title=title?title:L"";value.role=ROLE_SYSTEM_PANE;value.actionToken=token;
        value.selected=selected>=0&&unsigned(selected)<count?selected:-1;
        for(unsigned i=0;i<count;i++){
            value.labels.emplace_back(rows[i].name?rows[i].name:L"");value.regions.push_back(rows[i].rect);value.roles.push_back(rows[i].role);
            value.states.push_back(rows[i].state);value.actions.emplace_back(rows[i].action?rows[i].action:L"");
        }
        auto& model=*static_cast<Access*>(raw)->model;std::lock_guard<std::mutex> guard(model.mutex);model.state=std::move(value);
    }catch(...){if(raw)static_cast<Access*>(raw)->disconnect();}
}
LRESULT tlauncher_control_access_object(void* raw,WPARAM parameter)noexcept{
    if(!raw)return 0;auto& access=*static_cast<Access*>(raw);
    if(!access.provider)access.provider=new(std::nothrow)tlauncher_snap_access::Accessible(access.model);
    return access.provider?LresultFromObject(IID_IAccessible,parameter,access.provider):0;
}
// Hosted fixture retains the actual COM object across page changes/destruction.
void* tlauncher_control_access_get(HWND window)noexcept{
    IAccessible* value=nullptr;return SUCCEEDED(AccessibleObjectFromWindow(window,OBJID_CLIENT,IID_IAccessible,reinterpret_cast<void**>(&value)))?value:nullptr;
}
void tlauncher_control_access_release(void* raw)noexcept{if(raw)static_cast<IAccessible*>(raw)->Release();}
int tlauncher_control_access_probe(void* raw,unsigned expected,int selected,int mode)noexcept{
    auto* access=static_cast<IAccessible*>(raw);if(!access)return 1;
    long count=-1;if(FAILED(access->get_accChildCount(&count))||count!=LONG(expected))return 2;
    VARIANT child{},value{};child.vt=VT_I4;child.lVal=selected+1;
    if(mode==3)return access->accDoDefaultAction(child)==CO_E_OBJNOTCONNECTED?0:3;
    if(mode==1)return SUCCEEDED(access->accSelect(SELFLAG_TAKEFOCUS|SELFLAG_TAKESELECTION,child))?0:4;
    if(mode==2)return SUCCEEDED(access->accDoDefaultAction(child))?0:5;
    if(mode==4)return access->accDoDefaultAction(child)==E_ACCESSDENIED?0:6;
    if(mode==5){if(FAILED(access->get_accSelection(&value))||value.vt!=VT_I4||value.lVal!=selected+1)return 7;return 0;}
    VARIANT root{};root.vt=VT_I4;root.lVal=CHILDID_SELF;
    if(FAILED(access->get_accRole(root,&value))||value.vt!=VT_I4||value.lVal!=ROLE_SYSTEM_PANE)return 8;
    long rx=0,ry=0,rw=0,rh=0;if(FAILED(access->accLocation(&rx,&ry,&rw,&rh,root)))return 13;
    for(long i=1;i<=count;i++){
        child.lVal=i;BSTR name=nullptr;if(FAILED(access->get_accName(child,&name))||!name||!SysStringLen(name)){SysFreeString(name);return 9;}SysFreeString(name);
        long x=0,y=0,w=0,h=0;if(FAILED(access->accLocation(&x,&y,&w,&h,child))||w<=0||h<=0)return 10;
        if(x<rx||y<ry||x+w>rx+rw||y+h>ry+rh)return 14;
        if(FAILED(access->accHitTest(x+w/2,y+h/2,&value))||value.vt!=VT_I4||value.lVal!=i)return 11;
        if(FAILED(access->get_accRole(child,&value))||value.vt!=VT_I4||(value.lVal!=ROLE_SYSTEM_PUSHBUTTON&&value.lVal!=ROLE_SYSTEM_STATICTEXT))return 12;
    }
    return 0;
}
}
