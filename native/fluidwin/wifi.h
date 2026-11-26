// Reti Wi-Fi per il centro di controllo (issue #24: "non si puo' cambiare rete Wi-Fi dalle impostazioni rapide").
// API Native Wifi di Windows (wlanapi.dll, la stessa delle impostazioni di Windows): elenco delle reti visibili,
// collegamento a una rete gia' usata (profilo salvato da Windows) o aperta, scollegamento. Niente password: per una
// rete nuova protetta il pannello apre le impostazioni Wi-Fi di Windows (scelta dell'utente, 19/11/2026).
// wlanapi.dll si carica quando serve (LoadLibrary): su un'edizione di Windows senza il servizio WLAN la DLL puo'
// mancare, e un collegamento fisso impedirebbe a FluidWin di partire. Tutte le chiamate stanno in un thread a parte
// (scansione e collegamento possono metterci secondi); il pannello legge solo l'ultima copia pubblicata.
// Da Windows 11 24H2 i programmi desktop ricevono l'elenco solo con l'accesso alla posizione consentito: senza,
// WlanGetAvailableNetworkList risponde ERROR_ACCESS_DENIED e il pannello lo dice (stato S_PERMESSO).
// NON PROVATO su un PC con il Wi-Fi: quello di sviluppo non ha la scheda (servizio WLAN non attivo, errore 1062).
#pragma once
#include "tlauncher_identity.h"
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <wlanapi.h>
#include <string>
#include <vector>
#include <atomic>
#include <algorithm>
#include <cstdio>
#include <cstring>
#include <type_traits>

namespace wifi {

struct Rete {
    std::wstring nome;          // SSID da mostrare (UTF-8, o la pagina di codice del sistema se non e' UTF-8 valido)
    std::string ssid;           // SSID com'e' (byte): per collegarsi
    int segnale = 0;            // 0..100
    bool protetta = false;      // serve una chiave
    bool aperta = false;        // senza protezione (ci si collega senza chiedere niente)
    bool profilo = false;       // Windows la conosce gia' (password salvata): ci si collega con un clic
    bool connessa = false;
    std::wstring nomeProfilo;
};

enum Stato { S_AVVIO, S_NESSUNA, S_SPENTO, S_PERMESSO, S_PRONTA, S_ERRORE };
enum Esito { E_NIENTE, E_IN_CORSO, E_FATTO, E_FALLITO };

// SSID (fino a 32 byte) -> testo: UTF-8 se valido, altrimenti la pagina di codice del sistema
inline std::wstring nomeSsid(const std::string& s) {
    if (s.empty()) return L"";
    int n = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, s.data(), (int)s.size(), nullptr, 0);
    UINT cp = CP_UTF8;
    if (n <= 0) { cp = CP_ACP; n = MultiByteToWideChar(CP_ACP, 0, s.data(), (int)s.size(), nullptr, 0); }
    if (n <= 0) return L"";
    std::wstring w(n, L'\0');
    MultiByteToWideChar(cp, cp == CP_UTF8 ? MB_ERR_INVALID_CHARS : 0, s.data(), (int)s.size(), w.data(), n);
    for (auto& c : w) if (c < 32) c = L' ';   // (nomi con caratteri di controllo: si vedono come spazi)
    return w;
}

// testo per l'XML (nome del profilo temporaneo)
inline std::wstring xml(const std::wstring& s) {
    std::wstring o;
    for (wchar_t c : s) {
        if (c == L'&') o += L"&amp;"; else if (c == L'<') o += L"&lt;"; else if (c == L'>') o += L"&gt;";
        else if (c == L'"') o += L"&quot;"; else if (c == L'\'') o += L"&apos;"; else o += c;
    }
    return o;
}

