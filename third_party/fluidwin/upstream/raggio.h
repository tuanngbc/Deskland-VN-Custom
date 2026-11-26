// Raggio degli angoli che DWM da' alle finestre. Windows 11 usa 8 px e da solo non si puo' cambiare; se pero' e'
// attiva la mod "Custom Window Corner Radius" di Windhawk (che lo cambia dentro dwm.exe) se ne legge il valore
// dalle sue impostazioni, cosi' dock, menu, capsula, gelatina e genio hanno la stessa forma delle finestre vere.
#pragma once
#include <windows.h>
#include <dwmapi.h>
#include <vector>

namespace raggio {

// px a 96 dpi impostati nella mod, 0 se la mod non c'e' o e' disattivata (letto una volta all'avvio)
inline int windhawk() {
    static int v = [] {
        const wchar_t* chiave = L"SOFTWARE\\Windhawk\\Engine\\Mods\\custom-corner-radius";
        DWORD spenta = 1, r = 0, n = sizeof(DWORD);
        if (RegGetValueW(HKEY_LOCAL_MACHINE, chiave, L"Disabled", RRF_RT_REG_DWORD, nullptr, &spenta, &n) != ERROR_SUCCESS || spenta) return 0;
        n = sizeof(DWORD);
        if (RegGetValueW(HKEY_LOCAL_MACHINE, L"SOFTWARE\\Windhawk\\Engine\\Mods\\custom-corner-radius\\Settings", L"radius",
                         RRF_RT_REG_DWORD, nullptr, &r, &n) != ERROR_SUCCESS) r = 12;   // 12 = valore predefinito della mod
        return r >= 1 && r <= 100 ? (int)r : 0;
    }();
    return v;
}

// Impostazione "Angoli" (1.3.0, Fase 3): 0 arrotondati (quelli di Windows, o della mod), 1 poco arrotondati (la meta',
// come gli angoli piccoli di DWM), 2 squadrati. Vale per dock, notch, pannelli, menu e finestre di FluidWin; le finestre
// delle app restano come le fa Windows
inline int& stile() { static int s = 0; return s; }
inline float fattore() { return stile() == 0 ? 1.0f : stile() == 1 ? 0.5f : 0.0f; }

// angoli delle finestre "di vetro" di FluidWin (dock, isola, menu, pannelli): quelli di DWM, secondo l'impostazione
inline int vetro() {
    const int w = windhawk(), base = w > 0 ? w : 8;
    return stile() == 0 ? base : stile() == 1 ? (base + 1) / 2 : 0;
}

// preferenza di DWM per gli angoli del vetro (DWMWA_WINDOW_CORNER_PREFERENCE): 2 tondi, 3 piccoli, 1 squadrati
inline int preferenzaDwm() { return stile() == 0 ? 2 : stile() == 1 ? 3 : 1; }

// Finestre di vetro che seguono l'impostazione (le altre, come la panoramica a schermo intero, hanno angoli loro):
// cambiandola si riapplica la preferenza a quelle ancora aperte
// Ogni finestra registrata porta il segno PROP_ANGOLI, che sparisce con lei: un HWND riusato da Windows per un'altra
// finestra non ce l'ha, e si toglie dall'elenco invece di cambiarle gli angoli (R20261120-153844-S1)
inline const wchar_t* PROP_ANGOLI = L"FluidWin.Angoli";
inline std::vector<HWND>& registrate() { static std::vector<HWND> v; return v; }
inline void registra(HWND h) {
    if (!h) return;
    SetPropW(h, PROP_ANGOLI, (HANDLE)1);
    bool gia = false;
    for (HWND x : registrate()) if (x == h) gia = true;
    if (!gia) registrate().push_back(h);
    int p = preferenzaDwm(); DwmSetWindowAttribute(h, DWMWA_WINDOW_CORNER_PREFERENCE, &p, sizeof(p));   // (sempre, anche gia' in elenco)
}
inline void riapplica() {
    auto& v = registrate();
    for (size_t i = 0; i < v.size();) {
        if (!IsWindow(v[i]) || !GetPropW(v[i], PROP_ANGOLI)) { v[i] = v.back(); v.pop_back(); continue; }
        int p = preferenzaDwm(); DwmSetWindowAttribute(v[i], DWMWA_WINDOW_CORNER_PREFERENCE, &p, sizeof(p));
        i++;
    }
}

}  // namespace raggio
