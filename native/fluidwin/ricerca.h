// Ricerca rapida (come Spotlight): Win+Tab apre una barra di vetro al centro dello schermo. Mentre si scrive mostra
// app, pagine delle impostazioni, file (dall'indice di ricerca di Windows, in un thread a parte), il risultato dei
// calcoli e la ricerca sul web. Frecce e Invio per scegliere, Esc per chiudere.
#pragma once
#include "tlauncher_identity.h"
#include "vetro.h"
#include "onda.h"
#include "indice.h"
#include "app_icons.h"
#include "app_organization.h"
#include "pool.h"
#include <memory>
#include <mutex>
#include "menu.h"
#include <windowsx.h>
#include <shlobj.h>
#include <shobjidl.h>
#include <structuredquerycondition.h>
#include <propsys.h>
#include <vector>
#include <deque>
#include <condition_variable>
#include <unordered_map>
#include <functional>
#include <cwctype>

namespace ricerca {

enum Tipo { T_CALCOLO, T_APP, T_IMPOSTAZIONE, T_FILE, T_WEB, T_FOLDER };

struct Risultato {
    Tipo tipo = T_APP;
    std::wstring titolo, sotto, azione, icona;   // icona: percorso per la shell ("" = simbolo disegnato)
};

// ---- calcolatrice: + - * / ^ % e parentesi, virgola o punto per i decimali ----
class Calcolo {
public:
    // Limiti (audit FW-AUDIT-027): il parser e' ricorsivo e il testo puo' arrivare incollato; qualche migliaio di
    // parentesi esauriva lo stack del thread principale (provato: 100.000 parentesi, uscita C00000FD). Un calcolo vero
    // sta comodo in 200 caratteri; ogni parentesi costa tre livelli (somma, potenza, segno): 150 = circa 50 parentesi
    static constexpr size_t LUNGO_MAX = 200;
    static constexpr int PROFONDO_MAX = 150;
    static bool valuta(const std::wstring& s, double& out) {
        if (s.size() > LUNGO_MAX) return false;
        bool cifra = false, operatore = false;
        for (wchar_t c : s) {
            if (iswdigit(c)) cifra = true;
            else if (wcschr(L"+-*/^%x()", c)) operatore = true;
            else if (c != L' ' && c != L',' && c != L'.') return false;
        }
        if (!cifra || !operatore) return false;
        Calcolo c(s);
        out = c.somma();
        c.spazi();
        return c.ok && c.p == c.s.size() && std::isfinite(out);
    }
private:
    std::wstring s; size_t p = 0; bool ok = true; int prof = 0;
    explicit Calcolo(const std::wstring& t) : s(t) {}
    void spazi() { while (p < s.size() && s[p] == L' ') p++; }
    // un livello di ricorsione in piu' (parentesi, segno, potenza): oltre il limite il calcolo non vale
    struct Livello {
        Calcolo& c; bool troppo;
        explicit Livello(Calcolo& k) : c(k), troppo(++k.prof > PROFONDO_MAX) { if (troppo) c.ok = false; }
        ~Livello() { c.prof--; }
        Livello(const Livello&) = delete; Livello& operator=(const Livello&) = delete;
    };
    double somma() {
        const Livello l(*this); if (l.troppo) return 0;
        double v = prodotto();
        for (;;) { spazi(); if (p < s.size() && (s[p] == L'+' || s[p] == L'-')) { const wchar_t o = s[p++]; const double b = prodotto(); v = o == L'+' ? v + b : v - b; } else return v; }
    }
    double prodotto() {
        double v = potenza();
        for (;;) {
            spazi();
            if (p < s.size() && (s[p] == L'*' || s[p] == L'x' || s[p] == L'/')) { const wchar_t o = s[p++]; const double b = potenza(); v = o == L'/' ? v / b : v * b; }
            else if (p < s.size() && s[p] == L'%') { p++; v /= 100; }
            else return v;
        }
    }
    double potenza() {
        const Livello l(*this); if (l.troppo) return 0;
        const double b = unario(); spazi(); if (p < s.size() && s[p] == L'^') { p++; return std::pow(b, potenza()); } return b;
    }
    double unario() {
        const Livello l(*this); if (l.troppo) return 0;
        spazi(); if (p < s.size() && s[p] == L'-') { p++; return -unario(); } if (p < s.size() && s[p] == L'+') { p++; return unario(); } return atomo();
    }
    double atomo() {
        spazi();
        if (p < s.size() && s[p] == L'(') { p++; const double v = somma(); spazi(); if (p < s.size() && s[p] == L')') p++; else ok = false; return v; }
        std::wstring n;
        while (p < s.size() && (iswdigit(s[p]) || s[p] == L'.' || s[p] == L',')) n += s[p] == L',' ? L'.' : s[p], p++;
        if (n.empty()) { ok = false; return 0; }
        // il numero va letto tutto: "1.2.3" non e' un numero (wcstod leggeva "1.2" e il resto spariva, FW-AUDIT-028)
        wchar_t* fine = nullptr;
        const double v = wcstod(n.c_str(), &fine);
        if (!fine || *fine) ok = false;
        return v;
    }
};

class Ricerca {
public:
    bool fullscreen = false;
    std::function<void(bool)> modeChanged;
    std::function<void(const std::wstring&,POINT,HWND)> appMenu;
    std::function<void()> catalogChanged;
    std::function<std::vector<std::pair<std::wstring, std::wstring>>()> elencoApp;   // nome, comando d'avvio
    std::function<void()> apriImpostazioni;                                          // impostazioni di FluidWin
    std::function<bool()> conOnda;                                                   // onda di luce accesa nelle impostazioni
    void (*registro)(const char*) = nullptr;

    bool aperta() const { return apertaOra; }
    // Providers own only Work, never Ricerca or its HWND/D2D objects. Cancelling
    // is immediate even if a Shell provider takes time to return.
    void ferma() {
        apertaOra=chiudendo=false;annullaLavoro();attendiLavoro();onda.chiudi();
        if(v.fin){for(UINT id=1;id<=5;++id)KillTimer(v.fin,id);vetro::fermaBattito(v.fin,1);if(GetCapture()==v.fin)ReleaseCapture();}
        clearBitmaps();verifiedIcons.clear();
        for(auto f:{fTesto,fTitolo,fSotto,fSezione})if(f)f->Release();
        fTesto=fTitolo=fSotto=fSezione=nullptr;fontDpi=0;fontLanguage=-1;
        v.chiudi();testo.clear();app.clear();risultati.clear();file.clear();folder.clear();
    }
    ~Ricerca(){ferma();}

#ifdef TLAUNCHER_NATIVE_CI
    std::wstring ciQuery() const {return testo;}
    size_t ciResults() const {return risultati.size();}
    const std::vector<Risultato>& ciResultItems()const{return risultati;}
    bool ciSetting(const std::wstring& action,const std::wstring& title)const{
        for(const auto& r:risultati)if(r.tipo==T_IMPOSTAZIONE&&r.azione==action&&r.titolo==title)return true;return false;
    }
    std::wstring ciSelection()const{return sel>=0&&sel<int(risultati.size())?risultati[sel].azione:L"";}
    size_t ciBitmapCount()const{return icone.size();}
    const void* ciBitmap(const std::wstring& source){return bitmapFor(source);}
    bool ciWorkerIdle()const{std::lock_guard<std::mutex> lock(work->mutex);return !work->running;}
    size_t ciPendingWork()const{std::lock_guard<std::mutex> lock(work->mutex);return work->icons.size()+work->ready.size()+(work->queryGeneration>=0?1:0);}

#endif

    bool prefetch() {
        if(apertaOra||!elencoApp)return true;
        const RECT area=vetro::areaPrincipale();
        const float dpi=vetro::dpiMonitor(MonitorFromPoint({area.left,area.top},MONITOR_DEFAULTTOPRIMARY));
        const int count=gridColumns(area.right-area.left,dpi)*gridRows(area.bottom-area.top,dpi);
        auto catalog=elencoApp();
        std::vector<std::wstring> sources;
        for(const auto& item:catalog) {
            if(!appcatalog::get(item.second).folder.empty())continue;
            auto source=appcatalog::iconSource(item.second);
            if(!source.empty())sources.push_back(std::move(source));
            if(int(sources.size())==count)break;
        }
        queueIcons(sources,iconSize(dpi));
        return !sources.empty();
    }

