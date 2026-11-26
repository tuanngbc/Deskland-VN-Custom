// Anteprime delle finestre di un'app sopra la sua icona del dock (issue #24: "sarebbe bello vedere le anteprime delle
// app"). Mouse fermo mezzo secondo su un'icona con finestre aperte: sopra compaiono, una per finestra, delle schede di
// vetro separate con il titolo, una "x" per chiuderla e la miniatura dal vivo di DWM (DwmRegisterThumbnail, la stessa
// della panoramica: nessuna finestra vera viene toccata). La scheda sotto il mouse si alza un poco, come un tasto
// pronto a essere premuto (richiesta dell'utente: prima era un unico vetro con le anteprime dentro). Clic su una
// scheda: quella finestra davanti. Le schede non prendono il fuoco (l'app in uso resta attiva). Quando aprirle e
// chiuderle lo decide il dock (dock.h, controllaAnteprime).
// Ogni scheda e' una finestra di vetro sua (acrilico, angoli e ombra di Windows); le finestre si creano la prima volta
// e poi si riusano. Le finestre ridotte a icona non hanno una miniatura (DWM non la disegna): al suo posto la scritta.
#pragma once
#include "tlauncher_identity.h"
#include "vetro.h"
#include "testi.h"
#include <dwmapi.h>
#include <windowsx.h>
#include <vector>
#include <string>
#include <memory>
#include <functional>
#include <algorithm>

namespace anteprime {

class Anteprime {
public:
    std::function<void(HWND)> scegli;           // clic su una scheda: porta davanti quella finestra
    std::function<void(HWND)> chiudiFinestra;   // clic sulla "x": la chiude (con l'animazione di chiusura)

    bool aperta() const { return mostrata && !chiudendo; }
    bool mouseSopra() const { for (const auto& s : schede) if (s->dentro) return true; return false; }
    int icona() const { return iconaDi; }        // indice dell'icona del dock per cui sono aperte (-1 nessuna)

