// "Condividi" per i file, con la finestra di condivisione di Windows: come fanno le app desktop
// (IDataTransferManagerInterop::GetForWindow, evento DataRequested, ShowShareUIForWindow).
// Serve al menu del tasto destro (destro.h): la voce "Condividi" del menu di Esplora file e' un comando interno della
// vista, che chiamato da un altro processo non fa niente (provato: nessuna finestra). Qui i file si danno a Windows
// come StorageFile, e la finestra di condivisione si apre sopra una finestrella di FluidWin (1x1, quasi trasparente)
// messa dove si e' cliccato.
// MinGW ha le interfacce (windows.applicationmodel.datatransfer.h, windows.storage.h) ma solo dichiarati gli IID dei
// tipi generici e IStorageFileStatics vuota: gli IID sono scritti qui (dalle intestazioni) e GetFileFromPathAsync si
// chiama per posizione (primo metodo dopo IInspectable, come in musica.h).
#pragma once
#include "tlauncher_identity.h"
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <shobjidl.h>
#include <roapi.h>
#include <winstring.h>
#include <windows.storage.h>
#include <windows.applicationmodel.datatransfer.h>
#include <string>
#include <vector>
#include <atomic>
#include <functional>
#include <memory>

namespace condividi {

namespace dettagli {
namespace DT = ABI::Windows::ApplicationModel::DataTransfer;
namespace ST = ABI::Windows::Storage;
namespace FO = ABI::Windows::Foundation;
namespace CO = ABI::Windows::Foundation::Collections;

static const IID IID_Elenco = {0xbb8b8418, 0x65d1, 0x544b, {0xb0, 0x83, 0x6d, 0x17, 0x2f, 0x56, 0x8c, 0x73}};    // IIterable<IStorageItem>
static const IID IID_Iter = {0x05b487c2, 0x3830, 0x5d3c, {0x98, 0xda, 0x25, 0xfa, 0x11, 0x54, 0x2d, 0xbd}};      // IIterator<IStorageItem>
static const IID IID_Gestore = {0xec6f9cc8, 0x46d0, 0x5e0e, {0xb4, 0xd2, 0x7d, 0x77, 0x73, 0xae, 0x37, 0xa0}};   // TypedEventHandler<DTM, DataRequestedEventArgs>
static const IID IID_Insp = {0xaf86e2e0, 0xb12d, 0x4c6a, {0x9c, 0x5a, 0xd7, 0xaa, 0x65, 0x10, 0x1e, 0x90}};      // IInspectable
static const IID IID_StatFile = {0x5984c710, 0xdaf2, 0x43c8, {0x8b, 0xb4, 0xa4, 0xd3, 0xea, 0xcf, 0xd0, 0x3f}};  // IStorageFileStatics

struct Stringa {
    HSTRING s = nullptr;
    explicit Stringa(const std::wstring& t) { if (FAILED(WindowsCreateString(t.c_str(), (UINT32)t.size(), &s))) s = nullptr; }
    ~Stringa() { if (s) WindowsDeleteString(s); }
};

// IIterable<IStorageItem*> (e il suo IIterator) sopra un vettore: e' quello che vuole SetStorageItemsReadOnly
struct Iteratore final : CO::IIterator<ST::IStorageItem*> {
    std::atomic<ULONG> rif{1}; std::vector<ST::IStorageItem*> v; size_t i = 0;
    explicit Iteratore(const std::vector<ST::IStorageItem*>& x) : v(x) { for (auto* p : v) p->AddRef(); }
    ~Iteratore() { for (auto* p : v) p->Release(); }
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID r, void** p) override {
        if (r == IID_IUnknown || r == IID_Insp || r == IID_Iter) { *p = this; AddRef(); return S_OK; }
        *p = nullptr; return E_NOINTERFACE;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return ++rif; }
    ULONG STDMETHODCALLTYPE Release() override { const ULONG n = --rif; if (!n) delete this; return n; }
    HRESULT STDMETHODCALLTYPE GetIids(ULONG* n, IID** ids) override { *n = 0; *ids = nullptr; return S_OK; }
    HRESULT STDMETHODCALLTYPE GetRuntimeClassName(HSTRING* s) override { *s = nullptr; return S_OK; }
    HRESULT STDMETHODCALLTYPE GetTrustLevel(TrustLevel* t) override { *t = BaseTrust; return S_OK; }
    HRESULT STDMETHODCALLTYPE get_Current(ST::IStorageItem** c) override { if (i >= v.size()) return E_BOUNDS; *c = v[i]; v[i]->AddRef(); return S_OK; }
    HRESULT STDMETHODCALLTYPE get_HasCurrent(boolean* b) override { *b = i < v.size(); return S_OK; }
    HRESULT STDMETHODCALLTYPE MoveNext(boolean* b) override { if (i < v.size()) i++; *b = i < v.size(); return S_OK; }
    HRESULT STDMETHODCALLTYPE GetMany(UINT32 n, ST::IStorageItem** a, UINT32* presi) override {
        UINT32 k = 0;
        for (; k < n && i < v.size(); k++, i++) { a[k] = v[i]; v[i]->AddRef(); }
        *presi = k; return S_OK;
    }
};
struct Elenco final : CO::IIterable<ST::IStorageItem*> {
    std::atomic<ULONG> rif{1}; std::vector<ST::IStorageItem*> v;
    ~Elenco() { for (auto* p : v) p->Release(); }
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID r, void** p) override {
        if (r == IID_IUnknown || r == IID_Insp || r == IID_Elenco) { *p = this; AddRef(); return S_OK; }
        *p = nullptr; return E_NOINTERFACE;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return ++rif; }
    ULONG STDMETHODCALLTYPE Release() override { const ULONG n = --rif; if (!n) delete this; return n; }
    HRESULT STDMETHODCALLTYPE GetIids(ULONG* n, IID** ids) override { *n = 0; *ids = nullptr; return S_OK; }
    HRESULT STDMETHODCALLTYPE GetRuntimeClassName(HSTRING* s) override { *s = nullptr; return S_OK; }
    HRESULT STDMETHODCALLTYPE GetTrustLevel(TrustLevel* t) override { *t = BaseTrust; return S_OK; }
    HRESULT STDMETHODCALLTYPE First(CO::IIterator<ST::IStorageItem*>** it) override { *it = new Iteratore(v); return S_OK; }
};

