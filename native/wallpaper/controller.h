// Core-thread integration boundary: snapshots in, diagnostics out. No window hooks.
#pragma once
#include "store.h"
#include "session.h"
#include "../fluidwin/pool.h"

namespace wallpaper {
class Persistence {
    struct State {
        std::mutex mutex;Store store;Config pending;bool active=false;uint64_t revision=0;
        std::atomic<bool> failed{false};HANDLE done=CreateEventW(nullptr,TRUE,TRUE,nullptr);
        ~State(){if(done)CloseHandle(done);}
    };
    std::shared_ptr<State> state=std::make_shared<State>();
public:
    Config load(){return state->store.load();}
    void save(const Config&c){auto s=state;{std::lock_guard<std::mutex> lock(s->mutex);s->pending=c;s->revision++;if(s->active)return;s->active=true;ResetEvent(s->done);}
        auto*job=new std::shared_ptr<State>(s);
        bool queued=pool::lancia([](void*raw)->DWORD {
            auto owner=std::unique_ptr<std::shared_ptr<State>>(static_cast<std::shared_ptr<State>*>(raw));auto s=*owner;
            for(;;){Config config;uint64_t revision;{std::lock_guard<std::mutex> lock(s->mutex);config=s->pending;revision=s->revision;}
                try{s->failed=!s->store.save(config);}catch(...){s->failed=true;}
                std::lock_guard<std::mutex> lock(s->mutex);if(revision==s->revision){s->active=false;SetEvent(s->done);break;}
            }return 0;
        },job,THREAD_PRIORITY_BELOW_NORMAL);
        if(!queued){delete job;std::lock_guard<std::mutex> lock(s->mutex);s->active=false;s->failed=true;SetEvent(s->done);}
    }
    bool failed()const{return state->failed.load();}
    bool finish(DWORD ms){return WaitForSingleObject(state->done,ms)==WAIT_OBJECT_0;}
};
class Controller {
    Persistence persistence;Config config;std::unique_ptr<Session> session;Metrics last;
    Snapshot current;uint64_t retryAfter=0;bool stopping=false;
public:
    Controller(){config=persistence.load();current.config=config;}
    const Config& settings()const{return config;}
    void change(Config c){validate(c);config=std::move(c);persistence.save(config);retryAfter=0;current.config=config;tick(current);}
    void tick(Snapshot s){
        current=std::move(s);current.config=config;
        if(session&&session->finished()){last=session->metrics();session.reset();retryAfter=GetTickCount64()+10000;}
        if(stopping||!config.enabled){if(session)session->stop();return;}
        if(!session&&GetTickCount64()>=retryAfter){try{session=std::make_unique<Session>(current);}catch(const std::exception&e){last.error=e.what();retryAfter=GetTickCount64()+10000;}}
        else if(session)session->publish(current);
    }
    void clicked(POINT point,bool desktop){if(!config.enabled)return;current.pointer=point;current.desktopPointer=desktop;current.click=true;if(session)session->publish(current);current.click=false;}
    void stop(){stopping=true;if(session)session->stop();}
    bool finish(DWORD ms){stop();auto until=GetTickCount64()+ms;while(session&&!session->finished()&&GetTickCount64()<until)WaitForSingleObject(GetCurrentProcess(),10);if(session&&session->finished())session.reset();return !session&&persistence.finish(DWORD(std::max<int64_t>(0,int64_t(until-GetTickCount64()))));}
    Metrics metrics(){auto m=session?session->metrics():last;if(persistence.failed())m.error="Wallpaper settings could not be saved; previous file retained";return m;}
    bool enabled()const{return config.enabled;}
};
}
