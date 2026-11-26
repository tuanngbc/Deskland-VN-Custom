// Affiancamento di FluidWin al posto dello Snap di Windows: trascinando una finestra sul bordo in alto si ingrandisce,
// sul bordo a sinistra o a destra prende meta' schermo, negli angoli un quarto; Win+Sinistra/Destra fanno lo stesso
// da tastiera. Cosi' ogni ingrandimento o affiancamento chiesto dall'utente passa da FluidWin PRIMA di succedere, e
// l'animazione e' sempre preparata (copertura con la foto, poi il comando: vedi preparaZoom in fluidwin.cpp).
// Lo Snap di Windows si spegne solo mentre FluidWin e' aperto e solo per la sessione: SPI_SETWINARRANGING senza
// SPIF_UPDATEINIFILE (l'impostazione salvata non cambia; dopo un riavvio del PC torna da sola). Prima di spegnerlo si
// scrive il segno SnapDaRimettere in HKCU\Software\FluidWin: lo rimettono l'uscita, il guardiano e l'avvio dopo un
// crash (ripristinaSeServe in ripristinaTutte). Le finestre delle app avviate come amministratore FluidWin non le puo'
// spostare (Windows non lo permette a un programma normale): con lo Snap spento non si affiancano (scelta dell'utente).
// Staccata da una casella, la finestra resta con la forma della casella (niente "misura di prima": scelta dell'utente).
#pragma once
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <windowsx.h>
#include <cmath>
#include <algorithm>
#include <functional>
#include <vector>
#include <string>
#include "vetro.h"
#include "../snap_accessibility.h"

