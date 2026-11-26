// Notifiche di Windows lette con l'API ufficiale UserNotificationListener (Windows.UI.Notifications.Management),
// la stessa che usano gli smartwatch e le app "companion". Funziona solo se nelle impostazioni della privacy
// e' consentito l'accesso alle notifiche; non mostra richieste da solo. MinGW non ha le intestazioni di questa
// classe: gli IID li si chiede all'oggetto stesso (GetIids) e i metodi si chiamano per posizione, nell'ordine
// delle intestazioni ufficiali (verificato su windows.ui.notifications.h e windows.applicationmodel.h).
#pragma once
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <roapi.h>
#include <winstring.h>
#include <inspectable.h>
#include <string>
#include <vector>
#include <algorithm>

namespace notifiche {

struct Notifica {
    UINT32 id = 0;
    std::wstring app, aumid, titolo, testo;
    INT64 quando = 0;   // FILETIME (100 ns dal 1601, UTC)
};

template <class F> static F metodo(void* o, int i) { return (F)(*(void***)o)[i]; }
static void rilascia(void* o) { if (o) ((IUnknown*)o)->Release(); }
static std::wstring testoDa(HSTRING s) {
    UINT32 n = 0; const wchar_t* p = WindowsGetStringRawBuffer(s, &n);
    std::wstring r(p ? p : L"", p ? std::min(n,UINT32(2048)) : 0);
    WindowsDeleteString(s);
    return r;
}

typedef HRESULT (STDMETHODCALLTYPE *FnPtr)(void*, void**);
typedef HRESULT (STDMETHODCALLTYPE *FnStr)(void*, HSTRING*);

class Lettore {
public:
    Lettore() = default;
    ~Lettore() { chiudi(); } // TLauncher readers are scoped inside their WinRT apartment.
    Lettore(const Lettore&) = delete;              // possiede un riferimento COM: niente copie
    Lettore& operator=(const Lettore&) = delete;

    // false se l'accesso alle notifiche non e' consentito (o l'API non c'e')
    bool permesso() { return prepara(); }

    // Rilascia l'oggetto di Windows. Va chiamata dal thread che l'ha usato, prima di RoUninitialize (audit
    // FW-AUDIT-011: prima non si rilasciava mai, un riferimento perso a ogni lettura avviata). Volutamente non nel
    // distruttore: due Lettore sono globali e si distruggerebbero all'uscita, forse dopo la chiusura di COM
    void chiudi() { if (ascolta) { rilascia(ascolta); ascolta = nullptr; } provato = false; }