// Profilo temporaneo (non salvato da Windows) per una rete aperta: SSID in esadecimale (qualsiasi byte), nessuna
// protezione, collegamento manuale
inline std::wstring profiloAperto(const std::string& ssid) {
    std::wstring hex;
    for (unsigned char c : ssid) { wchar_t b[4]; swprintf(b, 4, L"%02X", c); hex += b; }
    return L"<?xml version=\"1.0\"?>\r\n"
           L"<WLANProfile xmlns=\"http://www.microsoft.com/networking/WLAN/profile/v1\">"
           // (nome ASCII ricavato dall'esadecimale, non dall'SSID: un SSID con caratteri vietati in XML 1.0, come
           // U+FFFE, rendeva il profilo non valido. R20261119-145819-S5. Il profilo e' temporaneo: il nome non si vede)
           L"<name>TLauncher-" + hex + L"</name>"
           L"<SSIDConfig><SSID><hex>" + hex + L"</hex></SSID></SSIDConfig>"
           L"<connectionType>ESS</connectionType><connectionMode>manual</connectionMode>"
           L"<MSM><security><authEncryption><authentication>open</authentication><encryption>none</encryption>"
           L"<useOneX>false</useOneX></authEncryption></security></MSM></WLANProfile>";
}

// Unisce le voci di WlanGetAvailableNetworkList (una per profilo, piu' una senza profilo per la stessa rete) in una
// riga per nome: collegata o con profilo prima, segnale migliore. Le reti nascoste (senza nome) non si mostrano.
// Ordine: collegata, poi con profilo, poi per segnale
inline std::vector<Rete> unisci(const std::vector<Rete>& voci) {
    std::vector<Rete> r;
    for (const auto& v : voci) {
        if (v.ssid.empty()) continue;
        auto it = std::find_if(r.begin(), r.end(), [&](const Rete& x) { return x.ssid == v.ssid; });
        if (it == r.end()) { r.push_back(v); continue; }
        it->segnale = std::max(it->segnale, v.segnale);
        it->connessa = it->connessa || v.connessa;
        if (v.profilo && !it->profilo) { it->profilo = true; it->nomeProfilo = v.nomeProfilo; }
        if (v.connessa && v.profilo) it->nomeProfilo = v.nomeProfilo;   // (il profilo in uso)
    }
    std::stable_sort(r.begin(), r.end(), [](const Rete& a, const Rete& b) {
        if (a.connessa != b.connessa) return a.connessa;
        if (a.profilo != b.profilo) return a.profilo;
        return a.segnale > b.segnale;
    });
    if (r.size() > 30) r.resize(30);
    return r;
}

class Wifi {
public:
    std::atomic<bool> pronto{false};   // copia nuova pubblicata (il pannello si ridisegna)

    Wifi() { InitializeCriticalSection(&cs); }
    Wifi(const Wifi&) = delete;
    Wifi& operator=(const Wifi&) = delete;
    ~Wifi() {   // (thread gia' fermato da ferma, oppure processo chiuso prima dei distruttori statici)
        if (fermo) CloseHandle(fermo);
        if (comando) CloseHandle(comando);
        if (thread) CloseHandle(thread);
    }

    // Parte alla prima apertura della vista Wi-Fi (non all'avvio: chi usa il cavo non paga niente)
    bool avvia() {
        if (thread) return true;
        fermo = CreateEventW(nullptr, TRUE, FALSE, nullptr);
        comando = CreateEventW(nullptr, FALSE, FALSE, nullptr);
        if (!fermo || !comando) return false;
        thread = CreateThread(nullptr, 0, lavoro, this, 0, nullptr);
        return thread != nullptr;
    }
    bool ferma(DWORD ms) {
        if (!thread) return true;
        SetEvent(fermo);
        return WaitForSingleObject(thread, ms) == WAIT_OBJECT_0;
    }

    void cerca() { chiedi(C_CERCA, ""); }
    // salvata: come la mostrava il pannello al clic (rete con profilo); il lavoro collega solo se e' ancora cosi'
    void collega(const std::string& ssid, bool salvata) { chiedi(salvata ? C_COLLEGA : C_COLLEGA_APERTA, ssid); }
    void scollega() { chiedi(C_SCOLLEGA, ""); }