// Il lavoro non conserva HWND, callback o oggetti UI. Solo il worker scrive il risultato, che il main legge dopo
// finito (release/acquire). Gli StorageItem passano fra appartamenti solo se esplicitamente agili.
struct Lavoro {
    std::vector<std::wstring> percorsi;
    std::vector<ST::IStorageItem*> elementi;
    std::atomic<bool> annulla{false}, finito{false};
    HRESULT esito = E_FAIL;
    ~Lavoro() { for (auto* p : elementi) p->Release(); }
};

// stato: la finestrella, il gestore dell'evento (uno solo, registrato una volta) e i file da dare alla prossima richiesta
struct Stato {
    HWND fin = nullptr;
    DT::IDataTransferManager* dtm = nullptr;
    ::IDataTransferManagerInterop* io = nullptr;
    Elenco* elenco = nullptr;   // i file in condivisione adesso
    std::wstring titolo;
    std::function<void(const char*)> registro;
    HWND ritorno = nullptr;   // a chi ridare il primo piano quando la condivisione si chiude
    bool vista = false;       // la finestra di condivisione e' stata in primo piano
    ULONGLONG t0 = 0;         // quando si e' chiesta
    std::shared_ptr<Lavoro> lavoro;
    HANDLE thread = nullptr;
    POINT punto{};
    HWND primoPiano = nullptr;   // se intanto l'utente cambia app, non aprire la condivisione sopra di lei
    bool fermo = false;
};
inline Stato& stato() { static Stato s; return s; }

// Windows chiede i dati mentre apre la finestra di condivisione
struct Richiesta final : FO::ITypedEventHandler<DT::DataTransferManager*, DT::DataRequestedEventArgs*> {
    std::atomic<ULONG> rif{1};
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID r, void** p) override {
        if (r == IID_IUnknown || r == IID_IAgileObject || r == IID_Gestore) { *p = this; AddRef(); return S_OK; }
        *p = nullptr; return E_NOINTERFACE;
    }
    ULONG STDMETHODCALLTYPE AddRef() override { return ++rif; }
    ULONG STDMETHODCALLTYPE Release() override { const ULONG n = --rif; if (!n) delete this; return n; }
    HRESULT STDMETHODCALLTYPE Invoke(DT::IDataTransferManager*, DT::IDataRequestedEventArgs* a) override {
        Stato& s = stato();
        if (!a || !s.elenco) return S_OK;
        DT::IDataRequest* r = nullptr; DT::IDataPackage* p = nullptr; DT::IDataPackagePropertySet* pr = nullptr;
        HRESULT hr = a->get_Request(&r);
        if (SUCCEEDED(hr) && r) hr = r->get_Data(&p);
        if (SUCCEEDED(hr) && p) hr = p->get_Properties(&pr);
        if (SUCCEEDED(hr) && pr) { Stringa t(s.titolo); if (t.s) pr->put_Title(t.s); }   // (senza titolo Windows rifiuta la condivisione)
        if (SUCCEEDED(hr) && p) hr = p->SetStorageItemsReadOnly(s.elenco);
        if (s.registro) { char b[96]; snprintf(b, 96, "condividi: Windows ha chiesto i file (0x%08lx)", (unsigned long)hr); s.registro(b); }
        if (pr) pr->Release();
        if (p) p->Release();
        if (r) r->Release();
        return S_OK;
    }
};

