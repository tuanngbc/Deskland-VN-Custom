// Comandi delle app (jump list): quelli che Windows mostra col tasto destro sull'icona della barra, come "Nuova
// finestra in incognito" di Chrome e Brave o "I miei giochi" di Steam (issue #24). FluidWin li legge dai file che le
// app stesse fanno scrivere a Windows in %APPDATA%\Microsoft\Windows\Recent\CustomDestinations: solo lettura di file,
// niente codice nelle altre app, nessuno stato di Windows cambiato.
//
// Formato (ricostruito e verificato su tutti i file di questo PC, 19 novembre 2026): versione (2), numero di
// categorie, un valore da saltare; per categoria il tipo (0 categoria con nome, 1 categoria nota di Windows, 2 compiti
// dell'app), per il tipo 0 il nome (lunghezza in caratteri + UTF-16), poi quante voci; ogni voce e' il CLSID del
// collegamento seguito dal collegamento salvato (IPersistStream). Ogni categoria finisce con 0xBABFFBAB.
// Nome del file: CRC-64 (polinomio riflesso 0x92C64265D32139A4, partenza tutti 1, nessuno xor finale) dell'AppUserModelID
// in maiuscolo, in esadecimale minuscolo senza zeri davanti. Verificato: Impostazioni, Brave, Claude, Spotify, Store,
// Blocco note, Esplora file, Collegamento al telefono, ChatGPT e Steam ("Valve.Steam.Client").
// Le app che si danno l'AppUserModelID da sole all'avvio (Steam) non lo espongono all'esterno: per loro si sceglie il
// file i cui comandi aprono lo stesso eseguibile della finestra, fra quelli che nessun'altra app del dock ha gia' preso.
#pragma once
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <shlobj.h>
#include <shobjidl.h>
#include <shlwapi.h>
#include <shellapi.h>
#include <propkey.h>
#include <propvarutil.h>
#include <wincodec.h>
#include <string>
#include <memory>
#include <atomic>
#include <initializer_list>
#include <vector>
#include <unordered_map>
#include <cstdint>
#include <cwchar>
#include <cwctype>
#include <algorithm>