    // false = accesso non concesso o lettura non riuscita. *tutte (se dato): false se qualche elemento non si e' letto
    // (la lista ha solo quelli leggibili)
    bool leggi(std::vector<Notifica>& out, bool* tutte = nullptr) {
        if (tutte) *tutte = false;
        out.clear();
        if (!prepara()) return false;
        void* op = nullptr;
        // GetNotificationsAsync(NotificationKinds::Toast)
        if (FAILED(metodo<HRESULT (STDMETHODCALLTYPE*)(void*, UINT32, void**)>(ascolta, 10)(ascolta, 1, &op)) || !op) return false;
        static const IID IID_Async = {0x00000036, 0, 0, {0xC0, 0, 0, 0, 0, 0, 0, 0x46}};   // IAsyncInfo
        void* info = nullptr; ((IUnknown*)op)->QueryInterface(IID_Async, &info);
        int stato = 0;
        for (int i = 0; i < 400 && info; i++) {
            metodo<HRESULT (STDMETHODCALLTYPE*)(void*, int*)>(info, 7)(info, &stato);   // get_Status
            if (stato != 0) break;
            Sleep(1);
        }
        if (info && stato == 0) metodo<HRESULT (STDMETHODCALLTYPE*)(void*)>(info, 9)(info); // Cancel on timeout
        rilascia(info);
        void* vista = nullptr;
        if (stato == 1) metodo<FnPtr>(op, 8)(op, &vista);   // GetResults
        rilascia(op);
        if (!vista) return false;
        // Lettura incompleta (misura o un elemento o il suo ID illeggibili): la lista porta quelli leggibili e *tutte
        // resta falso. Chi riceve toglie dalla coda le notifiche assenti solo da una lista completa: una parziale ne
        // faceva sparire di valide (R20261121-120306-S2). Scartarla del tutto invece perdeva i banner nuovi e il centro
        // notifiche la prendeva per un permesso negato (R20261121-123420-S1 e S2)
        UINT32 n = 0;
        bool completa = SUCCEEDED(metodo<HRESULT (STDMETHODCALLTYPE*)(void*, UINT32*)>(vista, 7)(vista, &n));
        if (!completa) n = 0;
        // tutte (fino a 500): il limite di 50 serviva al disegno, ma troncava anche i numeri sulle icone e "Cancella
        // tutto" (R20261121-002056-S10). Chi disegna mostra le prime (le piu' recenti, dopo l'ordinamento qui sotto)
        for (UINT32 i = 0; i < n && i < 500; i++) {
            void* un = nullptr;
            if (FAILED(metodo<HRESULT (STDMETHODCALLTYPE*)(void*, UINT32, void**)>(vista, 6)(vista, i, &un)) || !un) { completa = false; continue; }
            Notifica x;
            if (FAILED(metodo<HRESULT (STDMETHODCALLTYPE*)(void*, UINT32*)>(un, 8)(un, &x.id))) { rilascia(un); completa = false; continue; }
            metodo<HRESULT (STDMETHODCALLTYPE*)(void*, INT64*)>(un, 9)(un, &x.quando);
            void* app = nullptr;
            if (SUCCEEDED(metodo<FnPtr>(un, 7)(un, &app)) && app) {
                HSTRING s = nullptr;
                if (SUCCEEDED(metodo<FnStr>(app, 7)(app, &s))) x.aumid = testoDa(s);
                void* di = nullptr;
                if (SUCCEEDED(metodo<FnPtr>(app, 8)(app, &di)) && di) {
                    HSTRING d = nullptr; if (SUCCEEDED(metodo<FnStr>(di, 6)(di, &d))) x.app = testoDa(d);
                    rilascia(di);
                }
                rilascia(app);
            }
            void* no = nullptr;
            if (SUCCEEDED(metodo<FnPtr>(un, 6)(un, &no)) && no) {
                void* vis = nullptr;
                if (SUCCEEDED(metodo<FnPtr>(no, 8)(no, &vis)) && vis) {
                    HSTRING tg = nullptr; WindowsCreateString(L"ToastGeneric", 12, &tg);
                    void* bind = nullptr;
                    metodo<HRESULT (STDMETHODCALLTYPE*)(void*, HSTRING, void**)>(vis, 9)(vis, tg, &bind);
                    WindowsDeleteString(tg);
                    if (bind) {
                        void* testi = nullptr;
                        if (SUCCEEDED(metodo<FnPtr>(bind, 11)(bind, &testi)) && testi) {
                            UINT32 k = 0; metodo<HRESULT (STDMETHODCALLTYPE*)(void*, UINT32*)>(testi, 7)(testi, &k);
                            for (UINT32 j = 0; j < k && j < 16; j++) {
                                void* el = nullptr;
                                if (FAILED(metodo<HRESULT (STDMETHODCALLTYPE*)(void*, UINT32, void**)>(testi, 6)(testi, j, &el)) || !el) continue;
                                HSTRING t = nullptr;
                                if (SUCCEEDED(metodo<FnStr>(el, 6)(el, &t))) {
                                    std::wstring s = testoDa(t);
                                    if (j == 0) x.titolo = s;
                                    else if (!s.empty() && x.testo.size() < 2048) {
                                        x.testo += (x.testo.empty() ? L"" : L" ") + s;
                                        if (x.testo.size() > 2048) x.testo.resize(2048);
                                    }
                                }
                                rilascia(el);
                            }
                            rilascia(testi);
                        }
                        rilascia(bind);
                    }
                    rilascia(vis);
                }
                rilascia(no);
            }
            rilascia(un);
            for (auto& c : x.titolo) if (c == L'\n' || c == L'\r') c = L' ';
            for (auto& c : x.testo) if (c == L'\n' || c == L'\r') c = L' ';
            out.push_back(std::move(x));
        }
        rilascia(vista);
        if (tutte) *tutte = completa && n <= 500;   // (oltre 500 la lista e' tagliata: non e' completa)
        std::sort(out.begin(), out.end(), [](const Notifica& a, const Notifica& b) { return a.quando > b.quando; });
        return true;
    }

