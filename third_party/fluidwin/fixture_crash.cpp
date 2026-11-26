// Explicit hosted-fixture diagnostics. Never installed by the application.
#include <windows.h>
#include <dbghelp.h>
#include <cstdio>
#include <atomic>
#pragma comment(lib, "dbghelp.lib")

namespace {
std::atomic_flag captured=ATOMIC_FLAG_INIT;
LONG CALLBACK capture_fault(EXCEPTION_POINTERS* fault) {
    if(fault->ExceptionRecord->ExceptionCode!=EXCEPTION_ACCESS_VIOLATION || captured.test_and_set())
        return EXCEPTION_CONTINUE_SEARCH;
    wchar_t root[32768]{};
    const DWORD n=GetEnvironmentVariableW(L"TUANPC_DESK_STAGE_EVIDENCE",root,32768);
    if(!n || n>32000)return EXCEPTION_CONTINUE_SEARCH;
    wchar_t path[32768]{};
    swprintf_s(path,L"%s\\native-fault-%lu.dmp",root,GetCurrentProcessId());
    HANDLE file=CreateFileW(path,GENERIC_WRITE,0,nullptr,CREATE_ALWAYS,FILE_ATTRIBUTE_NORMAL,nullptr);
    if(file!=INVALID_HANDLE_VALUE){
        MINIDUMP_EXCEPTION_INFORMATION info{GetCurrentThreadId(),fault,FALSE};
        MiniDumpWriteDump(GetCurrentProcess(),GetCurrentProcessId(),file,
            MiniDumpWithThreadInfo,&info,nullptr,nullptr);
        CloseHandle(file);
    }
    swprintf_s(path,L"%s\\native-fault-%lu.txt",root,GetCurrentProcessId());
    FILE* out=nullptr;_wfopen_s(&out,path,L"w");
    if(!out)return EXCEPTION_CONTINUE_SEARCH;
    fprintf(out,"code=%08lx address=%p access=%llu target=%llx\n",
        fault->ExceptionRecord->ExceptionCode,fault->ExceptionRecord->ExceptionAddress,
        fault->ExceptionRecord->ExceptionInformation[0],fault->ExceptionRecord->ExceptionInformation[1]);
    SymSetOptions(SYMOPT_UNDNAME|SYMOPT_DEFERRED_LOADS|SYMOPT_LOAD_LINES);
    const HANDLE process=GetCurrentProcess(),thread=GetCurrentThread();
    SymInitialize(process,nullptr,TRUE);
    CONTEXT context=*fault->ContextRecord;
    STACKFRAME64 frame{};
    frame.AddrPC.Offset=context.Rip;frame.AddrStack.Offset=context.Rsp;frame.AddrFrame.Offset=context.Rbp;
    frame.AddrPC.Mode=frame.AddrStack.Mode=frame.AddrFrame.Mode=AddrModeFlat;
    for(unsigned i=0;i<48 && frame.AddrPC.Offset;i++){
        DWORD64 displacement=0;char buffer[sizeof(SYMBOL_INFO)+MAX_SYM_NAME]{};
        auto symbol=reinterpret_cast<SYMBOL_INFO*>(buffer);symbol->SizeOfStruct=sizeof(SYMBOL_INFO);symbol->MaxNameLen=MAX_SYM_NAME;
        const bool named=SymFromAddr(process,frame.AddrPC.Offset,&displacement,symbol)!=FALSE;
        HMODULE module=nullptr;wchar_t modulePath[MAX_PATH]{};
        GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS|GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            reinterpret_cast<LPCWSTR>(frame.AddrPC.Offset),&module);
        GetModuleFileNameW(module,modulePath,MAX_PATH);
        fprintf(out,"%02u %llx %ls+%llx %s+%llx\n",i,frame.AddrPC.Offset,modulePath,
            frame.AddrPC.Offset-reinterpret_cast<DWORD64>(module),named?symbol->Name:"?",displacement);
        if(!StackWalk64(IMAGE_FILE_MACHINE_AMD64,process,thread,&frame,&context,nullptr,
            SymFunctionTableAccess64,SymGetModuleBase64,nullptr))break;
    }
    fclose(out);
    return EXCEPTION_CONTINUE_SEARCH;
}
}
extern "C" void tlauncher_fixture_native_diagnostics(){
    wchar_t hosted[8]{};
    if(GetEnvironmentVariableW(L"GITHUB_ACTIONS",hosted,8) && wcscmp(hosted,L"true")==0)
        AddVectoredExceptionHandler(1,capture_fault);
}
