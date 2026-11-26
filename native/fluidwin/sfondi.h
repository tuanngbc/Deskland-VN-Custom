// Sfondi di FluidWin (1.3.0, Fase 3): tre immagini 3840x2160 dentro l'exe (risorse RCDATA 201..203, icona.rc) da
// scegliere nelle impostazioni, piu' lo sfondo che l'utente aveva prima ("Il tuo sfondo").
// Scegliendone uno si estrae in Immagini\FluidWin (vedi cartella) e lo si applica con IDesktopWallpaper (l'interfaccia
// ufficiale di Windows, la stessa delle Impostazioni: vale per tutti gli schermi, "Riempi"). E' una scelta dell'utente
// come quella fatta nelle Impostazioni di Windows: resta anche a FluidWin chiuso. Prima di mettere uno sfondo di
// FluidWin si ricorda quello di prima (HKCU\Software\FluidWin, SfondoTuo e SfondoTuoPosizione), e la scheda "Il tuo
// sfondo" lo rimette.
// Il lavoro con la shell (che puo' metterci qualche centinaio di ms) gira in un thread a parte (pool), mai nel thread
// delle finestre.
#pragma once
#include "tlauncher_identity.h"
#include <windows.h>
#include <shlobj.h>
#include <shobjidl.h>
#include <wincodec.h>
#include <d2d1.h>
#include <string>
#include <functional>
#include <algorithm>
#include <cmath>
#include <vector>
#include <cstring>
#include <utility>
#include <mutex>
#include <atomic>
#include "pool.h"