    void apri() {
        if (apertaOra) { chiudi(); return; }
        catalogGeneration=appcatalog::generation;
        if (!v.fin) {
            if (!v.crea(L"TLauncher.Native.Ricerca", vetro::procedura<Ricerca>, this, true)) return;
            DragAcceptFiles(v.fin,TRUE);

        }
        v.aggiornaTema();
        const RECT a = vetro::areaPrincipale();
        v.dpi = vetro::dpiMonitor(MonitorFromPoint({a.left, a.top}, MONITOR_DEFAULTTOPRIMARY));
        const float d = v.dpi;
        if (!fTesto||fontDpi!=d||fontLanguage!=testi::attuale()) {
            for(auto f:{fTesto,fTitolo,fSotto,fSezione})if(f)f->Release();fontDpi=d;fontLanguage=testi::attuale();
            fTesto = v.formato(21 * d);
            fTitolo = v.formato(14 * d, DWRITE_FONT_WEIGHT_MEDIUM);
            fSotto = v.formato(11.5f * d);
            fSezione = v.formato(11 * d, DWRITE_FONT_WEIGHT_SEMI_BOLD);
        }
        app = elencoApp ? elencoApp() : std::vector<std::pair<std::wstring, std::wstring>>{};
        testo.clear(); risultati.clear(); file.clear(); folder.clear(); page=0; sel = 0; sopra = -1; selUtente = false; saltaBasso = false;
        // (il mouse di adesso: un primo WM_MOUSEMOVE della finestra che cresce sotto il puntatore fermo non e' una
        // scelta, R20261121-124830 su S1 del giro 2)
        { POINT pm{}; GetCursorPos(&pm); ultimoMouse = (DWORD)MAKELONG((short)pm.x, (short)pm.y); }
        // una ricerca della sessione di prima ancora in corso non vale piu' (prima i suoi file arrivavano nella sessione
        // nuova vuota, R20261121-002818-S15)
        annullaLavoro();KillTimer(v.fin,3);
        verifiedIcons.clear();
        larg = (int)std::lround(680 * d);
        x = (a.left + a.right - larg) / 2; y = a.top + (int)std::lround((a.bottom - a.top) * 0.22);
        primaDi = GetForegroundWindow();
        apertaOra = true; chiudendo = false; tAnim = adesso(); ripreso = false;
        scalaW = 0.80; spostaY = 16 * d;   // come al primo fotogramma dell'animazione
        v.alfa(0);
        ridimensiona();
        // onda (goccia d'acqua e luce sui bordi), appena sotto la ricerca; i suoi fotogrammi danno anche il ritmo al
        // riquadro. Prima di mostrare la ricerca: l'onda fotografa lo schermo com'e' adesso, senza di lei
        if (!fullscreen && (!conOnda || conOnda())) {
            const double t1 = adesso();
            onda.avvisa = v.fin; onda.battito = true;
            onda.apri(vetro::areaPrincipale(), {x + larg / 2, y + (int)std::lround(ALTO_BARRA * d / 2)}, d, v.fin);
            if (registro && adesso() - t1 > 0.008) { char b[80]; snprintf(b, sizeof(b), "ricerca: foto per l'onda %.1f ms", (adesso() - t1) * 1000); registro(b); }
        }
        componi(); // Prepare cached icons before the first visible frame.
        ShowWindow(v.fin, SW_SHOW);
        prendiFuoco();
        vetro::battito(v.fin, 1);    // animazione del riquadro (se l'onda non c'e', o tra un battito e l'altro)
        SetTimer(v.fin, 2, 530, nullptr);   // cursore lampeggiante
        passoRiquadro();
        SetTimer(v.fin,5,300,nullptr); // bounded catalog readiness notification; no new discovery
    }

    void chiudi(bool restituisciFuoco = true) {
        if (!apertaOra) return;
        apertaOra = false; chiudendo = true; tAnim = adesso();annullaLavoro();
        if(GetCapture()==v.fin)ReleaseCapture();dragFrom=-1;dragMoved=false;
        KillTimer(v.fin, 2);
        KillTimer(v.fin,3);KillTimer(v.fin,4);KillTimer(v.fin,5);
        onda.chiudi();
        // il riquadro sfuma salendo un poco e restringendosi (sotto nessun clic: e' gia' chiusa)
        vetro::battito(v.fin, 1);
        if (restituisciFuoco && primaDi && IsWindow(primaDi) && IsWindowVisible(primaDi)) SetForegroundWindow(primaDi);
        passoRiquadro();
    }