    // Apre (o rifa' per un'altra icona) dal lato interno del dock, accanto all'icona (rettangolo a riposo, coordinate dello
    // schermo; cima: il bordo interno del dock, una y o ai lati una x; latoDock_: vedi latoDock) e oltre il quale stanno le schede,
    // dentro l'area di lavoro, con le finestre nell'ordine dato (al massimo quelle che
    // entrano in larghezza). dietro: la finestra delle icone del dock, sotto la quale le schede escono e rientrano
    void apri(const std::vector<HWND>& finestre, RECT icona, LONG cima, RECT area, int indiceIcona, HWND dietro, int latoDock_ = 0) {
        for (auto& s : schede) s->togliMiniatura();
        // (cima: per il dock ai lati e' una x, il bordo interno; l'ancora sta li', all'altezza dell'icona)
        const POINT ancora = latoDock_ >= 2 ? POINT{cima, (icona.top + icona.bottom) / 2} : POINT{(icona.left + icona.right) / 2, cima};
        iconaR = icona; hDietro = dietro;
        const HMONITOR mon = MonitorFromPoint(ancora, MONITOR_DEFAULTTONEAREST);
        dpi = vetro::dpiMonitor(mon);
        // quante entrano: n schede larghe largoScheda() con n-1 spazi, piu' 4 px per lato (xGiusta). Prima si contava
        // LARGO senza i bordi: 7 schede a 125% su 1920 px, larghe 1980 (R20261119-205727-S4)
        // (ai lati in colonna: quante ne entrano in altezza)
        latoDock = std::clamp(latoDock_, 0, 3);
        const int spazio = latoDock >= 2 ? (int)(area.bottom - area.top) : (int)(area.right - area.left);
        const int entrano = std::max(1, (int)((spazio - 8 + GAP * dpi) / (lungoScheda() + GAP * dpi)));
        n = 0;
        for (HWND h : finestre) {
            if (n >= std::min(entrano, MAX)) break;
            if (!IsWindow(h)) continue;
            if (n >= (int)schede.size()) schede.push_back(std::make_unique<Scheda>());
            Scheda& s = *schede[n];
            if (!s.v.fin && !s.v.crea(L"TLauncher.Native.Anteprima", vetro::procedura<Anteprime>, this, false)) { schede.pop_back(); break; }
            s.v.aggiornaTema();
            s.h = h;
            wchar_t t[256] = L""; GetWindowTextW(h, t, 256);   // (testo interno: per le finestre di un altro processo nessun messaggio)
            s.titolo = t;
            s.ridotta = IsIconic(h) != FALSE;
            if (!s.ridotta && FAILED(DwmRegisterThumbnail(s.v.fin, h, &s.th))) s.th = nullptr;
            // proporzioni: quelle della finestra (normale, se e' ridotta)
            RECT r{};
            if (s.ridotta) { WINDOWPLACEMENT wp{sizeof(wp)}; if (GetWindowPlacement(h, &wp)) r = wp.rcNormalPosition; }
            else GetWindowRect(h, &r);
            const float fw = (float)std::max(1L, r.right - r.left), fh = (float)std::max(1L, r.bottom - r.top);
            // (un poco rientrata nella scheda, con lo stesso margine attorno: prima la toccava ai lati, richiesta dell'utente)
            const float sc = std::min((LARGO - 2 * RIENTRO) * dpi / fw, (ALTO - 2 * RIENTRO) * dpi / fh);
            s.mw = fw * sc; s.mh = fh * sc;
            s.dentro = false; s.sopraX = false; s.su = 0;
            s.xPronta = false;   // (posizione da prendere subito, senza scivolare)
            n++;
        }
        if (n == 0) return;
        if (!fTitolo || fDpi != dpi) {
            if (fTitolo) fTitolo->Release();
            fTitolo = schede[0]->v.formato(12.0f * dpi, DWRITE_FONT_WEIGHT_SEMI_BOLD);
            fDpi = dpi;
        }
        // (ai lati la fila e' una colonna: ancoraX, areaL e areaR sono y, y0 e' la x delle schede)
        const bool vert = latoDock >= 2;
        ancoraX = vert ? ancora.y : ancora.x; areaL = vert ? area.top : area.left; areaR = vert ? area.bottom : area.right;
        switch (latoDock) {
            case 1: y0 = std::min((int)area.bottom - 4 - altoScheda(), (int)ancora.y + (int)(12 * dpi)); break;
            case 2: y0 = std::min((int)area.right - 4 - largoScheda(), (int)ancora.x + (int)(12 * dpi)); break;
            case 3: y0 = std::max((int)area.left + 4, (int)ancora.x - (int)(12 * dpi) - largoScheda()); break;
            default: y0 = std::max((int)area.top + 4, (int)ancora.y - (int)(12 * dpi) - altoScheda());
        }
        iconaDi = indiceIcona;
        mostrata = true; chiudendo = false;
        g = std::min(g, 1.0);   // (riaperte mentre si chiudevano: si riparte da dov'erano)
        t0 = vetro::secondi() - g * durata();
        tPasso = vetro::secondi();
        for (int i = (int)schede.size() - 1; i >= n; i--) schede[i]->nascondi();   // (quelle in piu' di prima)
        aggiorna();
        for (int i = 0; i < n; i++) ShowWindow(schede[i]->v.fin, SW_SHOWNOACTIVATE);
        avvia();
    }

    // chiusura animata: lo stesso percorso dell'apertura all'indietro (le schede si restringono in quadrati che
    // rientrano dietro l'icona), l'ultima uscita per prima
    void chiudi() {
        if (!mostrata || chiudendo) return;
        chiudendo = true;
        t0 = vetro::secondi() - (1 - g) * durata();
        avvia();
    }