namespace affianca {

inline std::function<void(const char*)>& registro() { static std::function<void(const char*)> r; return r; }
inline void scrivi(const char* s) { if (registro()) registro()(s); }

// ---- Snap di Windows ----
#ifndef TLAUNCHER_EMBEDDED_SNAP
inline const wchar_t* CHIAVE = L"Software\\FluidWin";
inline bool& spentoDaNoi() { static bool b = false; return b; }

// lettura che distingue "non si sa" da "spento" (R20261116-011042-S1): false se la lettura non riesce
inline bool leggiSnap(bool& acceso) { BOOL b = TRUE; if (!SystemParametersInfoW(SPI_GETWINARRANGING, 0, &b, 0)) return false; acceso = b != FALSE; return true; }

// spegne lo Snap di Windows (solo per la sessione) ricordandosi di rimetterlo. true: lo Snap e' spento (anche se lo
// era gia'), quindi l'affiancamento di FluidWin puo' prendere il suo posto; false: resta acceso, FluidWin no
// (R20261121-001054-S15). Stato iniziale non leggibile: non si tocca niente. Prima "non si sa" contava come acceso, e
// uno Snap spento dall'utente veniva poi riacceso all'uscita (R20261121-003154-S1)
inline bool spegniSnap() {
    // (niente scorciatoia su spentoDaNoi: un rimettiSnap riuscito ma non riletto lo lascia vero con lo Snap acceso, e
    // FluidWin affiancava insieme a Windows; si rilegge sempre, R20261121-115445 conferma di S15)
    bool prima = true;
    if (!leggiSnap(prima)) { scrivi("affianca: stato dello Snap di Windows non leggibile, resta com'e'"); return false; }
    if (!prima) return true;
    const DWORD uno = 1;
    // prima il segno: se FluidWin cade subito dopo, il guardiano sa cosa rimettere
    if (RegSetKeyValueW(HKEY_CURRENT_USER, CHIAVE, L"SnapDaRimettere", REG_DWORD, &uno, sizeof(uno)) != ERROR_SUCCESS) { scrivi("affianca: segno non scritto, lo Snap di Windows resta acceso"); return false; }
    // (il valore va in pvParam, come dice la documentazione di SPI_SETWINARRANGING; si mette anche in uiParam, che
    // misurato Windows legge pure: R20261116-010212-S1)
    bool acceso = true;
    const bool fatto = SystemParametersInfoW(SPI_SETWINARRANGING, FALSE, (PVOID)(UINT_PTR)FALSE, SPIF_SENDCHANGE) != FALSE;
    const bool letto = fatto && leggiSnap(acceso);
    // spento, o spento senza poterlo rileggere: si tiene il segno (meglio rimettere una volta di troppo che mai)
    if (fatto && (!letto || !acceso)) { spentoDaNoi() = true; scrivi(letto ? "affianca: Snap di Windows spento (solo per la sessione)" : "affianca: Snap di Windows spento (stato non riletto, segno tenuto)"); return true; }
    RegDeleteKeyValueW(HKEY_CURRENT_USER, CHIAVE, L"SnapDaRimettere"); scrivi("affianca: Snap di Windows non spento");
    return false;
}
// lo rimette se l'aveva spento FluidWin (anche un FluidWin di prima caduto: c'e' il segno)
inline void rimettiSnap() {
    DWORD v = 0, n = sizeof(v);
    const bool segno = RegGetValueW(HKEY_CURRENT_USER, CHIAVE, L"SnapDaRimettere", RRF_RT_REG_DWORD, nullptr, &v, &n) == ERROR_SUCCESS && v;
    if (!segno && !spentoDaNoi()) return;
    // il segno si toglie solo se lo Snap risulta davvero acceso dopo (riletto)
    bool acceso = false;
    if (SystemParametersInfoW(SPI_SETWINARRANGING, TRUE, (PVOID)(UINT_PTR)TRUE, SPIF_SENDCHANGE) && leggiSnap(acceso) && acceso) {
        RegDeleteKeyValueW(HKEY_CURRENT_USER, CHIAVE, L"SnapDaRimettere");
        spentoDaNoi() = false;
        scrivi("affianca: Snap di Windows rimesso");
    } else scrivi("affianca: Snap di Windows non rimesso (riprova al prossimo giro)");
}

#endif // Embedded layout UI never compiles the global Snap-policy writers.
// ---- caselle ----
enum Zona { NESSUNA = 0, PIENO, SINISTRA, DESTRA, ALTO_SX, ALTO_DX, BASSO_SX, BASSO_DX, CENTRO };

// rettangolo visibile della casella z sullo schermo m (area di lavoro)
inline RECT rettZona(HMONITOR m, Zona z) {
    MONITORINFO mi{}; mi.cbSize = sizeof(mi);
    if (!GetMonitorInfoW(m, &mi)) return {};
    const RECT a = mi.rcWork;
    const LONG cx = (a.left + a.right) / 2, cy = (a.top + a.bottom) / 2;
    switch (z) {
        case PIENO:    return a;
        case SINISTRA: return {a.left, a.top, cx, a.bottom};
        case DESTRA:   return {cx, a.top, a.right, a.bottom};
        case ALTO_SX:  return {a.left, a.top, cx, cy};
        case ALTO_DX:  return {cx, a.top, a.right, cy};
        case BASSO_SX: return {a.left, cy, cx, a.bottom};
        case BASSO_DX: return {cx, cy, a.right, a.bottom};
        case CENTRO: {   // finestra "staccata" da tastiera: al centro, 60% x 75%
            const LONG w = (a.right - a.left) * 6 / 10, h = (a.bottom - a.top) * 3 / 4;
            return {cx - w / 2, cy - h / 2, cx - w / 2 + w, cy - h / 2 + h};
        }
        default: return {};
    }
}

// in quale casella sta adesso una finestra (rettangolo visibile, tolleranza di qualche pixel); NESSUNA se in nessuna
inline Zona zonaDi(const RECT& v, HMONITOR m) {
    for (Zona z : {SINISTRA, DESTRA, ALTO_SX, ALTO_DX, BASSO_SX, BASSO_DX}) {
        const RECT r = rettZona(m, z);
        if (std::abs(r.left - v.left) <= 6 && std::abs(r.top - v.top) <= 6 && std::abs(r.right - v.right) <= 6 && std::abs(r.bottom - v.bottom) <= 6) return z;
    }
    return NESSUNA;
}

// un altro schermo subito oltre quel punto? (sui bordi in comune il mouse passa di la': niente casella)
inline bool oltre(POINT p) { return MonitorFromPoint(p, MONITOR_DEFAULTTONULL) != nullptr; }

// casella sotto il mouse mentre si trascina: bordo in alto -> pieno (negli angoli in alto: quarti), bordi laterali ->
// meta' (vicino agli angoli: quarti). Solo bordi veri dello schermo, non quelli in comune con un altro
inline Zona zonaDalPunto(POINT p, HMONITOR& m) {
    m = MonitorFromPoint(p, MONITOR_DEFAULTTONULL);
    MONITORINFO mi{}; mi.cbSize = sizeof(mi);
    if (!m || !GetMonitorInfoW(m, &mi)) return NESSUNA;
    const RECT s = mi.rcMonitor;
    const float d = vetro::dpiMonitor(m);
    const LONG angolo = (LONG)std::lround(72 * d), bordo = 2;
    const bool su = p.y <= s.top + bordo - 1 && !oltre({p.x, s.top - 1});
    const bool sx = p.x <= s.left + bordo - 1 && !oltre({s.left - 1, p.y});
    const bool dx = p.x >= s.right - bordo && !oltre({s.right, p.y});
    if (su) {
        if (p.x < s.left + angolo) return ALTO_SX;
        if (p.x >= s.right - angolo) return ALTO_DX;
        return PIENO;
    }
    if (sx) return p.y < s.top + angolo ? ALTO_SX : p.y >= s.bottom - angolo ? BASSO_SX : SINISTRA;
    if (dx) return p.y < s.top + angolo ? ALTO_DX : p.y >= s.bottom - angolo ? BASSO_DX : DESTRA;
    return NESSUNA;
}

// ---- anteprima: vetro che mostra dove andra' la finestra (molla verso la casella, sfuma uscendo) ----
class Anteprima {
public:
    // mostra (o sposta) l'anteprima sulla casella r; 'da' e' il punto da cui cresce la prima volta
    void verso(const RECT& r, POINT da, HWND sotto) {
        if (!fin.fin && !crea()) return;
        if (!acceso) {   // nasce piccola attorno al mouse
            x = da.x - 20; y = da.y - 20; w = 40; h = 40; vx = vy = vw = vh = 0; k = 0;
            fin.aggiornaTema();
        }
        obj = r; acceso = true; uscendo = false; sottoDi = sotto;
        vetro::battito(fin.fin, 1);
    }
    void togli() { if (acceso && !uscendo) { uscendo = true; vetro::battito(fin.fin, 1); } }
    void subito() { acceso = false; uscendo = false; if (fin.fin) { vetro::fermaBattito(fin.fin, 1); ShowWindow(fin.fin, SW_HIDE); } }

