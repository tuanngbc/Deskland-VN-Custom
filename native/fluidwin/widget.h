// Dati del pannello widget: meteo (Open-Meteo, servizio gratuito senza account: si invia solo la posizione della
// citta') e notizie (feed RSS ufficiali: ANSA, BBC News, The Guardian, Al Jazeera). Si scaricano in un thread a parte
// con WinHTTP, al massimo ogni 10 minuti e solo quando il pannello si apre; il pannello legge una copia gia' pronta.
// La citta' si sceglie dal pannello cercandola per nome (geocodifica di Open-Meteo: si invia solo il testo scritto),
// la fonte delle notizie da un menu (di serie segue la lingua di FluidWin), l'unita' fra gradi Celsius e Fahrenheit
// (di serie segue il sistema di misura di Windows). Issue #12 e #14: prima la citta' stava solo nel registro e le
// notizie erano sempre quelle italiane dell'ANSA.
#pragma once
#include "tlauncher_identity.h"
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <winhttp.h>
#include <string>
#include <vector>
#include <atomic>
#include <cstdlib>
#include <cmath>
#include <algorithm>
#include <cstdio>
#include <cstring>
#include "testi.h"

namespace widget {

struct Giorno { int codice = 0; double max = 0, min = 0; int settimana = -1; };   // settimana: 0 domenica .. 6 sabato, -1 oggi
struct Notizia { std::wstring titolo, link, quando; };

// Fonti delle notizie. F_AUTO segue la lingua di FluidWin (italiano: ANSA, inglese: BBC News); F_NESSUNA le spegne.
// I valori finiscono nel registro (NotizieFonte): non si riordinano
enum Fonte { F_AUTO = 0, F_ANSA = 1, F_BBC = 2, F_GUARDIAN = 3, F_ALJAZEERA = 4, F_NESSUNA = 5 };
struct InfoFonte { const wchar_t *nome, *host, *percorso; };
inline const InfoFonte& infoFonte(int f) {
    // solo feed RSS 2.0 (<item> senza attributi): quelli RDF, come DW, il lettore qui sotto non li capisce
    static const InfoFonte fonti[] = {
        {L"ANSA", L"www.ansa.it", L"/sito/ansait_rss.xml"},
        {L"BBC News", L"feeds.bbci.co.uk", L"/news/world/rss.xml"},
        {L"The Guardian", L"www.theguardian.com", L"/world/rss"},
        {L"Al Jazeera", L"www.aljazeera.com", L"/xml/rss/all.xml"},
    };
    return fonti[std::clamp(f, (int)F_ANSA, (int)F_ALJAZEERA) - F_ANSA];
}

// unita' di misura: U_AUTO segue Windows (sistema di misura statunitense -> Fahrenheit e mph). Nel registro: MeteoUnita
enum Unita { U_AUTO = 0, U_CELSIUS = 1, U_FAHRENHEIT = 2 };

// un luogo trovato cercando la citta' (nome come lo scrive Open-Meteo nella lingua chiesta)
struct Luogo { std::wstring nome, dove; double lat = 0, lon = 0; };   // dove: "Lombardia, Italia"

// stato della ricerca della citta'
enum StatoRicerca { R_VUOTA, R_CERCA, R_PRONTA, R_ERRORE };

struct Istantanea {
    bool meteoOk = false;
    double temp = 0, vento = 0, umidita = 0;   // sempre in gradi Celsius e km/h: l'unita' scelta si applica disegnando
    int codice = 0;
    std::vector<Giorno> giorni;
    std::vector<Notizia> notizie;
};

inline bool fahrenheitDiWindows() {
    // LOCALE_IMEASURE: "0" sistema metrico, "1" statunitense
    wchar_t b[4] = {};
    return GetLocaleInfoEx(LOCALE_NAME_USER_DEFAULT, LOCALE_IMEASURE, b, 4) > 0 && b[0] == L'1';
}

// descrizione del codice meteo WMO (quello che usa Open-Meteo)
inline const wchar_t* descrizione(int c) {
    using testi::t;
    if (c == 0) return t(L"Sereno", L"Clear");
    if (c == 1) return t(L"Quasi sereno", L"Mostly clear");
    if (c == 2) return t(L"Parzialmente nuvoloso", L"Partly cloudy");
    if (c == 3) return t(L"Coperto", L"Overcast");
    if (c == 45 || c == 48) return t(L"Nebbia", L"Fog");
    if (c >= 51 && c <= 57) return t(L"Pioviggine", L"Drizzle");
    if (c >= 61 && c <= 67) return t(L"Pioggia", L"Rain");
    if (c >= 71 && c <= 77) return t(L"Neve", L"Snow");
    if (c >= 80 && c <= 82) return t(L"Rovesci", L"Showers");
    if (c >= 85 && c <= 86) return t(L"Rovesci di neve", L"Snow showers");
    if (c >= 95) return t(L"Temporale", L"Thunderstorm");
    return L"";
}

// nome breve del giorno nelle previsioni ("Oggi", "Lun"...)
inline const wchar_t* nomeGiorno(const Giorno& g) {
    if (g.settimana < 0 || g.settimana > 6) return testi::t(L"Oggi", L"Today");
    // nomi brevi dalla lingua scelta (Windows li conosce tutti): letti una volta per lingua, dal 1 gennaio 2023 che era
    // domenica. Solo dal thread principale (lo usa il disegno del pannello)
    static wchar_t nomi[testi::N_LINGUE][7][24] = {};
    const int l = testi::attuale();
    wchar_t* s = nomi[l][g.settimana];
    if (!*s) {
        SYSTEMTIME st{}; st.wYear = 2023; st.wMonth = 1; st.wDay = (WORD)(1 + g.settimana); st.wDayOfWeek = (WORD)g.settimana;
        if (!GetDateFormatEx(testi::locale(), 0, &st, L"ddd", s, 24, nullptr)) {
            static const wchar_t* en[7] = {L"Sun", L"Mon", L"Tue", L"Wed", L"Thu", L"Fri", L"Sat"};
            wcsncpy(s, en[g.settimana], 23);
        }
        if (l == testi::ITALIANO || l == testi::SPAGNOLO || l == testi::PORTOGHESE || l == testi::FRANCESE) s[0] = (wchar_t)towupper(s[0]);   // ("lun" -> "Lun")
    }
    return s;
}

// Tutti i metodi pubblici si chiamano dal thread principale. I thread di lavoro leggono solo la richiesta copiata
// sotto la sezione critica (mai citta', lat, lon, fonte) e non toccano finestre: il pannello raccoglie i risultati
// con pronto / ricercaPronta (letti dal suo timer o da aggiornaStato del dock)
class Dati {
public:
    // la citta' del meteo sta nel registro (HKCU\Software\FluidWin\Impostazioni: MeteoCitta, MeteoLat, MeteoLon);
    // senza, Roma. Si cambia con impostaLuogo
    std::wstring citta = L"Roma";
    double lat = 41.9028, lon = 12.4964;
    int fonte = F_AUTO, unita = U_AUTO;
    std::atomic<bool> pronto{false};          // arrivati dati nuovi (il pannello si ridisegna)
    std::atomic<bool> ricercaPronta{false};   // arrivati (o cambiati) i risultati della ricerca della citta'