    // Le schede delle finestre che non ci sono piu' (distrutte, o nascoste da app che si chiudono nell'area di notifica)
    // se ne vanno, le altre scivolano al loro posto; nessuna rimasta: si chiudono. La chiama il dock a ogni controllo
    // (80 ms) mentre sono aperte. E' anche l'unico modo in cui la "x" toglie una scheda: la toglie la finestra che se
    // ne va davvero, non il clic, cosi' se l'app chiede "Salvare?" e si annulla la scheda resta (R20261119-205727-S1, S2)
    void riconcilia() {
        if (!mostrata || chiudendo) return;
        bool cambiate = false;
        for (int i = n - 1; i >= 0; i--) {
            const HWND h = schede[i]->h;
            if (!IsWindow(h) || !IsWindowVisible(h)) togli(h);
            else if ((IsIconic(h) != FALSE) != schede[i]->ridotta) {
                // ridotta o ripristinata con la scheda aperta: miniatura al posto della scritta (o il contrario), invece
                // dello stato preso all'apertura (R20261121-001416-S16)
                Scheda& s = *schede[i];
                s.ridotta = !s.ridotta;
                if (s.ridotta) s.togliMiniatura();
                else if (FAILED(DwmRegisterThumbnail(s.v.fin, h, &s.th))) s.th = nullptr;
                cambiate = true;
            }
            if (!mostrata || chiudendo) return;   // (togli ha chiuso l'ultima)
        }
        if (cambiate) aggiorna();
    }

private:
    void togli(HWND h) {
        for (int i = 0; i < n; i++) if (schede[i]->h == h) {
            schede[i]->togliMiniatura();
            schede[i]->nascondi();
            auto via = std::move(schede[i]);
            schede.erase(schede.begin() + i);
            schede.push_back(std::move(via));   // (la finestra resta, in fondo, per la prossima volta)
            n--;
            if (n == 0) { chiudi(); return; }
            avvia();
            return;
        }
    }

public:
    bool messaggio(HWND w, UINT m, WPARAM wp, LPARAM l, LRESULT& r) {
        Scheda* s = nullptr; int i = -1;
        for (int j = 0; j < (int)schede.size(); j++) if (schede[j]->v.fin == w) { s = schede[j].get(); i = j; }
        if (!s) return false;
        switch (m) {
            case WM_TIMER: if (wp == 1) passo(); return true;
            case WM_PAINT: disegna(*s); return true;
            case WM_MOUSEMOVE: {
                if (!s->dentro) { TRACKMOUSEEVENT tm{sizeof(tm), TME_LEAVE, w, 0}; TrackMouseEvent(&tm); s->dentro = true; avvia(); }
                const bool sx = s->piena && inX(*s, (float)GET_X_LPARAM(l), (float)GET_Y_LPARAM(l));
                if (sx != s->sopraX) { s->sopraX = sx; disegna(*s); }
                return true;
            }
            case WM_MOUSELEAVE: s->dentro = false; s->sopraX = false; avvia(); disegna(*s); return true;
            case WM_LBUTTONUP: {
                if (chiudendo || i >= n) return true;
                const HWND h = s->h;
                // (la x solo a scheda intera: mentre cresce le misure non sono quelle di rettX)
                if (s->piena && inX(*s, (float)GET_X_LPARAM(l), (float)GET_Y_LPARAM(l))) { if (chiudiFinestra) chiudiFinestra(h); }   // (la scheda: riconcilia)
                else { chiudi(); if (scegli) scegli(h); }
                return true;
            }
            case WM_MOUSEACTIVATE: r = MA_NOACTIVATE; return true;   // (il clic non toglie il fuoco all'app in uso)
            case WM_CLOSE: chiudi(); return true;
        }
        return false;
    }

    ~Anteprime() { for (auto& s : schede) s->togliMiniatura(); if (fTitolo) fTitolo->Release(); }

private:
    static constexpr float LARGO = 200, ALTO = 130, TESTA = 30, PAD = 8, GAP = 12, SALITA = 7;
    static constexpr float RIENTRO = 10;   // margine della miniatura dentro il suo spazio (LARGO x ALTO)
    static constexpr int MAX = 8;
    // Comparsa (richiesta dell'utente): ogni scheda spunta da dietro l'icona come un quadratino, sale e intanto si
    // allarga fino alla sua forma; la scomparsa e' lo stesso percorso all'indietro. Una scheda dura CORSA, la successiva
    // parte SFALSA dopo (si aprono a ventaglio)
    static constexpr double CORSA = 0.30, SFALSA = 0.035;
    struct Scheda {
        vetro::Finestra v;
        HWND h = nullptr; HTHUMBNAIL th = nullptr;
        std::wstring titolo;
        bool ridotta = false, dentro = false, sopraX = false;
        bool piena = false;   // alla sua misura finale (la x si clicca solo cosi')
        float mw = 0, mh = 0;
        double su = 0;    // 0..1: quanto e' alzata (mouse sopra), inseguito con una molla morbida
        double x = 0;     // posizione orizzontale che si vede (insegue quella giusta quando una scheda se ne va)
        bool xPronta = false;   // x non ancora presa: prima era x < 0, ma sugli schermi a sinistra le x sono negative (R20261121-001416-S17)
        // contenuto disegnato alla misura finale e rimpicciolito nella finestra di adesso (sc, tx, ty), con opacita' op
        float sc = 1, tx = 0, ty = 0, op = 1;
        void togliMiniatura() { if (th) DwmUnregisterThumbnail(th); th = nullptr; }
        void nascondi() { if (v.fin) ShowWindow(v.fin, SW_HIDE); dentro = false; sopraX = false; piena = false; }
    };
    std::vector<std::unique_ptr<Scheda>> schede;   // le prime n sono quelle mostrate
    int n = 0;
    IDWriteTextFormat* fTitolo = nullptr;
    float fDpi = 0, dpi = 1;
    int y0 = 0, iconaDi = -1;
    int latoDock = 0;          // lato del dock (0 basso, 1 alto, 2 sinistra, 3 destra): le schede escono dal lato interno
    LONG ancoraX = 0, areaL = 0, areaR = 0;
    RECT iconaR{};             // l'icona a riposo, da dove escono le schede
    HWND hDietro = nullptr;    // la finestra delle icone del dock: sotto di lei mentre escono e rientrano
    bool mostrata = false, chiudendo = false;
    double g = 0;              // 0..1: avanzamento lineare di tutta la comparsa (all'indietro mentre si chiudono)
    double t0 = 0, tPasso = 0;