    bool messaggio(HWND, UINT m, WPARAM w, LPARAM, LRESULT& r) {
        r = 0;
        if (m == WM_TIMER && w == 1) { passo(); return true; }
        if (m == WM_PAINT) { disegna(); return true; }
        return false;
    }

private:
    vetro::Finestra fin;
    bool acceso = false, uscendo = false;
    RECT obj{};
    double x = 0, y = 0, w = 0, h = 0, vx = 0, vy = 0, vw = 0, vh = 0, k = 0, tPrec = 0;
    HWND sottoDi = nullptr;

    bool crea() {
        if (!fin.crea(L"FluidWin.Affianca", vetro::procedura<Anteprima>, this, false)) return false;
        // trasparente al mouse: il trascinamento continua sotto
        SetWindowLongPtrW(fin.fin, GWL_EXSTYLE, GetWindowLongPtrW(fin.fin, GWL_EXSTYLE) | WS_EX_TRANSPARENT);
        tPrec = vetro::secondi();
        return true;
    }
    static bool molla(double& v, double& vel, double obj, double dt) {
        const double kk = 520, c = 2 * std::sqrt(kk) * 0.86;
        vel += (-kk * (v - obj) - c * vel) * dt; v += vel * dt;
        if (std::fabs(v - obj) < 0.4 && std::fabs(vel) < 4) { v = obj; vel = 0; return false; }
        return true;
    }
    void passo() {
        if (!acceso) { vetro::fermaBattito(fin.fin, 1); return; }   // (battito arrivato dopo la chiusura: vedi Riquadro::passo)
        const double t = vetro::secondi(), dt = std::clamp(t - tPrec, 0.001, 0.034); tPrec = t;
        bool ancora = false;
        if (uscendo) {   // si stringe verso il centro e sfuma
            k = std::max(0.0, k - dt / 0.16);
            if (k <= 0) { subito(); return; }
            ancora = true;
        } else {
            k = std::min(1.0, k + dt / 0.14); if (k < 1) ancora = true;
            ancora |= molla(x, vx, (double)obj.left, dt); ancora |= molla(y, vy, (double)obj.top, dt);
            ancora |= molla(w, vw, (double)(obj.right - obj.left), dt); ancora |= molla(h, vh, (double)(obj.bottom - obj.top), dt);
        }
        // margine interno di 8 punti (come i layout di Windows 11), e uscendo si rimpicciolisce del 6%
        const double m = 8 * fin.dpi, s = uscendo ? (0.94 + 0.06 * k) : 1.0;
        const double cx = x + w / 2, cy = y + h / 2, ww = std::max(8.0, (w - 2 * m) * s), hh = std::max(8.0, (h - 2 * m) * s);
        // sotto la finestra trascinata (e sotto la gelatina, che sta nell'overlay di FluidWin)
        fin.posiziona((int)std::lround(cx - ww / 2), (int)std::lround(cy - hh / 2), (int)std::lround(ww), (int)std::lround(hh),
                      sottoDi && IsWindow(sottoDi) ? sottoDi : HWND_TOPMOST, SWP_SHOWWINDOW);
        fin.alfa(k);
        disegna();
        if (!ancora) vetro::fermaBattito(fin.fin, 1);
    }
    void disegna() {
        if (!fin.inizia(0.6f)) return;
        // un velo azzurro e un bordo chiaro sopra il vetro
        fin.pen->SetColor(fin.scuro ? D2D1::ColorF(0.35f, 0.6f, 1.0f, 0.16f) : D2D1::ColorF(0.1f, 0.45f, 1.0f, 0.12f));
        fin.rt->FillRectangle({0, 0, (float)fin.W, (float)fin.H}, fin.pen);
        fin.pen->SetColor(fin.scuro ? D2D1::ColorF(1, 1, 1, 0.35f) : D2D1::ColorF(1, 1, 1, 0.7f));
        const float r = fin.raggio();
        fin.rt->DrawRoundedRectangle({{1, 1, (float)fin.W - 1, (float)fin.H - 1}, r, r}, fin.pen, 1.5f * fin.dpi);
        fin.finisci();
    }
};

// ---- riquadro dei layout (al posto di quello di Windows 11 sul tasto ingrandisci, e Win+Z) ----
// Disposizioni in millesimi dell'area di lavoro: meta' e meta', due terzi e un terzo, tre colonne, quattro quarti
struct Disposizione { int n; RECT p[4]; };
inline const Disposizione* disposizioni(int& quante) {
    static const Disposizione d[] = {
        {2, {{0, 0, 500, 1000}, {500, 0, 1000, 1000}}},
        {2, {{0, 0, 667, 1000}, {667, 0, 1000, 1000}}},
        {3, {{0, 0, 333, 1000}, {333, 0, 667, 1000}, {667, 0, 1000, 1000}}},
        {4, {{0, 0, 500, 500}, {500, 0, 1000, 500}, {0, 500, 500, 1000}, {500, 500, 1000, 1000}}},
    };
    quante = (int)(sizeof(d) / sizeof(d[0]));
    return d;
}
// rettangolo visibile di una casella (millesimi) sull'area di lavoro dello schermo m
inline RECT rettMillesimi(HMONITOR m, const RECT& p) {
    MONITORINFO mi{}; mi.cbSize = sizeof(mi);
    if (!GetMonitorInfoW(m, &mi)) return {};
    const RECT a = mi.rcWork; const LONG w = a.right - a.left, h = a.bottom - a.top;
    return {a.left + w * p.left / 1000, a.top + h * p.top / 1000, a.left + w * p.right / 1000, a.top + h * p.bottom / 1000};
}

// Il riquadro: vetro con le disposizioni in miniatura; la casella sotto il mouse si accende, il clic la sceglie.
// Si chiude scegliendo, con il mouse fuori dal riquadro e dal tasto per un attimo, con un clic altrove o con Esc
class Riquadro {
public:
    double motionSpeed=0.70;bool reducedMotion=false,abandoned=false;
    std::wstring panelTitle=L"Snap layouts",actionLabel=L"Choose";std::vector<std::wstring> labels;
    HWND window()const{return fin.fin;}
    ~Riquadro(){
        {std::lock_guard<std::mutex> guard(access->mutex);access->state.window=nullptr;access->state.selected=-1;}
        if(provider)provider->Release();if(fin.fin){vetro::fermaBattito(fin.fin,1);SetWindowLongPtrW(fin.fin,GWLP_USERDATA,0);}fin.chiudi();
    }
    void poll(){passo();}
    void key(WPARAM key){LRESULT result=0;messaggio(fin.fin,WM_KEYDOWN,key,0,result);}
    std::function<void(HWND, const RECT&, HMONITOR)> scelto;   // finestra, casella (rettangolo visibile), schermo