    Dati() {
        InitializeCriticalSection(&cs);
        wchar_t b[128]; DWORD n = sizeof(b);
        if (RegGetValueW(HKEY_CURRENT_USER, CHIAVE, L"MeteoCitta", RRF_RT_REG_SZ, nullptr, b, &n) == ERROR_SUCCESS && b[0]) {
            wchar_t la[32], lo[32]; DWORD nl = sizeof(la), no = sizeof(lo);
            if (RegGetValueW(HKEY_CURRENT_USER, CHIAVE, L"MeteoLat", RRF_RT_REG_SZ, nullptr, la, &nl) == ERROR_SUCCESS &&
                RegGetValueW(HKEY_CURRENT_USER, CHIAVE, L"MeteoLon", RRF_RT_REG_SZ, nullptr, lo, &no) == ERROR_SUCCESS) {
                const double a = _wtof(la), o = _wtof(lo);
                if (a >= -90 && a <= 90 && o >= -180 && o <= 180 && (a != 0 || o != 0)) { citta = b; lat = a; lon = o; }
            }
        }
        DWORD v = 0; n = sizeof(v);
        if (RegGetValueW(HKEY_CURRENT_USER, CHIAVE, L"NotizieFonte", RRF_RT_REG_DWORD, nullptr, &v, &n) == ERROR_SUCCESS && v <= F_NESSUNA) fonte = (int)v;
        v = 0; n = sizeof(v);
        if (RegGetValueW(HKEY_CURRENT_USER, CHIAVE, L"MeteoUnita", RRF_RT_REG_DWORD, nullptr, &v, &n) == ERROR_SUCCESS && v <= U_FAHRENHEIT) unita = (int)v;
    }

    // la fonte vera (F_AUTO risolta con la lingua di adesso), F_NESSUNA se le notizie sono spente
    // (automatica: ANSA in italiano, Al Jazeera in arabo, BBC nelle altre lingue)
    int fonteEffettiva() const { return fonte == F_AUTO ? (testi::italiano() ? F_ANSA : testi::rtl() ? F_ALJAZEERA : F_BBC) : fonte; }
    bool fahrenheit() const { return unita == U_AUTO ? fahrenheitDiWindows() : unita == U_FAHRENHEIT; }
    double gradi(double celsius) const { return fahrenheit() ? celsius * 9 / 5 + 32 : celsius; }

