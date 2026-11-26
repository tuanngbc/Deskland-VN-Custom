// Aggiornamenti su richiesta. All'avvio (se l'opzione e' accesa) un thread chiede a GitHub l'ultima versione
// pubblicata (API pubblica delle release, nessun dato dell'utente); se e' piu' nuova di questa compare un avviso di vetro
// in alto a destra con "Scarica". Solo con quel clic si scarica lo zip della release, si controllano misura e impronta
// SHA-256 pubblicate da GitHub, si estrae FluidWin.exe (tar.exe di Windows), se ne controlla la versione, si mette al
// posto di quello in uso (rinominato: Windows lo permette anche mentre gira) e FluidWin si riavvia. Al prossimo avvio
// il vecchio si cancella. Se un passo non riesce, il file in uso resta quello di prima e l'avviso offre la pagina della
// release.
// Fonti accettate: solo api.github.com per il controllo e solo gli allegati delle release di clooudfn/FluidWin
// (https://github.com/clooudfn/FluidWin/releases/download/...) per lo zip. Senza impronta pubblicata non si installa.
#pragma once
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <windowsx.h>
#include <shellapi.h>
#include <winhttp.h>
#include <bcrypt.h>
#include <cstdarg>
#include <cstdio>
#include <string>
#include <vector>
#include <functional>
#include <algorithm>
#include <atomic>
#include "vetro.h"
#include "testi.h"
#include "cursori.h"

