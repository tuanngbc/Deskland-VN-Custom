// Indicatore del volume al posto di quello di Windows: una pillola di vetro che scende in alto al centro mentre
// si premono i tasti del volume, poi sparisce. I tasti li intercetta il gancio di tastiera di FluidWin (che cambia
// il volume da se'): quello di Windows non compare piu'. Se FluidWin si chiude il gancio sparisce e torna quello
// di Windows.
#pragma once
#include "vetro.h"
#include <functional>

namespace osd {

class Indicatore {
public:
    double speed = 0.70;
    bool animate = true;
    std::wstring mutedLabel = L"Muted";
    std::wstring noticeText;
    int noticePercent = -1;
    bool noticeCharging = false;
    bool noticeHeadphones = false;
    std::function<void()> onHidden;
    HWND window() const { return v.fin; }
    bool inizializza() {
        return v.crea(L"FluidWin.Volume", vetro::procedura<Indicatore>, this, false);
    }

    // livello 0..1; muto. Ricomincia il conto alla rovescia se e' gia' a schermo
    void mostra(float livello, bool muto, const RECT* target = nullptr) {
        if (!v.fin) return;
        KillTimer(v.fin, 2);
        livelloObj = livello; this->muto = muto;
        const ULONGLONG ora = vetro::ms();
        if (stato == 0) {   // da nascosto: entra
            v.aggiornaTema();
            const RECT a = target ? *target : vetro::areaPrincipale();
            v.dpi = vetro::dpiMonitor(MonitorFromPoint({a.left, a.top}, MONITOR_DEFAULTTOPRIMARY));
            // (rifatto se la scala e' cambiata, per esempio il principale ora al 150%: R20261121-120306 su S18)
            if (fmt && fDpi != v.dpi) { fmt->Release(); fmt = nullptr; }
            if (!fmt) { fmt = v.formato(13.5f * v.dpi, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_TRAILING); fDpi = v.dpi; }
            const int available=std::max(1,a.right-a.left-(int)std::lround(28*v.dpi));
            const int desired=(int)std::ceil(noticeText.empty()?300*v.dpi:std::max(372*v.dpi,v.larghezza(noticeText,fmt)+64*v.dpi));
            const int w=std::min(available,desired),h=(int)std::lround(52*v.dpi);
            x=(a.left+a.right-w)/2;y=a.top+(int)std::lround(14*v.dpi);
            v.posiziona(x,y-(int)std::lround(12*v.dpi),w,h,HWND_TOPMOST,SWP_SHOWWINDOW);
            livelloVis = livello;
            t0 = ora; stato = 1;
            vetro::battito(v.fin, 1);
        } else if (stato == 3) { stato = 1; t0 = ora - (ULONGLONG)(k * 180 / speed); }   // stava uscendo: torna
        tFine = ora + (noticeText.empty() ? 1400 : 2500);
        if (!animate) { stato = 2; k = 1; livelloVis = livelloObj; v.alfa(1); SetWindowPos(v.fin, HWND_TOPMOST, x, y, 0, 0, SWP_NOACTIVATE | SWP_NOSIZE); }
        vetro::battito(v.fin, 1);
        disegna();
    }

    void chiudi() { if (v.fin) { KillTimer(v.fin, 2); vetro::fermaBattito(v.fin, 1); } if (fmt) fmt->Release(); fmt = nullptr; v.chiudi(); }

    bool messaggio(HWND, UINT m, WPARAM w, LPARAM, LRESULT& r) {
        if (m == WM_TIMER && w == 1) { passo(); r = 0; return true; }
        if (m == WM_TIMER && w == 2) { KillTimer(v.fin, 2); vetro::battito(v.fin, 1); passo(); r = 0; return true; }
        if (m == WM_PAINT) { disegna(); r = 0; return true; }
        if (m == WM_MOUSEACTIVATE) { r = MA_NOACTIVATE; return true; }
        if (m == WM_NCHITTEST) { r = HTTRANSPARENT; return true; }
        if (m == WM_CLOSE) { KillTimer(v.fin, 2); vetro::fermaBattito(v.fin, 1); stato = 0; ShowWindow(v.fin, SW_HIDE); if (onHidden) onHidden(); r = 0; return true; }
        return false;
    }

private:
    vetro::Finestra v;
    IDWriteTextFormat* fmt = nullptr;
    float fDpi = 0;   // scala a cui e' fatto fmt
    int stato = 0;   // 0 nascosto, 1 entra, 2 fermo, 3 esce
    ULONGLONG t0 = 0, tFine = 0;
    double k = 0;
    float livelloObj = 0, livelloVis = 0;
    bool muto = false;
    int x = 0, y = 0;

