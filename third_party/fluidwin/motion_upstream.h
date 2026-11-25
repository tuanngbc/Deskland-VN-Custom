// Copyright (c) 2026 Clooudy. MIT; see LICENSE in this directory.
// Extracted from genio.h and fluidwin.cpp at v1.3.1 (2f773a8733b84c7dbee88fff6628d14e9ac802b9).
// Keep the geometry, coordinates, timing and shader together for upstream comparisons.
#pragma once
#include <windows.h>
#include <algorithm>
#include <cmath>

namespace fluidwin_motion {
struct Vertice { float x, y, u, v; };

// Il dock puo' stare su ogni bordo (impostazione "Posizione"). Le curve verso il dock (genio, caduta) si calcolano
// sempre con il dock in basso, in un sistema "canonico" girato: lato 0 in basso (uguale allo schermo), 1 in alto
// (capovolto), 2 a sinistra, 3 a destra (girati di un quarto); poi i punti tornano sullo schermo
struct P2 { double x, y; };
inline P2 canonico(int lato, double x, double y) {
    switch (lato) {
        case 1: return {x, -y};
        case 2: return {y, -x};
        case 3: return {y, x};
        default: return {x, y};
    }
}
inline P2 dalCanonico(int lato, double x, double y) {   // l'inversa
    switch (lato) {
        case 1: return {x, -y};
        case 2: return {-y, x};
        case 3: return {y, x};
        default: return {x, y};
    }
}
inline RECT rettCanonico(int lato, const RECT& r) {
    const P2 a = canonico(lato, r.left, r.top), b = canonico(lato, r.right, r.bottom);
    return {(LONG)std::lround(std::min(a.x, b.x)), (LONG)std::lround(std::min(a.y, b.y)), (LONG)std::lround(std::max(a.x, b.x)), (LONG)std::lround(std::max(a.y, b.y))};
}
// frazioni (a da sinistra a destra, b dall'alto in basso) del sistema canonico -> coordinate della foto vera
inline void uvDalCanonico(int lato, float a, float b, float& u, float& v) {
    switch (lato) {
        case 1: u = a; v = 1 - b; break;
        case 2: u = 1 - b; v = a; break;
        case 3: u = b; v = a; break;
        default: u = a; v = b;
    }
}


struct Elemento { RECT finestra, icona; double p; int lato; };
static constexpr int RIGHE = 160;
static double morbido(double x) { x = std::clamp(x, 0.0, 1.0); return x * x * (3.0 - 2.0 * x); }
    static void griglia(const Elemento& e, Vertice* out) {
        const int lato = std::clamp(e.lato, 0, 3);
        const RECT fc = rettCanonico(lato, e.finestra), icc = rettCanonico(lato, e.icona);   // (vedi canonico)
        const double L = fc.left, T = fc.top, R = fc.right, B = fc.bottom;
        // la "bocca" e' meta' dell'icona: la finestra si stringe piu' a punta prima di entrarci
        const double ic = (icc.left + icc.right) / 2.0, mezza = (icc.right - icc.left) * 0.25;
        const double il = ic - mezza, ir = ic + mezza;
        double ty = icc.top;
        if (ty < B + 1) ty = B + 1;                         // la finestra tocca gia' la barra: la "bocca" e' appena sotto
        const double piega = morbido(e.p / 0.45);           // i lati si curvano
        const double scivola = morbido((e.p - 0.22) / 0.78); // poi la finestra scende nella barra
        const double D = ty - T;
        for (int k = 0; k < RIGHE; k++) {
            const double v = (double)k / (RIGHE - 1);
            const double y0 = T + v * (B - T);
            const double y = std::min(y0 + scivola * D, ty);
            const double f = morbido((y - T) / std::max(1.0, D));   // 0 in alto, 1 alla barra
            const double s = f * piega;
            const double xl = L + (il - L) * s, xr = R + (ir - R) * s;
            const P2 pl = dalCanonico(lato, xl, y), pr = dalCanonico(lato, xr, y);
            float ul, vl, ur, vr;   // (u e v della foto vera: vedi uvDalCanonico)
            uvDalCanonico(lato, 0.0f, (float)v, ul, vl); uvDalCanonico(lato, 1.0f, (float)v, ur, vr);
            out[k * 2 + 0] = {(float)pl.x, (float)pl.y, ul, vl};
            out[k * 2 + 1] = {(float)pr.x, (float)pr.y, ur, vr};
        }
    }

static const char* SHADER = R"(
cbuffer C : register(b0) { float2 origine; float2 dimensione; float alfa; float raggio; float2 texDim; float ombra; float alza; float raggioY; float2 estensione; float2 sfumaBordo; };
Texture2D tex : register(t0);
SamplerState camp : register(s0);
struct VO { float4 pos : SV_POSITION; float2 uv : TEXCOORD0; };
VO vs(float2 p : POSITION, float2 uv : TEXCOORD0) {
    VO o;
    o.pos = float4((p.x - origine.x) / dimensione.x * 2.0 - 1.0, 1.0 - (p.y - origine.y) / dimensione.y * 2.0, 0, 1);
    o.uv = uv;
    return o;
}
float4 ps(VO i) : SV_TARGET {
    float4 c = tex.Sample(camp, i.uv);
    float2 px = i.uv * texDim;
    if (ombra > 0.5) {
        // ombra attorno al rettangolo arrotondato della foto. Posata somiglia a quella di Windows 11 (piu' scura e
        // lunga sotto); sollevata si fa piu' chiara, piu' sfumata e quasi uguale su tutti i lati
        float a0 = lerp(0.20, 0.13, alza), tau = lerp(10.0, 16.0, alza), giu = lerp(7.0, 3.0, alza);
        float2 c = texDim * 0.5;
        float2 q = abs(px - float2(0, giu) - c) - (c - raggio);
        float d = length(max(q, 0)) + min(max(q.x, q.y), 0) - raggio;
        float s = alfa * a0 * exp(-max(d, 0) / tau);
        return float4(0, 0, 0, s);
    }
    // angoli arrotondati come le finestre di Windows 11 (ellittici nella foto se e' stirata: tondi sullo schermo)
    float2 E = texDim * estensione;   // (parte della foto usata: ritagliata, gli angoli vanno sul suo bordo)
    float2 q = min(px, E - px);
    float2 r2 = float2(raggio, raggioY > 0 ? raggioY : raggio);
    float m = 1.0;
    if (q.x < r2.x && q.y < r2.y) {
        float2 d = (r2 - q) / max(r2, 0.001);
        // distanza dal bordo dell'ellisse, in pixel a schermo circa (il raggio piu' piccolo fa da scala)
        m = saturate((1.0 - length(d)) * min(r2.x, r2.y) + 0.5);
    }
    // le foto sono opache (alfa 1); le immagini vive di Windows.Graphics.Capture sono premoltiplicate, con il bordo
    // sottile di DWM semitrasparente e gli angoli gia' tondi: si rispetta la loro trasparenza
    // fuori dalla foto (uv oltre 1: una foto che sfuma sopra l'altra non si ingrandisce, vedi elementiZoom) non c'e'
    // niente; il bordo dove finisce sfuma su sfumaBordo pixel (mezzo pixel se 0), cosi' sulla foto di sotto non
    // resta un taglio netto
    float2 fb = max(sfumaBordo, 0.5);
    m *= saturate((texDim.x - px.x) / fb.x + 0.5 / fb.x) * saturate((texDim.y - px.y) / fb.y + 0.5 / fb.y);
    float a = m * alfa;
    return float4(c.rgb * a, c.a * a);
}
)";
static double curvaGenio(double x) {
    x = std::clamp(x, 0.0, 1.0);
    return x < 0.5 ? 4 * x * x * x : 1 - std::pow(-2 * x + 2, 3) / 2;
}