    bool messaggio(HWND hw, UINT m, WPARAM w, LPARAM l, LRESULT& r) {
        r = 0;
        if (m == onda.messaggio) {
            if (w == 1) { onda.battitoRicevuto(); passoRiquadro(); } else onda.controlla();
            return true;
        }
        // mentre sfuma e' gia' chiusa: niente clic ne' tasti
        if (chiudendo && ((m >= WM_MOUSEFIRST && m <= WM_MOUSELAST) || m == WM_KEYDOWN || m == WM_CHAR)) return true;
        switch (m) {
            case WM_TIMER:
                if (w == 1) { passoRiquadro(); return true; }
                if (w == 2) cursore = !cursore;
                if (w == 3) { KillTimer(v.fin, 3); chiediFile(); }
                if (w == 4) { raccogliIcone(); }
                if(w==5&&apertaOra&&elencoApp){auto next=elencoApp();if(next!=app||catalogGeneration!=appcatalog::generation){catalogGeneration=appcatalog::generation;verifiedIcons.clear();app=std::move(next);componi();}return true;}
                disegna();
                return true;
            case WM_PAINT: disegna(); return true;
            case WM_APP + 1: {   // risultati dei file dal thread
                if (!consegna::riprendi(hw, m, (const void*)l)) return true;   // (non consegnato dal thread: ignorato)
                auto* lista = (std::vector<Risultato>*)l;
                if ((LONG)w == generazione && apertaOra) { file = std::move(*lista); componi(); }
                delete lista;
                return true;
            }
            case WM_CHAR: {
                // Tetto di 512 come l'incolla, ma un carattere fuori dal piano base (due unita') entra intero o non entra:
                // controllato sulla singola unita' lasciava mezza emoji nella query (R20261121-124830-S1)
                if (w < 32 || w == 127 || (GetKeyState(VK_CONTROL) & 0x8000)) return true;
                const wchar_t c = (wchar_t)w;
                if (IS_HIGH_SURROGATE(c)) { saltaBasso = testo.size() + 2 > 512; if (!saltaBasso) testo += c; return true; }
                if (IS_LOW_SURROGATE(c)) {
                    const bool ok = !saltaBasso && !testo.empty() && IS_HIGH_SURROGATE(testo.back());
                    saltaBasso = false;
                    if (ok) { testo += c; cambiato(); }
                    return true;
                }
                saltaBasso = false;
                if (!testo.empty() && IS_HIGH_SURROGATE(testo.back())) testo.pop_back();   // (meta' rimasta sola)
                if (testo.size() < 512) { testo += c; cambiato(); }
                return true;
            }
            case WM_KEYDOWN: tasto(w); return true;
            case WM_MOUSEMOVE: {
                if(GetCapture()==hw&&dragFrom>=0&&(abs(GET_X_LPARAM(l)-dragPoint.x)>GetSystemMetrics(SM_CXDRAG)||abs(GET_Y_LPARAM(l)-dragPoint.y)>GetSystemMetrics(SM_CYDRAG)))dragMoved=true;
                const int s = rigaIn(GET_X_LPARAM(l), GET_Y_LPARAM(l));
                // (scelta dell'utente solo se il mouse si e' mosso davvero: la finestra che cresce sotto il mouse fermo
                // manda anche lei WM_MOUSEMOVE)
                const DWORD mp = GetMessagePos(); const bool mosso = mp != ultimoMouse; ultimoMouse = mp;
                // (un movimento vero dentro la riga gia' evidenziata e' una scelta anche lui, R20261121-124830-S2)
                if (s >= 0 && mosso && !selUtente) { sel = s; selUtente = true; }
                if (s != sopra) { sopra = s; if (s >= 0) { sel = s; if (mosso) selUtente = true; } disegna(); }
                return true;
            }
            case WM_LBUTTONDOWN:
                dragFrom=rigaIn(GET_X_LPARAM(l),GET_Y_LPARAM(l));dragPoint={GET_X_LPARAM(l),GET_Y_LPARAM(l)};dragMoved=false;
                if(fullscreen&&dragFrom>=0)SetCapture(hw);return true;
            case WM_CAPTURECHANGED: if((HWND)l!=hw){dragFrom=-1;dragMoved=false;}return true;
            case WM_DROPFILES: {
                HDROP files=reinterpret_cast<HDROP>(w);POINT p{};DragQueryPoint(files,&p);int cell=rigaIn(p.x,p.y);
                std::wstring to=cell>=0&&cell<int(risultati.size())&&risultati[cell].tipo==T_FOLDER?risultati[cell].azione:folder;
                const UINT count=std::min(64u,DragQueryFileW(files,0xffffffff,nullptr,0));
                for(UINT i=0;i<count;i++){UINT n=DragQueryFileW(files,i,nullptr,0);if(n>32767)continue;std::wstring file(n+1,L'\0');DragQueryFileW(files,i,file.data(),n+1);file.resize(n);
                    auto ext=appcatalog::normalize(std::filesystem::path(file).extension().wstring());const DWORD a=GetFileAttributesW(file.c_str());
                    if((ext!=L".exe"&&ext!=L".lnk")||a==INVALID_FILE_ATTRIBUTES||(a&FILE_ATTRIBUTE_DIRECTORY))continue;
                    auto meta=appcatalog::get(file);meta.manual=true;meta.hidden=false;meta.folder=to;if(meta.name.empty())meta.name=std::filesystem::path(file).stem().wstring();appcatalog::save(file,meta);
                }
                DragFinish(files);if(catalogChanged)catalogChanged();componi();return true;
            }
            case WM_LBUTTONUP: {
                if(fullscreen&&!folder.empty()&&GET_Y_LPARAM(l)>int(80*v.dpi)&&GET_Y_LPARAM(l)<int(110*v.dpi)){folder.clear();page=0;componi();return true;}
                const int s=rigaIn(GET_X_LPARAM(l),GET_Y_LPARAM(l));
                const int from=dragFrom;const bool moved=dragMoved;dragFrom=-1;dragMoved=false;if(GetCapture()==hw)ReleaseCapture();
                if(fullscreen&&moved&&from>=0&&s>=0&&s!=from)drop(from,s);
                else if(!moved&&s>=0)esegui(s);else if(!moved&&fullscreen&&GET_Y_LPARAM(l)>int(ALTO_BARRA*v.dpi))chiudi();
                return true;
            }
            case WM_RBUTTONUP: context(rigaIn(GET_X_LPARAM(l),GET_Y_LPARAM(l)));return true;
            case WM_MOUSEWHEEL:
                if(fullscreen){page=std::clamp(page+(GET_WHEEL_DELTA_WPARAM(w)>0?-1:1),0,std::max(0,(int(risultati.size())-1)/pageSize()));chiediIcone();disegna();}return true;
            case WM_ACTIVATE:
                if (LOWORD(w) == WA_INACTIVE && apertaOra && !menu::aperti && !editing) {
                    // Appena aperta, il primo piano preso agganciandosi all'app di prima a volte torna a lei subito
                    // dopo lo sgancio (una volta su due nelle prove): non e' l'utente che ha cliccato altrove, si
                    // riprende (una volta sola) invece di chiudersi
                    if (agganciata && !ripreso && adesso() - tAnim < 0.3 && GetForegroundWindow() == primaDi) {
                        ripreso = true;
                        if (registro) registro("ricerca: primo piano tornato all'app di prima, lo riprende");
                        PostMessageW(v.fin, WM_APP + 2, 0, 0);
                        return true;
                    }
                    if (registro) {
                        char b[200]; wchar_t c[64] = L"", c2[64] = L""; GetClassNameW((HWND)l, c, 64);
                        HWND fg = GetForegroundWindow(); GetClassNameW(fg, c2, 64);
                        snprintf(b, sizeof(b), "ricerca: chiusa, attivata %p (%ls), in primo piano %p (%ls), primaDi %p, aggancio %d",
                                 (void*)l, c, (void*)fg, c2, (void*)primaDi, (int)agganciata);
                        registro(b);
                    }
                    chiudi(false);
                }
                return true;
            case WM_CLOSE: chiudi(); return true;
            case WM_APP + 2: if (apertaOra) prendiFuoco(); return true;
        }
        return false;
    }

private:
    vetro::Finestra v;
    IDWriteTextFormat *fTesto = nullptr, *fTitolo = nullptr, *fSotto = nullptr, *fSezione = nullptr;
    std::wstring testo;
    std::wstring folder;
    int page=0,dragFrom=-1;POINT dragPoint{};bool dragMoved=false;
    float fontDpi=0;int fontLanguage=-1;
    bool editing=false;uint64_t catalogGeneration=0;
    std::vector<std::pair<std::wstring, std::wstring>> app;
    std::vector<Risultato> risultati, file;
    int sel = 0, sopra = -1, larg = 0, x = 0, y = 0;
    DWORD ultimoMouse = 0;
    bool saltaBasso = false;   // WM_CHAR: la prima meta' di un carattere non entrata, si scarta anche la seconda
    bool selUtente = false;   // sel scelta con frecce o mouse (non la prima riga data da sola: vedi componi)
    bool apertaOra = false, cursore = true;
    HWND primaDi = nullptr;
    onda::Onda onda;
    // animazione del riquadro: all'apertura si allarga da una pillola piu' stretta con un piccolo rimbalzo, salendo
    // di poco e apparendo; alla chiusura sfuma salendo e restringendosi, poi si nasconde
    bool chiudendo = false;
    double tAnim = 0;
    double scalaW = 1, spostaY = 0;   // larghezza (frazione) e spostamento verticale (px) di adesso
    static constexpr double DURATA_APRI = 0.42, DURATA_CHIUDI = 0.16;

    static double adesso() {
        static LARGE_INTEGER f = [] { LARGE_INTEGER x; QueryPerformanceFrequency(&x); return x; }();
        LARGE_INTEGER c; QueryPerformanceCounter(&c); return (double)c.QuadPart / f.QuadPart;
    }

    void passoRiquadro() {
        if (!apertaOra && !chiudendo) return;
        const float d = v.dpi;
        const double e = adesso() - tAnim;
        double a;
        bool finita;
        if (apertaOra) {
            const double m = vetro::molla(e / DURATA_APRI);
            scalaW = 0.80 + 0.20 * m;
            spostaY = (1 - m) * 16 * d;
            a = vetro::uscita(e / 0.15);
            finita = e >= DURATA_APRI;
            if (finita) { scalaW = 1; spostaY = 0; a = 1; }
        } else {
            const double k = vetro::uscita(e / DURATA_CHIUDI);
            scalaW = 1 - 0.06 * k;
            spostaY = -8 * d * k;
            a = 1 - k;
            finita = e >= DURATA_CHIUDI;
        }
        ridimensiona();
        v.alfa(a);
        disegna();
        if (!finita) return;
        vetro::fermaBattito(v.fin, 1);
        onda.battito = false;
        if (chiudendo) { chiudendo = false; ShowWindow(v.fin, SW_HIDE); scalaW = 1; spostaY = 0; }
    }
    volatile LONG generazione = 0;
    struct IconJob { std::wstring source; int size; ULONGLONG generation; };
    struct Pixel { IconJob job; std::shared_ptr<const appicons::Pixels> pixels; };
    struct Bitmap {
        ID2D1Bitmap* image=nullptr;
        std::weak_ptr<const appicons::Pixels> pixels;
        ULONGLONG used=0;
    };
    struct Work {
        std::mutex mutex;std::condition_variable idle;bool running=false;LONG generation=0,queryGeneration=-1,fileGeneration=-1;
        std::wstring query;std::deque<IconJob> icons;std::vector<Pixel> ready;std::vector<Risultato> files;
    };
    std::shared_ptr<Work> work=std::make_shared<Work>();
    void attendiLavoro(){
        std::unique_lock<std::mutex> lock(work->mutex);
        work->idle.wait(lock,[&]{return !work->running;});
    }
    void annullaLavoro(){
        InterlockedIncrement(&generazione);std::lock_guard<std::mutex> lock(work->mutex);
        work->generation=generazione;work->queryGeneration=work->fileGeneration=-1;
        work->query.clear();work->icons.clear();work->ready.clear();work->files.clear();
        pendingIcons.clear();
    }
    void avviaLavoro(){
        std::lock_guard<std::mutex> lock(work->mutex);
        if(work->running||(work->queryGeneration<0&&work->icons.empty()))return;
        work->running=true;auto job=new std::shared_ptr<Work>(work);
        if(!pool::lancia(lavoratore,job,THREAD_PRIORITY_BELOW_NORMAL,true)){work->running=false;delete job;}
    }
    std::unordered_map<std::wstring,Bitmap> icone;
    std::unordered_set<std::wstring> pendingIcons;
    std::unordered_map<std::wstring,ULONGLONG> verifiedIcons;
    unsigned giroIcone=0;
    int bitmapSize=0;