    // Chiede dati nuovi se gli ultimi hanno piu' di 10 minuti (o non ci sono), oppure subito se da allora sono
    // cambiati citta' o fonte (anche la lingua, con la fonte automatica). Un solo thread alla volta: se ne sta gia'
    // lavorando uno, prende la richiesta nuova appena finisce e scarta il risultato vecchio (generazione)
    void aggiorna() {
        const Richiesta r{lat, lon, fonteEffettiva()};
        const ULONGLONG ora = GetTickCount64(), u = ultimo.load();
        const bool cambiata = !(r == chiesta);
        if (!cambiata && u && ora - u < 10 * 60 * 1000) return;
        chiedi(r, false, false);
    }

    Istantanea copia() { EnterCriticalSection(&cs); Istantanea c = dati; LeaveCriticalSection(&cs); return c; }

    // Citta' scelta dalla ricerca: si salva e si chiede subito il meteo nuovo. Il meteo della citta' di prima si
    // toglie gia' adesso: "Milano" con le temperature di Roma sarebbe sbagliato anche per un attimo
    void impostaLuogo(const Luogo& l) {
        if (l.nome.empty() || !(l.lat >= -90 && l.lat <= 90 && l.lon >= -180 && l.lon <= 180)) return;
        citta = l.nome; lat = l.lat; lon = l.lon;
        wchar_t la[32], lo[32];
        swprintf(la, 32, L"%.4f", lat); swprintf(lo, 32, L"%.4f", lon);   // (col punto: si rilegge con _wtof)
        scriviTesto(L"MeteoCitta", citta); scriviTesto(L"MeteoLat", la); scriviTesto(L"MeteoLon", lo);
        // sempre una richiesta, anche se e' lo stesso luogo di prima: il meteo appena tolto va riscaricato subito
        chiedi({lat, lon, fonteEffettiva()}, true, false);
        pronto = true;
    }

    // fonte delle notizie (F_AUTO .. F_NESSUNA): le notizie della fonte di prima spariscono subito
    void impostaFonte(int f) {
        if (f < F_AUTO || f > F_NESSUNA || f == fonte) return;
        const int prima = fonteEffettiva();
        fonte = f;
        scriviNumero(L"NotizieFonte", (DWORD)f);
        if (fonteEffettiva() != prima) {
            chiedi({lat, lon, fonteEffettiva()}, false, true);
            pronto = true;
        }
    }

    // unita' (U_AUTO .. U_FAHRENHEIT): i dati restano in Celsius, cambia solo il disegno
    void impostaUnita(int u) {
        if (u < U_AUTO || u > U_FAHRENHEIT || u == unita) return;
        unita = u;
        scriviNumero(L"MeteoUnita", (DWORD)u);
        pronto = true;
    }

    // Ricerca della citta' per nome (da 2 lettere in su: con una Open-Meteo non risponde niente). Come aggiorna: un
    // solo thread, che alla fine riparte se nel frattempo il testo e' cambiato; i risultati di un testo vecchio si
    // scartano. Testo vuoto: niente richiesta, elenco vuoto
    void cerca(const std::wstring& testo) {
        std::wstring t = testo;
        while (!t.empty() && t.back() == L' ') t.pop_back();
        while (!t.empty() && t.front() == L' ') t.erase(t.begin());
        EnterCriticalSection(&cs);
        cercaGen++;
        cercaTesto = t; cercaLingua = testi::codice();
        const bool corto = t.size() < 2;
        if (corto) { luoghi.clear(); statoRicerca = R_VUOTA; }
        else statoRicerca = R_CERCA;
        const bool avvia = !corto && !cercaInCorso;
        if (avvia) cercaInCorso = true;
        LeaveCriticalSection(&cs);
        ricercaPronta = true;   // lo stato e' cambiato (vuota o "cerco...")
        if (!avvia) return;
        attivi++;
        HANDLE h = CreateThread(nullptr, 0, lavoroCerca, this, 0, nullptr);
        if (h) CloseHandle(h);
        else { attivi--; EnterCriticalSection(&cs); cercaInCorso = false; statoRicerca = R_ERRORE; LeaveCriticalSection(&cs); }
    }

    std::vector<Luogo> risultati(int& stato) {
        EnterCriticalSection(&cs); std::vector<Luogo> r = luoghi; stato = statoRicerca; LeaveCriticalSection(&cs);
        return r;
    }

