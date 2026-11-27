// One wallpaper session owns all windows, one WGL context and its render thread.
// UI only submits snapshots; no renderer waits or graphics work on the core loop.
#pragma once
#include "scene.h"
#include "icons.h"
#include "present.h"
#include <thread>
#include <mutex>
#include <atomic>
#include <memory>
#include <chrono>
#include <wtsapi32.h>
#include <powersetting.h>
#include <dwmapi.h>

namespace wallpaper {
struct OutputMetrics {std::wstring monitor;int scene=0;bool visible=false,gpu=false;double cpuMs=0,gpuMs=-1,presentationMs=0,readbackMs=0;uint64_t readbackBytes=0,copyBytes=0;};
struct Metrics {
    uint64_t frames=0,missed=0,textureBytes=0,textures=0,surfaceBytes=0,uploadBytes=0;
    double lastFrameMs=0,fps=0,readbackPerSecond=0,gpuMs=-1,presentationMs=0,readbackMs=0,shaderMs=0,fieldMs=0,simulationMs=0,renderThreadCpuMs=0;
    int fpsTarget=0;size_t surfaces=0,scenes=0,targets=0,contexts=0,devices=0;
    bool running=false,paused=false;std::string error,presentation;std::wstring adapter;std::vector<OutputMetrics> outputs;
};
inline std::wstring diagnostics(const Metrics&m){
    std::wostringstream s;s.setf(std::ios::fixed);s.precision(3);
    s<<L"PRESENTATION_MODE="<<std::wstring(m.presentation.begin(),m.presentation.end())<<L"\r\nGPU_ADAPTER="<<m.adapter
     <<L"\r\nACTIVE_MONITORS="<<m.surfaces<<L"\r\nACTIVE_SCENES="<<m.scenes<<L"\r\nACTIVE_RENDER_TARGETS="<<m.targets
     <<L"\r\nGL_CONTEXTS="<<m.contexts<<L"\r\nD3D_DEVICES="<<m.devices<<L"\r\nFPS_TARGET="<<m.fpsTarget<<L"\r\nFPS_ACTUAL="<<m.fps
     <<L"\r\nCPU_FRAME_MS="<<m.lastFrameMs<<L"\r\nRENDER_THREAD_CPU_MS="<<m.renderThreadCpuMs<<L"\r\nGPU_RENDER_MS=";
    if(m.gpuMs>=0)s<<m.gpuMs;else s<<L"unavailable";
    s<<L"\r\nGPU_PRESENT_MS=unavailable (cross-API GPU timing)\r\nPRESENT_CPU_MS="<<m.presentationMs<<L"\r\nREADBACK_CPU_MS="<<m.readbackMs
     <<L"\r\nREADBACK_BYTES_PER_SECOND="<<m.readbackPerSecond<<L"\r\nMISSED_FRAME_DEADLINES="<<m.missed
     <<L"\r\nGL_TEXTURE_BYTES="<<m.textureBytes<<L"\r\nGL_TEXTURE_COUNT="<<m.textures<<L"\r\nPRESENT_SURFACE_BYTES="<<m.surfaceBytes
     <<L"\r\nTEXTURE_UPLOAD_BYTES="<<m.uploadBytes<<L"\r\nSHADER_COMPILE_MS="<<m.shaderMs<<L"\r\nICON_FIELD_MS="<<m.fieldMs<<L"\r\nSIMULATION_CPU_MS="<<m.simulationMs<<L"\r\n";
    for(const auto&o:m.outputs)s<<L"OUTPUT="<<o.monitor<<L" SCENE="<<o.scene<<L" VISIBLE="<<o.visible<<L" GPU="<<o.gpu<<L" CPU_MS="<<o.cpuMs<<L" GPU_MS="<<o.gpuMs<<L" PRESENT_MS="<<o.presentationMs<<L" READBACK_BYTES="<<o.readbackBytes<<L" COPY_BYTES="<<o.copyBytes<<L"\r\n";
    if(!m.error.empty())s<<L"WALLPAPER_ERROR="<<std::wstring(m.error.begin(),m.error.end())<<L"\r\n";
    return s.str();
}
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
inline HWND createWindow(bool gl=true){
    static std::once_flag once;std::call_once(once,[]{WNDCLASSW c{};c.hInstance=GetModuleHandleW(nullptr);c.lpfnWndProc=windowProcedure;c.lpszClassName=L"TLauncher.Native.Wallpaper";RegisterClassW(&c);c.lpszClassName=L"TLauncher.Native.WallpaperGL";c.style=CS_OWNDC;RegisterClassW(&c);});
    HWND h=CreateWindowExW(WS_EX_TOOLWINDOW|WS_EX_NOACTIVATE|WS_EX_TRANSPARENT,gl?L"TLauncher.Native.WallpaperGL":L"TLauncher.Native.Wallpaper",L"",WS_POPUP,0,0,8,8,nullptr,nullptr,GetModuleHandleW(nullptr),nullptr);
    if(!h)throw std::runtime_error("Wallpaper window unavailable");return h;
}
struct Window {HWND h;explicit Window(bool gl=true):h(createWindow(gl)){}~Window(){if(h)DestroyWindow(h);}Window(const Window&)=delete;};
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
    GL&g;Context&contextOwner;Bridge*bridge;Window window{false};GLuint framebuffer=0;Texture color;std::unique_ptr<Presentation> presentation;
    int bufferWidth=0,bufferHeight=0;RECT rect{};ShellHost host{};GpuMeasure timing{g};
    OutputMetrics measured;
public:
    std::wstring monitor;bool visible=false;
    Output(GL&gl,Context&context,Bridge*gpu,Monitor m,ShellHost shell):g(gl),contextOwner(context),bridge(gpu),rect(m.rect),host(shell),monitor(std::move(m.id)){
        try{
            if(!IsWindow(host.parent))throw std::runtime_error("Desktop host unavailable; Windows wallpaper retained");
            SetWindowLongPtrW(window.h,GWL_STYLE,WS_CHILD|WS_CLIPSIBLINGS);SetLastError(0);
            if(!SetParent(window.h,host.parent)&&GetLastError())throw std::runtime_error("Desktop attachment failed");
            if(bridge)SetWindowLongPtrW(window.h,GWL_EXSTYLE,GetWindowLongPtrW(window.h,GWL_EXSTYLE)|WS_EX_NOREDIRECTIONBITMAP);
            else if(host.anchor){SetWindowLongPtrW(window.h,GWL_EXSTYLE,GetWindowLongPtrW(window.h,GWL_EXSTYLE)|WS_EX_LAYERED);if(!SetLayeredWindowAttributes(window.h,0,255,LWA_ALPHA))throw std::runtime_error("Desktop composition failed");}
            POINT p{rect.left,rect.top};ScreenToClient(host.parent,&p);
            if(!SetWindowPos(window.h,host.anchor?host.anchor:HWND_BOTTOM,p.x,p.y,rect.right-rect.left,rect.bottom-rect.top,SWP_NOACTIVATE))throw std::runtime_error("Desktop positioning failed");
            g.GenFramebuffers(1,&framebuffer);if(!framebuffer)throw std::runtime_error("Wallpaper framebuffer unavailable");
        }catch(...){if(framebuffer)g.DeleteFramebuffers(1,&framebuffer);framebuffer=0;throw;}
    }
    ~Output(){contextOwner.current();presentation.reset();if(framebuffer)g.DeleteFramebuffers(1,&framebuffer);}
    void show(bool on){if(on!=visible){ShowWindow(window.h,on?SW_SHOWNOACTIVATE:SW_HIDE);visible=on;}}
    bool intact()const{return IsWindow(window.h)&&IsWindow(host.parent)&&GetParent(window.h)==host.parent;}
    bool matches(const Monitor&m)const{return monitor==m.id&&EqualRect(&rect,&m.rect);}
    bool gpu()const{return presentation&&presentation->gpu();}
    uint64_t surfaceBytes()const{return presentation?presentation->surfaceBytes():0;}
    size_t targets()const{return framebuffer?1+(presentation?1:0):0;}
    OutputMetrics metrics(bool rendered)const{auto result=measured;result.monitor=monitor;result.visible=visible;result.gpu=gpu();if(!rendered){result.cpuMs=result.presentationMs=result.readbackMs=0;result.readbackBytes=result.copyBytes=0;result.gpuMs=-1;}return result;}
    void draw(Context&context,const Snapshot&s,const View&view,SceneRuntime&runtime,Texture&noise,Program&post,double time,float dt,const std::atomic<bool>&stop){
        measured={};measured.scene=runtime.scene();CpuMeasure cpu{measured.cpuMs};timing.begin();
        if(!context.current())throw std::runtime_error("Wallpaper device/context lost");
        const int w=rect.right-rect.left,h=rect.bottom-rect.top;
        // Downscale allocation and shader coordinates together; no panorama atlas.
        const int bw=std::max(1,int(w*s.config.resolution)),bh=std::max(1,int(h*s.config.resolution));
        if(bw!=bufferWidth||bh!=bufferHeight){presentation.reset();color.image(g,4,bw,bh,0x8058,GL_RGBA,GL_UNSIGNED_BYTE,nullptr);bufferWidth=bw;bufferHeight=bh;
            g.BindFramebuffer(0x8D40,framebuffer);g.FramebufferTexture2D(0x8D40,0x8CE0,GL_TEXTURE_2D,color.name(),0);if(g.CheckFramebufferStatus(0x8D40)!=0x8CD5)throw std::runtime_error("Wallpaper surface incomplete");}
        g.BindFramebuffer(0x8D40,framebuffer);glViewport(0,0,bw,bh);noise.bind(g,0);
        std::vector<RECT> icons=s.icons;if(s.config.layout!=Layout::Panorama){for(auto&r:icons)OffsetRect(&r,-rect.left,-rect.top);}else{RECT all=bounds(s.monitors);for(auto&r:icons)OffsetRect(&r,-all.left,-all.top);}
        // Existing assets expect physical pixels. Resolution scaling is explicit.
        Config effective=s.config;
        if(s.config.layout==Layout::Independent){auto assignment=s.config.displays.find(monitor);if(assignment!=s.config.displays.end())for(const auto&[scene,params]:assignment->second.sceneParams)for(const auto&[key,value]:params)effective.sceneParams[scene][key]=value;}
        auto weather=resolvedWeather(effective,s.weather,time);runtime.render(effective,view,icons,weather,time,dt,stop,float(bw)/w,float(bh)/h);
        if(stop)return;
        if(!presentation){try{presentation=std::make_unique<Presentation>(g,bridge,window.h,bw,bh,w,h);}catch(const std::exception&){
                if(!bridge)throw;bridge=nullptr;SetWindowLongPtrW(window.h,GWL_EXSTYLE,(GetWindowLongPtrW(window.h,GWL_EXSTYLE)&~WS_EX_NOREDIRECTIONBITMAP)|(host.anchor?WS_EX_LAYERED:0));if(host.anchor)SetLayeredWindowAttributes(window.h,0,255,LWA_ALPHA);for(int i=0;i<8&&glGetError()!=GL_NO_ERROR;i++){}presentation=std::make_unique<Presentation>(g,nullptr,window.h,bw,bh,w,h);
            }}
        g.BindFramebuffer(0x8D40,presentation->begin());glViewport(0,0,bw,bh);color.bind(g,4);post.use();post.i("scene",4);post.v2("size",float(bw),float(bh));post.v2("physicalSize",float(w),float(h));post.f("flip",presentation->gpu()?1.f:0.f);post.v2("origin",view.x,view.y);post.v2("world",view.worldW,view.worldH);post.f("time",float(time));
        SYSTEMTIME st{};GetLocalTime(&st);post.f("hour",s.config.clockLighting?st.wHour+st.wMinute/60.f:s.config.manualHour);post.f("lighting",s.config.solar?1.f:0.f);
        post.f("rain",weather.rain);post.f("snow",weather.snow);post.f("cloud",weather.cloud);post.f("wind",weather.wind);glDrawArrays(GL_TRIANGLES,0,3);
        if(glGetError()!=GL_NO_ERROR)throw std::runtime_error("Wallpaper presentation failed; native core unaffected");if(presentation->present())show(true);
        timing.end();measured.gpuMs=timing.milliseconds();measured.readbackBytes=presentation->readbackBytes;measured.copyBytes=presentation->copyBytes;measured.readbackMs=presentation->readbackMs;measured.presentationMs=presentation->presentationMs;
    }
};
class Session {
    std::shared_ptr<SessionState> state=std::make_shared<SessionState>();std::thread thread;
    static void run(const std::shared_ptr<SessionState>&state){
        HRESULT com=CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED);graphicsStats={};
        auto sampleStart=SteadyClock::now();uint64_t sampleFrames=0,sampleReadback=0,sampleCpu=0;
        auto threadCpu=[](){FILETIME creation{},exit{},kernel{},user{};if(!GetThreadTimes(GetCurrentThread(),&creation,&exit,&kernel,&user))return uint64_t(0);ULARGE_INTEGER k{},u{};k.LowPart=kernel.dwLowDateTime;k.HighPart=kernel.dwHighDateTime;u.LowPart=user.dwLowDateTime;u.HighPart=user.dwHighDateTime;return uint64_t(k.QuadPart+u.QuadPart);};
        try{
            Window hidden;Context context(hidden.h);GL gl;GLuint vao=0;gl.GenVertexArrays(1,&vao);gl.BindVertexArray(vao);
            struct Vao {GL&g;GLuint id;~Vao(){g.DeleteVertexArrays(1,&id);}}vaoOwner{gl,vao};
            Texture noise;auto pixels=noisePixels();noise.image(gl,0,256,256,0x8058,GL_RGBA,GL_UNSIGNED_BYTE,pixels.data(),true);pixels.clear();pixels.shrink_to_fit();
            Program post(gl,assetText(assetsRoot()/L"wallpaper/shaders/post.frag"));
            std::unique_ptr<Bridge> bridge;try{bridge=std::make_unique<Bridge>();}catch(const std::exception&){}
            {std::lock_guard<std::mutex> lock(state->mutex);state->metrics.presentation=bridge?"GPU DirectComposition":"Asynchronous bounded DIB fallback";}
            std::vector<std::unique_ptr<Output>> outputs;std::vector<Monitor> topologyBefore;std::map<std::wstring,std::unique_ptr<SceneRuntime>> worlds;IconGeometry icons;
            auto deadline=std::chrono::steady_clock::now(),last=deadline;double elapsed=0;
            while(!state->stop){
                MSG message;while(PeekMessageW(&message,nullptr,0,0,PM_REMOVE)){TranslateMessage(&message);DispatchMessageW(&message);}
                Snapshot s;{std::lock_guard<std::mutex> lock(state->mutex);s=state->latest;state->latest.input.clear();state->latest.click=false;}
                if(!s.config.enabled)break;s.icons=icons.poll(state->stop);
                // Worst presentation path: scene + fallback + 3 PBOs + CPU readback,
                // 24 bytes per rendered pixel. Respect explicit quality settings.
                uint64_t surfaceBytes=0;for(const auto&m:s.monitors){auto a=s.config.displays.find(m.id);if(a!=s.config.displays.end()&&!a->second.enabled)continue;surfaceBytes+=uint64_t(std::max(1,int((m.rect.right-m.rect.left)*s.config.resolution)))*std::max(1,int((m.rect.bottom-m.rect.top)*s.config.resolution))*24;}
                if(surfaceBytes>192ull*1024*1024)throw std::runtime_error("Wallpaper surfaces exceed 192 MiB budget; select lower resolution or fewer displays");
                bool topologyChanged=s.monitors!=topologyBefore;bool hostLost=false;for(const auto&o:outputs)if(o&&!o->intact())hostLost=true;
                if(topologyChanged||hostLost){context.current();worlds.clear();outputs.clear();topologyBefore=s.monitors;outputs.resize(s.monitors.size());
                }
                auto now=std::chrono::steady_clock::now();float dt=std::clamp(std::chrono::duration<float>(now-last).count(),0.f,.1f);last=now;
                bool paused=s.config.paused||s.locked||s.displayOff;
                if(!paused)elapsed+=dt;
                bool active=false;RECT all=bounds(s.monitors);auto frameStart=now;auto cpuStart=threadCpu();std::vector<std::wstring> used,advanced;
                std::vector<OutputMetrics> measured(outputs.size());graphicsStats.fieldMs=graphicsStats.simulationMs=0;
                for(size_t i=0;i<outputs.size();i++){
                    const auto&m=s.monitors[i];auto assignment=s.config.displays.find(m.id);bool enabled=assignment==s.config.displays.end()||assignment->second.enabled;
                    if(!enabled){if(outputs[i]){context.current();outputs[i].reset();}continue;}
                    if(!outputs[i])outputs[i]=std::make_unique<Output>(gl,context,bridge.get(),m,shellHost());
                    auto&o=*outputs[i];
                    int scene=s.config.layout==Layout::Independent&&assignment!=s.config.displays.end()?assignment->second.scene:s.config.scene;
                    auto view=viewport(m,all,s.config.layout);std::wstring key=s.config.layout==Layout::Panorama?L"panorama":m.id;used.push_back(key);
                    if(paused||(s.config.pauseCovered&&covered(m.rect,s.opaqueWindows)))continue;
                    auto&world=worlds[key];if(!world||world->scene()!=scene){context.current();world.reset();world=std::make_unique<SceneRuntime>(gl,scene);}
                    bool first=std::find(advanced.begin(),advanced.end(),key)==advanced.end();
                    world->setGeometry(sceneGeometry(s.monitors,m,s.config.layout));
                    for(auto event:s.input)if(s.config.layout==Layout::Panorama?first:bool(PtInRect(&m.rect,event.point))){
                        bool blocked=!event.desktop||!icons.ready();for(auto r:s.icons)if(PtInRect(&r,event.point)){blocked=true;break;}
                        V2 point{float(event.point.x-(s.config.layout==Layout::Panorama?all.left:m.rect.left)),float(event.point.y-(s.config.layout==Layout::Panorama?all.top:m.rect.top))};
                        world->pointer(blocked?PointerAction::Cancel:event.action,point);
                    }
                    else if(event.action==PointerAction::Up||event.action==PointerAction::Cancel)world->pointer(PointerAction::Cancel,{});
                    bool iconHit=false;for(auto r:s.icons)if(PtInRect(&r,s.pointer)){iconHit=true;break;}
                    if(s.click&&s.desktopPointer&&!iconHit&&(s.config.layout==Layout::Panorama?first:bool(PtInRect(&m.rect,s.pointer)))){float x=float(s.pointer.x-(s.config.layout==Layout::Panorama?all.left:m.rect.left)),y=float(s.pointer.y-(s.config.layout==Layout::Panorama?all.top:m.rect.top));world->click(x,y);}
                    o.draw(context,s,view,*world,noise,post,elapsed,first?dt:0.f,state->stop);measured[i]=o.metrics(true);advanced.push_back(key);active=true;
                }
                // Hidden/disabled scenes release after they cease to be assigned;
                // paused assigned scenes keep only the resources needed to resume.
                if(!paused){context.current();for(auto it=worlds.begin();it!=worlds.end();)if(std::find(used.begin(),used.end(),it->first)==used.end())it=worlds.erase(it);else ++it;}
                {size_t gpu=0,targets=0,activeOutputs=0;uint64_t bytes=0,readback=0;double gpuMs=0,presentMs=0,readbackMs=0;bool timed=true;size_t rendered=0;
                    for(size_t i=0;i<outputs.size();i++){const auto&o=outputs[i];if(!o){measured[i].monitor=s.monitors[i].id;continue;}activeOutputs++;if(o->gpu())gpu++;targets+=o->targets();bytes+=o->surfaceBytes();if(measured[i].monitor.empty())measured[i]=o->metrics(false);
                        readback+=measured[i].readbackBytes;presentMs+=measured[i].presentationMs;readbackMs+=measured[i].readbackMs;if(measured[i].cpuMs>0){rendered++;if(measured[i].gpuMs>=0)gpuMs+=measured[i].gpuMs;else timed=false;}}
                    sampleFrames+=active?1:0;sampleReadback+=readback;sampleCpu+=threadCpu()-cpuStart;const double seconds=std::chrono::duration<double>(SteadyClock::now()-sampleStart).count();
                    std::lock_guard<std::mutex> lock(state->mutex);auto&m=state->metrics;
                    m.presentation=!activeOutputs?"Inactive":targets==activeOutputs?"Not initialized":gpu==activeOutputs&&gpu?"GPU DirectComposition":gpu?"Mixed GPU / asynchronous DIB fallback":"Asynchronous bounded DIB fallback";
                    const char*renderer=reinterpret_cast<const char*>(glGetString(GL_RENDERER));m.adapter=bridge&&gpu?bridge->adapter():renderer?std::wstring(renderer,renderer+strlen(renderer)):L"unavailable";
                    m.running=true;m.paused=!active;m.surfaces=activeOutputs;m.scenes=worlds.size();m.targets=targets;m.contexts=1;m.devices=bridge?1:0;if(active)m.frames++;
                    m.lastFrameMs=std::chrono::duration<double,std::milli>(SteadyClock::now()-frameStart).count();
                    m.gpuMs=timed&&rendered?gpuMs:-1;m.presentationMs=presentMs;m.readbackMs=readbackMs;m.textureBytes=graphicsStats.textureBytes;m.textures=graphicsStats.textures;m.surfaceBytes=bytes;m.uploadBytes=graphicsStats.uploadBytes;
                    m.shaderMs=graphicsStats.shaderMs;m.fieldMs=graphicsStats.fieldMs;m.simulationMs=graphicsStats.simulationMs;m.fpsTarget=s.config.fps;m.outputs=std::move(measured);
                    if(!active){m.fps=0;m.readbackPerSecond=0;sampleStart=SteadyClock::now();sampleFrames=sampleReadback=sampleCpu=0;m.renderThreadCpuMs=0;}
                    else if(seconds>=1){m.fps=sampleFrames/seconds;m.readbackPerSecond=sampleReadback/seconds;m.renderThreadCpuMs=sampleFrames?double(sampleCpu)/sampleFrames/10000.:0;sampleStart=SteadyClock::now();sampleFrames=sampleReadback=sampleCpu=0;}}
                if(!active){MsgWaitForMultipleObjects(1,&state->event,FALSE,INFINITE,QS_ALLINPUT);last=std::chrono::steady_clock::now();deadline=last;continue;}
                deadline+=std::chrono::microseconds(1000000/s.config.fps);now=std::chrono::steady_clock::now();if(deadline<now){std::lock_guard<std::mutex> lock(state->mutex);state->metrics.missed++;deadline=now;}
                DWORD wait=DWORD(std::max<int64_t>(0,std::chrono::duration_cast<std::chrono::milliseconds>(deadline-now).count()));
                // Input/configuration can wake the wait without allowing a frame
                // before its deadline. No artificial sleeps or UI thread pacing.
                while(!state->stop&&std::chrono::steady_clock::now()<deadline){MsgWaitForMultipleObjects(1,&state->event,FALSE,wait,QS_ALLINPUT);MSG msg;while(PeekMessageW(&msg,nullptr,0,0,PM_REMOVE)){TranslateMessage(&msg);DispatchMessageW(&msg);}now=std::chrono::steady_clock::now();wait=DWORD(std::max<int64_t>(1,std::chrono::duration_cast<std::chrono::milliseconds>(deadline-now).count()));}
            }
            context.current();worlds.clear();outputs.clear();
        }catch(const std::exception&e){std::lock_guard<std::mutex> lock(state->mutex);state->metrics.error=e.what();}
        if(SUCCEEDED(com))CoUninitialize();{std::lock_guard<std::mutex> lock(state->mutex);state->metrics.running=false;state->metrics.surfaces=0;state->metrics.scenes=state->metrics.targets=state->metrics.contexts=state->metrics.devices=0;state->metrics.textureBytes=state->metrics.surfaceBytes=state->metrics.textures=0;state->metrics.fps=state->metrics.readbackPerSecond=0;state->metrics.outputs.clear();}state->finished=true;SetEvent(state->event);
    }
public:
    explicit Session(const Snapshot&s){if(!state->event)throw std::runtime_error("Wallpaper cancellation event unavailable");state->latest=s;thread=std::thread(run,state);}
    ~Session(){stop();if(thread.joinable())thread.join();}Session(const Session&)=delete;
    void publish(Snapshot s){std::lock_guard<std::mutex> lock(state->mutex);s.click=s.click||state->latest.click;
        auto pending=std::move(state->latest.input);pending.insert(pending.end(),s.input.begin(),s.input.end());
        if(pending.size()>32)pending={{POINT{},PointerAction::Cancel,false}};s.input=std::move(pending);
        state->latest=std::move(s);state->revision++;SetEvent(state->event);}
    void stop(){state->stop=true;SetEvent(state->event);}bool finished()const{return state->finished.load();}
    Metrics metrics(){std::lock_guard<std::mutex> lock(state->mutex);return state->metrics;}
};
}
