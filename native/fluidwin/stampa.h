// PrintWindow con un limite di tempo. PrintWindow manda un messaggio alla finestra e aspetta la sua risposta senza
// limite: con un'app bloccata o lentissima fermerebbe tutto FluidWin (dock, capsule, animazioni). La fa un thread a
// parte; se entro 'limite' ms non ha finito si rinuncia, e il thread finisce da solo quando l'app risponde,
// liberando tutto lui. Con un'app che Windows da' gia' per bloccata ("Non risponde") non si prova nemmeno.
#pragma once
#include "tlauncher_identity.h"
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include "pool.h"
#include <algorithm>
#include <new>
#include <vector>
#include <memory>
#include <mutex>
#include <unordered_map>
#include <atomic>
#include <cstdint>

namespace stampa {

// Shared, bounded last-valid images from the accepted capture/motion pipeline.
// Retention exists only while Stage requests it; no capture session is added.
struct SavedImage {DWORD pid=0,tid=0;uint64_t generation=0;int w=0,h=0;ULONGLONG at=0;std::vector<UINT> pixels;};
inline std::atomic<bool> keepSnapshots{false};inline std::mutex savedMutex;
inline std::unordered_map<HWND,std::shared_ptr<const SavedImage>> saved;
inline std::unordered_map<HWND,uint64_t> generations;inline uint64_t nextGeneration=0;
inline uint64_t generation(HWND h){std::lock_guard<std::mutex> lock(savedMutex);auto& id=generations[h];if(!id)id=++nextGeneration;return id;}
inline void retain(bool enabled){keepSnapshots=enabled;if(!enabled){std::lock_guard<std::mutex> lock(savedMutex);saved.clear();generations.clear();}}
inline void forget(HWND h){std::lock_guard<std::mutex> lock(savedMutex);saved.erase(h);generations.erase(h);}
inline std::shared_ptr<const SavedImage> lastImage(HWND h){
    std::lock_guard<std::mutex> lock(savedMutex);auto i=saved.find(h);return i==saved.end()?nullptr:i->second;
}
inline void remember(HWND hwnd,DWORD pid,DWORD tid,uint64_t token,int w,int h,const UINT* pixels){
    if(!keepSnapshots||!pixels||w<=0||h<=0||!token)return;
    DWORD current=0;if(GetWindowThreadProcessId(hwnd,&current)!=tid||pid!=current)return;
    auto image=std::make_shared<SavedImage>();image->pid=pid;image->tid=tid;image->generation=token;image->at=GetTickCount64();
    const double scale=std::min({1.,1280./w,800./h});image->w=std::max(1,int(w*scale));image->h=std::max(1,int(h*scale));image->pixels.resize(size_t(image->w)*image->h);
    bool colored=false;for(int y=0;y<image->h;y++)for(int x=0;x<image->w;x++){UINT c=pixels[size_t(y*h/image->h)*w+x*w/image->w];image->pixels[size_t(y)*image->w+x]=c|0xff000000;if(c&0xffffff)colored=true;}
    if(!colored)return;
    std::lock_guard<std::mutex> lock(savedMutex);auto id=generations.find(hwnd);if(!keepSnapshots||id==generations.end()||id->second!=token)return;saved[hwnd]=std::move(image);
    size_t bytes=0;for(const auto& v:saved)bytes+=v.second->pixels.size()*4;
    while(bytes>16*1024*1024&&!saved.empty()){auto old=std::min_element(saved.begin(),saved.end(),[](auto&a,auto&b){return a.second->at<b.second->at;});bytes-=old->second->pixels.size()*4;saved.erase(old);}
}
struct Lavoro { HWND h; int w, hh; std::vector<UINT> px; BOOL ok; HANDLE fatto; LONG rif; DWORD pid=0,tid=0;uint64_t generation=0; };

// Finestre con una foto ancora in corso (thread fermo dentro PrintWindow perche' l'app non risponde): finche' non
// finisce non se ne avvia un'altra, altrimenti thread e memoria (una DIB grande quanto la finestra per ognuno) si
// accumulerebbero. (Mai liberato: un thread fermo puo' finire anche durante la chiusura di FluidWin)
struct InCorso { CRITICAL_SECTION cs; std::vector<HWND> h; InCorso() { InitializeCriticalSection(&cs); } };
inline InCorso& inCorso() { static InCorso* i = new InCorso; return *i; }
inline bool prenota(HWND h) {
    InCorso& c = inCorso(); EnterCriticalSection(&c.cs);
    const bool libera = std::find(c.h.begin(), c.h.end(), h) == c.h.end();
    if (libera) c.h.push_back(h);
    LeaveCriticalSection(&c.cs);
    return libera;
}
inline void libera(HWND h) {
    InCorso& c = inCorso(); EnterCriticalSection(&c.cs);
    c.h.erase(std::remove(c.h.begin(), c.h.end(), h), c.h.end());
    LeaveCriticalSection(&c.cs);
}

// chi finisce per ultimo (thread o chiamante) libera
inline void lascia(Lavoro* l) { if (InterlockedDecrement(&l->rif) == 0) { CloseHandle(l->fatto); delete l; } }

inline DWORD WINAPI thread(LPVOID p) {
    Lavoro* l = (Lavoro*)p;
    BITMAPINFO bi{}; bi.bmiHeader = {sizeof(BITMAPINFOHEADER), l->w, -l->hh, 1, 32, BI_RGB, 0, 0, 0, 0, 0};
    void* bits = nullptr;
    HDC scr = GetDC(nullptr), mem = CreateCompatibleDC(scr);
    HBITMAP dib = CreateDIBSection(scr, &bi, DIB_RGB_COLORS, &bits, nullptr, 0);
    if (dib && bits && mem) {
        HGDIOBJ vecchio = SelectObject(mem, dib);
        l->ok = PrintWindow(l->h, mem, PW_RENDERFULLCONTENT);
        GdiFlush();
        // (memoria finita: foto non riuscita invece di un'eccezione che chiuderebbe FluidWin, R20261121-001054-S18)
        if (l->ok) { try { l->px.assign((const UINT*)bits, (const UINT*)bits + (size_t)l->w * l->hh); } catch (const std::bad_alloc&) { l->ok = FALSE; l->px.clear(); } }
        SelectObject(mem, vecchio);
    }
    if (dib) DeleteObject(dib);
    if (mem) DeleteDC(mem);
    ReleaseDC(nullptr, scr);
    if(l->ok)try{remember(l->h,l->pid,l->tid,l->generation,l->w,l->hh,l->px.data());}catch(const std::bad_alloc&){}
    libera(l->h);
    SetEvent(l->fatto);
    lascia(l);
    return 0;
}

// Foto in sottofondo: parte adesso e non si aspetta (nullptr = non partita). Poi pronta() a ogni fotogramma e,
// quando lo e', prendi(); se non serve piu', abbandona(). Tutta la finestra (w x hh = GetWindowRect), BGRA
inline Lavoro* avvia(HWND h, int w, int hh, int priorita = THREAD_PRIORITY_NORMAL) {
    // (al massimo 8K x 8K: una finestra piu' grande e' un errore o un caso che non vale due copie in memoria)
    if (w <= 0 || hh <= 0 || (long long)w * hh > 7680LL * 4320 * 2 || !IsWindow(h) || IsHungAppWindow(h)) return nullptr;
    if (!prenota(h)) return nullptr;   // ce n'e' gia' una ferma su questa finestra
    Lavoro* l = new Lavoro{h, w, hh, {}, FALSE, CreateEventW(nullptr, TRUE, FALSE, nullptr), 2};
    l->tid=GetWindowThreadProcessId(h,&l->pid);if(keepSnapshots)l->generation=generation(h);
    // nel pool di thread (vedi pool.h): "lungo" perche' con un'app bloccata PrintWindow puo' non tornare per molto
    if (!l->fatto || !pool::lancia(thread, l, priorita, true)) { if (l->fatto) CloseHandle(l->fatto); delete l; libera(h); return nullptr; }
    return l;
}
inline bool pronta(Lavoro* l) { return l && WaitForSingleObject(l->fatto, 0) == WAIT_OBJECT_0; }
inline bool prendi(Lavoro* l, std::vector<UINT>& px) {
    if (!l) return false;
    const bool ok = pronta(l) && l->ok && l->px.size() == (size_t)l->w * l->hh;
    if (ok) px.swap(l->px);
    lascia(l);
    return ok;
}
inline void abbandona(Lavoro* l) { if (l) lascia(l); }   // il thread, se e' ancora fermo, libera lui quando finisce

// Tutta la finestra h (w x hh = GetWindowRect), pixel BGRA riga per riga. false: non riuscita o troppo lenta
inline bool finestra(HWND h, int w, int hh, std::vector<UINT>& px, DWORD limite = 300) {
    Lavoro* l = avvia(h, w, hh, GetThreadPriority(GetCurrentThread()));
    if (!l) return false;
    WaitForSingleObject(l->fatto, limite);
    return prendi(l, px);
}

}  // namespace stampa