    // All'uscita: aspetta (al massimo ms) che i thread di meteo, notizie e ricerca finiscano. Dopo wWinMain i
    // distruttori statici distruggono questi dati: un thread ancora dentro WinHTTP (fino a 26 s di limiti con la rete
    // appesa) li userebbe dopo. false: non hanno finito, il chiamante chiude il processo senza i distruttori
    bool ferma(DWORD ms) {
        const ULONGLONG fine = GetTickCount64() + ms;
        while (attivi.load() > 0 && GetTickCount64() < fine) Sleep(10);
        return attivi.load() == 0;
    }

private:
    static constexpr const wchar_t* CHIAVE = tlauncher::settingsRegistry();
    struct Richiesta {
        double lat = 1000, lon = 1000; int fonte = -1;   // (1000: nessuna richiesta ancora)
        bool operator==(const Richiesta& o) const { return lat == o.lat && lon == o.lon && fonte == o.fonte; }
    };
    CRITICAL_SECTION cs{};
    // protetti da cs
    Istantanea dati;
    Richiesta richiesta;
    unsigned gen = 0;
    bool inCorso = false;
    int fonteNotizie = -1;   // fonte delle notizie in dati
    std::vector<Luogo> luoghi;
    int statoRicerca = R_VUOTA;
    std::wstring cercaTesto;
    const wchar_t* cercaLingua = L"en";   // lingua dei nomi delle citta' (codice di due lettere, statico)
    unsigned cercaGen = 0;
    bool cercaInCorso = false;
    // solo thread principale (ultimo e' scritto anche dal thread, per riprovare prima quando la rete manca)
    Richiesta chiesta;
    std::atomic<ULONGLONG> ultimo{0};
    std::atomic<int> attivi{0};   // thread di lavoro vivi (ferma); ognuno lo scala come ultima cosa che tocca di Dati

    // Nuova richiesta di meteo e notizie. Pulizia dei dati vecchi (togliMeteo: citta' cambiata; togliNotizie: fonte
    // cambiata), nuova generazione e decisione di avviare il thread nella STESSA sezione critica: un thread che stava
    // finendo la richiesta di prima, da qui in poi, trova la generazione cambiata e scarta il suo risultato.
    // R20261119-132417-S1: prima la pulizia e l'aumento di gen erano in due sezioni separate, e fra le due il thread
    // poteva pubblicare il meteo di Roma sotto il nome di Milano (e tenerlo, se lo scaricamento nuovo falliva)
    void chiedi(const Richiesta& r, bool togliMeteo, bool togliNotizie) {
        chiesta = r; ultimo = GetTickCount64();
        EnterCriticalSection(&cs);
        if (togliMeteo) { dati.meteoOk = false; dati.giorni.clear(); }
        if (togliNotizie) dati.notizie.clear();
        richiesta = r; gen++;
        const bool avvia = !inCorso;
        if (avvia) inCorso = true;
        LeaveCriticalSection(&cs);
        if (!avvia) return;   // il thread che lavora vedra' la generazione nuova
        attivi++;
        HANDLE t = CreateThread(nullptr, 0, lavoro, this, 0, nullptr);
        if (t) CloseHandle(t);
        else { attivi--; EnterCriticalSection(&cs); inCorso = false; LeaveCriticalSection(&cs); ultimo = 0; chiesta = {}; }
    }

    static void scriviTesto(const wchar_t* nome, const std::wstring& v) {
        RegSetKeyValueW(HKEY_CURRENT_USER, CHIAVE, nome, REG_SZ, v.c_str(), (DWORD)((v.size() + 1) * sizeof(wchar_t)));
    }
    static void scriviNumero(const wchar_t* nome, DWORD v) { RegSetKeyValueW(HKEY_CURRENT_USER, CHIAVE, nome, REG_DWORD, &v, sizeof(v)); }

    static std::string scarica(const wchar_t* host, const std::wstring& percorso) {
        std::string out;
        HINTERNET s = WinHttpOpen(L"TLauncher/1.0", WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, nullptr, nullptr, 0);
        if (!s) return out;
        WinHttpSetTimeouts(s, 5000, 5000, 8000, 8000);
        HINTERNET c = WinHttpConnect(s, host, INTERNET_DEFAULT_HTTPS_PORT, 0);
        HINTERNET r = c ? WinHttpOpenRequest(c, L"GET", percorso.c_str(), nullptr, WINHTTP_NO_REFERER, WINHTTP_DEFAULT_ACCEPT_TYPES, WINHTTP_FLAG_SECURE) : nullptr;
        DWORD stato = 0, ls = sizeof(stato);
        if (r && WinHttpSendRequest(r, WINHTTP_NO_ADDITIONAL_HEADERS, 0, nullptr, 0, 0, 0) && WinHttpReceiveResponse(r, nullptr) &&
            // solo le risposte buone: una pagina d'errore (404, 500...) non va letta come dati
            WinHttpQueryHeaders(r, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_HEADER_NAME_BY_INDEX, &stato, &ls, WINHTTP_NO_HEADER_INDEX) &&
            stato == 200) {
            DWORD n = 0;
            while (WinHttpQueryDataAvailable(r, &n) && n > 0 && out.size() < 4 * 1024 * 1024) {
                std::string b(n, '\0'); DWORD letti = 0;
                if (!WinHttpReadData(r, b.data(), n, &letti) || !letti) break;
                out.append(b.data(), letti);
            }
        }
        if (r) WinHttpCloseHandle(r);
        if (c) WinHttpCloseHandle(c);
        WinHttpCloseHandle(s);
        return out;
    }