    // ultima copia: reti, stato, esito dell'ultimo collegamento e la sua rete, ricerca in corso
    std::vector<Rete> reti(int& stato, int& esito, std::string& esitoDi, bool& cercando) {
        EnterCriticalSection(&cs);
        std::vector<Rete> r = elenco; stato = statoP; esito = esitoP; esitoDi = esitoSsid; cercando = cercandoP;
        LeaveCriticalSection(&cs);
        return r;
    }

private:
    enum Comando { C_NIENTE, C_CERCA, C_COLLEGA, C_SCOLLEGA, C_COLLEGA_APERTA };
    HANDLE thread = nullptr, fermo = nullptr, comando = nullptr;
    CRITICAL_SECTION cs{};
    // protetti da cs
    std::vector<Rete> elenco;
    int statoP = S_AVVIO, esitoP = E_NIENTE;
    std::string esitoSsid;
    GUID esitoScheda{}; bool esitoSchedaOk = false;   // scheda del collegamento chiesto (R20261121-001738-S8)
    bool cercandoP = false;
    int cmd = C_NIENTE;
    std::string cmdSsid;
    // notifiche di Windows (dal suo thread): si sveglia il thread di lavoro
    HANDLE notifica = nullptr;
    std::atomic<int> ultimaNotifica{0};

    void chiedi(int c, const std::string& s) {
        EnterCriticalSection(&cs);
        // (un comando in attesa si sostituisce: conta l'ultimo clic; una scansione non cancella un collegamento o uno
        // scollegamento in attesa: prima "Scollega" si perdeva se scadeva la scansione automatica, R20261121-001738-S9)
        if (!(c == C_CERCA && cmd != C_NIENTE && cmd != C_CERCA)) { cmd = c; cmdSsid = s; }
        if (c == C_COLLEGA || c == C_COLLEGA_APERTA) { esitoP = E_IN_CORSO; esitoSsid = s; }
        LeaveCriticalSection(&cs);
        if (comando) SetEvent(comando);
        pronto = true;
    }
    void pubblica(int stato, const std::vector<Rete>* r, int esito = -1, bool cercando = false, bool tocca = true) {
        EnterCriticalSection(&cs);
        statoP = stato;
        if (r) elenco = *r;
        if (esito >= 0) esitoP = esito;
        if (tocca) cercandoP = cercando;
        LeaveCriticalSection(&cs);
        pronto = true;
    }

    // funzioni di wlanapi.dll, caricata nel thread
    struct Api {
        HMODULE dll = nullptr;
        decltype(&WlanOpenHandle) apri = nullptr;
        decltype(&WlanCloseHandle) chiudi = nullptr;
        decltype(&WlanEnumInterfaces) interfacce = nullptr;
        decltype(&WlanScan) scansione = nullptr;
        decltype(&WlanGetAvailableNetworkList) elenco = nullptr;
        decltype(&WlanConnect) collega = nullptr;
        decltype(&WlanDisconnect) scollega = nullptr;
        decltype(&WlanRegisterNotification) notifiche = nullptr;
        decltype(&WlanFreeMemory) libera = nullptr;
        bool carica() {
            dll = LoadLibraryExW(L"wlanapi.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32);
            if (!dll) return false;
            auto f = [&](auto& p, const char* n) { p = reinterpret_cast<std::remove_reference_t<decltype(p)>>(GetProcAddress(dll, n)); return p != nullptr; };
            const bool ok = f(apri, "WlanOpenHandle") && f(chiudi, "WlanCloseHandle") && f(interfacce, "WlanEnumInterfaces") &&
                   f(scansione, "WlanScan") && f(elenco, "WlanGetAvailableNetworkList") && f(collega, "WlanConnect") &&
                   f(scollega, "WlanDisconnect") && f(notifiche, "WlanRegisterNotification") && f(libera, "WlanFreeMemory");
            // tabella incompleta: via tutto, cosi' il giro dopo non la scambia per caricata (dll valorizzata) e non
            // chiama un puntatore nullo (R20261121-001738-S10)
            if (!ok) { FreeLibrary(dll); *this = Api{}; }
            return ok;
        }
    };

