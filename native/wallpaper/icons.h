// Desktop icon geometry only; bounded Shell queries, no icon/content capture.
#pragma once
#include "model.h"
#include <commctrl.h>
#include <memory>

namespace wallpaper {
inline HWND desktopList(){
    HWND list=nullptr;EnumWindows([](HWND h,LPARAM p)->BOOL{HWND view=FindWindowExW(h,nullptr,L"SHELLDLL_DefView",nullptr);if(!view)return TRUE;*reinterpret_cast<HWND*>(p)=FindWindowExW(view,nullptr,L"SysListView32",nullptr);return FALSE;},reinterpret_cast<LPARAM>(&list));return list;
}
class IconGeometry {
    HWND list=nullptr;HANDLE process=nullptr;void*scratch=nullptr;DWORD pid=0;std::vector<RECT> snapshot;uint64_t next=0;
    void close(){if(scratch&&process)VirtualFreeEx(process,scratch,0,MEM_RELEASE);if(process)CloseHandle(process);process=nullptr;scratch=nullptr;list=nullptr;pid=0;}
public:
    ~IconGeometry(){close();}
    const std::vector<RECT>& poll(const std::atomic<bool>&stop){
        if(GetTickCount64()<next)return snapshot;next=GetTickCount64()+1000;
        HWND current=desktopList();DWORD owner=0;if(current)GetWindowThreadProcessId(current,&owner);
        if(current!=list||owner!=pid||!process||WaitForSingleObject(process,0)!=WAIT_TIMEOUT){close();list=current;pid=owner;
            if(!pid)return snapshot;process=OpenProcess(PROCESS_VM_OPERATION|PROCESS_VM_READ|PROCESS_VM_WRITE|SYNCHRONIZE,FALSE,pid);if(!process)return snapshot;scratch=VirtualAllocEx(process,nullptr,sizeof(RECT),MEM_COMMIT|MEM_RESERVE,PAGE_READWRITE);if(!scratch){close();return snapshot;}}
        DWORD_PTR count=0;if(!SendMessageTimeoutW(list,LVM_GETITEMCOUNT,0,0,SMTO_ABORTIFHUNG|SMTO_BLOCK,20,&count))return snapshot;
        std::vector<RECT> fresh;uint64_t start=GetTickCount64();
        for(size_t i=0;i<std::min<DWORD_PTR>(count,512)&&!stop;i++){
            if(GetTickCount64()-start>100)return snapshot;RECT r{LVIR_BOUNDS,0,0,0};SIZE_T done=0;
            if(!WriteProcessMemory(process,scratch,&r,sizeof(r),&done)||done!=sizeof(r))return snapshot;
            DWORD_PTR ok=0;if(!SendMessageTimeoutW(list,LVM_GETITEMRECT,i,reinterpret_cast<LPARAM>(scratch),SMTO_ABORTIFHUNG|SMTO_BLOCK,20,&ok)||!ok)return snapshot;
            if(!ReadProcessMemory(process,scratch,&r,sizeof(r),&done)||done!=sizeof(r))return snapshot;
            MapWindowPoints(list,nullptr,reinterpret_cast<POINT*>(&r),2);if(r.right>r.left&&r.bottom>r.top)fresh.push_back(r);
        }snapshot=std::move(fresh);return snapshot;
    }
};
}
