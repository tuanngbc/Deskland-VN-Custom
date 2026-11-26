// Hosted-only recovery prerequisite. Does not call SetSystemCursor, SetCursor,
// SPI_SETCURSORS or modify cursor settings. Only synthetic ANI pixels export.
#include <windows.h>
#include <vector>
#include <array>
#include <string>
#include <cstring>
#include <cstdio>
namespace {
using Bytes=std::vector<BYTE>;
void u16(Bytes& b,WORD n){b.push_back(BYTE(n));b.push_back(BYTE(n>>8));}
void u32(Bytes& b,DWORD n){for(int i=0;i<4;++i)b.push_back(BYTE(n>>(i*8)));}
void tag(Bytes& b,const char* t){for(int i=0;i<4;++i)b.push_back(BYTE(t[i]));}
void chunk(Bytes& b,const char* name,const Bytes& content){tag(b,name);u32(b,DWORD(content.size()));b.insert(b.end(),content.begin(),content.end());if(content.size()%2)b.push_back(0);}
Bytes cursor(DWORD color){
    Bytes b;u16(b,0);u16(b,2);u16(b,1);b.insert(b.end(),{16,16,0,0});u16(b,3);u16(b,4);u32(b,40+16*16*4+4*16);u32(b,22);
    u32(b,40);u32(b,16);u32(b,32);u16(b,1);u16(b,32);u32(b,BI_RGB);u32(b,16*16*4+4*16);u32(b,0);u32(b,0);u32(b,0);u32(b,0);
    for(int i=0;i<16*16;++i)u32(b,color|0xff000000);b.resize(b.size()+4*16,0);return b;
}
Bytes animation(){
    Bytes header;for(DWORD n:{36u,2u,2u,16u,16u,32u,1u,8u,1u})u32(header,n);
    Bytes frames;tag(frames,"fram");chunk(frames,"icon",cursor(0x00ff0000));chunk(frames,"icon",cursor(0x000000ff));
    Bytes body;tag(body,"ACON");chunk(body,"anih",header);chunk(body,"LIST",frames);
    Bytes result;tag(result,"RIFF");u32(result,DWORD(body.size()));result.insert(result.end(),body.begin(),body.end());return result;
}
struct File{std::wstring path;~File(){if(!path.empty())DeleteFileW(path.c_str());}};
struct Cursor{HCURSOR h=nullptr;~Cursor(){if(h)DestroyCursor(h);}Cursor()=default;Cursor(const Cursor&)=delete;};
struct BitmapInfo{ICONINFO value{};~BitmapInfo(){if(value.hbmColor)DeleteObject(value.hbmColor);if(value.hbmMask)DeleteObject(value.hbmMask);}};
bool frame(HCURSOR cursor,UINT step,Bytes& pixels){
    HDC dc=CreateCompatibleDC(nullptr);if(!dc)return false;
    BITMAPINFO info{};info.bmiHeader.biSize=sizeof(BITMAPINFOHEADER);info.bmiHeader.biWidth=32;info.bmiHeader.biHeight=-32;info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;
    void* raw=nullptr;HBITMAP bitmap=CreateDIBSection(dc,&info,DIB_RGB_COLORS,&raw,nullptr,0);if(!bitmap){DeleteDC(dc);return false;}
    auto old=SelectObject(dc,bitmap);std::memset(raw,0,32*32*4);const bool drawn=DrawIconEx(dc,0,0,cursor,32,32,step,nullptr,DI_NORMAL)!=FALSE;GdiFlush();
    if(drawn)pixels.assign(static_cast<BYTE*>(raw),static_cast<BYTE*>(raw)+32*32*4);
    SelectObject(dc,old);DeleteObject(bitmap);DeleteDC(dc);return drawn;
}
bool sameHotspot(HCURSOR a,HCURSOR b){BitmapInfo x,y;return GetIconInfo(a,&x.value)&&GetIconInfo(b,&y.value)&&!x.value.fIcon&&!y.value.fIcon&&x.value.xHotspot==y.value.xHotspot&&x.value.yHotspot==y.value.yHotspot;}
}
extern "C" int tlauncher_cursor_recovery_copy_fixture()noexcept{
    wchar_t hosted[8]{};if(GetEnvironmentVariableW(L"GITHUB_ACTIONS",hosted,8)!=4||wcscmp(hosted,L"true"))return -1;
    try{
        wchar_t directory[MAX_PATH]{},name[MAX_PATH]{};if(!GetTempPathW(MAX_PATH,directory)||!GetTempFileNameW(directory,L"cur",0,name))return 1;
        File reservation;reservation.path=name;
        File file;file.path=reservation.path+L".ani";auto bytes=animation();
        // LoadCursorFromFile may select its decoder by extension. Keep the
        // reserved temporary file until the uniquely named ANI is closed.
        HANDLE out=CreateFileW(file.path.c_str(),GENERIC_WRITE,0,nullptr,CREATE_NEW,FILE_ATTRIBUTE_TEMPORARY,nullptr);if(out==INVALID_HANDLE_VALUE){file.path.clear();return 2;}
        DWORD written=0;const bool saved=WriteFile(out,bytes.data(),DWORD(bytes.size()),&written,nullptr)&&written==bytes.size();CloseHandle(out);if(!saved)return 3;
        // LoadCursorFromFile returns a shared cursor according to
        // DestroyCursor's contract. Explicitly omit LR_SHARED so this source
        // has a private lifetime that the fixture is permitted to destroy.
        Cursor original;original.h=static_cast<HCURSOR>(LoadImageW(nullptr,file.path.c_str(),IMAGE_CURSOR,0,0,LR_LOADFROMFILE));if(!original.h)return 4;
        // CopyCursor/CopyIcon is explicitly unsuitable for ANI. CopyImage with
        // no RETURNORG/FROMRESOURCE/DELETEORG flags must create a private copy.
        // https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-copycursor
        Cursor copy;copy.h=static_cast<HCURSOR>(CopyImage(original.h,IMAGE_CURSOR,0,0,0));if(!copy.h||copy.h==original.h)return 5;
        if(!sameHotspot(original.h,copy.h))return 6;
        std::array<Bytes,2> before,after;
        for(UINT i=0;i<2;++i)if(!frame(original.h,i,before[i]))return 7;
        if(before[0]==before[1])return 8;
        bool exactCopy=true;
        for(UINT i=0;i<2;++i){if(!frame(copy.h,i,after[i]))return 14;if(after[i]!=before[i])exactCopy=false;}
        const bool copiedFramesDistinct=after[0]!=after[1];
        // Retain an independent file-loaded original too. This preserves the
        // whole ANI rather than assuming CopyImage retains its frame table.
        Cursor reload;reload.h=static_cast<HCURSOR>(LoadImageW(nullptr,file.path.c_str(),IMAGE_CURSOR,0,0,LR_LOADFROMFILE));
        if(!reload.h||reload.h==original.h||!sameHotspot(original.h,reload.h))return 16;
        for(UINT i=0;i<2;++i){Bytes pixels;if(!frame(reload.h,i,pixels)||pixels!=before[i])return 17;}
        if(!DestroyCursor(original.h))return 13;original.h=nullptr;
        for(UINT i=0;i<2;++i){
            Bytes pixels;if(!frame(reload.h,i,pixels)||pixels!=before[i])return 18;
            if(exactCopy&&(!frame(copy.h,i,after[i])||after[i]!=before[i]))return 9;
        }
        // Standard cursor handles stay shared and untouched; only independent
        // copies and the bitmaps returned by GetIconInfo are destroyed.
        for(WORD id:{WORD(32512),WORD(32514),WORD(32650),WORD(32513),WORD(32649)}){
            const auto shared=LoadCursorW(nullptr,MAKEINTRESOURCEW(id));if(!shared)return 10;
            Cursor snapshot;snapshot.h=static_cast<HCURSOR>(CopyImage(shared,IMAGE_CURSOR,0,0,0));if(!snapshot.h||snapshot.h==shared||!sameHotspot(shared,snapshot.h))return 11;
        }
        std::printf("CURSOR_RECOVERY CopyImage_exact=%d copied_frames_distinct=%d file_reload_both_frames=1 private_source_destroyed=1 global_writes=0\n",int(exactCopy),int(copiedFramesDistinct));
        // 100 is an observed lossy CopyImage rejected as a backup, not parity.
        return exactCopy?0:100;
    }catch(...){return 12;}
}