inline bool attendi(IUnknown* op, const Lavoro& lavoro, ULONGLONG scadenza) {
    ::IAsyncInfo* info = nullptr;
    if (FAILED(op->QueryInterface(IID_PPV_ARGS(&info))) || !info) return false;
    AsyncStatus st = AsyncStatus::Started;
    while (!lavoro.annulla.load() && GetTickCount64() < scadenza) {
        if (FAILED(info->get_Status(&st)) || st != AsyncStatus::Started) break;
        Sleep(1);
    }
    if (st == AsyncStatus::Started) info->Cancel();
    info->Release();
    return st == AsyncStatus::Completed;
}

inline void leggiFile(Lavoro& lavoro) {
    // Un solo budget per tutta la selezione, non 2 s per ognuno dei (fino a 500) file. Anche l'attivazione e le
    // chiamate iniziali, che un provider potrebbe rallentare, restano fuori dal thread dell'interfaccia.
    const ULONGLONG scadenza = GetTickCount64() + 5000;
    IInspectable* sf = nullptr;
    { Stringa c(L"Windows.Storage.StorageFile"); if (!c.s || FAILED(RoGetActivationFactory(c.s, IID_StatFile, (void**)&sf))) sf = nullptr; }
    if (!sf) return;
    typedef HRESULT (STDMETHODCALLTYPE *DaPercorso)(void*, HSTRING, FO::IAsyncOperation<ST::StorageFile*>**);
    bool interrotto = false;
    for (const auto& p : lavoro.percorsi) {
        if (lavoro.annulla.load() || GetTickCount64() >= scadenza) { interrotto = true; break; }
        Stringa t(p); FO::IAsyncOperation<ST::StorageFile*>* op = nullptr; ST::IStorageFile* f = nullptr;
        if (t.s && SUCCEEDED(((DaPercorso)(*(void***)sf)[6])(sf, t.s, &op)) && op && attendi(op, lavoro, scadenza) && SUCCEEDED(op->GetResults(&f)) && f) {
            ST::IStorageItem* it = nullptr; IAgileObject* agile = nullptr;
            if (SUCCEEDED(f->QueryInterface(IID_PPV_ARGS(&it))) && it) {
                if (SUCCEEDED(it->QueryInterface(IID_PPV_ARGS(&agile))) && agile) {
                    agile->Release(); lavoro.elementi.push_back(it);
                } else { it->Release(); interrotto = true; }
            }
        }
        if (f) f->Release();
        if (op) op->Release();
        if (interrotto || lavoro.annulla.load() || GetTickCount64() >= scadenza) { interrotto = true; break; }
    }
    sf->Release();
    // Un timeout non deve aprire una condivisione della sola prima parte della selezione. Lo stesso per un file che non
    // si prepara (sparito intanto): tutti o niente, prima si condivideva il resto senza dirlo (R20261121-002818-S12)
    lavoro.esito = interrotto ? E_ABORT : lavoro.elementi.size() != lavoro.percorsi.size() ? E_FAIL : S_OK;
}

inline DWORD WINAPI preparaFile(LPVOID arg) {
    std::unique_ptr<std::shared_ptr<Lavoro>> dato((std::shared_ptr<Lavoro>*)arg);
    const auto lavoro = std::move(*dato);
    const HRESULT hr = RoInitialize(RO_INIT_MULTITHREADED);
    if (SUCCEEDED(hr)) { leggiFile(*lavoro); RoUninitialize(); }
    else lavoro->esito = hr;
    lavoro->finito.store(true, std::memory_order_release);
    return 0;
}