namespace jumplist {

struct Comando {
    std::wstring titolo;
    std::vector<BYTE> pidl;                 // bersaglio come lista di ID della shell: si avvia come fa Windows
    std::wstring percorso, argomenti, cartella;   // percorso dell'eseguibile (abbinamento, icona)
    std::wstring aumid, attivazione;        // app dello Store: si avviano con il loro sistema (ActivateApplication)
    std::wstring iconaFile;
    int iconaIndice = 0;
    bool separatore = false;
    std::vector<uint32_t> pixel;            // icona gia' pronta (LATO_ICONA x LATO_ICONA, BGRA premoltiplicato), o vuota
};
struct Categoria { std::wstring nome; bool compiti = false; std::vector<Comando> voci; };
struct Elenco {
    std::vector<Categoria> categorie;
    bool vuoto() const { for (const auto& c : categorie) for (const auto& v : c.voci) if (!v.separatore) return false; return true; }
};

// identificativo del file di un AppUserModelID (vedi in cima)
inline uint64_t idApp(std::wstring aumid) {
    CharUpperBuffW(aumid.data(), (DWORD)aumid.size());
    uint64_t c = ~0ULL;
    const BYTE* b = (const BYTE*)aumid.data();
    for (size_t i = 0; i < aumid.size() * sizeof(wchar_t); i++) {
        c ^= b[i];
        for (int k = 0; k < 8; k++) c = (c & 1) ? (c >> 1) ^ 0x92C64265D32139A4ULL : c >> 1;
    }
    return c;
}
inline std::wstring nomeFile(const std::wstring& aumid) { wchar_t b[24]; swprintf(b, 24, L"%llx", (unsigned long long)idApp(aumid)); return b; }

inline std::wstring minuscolo(std::wstring s) { for (auto& c : s) c = (wchar_t)towlower(c); return s; }

inline std::wstring espandi(const std::wstring& s) {
    if (s.find(L'%') == std::wstring::npos) return s;
    wchar_t b[MAX_PATH * 2];
    const DWORD n = ExpandEnvironmentStringsW(s.c_str(), b, MAX_PATH * 2);
    return n && n <= MAX_PATH * 2 ? std::wstring(b) : s;
}

// Un file da cui si puo' leggere sul thread principale senza aspettare la rete: un nome senza cartella (DLL di sistema,
// cercata da Windows sul PC) o un percorso su un disco del PC. No: \\server\..., \\?\UNC\..., unita' di rete, percorsi
// relativi con cartelle. R20261119-140644-S1: un titolo "@\\server\x.dll,-1" in una jump list qualsiasi fermava FluidWin
// finche' il server non rispondeva
inline bool locale(const std::wstring& p) {
    if (p.empty()) return false;
    if (p.find_first_of(L"\\/") == std::wstring::npos) return p.find(L':') == std::wstring::npos;   // solo un nome
    std::wstring q = p;
    if (q.rfind(L"\\\\?\\", 0) == 0) q = q.substr(4);   // \\?\C:\... (\\?\UNC\... non ha la lettera)
    if (q.size() < 3 || !iswalpha(q[0]) || q[1] != L':' || (q[2] != L'\\' && q[2] != L'/')) return false;
    const wchar_t radice[4] = {q[0], L':', L'\\', 0};
    const UINT t = GetDriveTypeW(radice);
    return t == DRIVE_FIXED || t == DRIVE_REMOVABLE || t == DRIVE_RAMDISK || t == DRIVE_CDROM;
}

// Riferimento a una risorsa ("@file,-id" o "@{pacchetto?ms-resource://...}") che si puo' risolvere senza la rete
inline bool risorsaLocale(const std::wstring& s) {
    if (s.size() < 2 || s[0] != L'@') return false;
    if (s[1] == L'{') {
        // "@{NomeCompletoDelPacchetto?ms-resource://...}" (pacchetto installato, sul PC) oppure "@{percorso\file.pri?...}":
        // il secondo e' un file come gli altri (R20261119-141543: un .pri su \\server passava il filtro)
        const size_t q = s.find(L'?');
        const std::wstring dentro = s.substr(2, q == std::wstring::npos ? std::wstring::npos : q - 2);
        return dentro.find_first_of(L"\\/") == std::wstring::npos ? dentro.find(L':') == std::wstring::npos : locale(espandi(dentro));
    }
    const size_t v = s.rfind(L',');
    return locale(espandi(s.substr(1, v == std::wstring::npos ? std::wstring::npos : v - 1)));
}

namespace dettaglio {

// {9F4C2855-...},5 e ,20: AppUserModelID e contesto d'attivazione dei comandi delle app dello Store
inline const PROPERTYKEY CHIAVE_AUMID = {{0x9f4c2855, 0x9f79, 0x4b39, {0xa8, 0xd0, 0xe1, 0xd4, 0x2d, 0xe1, 0xd5, 0xf3}}, 5};
inline const PROPERTYKEY CHIAVE_ATTIVAZIONE = {{0x9f4c2855, 0x9f79, 0x4b39, {0xa8, 0xd0, 0xe1, 0xd4, 0x2d, 0xe1, 0xd5, 0xf3}}, 20};
constexpr DWORD FINE_CATEGORIA = 0xBABFFBAB;
constexpr DWORD MAX_FILE = 2 * 1024 * 1024, MAX_CATEGORIE = 32, MAX_VOCI = 100, MAX_NOME = 256;

inline std::wstring testo(IPropertyStore* p, const PROPERTYKEY& k) {
    std::wstring r;
    PROPVARIANT v; PropVariantInit(&v);
    if (SUCCEEDED(p->GetValue(k, &v)) && v.vt != VT_EMPTY) {   // (il titolo puo' essere VT_LPWSTR o VT_BSTR: lo Store usa BSTR)
        PWSTR s = nullptr;
        if (SUCCEEDED(PropVariantToStringAlloc(v, &s)) && s) { r = s; CoTaskMemFree(s); }
    }
    PropVariantClear(&v);
    return r;
}

// una voce: il collegamento salvato nel flusso, dalla posizione attuale (che IPersistStream::Load porta alla fine)
inline bool leggiVoce(IStream* s, Comando& c) {
    IShellLinkW* sl = nullptr;
    if (FAILED(CoCreateInstance(CLSID_ShellLink, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&sl))) || !sl) return false;
    IPersistStream* ps = nullptr;
    const bool ok = SUCCEEDED(sl->QueryInterface(IID_PPV_ARGS(&ps))) && ps && SUCCEEDED(ps->Load(s));
    if (ps) ps->Release();
    bool intero = true;   // false: un testo non e' entrato nel buffer (la voce si scarta: mai un comando troncato)
    if (ok) {
        // Buffer grande quanto la riga di comando piu' lunga che Windows accetta (32767). Un testo che lo riempie e'
        // troncato: GetArguments lo tronca in silenzio (documentato), e un comando con gli argomenti tagliati farebbe
        // altro da quello che il titolo promette. R20261119-140644-S3: prima il buffer era di 4096 e si eseguiva
        // comunque ("Nuova finestra privata" con --incognito oltre il taglio apriva una finestra normale)
        std::vector<wchar_t> b(32768, L'\0');
        const size_t pieno = b.size() - 1;
        auto preso = [&](HRESULT hr, std::wstring& dove, bool espandere) {
            if (FAILED(hr)) { b[0] = 0; return; }
            b.back() = 0;
            if (wcslen(b.data()) >= pieno) intero = false;
            dove = espandere ? espandi(b.data()) : std::wstring(b.data());
            b[0] = 0;
        };
        b[0] = 0; preso(sl->GetPath(b.data(), (int)b.size(), nullptr, SLGP_RAWPATH), c.percorso, true);
        preso(sl->GetArguments(b.data(), (int)b.size()), c.argomenti, false);
        preso(sl->GetWorkingDirectory(b.data(), (int)b.size()), c.cartella, true);
        int ix = 0;
        std::wstring ic;
        preso(sl->GetIconLocation(b.data(), (int)b.size(), &ix), ic, true);
        if (!ic.empty()) { c.iconaFile = ic; c.iconaIndice = ix; }
        PIDLIST_ABSOLUTE id = nullptr;
        if (SUCCEEDED(sl->GetIDList(&id)) && id) {
            const UINT n = ILGetSize(id);
            if (n > 0 && n < 64 * 1024) c.pidl.assign((const BYTE*)id, (const BYTE*)id + n);
            CoTaskMemFree(id);
        }
        IPropertyStore* pr = nullptr;
        if (SUCCEEDED(sl->QueryInterface(IID_PPV_ARGS(&pr))) && pr) {
            c.titolo = testo(pr, PKEY_Title);
            PROPVARIANT v; PropVariantInit(&v);
            if (SUCCEEDED(pr->GetValue(PKEY_AppUserModel_IsDestListSeparator, &v)) && v.vt == VT_BOOL) c.separatore = v.boolVal != VARIANT_FALSE;
            PropVariantClear(&v);
            c.aumid = testo(pr, CHIAVE_AUMID);
            c.attivazione = testo(pr, CHIAVE_ATTIVAZIONE);
            pr->Release();
        }
        // titolo da una risorsa ("@C:\...\app.dll,-123" o "@{pacchetto?ms-resource://...}"), come fa la barra; solo se
        // la risorsa e' sul PC (vedi locale), altrimenti la voce resta senza titolo e non si mostra
        if (!c.titolo.empty() && c.titolo[0] == L'@') {
            wchar_t t[512];
            if (risorsaLocale(c.titolo) && SUCCEEDED(SHLoadIndirectString(c.titolo.c_str(), t, 512, nullptr))) c.titolo = t; else c.titolo.clear();
        }
        if (c.titolo.size() > 200) c.titolo = c.titolo.substr(0, 199) + L"…";
        if (!intero) c.titolo.clear();   // (testo troncato: la voce si scarta, vedi sopra)
    }
    sl->Release();
    return ok;
}

}  // namespace dettaglio