    double durata() const { return CORSA + std::max(0, n - 1) * SFALSA; }
    int largoScheda() const { return (int)std::lround((LARGO + 2 * PAD) * dpi); }
    int altoScheda() const { return (int)std::lround((PAD + TESTA + ALTO + PAD) * dpi); }

    // posizione giusta della scheda i lungo la fila: centrate sull'icona, dentro l'area di lavoro (ai lati, in colonna:
    // ancoraX, areaL e areaR sono allora y)
    int lungoScheda() const { return latoDock >= 2 ? altoScheda() : largoScheda(); }   // lungo la fila (ai lati: in colonna)
    double xGiusta(int i) const {
        const double tot = n * lungoScheda() + (n - 1) * GAP * dpi;
        const double x0 = std::clamp((double)ancoraX - tot / 2, (double)areaL + 4, std::max((double)areaL + 4, (double)areaR - 4 - tot));
        return x0 + i * (lungoScheda() + GAP * dpi);
        return x0 + i * (largoScheda() + GAP * dpi);
    }
    D2D1_RECT_F rettX() const { return {(LARGO + 2 * PAD - 30) * dpi, (PAD - 2) * dpi, (LARGO + 2 * PAD - 6) * dpi, (PAD + 22) * dpi}; }
    bool inX(const Scheda&, float x, float y) const { const D2D1_RECT_F r = rettX(); return x >= r.left && x < r.right && y >= r.top && y < r.bottom; }

    // Animazione al ritmo dello schermo, sempre sulla stessa finestra (la prima scheda creata): le schede cambiano
    // ordine quando una se ne va, e un aggancio sulla "prima" di oggi non si spegnerebbe piu' sulla prima di ieri
    HWND hBattito = nullptr;
    void avvia() { if (!hBattito && !schede.empty()) hBattito = schede[0]->v.fin; if (hBattito) vetro::battito(hBattito, 1); }

    // un fotogramma: comparsa o scomparsa, schede alzate sotto il mouse, schede che scivolano al loro posto
    void passo() {
        const double ora = vetro::secondi(), t = ora - t0;
        const double dt = std::clamp(ora - tPasso, 1.0 / 360, 1.0 / 30);
        tPasso = ora;
        g = std::clamp(chiudendo ? 1 - t / durata() : t / durata(), 0.0, 1.0);
        bool muove = chiudendo || g < 1;
        for (int i = 0; i < n; i++) {
            Scheda& s = *schede[i];
            const double suObj = s.dentro && !chiudendo ? 1.0 : 0.0;
            s.su = suObj + (s.su - suObj) * std::exp(-dt / 0.06);
            if (std::abs(s.su - suObj) < 0.004) s.su = suObj; else muove = true;
            const double xObj = xGiusta(i);
            if (!s.xPronta || std::abs(s.x - xObj) < 0.5) { s.x = xObj; s.xPronta = true; } else { s.x = xObj + (s.x - xObj) * std::exp(-dt / 0.05); muove = true; }
        }
        aggiorna();
        if (chiudendo && g <= 0) {
            vetro::fermaBattito(hBattito, 1);
            for (auto& s : schede) { s->togliMiniatura(); s->nascondi(); }
            mostrata = false; chiudendo = false; iconaDi = -1; n = 0;
            return;
        }
        if (!muove) vetro::fermaBattito(hBattito, 1);
    }

