// One wallpaper session owns all windows, one WGL context and its render thread.
// UI only submits snapshots; no renderer waits or graphics work on the core loop.
#pragma once
#include "scene.h"
#include <thread>
#include <mutex>
#include <atomic>
#include <memory>
#include <chrono>
#include <wtsapi32.h>
#include <powersetting.h>
#include <dwmapi.h>

namespace wallpaper {
struct Metrics {uint64_t frames=0,missed=0;double lastFrameMs=0;size_t surfaces=0,scenes=0;bool running=false,paused=false;std::string error;};
struct SessionState {
    std::mutex mutex;Snapshot latest;Metrics metrics;uint64_t revision=0;
    std::atomic<bool> stop{false},finished{false};HANDLE event=CreateEventW(nullptr,FALSE,FALSE,nullptr);
    ~SessionState(){if(event)CloseHandle(event);}
};
inline LRESULT CALLBACK windowProcedure(HWND h,UINT m,WPARAM w,LPARAM l){
    if(m==WM_NCHITTEST)return HTTRANSPARENT;
    if(m==WM_MOUSEACTIVATE)return MA_NOACTIVATE;
    if(m==WM_ERASEBKGND)return 1;
    if(m==WM_CLOSE)return 0; // lifecycle is exclusively session-owned
    return DefWindowProcW(h,m,w,l);
}
inline HWND createWindow(){
    static std::once_flag once;std::call_once(once,[]{WNDCLASSW c{};c.hInstance=GetModuleHandleW(nullptr);c.lpfnWndProc=windowProcedure;c.lpszClassName=L"TLauncher.Native.Wallpaper";c.style=CS_OWNDC;RegisterClassW(&c);});
    HWND h=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_TRANSPARENT,L"TLauncher.Native.Wallpaper",L"",WS_POPUP,0,0,8,8,nullptr,nullptr,GetModuleHandleW(nullptr),nullptr);
    if(!h)throw std::runtime_error("Wallpaper window unavailable");return h;
}
struct Window {HWND h=createWindow();~Window(){if(h)DestroyWindow(h);}Window()=default;Window(const Window&)=delete;};
struct ShellHost {HWND parent=nullptr,anchor=nullptr;};
inline ShellHost shellHost(){
    HWND prog=FindWindowW(L"Progman",nullptr);if(!prog)return {};
    HWND view=FindWindowExW(prog,nullptr,L"SHELLDLL_DefView",nullptr);
    DWORD_PTR result=0;
    if(view){
        if(!FindWindowExW(prog,nullptr,L"WorkerW",nullptr))SendMessageTimeoutW(prog,0x052c,0xd,1,SMTO_ABORTIFHUNG|SMTO_BLOCK,500,&result);
        return {prog,view};
    }
    auto classic=[]()->HWND {HWND host=nullptr;EnumWindows([](HWND h,LPARAM p)->BOOL{if(FindWindowExW(h,nullptr,L"SHELLDLL_DefView",nullptr)){*reinterpret_cast<HWND*>(p)=FindWindowExW(nullptr,h,L"WorkerW",nullptr);return FALSE;}return TRUE;},reinterpret_cast<LPARAM>(&host));return host;};
    HWND host=classic();if(!host){SendMessageTimeoutW(prog,0x052c,0xd,1,SMTO_ABORTIFHUNG|SMTO_BLOCK,500,&result);host=classic();}return {host,nullptr};
}
class Output {
    GL&g;Context&contextOwner;Window window;HDC dc=nullptr;GLuint framebuffer=0;Texture color;std::unique_ptr<SceneRuntime> runtime;
    int sceneId=-1,bufferWidth=0,bufferHeight=0;RECT rect{};ShellHost host{};
public:
    std::wstring monitor;bool visible=false;
    Output(GL&gl,Context&context,Monitor m,ShellHost shell):g(gl),contextOwner(context),rect(m.rect),host(shell),monitor(std::move(m.id)){
        try{
            if(!IsWindow(host.parent))throw std::runtime_error("Desktop host unavailable; Windows wallpaper retained");
            SetWindowLongPtrW(window.h,GWL_STYLE,WS_CHILD|WS_CLIPSIBLINGS);SetLastError(0);
            if(!SetParent(window.h,host.parent)&&GetLastError())throw std::runtime_error("Desktop attachment failed");
            if(host.anchor){SetWindowLongPtrW(window.h,GWL_EXSTYLE,GetWindowLongPtrW(window.h,GWL_EXSTYLE)|WS_EX_LAYERED);if(!SetLayeredWindowAttributes(window.h,0,255,LWA_ALPHA))throw std::runtime_error("Desktop composition failed");}
            POINT p{rect.left,rect.top};ScreenToClient(host.parent,&p);
            if(!SetWindowPos(window.h,host.anchor?host.anchor:HWND_BOTTOM,p.x,p.y,rect.right-rect.left,rect.bottom-rect.top,SWP_NOACTIVATE))throw std::runtime_error("Desktop positioning failed");
            dc=GetDC(window.h);PIXELFORMATDESCRIPTOR fmt{};fmt.nSize=sizeof(fmt);fmt.nVersion=1;fmt.dwFlags=PFD_DRAW_TO_WINDOW|PFD_SUPPORT_OPENGL|PFD_DOUBLEBUFFER;fmt.iPixelType=PFD_TYPE_RGBA;fmt.cColorBits=32;fmt.cAlphaBits=8;int pixel=ChoosePixelFormat(dc,&fmt);
            if(!pixel||!SetPixelFormat(dc,pixel,&fmt))throw std::runtime_error("Monitor pixel format unavailable");g.GenFramebuffers(1,&framebuffer);if(!framebuffer)throw std::runtime_error("Wallpaper framebuffer unavailable");
        }catch(...){if(dc){ReleaseDC(window.h,dc);dc=nullptr;}throw;}
    }
    ~Output(){contextOwner.current();runtime.reset();if(framebuffer)g.DeleteFramebuffers(1,&framebuffer);if(dc)ReleaseDC(window.h,dc);}
    void show(bool on){if(on!=visible){ShowWindow(window.h,on?SW_SHOWNOACTIVATE:SW_HIDE);visible=on;}}
    bool intact()const{return IsWindow(window.h)&&IsWindow(host.parent)&&GetParent(window.h)==host.parent;}
    bool matches(const Monitor&m)const{return monitor==m.id&&EqualRect(&rect,&m.rect);}
    void draw(Context&context,const Snapshot&s,const View&view,int scene,Texture&noise,Program&post,double time,float dt,const std::atomic<bool>&stop){
        if(!context.current(dc))throw std::runtime_error("Wallpaper device/context lost");
        if(sceneId!=scene){runtime.reset();runtime=std::make_unique<SceneRuntime>(g,scene);sceneId=scene;}
        const int w=rect.right-rect.left,h=rect.bottom-rect.top;
        // Downscale allocation and shader coordinates together; no panorama atlas.
        const int bw=std::max(1,int(w*s.config.resolution)),bh=std::max(1,int(h*s.config.resolution));
        if(bw!=bufferWidth||bh!=bufferHeight){color.image(g,4,bw,bh,0x8058,GL_RGBA,GL_UNSIGNED_BYTE,nullptr);bufferWidth=bw;bufferHeight=bh;
            g.BindFramebuffer(0x8D40,framebuffer);g.FramebufferTexture2D(0x8D40,0x8CE0,GL_TEXTURE_2D,color.name(),0);if(g.CheckFramebufferStatus(0x8D40)!=0x8CD5)throw std::runtime_error("Wallpaper surface incomplete");}
        g.BindFramebuffer(0x8D40,framebuffer);glViewport(0,0,bw,bh);noise.bind(g,0);
        std::vector<RECT> icons=s.icons;if(s.config.layout!=Layout::Panorama){for(auto&r:icons)OffsetRect(&r,-rect.left,-rect.top);}else{RECT all=bounds(s.monitors);for(auto&r:icons)OffsetRect(&r,-all.left,-all.top);}
        // Existing assets expect physical pixels. Resolution scaling is explicit.
        runtime->render(s.config,view,icons,s.weather,time,dt,stop,float(bw)/w,float(bh)/h);
        if(stop)return;g.BindFramebuffer(0x8D40,0);glViewport(0,0,w,h);color.bind(g,4);post.use();post.i("scene",4);post.v2("size",float(w),float(h));post.v2("origin",view.x,view.y);post.v2("world",view.worldW,view.worldH);post.f("time",float(time));
        SYSTEMTIME st{};GetLocalTime(&st);post.f("hour",s.config.clockLighting?st.wHour+st.wMinute/60.f:s.config.manualHour);post.f("lighting",s.config.solar?1.f:0.f);
        float rain=0,snow=0,cloud=0,wind=s.config.wind;switch(s.config.weather){case WeatherMode::Rain:rain=1;cloud=.8f;break;case WeatherMode::Snow:snow=1;cloud=.7f;break;case WeatherMode::Cloudy:cloud=1;break;case WeatherMode::Live:if(s.weather.available){rain=s.weather.rain;snow=s.weather.snow;cloud=s.weather.cloud;wind=s.weather.wind;}break;case WeatherMode::Offline:{float k=sin(float(time)*.0021f);cloud=abs(k);rain=std::max(0.f,k-.4f);break;}default:break;}
        post.f("rain",rain);post.f("snow",snow);post.f("cloud",cloud);post.f("wind",wind);glDrawArrays(GL_TRIANGLES,0,3);
        if(glGetError()!=GL_NO_ERROR||!SwapBuffers(dc))throw std::runtime_error("Wallpaper presentation failed; native core unaffected");show(true);
    }
    void click(float x,float y){if(runtime)runtime->click(x,y);}
};
class Session {
    std::shared_ptr<SessionState> state=std::make_shared<SessionState>();std::thread thread;
    static void run(const std::shared_ptr<SessionState>&state){
        HRESULT com=CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED);auto start=std::chrono::steady_clock::now();
        try{
            Window hidden;Context context(hidden.h);GL gl;GLuint vao=0;gl.GenVertexArrays(1,&vao);gl.BindVertexArray(vao);
            struct Vao {GL&g;GLuint id;~Vao(){g.DeleteVertexArrays(1,&id);}}vaoOwner{gl,vao};
            Texture noise;auto pixels=noisePixels();noise.image(gl,0,256,256,0x8058,GL_RGBA,GL_UNSIGNED_BYTE,pixels.data(),true);pixels.clear();pixels.shrink_to_fit();
            Program post(gl,assetText(assetsRoot()/L"wallpaper/shaders/post.frag"));
            std::vector<std::unique_ptr<Output>> outputs;std::vector<Monitor> topologyBefore;
            auto deadline=std::chrono::steady_clock::now(),last=deadline;double elapsed=0;
            while(!state->stop){
                MSG message;while(PeekMessageW(&message,nullptr,0,0,PM_REMOVE)){TranslateMessage(&message);DispatchMessageW(&message);}
                Snapshot s;{std::lock_guard<std::mutex> lock(state->mutex);s=state->latest;}
                if(!s.config.enabled)break;
                bool topologyChanged=s.monitors!=topologyBefore;bool hostLost=false;for(const auto&o:outputs)if(!o->intact())hostLost=true;
                if(topologyChanged||hostLost){context.current();outputs.clear();topologyBefore=s.monitors;ShellHost host=shellHost();
                    uint64_t pixels=0;for(const auto&m:s.monitors){pixels+=uint64_t(m.rect.right-m.rect.left)*(m.rect.bottom-m.rect.top);if(pixels>32*1024*1024)throw std::runtime_error("Monitor surfaces exceed 128 MiB budget");outputs.push_back(std::make_unique<Output>(gl,context,m,host));}
                }
                auto now=std::chrono::steady_clock::now();float dt=std::clamp(std::chrono::duration<float>(now-last).count(),0.f,.1f);last=now;
                bool paused=s.config.paused||s.locked||s.displayOff;
                if(!paused)elapsed+=dt;
                bool active=false;RECT all=bounds(s.monitors);auto frameStart=now;
                for(size_t i=0;i<outputs.size();i++){
                    auto&o=*outputs[i];const auto&m=s.monitors[i];auto assignment=s.config.displays.find(m.id);bool enabled=assignment==s.config.displays.end()||assignment->second.enabled;
                    if(!enabled){o.show(false);continue;}if(paused||(s.config.pauseCovered&&covered(m.rect,s.opaqueWindows)))continue;
                    int scene=s.config.layout==Layout::Independent&&assignment!=s.config.displays.end()?assignment->second.scene:s.config.scene;
                    auto view=viewport(m,all,s.config.layout);
                    if(s.click&&s.desktopPointer&&PtInRect(&m.rect,s.pointer)){float x=float(s.pointer.x-(s.config.layout==Layout::Panorama?all.left:m.rect.left)),y=float(s.pointer.y-(s.config.layout==Layout::Panorama?all.top:m.rect.top));o.click(x,y);}
                    o.draw(context,s,view,scene,noise,post,elapsed,dt,state->stop);active=true;
                }
                if(s.click){std::lock_guard<std::mutex> lock(state->mutex);state->latest.click=false;}
                {std::lock_guard<std::mutex> lock(state->mutex);state->metrics.running=true;state->metrics.paused=!active;state->metrics.surfaces=outputs.size();state->metrics.scenes=outputs.size();if(active)state->metrics.frames++;state->metrics.lastFrameMs=std::chrono::duration<double,std::milli>(std::chrono::steady_clock::now()-frameStart).count();}
                if(!active){MsgWaitForMultipleObjects(1,&state->event,FALSE,INFINITE,QS_ALLINPUT);last=std::chrono::steady_clock::now();deadline=last;continue;}
                deadline+=std::chrono::microseconds(1000000/s.config.fps);now=std::chrono::steady_clock::now();if(deadline<now){std::lock_guard<std::mutex> lock(state->mutex);state->metrics.missed++;deadline=now;}
                DWORD wait=DWORD(std::max<int64_t>(0,std::chrono::duration_cast<std::chrono::milliseconds>(deadline-now).count()));
                // Input/configuration can wake the wait without allowing a frame
                // before its deadline. No artificial sleeps or UI thread pacing.
                while(!state->stop&&std::chrono::steady_clock::now()<deadline){MsgWaitForMultipleObjects(1,&state->event,FALSE,wait,QS_ALLINPUT);MSG msg;while(PeekMessageW(&msg,nullptr,0,0,PM_REMOVE)){TranslateMessage(&msg);DispatchMessageW(&msg);}now=std::chrono::steady_clock::now();wait=DWORD(std::max<int64_t>(1,std::chrono::duration_cast<std::chrono::milliseconds>(deadline-now).count()));}
            }
            context.current();outputs.clear();
        }catch(const std::exception&e){std::lock_guard<std::mutex> lock(state->mutex);state->metrics.error=e.what();}
        if(SUCCEEDED(com))CoUninitialize();{std::lock_guard<std::mutex> lock(state->mutex);state->metrics.running=false;state->metrics.surfaces=0;state->metrics.scenes=0;}state->finished=true;SetEvent(state->event);
    }
public:
    explicit Session(const Snapshot&s){if(!state->event)throw std::runtime_error("Wallpaper cancellation event unavailable");state->latest=s;thread=std::thread(run,state);}
    ~Session(){stop();if(thread.joinable())thread.join();}Session(const Session&)=delete;
    void publish(Snapshot s){std::lock_guard<std::mutex> lock(state->mutex);s.click=s.click||state->latest.click;state->latest=std::move(s);state->revision++;SetEvent(state->event);}
    void stop(){state->stop=true;SetEvent(state->event);}bool finished()const{return state->finished.load();}
    Metrics metrics(){std::lock_guard<std::mutex> lock(state->mutex);return state->metrics;}
};
}