// Legge un file. Con un file rovinato o di un formato diverso tiene quello che ha letto fino a li' (le categorie
// complete) e si ferma: niente letture oltre la fine (il flusso in memoria le rifiuta) e limiti a categorie e voci
inline Elenco leggiFile(const std::wstring& percorso) {
    using namespace dettaglio;
    Elenco e;
    HANDLE h = CreateFileW(percorso.c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr, OPEN_EXISTING, 0, nullptr);
    if (h == INVALID_HANDLE_VALUE) return e;
    LARGE_INTEGER dim{};
    std::vector<BYTE> d;
    if (GetFileSizeEx(h, &dim) && dim.QuadPart >= 12 && dim.QuadPart <= MAX_FILE) {
        d.resize((size_t)dim.QuadPart);
        DWORD n = 0;
        if (!ReadFile(h, d.data(), (DWORD)d.size(), &n, nullptr)) n = 0;
        d.resize(n);
    }
    CloseHandle(h);
    if (d.size() < 12) return e;
    IStream* s = SHCreateMemStream(d.data(), (UINT)d.size());
    if (!s) return e;
    auto leggi = [&](void* p, ULONG n) { ULONG l = 0; return SUCCEEDED(s->Read(p, n, &l)) && l == n; };
    DWORD versione = 0, quante = 0, ignoto = 0;
    if (leggi(&versione, 4) && leggi(&quante, 4) && leggi(&ignoto, 4) && versione == 2) {
        for (DWORD c = 0; c < quante && c < MAX_CATEGORIE; c++) {
            DWORD tipo = 0;
            if (!leggi(&tipo, 4)) break;
            Categoria cat;
            if (tipo == 1) {   // categoria nota (recenti, frequenti): i suoi elementi stanno altrove
                DWORD id = 0, fine = 0;
                if (!leggi(&id, 4) || !leggi(&fine, 4) || fine != FINE_CATEGORIA) break;
                continue;
            }
            if (tipo == 0) {
                WORD lun = 0;
                if (!leggi(&lun, 2) || lun > MAX_NOME) break;
                cat.nome.assign(lun, L'\0');
                if (lun && !leggi(cat.nome.data(), lun * 2)) break;
                if (!cat.nome.empty() && cat.nome[0] == L'@') {   // (come i titoli: solo risorse sul PC)
                    wchar_t t[256];
                    if (risorsaLocale(cat.nome) && SUCCEEDED(SHLoadIndirectString(cat.nome.c_str(), t, 256, nullptr))) cat.nome = t; else cat.nome.clear();
                }
            } else if (tipo == 2) cat.compiti = true;
            else break;   // tipo sconosciuto: il resto non si sa leggere
            DWORD num = 0;
            if (!leggi(&num, 4) || num > MAX_VOCI) break;
            bool rotta = false;
            for (DWORD i = 0; i < num; i++) {
                GUID g{};
                if (!leggi(&g, sizeof(g)) || g != CLSID_ShellLink) { rotta = true; break; }
                Comando v;
                if (!leggiVoce(s, v)) { rotta = true; break; }
                if (v.separatore || !v.titolo.empty()) cat.voci.push_back(std::move(v));   // (senza titolo non si mostra)
            }
            DWORD fine = 0;
            if (rotta || !leggi(&fine, 4) || fine != FINE_CATEGORIA) break;
            e.categorie.push_back(std::move(cat));
        }
    }
    s->Release();
    return e;
}