    // per la finestra h, appeso sotto il punto 'ancora' (centro in basso del tasto); 'tasto': zona in cui il mouse
    // puo' restare senza che si chiuda (il tasto ingrandisci). daTastiera: Win+Z (si chiude solo con Esc o un clic)
    void mostra(HWND h, POINT ancora, const RECT& tasto, bool daTastiera) {
        if (!fin.fin && !crea()) return;
        finestra = h; zonaTasto = tasto; tastiera = daTastiera;
        mon = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
        fin.aggiornaTema();
        fin.dpi = vetro::dpiMonitor(MonitorFromPoint(ancora, MONITOR_DEFAULTTONEAREST));
        const float d = fin.dpi;
        int quante = 0; disposizioni(quante);
        w = (int)std::lround((2 * MARGINE + quante * CARTA_L + (quante - 1) * SPAZIO) * d);
        hh = (int)std::lround((2 * MARGINE + CARTA_A) * d);
        MONITORINFO mi{}; mi.cbSize = sizeof(mi);
        if (!GetMonitorInfoW(MonitorFromPoint(ancora, MONITOR_DEFAULTTONEAREST), &mi)) return;
        // (riquadro piu' largo dello schermo, verticale ad alta scala: il limite alto mai sotto quello basso, perche'
        // std::clamp con i limiti invertiti non e' definito, R20261121-003154-S10)
        x0 = std::clamp((int)ancora.x - w / 2, (int)mi.rcWork.left + 4, std::max((int)mi.rcWork.left + 4, (int)mi.rcWork.right - w - 4));
        y0 = std::clamp((int)ancora.y + (int)std::lround(6 * d), (int)mi.rcWork.top + 4, std::max((int)mi.rcWork.top + 4, (int)mi.rcWork.bottom - hh - 4));
        abandoned=false;keyboardSelection=daTastiera;sopraD = sopraC = -1; for (auto& l : luce) l = 0;
        k = 0; uscendo = false; aperto = true; tFuori = 0; t0 = tPrec = clock();
        if(daTastiera){sopraD=sopraC=0;announce();}
        posa();
        ShowWindow(fin.fin, SW_SHOW);SetForegroundWindow(fin.fin);SetFocus(fin.fin);
        vetro::battito(fin.fin, 1);
        if (registro()) registro()("affianca: riquadro dei layout aperto");
    }
    void chiudi() { if (aperto && !uscendo) { uscendo = true; tOut = clock(); vetro::battito(fin.fin, 1); } }
    bool aperta() const { return aperto; }
    HWND di() const { return finestra; }