    void passo() {
        const ULONGLONG ora = vetro::ms();
        if (stato == 1) { k = std::min(1.0, elapsed(t0) / 180.0); if (k >= 1) stato = 2; }
        else if (stato == 2 && ora >= tFine) { stato = 3; t0 = ora; }
        else if (stato == 3) {
            k = 1 - std::min(1.0, elapsed(t0) / 220.0);
            if (k <= 0) { stato = 0; vetro::fermaBattito(v.fin, 1); ShowWindow(v.fin, SW_HIDE); if (onHidden) onHidden(); return; }
        }
        livelloVis += (livelloObj - livelloVis) * 0.35f;   // la barra scorre fino al valore nuovo
        const double e = vetro::uscita(k);
        SetWindowPos(v.fin, HWND_TOPMOST, x, y - (int)std::lround((1 - e) * 12 * v.dpi), 0, 0, SWP_NOACTIVATE | SWP_NOSIZE);
        v.alfa(e);
        disegna();
        if (stato == 2 && std::abs(livelloVis - livelloObj) < 0.002f && ora < tFine) {
            vetro::fermaBattito(v.fin, 1); SetTimer(v.fin, 2, (UINT)std::max<ULONGLONG>(1, tFine - ora), nullptr);
        }
    }

    double elapsed(ULONGLONG start) const { return animate ? double(vetro::ms() - start) * speed : 1e9; }