// Avvia un comando come la barra di Windows: le app dello Store con ActivateApplication (AppUserModelID e contesto),
// le altre con il loro bersaglio e gli argomenti del collegamento
inline bool avvia(const Comando& c) {
    // (anche i comandi di Chrome e Brave possono portare l'AppUserModelID, per raggruppare le finestre: per loro conta il
    // file da aprire. ActivateApplication solo se il bersaglio non e' un file, come per le app dello Store, il cui
    // "percorso" e' il nome dell'app nel pacchetto)
    // (il controllo del file solo se serve, cioe' con un AUMID: senza, su una cartella di rete scollegata fermava il
    // dock per niente, R20261121-001416-S12)
    const bool file = !c.aumid.empty() && !c.percorso.empty() && GetFileAttributesW(c.percorso.c_str()) != INVALID_FILE_ATTRIBUTES;
    if (!c.aumid.empty() && !file) {
        IApplicationActivationManager* am = nullptr;
        bool ok = false;
        if (SUCCEEDED(CoCreateInstance(CLSID_ApplicationActivationManager, nullptr, CLSCTX_LOCAL_SERVER, IID_PPV_ARGS(&am))) && am) {
            DWORD pid = 0;
            ok = SUCCEEDED(am->ActivateApplication(c.aumid.c_str(), c.attivazione.empty() ? nullptr : c.attivazione.c_str(), AO_NONE, &pid));
            am->Release();
        }
        if (ok) return true;
    }
    SHELLEXECUTEINFOW si{sizeof(si)};
    si.nShow = SW_SHOWNORMAL;
    si.lpParameters = c.argomenti.empty() ? nullptr : c.argomenti.c_str();
    si.lpDirectory = c.cartella.empty() ? nullptr : c.cartella.c_str();
    if (!c.pidl.empty()) { si.fMask = SEE_MASK_IDLIST; si.lpIDList = (void*)c.pidl.data(); }
    else if (!c.percorso.empty()) si.lpFile = c.percorso.c_str();
    else return false;
    return ShellExecuteExW(&si) != FALSE;
}

