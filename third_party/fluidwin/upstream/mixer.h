// Mixer del volume (come EarTrumpet, nello stile di FluidWin): per ogni uscita audio il suo volume e, sotto, le app che
// ci suonano, ognuna col suo volume e il suo muto. API ufficiali di Windows (WASAPI): IAudioEndpointVolume per le
// uscite, IAudioSessionManager2 + ISimpleAudioVolume per le app. Le sessioni della stessa app sulla stessa uscita
// (Chrome ne apre parecchie) sono una riga sola: il volume si cambia a tutte insieme.
// Si legge entrando nella vista e ogni secondo finche' e' aperta; solo dal thread principale.
#pragma once
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <mmdeviceapi.h>
#include <audiopolicy.h>
#include <endpointvolume.h>
#include <tlhelp32.h>
#include <map>
#include <string>
#include <vector>
#include <algorithm>
#include <cwctype>
#include "testi.h"

namespace mixer {

struct App {
    std::wstring nome, exe;            // exe: percorso completo (per l'icona), vuoto se non si sa
    bool sistema = false;              // suoni di sistema
    float volume = 1; bool muto = false;
    std::vector<ISimpleAudioVolume*> vol;
};

struct Uscita {
    std::wstring id, nome;
    bool predefinita = false;
    float volume = 1; bool muto = false;
    IAudioEndpointVolume* vol = nullptr;
    std::vector<App> app;
};

class Mixer {
public:
    std::vector<Uscita> uscite;   // la predefinita per prima; le altre solo se ci suona qualcosa

    Mixer() = default;
    Mixer(const Mixer&) = delete;
    Mixer& operator=(const Mixer&) = delete;
    ~Mixer() { libera(); if (en) en->Release(); }

    void leggi() {
        libera();
        if (!en) CoCreateInstance(__uuidof(MMDeviceEnumerator), nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&en));
        if (!en) return;
        std::wstring idPred;
        IMMDevice* dp = nullptr;
        if (SUCCEEDED(en->GetDefaultAudioEndpoint(eRender, eConsole, &dp)) && dp) { LPWSTR s = nullptr; if (SUCCEEDED(dp->GetId(&s)) && s) { idPred = s; CoTaskMemFree(s); } dp->Release(); }
        IMMDeviceCollection* col = nullptr;
        if (FAILED(en->EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE, &col)) || !col) return;
        HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        UINT n = 0; col->GetCount(&n);
        for (UINT i = 0; i < n; i++) {
            IMMDevice* d = nullptr; if (FAILED(col->Item(i, &d)) || !d) continue;
            Uscita u;
            LPWSTR s = nullptr; if (SUCCEEDED(d->GetId(&s)) && s) { u.id = s; CoTaskMemFree(s); }
            u.predefinita = u.id == idPred;
            u.nome = nomeDispositivo(d);
            if (SUCCEEDED(d->Activate(__uuidof(IAudioEndpointVolume), CLSCTX_INPROC_SERVER, nullptr, (void**)&u.vol)) && u.vol) {
                u.vol->GetMasterVolumeLevelScalar(&u.volume); BOOL m = FALSE; u.vol->GetMute(&m); u.muto = m != FALSE;
            }
            sessioni(d, u, snap);
            d->Release();
            const bool app = std::any_of(u.app.begin(), u.app.end(), [](const App& a) { return !a.sistema; });
            if (u.predefinita || app) uscite.push_back(std::move(u));
            else {   // uscita scartata: si rilascia tutto, anche le sessioni (prima solo u.vol: FW-AUDIT-020)
                if (u.vol) u.vol->Release();
                for (auto& a : u.app) for (auto* s : a.vol) s->Release();
            }
        }
        if (snap != INVALID_HANDLE_VALUE) CloseHandle(snap);
        col->Release();
        std::stable_sort(uscite.begin(), uscite.end(), [](const Uscita& a, const Uscita& b) { return a.predefinita > b.predefinita; });
    }

    // a = -1: l'uscita stessa
    bool volume(size_t u, int a, float v) {
        if (u >= uscite.size()) return false;
        v = std::clamp(v, 0.0f, 1.0f);
        Uscita& x = uscite[u];
        if (a < 0) { if (!x.vol || FAILED(x.vol->SetMasterVolumeLevelScalar(v, nullptr))) return false; if (x.muto && v > 0) { if (FAILED(x.vol->SetMute(FALSE, nullptr))) return false; x.muto = false; } x.volume = v; return true; }
        if (a >= (int)x.app.size()) return false;
        App& p = x.app[a];
        bool ok = !p.vol.empty();
        for (auto* s : p.vol) { ok = SUCCEEDED(s->SetMasterVolume(v, nullptr)) && ok; if (p.muto && v > 0) ok = SUCCEEDED(s->SetMute(FALSE, nullptr)) && ok; }
        p.volume = v; if (v > 0) p.muto = false;
        return ok;
    }

    bool muto(size_t u, int a) {
        if (u >= uscite.size()) return false;
        Uscita& x = uscite[u];
        if (a < 0) { x.muto = !x.muto; return x.vol && SUCCEEDED(x.vol->SetMute(x.muto, nullptr)); }
        if (a >= (int)x.app.size()) return false;
        App& p = x.app[a];
        p.muto = !p.muto;
        bool ok = !p.vol.empty();
        for (auto* s : p.vol) ok = SUCCEEDED(s->SetMute(p.muto, nullptr)) && ok;
        return ok;
    }

    size_t righe() const { size_t r = 0; for (auto& u : uscite) r += 1 + u.app.size(); return r; }