    void disegna() {
        if (!v.inizia()) return;
        const float d = v.dpi, W = (float)v.W, H = (float)v.H, cy = H / 2;
        const D2D1_COLOR_F ink = v.inchiostro();
        if (!noticeText.empty()) {
            if (fmt) fmt->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
            if (noticeHeadphones) {
                // Original notch.h::cuffie, hosted independently of the Notch.
                const float cx=24*d;v.pen->SetColor(ink);
                ID2D1PathGeometry* g=nullptr;ID2D1GeometrySink* s=nullptr;
                if (SUCCEEDED(v.f2d->CreatePathGeometry(&g)) && SUCCEEDED(g->Open(&s))) {
                    s->BeginFigure({cx-7*d,cy+2*d},D2D1_FIGURE_BEGIN_HOLLOW);
                    s->AddArc(D2D1::ArcSegment({cx+7*d,cy+2*d},{7*d,7*d},0,D2D1_SWEEP_DIRECTION_CLOCKWISE,D2D1_ARC_SIZE_SMALL));
                    s->EndFigure(D2D1_FIGURE_END_OPEN);s->Close();s->Release();v.rt->DrawGeometry(g,v.pen,1.6f*d);
                }
                if(g)g->Release();
                v.rt->FillRoundedRectangle({{cx-8.5f*d,cy+.5f*d,cx-4.5f*d,cy+7.5f*d},1.5f*d,1.5f*d},v.pen);
                v.rt->FillRoundedRectangle({{cx+4.5f*d,cy+.5f*d,cx+8.5f*d,cy+7.5f*d},1.5f*d,1.5f*d},v.pen);
                v.testo(noticeText,fmt,{48*d,0,W-16*d,H},ink);v.finisci();return;
            }
            // Original notch.h::batteriaSimbolo geometry, with the shared glass
            // ink for a light theme. Independent utility; no Notch is created.
            const float cx = 24 * d, w = 18 * d, h = 9 * d, x = cx - w / 2 - d, y = cy - h / 2;
            auto outline = ink; outline.a *= .55f; v.pen->SetColor(outline);
            v.rt->DrawRoundedRectangle({{x,y,x+w,y+h},2.5f*d,2.5f*d},v.pen,1.2f*d);
            v.rt->FillRoundedRectangle({{x+w+d,cy-2*d,x+w+2.6f*d,cy+2*d},.8f*d,.8f*d},v.pen);
            const float k = std::clamp(noticePercent < 0 ? .5f : noticePercent / 100.f, .08f, 1.f);
            v.pen->SetColor(noticeCharging ? D2D1::ColorF(.2f,.84f,.29f,1) : noticePercent>=0&&noticePercent<=20 ? D2D1::ColorF(1,.27f,.23f,1) : ink);
            v.rt->FillRoundedRectangle({{x+2*d,y+2*d,x+2*d+(w-4*d)*k,y+h-2*d},1.2f*d,1.2f*d},v.pen);
            v.testo(noticeText,fmt,{48*d,0,W-16*d,H},ink);v.finisci();return;
        }
        // altoparlante
        const float sx = 22 * d;
        v.pen->SetColor(ink);
        // (Open puo' fallire anche con la geometria creata: R20261121-002056-S17)
        ID2D1PathGeometry* g = nullptr; ID2D1GeometrySink* sg = nullptr;
        if (SUCCEEDED(v.f2d->CreatePathGeometry(&g)) && SUCCEEDED(g->Open(&sg))) {
            sg->BeginFigure({sx - 6 * d, cy - 3.5f * d}, D2D1_FIGURE_BEGIN_FILLED);
            const D2D1_POINT_2F p[] = {{sx - 2 * d, cy - 3.5f * d}, {sx + 3 * d, cy - 8 * d}, {sx + 3 * d, cy + 8 * d}, {sx - 2 * d, cy + 3.5f * d}, {sx - 6 * d, cy + 3.5f * d}};
            sg->AddLines(p, 5); sg->EndFigure(D2D1_FIGURE_END_CLOSED); sg->Close(); sg->Release();
            v.rt->FillGeometry(g, v.pen);
        }
        if (g) g->Release();
        const float liv = muto ? 0.0f : livelloVis;
        if (muto) {
            v.rt->DrawLine({sx + 7 * d, cy - 4 * d}, {sx + 15 * d, cy + 4 * d}, v.pen, 1.8f * d);
            v.rt->DrawLine({sx + 15 * d, cy - 4 * d}, {sx + 7 * d, cy + 4 * d}, v.pen, 1.8f * d);
        } else {
            for (int i = 0; i < 2; i++) {
                if (liv < (i == 0 ? 0.01f : 0.5f)) break;
                const float r = (i == 0 ? 6.5f : 11.0f) * d;
                ID2D1PathGeometry* a = nullptr;
                if (FAILED(v.f2d->CreatePathGeometry(&a))) break;
                ID2D1GeometrySink* s = nullptr;
                if (FAILED(a->Open(&s))) { a->Release(); break; }
                s->BeginFigure({sx + 3 * d + r * 0.6f, cy - r * 0.8f}, D2D1_FIGURE_BEGIN_HOLLOW);
                s->AddArc(D2D1::ArcSegment({sx + 3 * d + r * 0.6f, cy + r * 0.8f}, {r, r}, 0, D2D1_SWEEP_DIRECTION_CLOCKWISE, D2D1_ARC_SIZE_SMALL));
                s->EndFigure(D2D1_FIGURE_END_OPEN); s->Close(); s->Release();
                v.rt->DrawGeometry(a, v.pen, 1.8f * d); a->Release();
            }
        }
        // barra
        const float labelWidth = muto && fmt ? std::clamp(v.larghezza(mutedLabel, fmt) + 8 * d, 42 * d, 100 * d) : 42 * d;
        const D2D1_RECT_F b = {56 * d, cy - 4 * d, W - labelWidth - 22 * d, cy + 4 * d};
        v.pen->SetColor(v.scuro ? D2D1::ColorF(1, 1, 1, 0.18f) : D2D1::ColorF(0, 0, 0, 0.12f));
        v.rt->FillRoundedRectangle({b, 4 * d, 4 * d}, v.pen);
        if (liv > 0.005f) {
            v.pen->SetColor(v.scuro ? D2D1::ColorF(1, 1, 1, 0.95f) : D2D1::ColorF(0.1f, 0.1f, 0.12f, 0.85f));
            v.rt->FillRoundedRectangle({{b.left, b.top, b.left + std::max(8 * d, (b.right - b.left) * liv), b.bottom}, 4 * d, 4 * d}, v.pen);
        }
        wchar_t t[16]; swprintf(t, 16, L"%d", (int)std::lround(livelloObj * 100));
        v.testo(muto ? mutedLabel : std::wstring(t), fmt, {W - labelWidth - 18 * d, 0, W - 18 * d, H}, ink);
        v.finisci();
    }
};

}  // namespace osd