    static std::wstring daUtf8(const std::string& s) {
        if (s.empty()) return L"";
        const int n = MultiByteToWideChar(CP_UTF8, 0, s.data(), (int)s.size(), nullptr, 0);
        std::wstring w(n, L'\0');
        MultiByteToWideChar(CP_UTF8, 0, s.data(), (int)s.size(), w.data(), n);
        return w;
    }

    // ---- JSON di Open-Meteo: bastano un numero dopo una chiave e un elenco di numeri ----
    static size_t dopo(const std::string& j, const std::string& chiave, size_t da = 0) {
        const size_t p = j.find("\"" + chiave + "\"", da);
        return p == std::string::npos ? p : j.find(':', p) + 1;
    }
    static double numero(const std::string& j, const std::string& chiave, size_t da, bool& ok) {
        const size_t p = dopo(j, chiave, da);
        if (p == std::string::npos || p == 0) { ok = false; return 0; }
        // (un null o un valore non numerico non e' uno 0 vero: dato mancante, S19)
        const char* c = j.c_str() + p; while (*c == ' ') c++;
        char* e = nullptr; const double x = strtod(c, &e);
        if (e == c || !std::isfinite(x)) { ok = false; return 0; }
        return x;
    }
    // giorno della settimana (0 domenica) di ogni data "AAAA-MM-GG" dell'elenco 'chiave' (-1 se non si legge)
    static std::vector<int> giorniSettimana(const std::string& j, const std::string& chiave, size_t da) {
        std::vector<int> v;
        size_t p = dopo(j, chiave, da);
        if (p == std::string::npos || p == 0) return v;
        p = j.find('[', p); const size_t fine = j.find(']', p);
        if (p == std::string::npos || fine == std::string::npos) return v;
        for (size_t q = j.find('"', p); q != std::string::npos && q < fine; q = j.find('"', q + 1)) {
            const size_t r = j.find('"', q + 1); if (r == std::string::npos || r > fine) break;
            SYSTEMTIME st{}; FILETIME f{};
            const char* s = j.c_str() + q + 1; char* e = nullptr;
            const long a = strtol(s, &e, 10); const bool t1 = e != s && *e == '-';
            const long m = t1 ? strtol(e + 1, &e, 10) : 0; const bool t2 = t1 && *e == '-';
            const long g = t2 ? strtol(e + 1, &e, 10) : 0;
            if (t2 && *e == '"' && a > 1600 && a < 3000 && m >= 1 && m <= 12 && g >= 1 && g <= 31) {
                st.wYear = (WORD)a; st.wMonth = (WORD)m; st.wDay = (WORD)g;
                v.push_back(SystemTimeToFileTime(&st, &f) && FileTimeToSystemTime(&f, &st) ? (int)st.wDayOfWeek : -1);
            } else v.push_back(-1);
            q = r;
        }
        return v;
    }
    static std::vector<double> elenco(const std::string& j, const std::string& chiave, size_t da) {
        std::vector<double> v;
        size_t p = dopo(j, chiave, da);
        if (p == std::string::npos || p == 0) return v;
        p = j.find('[', p); const size_t fine = j.find(']', p);
        if (p == std::string::npos || fine == std::string::npos) return v;
        const char* c = j.c_str() + p + 1;
        while (c < j.c_str() + fine) { char* e = nullptr; const double x = strtod(c, &e); if (e == c) break; v.push_back(x); c = e; while (*c == ',' || *c == ' ') c++; }
        return v;
    }

    static void meteo(Istantanea& d, const Richiesta& q) {
        wchar_t p[400];
        swprintf(p, 400, L"/v1/forecast?latitude=%.4f&longitude=%.4f&current=temperature_2m,weather_code,wind_speed_10m,relative_humidity_2m"
                         L"&daily=weather_code,temperature_2m_max,temperature_2m_min&timezone=auto&forecast_days=5", q.lat, q.lon);   // (giorni del fuso della citta')
        const std::string j = scarica(L"api.open-meteo.com", p);
        const size_t cur = j.find("\"current\":");
        if (cur == std::string::npos) return;
        bool ok = true;
        d.temp = numero(j, "temperature_2m", cur, ok);
        d.codice = (int)numero(j, "weather_code", cur, ok);
        d.vento = numero(j, "wind_speed_10m", cur, ok);
        d.umidita = numero(j, "relative_humidity_2m", cur, ok);
        const size_t day = j.find("\"daily\":");
        if (day != std::string::npos) {
            const auto cod = elenco(j, "weather_code", day), mx = elenco(j, "temperature_2m_max", day), mn = elenco(j, "temperature_2m_min", day);
            // il giorno della settimana dalla data di ogni previsione (fuso della citta'): prima dal giorno del PC, e
            // con la citta' gia' al giorno dopo le etichette erano spostate di uno (R20261121-002818-S18)
            const auto gs = giorniSettimana(j, "time", day);
            SYSTEMTIME st; GetLocalTime(&st);
            for (size_t i = 0; i < cod.size() && i < mx.size() && i < mn.size(); i++)
                d.giorni.push_back({(int)cod[i], mx[i], mn[i], i == 0 ? -1 : (i < gs.size() && gs[i] >= 0 ? gs[i] : (int)((st.wDayOfWeek + i) % 7))});
        }
        d.meteoOk = ok;
    }