// Icona piccola del comando (lato in pixel), o nullptr. La libera chi la riceve (DestroyIcon). Senza un'icona propria,
// quella del programma che il comando apre
inline HICON icona(const Comando& c, int lato) {
    const std::wstring f = !c.iconaFile.empty() ? c.iconaFile : c.percorso;
    if (f.empty() || !locale(f)) return nullptr;   // (anche "ms-appx:..." delle app dello Store; rete: R20261119-140644-S1)
    HICON h = nullptr;
    if (SHDefExtractIconW(f.c_str(), c.iconaFile.empty() ? 0 : c.iconaIndice, 0, &h, nullptr, (UINT)lato) != S_OK) h = nullptr;
    return h;
}

// L'icona del comando come pixel BGRA premoltiplicati, lato x lato (vuoto se non c'e'): cosi' passa dal thread di
// lettura al menu senza handle di Windows. Serve COM inizializzato nel thread che la chiama (WIC)
inline std::vector<uint32_t> pixelIcona(const Comando& c, int lato) {
    std::vector<uint32_t> px;
    HICON h = icona(c, lato);
    if (!h) return px;
    IWICImagingFactory* w = nullptr; IWICBitmap* b = nullptr; IWICFormatConverter* cv = nullptr;
    UINT lw = 0, lh = 0;
    if (SUCCEEDED(CoCreateInstance(CLSID_WICImagingFactory, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&w))) && w &&
        SUCCEEDED(w->CreateBitmapFromHICON(h, &b)) && b && SUCCEEDED(b->GetSize(&lw, &lh)) && lw == (UINT)lato && lh == (UINT)lato &&
        SUCCEEDED(w->CreateFormatConverter(&cv)) && cv &&
        SUCCEEDED(cv->Initialize(b, GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, nullptr, 0, WICBitmapPaletteTypeCustom))) {
        px.resize((size_t)lato * lato);
        if (FAILED(cv->CopyPixels(nullptr, lato * 4, (UINT)(px.size() * 4), (BYTE*)px.data()))) px.clear();
    }
    for (IUnknown* o : std::initializer_list<IUnknown*>{cv, b, w}) if (o) o->Release();
    DestroyIcon(h);
    return px;
}