private:
    IMMDeviceEnumerator* en = nullptr;

    void libera() {
        for (auto& u : uscite) { if (u.vol) u.vol->Release(); for (auto& a : u.app) for (auto* s : a.vol) s->Release(); }
        uscite.clear();
    }

    static std::wstring nomeDispositivo(IMMDevice* d) {
        static const PROPERTYKEY NOME = {{0xa45c254e, 0xdf1c, 0x4efd, {0x80, 0x20, 0x67, 0xd1, 0x46, 0xa8, 0x50, 0xe0}}, 14};
        std::wstring r;
        IPropertyStore* ps = nullptr;
        if (SUCCEEDED(d->OpenPropertyStore(STGM_READ, &ps)) && ps) {
            PROPVARIANT v; PropVariantInit(&v);
            if (SUCCEEDED(ps->GetValue(NOME, &v)) && v.vt == VT_LPWSTR && v.pwszVal) r = v.pwszVal;
            PropVariantClear(&v); ps->Release();
        }
        return r;
    }

    // percorso completo dell'eseguibile (accesso minimo, come per le app in background); se no, il nome dall'istantanea
    static void exeDi(DWORD pid, HANDLE snap, std::wstring& percorso, std::wstring& nome) {
        if (HANDLE p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid)) {
            wchar_t b[MAX_PATH]; DWORD n = MAX_PATH;
            if (QueryFullProcessImageNameW(p, 0, b, &n)) percorso = b;
            CloseHandle(p);
        }
        std::wstring f;
        if (!percorso.empty()) { const size_t s = percorso.find_last_of(L"\\/"); f = s == std::wstring::npos ? percorso : percorso.substr(s + 1); }
        else if (snap != INVALID_HANDLE_VALUE) {
            PROCESSENTRY32W pe{}; pe.dwSize = sizeof(pe);
            for (BOOL ok = Process32FirstW(snap, &pe); ok; ok = Process32NextW(snap, &pe)) if (pe.th32ProcessID == pid) { f = pe.szExeFile; break; }
        }
        // nome leggibile: la descrizione del file se c'e' ("Google Chrome"), altrimenti il nome senza .exe
        nome = descrizione(percorso);
        if (nome.empty()) { nome = f; const size_t q = nome.rfind(L'.'); if (q != std::wstring::npos && q > 0) nome.resize(q); if (!nome.empty()) nome[0] = (wchar_t)towupper(nome[0]); }
    }

    // Una volta per eseguibile (solo dal thread principale): prima il mixer aperto rileggeva dal disco i dati di ogni
    // exe a ogni secondo, per ogni sessione (R20261121-001738-S14). Al massimo 256 voci, poi si ricomincia
    // La voce ricorda anche la data di modifica del file: un'app aggiornata nella stessa cartella (descrizione nuova)
    // si rilegge invece di mostrare il nome vecchio finche' FluidWin resta aperto (R20261121-120053-S1)
    // La data si ricontrolla al massimo ogni 30 s per voce: il mixer rilegge ogni secondo, e un'app avviata da una
    // condivisione lenta faceva attendere il thread principale a ogni giro (R20261121-123247-S1)
    static std::wstring descrizione(const std::wstring& exe) {
        struct Voce { ULONGLONG data; std::wstring nome; ULONGLONG visto = 0; };
        static std::map<std::wstring, Voce> gia;
        if (exe.empty()) return L"";
        const ULONGLONG adesso = GetTickCount64();
        if (auto it = gia.find(exe); it != gia.end() && adesso - it->second.visto < 30000) return it->second.nome;
        WIN32_FILE_ATTRIBUTE_DATA a{};
        const ULONGLONG data = GetFileAttributesExW(exe.c_str(), GetFileExInfoStandard, &a)
            ? ((ULONGLONG)a.ftLastWriteTime.dwHighDateTime << 32) | a.ftLastWriteTime.dwLowDateTime : 0;
        if (auto it = gia.find(exe); it != gia.end() && it->second.data == data) { it->second.visto = adesso; return it->second.nome; }
        if (gia.size() > 256) gia.clear();
        Voce& v = gia[exe];
        v.data = data; v.nome = leggiDescrizione(exe); v.visto = adesso;
        return v.nome;
    }
    static std::wstring leggiDescrizione(const std::wstring& exe) {
        DWORD h = 0, n = GetFileVersionInfoSizeW(exe.c_str(), &h);
        if (!n || n > 4 * 1024 * 1024) return L"";
        std::vector<BYTE> b(n);
        if (!GetFileVersionInfoW(exe.c_str(), 0, n, b.data())) return L"";
        struct Lingua { WORD l, c; } *t = nullptr; UINT lt = 0;
        if (!VerQueryValueW(b.data(), L"\\VarFileInfo\\Translation", (void**)&t, &lt) || lt < sizeof(Lingua)) return L"";
        wchar_t q[64]; swprintf(q, 64, L"\\StringFileInfo\\%04x%04x\\FileDescription", t[0].l, t[0].c);
        wchar_t* s = nullptr; UINT ls = 0;
        if (VerQueryValueW(b.data(), q, (void**)&s, &ls) && s && ls > 1) return s;
        return L"";
    }

    void sessioni(IMMDevice* d, Uscita& u, HANDLE snap) {
        IAudioSessionManager2* m = nullptr;
        if (FAILED(d->Activate(__uuidof(IAudioSessionManager2), CLSCTX_INPROC_SERVER, nullptr, (void**)&m)) || !m) return;
        IAudioSessionEnumerator* se = nullptr;
        if (SUCCEEDED(m->GetSessionEnumerator(&se)) && se) {
            int k = 0; se->GetCount(&k);
            for (int j = 0; j < k; j++) {
                IAudioSessionControl* sc = nullptr; if (FAILED(se->GetSession(j, &sc)) || !sc) continue;
                AudioSessionState st = AudioSessionStateExpired; sc->GetState(&st);
                IAudioSessionControl2* s2 = nullptr; ISimpleAudioVolume* sv = nullptr;
                if (st != AudioSessionStateExpired && SUCCEEDED(sc->QueryInterface(IID_PPV_ARGS(&s2))) && s2 &&
                    SUCCEEDED(sc->QueryInterface(IID_PPV_ARGS(&sv))) && sv) {
                    DWORD pid = 0; s2->GetProcessId(&pid);
                    const bool sistema = s2->IsSystemSoundsSession() == S_OK;
                    if (pid != GetCurrentProcessId() || sistema) {
                        std::wstring exe, nome;
                        if (sistema) nome = testi::t(L"Suoni di sistema", L"System sounds");
                        else exeDi(pid, snap, exe, nome);
                        if (nome.empty()) nome = testi::t(L"App", L"App");
                        auto it = std::find_if(u.app.begin(), u.app.end(), [&](const App& a) { return a.sistema == sistema && (sistema || (exe.empty() ? a.nome == nome : a.exe == exe)); });
                        if (it == u.app.end()) {
                            App a; a.nome = nome; a.exe = exe; a.sistema = sistema;
                            sv->GetMasterVolume(&a.volume); BOOL mu = FALSE; sv->GetMute(&mu); a.muto = mu != FALSE;
                            u.app.push_back(std::move(a)); it = u.app.end() - 1;
                        }
                        it->vol.push_back(sv); sv = nullptr;
                    }
                }
                if (sv) sv->Release();
                if (s2) s2->Release();
                sc->Release();
            }
            se->Release();
        }
        m->Release();
        // i suoni di sistema in fondo, le altre in ordine di nome
        std::sort(u.app.begin(), u.app.end(), [](const App& a, const App& b) { if (a.sistema != b.sistema) return b.sistema; return _wcsicmp(a.nome.c_str(), b.nome.c_str()) < 0; });
    }
};

}  // namespace mixer