static double inversaCurvaGenio(double p) {
    p = std::clamp(p, 0.0, 1.0);
    return p < 0.5 ? std::cbrt(p / 4) : 1 - std::cbrt(2 * (1 - p)) / 2;
}

// ---- tempi della "caduta nel dock" (1.3.0) ----
// L'avanzamento p e' anche il punto del percorso, uguale nei due versi (cosi' un'inversione a meta' riparte dallo
// stesso disegno, R20261120-132320-S1): da 0 a 0,5 il ritiro (finestra -> quadratino al punto intermedio), da 0,5 a 1
// il volo (punto intermedio -> icona). Il tempo invece segue il modello dell'utente: riducendo il ritiro occupa il primo
// 28% e accelera fino alla giunzione, il volo parte lanciato e frena; ripristinando il volo (all'indietro) occupa il
// primo 28% accelerando, poi la crescita frena fino alla finestra. Le inverse servono alle inversioni (t0 da p)
static double bezier(double x1, double y1, double x2, double y2, double x) {   // come cubic-bezier del CSS
    if (x <= 0) return 0;
    if (x >= 1) return 1;
    const double cx = 3 * x1, bx = 3 * (x2 - x1) - cx, ax = 1 - cx - bx, cy = 3 * y1, by = 3 * (y2 - y1) - cy, ay = 1 - cy - by;
    auto sx = [&](double t) { return ((ax * t + bx) * t + cx) * t; };
    double t = x;
    for (int i = 0; i < 8; i++) {   // Newton, poi bisezione se non converge
        const double err = sx(t) - x, d = (3 * ax * t + 2 * bx) * t + cx;
        if (std::abs(err) < 1e-7) return ((ay * t + by) * t + cy) * t;
        if (std::abs(d) < 1e-7) break;
        t -= err / d;
    }
    double lo = 0, hi = 1; t = x;
    for (int i = 0; i < 60; i++) { const double v = sx(t); if (std::abs(v - x) < 1e-7) break; if (v < x) lo = t; else hi = t; t = (lo + hi) / 2; }
    return ((ay * t + by) * t + cy) * t;
}
static double caduta1(double u) { return bezier(0.4, 0.0, 1.0, 1.0, u); }    // accelera fino alla giunzione
static double caduta2(double u) { return bezier(0.16, 1.0, 0.3, 1.0, u); }   // riparte lanciato e frena a lungo
static const double CADUTA_GIUNZIONE = 0.28;
static double cadutaChiude(double x) {   // riduzione: tempo lineare -> p (crescente)
    const double A = CADUTA_GIUNZIONE;
    x = std::clamp(x, 0.0, 1.0);
    return x <= A ? 0.5 * caduta1(x / A) : 0.5 + 0.5 * caduta2((x - A) / (1 - A));
}
static double cadutaApre(double x) {     // ripristino: tempo lineare -> p (decrescente)
    const double A = CADUTA_GIUNZIONE;
    x = std::clamp(x, 0.0, 1.0);
    return x <= A ? 1.0 - 0.5 * caduta1(x / A) : 0.5 - 0.5 * caduta2((x - A) / (1 - A));
}

}