    static double liscia(double x) { x = std::clamp(x, 0.0, 1.0); return x * x * (3 - 2 * x); }

    void aggiorna() {
        const int LW = largoScheda(), LH = altoScheda();
        // quadrato di partenza: grande quanto l'icona, al suo centro, nascosto dietro di lei. Piu' piccolo (sotto ~45 px)
        // il vetro di Windows lo disegnava come un disco chiaro con un rombo scuro dentro (ripresa a 30 fps)
        // Con gli angoli rotondi di Windhawk (raggio 24) un quadrato largo circa 2 raggi diventa un disco: almeno 2,6 raggi
        const double lato = std::max({8.0, (double)std::min(iconaR.right - iconaR.left, iconaR.bottom - iconaR.top),
                                      2.6 * raggio::vetro() * dpi});
        const double icx = (iconaR.left + iconaR.right) / 2.0, icy = (iconaR.top + iconaR.bottom) / 2.0;
        for (int i = 0; i < n; i++) {
            Scheda& s = *schede[i];
            // (xPronta, non il segno di x: sugli schermi a sinistra x e' negativa, e una scheda riusata all'apertura
            // per un'altra app teneva la x positiva di prima per un fotogramma, R20261121-115814-S3 e S17)
            if (!s.xPronta) { s.x = xGiusta(i); s.xPronta = true; }
            // avanzamento di questa scheda (le altre partono un poco dopo; all'indietro rientra prima l'ultima)
            const double r = std::clamp((g * durata() - i * SFALSA) / CORSA, 0.0, 1.0);
            const double sale = vetro::uscita(r);                               // dal centro dell'icona al suo posto
            // prima il quadrato sale senza cambiare misura (cambiarla a ogni fotogramma fa arrivare il contenuto un
            // fotogramma dopo, e accanto al quadrato piccolo si vedeva una falce di vetro chiaro), poi si allarga
            const double cresce = vetro::uscita(std::clamp((r - 0.35) / 0.65, 0.0, 1.0));
            const int alza = (int)std::lround(SALITA * dpi * s.su);
            // (dal lato interno del dock: sopra, sotto, a destra o a sinistra; s.x e' la posizione lungo la fila)
            double fx = s.x + LW / 2.0, fy = y0 - alza + LH / 2.0;            // centro della scheda intera
            if (latoDock == 1) fy = y0 + alza + LH / 2.0;
            else if (latoDock >= 2) { fx = y0 + (latoDock == 2 ? alza : -alza) + LW / 2.0; fy = s.x + LH / 2.0; }
            const double w = lato + (LW - lato) * cresce, h = lato + (LH - lato) * cresce;
            const bool vert = latoDock >= 2;   // (ai lati la scheda esce di traverso subito e si allarga lungo la colonna)
            const double cx = icx + (fx - icx) * (vert ? sale : cresce), cy = icy + (fy - icy) * (vert ? cresce : sale);
            const int iw = std::max(1, (int)std::lround(w)), ih = std::max(1, (int)std::lround(h));
            s.piena = r >= 1 && iw == LW && ih == LH;
            // dietro le icone mentre esce o rientra; intera, in cima (la finestra del dock e' piu' alta delle icone e,
            // davanti, prenderebbe i clic sulle schede che le stanno sopra)
            s.v.posiziona((int)std::lround(cx - w / 2), (int)std::lround(cy - h / 2), iw, ih,
                          s.piena || !hDietro || !IsWindow(hDietro) ? HWND_TOPMOST : hDietro);
            s.v.alfa(r / 0.12);   // (compare subito: all'inizio e' nascosta dall'icona)
            s.sc = std::min((float)iw / LW, (float)ih / LH);
            s.tx = (iw - LW * s.sc) / 2; s.ty = (ih - LH * s.sc) / 2;
            s.op = (float)liscia((cresce - 0.55) / 0.4);   // il contenuto compare quando la scheda e' quasi intera
            if (s.th) {
                const float mx = (LARGO / 2 + PAD) * dpi, my = (PAD + TESTA + ALTO / 2) * dpi;
                DWM_THUMBNAIL_PROPERTIES p{};
                p.dwFlags = DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY | DWM_TNP_SOURCECLIENTAREAONLY;
                p.rcDestination = {(LONG)std::lround(s.tx + (mx - s.mw / 2) * s.sc), (LONG)std::lround(s.ty + (my - s.mh / 2) * s.sc),
                                   (LONG)std::lround(s.tx + (mx + s.mw / 2) * s.sc), (LONG)std::lround(s.ty + (my + s.mh / 2) * s.sc)};
                p.fVisible = s.op > 0.01f; p.opacity = (BYTE)std::lround(255 * std::clamp(s.op, 0.0f, 1.0f)); p.fSourceClientAreaOnly = FALSE;
                DwmUpdateThumbnailProperties(s.th, &p);
            }
            disegna(s);
        }
    }