inline void preparazioneFinita() {
    Stato& s = stato();
    if (!s.lavoro || !s.lavoro->finito.load(std::memory_order_acquire)) return;
    KillTimer(s.fin, 2);
    auto lavoro = std::move(s.lavoro);
    if (s.fermo || lavoro->annulla.load() || FAILED(lavoro->esito)) {
        if (s.registro) s.registro("condividi: preparazione non riuscita o interrotta (limite complessivo 5 s)");
        return;
    }
    const HWND fg = GetForegroundWindow();
    if (fg != s.primoPiano && fg != s.ritorno) {
        if (s.registro) s.registro("condividi: preparazione finita, utente passato a un'altra app");
        return;
    }
    Elenco* el = new Elenco; el->v = std::move(lavoro->elementi);
    if (s.elenco) s.elenco->Release();
    s.elenco = el;
    const std::wstring& primo = lavoro->percorsi[0];
    s.titolo = lavoro->percorsi.size() == 1 ? primo.substr(primo.find_last_of(L'\\') + 1) : std::to_wstring(el->v.size()) + L" file";
    SetWindowPos(s.fin, HWND_TOP, s.punto.x, s.punto.y, 1, 1, SWP_NOACTIVATE | SWP_SHOWWINDOW);
    SetForegroundWindow(s.fin);
    const HRESULT hr = s.io->ShowShareUIForWindow(s.fin);
    s.t0 = GetTickCount64(); s.vista = false;
    if (SUCCEEDED(hr)) SetTimer(s.fin, 1, 250, nullptr);
    else ShowWindow(s.fin, SW_HIDE);
    if (s.registro) { char b[120]; snprintf(b, 120, "condividi: %zu file, finestra di condivisione 0x%08lx", el->v.size(), (unsigned long)hr); s.registro(b); }
}

inline bool diShellExperienceHost(HWND h) {
    DWORD pid = 0; GetWindowThreadProcessId(h, &pid);
    HANDLE p = pid ? OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid) : nullptr;
    wchar_t nome[MAX_PATH] = L""; DWORD n = MAX_PATH;
    const bool ok = p && QueryFullProcessImageNameW(p, 0, nome, &n);
    if (p) CloseHandle(p);
    if (!ok) return false;
    const wchar_t* b = wcsrchr(nome, L'\\');
    return !_wcsicmp(b ? b + 1 : nome, L"ShellExperienceHost.exe");
}
// la finestra in primo piano e' quella della condivisione? Una ApplicationFrameWindow (di explorer.exe) con dentro il
// contenuto di ShellExperienceHost.exe (misurato: guardando solo la finestra in primo piano non la si riconosceva)
inline bool condivisioneDavanti() {
    const HWND fg = GetForegroundWindow();
    if (!fg) return false;
    if (diShellExperienceHost(fg)) return true;
    bool si = false;
    EnumChildWindows(fg, [](HWND h, LPARAM l) -> BOOL { if (diShellExperienceHost(h)) { *(bool*)l = true; return FALSE; } return TRUE; }, (LPARAM)&si);
    return si;
}

// La finestrella resta finche' la condivisione e' aperta: ogni 250 ms si guarda se e' ancora davanti. Chiusa, si
// nasconde e il primo piano torna alla finestra da cui si era partiti (Esplora file, il desktop), se intanto non si e'
// cliccato altrove (Windows altrimenti lo dava a un'app qualsiasi: provato). Al massimo 2 minuti
inline LRESULT CALLBACK proc(HWND h, UINT m, WPARAM w, LPARAM l) {
    Stato& s = stato();
    if (m == WM_TIMER && w == 2) { preparazioneFinita(); return 0; }
    if (m == WM_TIMER && w == 1) {
        const ULONGLONG t = GetTickCount64();
        const bool davanti = condivisioneDavanti();
        s.vista |= davanti;
        if (davanti && t - s.t0 < 120000) return 0;               // aperta
        if (!davanti && !s.vista && t - s.t0 < 3000) return 0;    // non ancora comparsa
        // (chiusa: il primo piano torna di solito alla finestrella, che e' nostra e invisibile)
        KillTimer(h, 1);
        const bool tasti = ((GetAsyncKeyState(VK_LBUTTON) | GetAsyncKeyState(VK_RBUTTON)) & 0x8000) != 0;
        const HWND fg = GetForegroundWindow();
        ShowWindow(h, SW_HIDE);
        // (chiusa con Esc o con la x: nessun tasto premuto; un clic su un'altra app la lascia davanti)
        if (s.vista && !tasti && s.ritorno && IsWindow(s.ritorno) && fg == h) SetForegroundWindow(s.ritorno);
        if (s.registro) s.registro(s.vista ? "condividi: finestra di condivisione chiusa" : "condividi: la finestra di condivisione non e' comparsa");
        return 0;
    }
    return DefWindowProcW(h, m, w, l);
}