    static VOID WINAPI suNotifica(PWLAN_NOTIFICATION_DATA d, PVOID ctx) {
        Wifi* me = (Wifi*)ctx;
        if (!d || !me || d->NotificationSource != WLAN_NOTIFICATION_SOURCE_ACM) return;
        me->ultimaNotifica = (int)d->NotificationCode;
        // Esito del collegamento chiesto dal pannello: solo se l'avviso parla della SUA rete. Windows avvisa anche dei
        // collegamenti che fa da se' (ritorno automatico a un'altra rete): uno di quelli che fallisce non deve scrivere
        // "Collegamento non riuscito" sulla rete scelta. Deciso qui (thread delle notifiche, sotto cs): nel thread di
        // lavoro piu' avvisi di fila si riducono all'ultimo codice
        if ((d->NotificationCode == wlan_notification_acm_connection_complete || d->NotificationCode == wlan_notification_acm_connection_attempt_fail) &&
            d->pData && d->dwDataSize >= sizeof(WLAN_CONNECTION_NOTIFICATION_DATA)) {
            const auto* c = (const WLAN_CONNECTION_NOTIFICATION_DATA*)d->pData;
            const std::string s((const char*)c->dot11Ssid.ucSSID, std::min<ULONG>(c->dot11Ssid.uSSIDLength, DOT11_SSID_MAX_LENGTH));
            EnterCriticalSection(&me->cs);
            // (e della SUA scheda: con due schede Wi-Fi sulla stessa rete l'altra poteva chiudere l'attesa, S8)
            if (me->esitoP == E_IN_CORSO && s == me->esitoSsid && (!me->esitoSchedaOk || IsEqualGUID(d->InterfaceGuid, me->esitoScheda)))
                me->esitoP = d->NotificationCode == wlan_notification_acm_connection_complete && c->wlanReasonCode == WLAN_REASON_CODE_SUCCESS ? E_FATTO : E_FALLITO;
            LeaveCriticalSection(&me->cs);
        }
        SetEvent(me->notifica);
    }

    // Errori che vogliono dire "sessione col servizio WLAN non piu' valida" (servizio fermato o riavviato): la maniglia
    // va chiusa e riaperta. R20261119-145819-S2: prima si ricercava solo l'interfaccia con la maniglia vecchia, per sempre
    static bool sessioneRotta(DWORD e) {
        return e == ERROR_INVALID_HANDLE || e == ERROR_SERVICE_NOT_ACTIVE || e == RPC_S_SERVER_UNAVAILABLE ||
               e == RPC_S_CALL_FAILED || e == RPC_S_CALL_FAILED_DNE || e == RPC_S_UNKNOWN_IF;
    }

    // l'elenco delle reti dell'interfaccia (o lo stato che lo impedisce); errore: il codice di Windows
    static int leggi(const Api& a, HANDLE h, const GUID& g, std::vector<Rete>& out, DWORD& errore) {
        PWLAN_AVAILABLE_NETWORK_LIST nl = nullptr;
        const DWORD e = a.elenco(h, &g, 0, nullptr, &nl);
        errore = e;
        if (e == ERROR_ACCESS_DENIED) return S_PERMESSO;
        if (e != ERROR_SUCCESS || !nl) return S_ERRORE;
        std::vector<Rete> v;
        for (DWORD i = 0; i < nl->dwNumberOfItems && i < 512; i++) {
            const auto& n = nl->Network[i];
            if (n.dot11BssType != dot11_BSS_type_infrastructure) continue;
            Rete r;
            r.ssid.assign((const char*)n.dot11Ssid.ucSSID, std::min<ULONG>(n.dot11Ssid.uSSIDLength, DOT11_SSID_MAX_LENGTH));
            r.nome = nomeSsid(r.ssid);
            r.segnale = (int)std::min<ULONG>(n.wlanSignalQuality, 100);
            r.protetta = n.bSecurityEnabled != FALSE;
            r.aperta = !r.protetta && n.dot11DefaultAuthAlgorithm == DOT11_AUTH_ALGO_80211_OPEN && n.dot11DefaultCipherAlgorithm == DOT11_CIPHER_ALGO_NONE;
            r.profilo = (n.dwFlags & WLAN_AVAILABLE_NETWORK_HAS_PROFILE) != 0 && n.strProfileName[0];
            if (r.profilo) r.nomeProfilo = n.strProfileName;
            r.connessa = (n.dwFlags & WLAN_AVAILABLE_NETWORK_CONNECTED) != 0;
            v.push_back(std::move(r));
        }
        a.libera(nl);
        out = unisci(v);
        return S_PRONTA;
    }