    void disegna(Scheda& s) {
        if (!mostrata || !s.v.fin || !IsWindowVisible(s.v.fin) || !s.v.inizia()) return;
        const float d = dpi;
        // velo un po' piu' chiaro (o scuro) quando la scheda e' alzata: si vede che e' quella che si sceglie
        if (s.su > 0.01) {
            s.v.pen->SetColor(s.v.scuro ? D2D1::ColorF(1, 1, 1, 0.07f * (float)s.su) : D2D1::ColorF(1, 1, 1, 0.25f * (float)s.su));
            s.v.rt->FillRectangle({0, 0, (float)s.v.W, (float)s.v.H}, s.v.pen);
        }
        if (s.op > 0.01f) {
            // il contenuto alla misura finale, rimpicciolito nella scheda che cresce, con la sua opacita'
            s.v.rt->SetTransform(D2D1::Matrix3x2F::Scale(s.sc, s.sc) * D2D1::Matrix3x2F::Translation(s.tx, s.ty));
            s.v.rt->PushLayer(D2D1::LayerParameters(D2D1::InfiniteRect(), nullptr, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                                                    D2D1::IdentityMatrix(), s.op), nullptr);
            s.v.testo(s.titolo.empty() ? std::wstring(L"—") : s.titolo, fTitolo, {(PAD + 4) * d, PAD * d, (LARGO + 2 * PAD - 34) * d, (PAD + TESTA) * d}, s.v.inchiostro());
            // "x": cerchio rosso quando il mouse e' sopra
            const D2D1_RECT_F rx = rettX();
            const float cx = (rx.left + rx.right) / 2, cy = (rx.top + rx.bottom) / 2;
            if (s.sopraX) { s.v.pen->SetColor(D2D1::ColorF(0.95f, 0.30f, 0.25f, 0.95f)); s.v.rt->FillEllipse({{cx, cy}, 10 * d, 10 * d}, s.v.pen); }
            s.v.pen->SetColor(s.sopraX ? D2D1::ColorF(1, 1, 1, 1) : s.v.inchiostro(0.7f));
            s.v.rt->DrawLine({cx - 4 * d, cy - 4 * d}, {cx + 4 * d, cy + 4 * d}, s.v.pen, 1.6f * d);
            s.v.rt->DrawLine({cx + 4 * d, cy - 4 * d}, {cx - 4 * d, cy + 4 * d}, s.v.pen, 1.6f * d);
            // finestra ridotta: niente miniatura di DWM, un riquadro con la scritta
            if (!s.th) {
                const float mx = (LARGO / 2 + PAD) * d, my = (PAD + TESTA + ALTO / 2) * d;
                const D2D1_RECT_F m = {mx - s.mw / 2, my - s.mh / 2, mx + s.mw / 2, my + s.mh / 2};
                s.v.pen->SetColor(s.v.scuro ? D2D1::ColorF(1, 1, 1, 0.08f) : D2D1::ColorF(0, 0, 0, 0.05f));
                s.v.rt->FillRoundedRectangle({m, 8 * d, 8 * d}, s.v.pen);
                s.v.testo(s.ridotta ? testi::t(L"Ridotta a icona", L"Minimized") : testi::t(L"Anteprima non disponibile", L"No preview"),
                          fTitolo, {m.left + 10 * d, m.top, m.right - 10 * d, m.bottom}, s.v.inchiostro(0.6f));
            }
            s.v.rt->PopLayer();
            s.v.rt->SetTransform(D2D1::Matrix3x2F::Identity());
        }
        s.v.finisci();
    }
};

}  // namespace anteprime