// la finestrella e il collegamento con DataTransferManager (una volta sola)
inline bool prepara() {
    Stato& s = stato();
    if (s.fin && s.dtm && s.io) return true;
    if (!s.fin) {
        WNDCLASSW wc{}; wc.lpfnWndProc = proc; wc.hInstance = GetModuleHandleW(nullptr); wc.lpszClassName = L"TLauncher.Native.Condividi";
        RegisterClassW(&wc);
        s.fin = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_LAYERED, wc.lpszClassName, L"", WS_POPUP,
                                0, 0, 1, 1, nullptr, nullptr, wc.hInstance, nullptr);
        if (!s.fin) return false;
        SetLayeredWindowAttributes(s.fin, 0, 1, LWA_ALPHA);
    }
    if (!s.io) { Stringa c(L"Windows.ApplicationModel.DataTransfer.DataTransferManager"); if (!c.s || FAILED(RoGetActivationFactory(c.s, IID_PPV_ARGS(&s.io)))) s.io = nullptr; }
    if (s.io && !s.dtm) {
        if (FAILED(s.io->GetForWindow(s.fin, IID_PPV_ARGS(&s.dtm)))) s.dtm = nullptr;
        if (s.dtm) {
            Richiesta* rq = new Richiesta; EventRegistrationToken tok{};
            if (FAILED(s.dtm->add_DataRequested(rq, &tok))) { s.dtm->Release(); s.dtm = nullptr; }
            rq->Release();   // (lo tiene DataTransferManager finche' la finestrella c'e')
        }
    }
    return s.fin && s.dtm && s.io;
}
}  // namespace dettagli

// Accetta la richiesta e prepara i file in MTA; solo il main, dal timer 2, apre poi la condivisione.
// false: richiesta non avviata; true: accettata (anche se il provider fallisce piu' tardi) o gia' in corso.
inline bool file(const std::vector<std::wstring>& percorsi, POINT pt, HWND ritorno, const std::function<void(const char*)>& registro) {
    using namespace dettagli;
    Stato& s = stato();
    if (s.fermo) return false;
    if (s.lavoro || (s.thread && WaitForSingleObject(s.thread, 0) != WAIT_OBJECT_0)) {
        if (registro) registro("condividi: preparazione gia' in corso");
        return true;   // niente secondo worker ne' fallback sincrono mentre si aspetta il primo
    }
    if (s.thread) { CloseHandle(s.thread); s.thread = nullptr; }
    s.registro = registro;
    if (percorsi.empty() || !prepara()) { if (registro) registro("condividi: finestra di condivisione non disponibile"); return false; }
    s.ritorno = ritorno; s.primoPiano = GetForegroundWindow(); s.punto = pt;
    s.lavoro = std::make_shared<Lavoro>(); s.lavoro->percorsi = percorsi;
    if (!SetTimer(s.fin, 2, 50, nullptr)) { s.lavoro.reset(); return false; }
    auto* arg = new std::shared_ptr<Lavoro>(s.lavoro);
    s.thread = CreateThread(nullptr, 0, preparaFile, arg, 0, nullptr);
    if (!s.thread) {
        delete arg; KillTimer(s.fin, 2); s.lavoro.reset();
        if (registro) registro("condividi: thread della preparazione non avviato");
        return false;
    }
    if (registro) registro("condividi: preparazione dei file in sottofondo");
    return true;
}

// L'uscita chiama prima con 0 per annullare, poi attende dopo il ripristino di Windows. Se un provider non ritorna,
// false permette al main di terminare senza distruttori statici mentre il worker e' ancora vivo.
inline bool ferma(DWORD attesa = 0) {
    auto& s = dettagli::stato();
    s.fermo = true;
    if (s.fin) { KillTimer(s.fin, 2); KillTimer(s.fin, 1); ShowWindow(s.fin, SW_HIDE); }
    if (s.lavoro) s.lavoro->annulla.store(true);
    if (s.thread) {
        if (WaitForSingleObject(s.thread, attesa) != WAIT_OBJECT_0) return false;
        CloseHandle(s.thread); s.thread = nullptr;
    }
    s.lavoro.reset();
    return true;
}

}  // namespace condividi