    static int iconSize(float dpi){return std::clamp(int(std::ceil(58*dpi/32))*32,64,256);}

    void clearBitmaps() {
        for(auto& [key,bitmap]:icone)if(bitmap.image)bitmap.image->Release();
        icone.clear();
    }

    ID2D1Bitmap* bitmapFor(const std::wstring& source) {
        auto found=icone.find(source);
        if(found==icone.end())return nullptr;
        found->second.used=GetTickCount64();
        return found->second.image;
    }

    void putBitmap(const std::wstring& source,const std::shared_ptr<const appicons::Pixels>& pixels) {
        if(!pixels||!v.rt)return;
        auto found=icone.find(source);
        if(found!=icone.end()&&found->second.pixels.lock()==pixels)return;
        ID2D1Bitmap* image=nullptr;
        if(FAILED(v.rt->CreateBitmap(D2D1::SizeU(pixels->w,pixels->h),pixels->bgra.data(),pixels->w*4,
                D2D1::BitmapProperties(D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM,D2D1_ALPHA_MODE_PREMULTIPLIED),96,96),&image)))return;
        const size_t limit=std::min<size_t>(256,16*1024*1024/(4*bitmapSize*bitmapSize));
        if(found==icone.end()&&icone.size()>=limit) {
            auto oldest=std::min_element(icone.begin(),icone.end(),[](const auto& a,const auto& b){return a.second.used<b.second.used;});
            oldest->second.image->Release();
            icone.erase(oldest);
        }
        auto& slot=icone[source];
        if(slot.image)slot.image->Release();
        slot={image,pixels,GetTickCount64()};
    }

    static constexpr float ALTO_BARRA = 64, ALTO_RIGA = 48;

    bool agganciata = false, ripreso = false;
    void prendiFuoco() {
        if (!ripreso) agganciata = false;
        SetForegroundWindow(v.fin);
        if (GetForegroundWindow() == v.fin) return;
        agganciata = true;
        HWND fg = GetForegroundWindow();
        const DWORD a = fg ? GetWindowThreadProcessId(fg, nullptr) : 0, b = GetCurrentThreadId();
        // (mai agganciati a un'app che non risponde: il suo input bloccato bloccherebbe anche quello di FluidWin)
        if (a && a != b && !IsHungAppWindow(fg) && AttachThreadInput(b, a, TRUE)) { SetForegroundWindow(v.fin); SetFocus(v.fin); AttachThreadInput(b, a, FALSE); }
    }

    void ridimensiona() {
        if(fullscreen){RECT a=vetro::areaPrincipale();v.posiziona(a.left,a.top,a.right-a.left,a.bottom-a.top);return;}
        const float d = v.dpi;
        const int righe = (int)risultati.size();
        const int h = (int)std::lround((ALTO_BARRA + (righe ? 10 + righe * ALTO_RIGA + 8 : 0)) * d);
        const int w = (int)std::lround(larg * scalaW);
        v.posiziona(x + (larg - w) / 2, y + (int)std::lround(spostaY), w, h, HWND_TOPMOST);
    }

    void tasto(WPARAM k) {
        const bool ctrl = GetKeyState(VK_CONTROL) & 0x8000;
        if(k==VK_F6){selUtente=true;fullscreen=!fullscreen;if(modeChanged)modeChanged(fullscreen);onda.chiudi();ridimensiona();componi();if(fullscreen){page=sel/pageSize();chiediIcone();disegna();}return;}
        if(k==VK_ESCAPE&&!folder.empty()){folder.clear();componi();return;}
        if(fullscreen&&(k==VK_LEFT||k==VK_RIGHT||k==VK_UP||k==VK_DOWN)&&!risultati.empty()) {
            int delta=k==VK_LEFT?-1:k==VK_RIGHT?1:k==VK_UP?-columns():columns();
            sel=std::clamp(sel+delta,0,int(risultati.size())-1);page=sel/pageSize();selUtente=true;chiediIcone();disegna();return;
        }
        if (k == VK_ESCAPE) chiudi();
        else if (k == VK_BACK && !testo.empty()) {
            if (ctrl) { while (!testo.empty() && testo.back() == L' ') testo.pop_back(); while (!testo.empty() && testo.back() != L' ') testo.pop_back(); }
            else { testo.pop_back(); if (!testo.empty() && IS_HIGH_SURROGATE(testo.back())) testo.pop_back(); }   // (l'emoji intera)
            cambiato();
        }
        else if (k == VK_DOWN && !risultati.empty()) { sel = (sel + 1) % (int)risultati.size(); selUtente = true; disegna(); }
        else if (k == VK_UP && !risultati.empty()) { sel = (sel - 1 + (int)risultati.size()) % (int)risultati.size(); selUtente = true; disegna(); }
        else if (k == VK_RETURN) esegui(sel);
        else if (ctrl && k == 'V' && OpenClipboard(v.fin)) {
            const size_t primaIncolla = testo.size();
            // dentro il blocco e al massimo 512 caratteri in tutto: un incolla enorme fermava l'interfaccia (S11)
            if (HANDLE hd = GetClipboardData(CF_UNICODETEXT)) if (const wchar_t* p = (const wchar_t*)GlobalLock(hd)) {
                // Si esaminano al massimo 4096 unita' (un blocco di soli a capo non fa crescere il testo, e prima si
                // percorreva tutto). Una coppia surrogata entra intera o non entra: tagliata a 512 restava a meta'
                // (R20261121-120822-S4 e conferma di S11)
                const size_t n = std::min<size_t>(GlobalSize(hd) / sizeof(wchar_t), 4096);
                for (size_t i = 0; i < n && p[i] && testo.size() < 512; i++) {
                    if (p[i] < 32) continue;
                    if (IS_HIGH_SURROGATE(p[i])) {
                        if (i + 1 >= n || !IS_LOW_SURROGATE(p[i + 1]) || testo.size() + 2 > 512) break;
                        testo += p[i]; testo += p[++i];
                    } else if (!IS_LOW_SURROGATE(p[i])) testo += p[i];
                }
                GlobalUnlock(hd);
            }
            // (cambiato solo se e' entrato qualcosa: un incolla di un'immagine azzerava la riga scelta, R20261121-124830-S3)
            CloseClipboard(); if (testo.size() != primaIncolla) cambiato();
        }
    }

    void cambiato() {
        cursore = true; SetTimer(v.fin, 2, 530, nullptr);
        selUtente = false; sel = 0;   // testo nuovo: torna la prima riga (la scelta di prima era per l'altro testo)
        annullaLavoro();
        file.clear();
        componi();
        // i file si cercano quando si smette di scrivere per un attimo
        KillTimer(v.fin, 3);
        if (testo.size() >= 2) SetTimer(v.fin, 3, 180, nullptr);
    }

    static std::wstring minuscolo(std::wstring s) { return appcatalog::fold(s); }

    static int punteggio(const std::wstring& nome, const std::wstring& q) {
        const std::wstring n = minuscolo(nome);
        const size_t p = n.find(q);
        if (p == std::wstring::npos) return -1;
        if (p == 0) return 3;
        if (n[p - 1] == L' ' || n[p - 1] == L'-') return 2;
        return 1;
    }