// Lato delle icone preparate (pixel): il menu le disegna a 16 punti, cioe' fino a 32 pixel al 200%
constexpr int LATO_ICONA = 32;

// Tutti i file della cartella, tenuti pronti da un thread a parte (avvia / ferma): il menu, sul thread principale,
// prende solo l'ultima copia pubblicata, senza mai toccare file, risorse o rete. R20261119-141543 (S1 del giro 1
// rimasta aperta): un filtro sul testo dei percorsi non vede una cartella di C: che e' un collegamento verso la rete,
// ne' quanto puo' metterci Windows a rispondere su un'unita' scollegata; ora qualunque attesa capita nel thread.
// Il thread rilegge quando Windows avvisa che la cartella e' cambiata (FindFirstChangeNotification), quando il menu
// chiede (richiedi) e, senza avvisi (cartella non ancora creata), ogni 30 s. Un file si rilegge solo se ne cambiano
// data o misura
class Indice {
public:
    Indice() = default;
    Indice(const Indice&) = delete;
    Indice& operator=(const Indice&) = delete;
    ~Indice() {
        // (all'uscita normale ferma ha gia' chiuso il thread; se non ha potuto, il processo e' stato chiuso prima dei
        // distruttori statici: qui il thread non puo' essere vivo)
        if (fermo) CloseHandle(fermo);
        if (richiesta) CloseHandle(richiesta);
        if (thread) CloseHandle(thread);
    }

    // parte il thread (una volta). false: non e' partito, e il menu non avra' i comandi delle app
    bool avvia() {
        if (thread) return true;
        fermo = CreateEventW(nullptr, TRUE, FALSE, nullptr);
        richiesta = CreateEventW(nullptr, FALSE, FALSE, nullptr);   // (la prima lettura il thread la fa da se')
        if (!fermo || !richiesta) return false;
        thread = CreateThread(nullptr, 0, lavoro, this, 0, nullptr);
        return thread != nullptr;
    }

    // All'uscita: ferma il thread e lo aspetta al massimo ms. false: non ha finito (dentro una lettura appesa); il
    // chiamante chiude il processo senza i distruttori statici
    bool ferma(DWORD ms) {
        if (!thread) return true;
        SetEvent(fermo);
        return WaitForSingleObject(thread, ms) == WAIT_OBJECT_0;
    }

    // chiede al thread di ricontrollare la cartella (dopo l'apertura di un menu: un avviso di Windows perso non conta)
    void richiedi() { if (richiesta) SetEvent(richiesta); }

    // la prima lettura e' stata pubblicata (per il registro e le prove)
    bool pronto() const { return pubblicato.load(); }

    // TLauncher: called only on the temporary Shell worker, never on the UI.
    // Reuse the original bounded parser/cache without a permanent watcher.
    void leggiUnaVolta() {
        Mappa cache;const auto dir=cartellaJumpList();
        HANDLE stop=CreateEventW(nullptr,TRUE,FALSE,nullptr);
        if(stop){if(!dir.empty()&&locale(dir))rileggi(dir,cache,stop);CloseHandle(stop);}
        pubblica(std::make_shared<const Mappa>(std::move(cache)));
    }