    // ---- notizie: RSS 2.0 della fonte scelta ----
    static std::string tra(const std::string& s, const std::string& a, const std::string& b, size_t& da) {
        const size_t p = s.find(a, da); if (p == std::string::npos) { da = std::string::npos; return ""; }
        const size_t q = s.find(b, p + a.size()); if (q == std::string::npos) { da = std::string::npos; return ""; }
        da = q + b.size();
        return s.substr(p + a.size(), q - p - a.size());
    }
    static std::wstring pulisci(std::string s) {
        if (s.rfind("<![CDATA[", 0) == 0 && s.size() >= 12) s = s.substr(9, s.size() - 12);
        std::wstring w = daUtf8(s), o;
        for (size_t i = 0; i < w.size(); i++) {
            if (w[i] == L'&') {
                const size_t f = w.find(L';', i);
                if (f != std::wstring::npos && f - i < 10) {
                    const std::wstring e = w.substr(i + 1, f - i - 1);
                    unsigned long c = 0;
                    if (e == L"amp") c = L'&'; else if (e == L"quot") c = L'"'; else if (e == L"apos") c = L'\''; else if (e == L"lt") c = L'<'; else if (e == L"gt") c = L'>';
                    else if (!e.empty() && e[0] == L'#') c = e.size() > 1 && (e[1] == L'x' || e[1] == L'X') ? wcstoul(e.c_str() + 2, nullptr, 16) : wcstoul(e.c_str() + 1, nullptr, 10);
                    if (c && c <= 0x10FFFF) {
                        // oltre 0xFFFF (emoji): due wchar_t (coppia surrogata), prima si troncava
                        if (c > 0xFFFF) { c -= 0x10000; o += (wchar_t)(0xD800 + (c >> 10)); o += (wchar_t)(0xDC00 + (c & 0x3FF)); }
                        else o += (wchar_t)c;
                        i = f; continue;
                    }
                }
            }
            o += (w[i] == L'\n' || w[i] == L'\r' || w[i] == L'\t') ? L' ' : w[i];
        }
        while (!o.empty() && o.back() == L' ') o.pop_back();
        while (!o.empty() && o.front() == L' ') o.erase(o.begin());
        return o;
    }

    // Ora di pubblicazione (RFC 822: "Thu, 19 Nov 2026 04:42:52 GMT", "... +0100") nell'ora locale del PC, "HH:MM".
    // Prima si prendeva l'ora scritta nel feed: andava bene per l'ANSA (ora italiana), ma BBC e Al Jazeera la danno in
    // UTC e chi sta altrove avrebbe visto un'ora sbagliata. Se la data non si capisce, l'ora com'e' scritta
    static std::wstring oraLocale(const std::string& data) {
        static const char* mesi[12] = {"Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"};
        const size_t virgola = data.find(',');
        const char* c = data.c_str() + (virgola == std::string::npos ? 0 : virgola + 1);
        // "19 Nov 2026 04:42:52 GMT": numero, mese, numero, ore:minuti[:secondi], fuso facoltativo
        auto spazi = [&] { while (*c == ' ' || *c == '\t') c++; };
        auto intero = [&](int& v) { spazi(); char* e = nullptr; const long x = strtol(c, &e, 10); if (e == c) return false; v = (int)x; c = e; return true; };
        int g = 0, a = 0, h = 0, mi = 0, s = 0; char mese[4] = {}, zona[8] = {};
        bool letto = intero(g);
        spazi();
        for (int i = 0; i < 3 && *c && *c != ' '; i++) mese[i] = *c++;
        letto = letto && intero(a) && intero(h) && *c == ':';
        if (letto) { c++; letto = intero(mi); }
        if (letto && *c == ':') { c++; letto = intero(s); }
        spazi();
        for (int i = 0; i < 7 && *c && *c != ' '; i++) zona[i] = *c++;
        int m = -1;
        for (int i = 0; i < 12; i++) if (_stricmp(mese, mesi[i]) == 0) m = i;
        SYSTEMTIME st{};
        FILETIME ft{};
        if (letto && m >= 0 && a >= 1970 && a < 3000 && g >= 1 && g <= 31 && h >= 0 && h < 24 && mi >= 0 && mi < 60 && s >= 0 && s <= 60) {
            st.wYear = (WORD)a; st.wMonth = (WORD)(m + 1); st.wDay = (WORD)g; st.wHour = (WORD)h; st.wMinute = (WORD)mi; st.wSecond = (WORD)std::min(s, 59);
            if (SystemTimeToFileTime(&st, &ft)) {
                // fuso: "+hhmm" / "-hhmm"; GMT, UT, Z o nessuno = UTC (le sigle americane EST/PST nei feed scelti non ci sono)
                long minuti = 0;
                if ((zona[0] == '+' || zona[0] == '-') && strlen(zona) == 5) {
                    const long v = strtol(zona + 1, nullptr, 10);
                    minuti = (v / 100) * 60 + v % 100;
                    if (zona[0] == '-') minuti = -minuti;
                }
                ULARGE_INTEGER u; u.LowPart = ft.dwLowDateTime; u.HighPart = ft.dwHighDateTime;
                u.QuadPart -= (ULONGLONG)((LONGLONG)minuti * 60 * 10000000LL);   // ora scritta - fuso = UTC
                ft.dwLowDateTime = u.LowPart; ft.dwHighDateTime = u.HighPart;
                SYSTEMTIME utc{}, loc{};
                if (FileTimeToSystemTime(&ft, &utc) && SystemTimeToTzSpecificLocalTime(nullptr, &utc, &loc)) {
                    wchar_t b[8]; swprintf(b, 8, L"%02d:%02d", loc.wHour, loc.wMinute);
                    return b;
                }
            }
        }
        const size_t due = data.find(':');
        return due != std::string::npos && due >= 2 ? daUtf8(data.substr(due - 2, 5)) : L"";
    }

