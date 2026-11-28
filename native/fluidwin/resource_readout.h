#pragma once
#include <winsock2.h>
#include <windows.h>
#include <iphlpapi.h>
#include <pdh.h>
#include <algorithm>
#include <atomic>
#include <cmath>
#include <memory>
#include <mutex>
#include <string>
#include <unordered_map>
#include <vector>

namespace resource_readout {
struct Values {
    int cpu=-1, ram=-1, gpu=-1;
    double down=-1, up=-1;
};
struct Counters {
    struct Bytes { ULONG64 down,up; };
    std::unordered_map<ULONG64,Bytes> network;
    ULONGLONG time=0,idle=0,total=0;
    PDH_HQUERY query=nullptr;
    PDH_HCOUNTER gpu=nullptr;
    bool initialized=false;
    ~Counters(){if(query)PdhCloseQuery(query);}
    static ULONGLONG ticks(FILETIME v){return (ULONGLONG(v.dwHighDateTime)<<32)|v.dwLowDateTime;}
    Values sample(){
        Values value;
        MEMORYSTATUSEX memory{sizeof(memory)};
        if(GlobalMemoryStatusEx(&memory))value.ram=int(memory.dwMemoryLoad);
        FILETIME i{},k{},u{};
        if(GetSystemTimes(&i,&k,&u)){
            auto nowIdle=ticks(i),nowTotal=ticks(k)+ticks(u);
            if(total&&nowTotal>total&&nowIdle>=idle){
                auto elapsed=nowTotal-total,unused=std::min(elapsed,nowIdle-idle);
                value.cpu=int(std::lround(100.0*(elapsed-unused)/elapsed));
            }
            idle=nowIdle;total=nowTotal;
        }
        MIB_IF_TABLE2* table=nullptr;
        const auto now=GetTickCount64();
        if(GetIfTable2(&table)==NO_ERROR&&table){
            const auto freeTable=[](MIB_IF_TABLE2* p){FreeMibTable(p);};
            const std::unique_ptr<MIB_IF_TABLE2,decltype(freeTable)> owned(table,freeTable);
            decltype(network) next;
            ULONGLONG down=0,up=0;
            for(ULONG j=0;j<table->NumEntries;++j){
                const auto& row=table->Table[j];
                if(row.OperStatus!=IfOperStatusUp||!row.InterfaceAndOperStatusFlags.HardwareInterface||row.Type==IF_TYPE_SOFTWARE_LOOPBACK)continue;
                auto id=row.InterfaceLuid.Value;
                next[id]={row.InOctets,row.OutOctets};
                auto old=network.find(id);
                if(old==network.end())continue;
                if(row.InOctets>=old->second.down)down+=row.InOctets-old->second.down;
                if(row.OutOctets>=old->second.up)up+=row.OutOctets-old->second.up;
            }
            if(time&&now>time){value.down=1000.0*down/(now-time);value.up=1000.0*up/(now-time);}
            network=std::move(next);time=now;
        }
        if(!initialized){
            initialized=true;
            if(PdhOpenQueryW(nullptr,0,&query)==ERROR_SUCCESS&&
               PdhAddEnglishCounterW(query,L"\\GPU Engine(*)\\Utilization Percentage",0,&gpu)!=ERROR_SUCCESS)gpu=nullptr;
        }
        if(!gpu||PdhCollectQueryData(query)!=ERROR_SUCCESS)return value;
        DWORD bytes=0,count=0;
        PdhGetFormattedCounterArrayW(gpu,PDH_FMT_DOUBLE,&bytes,&count,nullptr);
        if(!bytes||bytes>4*1024*1024)return value;
        std::vector<ULONG_PTR> buffer((bytes+sizeof(ULONG_PTR)-1)/sizeof(ULONG_PTR));
        auto* entries=reinterpret_cast<PDH_FMT_COUNTERVALUE_ITEM_W*>(buffer.data());
        if(PdhGetFormattedCounterArrayW(gpu,PDH_FMT_DOUBLE,&bytes,&count,entries)!=ERROR_SUCCESS||
           count>bytes/sizeof(*entries))return value;
        std::unordered_map<std::wstring,double> engines;
        for(DWORD j=0;j<count;++j){
            const auto& entry=entries[j];
            const double n=entry.FmtValue.doubleValue;
            if(!entry.szName||entry.FmtValue.CStatus>1||!std::isfinite(n)||n<0)continue;
            std::wstring name(entry.szName);
            const auto first=name.find(L"luid_");
            if(first==name.npos)continue;
            const auto end=name.find(L"_engtype_",first);
            engines[name.substr(first,end==name.npos?end:end-first)]+=n;
        }
        for(const auto& engine:engines)value.gpu=std::max(value.gpu,int(std::lround(std::clamp(engine.second,0.0,100.0))));
        return value;
    }
};

class Readout {
    struct Session {
        Counters counters;
        std::mutex mutex;
        Values values;
        bool ready=false;
    };
    inline static std::atomic<bool> busy{false};
    std::shared_ptr<Session> session;
    ULONGLONG next=0;
    Values current;
    static void CALLBACK work(PTP_CALLBACK_INSTANCE,void* ptr){
        std::unique_ptr<std::shared_ptr<Session>> request(static_cast<std::shared_ptr<Session>*>(ptr));
        try {
            auto value=(*request)->counters.sample();
            std::lock_guard<std::mutex> guard((*request)->mutex);
            (*request)->values=value;(*request)->ready=true;
        } catch(...) {}
        busy.store(false);
    }
public:
    bool update() {
        const auto now=GetTickCount64();
        if(!session)session=std::make_shared<Session>();
        bool changed=false;
        {
            std::lock_guard<std::mutex> guard(session->mutex);
            if(session->ready){current=session->values;session->ready=false;changed=true;}
        }
        if(now>=next&&!busy.load()){
            auto request=std::make_unique<std::shared_ptr<Session>>(session);
            if(!busy.exchange(true)){
                auto* raw=request.release();
                if(!TrySubmitThreadpoolCallback(work,raw,nullptr)){delete raw;busy.store(false);}
                next=now+2000;
            }
        }
        return changed;
    }
    void stop(){session.reset();next=0;current={};}
    const Values& values()const{return current;}
};
}