    // Comandi dell'app: aumid (anche piu' d'uno: quello della finestra, del collegamento fissato, del pacchetto) ed
    // eseguibile della finestra (minuscolo). altri: gli AppUserModelID delle altre app del dock (i loro file non si
    // prendono col piano B). Elenco vuoto se l'app non ne ha o se il thread non ha ancora letto niente.
    // Dal thread principale: non legge file, prende solo la copia pubblicata
    Elenco per(const std::vector<std::wstring>& aumid, const std::wstring& exe, const std::vector<std::wstring>& altri, const char** come = nullptr) {
        std::shared_ptr<const Mappa> file;
        AcquireSRWLockShared(&blocco); file = copia; ReleaseSRWLockShared(&blocco);
        if (!file) return {};
        bool identita = false;
        for (const auto& a : aumid) {
            if (a.empty()) continue;
            identita = true;
            auto it = file->find(nomeFile(a));
            if (it != file->end()) { if (come) *come = "aumid"; return it->second->elenco; }
        }
        // Piano B solo per le app senza nessun identificativo noto, e solo con un unico file possibile.
        // R20261119-140644-S2: prima valeva anche per un'app con il suo identificativo ma senza il file, e con piu' file
        // vinceva il piu' recente: il profilo A di un browser poteva mostrare (e aprire) i comandi del profilo B, che
        // usa lo stesso eseguibile. Nel dubbio nessun comando, come senza jump list
        if (identita || exe.empty()) return {};
        std::vector<std::wstring> presi;
        for (const auto& a : altri) if (!a.empty()) presi.push_back(nomeFile(a));
        const File* scelto = nullptr;
        for (const auto& [nome, f] : *file) {
            if (std::find(presi.begin(), presi.end(), nome) != presi.end()) continue;
            if (std::find(f->bersagli.begin(), f->bersagli.end(), exe) == f->bersagli.end()) continue;
            if (scelto) { if (come) *come = "eseguibile ambiguo"; return {}; }
            scelto = f.get();
        }
        if (scelto && come) *come = "eseguibile";
        return scelto ? scelto->elenco : Elenco{};
    }

private:
    // Un file letto: non cambia piu' dopo la pubblicazione (lo condividono la cache del thread e le copie pubblicate)
    struct File { FILETIME scritto{}; ULONGLONG misura = 0; Elenco elenco; std::vector<std::wstring> bersagli; };
    using Mappa = std::unordered_map<std::wstring, std::shared_ptr<const File>>;   // per nome (identificativo esadecimale)

    HANDLE thread = nullptr, fermo = nullptr, richiesta = nullptr;
    SRWLOCK blocco = SRWLOCK_INIT;
    std::shared_ptr<const Mappa> copia;     // l'ultima pubblicata (sotto blocco)
    std::atomic<bool> pubblicato{false};

    void pubblica(std::shared_ptr<const Mappa> m) {
        AcquireSRWLockExclusive(&blocco); copia = std::move(m); ReleaseSRWLockExclusive(&blocco);
        pubblicato = true;
    }

    // cartella delle jump list, o "" (variabile mancante, percorso troppo lungo)
    static std::wstring cartellaJumpList() {
        // (R20261119-140644-S4: un ritorno oltre la misura del buffer vuol dire "non basta", non "fatto": la cartella si
        // espande in un buffer grande quanto serve, chiesto prima)
        const wchar_t* modello = L"%APPDATA%\\Microsoft\\Windows\\Recent\\CustomDestinations\\";
        const DWORD serve = ExpandEnvironmentStringsW(modello, nullptr, 0);
        if (!serve || serve > 32768) return L"";
        std::vector<wchar_t> d(serve + 1, L'\0');
        const DWORD scritti = ExpandEnvironmentStringsW(modello, d.data(), (DWORD)d.size());
        if (!scritti || scritti > d.size()) return L"";
        return d.data();
    }