namespace sfondi {

struct Sfondo { int risorsa; const wchar_t* file; const wchar_t* it; const wchar_t* en; };
inline constexpr Sfondo ELENCO[] = {
    {201, L"TLauncher-Native-Colline.jpg", L"Colline", L"Hills"},
    {202, L"TLauncher-Native-Seta.jpg", L"Seta", L"Silk"},
    {203, L"TLauncher-Native-Pastello.jpg", L"Pastello", L"Pastel"},
};
inline constexpr int N = (int)(sizeof(ELENCO) / sizeof(ELENCO[0]));
inline const wchar_t* CHIAVE = tlauncher::registry();

// {C2CF3110-460E-4fc1-B9D0-8A1C0C9CC4BD}: la classe DesktopWallpaper della shell
inline const CLSID CLSID_SFONDO = {0xC2CF3110, 0x460E, 0x4FC1, {0xB9, 0xD0, 0x8A, 0x1C, 0x0C, 0x9C, 0xC4, 0xBD}};

// Immagini\FluidWin. Non in AppData: provato su Windows 11 (build 26300), IDesktopWallpaper::SetWallpaper rifiuta con
// 0x80070002 i file sotto AppData\Local e AppData\Roaming, anche copiati a mano; da Immagini, dal Desktop o dalla
// cartella dell'utente li accetta
inline std::wstring cartella() {
    wchar_t* p = nullptr; std::wstring r;
    if (SUCCEEDED(SHGetKnownFolderPath(FOLDERID_Pictures, KF_FLAG_CREATE, nullptr, &p)) && p) r = std::wstring(p) + L"\\TLauncher-Native";
    CoTaskMemFree(p);
    return r;
}
inline std::wstring percorso(int i) { return (i >= 0 && i < N) ? cartella() + L"\\" + ELENCO[i].file : std::wstring(); }

// i byte del JPG i dentro l'exe (nessuna copia: la risorsa resta mappata finche' l'exe e' aperto)
inline bool risorsa(int i, const void*& dati, DWORD& n) {
    dati = nullptr; n = 0;
    if (i < 0 || i >= N) return false;
    HRSRC r = FindResourceW(nullptr, MAKEINTRESOURCEW(ELENCO[i].risorsa), MAKEINTRESOURCEW(10) /* RT_RCDATA */);
    if (!r) return false;
    HGLOBAL g = LoadResource(nullptr, r);
    n = SizeofResource(nullptr, r);
    dati = g ? LockResource(g) : nullptr;
    return dati && n > 0;
}

// scrive i byte nel file (prima in un file a parte, poi al suo posto: un file a meta' non lo vede mai nessuno)
inline bool scrivi(const std::wstring& out, const void* dati, DWORD n) {
    const std::wstring tmp = out + L".nuovo";
    HANDLE f = CreateFileW(tmp.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (f == INVALID_HANDLE_VALUE) return false;
    DWORD scritti = 0;
    const bool ok = WriteFile(f, dati, n, &scritti, nullptr) && scritti == n;
    CloseHandle(f);
    if (!ok || !MoveFileExW(tmp.c_str(), out.c_str(), MOVEFILE_REPLACE_EXISTING)) { DeleteFileW(tmp.c_str()); return false; }
    return true;
}

// il file e' identico ai byte dati? (stessa misura e stesso contenuto: un file cambiato a mano si rifa', R20261120-210024-S4)
inline bool uguale(const std::wstring& file, const void* dati, DWORD n) {
    HANDLE f = CreateFileW(file.c_str(), GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (f == INVALID_HANDLE_VALUE) return false;
    LARGE_INTEGER m{}; bool ok = GetFileSizeEx(f, &m) && m.QuadPart == (LONGLONG)n;
    if (ok) {
        std::string b(n, '\0'); DWORD letti = 0;
        ok = ReadFile(f, b.data(), n, &letti, nullptr) && letti == n && memcmp(b.data(), dati, n) == 0;
    }
    CloseHandle(f);
    return ok;
}

// lo sfondo i nella sua cartella (rifatto se manca o e' diverso dalla risorsa: una versione nuova di FluidWin)
inline bool estrai(int i, std::wstring& out) {
    const void* dati = nullptr; DWORD n = 0;
    if (!risorsa(i, dati, n)) return false;
    const std::wstring dir = cartella();
    if (dir.empty()) return false;
    SHCreateDirectoryExW(nullptr, dir.c_str(), nullptr);
    out = percorso(i);
    return uguale(out, dati, n) || scrivi(out, dati, n);
}

inline std::wstring minuscolo(std::wstring s) { for (auto& c : s) c = (wchar_t)towlower(c); return s; }
inline bool esiste(const std::wstring& f) { return !f.empty() && GetFileAttributesW(f.c_str()) != INVALID_FILE_ATTRIBUTES; }

// quale sfondo di FluidWin e' quello del file (-1: un altro, cioe' "Il tuo sfondo")
inline int indiceDi(const std::wstring& file) {
    const std::wstring f = minuscolo(file);
    for (int i = 0; i < N; i++) if (f == minuscolo(percorso(i))) return i;
    return -1;
}

// la copia che Windows tiene dello sfondo mostrato (il file di partenza puo' non esserci piu')
inline std::wstring copiaDiWindows() {
    wchar_t* p = nullptr; std::wstring c;
    if (SUCCEEDED(SHGetKnownFolderPath(FOLDERID_RoamingAppData, 0, nullptr, &p)) && p) c = std::wstring(p) + L"\\Microsoft\\Windows\\Themes\\TranscodedWallpaper";
    CoTaskMemFree(p);
    return esiste(c) ? c : std::wstring();
}

// Lo stato dello sfondo di Windows, abbastanza per rimetterlo com'era (R20261120-210024-S1/S2/S3): un'immagine per
// schermo (anche diverse fra loro), la tinta unita col suo colore, oppure la presentazione con le sue immagini
enum { NESSUNO = 0, IMMAGINI = 1, TINTA = 2, PRESENTAZIONE = 3 };
struct Stato {
    int tipo = NESSUNO;
    std::vector<std::pair<std::wstring, std::wstring>> schermi;   // (schermo, immagine)
    std::vector<std::wstring> presentazione;                       // le immagini della presentazione
    int posizione = DWPOS_FILL;
    DWORD colore = 0;
    // l'immagine da mostrare nella miniatura (vuota: nessuna)
    // (solo file: una presentazione fatta scegliendo una cartella ha la cartella, che WIC non sa leggere. Allora la copia
    // che Windows tiene dello sfondo mostrato, R20261121-002419-S17)
    std::wstring immagine() const {
        auto file = [](const std::wstring& f) { const DWORD a = f.empty() ? INVALID_FILE_ATTRIBUTES : GetFileAttributesW(f.c_str()); return a != INVALID_FILE_ATTRIBUTES && !(a & FILE_ATTRIBUTE_DIRECTORY); };
        for (const auto& s : schermi) if (file(s.second)) return s.second;
        for (const auto& f : presentazione) if (file(f)) return f;
        // Cartella: una sua immagine (fra le prime 1000 voci; prima 64, e un JPG oltre lasciava vuota la miniatura,
        // R20261121-123628-S1). Non piu' la copia di Windows dello sfondo mostrato: dopo il
        // passaggio a uno sfondo di FluidWin quella e' il JPG nostro, e la scheda "Il tuo sfondo" lo mostrava
        // (R20261121-120551-S5)
        for (const auto& d : presentazione) {
            WIN32_FIND_DATAW fd; HANDLE h = FindFirstFileW((d + L"\\*").c_str(), &fd);
            if (h == INVALID_HANDLE_VALUE) continue;
            std::wstring trovata;
            int voci = 0;
            do {
                if (fd.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) continue;
                const wchar_t* e = wcsrchr(fd.cFileName, L'.');
                if (e && (!_wcsicmp(e, L".jpg") || !_wcsicmp(e, L".jpeg") || !_wcsicmp(e, L".png") || !_wcsicmp(e, L".bmp"))) { trovata = d + L"\\" + fd.cFileName; break; }
            } while (++voci < 1000 && FindNextFileW(h, &fd));
            FindClose(h);
            if (!trovata.empty()) return trovata;
        }
        return {};
    }
    // e' uno sfondo di FluidWin? (tutti gli schermi con lo stesso sfondo nostro): allora non si ricorda
    int nostro() const {
        if (tipo != IMMAGINI || schermi.empty()) return -1;
        const int k = indiceDi(schermi[0].second);
        for (const auto& s : schermi) if (indiceDi(s.second) != k) return -1;
        return k;
    }
};

inline IDesktopWallpaper* shell() {
    IDesktopWallpaper* dw = nullptr;
    if (FAILED(CoCreateInstance(CLSID_SFONDO, nullptr, CLSCTX_ALL, IID_IDesktopWallpaper, (void**)&dw))) dw = nullptr;
    return dw;
}


// lo stato di adesso (nel thread di chi chiama, con COM); false se non si legge per intero. Un errore di lettura non
// vale "tinta unita" o "meno schermi": prima, durante un cambio di schermi, un conteggio fallito restava 0 e lo sfondo
// di prima veniva ricordato come tinta unita (R20261121-002419-S7)
inline bool leggi(Stato& s) {
    s = Stato{};
    IDesktopWallpaper* dw = shell();
    if (!dw) return false;
    DESKTOP_WALLPAPER_POSITION pos{};
    if (FAILED(dw->GetPosition(&pos))) { dw->Release(); return false; }
    s.posizione = (int)pos;
    COLORREF c = 0; if (SUCCEEDED(dw->GetBackgroundColor(&c))) s.colore = c;
    DESKTOP_SLIDESHOW_STATE st{};
    IShellItemArray* arr = nullptr;
    if (SUCCEEDED(dw->GetStatus(&st)) && (st & DSS_ENABLED) && (st & DSS_SLIDESHOW) && SUCCEEDED(dw->GetSlideshow(&arr)) && arr) {
        DWORD n = 0; arr->GetCount(&n);
        for (DWORD k = 0; k < n; k++) {
            IShellItem* it = nullptr; LPWSTR p = nullptr;
            if (SUCCEEDED(arr->GetItemAt(k, &it)) && it && SUCCEEDED(it->GetDisplayName(SIGDN_FILESYSPATH, &p)) && p) s.presentazione.push_back(p);
            CoTaskMemFree(p); if (it) it->Release();
        }
        arr->Release();
        if (!s.presentazione.empty()) { s.tipo = PRESENTAZIONE; dw->Release(); return true; }
    }
    // (GetWallpaper(nullptr) da' S_FALSE e una stringa vuota se gli schermi hanno sfondi diversi: si leggono uno per uno)
    UINT n = 0;
    if (FAILED(dw->GetMonitorDevicePathCount(&n)) || n == 0) { dw->Release(); return false; }
    bool qualcuna = false, tutto = true;
    for (UINT k = 0; k < n && tutto; k++) {
        LPWSTR id = nullptr, p = nullptr;
        if (SUCCEEDED(dw->GetMonitorDevicePathAt(k, &id)) && id && SUCCEEDED(dw->GetWallpaper(id, &p)) && p) { s.schermi.push_back({id, p}); if (*p) qualcuna = true; }
        else tutto = false;
        CoTaskMemFree(p);
        CoTaskMemFree(id);
    }
    dw->Release();
    if (!tutto) { s = Stato{}; return false; }
    s.tipo = qualcuna ? IMMAGINI : TINTA;   // (nessuna immagine: tinta unita)
    return true;
}

// lo sfondo di adesso e' uno di FluidWin? (-1 no, o non si legge)
inline int attualeNostro() { Stato s; return leggi(s) ? s.nostro() : -1; }

// ---- lo sfondo di prima, nel registro: SfondoTuoTipo, SfondoTuo (stringhe: schermo e immagine alternati, o le immagini
// della presentazione), SfondoTuoPosizione, SfondoTuoColore ----
inline void scriviDword(const wchar_t* nome, DWORD v) { RegSetKeyValueW(HKEY_CURRENT_USER, CHIAVE, nome, REG_DWORD, &v, sizeof(v)); }
inline DWORD leggiDword(const wchar_t* nome, DWORD def) { DWORD v = def, n = sizeof(v); return RegGetValueW(HKEY_CURRENT_USER, CHIAVE, nome, RRF_RT_REG_DWORD, nullptr, &v, &n) == ERROR_SUCCESS ? v : def; }

inline Stato tuo() {
    Stato s;
    s.tipo = (int)leggiDword(L"SfondoTuoTipo", NESSUNO);
    if (s.tipo < NESSUNO || s.tipo > PRESENTAZIONE) s.tipo = NESSUNO;
    s.posizione = (int)leggiDword(L"SfondoTuoPosizione", DWPOS_FILL);
    s.colore = leggiDword(L"SfondoTuoColore", 0);
    DWORD n = 0;
    if (s.tipo != NESSUNO && RegGetValueW(HKEY_CURRENT_USER, CHIAVE, L"SfondoTuo", RRF_RT_REG_MULTI_SZ, nullptr, nullptr, &n) == ERROR_SUCCESS && n < 256 * 1024) {
        std::vector<wchar_t> b(n / sizeof(wchar_t) + 2, L'\0');
        DWORD cap = (DWORD)((b.size() - 2) * sizeof(wchar_t));
        if (RegGetValueW(HKEY_CURRENT_USER, CHIAVE, L"SfondoTuo", RRF_RT_REG_MULTI_SZ, nullptr, b.data(), &cap) == ERROR_SUCCESS) {
            std::vector<std::wstring> v;
            for (const wchar_t* p = b.data(); p < b.data() + b.size() && *p; p += wcslen(p) + 1) v.emplace_back(p);
            if (s.tipo == IMMAGINI) for (size_t k = 0; k + 1 < v.size(); k += 2) s.schermi.push_back({v[k], v[k + 1]});
            else if (s.tipo == PRESENTAZIONE) s.presentazione = std::move(v);
        }
    }
    if ((s.tipo == IMMAGINI && s.schermi.empty()) || (s.tipo == PRESENTAZIONE && s.presentazione.empty())) s.tipo = NESSUNO;
    return s;
}

// Ricorda lo stato s come "il tuo sfondo". Un'immagine che non c'e' piu' (cancellata dopo averla messa) si sostituisce
// con una copia di quella che Windows tiene dello sfondo mostrato, in Immagini\FluidWin: senza, non si potrebbe rimettere
// (R20261120-210024-S3). false se non c'e' modo di rimetterlo: allora non si cambia lo sfondo
inline bool ricordaTuo(Stato s) {
    if (s.tipo == IMMAGINI) {
        // Configurazioni che non si saprebbero rimettere: lo sfondo non si cambia (R20261120-211010-S1/S2).
        //  - schermi misti, a tinta unita e con un'immagine (il registro non tiene stringhe vuote, e IDesktopWallpaper
        //    non mette la tinta su un solo schermo);
        //  - immagini cancellate su schermi con immagini diverse (Windows tiene la copia di una sola).
        bool vuoto = false, diverse = false, manca = false;
        for (const auto& sc : s.schermi) {
            if (sc.second.empty()) vuoto = true;
            else if (!esiste(sc.second)) manca = true;
            if (sc.second != s.schermi[0].second) diverse = true;
        }
        if (vuoto || (manca && diverse)) return false;
        if (manca) {   // stessa immagine su tutti gli schermi, cancellata: la copia di Windows e' proprio lei
            const std::wstring w = copiaDiWindows();
            if (w.empty()) return false;
            const std::wstring copia = cartella() + L"\\Il-tuo-sfondo.jpg";
            SHCreateDirectoryExW(nullptr, cartella().c_str(), nullptr);
            if (!CopyFileW(w.c_str(), copia.c_str(), FALSE)) return false;
            for (auto& sc : s.schermi) sc.second = copia;
        }
    }
    std::wstring multi;
    if (s.tipo == IMMAGINI) for (const auto& sc : s.schermi) { multi += sc.first; multi += L'\0'; multi += sc.second; multi += L'\0'; }
    else for (const auto& f : s.presentazione) { multi += f; multi += L'\0'; }
    multi += L'\0';
    // Ogni scrittura controllata, e lo stesso limite della lettura (tuo): se qualcosa non si salva lo sfondo non si
    // cambia. Prima l'esito non si guardava, e un elenco da 256 KB o piu' si salvava ma non si rileggeva
    // (R20261121-002419-S6). Il tipo resta per ultimo: un salvataggio a meta' lascia il record di prima
    const DWORD byte = (DWORD)(multi.size() * sizeof(wchar_t));
    if (byte >= 256 * 1024) return false;
    if (RegSetKeyValueW(HKEY_CURRENT_USER, CHIAVE, L"SfondoTuo", REG_MULTI_SZ, multi.data(), byte) != ERROR_SUCCESS) return false;
    const DWORD pos = (DWORD)s.posizione, col = s.colore, tipo = (DWORD)s.tipo;
    return RegSetKeyValueW(HKEY_CURRENT_USER, CHIAVE, L"SfondoTuoPosizione", REG_DWORD, &pos, sizeof(pos)) == ERROR_SUCCESS &&
           RegSetKeyValueW(HKEY_CURRENT_USER, CHIAVE, L"SfondoTuoColore", REG_DWORD, &col, sizeof(col)) == ERROR_SUCCESS &&
           RegSetKeyValueW(HKEY_CURRENT_USER, CHIAVE, L"SfondoTuoTipo", REG_DWORD, &tipo, sizeof(tipo)) == ERROR_SUCCESS;
}

// mette il file come sfondo di tutti gli schermi, "Riempi" (nel thread di chi chiama, con COM). Non riuscito: posizione
// e tinta unita tornano com'erano in 'prima' (lo stato letto subito prima). Prima restava "Riempi" su uno sfondo che
// era centrato, e un nuovo tentativo l'avrebbe ricordato cosi' (R20261121-002419-S9)
inline bool metti(const std::wstring& file, const Stato& prima) {
    IDesktopWallpaper* dw = shell();
    if (!dw) return false;
    // (niente Enable(TRUE) prima: fallisce se manca l'immagine che Windows riattiverebbe, e SetWallpaper abilita gia'
    // lo sfondo da sola; con il prerequisito il JPG nuovo non arrivava mai a Windows, R20261121-120551-S3)
    bool ok = SUCCEEDED(dw->SetPosition(DWPOS_FILL)) && SUCCEEDED(dw->SetWallpaper(nullptr, file.c_str()));
    if (!ok) {
        dw->SetPosition((DESKTOP_WALLPAPER_POSITION)prima.posizione);
        if (prima.tipo == TINTA) dw->Enable(FALSE);
    }
    dw->Release();
    return ok;
}

// rimette lo stato s (il tuo sfondo)
inline bool rimetti(const Stato& s) {
    IDesktopWallpaper* dw = shell();
    if (!dw) return false;
    bool ok = false;
    dw->SetBackgroundColor((COLORREF)s.colore);
    if (s.tipo == TINTA) ok = SUCCEEDED(dw->Enable(FALSE));
    else if (s.tipo == PRESENTAZIONE) {
        std::vector<PIDLIST_ABSOLUTE> pidl;
        for (const auto& f : s.presentazione) { PIDLIST_ABSOLUTE p = nullptr; if (SUCCEEDED(SHParseDisplayName(f.c_str(), nullptr, &p, 0, nullptr)) && p) pidl.push_back(p); }
        IShellItemArray* arr = nullptr;
        if (!pidl.empty() && SUCCEEDED(SHCreateShellItemArrayFromIDLists((UINT)pidl.size(), (PCIDLIST_ABSOLUTE_ARRAY)pidl.data(), &arr)) && arr) {
            dw->Enable(TRUE);
            dw->SetPosition((DESKTOP_WALLPAPER_POSITION)s.posizione);
            ok = SUCCEEDED(dw->SetSlideshow(arr));
            arr->Release();
        }
        for (auto p : pidl) CoTaskMemFree(p);
    } else if (s.tipo == IMMAGINI) {
        dw->Enable(TRUE);
        dw->SetPosition((DESKTOP_WALLPAPER_POSITION)s.posizione);
        // schermo per schermo; uno che non c'e' piu' (schermi cambiati) prende l'immagine del primo
        // riuscito solo se tutti gli schermi l'hanno preso: uno rimasto con lo sfondo di FluidWin non e' "rimesso"
        // (prima bastava il primo, e la scheda "Il tuo sfondo" risultava scelta, R20261121-002419-S8)
        std::wstring prima;
        for (const auto& sc : s.schermi) if (esiste(sc.second)) { prima = sc.second; break; }
        UINT n = 0;
        // (un errore su uno schermo non ferma gli altri: si prova su tutti e l'esito e' l'insieme, R20261121-120551-S4)
        bool tutti = SUCCEEDED(dw->GetMonitorDevicePathCount(&n)) && n > 0;
        for (UINT k = 0; k < n; k++) {
            LPWSTR id = nullptr;
            if (FAILED(dw->GetMonitorDevicePathAt(k, &id)) || !id) { CoTaskMemFree(id); tutti = false; continue; }
            std::wstring f = prima;
            for (const auto& sc : s.schermi) if (sc.first == id && esiste(sc.second)) f = sc.second;
            if (f.empty() || FAILED(dw->SetWallpaper(id, f.c_str()))) tutti = false;
            CoTaskMemFree(id);
        }
        ok = tutti;
    }
    dw->Release();
    return ok;
}

// Sceglie lo sfondo i (0..N-1 di FluidWin, -1 il tuo) in un thread del pool; false se il lavoro non e' partito.
// fatto(ok) lo chiama poi il thread di 'dest' col messaggio 'msg' (wParam = ok, lParam = puntatore a
// std::function<void(bool)> da chiamare e cancellare). Se il messaggio non parte (finestra distrutta) la risposta si
// perde: chi aspetta non resta bloccato per sempre (vedi impostazioni, sfondoDa)
inline std::atomic<unsigned>& ultimaScelta() { static std::atomic<unsigned> n{0}; return n; }   // numero dell'ultima scelta (scegli)
inline bool scegli(int i, HWND dest, UINT msg, std::function<void(bool)> fatto) {
    if (tlauncher::trial()) return false; // UAT never persists a Windows wallpaper change.
    struct Lavoro { int i; HWND dest; UINT msg; std::function<void(bool)>* cb; unsigned numero; };
    Lavoro* l = new Lavoro{i, dest, msg, new std::function<void(bool)>(std::move(fatto)), ++ultimaScelta()};
    const bool partito = pool::lancia([](LPVOID p) -> DWORD {
        Lavoro* l = (Lavoro*)p;
        // un lavoro alla volta (il mutex non garantisce l'ordine: vedi sotto, solo l'ultima scelta si applica; prima
        // due lavori potevano mettere lo sfondo insieme, R20261120-211010-S3)
        static std::mutex fila;
        std::lock_guard<std::mutex> turno(fila);
        // (multithread: un thread del pool puo' essere gia' stato inizializzato cosi' da un altro lavoro)
        const HRESULT co = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
        bool ok = false;
        // superata da una scelta piu' recente (in attesa dietro questa o gia' fatta): non si applica niente, cosi' alla
        // fine resta sempre l'ultima scelta, in qualsiasi ordine il mutex lasci passare i lavori (R20261120-233034-S3)
        if (l->numero != ultimaScelta().load()) { /* ok resta false: la risposta e' comunque di una scelta superata */ }
        else if (l->i >= 0) {
            std::wstring f;
            Stato prima;
            // lo sfondo di prima si ricorda solo se non e' gia' uno di FluidWin (cambiando fra i nostri resta il tuo);
            // se non si legge o non si potrebbe rimettere, lo sfondo non si cambia
            const bool letto = leggi(prima);
            const bool salvo = letto && (prima.nostro() >= 0 || ricordaTuo(prima));
            if (salvo && estrai(l->i, f)) ok = metti(f, prima);
        } else {
            const Stato t = tuo();
            ok = t.tipo != NESSUNO && rimetti(t);
        }
        if (SUCCEEDED(co)) CoUninitialize();
        if (!consegna::posta(l->dest, l->msg, ok, l->cb)) delete l->cb;
        delete l;
        return 0;
    }, l, THREAD_PRIORITY_NORMAL, true);
    if (!partito) { delete l->cb; delete l; }
    return partito;
}


// Miniatura (w x h, riempita e tagliata al centro) di un JPG/PNG in memoria o su disco, come bitmap per rt
inline ID2D1Bitmap* miniatura(ID2D1RenderTarget* rt, const void* dati, DWORD n, const std::wstring& file, UINT w, UINT h) {
    if (!rt) return nullptr;
    IWICImagingFactory* wic = nullptr;
    if (FAILED(CoCreateInstance(CLSID_WICImagingFactory, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&wic))) || !wic) return nullptr;
    IWICStream* st = nullptr; IWICBitmapDecoder* dec = nullptr; IWICBitmapFrameDecode* fr = nullptr;
    IWICBitmapClipper* cl = nullptr; IWICBitmapScaler* sc = nullptr; IWICFormatConverter* cv = nullptr; ID2D1Bitmap* out = nullptr;
    HRESULT hr = E_FAIL;
    if (dati) {
        if (SUCCEEDED(wic->CreateStream(&st)) && SUCCEEDED(st->InitializeFromMemory((BYTE*)dati, n)))
            hr = wic->CreateDecoderFromStream(st, nullptr, WICDecodeMetadataCacheOnDemand, &dec);
    } else hr = wic->CreateDecoderFromFilename(file.c_str(), nullptr, GENERIC_READ, WICDecodeMetadataCacheOnDemand, &dec);
    UINT iw = 0, ih = 0;
    if (SUCCEEDED(hr)) hr = dec->GetFrame(0, &fr);
    if (SUCCEEDED(hr)) hr = fr->GetSize(&iw, &ih);
    if (SUCCEEDED(hr) && iw && ih) {
        // prima la scala di tutta l'immagine fino a coprire la miniatura (direttamente sul fotogramma: il decoder JPG
        // scala gia' mentre decodifica, molto piu' in fretta), poi il ritaglio centrale
        const double s = std::max((double)w / iw, (double)h / ih);
        const UINT sw = std::max(w, (UINT)std::ceil(iw * s)), sh = std::max(h, (UINT)std::ceil(ih * s));
        const WICRect rc{(INT)(sw - w) / 2, (INT)(sh - h) / 2, (INT)w, (INT)h};
        hr = wic->CreateBitmapScaler(&sc);
        if (SUCCEEDED(hr)) hr = sc->Initialize(fr, sw, sh, WICBitmapInterpolationModeFant);
        if (SUCCEEDED(hr)) hr = wic->CreateBitmapClipper(&cl);
        if (SUCCEEDED(hr)) hr = cl->Initialize(sc, &rc);
        if (SUCCEEDED(hr)) hr = wic->CreateFormatConverter(&cv);
        if (SUCCEEDED(hr)) hr = cv->Initialize(cl, GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, nullptr, 0, WICBitmapPaletteTypeCustom);
        if (SUCCEEDED(hr)) rt->CreateBitmapFromWicBitmap(cv, nullptr, &out);
    }
    for (IUnknown* u : {(IUnknown*)cv, (IUnknown*)sc, (IUnknown*)cl, (IUnknown*)fr, (IUnknown*)dec, (IUnknown*)st, (IUnknown*)wic}) if (u) u->Release();
    return out;
}

}  // namespace sfondi