    static void notizie(Istantanea& d, int fonte) {
        if (fonte < F_ANSA || fonte > F_ALJAZEERA) return;
        const InfoFonte& f = infoFonte(fonte);
        const std::string x = scarica(f.host, f.percorso);
        size_t p = 0;
        while (d.notizie.size() < 8) {
            const std::string item = tra(x, "<item>", "</item>", p);
            if (p == std::string::npos) break;
            size_t q = 0; Notizia n;
            n.titolo = pulisci(tra(item, "<title>", "</title>", q)); q = 0;
            n.link = pulisci(tra(item, "<link>", "</link>", q)); q = 0;
            n.quando = oraLocale(tra(item, "<pubDate>", "</pubDate>", q));
            if (!n.titolo.empty() && n.link.rfind(L"https://", 0) == 0) d.notizie.push_back(n);
        }
    }

    static DWORD WINAPI lavoro(LPVOID p) {
        Dati* me = (Dati*)p;
        for (;;) {
            EnterCriticalSection(&me->cs);
            const Richiesta q = me->richiesta; const unsigned g = me->gen;
            LeaveCriticalSection(&me->cs);
            Istantanea d;
            meteo(d, q);
            notizie(d, q.fonte);
            EnterCriticalSection(&me->cs);
            if (g != me->gen) { LeaveCriticalSection(&me->cs); continue; }   // nel frattempo e' cambiata la richiesta
            // Meteo: il nuovo se c'e'; altrimenti resta il vecchio (stessa citta': impostaLuogo toglie quello di
            // un'altra). Notizie: le nuove se ci sono; con le notizie spente nessuna
            if (d.meteoOk || me->dati.giorni.empty()) { me->dati.meteoOk = d.meteoOk; me->dati.temp = d.temp; me->dati.codice = d.codice; me->dati.vento = d.vento; me->dati.umidita = d.umidita; me->dati.giorni = d.giorni; }
            // (fonte diversa da quella delle notizie che ci sono, per esempio dopo un cambio di lingua: quelle vecchie
            // si tolgono anche se le nuove non sono arrivate)
            if (!d.notizie.empty() || q.fonte != me->fonteNotizie) { me->dati.notizie = d.notizie; me->fonteNotizie = q.fonte; }
            const bool vuoto = !d.meteoOk && d.notizie.empty();
            me->inCorso = false;   // (deciso sotto cs: aggiorna, se chiama adesso, avvia un thread nuovo)
            LeaveCriticalSection(&me->cs);
            if (vuoto) me->ultimo = GetTickCount64() - 9 * 60 * 1000;   // niente rete: si riprova fra un minuto
            me->pronto = true;
            me->attivi--;
            return 0;
        }
    }

    // ---- ricerca della citta' (geocodifica di Open-Meteo) ----