    // Rilegge la cartella nel thread: i file con la stessa data e misura restano quelli della cache
    static void rileggi(const std::wstring& cartella, Mappa& cache, HANDLE fermo) {
        Mappa nuovi;
        WIN32_FIND_DATAW fd;
        HANDLE f = FindFirstFileW((cartella + L"*.customDestinations-ms").c_str(), &fd);
        if (f == INVALID_HANDLE_VALUE) { cache.clear(); return; }
        int n = 0;
        do {
            if (WaitForSingleObject(fermo, 0) == WAIT_OBJECT_0) break;   // (uscita: si smette subito)
            if (fd.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) continue;
            if (++n > 500) break;   // (una cartella enorme: le prime 500)
            std::wstring nome = fd.cFileName;
            nome = minuscolo(nome.substr(0, nome.find(L'.')));
            const ULONGLONG misura = ((ULONGLONG)fd.nFileSizeHigh << 32) | fd.nFileSizeLow;
            auto it = cache.find(nome);
            if (it != cache.end() && CompareFileTime(&it->second->scritto, &fd.ftLastWriteTime) == 0 && it->second->misura == misura) {
                nuovi[nome] = it->second;
                continue;
            }
            auto x = std::make_shared<File>();
            x->scritto = fd.ftLastWriteTime; x->misura = misura;
            x->elenco = leggiFile(cartella + fd.cFileName);
            for (auto& c : x->elenco.categorie)
                for (auto& v : c.voci) {
                    if (!v.separatore) v.pixel = pixelIcona(v, LATO_ICONA);
                    if (!v.percorso.empty()) {
                        const std::wstring p = minuscolo(v.percorso);
                        if (std::find(x->bersagli.begin(), x->bersagli.end(), p) == x->bersagli.end()) x->bersagli.push_back(p);
                    }
                }
            nuovi[nome] = std::move(x);
        } while (FindNextFileW(f, &fd));
        FindClose(f);
        cache = std::move(nuovi);
    }

    static DWORD WINAPI lavoro(LPVOID p) {
        Indice* me = (Indice*)p;
        const HRESULT co = CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);   // (collegamenti, risorse, WIC)
        Mappa cache;
        HANDLE avvisi = INVALID_HANDLE_VALUE;
        for (;;) {
            const std::wstring cartella = cartellaJumpList();
            // %APPDATA% su una cartella di rete (reindirizzamento delle cartelle nelle aziende): niente comandi delle app
            // (le letture, anche qui, aspetterebbero la rete a ogni avviso; e l'uscita aspetterebbe loro)
            if (cartella.empty() || !locale(cartella)) cache.clear();
            else {
                rileggi(cartella, cache, me->fermo);
                if (avvisi == INVALID_HANDLE_VALUE)
                    avvisi = FindFirstChangeNotificationW(cartella.c_str(), FALSE, FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_SIZE);
            }
            me->pubblica(std::make_shared<const Mappa>(cache));
            HANDLE h[3] = {me->fermo, me->richiesta, avvisi};
            const DWORD quanti = avvisi != INVALID_HANDLE_VALUE ? 3 : 2;
            const DWORD w = WaitForMultipleObjects(quanti, h, FALSE, avvisi != INVALID_HANDLE_VALUE ? INFINITE : 30000);
            if (w == WAIT_OBJECT_0 || w == WAIT_FAILED) break;
            if (w == WAIT_OBJECT_0 + 2) {
                // Windows scrive un file in piu' passi: si aspetta che finisca (200 ms) prima di rileggere, senza
                // perdere l'uscita
                if (!FindNextChangeNotification(avvisi)) { FindCloseChangeNotification(avvisi); avvisi = INVALID_HANDLE_VALUE; }
                if (WaitForSingleObject(me->fermo, 200) == WAIT_OBJECT_0) break;
            }
        }
        if (avvisi != INVALID_HANDLE_VALUE) FindCloseChangeNotification(avvisi);
        cache.clear();
        if (SUCCEEDED(co)) CoUninitialize();
        return 0;
    }
};

}  // namespace jumplist