namespace aggiorna {

inline const wchar_t* REPO = L"/repos/clooudfn/FluidWin/releases/latest";
inline const char* PREFISSO_ZIP = "https://github.com/clooudfn/FluidWin/releases/download/";

// FluidWin si chiude: i thread smettono al prossimo passo (vedi dettagli::fermato)
namespace dettagli { inline std::atomic<bool>& fermato(); }
inline void ferma() { dettagli::fermato() = true; }

struct Versione {
    std::wstring testo;        // "1.2.3"
    std::wstring pagina;       // pagina della release (per scaricarla a mano)
    std::string zip;           // indirizzo dello zip (solo PREFISSO_ZIP)
    unsigned long long misura = 0;
    std::string sha256;        // 64 cifre esadecimali minuscole
};

namespace dettagli {

inline std::function<void(const char*)>& registro() { static std::function<void(const char*)> r; return r; }
inline void scrivi(const char* fmt, ...) {
    if (!registro()) return;
    char b[400]; va_list a; va_start(a, fmt); vsnprintf(b, sizeof(b), fmt, a); va_end(a);
    registro()(b);
}

inline std::wstring daUtf8(const std::string& s) {
    if (s.empty()) return L"";
    const int n = MultiByteToWideChar(CP_UTF8, 0, s.data(), (int)s.size(), nullptr, 0);
    std::wstring w(n > 0 ? n : 0, L'\0');
    if (n > 0) MultiByteToWideChar(CP_UTF8, 0, s.data(), (int)s.size(), w.data(), n);
    return w;
}

// "1.2.3" o "v1.2.3" -> {1,2,3}; vuoto se non e' una versione
inline std::vector<unsigned> numeri(std::wstring s) {
    std::vector<unsigned> v;
    if (!s.empty() && (s[0] == L'v' || s[0] == L'V')) s.erase(0, 1);
    unsigned n = 0; bool cifra = false;
    for (wchar_t c : s) {
        if (c >= L'0' && c <= L'9') { n = n * 10 + (c - L'0'); cifra = true; if (n > 100000) return {}; }
        else if (c == L'.' && cifra) { v.push_back(n); n = 0; cifra = false; }
        else return {};
    }
    if (!cifra) return {};
    v.push_back(n);
    return v.size() >= 2 && v.size() <= 4 ? v : std::vector<unsigned>{};
}
inline bool piuNuova(const std::wstring& a, const std::wstring& b) {   // a > b
    auto x = numeri(a), y = numeri(b);
    if (x.empty() || y.empty()) return false;
    x.resize(4, 0); y.resize(4, 0);
    return x > y;
}

// versione di un exe (VERSIONINFO): "1.2.2"
inline std::wstring versioneDi(const std::wstring& exe) {
    DWORD z = 0, n = GetFileVersionInfoSizeW(exe.c_str(), &z);
    if (!n) return L"";
    std::vector<BYTE> b(n);
    VS_FIXEDFILEINFO* f = nullptr; UINT lf = 0;
    if (!GetFileVersionInfoW(exe.c_str(), 0, n, b.data()) || !VerQueryValueW(b.data(), L"\\", (void**)&f, &lf) || !f) return L"";
    wchar_t s[32]; swprintf(s, 32, L"%u.%u.%u", HIWORD(f->dwFileVersionMS), LOWORD(f->dwFileVersionMS), HIWORD(f->dwFileVersionLS));
    return s;
}
inline std::wstring exeInUso() { wchar_t p[MAX_PATH * 2] = L""; GetModuleFileNameW(nullptr, p, MAX_PATH * 2); return p; }

// ---- JSON della release ----
// Lettura minima ma strutturale: si seguono oggetti, array e stringhe (con gli escape), e i campi si cercano solo al
// primo livello dell'oggetto, in qualsiasi ordine. Prima si cercava il testo "chiave" dopo il nome dell'allegato: con
// i campi in un altro ordine (JSON valido) misura e indirizzo restavano vuoti (R20261116-000127-S4)
constexpr size_t NIENTE = std::string::npos;
inline size_t spazi(const std::string& j, size_t p) { while (p < j.size() && (j[p] == ' ' || j[p] == '\n' || j[p] == '\r' || j[p] == '\t')) p++; return p; }
// p su '"': la posizione dopo la virgoletta che la chiude
inline size_t dopoStringa(const std::string& j, size_t p) {
    for (p++; p < j.size(); p++) { if (j[p] == '\\') p++; else if (j[p] == '"') return p + 1; }
    return NIENTE;
}
// p all'inizio di un valore: la posizione subito dopo il valore (NIENTE se il testo e' troncato o rotto)
inline size_t dopoValore(const std::string& j, size_t p) {
    p = spazi(j, p);
    if (p >= j.size()) return NIENTE;
    if (j[p] == '"') return dopoStringa(j, p);
    if (j[p] == '{' || j[p] == '[') {
        int liv = 0;
        for (; p < j.size(); p++) {
            const char c = j[p];
            if (c == '"') { p = dopoStringa(j, p); if (p == NIENTE) return NIENTE; p--; }
            else if (c == '{' || c == '[') { if (++liv > 64) return NIENTE; }
            else if (c == '}' || c == ']') { if (--liv == 0) return p + 1; }
        }
        return NIENTE;
    }
    while (p < j.size() && j[p] != ',' && j[p] != '}' && j[p] != ']') p++;   // numero, true, false, null
    return p < j.size() ? p : NIENTE;
}
// 'ogg' su '{': la posizione del valore della chiave k al primo livello (NIENTE se manca)
inline size_t campo(const std::string& j, size_t ogg, const char* k) {
    if (ogg >= j.size() || j[ogg] != '{') return NIENTE;
    size_t p = ogg + 1;
    for (int n = 0; n < 4096; n++) {
        p = spazi(j, p);
        if (p >= j.size() || j[p] != '"') return NIENTE;   // ('}': fine dell'oggetto)
        const size_t q = dopoStringa(j, p);
        if (q == NIENTE) return NIENTE;
        const bool questa = j.compare(p + 1, q - p - 2, k) == 0 && std::char_traits<char>::length(k) == q - p - 2;
        p = spazi(j, q);
        if (p >= j.size() || j[p] != ':') return NIENTE;
        p = spazi(j, p + 1);
        if (questa) return p;
        p = dopoValore(j, p);
        if (p == NIENTE) return NIENTE;
        p = spazi(j, p);
        if (p < j.size() && j[p] == ',') { p++; continue; }
        return NIENTE;
    }
    return NIENTE;
}
inline bool stringa(const std::string& j, size_t p, std::string& out) {
    out.clear();
    if (p == std::string::npos) return false;
    while (p < j.size() && (j[p] == ' ' || j[p] == '\n' || j[p] == '\r' || j[p] == '\t')) p++;
    if (p >= j.size() || j[p] != '"') return false;
    for (p++; p < j.size() && out.size() < 65536; p++) {
        const char c = j[p];
        if (c == '"') return true;
        if (c != '\\') { out += c; continue; }
        if (++p >= j.size()) return false;
        switch (j[p]) {
            case 'n': out += '\n'; break; case 'r': break; case 't': out += ' '; break;
            case 'u': {   // \uXXXX -> UTF-8 (basta il piano base)
                if (p + 4 >= j.size()) return false;
                const unsigned u = (unsigned)strtoul(j.substr(p + 1, 4).c_str(), nullptr, 16); p += 4;
                if (u < 0x80) out += (char)u;
                else if (u < 0x800) { out += (char)(0xC0 | (u >> 6)); out += (char)(0x80 | (u & 0x3F)); }
                else { out += (char)(0xE0 | (u >> 12)); out += (char)(0x80 | ((u >> 6) & 0x3F)); out += (char)(0x80 | (u & 0x3F)); }
                break;
            }
            default: out += j[p];
        }
    }
    return false;
}

// ---- fermo all'uscita (R20261116-000127-S2): FluidWin che si chiude chiama ferma(); il thread lo guarda fra un
// passo e l'altro (prima di inviare, a ogni pezzo letto, prima di estrarre e prima dello scambio) e smette. Le maniglie
// WinHTTP le chiude solo il thread che le usa: chiuderle dal thread principale durante una chiamata sincrona non e'
// permesso ("Concurrency in WinHTTP"; R20261116-000950-S1). Una chiamata gia' in attesa finisce col suo limite di
// tempo: FluidWin aspetta il thread 5 s, poi chiude il processo (Windows e' gia' rimesso a posto)
inline std::atomic<bool>& fermato() { static std::atomic<bool> f{false}; return f; }

// ---- rete (WinHTTP, solo HTTPS; i reindirizzamenti di GitHub verso i suoi server dei file li segue WinHTTP) ----
struct Http {
    HINTERNET s = nullptr, c = nullptr, r = nullptr;
    ~Http() { if (r) WinHttpCloseHandle(r); if (c) WinHttpCloseHandle(c); if (s) WinHttpCloseHandle(s); }
    // true con risposta 200
    bool apri(const std::wstring& host, const std::wstring& percorso, const wchar_t* accetta) {
        s = WinHttpOpen(L"FluidWin", WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, nullptr, nullptr, 0);
        if (!s) return false;
        WinHttpSetTimeouts(s, 8000, 8000, 15000, 30000);
        c = WinHttpConnect(s, host.c_str(), INTERNET_DEFAULT_HTTPS_PORT, 0);
        r = c ? WinHttpOpenRequest(c, L"GET", percorso.c_str(), nullptr, WINHTTP_NO_REFERER, WINHTTP_DEFAULT_ACCEPT_TYPES, WINHTTP_FLAG_SECURE) : nullptr;
        if (!r || fermato()) return false;
        std::wstring h = L"Accept: "; h += accetta;
        DWORD stato = 0, ls = sizeof(stato);
        return WinHttpSendRequest(r, h.c_str(), (DWORD)-1L, nullptr, 0, 0, 0) && WinHttpReceiveResponse(r, nullptr) &&
               WinHttpQueryHeaders(r, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_HEADER_NAME_BY_INDEX, &stato, &ls, WINHTTP_NO_HEADER_INDEX) &&
               stato == 200;
    }
    // legge tutto (al massimo 'tetto' byte); avanzamento(letti) a ogni pezzo; false se interrotto o oltre il tetto
    bool leggi(std::string& out, size_t tetto, const std::function<bool(size_t)>& avanzamento = nullptr) {
        DWORD n = 0;
        while (WinHttpQueryDataAvailable(r, &n) && n > 0) {
            if (fermato() || out.size() + n > tetto) return false;
            const size_t prima = out.size(); out.resize(prima + n);
            DWORD letti = 0;
            if (!WinHttpReadData(r, out.data() + prima, n, &letti) || !letti) { out.resize(prima); return false; }
            out.resize(prima + letti);
            if (avanzamento && !avanzamento(out.size())) return false;
        }
        return true;
    }
};

inline std::string sha256(const std::string& dati) {
    BCRYPT_ALG_HANDLE alg = nullptr; BCRYPT_HASH_HANDLE h = nullptr;
    unsigned char d[32]; std::string r;
    if (BCryptOpenAlgorithmProvider(&alg, BCRYPT_SHA256_ALGORITHM, nullptr, 0) == 0) {
        if (BCryptCreateHash(alg, &h, nullptr, 0, nullptr, 0, 0) == 0 &&
            BCryptHashData(h, (PUCHAR)dati.data(), (ULONG)dati.size(), 0) == 0 && BCryptFinishHash(h, d, 32, 0) == 0) {
            static const char* x = "0123456789abcdef";
            for (unsigned char c : d) { r += x[c >> 4]; r += x[c & 15]; }
        }
        if (h) BCryptDestroyHash(h);
        BCryptCloseAlgorithmProvider(alg, 0);
    }
    return r;
}

inline bool scriviFile(const std::wstring& p, const std::string& dati) {
    HANDLE f = CreateFileW(p.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (f == INVALID_HANDLE_VALUE) return false;
    DWORD n = 0; const bool ok = WriteFile(f, dati.data(), (DWORD)dati.size(), &n, nullptr) && n == dati.size() && FlushFileBuffers(f);
    CloseHandle(f);
    if (!ok) DeleteFileW(p.c_str());
    return ok;
}

}  // namespace dettagli

// Dalla risposta JSON di GitHub (la release): versione, pagina, zip, misura e impronta. false: non e' una release valida
inline bool leggiRelease(const std::string& j, Versione& v) {
    using namespace dettagli;
    std::string tag, pagina;
    const size_t rel = spazi(j, 0);
    if (!stringa(j, campo(j, rel, "tag_name"), tag) || numeri(daUtf8(tag)).empty()) { scrivi("aggiornamenti: risposta senza versione"); return false; }
    auto vero = [&](const char* k) { const size_t q = campo(j, rel, k); return q != NIENTE && j.compare(q, 4, "true") == 0; };
    if (vero("draft") || vero("prerelease")) return false;
    stringa(j, campo(j, rel, "html_url"), pagina);
    v.testo = daUtf8(tag); if (!v.testo.empty() && (v.testo[0] == L'v' || v.testo[0] == L'V')) v.testo.erase(0, 1);
    v.pagina = pagina.rfind("https://github.com/clooudfn/FluidWin/", 0) == 0 ? daUtf8(pagina) : L"https://github.com/clooudfn/FluidWin/releases/latest";
    // allegato: il primo .zip il cui nome comincia con FluidWin (FluidWin-1.2.3.zip), oggetto per oggetto dell'array
    size_t a = campo(j, rel, "assets");
    if (a != NIENTE && j[a] == '[') {
        for (a = spazi(j, a + 1); a < j.size() && j[a] == '{';) {
            std::string nome; stringa(j, campo(j, a, "name"), nome);
            std::string basso = nome; for (auto& c : basso) c = (char)tolower((unsigned char)c);
            if (basso.rfind("fluidwin", 0) == 0 && basso.size() > 4 && basso.compare(basso.size() - 4, 4, ".zip") == 0) {
                const size_t pm = campo(j, a, "size");
                if (pm != NIENTE && j[pm] >= '0' && j[pm] <= '9') v.misura = strtoull(j.c_str() + pm, nullptr, 10);
                std::string dig, url;
                stringa(j, campo(j, a, "digest"), dig);
                stringa(j, campo(j, a, "browser_download_url"), url);
                if (dig.rfind("sha256:", 0) == 0 && dig.size() == 71) { v.sha256 = dig.substr(7); for (auto& c : v.sha256) c = (char)tolower((unsigned char)c); }
                if (url.rfind(PREFISSO_ZIP, 0) == 0 && url.find("..") == std::string::npos) v.zip = url;
                break;
            }
            a = dopoValore(j, a);
            if (a == NIENTE) break;
            a = spazi(j, a);
            if (a < j.size() && j[a] == ',') a = spazi(j, a + 1); else break;
        }
    }
    scrivi("aggiornamenti: ultima pubblicata %ls (zip %s, %llu byte, impronta %s)", v.testo.c_str(), v.zip.empty() ? "assente" : "ok", v.misura, v.sha256.empty() ? "assente" : "ok");
    return true;
}

// Ultima versione pubblicata (chiamata da un thread: aspetta la rete). false: nessuna risposta valida
inline bool ultima(Versione& v) {
    using namespace dettagli;
    Http h;
    std::string j;
    if (!h.apri(L"api.github.com", REPO, L"application/vnd.github+json") || !h.leggi(j, 2u << 20)) { scrivi("aggiornamenti: GitHub non risponde"); return false; }
    return leggiRelease(j, v);
}

// Scarica, controlla e mette al posto dell'exe in uso. avanzamento(0..1). Chiamata da un thread. false: errore (in
// 'perche', da mostrare), l'exe in uso resta com'era. soloProva: tutto tranne lo scambio (prove con --versione-finta:
// l'exe in uso non si tocca)
inline bool installa(const Versione& v, std::wstring& perche, const std::function<void(double)>& avanzamento, bool soloProva = false) {
    using namespace dettagli;
    if (v.zip.empty() || v.sha256.size() != 64 || !v.misura || v.misura > (64ull << 20)) {
        perche = testi::t(L"Questa versione non ha un pacchetto verificabile", L"This version has no verifiable package"); return false;
    }
    // https://github.com/<percorso>
    const std::wstring url = daUtf8(v.zip), percorso = url.substr(std::wstring(L"https://github.com").size());
    Http h; std::string zip;
    if (!h.apri(L"github.com", percorso, L"application/octet-stream") ||
        !h.leggi(zip, (size_t)v.misura, [&](size_t n) { if (avanzamento) avanzamento((double)n / v.misura); return true; })) {
        perche = testi::t(L"Scaricamento non riuscito", L"Download failed"); scrivi("aggiornamenti: scaricamento non riuscito (%zu byte)", zip.size()); return false;
    }
    if (zip.size() != v.misura || sha256(zip) != v.sha256) {
        perche = testi::t(L"Il file scaricato non corrisponde a quello pubblicato", L"The downloaded file doesn't match the published one");
        scrivi("aggiornamenti: impronta o misura diverse (%zu byte), niente installazione", zip.size()); return false;
    }
    // lo zip in %LOCALAPPDATA%\FluidWin\Aggiornamento, l'exe nuovo accanto a quello in uso (stesso disco: si rinomina)
    wchar_t la[MAX_PATH] = L""; GetEnvironmentVariableW(L"LOCALAPPDATA", la, MAX_PATH);
    const std::wstring cartella = std::wstring(la) + L"\\FluidWin\\Aggiornamento";
    CreateDirectoryW((std::wstring(la) + L"\\FluidWin").c_str(), nullptr); CreateDirectoryW(cartella.c_str(), nullptr);
    const std::wstring fileZip = cartella + L"\\FluidWin.zip";
    if (!la[0] || !scriviFile(fileZip, zip)) { perche = testi::t(L"Impossibile salvare il file scaricato", L"Couldn't save the download"); return false; }
    if (fermato()) { DeleteFileW(fileZip.c_str()); return false; }
    // Lo zip sul disco riaperto senza permettere ad altri di scriverlo, riletto e confrontato con i byte verificati
    // (SHA-256), e tenuto aperto cosi' finche' tar lo estrae: prima tar riapriva un percorso fisso, e un altro
    // processo poteva cambiarlo fra la verifica e l'estrazione (R20261121-002419-S3)
    const HANDLE blocco = CreateFileW(fileZip.c_str(), GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
    bool uguale = false;
    if (blocco != INVALID_HANDLE_VALUE) {
        std::string letto(zip.size(), '\0'); DWORD n = 0; size_t fatto = 0;
        while (fatto < letto.size() && ReadFile(blocco, letto.data() + fatto, (DWORD)std::min<size_t>(letto.size() - fatto, 1u << 20), &n, nullptr) && n) fatto += n;
        LARGE_INTEGER dim{}; GetFileSizeEx(blocco, &dim);
        uguale = fatto == zip.size() && (ULONGLONG)dim.QuadPart == zip.size() && letto == zip;
    }
    if (!uguale) {
        if (blocco != INVALID_HANDLE_VALUE) CloseHandle(blocco);
        DeleteFileW(fileZip.c_str());
        perche = testi::t(L"Il file scaricato e' cambiato sul disco", L"The downloaded file changed on disk");
        scrivi("aggiornamenti: zip sul disco diverso da quello verificato, niente installazione"); return false;
    }
    std::string exe;
    const bool estratto = cursori::tar(L"-xOf \"" + fileZip + L"\" FluidWin.exe", exe, 30000, 64u << 20);
    CloseHandle(blocco);
    DeleteFileW(fileZip.c_str());
    if (!estratto || exe.size() < 4096 || exe[0] != 'M' || exe[1] != 'Z') { perche = testi::t(L"Nel pacchetto manca FluidWin.exe", L"FluidWin.exe is missing from the package"); return false; }
    const std::wstring inUso = exeInUso(), nuovo = inUso + L".nuovo", vecchio = inUso + L".vecchio";
    if (!scriviFile(nuovo, exe)) {
        perche = testi::t(L"Non posso scrivere nella cartella di FluidWin (serve l'amministratore?)", L"Can't write to FluidWin's folder (administrator needed?)");
        scrivi("aggiornamenti: cartella non scrivibile (%lu)", GetLastError()); return false;
    }
    if (versioneDi(nuovo) != v.testo) {
        DeleteFileW(nuovo.c_str());
        perche = testi::t(L"La versione nel pacchetto non e' quella annunciata", L"The package version isn't the announced one"); return false;
    }
    if (fermato()) { DeleteFileW(nuovo.c_str()); return false; }   // (FluidWin si chiude: niente scambio)
    if (soloProva) { DeleteFileW(nuovo.c_str()); scrivi("aggiornamenti: prova, la %ls e' pronta ma l'exe in uso non si cambia", v.testo.c_str()); return true; }
    // scambio: quello in uso diventa .vecchio (si puo' anche mentre gira), il nuovo prende il suo nome
    DeleteFileW(vecchio.c_str());
    if (!MoveFileExW(inUso.c_str(), vecchio.c_str(), MOVEFILE_REPLACE_EXISTING)) {
        DeleteFileW(nuovo.c_str());
        perche = testi::t(L"Non posso sostituire FluidWin.exe", L"Can't replace FluidWin.exe"); scrivi("aggiornamenti: rinomina dell'exe in uso non riuscita (%lu)", GetLastError()); return false;
    }
    if (!MoveFileExW(nuovo.c_str(), inUso.c_str(), MOVEFILE_REPLACE_EXISTING)) {
        const DWORD e = GetLastError();
        perche = testi::t(L"Non posso sostituire FluidWin.exe", L"Can't replace FluidWin.exe");
        if (MoveFileExW(vecchio.c_str(), inUso.c_str(), MOVEFILE_REPLACE_EXISTING)) {   // si rimette com'era
            DeleteFileW(nuovo.c_str());
            scrivi("aggiornamenti: exe nuovo non messo al posto (%lu), rimesso il vecchio", e);
        } else {
            // neanche il vecchio torna: si lasciano .vecchio e .nuovo, li rimette il guardiano all'uscita (recupera)
            scrivi("aggiornamenti: exe nuovo non messo al posto (%lu) e vecchio non rimesso (%lu): lo rimettera' il guardiano", e, GetLastError());
        }
        return false;
    }
    SetFileAttributesW(vecchio.c_str(), FILE_ATTRIBUTE_HIDDEN);
    scrivi("aggiornamenti: installata la %ls (%zu byte), al riavvio", v.testo.c_str(), exe.size());
    return true;
}

// Recupero di uno scambio interrotto (R20261116-000127-S1): lo fa il guardiano quando FluidWin finisce, perche' se
// manca FluidWin.exe nessuno lo puo' piu' avviare. Se manca: si rimette il .vecchio (quello che girava), altrimenti il
// .nuovo (gia' controllato prima dello scambio). p: il percorso di FluidWin.exe
inline void recupera(const std::wstring& p) {
    if (p.empty() || GetFileAttributesW(p.c_str()) != INVALID_FILE_ATTRIBUTES) return;
    for (const wchar_t* s : {L".vecchio", L".nuovo"}) {
        const std::wstring f = p + s;
        if (GetFileAttributesW(f.c_str()) == INVALID_FILE_ATTRIBUTES) continue;
        SetFileAttributesW(f.c_str(), FILE_ATTRIBUTE_NORMAL);
        if (MoveFileExW(f.c_str(), p.c_str(), 0)) { dettagli::scrivi("aggiornamenti: FluidWin.exe mancava, rimesso da %ls", s); return; }
    }
}

// all'avvio: via l'exe di prima di un aggiornamento (FluidWin.exe.vecchio) e un .nuovo rimasto da un tentativo interrotto
inline void pulisci() {
    const std::wstring p = dettagli::exeInUso();
    for (const wchar_t* s : {L".vecchio", L".nuovo"}) {
        const std::wstring f = p + s;
        if (GetFileAttributesW(f.c_str()) == INVALID_FILE_ATTRIBUTES) continue;
        SetFileAttributesW(f.c_str(), FILE_ATTRIBUTE_NORMAL);
        if (DeleteFileW(f.c_str())) dettagli::scrivi("aggiornamenti: tolto %ls", s);
    }
}

// ---- avviso di vetro in alto a destra ----
// Tutto animato (come il resto di FluidWin): entra scendendo dall'alto e comparendo, esce salendo e svanendo; i tasti
// cambiano colore con una dissolvenza e si stringono appena quando si premono; cambiando stato (scaricamento, errore,
// fatto) il contenuto nuovo compare con una dissolvenza e la barra si riempie inseguendo l'avanzamento vero.
// Un solo battito (vetro::battito) per tutto, fermo quando non c'e' niente in movimento
class Avviso {
public:
    std::function<void()> scarica;       // clic su "Scarica"
    std::function<void()> chiuso;        // "Piu' tardi" o "Chiudi"

    enum Stato { PRONTO, SCARICA, ERRORE, FATTO };

    void mostra(const Versione& nv, const std::wstring& attuale) {
        v = nv; ora = attuale; stato = PRONTO; avanz = barra = 0; perche.clear();
        sopra = premuto = -1; luce[0] = luce[1] = 0; uscendo = false;
        if (!fin.fin && !fin.crea(L"FluidWin.Aggiornamento", vetro::procedura<Avviso>, this, false)) return;
        fin.aggiornaTema();
        const RECT a = vetro::areaPrimaria();
        const HMONITOR mon = MonitorFromPoint({a.left, a.top}, MONITOR_DEFAULTTOPRIMARY);
        fin.dpi = vetro::dpiMonitor(mon);
        const float d = fin.dpi;
        RECT lav = a; MONITORINFO mi{}; mi.cbSize = sizeof(mi);
        if (GetMonitorInfoW(mon, &mi)) lav = mi.rcWork;
        w = (int)std::lround(380 * d); h = (int)std::lround(128 * d);
        x0 = lav.right - w - (int)std::lround(16 * d); y0 = lav.top + (int)std::lround(16 * d);
        if (!fTitolo) {
            fTitolo = fin.formato(14.5f * d, DWRITE_FONT_WEIGHT_SEMI_BOLD);
            fTesto = fin.formato(12 * d);
            fTasto = fin.formato(12.5f * d, DWRITE_FONT_WEIGHT_MEDIUM, DWRITE_TEXT_ALIGNMENT_CENTER);
        }
        tIn = tStato = tPrec = adesso(); kIn = 0; kStato = 1;
        posa();
        ShowWindow(fin.fin, SW_SHOWNOACTIVATE);
        vetro::battito(fin.fin, 1);
        disegna();
    }
    void avanzamento(double x) { avanz = std::clamp(x, 0.0, 1.0); anima(); }
    void errore(const std::wstring& p) { cambia(ERRORE); perche = p; }
    void fatto() { avanz = 1; cambia(FATTO); }
    // esce con l'animazione (sale e svanisce), poi si nasconde
    void nascondi() {
        if (!fin.fin || !IsWindowVisible(fin.fin) || uscendo) return;
        uscendo = true; tOut = adesso(); kOutDa = kIn; anima();
    }
    bool visibile() const { return fin.fin && IsWindowVisible(fin.fin); }

    bool messaggio(HWND, UINT m, WPARAM wp, LPARAM l, LRESULT& r) {
        r = 0;
        switch (m) {
            case WM_TIMER: if (wp == 1) passo(); return true;
            case WM_MOUSEMOVE: {
                TRACKMOUSEEVENT te{sizeof(te), TME_LEAVE, fin.fin, 0}; TrackMouseEvent(&te);
                const int s = zona((float)GET_X_LPARAM(l), (float)GET_Y_LPARAM(l));
                if (s != sopra) { sopra = s; anima(); }
                return true;
            }
            case WM_MOUSELEAVE: if (sopra != -1 || premuto != -1) { sopra = premuto = -1; anima(); } return true;
            case WM_LBUTTONDOWN: premuto = zona((float)GET_X_LPARAM(l), (float)GET_Y_LPARAM(l)); SetCapture(fin.fin); anima(); return true;
            case WM_LBUTTONUP: {
                const int s = zona((float)GET_X_LPARAM(l), (float)GET_Y_LPARAM(l)), p = premuto;
                premuto = -1; ReleaseCapture(); anima();
                if (s < 0 || s != p || uscendo) return true;   // (premuto su un tasto e lasciato fuori: niente)
                if (s == 0) { nascondi(); if (chiuso) chiuso(); }                         // piu' tardi / chiudi
                else if (stato == PRONTO) { avanz = barra = 0; cambia(SCARICA); if (scarica) scarica(); }
                else if (stato == ERRORE) { ShellExecuteW(nullptr, L"open", v.pagina.c_str(), nullptr, nullptr, SW_SHOWNORMAL); nascondi(); if (chiuso) chiuso(); }
                return true;
            }
            case WM_PAINT: disegna(); return true;
        }
        return false;
    }

private:
    vetro::Finestra fin;
    IDWriteTextFormat *fTitolo = nullptr, *fTesto = nullptr, *fTasto = nullptr;
    Versione v; std::wstring ora, perche;
    Stato stato = PRONTO;
    int w = 0, h = 0, x0 = 0, y0 = 0;
    double avanz = 0, barra = 0;           // avanzamento vero e quello disegnato (lo insegue)
    double tIn = 0, tOut = 0, tStato = 0, tPrec = 0, kIn = 0, kOutDa = 1, kStato = 1;
    bool uscendo = false;
    int sopra = -1, premuto = -1;
    double luce[2] = {0, 0};               // quanto e' illuminato ogni tasto (mouse sopra), 0..1

    static constexpr double ENTRATA = 0.32, USCITA = 0.20, CAMBIO = 0.22;   // s
    static double adesso() { return vetro::secondi(); }
    static double fuori(double k) { k = std::clamp(k, 0.0, 1.0); return 1 - std::pow(1 - k, 3); }   // arriva morbida
    static double dentro(double k) { k = std::clamp(k, 0.0, 1.0); return k * k * k; }               // parte morbida

    void anima() { if (fin.fin) vetro::battito(fin.fin, 1); }
    void cambia(Stato s) { stato = s; tStato = adesso(); kStato = 0; sopra = premuto = -1; anima(); }

    // posizione e trasparenza: entrando scende di 14 punti dall'alto, uscendo sale di 10 e svanisce
    void posa() {
        const float d = fin.dpi;
        double y, a;
        if (uscendo) { const double e = dentro((adesso() - tOut) / USCITA); y = y0 - 10 * d * e; a = kOutDa * (1 - e); }
        else { const double e = fuori(kIn); y = y0 - 14 * d * (1 - e); a = std::min(1.0, kIn * 1.6); }
        fin.posiziona(x0, (int)std::lround(y), w, h, HWND_TOPMOST);
        fin.alfa(a);
    }

    void passo() {
        if (!IsWindowVisible(fin.fin)) { vetro::fermaBattito(fin.fin, 1); return; }   // (battito arrivato dopo la chiusura)
        const double t = adesso(), dt = std::clamp(t - tPrec, 0.001, 0.05); tPrec = t;
        bool ancora = false;
        if (uscendo) {
            if (t - tOut >= USCITA) { uscendo = false; vetro::fermaBattito(fin.fin, 1); ShowWindow(fin.fin, SW_HIDE); return; }
            ancora = true;
        } else if (kIn < 1) { kIn = std::min(1.0, (t - tIn) / ENTRATA); ancora = true; }
        if (kStato < 1) { kStato = std::min(1.0, (t - tStato) / CAMBIO); ancora = true; }
        for (int i = 0; i < 2; i++) {   // luce dei tasti: insegue il mouse in ~0,1 s
            const double obj = sopra == i ? 1.0 : 0.0;
            if (std::fabs(luce[i] - obj) > 0.01) { luce[i] += (obj - luce[i]) * std::min(1.0, dt * 16); ancora = true; } else luce[i] = obj;
        }
        if (std::fabs(barra - avanz) > 0.002) { barra += (avanz - barra) * std::min(1.0, dt * 9); ancora = true; } else barra = avanz;
        posa();
        disegna();
        if (!ancora) vetro::fermaBattito(fin.fin, 1);
    }

    // tasti: 0 a sinistra (Piu' tardi / Chiudi), 1 a destra (Scarica / Apri la pagina)
    D2D1_RECT_F tasto(int i) const {
        const float d = fin.dpi, W = (float)fin.W, H = (float)fin.H, l = 116 * d, a = 30 * d, m = 14 * d;
        const float x1 = i == 1 ? W - m : W - m - l - 8 * d;
        return {x1 - l, H - m - a, x1, H - m};
    }
    int zona(float x, float y) const {
        if (stato == SCARICA || stato == FATTO || kStato < 1) return -1;
        for (int i = 0; i < 2; i++) { const D2D1_RECT_F t = tasto(i); if (x >= t.left && x < t.right && y >= t.top && y < t.bottom) return i; }
        return -1;
    }

    void disegna() {
        if (!fin.fin || !fin.inizia()) return;
        const float d = fin.dpi, W = (float)fin.W, m = 16 * d;
        const float ks = (float)fuori(kStato);   // contenuto dello stato nuovo: compare (e sale di 4 punti)
        const float sale = (1 - ks) * 4 * d;
        D2D1_COLOR_F ink = fin.inchiostro(), tenue = fin.inchiostro(0.62f);
        const D2D1_COLOR_F blu = fin.scuro ? D2D1::ColorF(0.40f, 0.68f, 1.0f, 1) : D2D1::ColorF(0.0f, 0.42f, 0.95f, 1);
        wchar_t t[160];
        swprintf(t, 160, testi::t(L"È disponibile FluidWin %ls", L"FluidWin %ls is available"), v.testo.c_str());
        fin.testo(t, fTitolo, {m, 12 * d, W - m, 36 * d}, ink);
        std::wstring sotto;
        if (stato == PRONTO) { swprintf(t, 160, testi::t(L"Hai la %ls. Si scarica da GitHub e FluidWin si riavvia.", L"You have %ls. It downloads from GitHub and FluidWin restarts."), ora.c_str()); sotto = t; }
        else if (stato == SCARICA) { swprintf(t, 160, testi::t(L"Scaricamento… %d%%", L"Downloading… %d%%"), (int)std::lround(barra * 100)); sotto = t; }
        else if (stato == FATTO) sotto = testi::t(L"Installata. Riavvio…", L"Installed. Restarting…");
        else sotto = perche;
        D2D1_COLOR_F cs = stato == ERRORE ? D2D1::ColorF(0.95f, 0.35f, 0.3f, 1) : tenue; cs.a *= ks;
        fin.testo(sotto, fTesto, {m, 38 * d + sale, W - m, 58 * d + sale}, cs);
        if (stato == SCARICA || stato == FATTO) {   // barra: compare allargandosi dal centro, poi si riempie
            const float cx = W / 2, mezza = (W / 2 - m) * (0.6f + 0.4f * ks);
            const D2D1_RECT_F b = {cx - mezza, 76 * d, cx + mezza, 82 * d};
            fin.pen->SetColor(fin.scuro ? D2D1::ColorF(1, 1, 1, 0.15f * ks) : D2D1::ColorF(0, 0, 0, 0.1f * ks));
            fin.rt->FillRoundedRectangle({b, 3 * d, 3 * d}, fin.pen);
            const float pieno = (float)(b.right - b.left) * (float)barra;
            if (pieno > 1) {
                fin.pen->SetColor(D2D1::ColorF(blu.r, blu.g, blu.b, ks));
                fin.rt->FillRoundedRectangle({{b.left, b.top, b.left + std::max(6 * d, pieno), b.bottom}, 3 * d, 3 * d}, fin.pen);
            }
        } else {
            const wchar_t* nomi[2] = {stato == ERRORE ? testi::t(L"Chiudi", L"Close") : testi::t(L"Più tardi", L"Later"),
                                      stato == ERRORE ? testi::t(L"Apri la pagina", L"Open the page") : testi::t(L"Scarica", L"Download")};
            for (int i = 0; i < 2; i++) {
                D2D1_RECT_F r = tasto(i);
                r.top += sale; r.bottom += sale;
                const float c = (float)luce[i];
                // premuto: si stringe appena dal centro (3%)
                if (premuto == i && sopra == i) { const float dx = (r.right - r.left) * 0.015f, dy = (r.bottom - r.top) * 0.03f; r = {r.left + dx, r.top + dy, r.right - dx, r.bottom - dy}; }
                if (i == 1) fin.pen->SetColor(D2D1::ColorF(blu.r, blu.g, blu.b, (0.86f + 0.14f * c) * ks));
                else fin.pen->SetColor(fin.scuro ? D2D1::ColorF(1, 1, 1, (0.12f + 0.08f * c) * ks) : D2D1::ColorF(0, 0, 0, (0.07f + 0.05f * c) * ks));
                const float rr = (r.bottom - r.top) / 2;
                fin.rt->FillRoundedRectangle({r, rr, rr}, fin.pen);
                D2D1_COLOR_F ct = i == 1 ? D2D1::ColorF(1, 1, 1, 1) : ink; ct.a *= ks;
                fin.testo(nomi[i], fTasto, r, ct);
            }
        }
        fin.finisci();
    }
};

}  // namespace aggiorna