    void componi() {
        // la voce scelta con le frecce si ritrova per tipo e azione, non per numero di riga: i file che arrivano dopo
        // entrano prima di "Cerca sul web", e Invio apriva un file al suo posto (R20261121-002818-S9)
        // Solo una scelta vera (frecce o mouse): la prima riga data da sola resta la prima. Prima anche quella si
        // ritrovava per tipo, e se in attesa dei file c'era solo "Cerca sul web", Invio mandava al browser il nome del
        // file appena trovato (R20261121-120822-S1)
        const bool scelta = selUtente && sel >= 0 && sel < (int)risultati.size();
        if (!selUtente) sel = 0;
        const int tipoScelto = scelta ? (int)risultati[sel].tipo : -1;
        const std::wstring azioneScelta = scelta ? risultati[sel].azione : L"";
        risultati.clear();
        const size_t a = testo.find_first_not_of(L' '), b = testo.find_last_not_of(L' ');
        std::wstring q = a == std::wstring::npos ? L"" : testo.substr(a, b - a + 1);   // (senza spazi ai lati, in un passo)
        if (!q.empty()) {
            const std::wstring ql = minuscolo(q);
            double val;
            if (Calcolo::valuta(q, val)) {
                wchar_t b[64]; swprintf(b, 64, L"%.10g", val);
                std::wstring s = b; const wchar_t vg = testi::virgola(); for (auto& c : s) if (c == L'.') c = vg;   // (virgola in italiano, spagnolo...)
                risultati.push_back({T_CALCOLO, L"= " + s, testi::t(L"Invio per copiare il risultato", L"Enter to copy the result"), s, L""});
            }
            // app: prima chi inizia con il testo
            std::vector<std::pair<int, size_t>> trovate;
            for (size_t i = 0; i < app.size(); i++) { const int p = punteggio(app[i].first, ql); if (p > 0) trovate.push_back({p, i}); }
            std::stable_sort(trovate.begin(), trovate.end(), [](auto& a, auto& b) { return a.first > b.first; });
            int selectedRank=-1;
            if(scelta&&tipoScelto==T_APP)for(size_t i=0;i<trovate.size();i++)if(app[trovate[i].second].second==azioneScelta)selectedRank=int(i);
            for (size_t i = 0; i < trovate.size(); i++) {
                if(!fullscreen&&i>=5&&int(i)!=selectedRank)continue;
                if(!fullscreen&&i==4&&selectedRank>=5)continue;
                const auto& a = app[trovate[i].second];
                risultati.push_back({T_APP, a.first, testi::t(L"App",L"App"), a.second, appcatalog::iconSource(a.second)});
            }
            int n = 0;
            for (const auto& s : pagine()) {
                if (n >= 3) break;
                if (punteggio(s.titolo, ql) > 0 || minuscolo(s.sotto).find(ql) != std::wstring::npos) { Risultato r = s; r.sotto = testi::t(L"Impostazioni", L"Settings"); risultati.push_back(r); n++; }
            }
            // i file che entrano: la barra resta dentro tre quarti dello schermo (con 8 file, piu' app, impostazioni e
            // web, su uno schermo basso usciva di sotto). Una riga resta per la ricerca sul web
            {
                MONITORINFO mi{}; mi.cbSize = sizeof(mi);
                const bool ok = GetMonitorInfoW(MonitorFromWindow(v.fin, MONITOR_DEFAULTTONEAREST), &mi);
                const float alto = ok ? (mi.rcWork.bottom - mi.rcWork.top) / v.dpi * 0.75f : 1000;
                const int righe = std::max(0, (int)((alto - ALTO_BARRA - 18) / ALTO_RIGA));
                const int posto = fullscreen?int(file.size()):std::max(0, righe - (int)risultati.size() - 1);
                for (size_t i = 0; i < file.size() && (int)i < posto; i++) risultati.push_back(file[i]);
            }
            risultati.push_back({T_WEB, testi::t(L"Cerca “", L"Search “") + testi::isola(q) + testi::t(L"” sul web", L"” on the web"),
                                 testi::t(L"Nel browser predefinito", L"In the default browser"), q, L""});
        } else if(fullscreen) {
            if(folder.empty())for(const auto& f:appcatalog::folders())risultati.push_back({T_FOLDER,f,L"",f,L""});
            for(const auto& a:app)if(appcatalog::get(a.second).folder==folder)risultati.push_back({T_APP,a.first,L"",a.second,appcatalog::iconSource(a.second)});
        }
        sel = std::clamp(sel, 0, std::max(0, (int)risultati.size() - 1));
        if (scelta) for (int i = 0; i < (int)risultati.size(); i++) if ((int)risultati[i].tipo == tipoScelto && risultati[i].azione == azioneScelta) { sel = i; break; }
        if (risultati.empty()) sel = 0;
        sopra = -1;
        chiediIcone();
        ridimensiona();
        disegna();
    }

    void queueIcons(const std::vector<std::wstring>& sources,int size) {
        bool queued=false;
        {
            std::lock_guard<std::mutex> lock(work->mutex);
            const auto now=GetTickCount64();
            if(verifiedIcons.size()>1024)verifiedIcons.clear();
            for(const auto& source:sources) {
                if(source.empty()||work->icons.size()>=256)continue;
                const auto key=appicons::requestKey(source,size);
                const auto verified=verifiedIcons.find(key);
                if(verified!=verifiedIcons.end()&&now-verified->second<5000)continue;
                if(!pendingIcons.insert(key).second)continue;
                work->icons.push_back({source,size,ULONGLONG(generazione)});
                queued=true;
            }
        }
        if(queued) {
            avviaLavoro();
            if(v.fin)SetTimer(v.fin,4,50,nullptr);
        }
    }

    void chiediIcone() {
        const int size=iconSize(v.dpi);
        if(giroIcone!=v.giroRt||bitmapSize!=size) {
            giroIcone=v.giroRt;
            bitmapSize=size;
            clearBitmaps();
            verifiedIcons.clear();
        }
        std::vector<std::wstring> sources;
        const int begin=fullscreen?page*pageSize():0;
        const int end=fullscreen?std::min(int(risultati.size()),begin+pageSize()):int(risultati.size());
        for(int i=begin;i<end;++i) {
            const auto& source=risultati[i].icona;
            if(source.empty())continue;
            if(!icone.count(source))putBitmap(source,appicons::peek(source,size));
            sources.push_back(source);
        }
        queueIcons(sources,size);
    }

    void chiediFile() {
        if (!apertaOra || testo.size() < 2) return;
        {std::lock_guard<std::mutex> lock(work->mutex);work->query=testo;work->queryGeneration=generazione;}
        avviaLavoro();SetTimer(v.fin,4,50,nullptr);
    }

    void raccogliIcone() {
        std::vector<Pixel> arrivate;
        std::vector<Risultato> found;LONG foundGeneration=-1;bool finite=false;
        {std::lock_guard<std::mutex> lock(work->mutex);arrivate.swap(work->ready);found.swap(work->files);
            foundGeneration=work->fileGeneration;work->fileGeneration=-1;
            finite=!work->running&&work->icons.empty()&&work->queryGeneration<0;}
        if(foundGeneration==generazione&&apertaOra){file=std::move(found);componi();finite=false;}
        avviaLavoro();
        for(auto& item:arrivate) {
            const auto& job=item.job;
            if(job.generation!=ULONGLONG(generazione))continue;
            const auto key=appicons::requestKey(job.source,job.size);
            pendingIcons.erase(key);
            verifiedIcons[key]=GetTickCount64();
            if(job.size==bitmapSize&&apertaOra) {
                if(item.pixels)putBitmap(job.source,item.pixels);
                else if(auto found=icone.find(job.source);found!=icone.end()) {
                    found->second.image->Release();
                    icone.erase(found);
                }
            }
        }
        if (finite && arrivate.empty()) KillTimer(v.fin, 4);
    }