    bool messaggio(HWND, UINT m, WPARAM wp, LPARAM l, LRESULT& r) {
        r = 0;
        switch (m) {
            case WM_CLOSE:chiudi();return true;
            case WM_GETOBJECT:if(static_cast<LONG>(l)==OBJID_CLIENT){if(aperto&&!uscendo&&fin.fin){if(!provider)provider=new(std::nothrow)tlauncher_snap_access::Accessible(access);if(provider)r=LresultFromObject(IID_IAccessible,wp,provider);}return true;}break;
            case tlauncher_snap_access::SELECT:case tlauncher_snap_access::CHOOSE:{
                if(l!=reinterpret_cast<LPARAM>(access.get())||!aperto||uscendo)return true;
                int n=0;auto ds=disposizioni(n);int index=int(wp);if(index<0||index>=11)return true;sopraD=0;
                while(index>=ds[sopraD].n)index-=ds[sopraD++].n;sopraC=index;keyboardSelection=true;
                announce();vetro::battito(fin.fin,1);if(m==tlauncher_snap_access::CHOOSE)choose(sopraD,sopraC);return true;
            }
            case WM_ACTIVATE:if(LOWORD(wp)==WA_INACTIVE&&aperto){abandoned=true;chiudi();}return true;
            case WM_KEYDOWN:{
                if(wp==VK_ESCAPE){chiudi();return true;}
                int n=0;const auto ds=disposizioni(n);int count=0,index=0;
                for(int d=0;d<n;d++){if(d<sopraD)index+=ds[d].n;count+=ds[d].n;}index+=std::max(0,sopraC);
                if(wp==VK_RETURN||wp==VK_SPACE){choose(sopraD,sopraC);return true;}
                if(wp>=L'1'&&wp<=L'4'){sopraD=int(wp-L'1');sopraC=0;}
                else if(wp==VK_LEFT||wp==VK_RIGHT||wp==VK_TAB||wp==VK_UP||wp==VK_DOWN){
                    const int delta=wp==VK_LEFT||wp==VK_UP||(wp==VK_TAB&&(GetKeyState(VK_SHIFT)&0x8000))?-1:1;
                    index=(index+delta+count)%count;sopraD=0;while(index>=ds[sopraD].n){index-=ds[sopraD++].n;}sopraC=index;
                }else return false;
                keyboardSelection=true;announce();vetro::battito(fin.fin,1);return true;
            }
            case WM_TIMER: if (wp == 1) passo(); return true;
            case WM_MOUSEMOVE: {
                keyboardSelection=false;
                TRACKMOUSEEVENT te{sizeof(te), TME_LEAVE, fin.fin, 0}; TrackMouseEvent(&te);
                int dd = -1, cc = -1; casellaIn((float)GET_X_LPARAM(l), (float)GET_Y_LPARAM(l), dd, cc);
                if (dd != sopraD || cc != sopraC) { sopraD = dd; sopraC = cc;announce(); vetro::battito(fin.fin, 1); }
                return true;
            }
            case WM_MOUSELEAVE: if (!keyboardSelection&&sopraD != -1) { sopraD = sopraC = -1;announce(); vetro::battito(fin.fin, 1); } return true;
            case WM_LBUTTONUP: {
                int dd = -1, cc = -1; casellaIn((float)GET_X_LPARAM(l), (float)GET_Y_LPARAM(l), dd, cc);
                choose(dd,cc);
                return true;
            }
            case WM_PAINT: disegna(); return true;
        }
        return false;
    }

private:
    double clock()const{return vetro::secondi()*(reducedMotion?10000.0:std::clamp(motionSpeed,0.05,4.0));}
    void announce(){int n=0;auto ds=disposizioni(n);int index=0;for(int d=0;d<sopraD;d++)index+=ds[d].n;index+=std::max(0,sopraC);
        std::wstring name=panelTitle;if(sopraD>=0&&index<(int)labels.size())name+=L" - "+labels[index];if(fin.fin)SetWindowTextW(fin.fin,name.c_str());
        updateAccess();if(fin.fin&&sopraD>=0){NotifyWinEvent(EVENT_OBJECT_SELECTION,fin.fin,OBJID_CLIENT,index+1);NotifyWinEvent(EVENT_OBJECT_FOCUS,fin.fin,OBJID_CLIENT,index+1);}}
    void choose(int d,int c){int n=0;const auto ds=disposizioni(n);if(d<0||d>=n||c<0||c>=ds[d].n||uscendo)return;
        const RECT r=rettMillesimi(mon,ds[d].p[c]);chiudi();if(scelto&&IsWindow(finestra))scelto(finestra,r,mon);}
    static constexpr float MARGINE = 12, CARTA_L = 92, CARTA_A = 58, SPAZIO = 10, FESSURA = 3;
    vetro::Finestra fin;
    std::shared_ptr<tlauncher_snap_access::Model> access=std::make_shared<tlauncher_snap_access::Model>();
    tlauncher_snap_access::Accessible* provider=nullptr;
    HWND finestra = nullptr; HMONITOR mon = nullptr;
    RECT zonaTasto{};
    bool tastiera = false, aperto = false, uscendo = false,keyboardSelection=false;
    int w = 0, hh = 0, x0 = 0, y0 = 0, sopraD = -1, sopraC = -1;
    double k = 0, t0 = 0, tOut = 0, tPrec = 0, tFuori = 0;
    double luce[16] = {};   // per casella (disposizione*4 + casella): quanto e' accesa, 0..1