    // testo per l'indirizzo: UTF-8, e %XX per tutto quello che non e' lettera, cifra o - _ . ~
    static std::wstring perIndirizzo(const std::wstring& w) {
        std::string u;
        const int n = WideCharToMultiByte(CP_UTF8, 0, w.data(), (int)w.size(), nullptr, 0, nullptr, nullptr);
        if (n > 0) { u.resize(n); WideCharToMultiByte(CP_UTF8, 0, w.data(), (int)w.size(), u.data(), n, nullptr, nullptr); }
        std::wstring o;
        for (unsigned char c : u) {
            if ((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '-' || c == '_' || c == '.' || c == '~') o += (wchar_t)c;
            else { wchar_t b[4]; swprintf(b, 4, L"%%%02X", c); o += b; }
        }
        return o;
    }

    // Valore stringa di una chiave in un oggetto JSON piatto (con gli escape \" \\ \/ \n \uXXXX). La chiave si cerca con
    // le virgolette e i due punti: "country" non trova "country_code". "" se manca
    static std::wstring testoJson(const std::string& o, const char* chiave) {
        const std::string k = std::string("\"") + chiave + "\":";
        size_t p = o.find(k);
        if (p == std::string::npos) return L"";
        p += k.size();
        while (p < o.size() && o[p] == ' ') p++;
        if (p >= o.size() || o[p] != '"') return L"";
        std::wstring w; std::string pezzo;   // pezzo: byte UTF-8 ancora da convertire
        for (p++; p < o.size() && o[p] != '"'; p++) {
            if (o[p] != '\\') { pezzo += o[p]; continue; }
            if (++p >= o.size()) break;
            w += daUtf8(pezzo); pezzo.clear();
            const char e = o[p];
            if (e == 'u' && p + 4 < o.size()) { w += (wchar_t)strtoul(o.substr(p + 1, 4).c_str(), nullptr, 16); p += 4; }   // (le coppie surrogate arrivano come due \u: restano due wchar_t)
            else w += e == 'n' || e == 't' || e == 'r' ? L' ' : (wchar_t)(unsigned char)e;
        }
        return w + daUtf8(pezzo);
    }

    static bool numeroJson(const std::string& o, const char* chiave, double& v) {
        const std::string k = std::string("\"") + chiave + "\":";
        const size_t p = o.find(k);
        if (p == std::string::npos) return false;
        const char* c = o.c_str() + p + k.size();
        char* e = nullptr;
        v = strtod(c, &e);
        return e != c && std::isfinite(v);
    }

    // gli oggetti di primo livello dell'array "results" (contando le graffe fuori dalle stringhe)
    static std::vector<std::string> oggettiRisultati(const std::string& j) {
        std::vector<std::string> v;
        size_t p = j.find("\"results\":");
        if (p == std::string::npos) return v;
        p = j.find('[', p);
        if (p == std::string::npos) return v;
        int liv = 0; bool str = false; size_t inizio = 0;
        for (size_t i = p + 1; i < j.size(); i++) {
            const char c = j[i];
            if (str) { if (c == '\\') i++; else if (c == '"') str = false; continue; }
            if (c == '"') str = true;
            else if (c == '{') { if (liv++ == 0) inizio = i; }
            else if (c == '}') { if (--liv == 0) v.push_back(j.substr(inizio, i - inizio + 1)); if (liv < 0) break; }
            else if (c == ']' && liv == 0) break;
        }
        return v;
    }

    static DWORD WINAPI lavoroCerca(LPVOID p) {
        Dati* me = (Dati*)p;
        for (;;) {
            EnterCriticalSection(&me->cs);
            const std::wstring testo = me->cercaTesto; const std::wstring lingua = me->cercaLingua; const unsigned g = me->cercaGen;
            // Avviato per un testo lungo che nel frattempo e' diventato corto: cerca ha gia' messo stato ed elenco
            // (vuoti); il thread se ne va senza toccarli. Prima risultava "nessuna connessione" (prova_widget:
            // "Parigi" e subito "P")
            if (testo.size() < 2) { me->cercaInCorso = false; LeaveCriticalSection(&me->cs); me->attivi--; return 0; }
            LeaveCriticalSection(&me->cs);
            std::vector<Luogo> trovati;
            bool ok = false;
            {
                const std::wstring percorso = L"/v1/search?name=" + perIndirizzo(testo.substr(0, 60)) + L"&count=6&format=json&language=" + lingua;
                const std::string j = scarica(L"geocoding-api.open-meteo.com", percorso);
                ok = j.find('{') != std::string::npos;   // risposta arrivata (anche senza "results": nessun luogo)
                for (const std::string& o : oggettiRisultati(j)) {
                    Luogo l; double a = 0, b = 0;
                    l.nome = testoJson(o, "name");
                    if (l.nome.empty() || !numeroJson(o, "latitude", a) || !numeroJson(o, "longitude", b) || a < -90 || a > 90 || b < -180 || b > 180) continue;
                    l.lat = a; l.lon = b;
                    const std::wstring regione = testoJson(o, "admin1"), paese = testoJson(o, "country");
                    l.dove = regione;
                    if (!paese.empty() && paese != regione) l.dove += (l.dove.empty() ? L"" : L", ") + paese;
                    trovati.push_back(l);
                }
            }
            EnterCriticalSection(&me->cs);
            if (g != me->cercaGen) {   // il testo e' cambiato: si riparte con quello nuovo (o ci si ferma, se e' corto)
                const bool ancora = me->cercaTesto.size() >= 2;
                if (!ancora) me->cercaInCorso = false;
                LeaveCriticalSection(&me->cs);
                if (ancora) continue;
                me->attivi--;
                return 0;
            }
            me->luoghi = std::move(trovati);
            me->statoRicerca = ok ? R_PRONTA : R_ERRORE;
            me->cercaInCorso = false;
            LeaveCriticalSection(&me->cs);
            me->ricercaPronta = true;
            me->attivi--;
            return 0;
        }
    }
};

}  // namespace widget