    static DWORD WINAPI lavoro(LPVOID p) {
        Wifi* me = (Wifi*)p;
        Api a;
        HANDLE h = nullptr;
        GUID g{};
        bool interfaccia = false, registrata = false;
        me->notifica = CreateEventW(nullptr, FALSE, FALSE, nullptr);
        ULONGLONG scansioneT = 0;   // scansione chiesta (0: nessuna in corso)
        ULONGLONG collegaT = 0;     // collegamento chiesto a Windows (0: nessuno in attesa del suo esito)
        std::vector<Rete> ultime;
        // sessione non piu' valida: avvisi tolti, maniglia chiusa; si riapre al giro dopo (con l'attesa di 15 s)
        auto chiudiSessione = [&]() {
            if (!h) return;
            if (registrata) a.notifiche(h, WLAN_NOTIFICATION_SOURCE_NONE, TRUE, nullptr, nullptr, nullptr, nullptr);
            a.chiudi(h, nullptr);
            h = nullptr; interfaccia = false; registrata = false; scansioneT = 0;
            // con la maniglia se ne vanno anche gli avvisi in arrivo: un collegamento in corso non avra' mai l'esito
            // (R20261119-150434-S1: restava "Collegamento…" per sempre)
            EnterCriticalSection(&me->cs);
            if (me->esitoP == E_IN_CORSO) me->esitoP = E_FALLITO;
            LeaveCriticalSection(&me->cs);
            collegaT = 0;
            me->pubblica(S_NESSUNA, nullptr);
        };
        for (;;) {
            // apertura (anche piu' tardi: il servizio WLAN puo' partire dopo, una scheda USB arrivare)
            if (!h) {
                DWORD ver = 0;
                if ((a.dll || a.carica()) && a.apri(2, nullptr, &ver, &h) == ERROR_SUCCESS && h) {
                    registrata = a.notifiche(h, WLAN_NOTIFICATION_SOURCE_ACM, TRUE, suNotifica, me, nullptr, nullptr) == ERROR_SUCCESS;
                } else { h = nullptr; me->pubblica(S_NESSUNA, nullptr); }
            }
            if (h && !interfaccia) {
                PWLAN_INTERFACE_INFO_LIST il = nullptr;
                const DWORD ei = a.interfacce(h, nullptr, &il);
                if (ei == ERROR_SUCCESS && il && il->dwNumberOfItems > 0) {
                    // la prima collegata, altrimenti la prima
                    DWORD k = 0;
                    for (DWORD i = 0; i < il->dwNumberOfItems; i++) if (il->InterfaceInfo[i].isState == wlan_interface_state_connected) { k = i; break; }
                    g = il->InterfaceInfo[k].InterfaceGuid; interfaccia = true;
                } else me->pubblica(S_NESSUNA, nullptr);
                if (il) a.libera(il);
                if (sessioneRotta(ei)) chiudiSessione();
            }
            // comando
            EnterCriticalSection(&me->cs);
            const int c = me->cmd; const std::string s = me->cmdSsid; me->cmd = C_NIENTE;
            LeaveCriticalSection(&me->cs);
            if (h && interfaccia) {
                if (c == C_CERCA) {
                    const DWORD es = a.scansione(h, &g, nullptr, nullptr, nullptr);
                    if (es == ERROR_SUCCESS) { scansioneT = GetTickCount64(); me->pubblica(S_PRONTA, nullptr, -1, true); }
                    else if (sessioneRotta(es)) chiudiSessione();
                }
                else if (c == C_SCOLLEGA) { if (sessioneRotta(a.scollega(h, &g, nullptr))) chiudiSessione(); }
                else if (c == C_COLLEGA || c == C_COLLEGA_APERTA) {
                    // la rete com'era nel pannello al clic: salvata col suo profilo, o aperta. Se intanto e' cambiata
                    // (la scansione vede ormai solo una rete aperta con lo stesso nome) non si collega: prima una riga
                    // "Salvata" poteva collegarsi a una rete aperta omonima (R20261121-001738-S1)
                    const bool salvata = c == C_COLLEGA;
                    auto it = std::find_if(ultime.begin(), ultime.end(), [&](const Rete& r) { return r.ssid == s && (salvata ? r.profilo : (r.aperta && !r.profilo)); });
                    DWORD e = ERROR_NOT_FOUND;
                    if (it != ultime.end()) {
                        WLAN_CONNECTION_PARAMETERS cp{};
                        DOT11_SSID ss{}; ss.uSSIDLength = (ULONG)std::min<size_t>(s.size(), DOT11_SSID_MAX_LENGTH); memcpy(ss.ucSSID, s.data(), ss.uSSIDLength);
                        const std::wstring xmlProfilo = it->profilo ? L"" : profiloAperto(s);
                        cp.wlanConnectionMode = it->profilo ? wlan_connection_mode_profile : wlan_connection_mode_temporary_profile;
                        cp.strProfile = it->profilo ? it->nomeProfilo.c_str() : xmlProfilo.c_str();
                        cp.pDot11Ssid = &ss;
                        cp.dot11BssType = dot11_BSS_type_infrastructure;
                        EnterCriticalSection(&me->cs); me->esitoScheda = g; me->esitoSchedaOk = true; LeaveCriticalSection(&me->cs);
                        e = a.collega(h, &g, &cp, nullptr);
                    }
                    // (l'esito vero arriva con la notifica; un errore subito chiude qui)
                    if (e != ERROR_SUCCESS) { me->pubblica(S_PRONTA, nullptr, E_FALLITO, false, false); collegaT = 0; }
                    else collegaT = GetTickCount64();
                    if (sessioneRotta(e)) chiudiSessione();
                }
            } else if (c == C_COLLEGA || c == C_COLLEGA_APERTA) me->pubblica(S_NESSUNA, nullptr, E_FALLITO);
            // elenco: subito dopo un comando, alla fine di una scansione (notifica o 4 s), a ogni notifica
            if (h && interfaccia) {
                const int n = me->ultimaNotifica.exchange(0);
                const bool scansioneFinita = scansioneT && (n == wlan_notification_acm_scan_complete || n == wlan_notification_acm_scan_fail || GetTickCount64() - scansioneT > 4000);
                if (c != C_NIENTE || n || scansioneFinita) {
                    std::vector<Rete> r;
                    DWORD errore = 0;
                    const int st = leggi(a, h, g, r, errore);
                    if (st == S_PRONTA) ultime = r;
                    // (l'esito del collegamento lo scrive suNotifica; radio spenta: lettura vuota, lo stato "spento" lo
                    // decide il pannello con sis.wifi)
                    me->pubblica(st, st == S_PRONTA ? &r : nullptr, -1, scansioneT && !scansioneFinita, true);
                    if (scansioneFinita) scansioneT = 0;
                    if (sessioneRotta(errore)) chiudiSessione();          // servizio fermato o riavviato
                    else if (st == S_ERRORE) interfaccia = false;         // (scheda tolta: si ricerca l'interfaccia)
                }
            }
            // Esito che non arriva (avvisi non registrati, avviso perso): dopo 25 s il collegamento risulta non riuscito,
            // invece di "Collegamento…" per sempre. Windows di solito risponde in pochi secondi
            if (collegaT) {
                EnterCriticalSection(&me->cs);
                const bool attesa = me->esitoP == E_IN_CORSO;
                if (attesa && GetTickCount64() - collegaT > 25000) me->esitoP = E_FALLITO;
                const bool finito = me->esitoP != E_IN_CORSO;
                LeaveCriticalSection(&me->cs);
                if (finito) { collegaT = 0; if (attesa) me->pronto = true; }
            }
            HANDLE w[3] = {me->fermo, me->comando, me->notifica};
            const DWORD attendi = scansioneT ? 500 : collegaT ? 1000 : (h ? 30000 : 15000);
            const DWORD r = WaitForMultipleObjects(me->notifica ? 3 : 2, w, FALSE, attendi);
            if (r == WAIT_OBJECT_0 || r == WAIT_FAILED) break;
        }
        // uscita: prima si toglie la notifica (il callback non deve piu' arrivare), poi la maniglia
        if (h) {
            if (registrata) a.notifiche(h, WLAN_NOTIFICATION_SOURCE_NONE, TRUE, nullptr, nullptr, nullptr, nullptr);
            a.chiudi(h, nullptr);
        }
        if (me->notifica) { CloseHandle(me->notifica); me->notifica = nullptr; }
        if (a.dll) FreeLibrary(a.dll);
        return 0;
    }
};

}  // namespace wifi