    bool rimuovi(UINT32 id) { return prepara() && SUCCEEDED(metodo<HRESULT (STDMETHODCALLTYPE*)(void*, UINT32)>(ascolta, 13)(ascolta, id)); }
    // ClearNotifications per le app desktop non ha effetto: si toglie una notifica alla volta (questo funziona)
    void cancellaTutte() {
        std::vector<Notifica> tutte;
        if (!leggi(tutte)) return;
        for (const auto& n : tutte) rimuovi(n.id);
    }

private:
    void* ascolta = nullptr;   // IUserNotificationListener
    bool provato = false;
    ULONGLONG tProva = 0;

    bool prepara() {
        if (ascolta) {
            int accesso = 0;
            if (SUCCEEDED(metodo<HRESULT (STDMETHODCALLTYPE*)(void*, int*)>(ascolta, 7)(ascolta, &accesso)) && accesso == 1) return true;
            chiudi(); // Permission can be revoked while the panel is open.
            return false;
        }
        // permesso negato: si riprova al massimo ogni 5 s (l'utente puo' averlo appena concesso)
        if (provato && GetTickCount64() - tProva < 5000) return false;
        provato = true; tProva = GetTickCount64();
        HSTRING cl = nullptr; const wchar_t* nome = L"Windows.UI.Notifications.Management.UserNotificationListener";
        WindowsCreateString(nome, (UINT32)wcslen(nome), &cl);
        IInspectable* fab = nullptr;
        const HRESULT hr = RoGetActivationFactory(cl, __uuidof(IInspectable), (void**)&fab);
        WindowsDeleteString(cl);
        if (FAILED(hr) || !fab) return false;
        // Le interfacce esatte dell'SDK (windows.ui.notifications.management.h, 10.0.26100): prima si prendeva la prima
        // interfaccia diversa da IActivationFactory, e su un'implementazione con un altro ordine lo slot 6 o 7 poteva
        // essere il metodo di un'altra interfaccia (R20261121-002056-S3). Primo metodo di Statics: get_Current
        static const IID IID_Statics = {0xff6123cf, 0x4386, 0x4aa3, {0xb7, 0x3d, 0xb8, 0x04, 0xe5, 0xb6, 0x3b, 0x23}};
        static const IID IID_Ascolta = {0x62553e41, 0x8a06, 0x4cef, {0x82, 0x15, 0x60, 0x33, 0xa5, 0xbe, 0x4b, 0x03}};
        void* stat = nullptr;
        fab->QueryInterface(IID_Statics, &stat);
        fab->Release();
        if (!stat) return false;
        IInspectable* corrente = nullptr;
        metodo<FnPtr>(stat, 6)(stat, (void**)&corrente);
        rilascia(stat);
        if (!corrente) return false;
        corrente->QueryInterface(IID_Ascolta, &ascolta);
        corrente->Release();
        if (!ascolta) return false;
        int accesso = 0;
        metodo<HRESULT (STDMETHODCALLTYPE*)(void*, int*)>(ascolta, 7)(ascolta, &accesso);   // GetAccessStatus
        if (accesso != 1) { rilascia(ascolta); ascolta = nullptr; return false; }
        return true;
    }
};

} // namespace notifiche