    void esegui(int i) {
        if (i < 0 || i >= (int)risultati.size()) return;
        const Risultato r = risultati[i];
        if(r.tipo==T_FOLDER){folder=r.azione;page=0;sel=0;componi();return;}
        chiudi(false);
        switch (r.tipo) {
            case T_CALCOLO:
                if (OpenClipboard(v.fin)) {
                    EmptyClipboard();
                    const size_t n = (r.azione.size() + 1) * sizeof(wchar_t);
                    if (HGLOBAL g = GlobalAlloc(GMEM_MOVEABLE, n)) { memcpy(GlobalLock(g), r.azione.c_str(), n); GlobalUnlock(g); SetClipboardData(CF_UNICODETEXT, g); }
                    CloseClipboard();
                }
                if (primaDi && IsWindow(primaDi)) SetForegroundWindow(primaDi);
                break;
            case T_WEB: {
                // tutta la stringa in UTF-8 in una volta, poi in percentuale: prima un carattere alla volta, e le emoji o
                // gli ideogrammi fuori dal piano base (due wchar_t) diventavano caratteri sostitutivi (S20)
                std::wstring u = L"https://www.google.com/search?q=";
                const int n8 = WideCharToMultiByte(CP_UTF8, 0, r.azione.c_str(), (int)r.azione.size(), nullptr, 0, nullptr, nullptr);
                std::string b8(n8 > 0 ? n8 : 0, '\0');
                if (n8 > 0) WideCharToMultiByte(CP_UTF8, 0, r.azione.c_str(), (int)r.azione.size(), b8.data(), n8, nullptr, nullptr);
                for (unsigned char c : b8) {
                    if ((c >= '0' && c <= '9') || (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')) u += (wchar_t)c;
                    else if (c == ' ') u += L'+';
                    else { wchar_t h[4]; swprintf(h, 4, L"%%%02X", c); u += h; }
                }
                ShellExecuteW(nullptr, L"open", u.c_str(), nullptr, nullptr, SW_SHOWNORMAL);
                break;
            }
            default:
                if (r.azione == L"*tlauncher-native") { if (apriImpostazioni) apriImpostazioni(); }
                else ShellExecuteW(nullptr, L"open", r.azione.c_str(), nullptr, nullptr, SW_SHOWNORMAL);
        }
    }

    int rigaIn(int px, int py) const {
        if(fullscreen){for(int i=page*pageSize();i<std::min(int(risultati.size()),(page+1)*pageSize());++i){auto r=cell(i);if(px>=r.left&&px<r.right&&py>=r.top&&py<r.bottom)return i;}return -1;}
        const float d = v.dpi, y0 = (ALTO_BARRA + 10) * d;
        if (py < y0 || px < 8 * d || px > v.W - 8 * d) return -1;
        const int i = (int)((py - y0) / (ALTO_RIGA * d));
        return i < (int)risultati.size() ? i : -1;
    }

    static int gridColumns(int width,float dpi){return std::clamp(int(width/(136*dpi)),3,12);}
    static int gridRows(int height,float dpi){return std::clamp(int((height-160*dpi)/(124*dpi)),1,16);}
    int columns() const {return gridColumns(v.W,v.dpi);}
    int rows() const {return gridRows(v.H,v.dpi);}
    int pageSize() const {return columns()*rows();}
    D2D1_RECT_F cell(int index) const {
        const int col=columns(),i=index%pageSize();const float d=v.dpi,cw=std::min(148*d,(v.W-48*d)/col);
        const float start=(v.W-cw*col)/2,x=start+(i%col)*cw,y=112*d+(i/col)*124*d;
        return {x,y,x+cw,y+116*d};
    }
    void drawFullscreen() {
        const float d=v.dpi;
        const float width=std::min(680*d,v.W-48*d),left=(v.W-width)/2;
        v.pen->SetColor(v.inchiostro(.07f));v.rt->FillRoundedRectangle({{left,24*d,left+width,80*d},22*d,22*d},v.pen);
        v.testo(testo.empty()?testi::t(L"Cerca app, file, impostazioni…",L"Search apps, files, settings…"):testo,fTesto,{left+22*d,24*d,left+width-22*d,80*d},v.inchiostro(testo.empty()?.55f:1.f));
        if(!folder.empty())v.testo(std::wstring(L"‹ ")+testi::t(L"Tutte le app",L"All apps")+L" / "+folder,fSotto,{24*d,80*d,float(v.W)-24*d,108*d},v.inchiostro(.7f));
        page=std::clamp(page,0,std::max(0,(int(risultati.size())-1)/pageSize()));
        for(int i=page*pageSize();i<std::min(int(risultati.size()),(page+1)*pageSize());++i) {
            const auto& result=risultati[i];auto r=cell(i);
            if(i==sel||i==sopra){v.pen->SetColor(v.inchiostro(i==sopra?.13f:.07f));v.rt->FillRoundedRectangle({r,16*d,16*d},v.pen);}
            const float cx=(r.left+r.right)/2,iy=r.top+10*d,size=58*d;
            auto image=bitmapFor(result.icona);
            if(image)v.rt->DrawBitmap(image,{cx-size/2,iy,cx+size/2,iy+size});
            else if(result.tipo==T_FOLDER){v.pen->SetColor(D2D1::ColorF(.95f,.72f,.25f,.95f));v.rt->FillRoundedRectangle({{cx-28*d,iy+8*d,cx+28*d,iy+52*d},7*d,7*d},v.pen);}
            else simbolo(result.tipo,cx,iy+size/2,v.inchiostro(.6f));
            v.testo(result.titolo,fTitolo,{r.left+4*d,iy+size+8*d,r.right-4*d,r.bottom-3*d},v.inchiostro());
        }
        const int pages=std::max(1,(int(risultati.size())+pageSize()-1)/pageSize());
        if(pages>1)for(int i=0;i<pages&&i<30;++i){v.pen->SetColor(v.inchiostro(i==page?.8f:.25f));v.rt->FillEllipse({{v.W/2.f+(i-(pages-1)/2.f)*14*d,v.H-24*d},3*d,3*d},v.pen);}
    }
    void drop(int from,int to) {
        if(from<0||to<0||from>=int(risultati.size())||to>=int(risultati.size()))return;
        auto a=risultati[from],b=risultati[to];
        if(a.tipo==T_FOLDER&&b.tipo==T_FOLDER){auto ordered=appcatalog::folders();auto fromIt=std::find(ordered.begin(),ordered.end(),a.azione),toIt=std::find(ordered.begin(),ordered.end(),b.azione);
            if(fromIt!=ordered.end()&&toIt!=ordered.end()){size_t position=size_t(toIt-ordered.begin());auto name=*fromIt;ordered.erase(fromIt);ordered.insert(ordered.begin()+std::min(position,ordered.size()),name);for(size_t i=0;i<ordered.size();i++)appcatalog::write(appcatalog::root()+L"\\FolderOrder",ordered[i].c_str(),std::to_wstring(i));componi();}return;}
        if(a.tipo!=T_APP)return;
        auto m=appcatalog::get(a.azione);
        if(b.tipo==T_FOLDER)m.folder=b.azione;
        else if(b.tipo==T_APP) {
            for(int i=0;i<int(risultati.size());++i)if(risultati[i].tipo==T_APP){auto x=appcatalog::get(risultati[i].azione);x.order=i==to?from:i;appcatalog::save(risultati[i].azione,x);}m.order=to;
        }
        appcatalog::save(a.azione,m);if(catalogChanged)catalogChanged();componi();
    }
    void context(int index) {
        POINT p{};GetCursorPos(&p);editing=true;
        if(index>=0&&index<int(risultati.size())&&risultati[index].tipo==T_APP) {
            auto command=risultati[index].azione;if(appMenu)appMenu(command,p,v.fin);
        } else {
            std::vector<menu::Voce> items;
            items.push_back(menu::voce(testi::t(L"Aggiungi app…",L"Add application…"),1));
            items.push_back(menu::voce(testi::t(L"Nuova cartella…",L"New folder…"),2));
            std::wstring selectedFolder=index>=0&&index<int(risultati.size())&&risultati[index].tipo==T_FOLDER?risultati[index].azione:folder;
            if(!selectedFolder.empty()) {items.push_back(menu::voce(testi::t(L"Rinomina cartella…",L"Rename folder…"),3));items.push_back(menu::voce(testi::t(L"Elimina cartella",L"Delete folder"),4));items.push_back(menu::voce(testi::t(L"Tutte le app",L"All apps"),5));}
            std::vector<std::wstring> hidden;appcatalog::load();for(const auto& e:appcatalog::metadata)if(e.second.hidden){hidden.push_back(e.first);items.push_back(menu::voce(testi::t(L"Mostra: ",L"Restore: ")+(e.second.name.empty()?e.first:e.second.name),UINT(100+hidden.size()-1)));}
            UINT command=menu::mostra(items,p,v.fin);
            if(command==1){auto file=appcatalog::choose(v.fin,false);if(!file.empty()){auto m=appcatalog::get(file);m.manual=true;m.hidden=false;m.name=std::filesystem::path(file).stem().wstring();appcatalog::save(file,m);}}
            if(command==2){std::wstring name;if(appcatalog::prompt(v.fin,testi::t(L"Nuova cartella…",L"New folder…"),name)&&!name.empty())appcatalog::folder(L"",name);}
            if(command==3){auto name=selectedFolder;if(appcatalog::prompt(v.fin,testi::t(L"Rinomina cartella…",L"Rename folder…"),name)&&!name.empty()){appcatalog::folder(selectedFolder,name);if(folder==selectedFolder)folder=name;}}
            if(command==4){appcatalog::folder(selectedFolder,L"");if(folder==selectedFolder)folder.clear();}
            if(command==5)folder.clear();
            if(command>=100&&command-100<hidden.size()){auto m=appcatalog::get(hidden[command-100]);m.hidden=false;appcatalog::save(hidden[command-100],m);}
            if(command&&catalogChanged)catalogChanged();
        }
        editing=false;verifiedIcons.clear();app=elencoApp?elencoApp():app;componi();
    }

    void disegna() {
        if (!apertaOra && !chiudendo) return;
        if(giroIcone!=v.giroRt||bitmapSize!=iconSize(v.dpi))chiediIcone();
        if (!v.inizia()) return;
        if(fullscreen){drawFullscreen();v.finisci();return;}
        const float d = v.dpi, W = (float)v.W;
        const D2D1_COLOR_F ink = v.inchiostro(), tenue = v.inchiostro(0.55f);
        // lente
        v.pen->SetColor(tenue);
        const float lx = 30 * d, ly = ALTO_BARRA * d / 2;
        v.rt->DrawEllipse({{lx - 1.5f * d, ly - 1.5f * d}, 7.5f * d, 7.5f * d}, v.pen, 2.0f * d);
        v.rt->DrawLine({lx + 4 * d, ly + 4 * d}, {lx + 9 * d, ly + 9 * d}, v.pen, 2.2f * d);
        const D2D1_RECT_F rt = {54 * d, 0, W - 24 * d, ALTO_BARRA * d};
        if (testo.empty()) v.testo(testi::t(L"Cerca app, file, impostazioni…", L"Search apps, files, settings…"), fTesto, rt, tenue);
        else v.testo(testo, fTesto, rt, ink);
        if (cursore) {
            const float cx = rt.left + (testo.empty() ? 0 : std::min(rt.right - rt.left, v.larghezza(testo, fTesto))) + 1.5f * d;
            v.pen->SetColor(D2D1::ColorF(0.25f, 0.6f, 1.0f, 1));
            v.rt->FillRectangle({cx, ly - 13 * d, cx + 2 * d, ly + 13 * d}, v.pen);
        }
        if (!risultati.empty()) {
            v.pen->SetColor(v.scuro ? D2D1::ColorF(1, 1, 1, 0.10f) : D2D1::ColorF(0, 0, 0, 0.08f));
            v.rt->FillRectangle({0, ALTO_BARRA * d, W, ALTO_BARRA * d + 1}, v.pen);
        }
        const float rr = std::max(8.0f, raggio::vetro() - 10.0f) * d;
        for (int i = 0; i < (int)risultati.size(); i++) {
            const Risultato& r = risultati[i];
            const float y0 = (ALTO_BARRA + 10 + i * ALTO_RIGA) * d;
            const D2D1_RECT_F riga = {8 * d, y0, W - 8 * d, y0 + ALTO_RIGA * d};
            const bool s = i == sel;
            if (s) {
                v.pen->SetColor(D2D1::ColorF(0.04f, 0.52f, 1.0f, 1));
                v.rt->FillRoundedRectangle({riga, std::min(rr, ALTO_RIGA * d / 2), std::min(rr, ALTO_RIGA * d / 2)}, v.pen);
            }
            const D2D1_COLOR_F c = s ? D2D1::ColorF(1, 1, 1, 1) : ink, c2 = s ? D2D1::ColorF(1, 1, 1, 0.75f) : tenue;
            // icona
            const float ix = riga.left + 12 * d, iy = y0 + (ALTO_RIGA - 30) * d / 2, il = 30 * d;
            auto image=bitmapFor(r.icona);
            if(image) v.rt->DrawBitmap(image, {ix, iy, ix + il, iy + il});
            else simbolo(r.tipo, ix + il / 2, iy + il / 2, c);
            const float tx = ix + il + 12 * d;
            v.testo(r.titolo, fTitolo, {tx, y0 + 5 * d, riga.right - 12 * d, y0 + 26 * d}, c);
            v.testo(r.sotto, fSotto, {tx, y0 + 25 * d, riga.right - 12 * d, y0 + 43 * d}, c2);
        }
        v.finisci();
    }

    void simbolo(Tipo t, float cx, float cy, D2D1_COLOR_F c) {
        const float d = v.dpi;
        v.pen->SetColor({c.r, c.g, c.b, c.a * 0.85f});
        switch (t) {
            case T_CALCOLO:
                v.rt->DrawRoundedRectangle({{cx - 9 * d, cy - 11 * d, cx + 9 * d, cy + 11 * d}, 3 * d, 3 * d}, v.pen, 1.6f * d);
                v.rt->DrawLine({cx - 5 * d, cy - 5 * d}, {cx + 5 * d, cy - 5 * d}, v.pen, 1.6f * d);
                for (int i = 0; i < 2; i++) for (int j = 0; j < 3; j++) v.rt->FillEllipse({{cx - 4.5f * d + j * 4.5f * d, cy + 1 * d + i * 5 * d}, 1.2f * d, 1.2f * d}, v.pen);
                break;
            case T_WEB:
                v.rt->DrawEllipse({{cx, cy}, 10 * d, 10 * d}, v.pen, 1.6f * d);
                v.rt->DrawEllipse({{cx, cy}, 4.5f * d, 10 * d}, v.pen, 1.4f * d);
                v.rt->DrawLine({cx - 10 * d, cy}, {cx + 10 * d, cy}, v.pen, 1.4f * d);
                break;
            case T_IMPOSTAZIONE:
                v.rt->DrawEllipse({{cx, cy}, 6 * d, 6 * d}, v.pen, 2.2f * d);
                for (int i = 0; i < 8; i++) {
                    const float a = i * 3.14159265f / 4 + 0.39f, co = std::cos(a), si = std::sin(a);
                    v.rt->DrawLine({cx + co * 7 * d, cy + si * 7 * d}, {cx + co * 9.5f * d, cy + si * 9.5f * d}, v.pen, 2.6f * d);
                }
                break;
            default:   // file o app senza icona
                v.rt->DrawRoundedRectangle({{cx - 8 * d, cy - 11 * d, cx + 8 * d, cy + 11 * d}, 2.5f * d, 2.5f * d}, v.pen, 1.6f * d);
        }
    }

    // Settings titles use the shared translation table; search aliases also cover
    // Vietnamese concepts, folded through the same accent-insensitive matcher.
    static const std::vector<Risultato>& pagine() {
        static const wchar_t* t[][5] = {
                {L"Schermo", L"Display", L"display risoluzione monitor scala resolution scale", L"ms-settings:display", L"màn hình độ phân giải tỷ lệ hiển thị màn dọc"},
                {L"Audio", L"Sound", L"suono volume microfono altoparlanti audio microphone speakers", L"ms-settings:sound", L"âm thanh âm lượng loa micro thiết bị phát ghi âm"},
                {L"Notifiche", L"Notifications", L"avvisi non disturbare alerts do not disturb", L"ms-settings:notifications", L"thông báo không làm phiền cảnh báo"},
                {L"Bluetooth e dispositivi", L"Bluetooth & devices", L"cuffie mouse tastiera headphones keyboard", L"ms-settings:bluetooth", L"bluetooth thiết bị tai nghe kết nối không dây"},
                {L"Wi-Fi", L"Wi-Fi", L"wireless rete internet network", L"ms-settings:network-wifi", L"wifi wi-fi mạng không dây internet"},
                {L"Rete e Internet", L"Network & internet", L"ethernet connessione proxy vpn connection", L"ms-settings:network-status", L"mạng internet kết nối ethernet bộ điều hợp proxy vpn"},
                {L"Personalizzazione", L"Personalization", L"aspetto appearance", L"ms-settings:personalization", L"cá nhân hóa giao diện diện mạo"},
                {L"Sfondo", L"Background", L"wallpaper desktop sfondo", L"ms-settings:personalization-background", L"hình nền nền màn hình ảnh nền"},
                {L"Colori e tema scuro", L"Colors and dark mode", L"modalità scura chiara colore dark light theme", L"ms-settings:colors", L"màu sắc chế độ tối sáng chủ đề"},
                {L"App installate", L"Installed apps", L"disinstalla programmi uninstall programs", L"ms-settings:appsfeatures", L"ứng dụng đã cài gỡ cài đặt chương trình"},
                {L"App predefinite", L"Default apps", L"browser apri con open with", L"ms-settings:defaultapps", L"ứng dụng mặc định trình duyệt mở bằng"},
                {L"App all'avvio", L"Startup apps", L"esecuzione automatica autostart", L"ms-settings:startupapps", L"ứng dụng khởi động cùng windows tự khởi động"},
                {L"Account", L"Accounts", L"utente profilo user profile", L"ms-settings:yourinfo", L"tài khoản người dùng hồ sơ thông tin cá nhân"},
                {L"Data e ora", L"Date & time", L"orologio fuso orario clock time zone", L"ms-settings:dateandtime", L"ngày giờ thời gian múi giờ đồng hồ"},
                {L"Lingua e area geografica", L"Language & region", L"italiano inglese formato english format", L"ms-settings:regionlanguage", L"ngôn ngữ khu vực tiếng việt định dạng vùng"},
                {L"Tastiera e digitazione", L"Keyboard & typing", L"layout tastiera keyboard", L"ms-settings:typing", L"bàn phím gõ nhập liệu bố cục"},
                {L"Mouse", L"Mouse", L"puntatore velocità touchpad pointer speed", L"ms-settings:mousetouchpad", L"chuột con trỏ tốc độ bàn di chuột"},
                {L"Energia", L"Power", L"batteria sospensione spegnimento battery sleep", L"ms-settings:powersleep", L"nguồn điện pin ngủ tắt màn hình tiết kiệm năng lượng"},
                {L"Archiviazione", L"Storage", L"spazio disco pulizia disk space cleanup", L"ms-settings:storagesense", L"lưu trữ dung lượng ổ đĩa dọn dẹp"},
                {L"Windows Update", L"Windows Update", L"aggiornamenti updates", L"ms-settings:windowsupdate", L"cập nhật windows bản vá"},
                {L"Sicurezza di Windows", L"Windows Security", L"antivirus defender firewall", L"windowsdefender:", L"bảo mật windows chống vi-rút tường lửa"},
                {L"Privacy e sicurezza", L"Privacy & security", L"autorizzazioni fotocamera posizione permissions camera location", L"ms-settings:privacy", L"quyền riêng tư bảo mật quyền truy cập camera vị trí"},
                {L"Stampanti e scanner", L"Printers & scanners", L"stampa print", L"ms-settings:printers", L"máy in máy quét in ấn"},
                {L"Informazioni sul sistema", L"About", L"pc specifiche nome versione specs name version", L"ms-settings:about", L"giới thiệu hệ thống thông số tên máy phiên bản"},
                {L"Gioco", L"Gaming", L"game bar modalità gioco game mode", L"ms-settings:gaming-gamebar", L"trò chơi chế độ chơi game"},
                {L"Accessibilità", L"Accessibility", L"testo grande contrasto narratore large text contrast narrator", L"ms-settings:easeofaccess", L"trợ năng cỡ chữ tương phản trình tường thuật"},
                {L"Pannello di controllo", L"Control Panel", L"classico classic", L"control", L"bảng điều khiển cổ điển"},
                {L"Gestione attività", L"Task Manager", L"processi task manager processes", L"taskmgr", L"trình quản lý tác vụ tiến trình hiệu suất bộ nhớ"},
                {L"Impostazioni di TLauncher", L"TLauncher settings", L"dock gelatina genio panoramica jelly genie overview", L"*tlauncher-native", L"cài đặt tlauncher thanh dock hình nền stage spotlight hiệu ứng phím tắt"},
        };
        static std::vector<Risultato> p[testi::N_LINGUE];
        std::vector<Risultato>& v = p[testi::attuale()];
        if (v.empty()) for (auto& x : t) v.push_back({T_IMPOSTAZIONE, testi::t(x[0], x[1]), std::wstring(x[2])+L" "+x[4], x[3], L""});
        return v;
    }

    // ---- thread: file dall'indice di ricerca di Windows e icone ----
    static DWORD WINAPI lavoratore(LPVOID p) {
        std::unique_ptr<std::shared_ptr<Work>> job(static_cast<std::shared_ptr<Work>*>(p));auto state=*job;
        const HRESULT com=CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED);
        for(;;){
            std::wstring q;IconJob icon{};LONG queryGeneration=-1;
            {std::lock_guard<std::mutex> lock(state->mutex);
                if(state->queryGeneration>=0){q=std::move(state->query);queryGeneration=state->queryGeneration;state->queryGeneration=-1;}
                else if(!state->icons.empty()){icon=std::move(state->icons.front());state->icons.pop_front();}
                else break;
            }
            try{
                if(queryGeneration>=0){auto result=cercaFile(q,8);std::lock_guard<std::mutex> lock(state->mutex);
                    if(queryGeneration==state->generation){state->files=std::move(result);state->fileGeneration=queryGeneration;}}
                else {
                    std::shared_ptr<const appicons::Pixels> pixels;
                    int width=0,height=0;
                    std::vector<UINT> unused;
                    appicons::load(icon.source,icon.size,width,height,unused,pixelIcona,&pixels);
                    std::lock_guard<std::mutex> lock(state->mutex);
                    if(icon.generation==ULONGLONG(state->generation)&&state->ready.size()<256)
                        state->ready.push_back({std::move(icon),std::move(pixels)});
                }
            }catch(...){/* A failing Shell provider cannot retain or touch UI state. */}
        }
        if(SUCCEEDED(com))CoUninitialize();
        // Mark idle only after COM teardown; UI can then enqueue exactly one new
        // worker. Pending jobs that arrived during teardown are started by timer 4.
        {std::lock_guard<std::mutex> lock(state->mutex);state->running=false;}
        state->idle.notify_all();
        return 0;
    }

    // file e cartelle dall'indice di Windows (indice.h, condiviso con il launcher): tutti i luoghi indicizzati, senza
    // AppData, cartelle nascoste e menu Start. Prima solo la cartella dell'utente
    static std::vector<Risultato> cercaFile(const std::wstring& q, int quanti) {
        std::vector<Risultato> out;
        for (auto& f : indice::cerca(q, quanti, true)) out.push_back({T_FILE, f.nome, f.cartella, f.percorso, f.percorso});
        return out;
    }

    // pixel dell'icona (premoltiplicati) dalla shell
    static bool pixelIcona(const std::wstring& percorso, int lato, int& w, int& h, std::vector<UINT>& px) {
        IShellItemImageFactory* fab = nullptr;
        if (FAILED(SHCreateItemFromParsingName(percorso.c_str(), nullptr, IID_PPV_ARGS(&fab)))) return false;
        HBITMAP hb = nullptr;
        const HRESULT hr = fab->GetImage({lato, lato}, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK, &hb);
        fab->Release();
        if (FAILED(hr) || !hb) return false;
        BITMAP bm{}; GetObject(hb, sizeof(bm), &bm);
        w = bm.bmWidth; h = bm.bmHeight;
        px.assign((size_t)w * h, 0);
        BITMAPINFO bi{}; bi.bmiHeader = {sizeof(BITMAPINFOHEADER), w, -h, 1, 32, BI_RGB};
        HDC scr = GetDC(nullptr); GetDIBits(scr, hb, 0, h, px.data(), &bi, DIB_RGB_COLORS); ReleaseDC(nullptr, scr);
        DeleteObject(hb);
        bool alfa = false;
        for (UINT p : px) if (p >> 24) { alfa = true; break; }
        for (UINT& p : px) {
            const UINT a = alfa ? (p >> 24) : 255;
            p = (a << 24) | ((((p >> 16) & 255) * a / 255) << 16) | ((((p >> 8) & 255) * a / 255) << 8) | ((p & 255) * a / 255);
        }
        return true;
    }
};

}  // namespace ricerca