    bool crea() {
        if (!fin.crea(L"FluidWin.Layout", vetro::procedura<Riquadro>, this, true)) return false;
        return true;
    }
    D2D1_RECT_F carta(int i) const {
        const float d = fin.dpi, x = (MARGINE + i * (CARTA_L + SPAZIO)) * d, y = MARGINE * d;
        return {x, y, x + CARTA_L * d, y + CARTA_A * d};
    }
    D2D1_RECT_F casella(int i, int c) const {
        int quante = 0; const Disposizione* ds = disposizioni(quante);
        const D2D1_RECT_F r = carta(i); const float W = r.right - r.left, H = r.bottom - r.top, f = FESSURA * fin.dpi / 2;
        const RECT& p = ds[i].p[c];
        return {r.left + W * p.left / 1000 + f, r.top + H * p.top / 1000 + f, r.left + W * p.right / 1000 - f, r.top + H * p.bottom / 1000 - f};
    }
    void casellaIn(float x, float y, int& dd, int& cc) const {
        int quante = 0; const Disposizione* ds = disposizioni(quante);
        for (int i = 0; i < quante; i++) for (int c = 0; c < ds[i].n; c++) {
            const D2D1_RECT_F r = casella(i, c);
            if (x >= r.left && x < r.right && y >= r.top && y < r.bottom) { dd = i; cc = c; return; }
        }
    }
    void posa() {
        double e, a;
        if (uscendo) { const double u = std::clamp((clock() - tOut) / 0.14, 0.0, 1.0); e = 1 - u * u; a = 1 - u; }
        else { e = 1 - std::pow(1 - k, 3); a = std::min(1.0, k * 1.5); }
        fin.posiziona(x0, (int)std::lround(y0 - (1 - e) * 8 * fin.dpi), w, hh, HWND_TOPMOST, SWP_SHOWWINDOW);
        fin.alfa(a);
        updateAccess();
    }
    void updateAccess(){
        tlauncher_snap_access::Snapshot value;value.window=aperto&&!uscendo?fin.fin:nullptr;value.title=panelTitle;value.action=actionLabel;value.labels=labels;
        int n=0;auto ds=disposizioni(n);RECT window{};if(fin.fin)GetWindowRect(fin.fin,&window);
        int index=0;for(int d=0;d<n;d++)for(int c=0;c<ds[d].n;c++,index++){
            auto r=casella(d,c);value.regions.push_back({window.left+(LONG)std::floor(r.left),window.top+(LONG)std::floor(r.top),window.left+(LONG)std::ceil(r.right),window.top+(LONG)std::ceil(r.bottom)});
            if(value.window&&d==sopraD&&c==sopraC)value.selected=index;
        }
        std::lock_guard<std::mutex> guard(access->mutex);access->state=std::move(value);
    }
    void passo() {
        // gia' chiuso: un battito arrivato dopo (era gia' in viaggio) non lo deve rimettere a schermo. Misurato: dopo
        // il clic si chiudeva e a 154 ms ricompariva al suo posto
        if (!aperto) { vetro::fermaBattito(fin.fin, 1); return; }
        const double t = clock(), dt = std::clamp(t - tPrec, 0.001, 0.05); tPrec = t;
        bool ancora = false;
        bool repaint=uscendo||k<1;
        if (uscendo) {
            if (t - tOut >= 0.14) { aperto = uscendo = false; vetro::fermaBattito(fin.fin, 1); ShowWindow(fin.fin, SW_HIDE); return; }
            ancora = true;
        } else if (k < 1) { k = std::min(1.0, (t - t0) / 0.2); ancora = true; }
        int quante = 0; const Disposizione* ds = disposizioni(quante);
        for (int i = 0; i < quante; i++) for (int c = 0; c < ds[i].n; c++) {
            double& l = luce[i * 4 + c]; const double obj = (i == sopraD && c == sopraC) ? 1.0 : (i == sopraD ? 0.35 : 0.0);
            if (std::fabs(l - obj) > 0.01) { l += (obj - l) * std::min(1.0, dt * 18); ancora = repaint = true; } else l = obj;
        }
        // chiusura da sola: il mouse fuori dal riquadro e dal tasto per 0,35 s (non con Win+Z), o un clic altrove
        if (!uscendo) {
            POINT p; GetCursorPos(&p);
            RECT rr; GetWindowRect(fin.fin, &rr); InflateRect(&rr, 6, 6);
            const bool dentro = PtInRect(&rr, p) || PtInRect(&zonaTasto, p);
            const bool clic = ((GetAsyncKeyState(VK_LBUTTON) | GetAsyncKeyState(VK_RBUTTON)) & 0x8000) != 0;
            RECT solo; GetWindowRect(fin.fin, &solo);   // (un clic sul tasto ingrandisci lo chiude: il tasto fa il suo lavoro)
            if ((clic && !PtInRect(&solo, p)) || (GetAsyncKeyState(VK_ESCAPE) & 0x8000) || !IsWindow(finestra)) chiudi();
            else if (!tastiera) {
                if (dentro) tFuori = 0; else if (!tFuori) tFuori = t; else if (t - tFuori > 0.35) chiudi();
            }
            ancora = true;   // (finche' e' aperto si controlla il mouse a ogni fotogramma)
        }
        if(repaint||uscendo){posa();disegna();}
        else if(aperto){SetTimer(fin.fin,1,80,nullptr);}
        if (!ancora) vetro::fermaBattito(fin.fin, 1);
    }
    void disegna() {
        if (!fin.inizia()) return;
        const D2D1_COLOR_F blu = fin.scuro ? D2D1::ColorF(0.40f, 0.68f, 1.0f, 1) : D2D1::ColorF(0.0f, 0.42f, 0.95f, 1);
        int quante = 0; const Disposizione* ds = disposizioni(quante);
        const float r = 5 * fin.dpi;
        for (int i = 0; i < quante; i++) for (int c = 0; c < ds[i].n; c++) {
            const float l = (float)luce[i * 4 + c];
            const D2D1_COLOR_F base = fin.scuro ? D2D1::ColorF(1, 1, 1, 0.16f) : D2D1::ColorF(0, 0, 0, 0.10f);
            fin.pen->SetColor(D2D1::ColorF(base.r + (blu.r - base.r) * l, base.g + (blu.g - base.g) * l, base.b + (blu.b - base.b) * l, base.a + (1 - base.a) * l));
            fin.rt->FillRoundedRectangle({casella(i, c), r, r}, fin.pen);
        }
        fin.finisci();
    }
};

}  // namespace affianca
