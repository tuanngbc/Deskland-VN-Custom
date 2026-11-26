// Calendari non gregoriani (issue #24): il calendario del centro di controllo segue quello scelto in Windows
// (Impostazioni > Ora e lingua > Area geografica: LOCALE_ICALENDARTYPE dell'utente), con i calcoli di ICU di Windows
// (icu.dll in System32, da Windows 10 1903; API C documentata da Microsoft, nessuna dipendenza nuova). Caricata la
// prima volta che serve; senza icu.dll, o con un calendario che ICU non ha, resta il gregoriano di sempre.
// Solo thread principale (cache statiche).
#pragma once
#include <windows.h>
#include <string>
#include <cwctype>
#include <type_traits>
#include <cmath>

namespace calendario {

// ---- ICU: solo le funzioni che servono, prese per nome (icu.dll le esporta senza suffisso di versione) ----
using UDate = double;
using UErrorCode = int;   // > 0 errore, <= 0 riuscita (anche con avvertimento)
struct UCalendar; struct UDateFormat;
enum : int { UCAL_ERA = 0, UCAL_YEAR = 1, UCAL_MONTH = 2, UCAL_DATE = 5, UCAL_DAY_OF_WEEK = 7, UCAL_HOUR_OF_DAY = 11 };
enum : int { UCAL_ACTUAL_MAXIMUM = 5 };
enum : int { UDAT_PATTERN = -2 };

struct Icu {
    UCalendar* (*apri)(const wchar_t*, int, const char*, int, UErrorCode*) = nullptr;   // ucal_open
    void (*chiudi)(UCalendar*) = nullptr;                                              // ucal_close
    int (*leggi)(const UCalendar*, int, UErrorCode*) = nullptr;                        // ucal_get
    void (*imposta)(UCalendar*, int, int) = nullptr;                                   // ucal_set
    void (*aggiungi)(UCalendar*, int, int, UErrorCode*) = nullptr;                       // ucal_add
    void (*impostaMillis)(UCalendar*, UDate, UErrorCode*) = nullptr;                     // ucal_setMillis
    int (*limite)(const UCalendar*, int, int, UErrorCode*) = nullptr;                  // ucal_getLimit
    UDate (*millis)(const UCalendar*, UErrorCode*) = nullptr;                          // ucal_getMillis
    UDateFormat* (*apriFormato)(int, int, const char*, const wchar_t*, int, const wchar_t*, int, UErrorCode*) = nullptr;   // udat_open
    void (*chiudiFormato)(UDateFormat*) = nullptr;                                     // udat_close
    int (*formatta)(const UDateFormat*, UDate, wchar_t*, int, void*, UErrorCode*) = nullptr;   // udat_format
    bool ok = false;
};

inline const Icu& icu() {
    static Icu i;
    static bool provato = false;
    if (provato) return i;
    provato = true;
    // solo da System32: un icu.dll accanto all'exe non deve essere caricato al suo posto
    HMODULE m = LoadLibraryExW(L"icu.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (!m) return i;
    auto f = [m](auto& p, const char* nome) { p = reinterpret_cast<std::remove_reference_t<decltype(p)>>(GetProcAddress(m, nome)); return p != nullptr; };   // (come wifi.h)
    i.ok = f(i.apri, "ucal_open") && f(i.chiudi, "ucal_close") && f(i.leggi, "ucal_get") && f(i.imposta, "ucal_set") &&
           f(i.limite, "ucal_getLimit") && f(i.millis, "ucal_getMillis") && f(i.apriFormato, "udat_open") &&
           f(i.chiudiFormato, "udat_close") && f(i.formatta, "udat_format") &&
           f(i.aggiungi, "ucal_add") && f(i.impostaMillis, "ucal_setMillis");
    return i;   // (la DLL resta caricata fino alla fine: e' di Windows e serve a ogni giorno nuovo)
}

// CALID di Windows -> nome del calendario in ICU. nullptr: gregoriano (anche le varianti gregoriane di Windows, e il
// coreano, che ha i mesi gregoriani e in ICU non ha un equivalente: "dangi" e' il lunisolare)
inline const char* chiaveIcu(int calid) {
    switch (calid) {
        case CAL_JAPAN: return "japanese";
        case CAL_TAIWAN: return "roc";
        case CAL_THAI: return "buddhist";
        case CAL_HIJRI: return "islamic-civil";   // l'Hijri di Windows e' quello tabellare (aritmetico)
        case CAL_HEBREW: return "hebrew";
        case 22: return "persian";                // CAL_PERSIAN
        case 23: return "islamic-umalqura";       // CAL_UMALQURA
        default: return nullptr;
    }
}
inline bool conEra(int calid) { return calid == CAL_JAPAN || calid == CAL_TAIWAN || calid == CAL_THAI; }

inline int tipoUtente() {   // calendario scelto in Windows (CAL_GREGORIAN se non si legge)
    wchar_t b[8] = L"";
    return GetLocaleInfoEx(LOCALE_NAME_USER_DEFAULT, LOCALE_ICALENDARTYPE, b, 8) ? _wtoi(b) : CAL_GREGORIAN;
}
inline int primoGiornoUtente() {   // primo giorno della settimana scelto in Windows, 0 = lunedi' ... 6 = domenica
    wchar_t b[8] = L"";
    const int g = GetLocaleInfoEx(LOCALE_NAME_USER_DEFAULT, LOCALE_IFIRSTDAYOFWEEK, b, 8) ? _wtoi(b) : 0;
    return g >= 0 && g <= 6 ? g : 0;
}

// Il mese di oggi in un calendario. ok = false: non c'e' (gregoriano, ICU assente o errore), chi chiama usa il suo
struct Mese {
    bool ok = false;
    std::wstring titolo, dataLunga;   // "Aban 1405", "mercoledi' 28 Aban 1405"
    int giorni = 0;                   // giorni del mese
    int lunediPrimo = 0;              // giorno della settimana del primo del mese, 0 = lunedi'
    int oggi = 0;                     // giorno di oggi nel mese
};

namespace dettaglio {
inline std::wstring formatta(const Icu& u, const char* loc, const wchar_t* modello, UDate t) {
    UErrorCode st = 0;
    UDateFormat* f = u.apriFormato(UDAT_PATTERN, UDAT_PATTERN, loc, nullptr, -1, modello, -1, &st);
    if (!f || st > 0) { if (f) u.chiudiFormato(f); return {}; }
    wchar_t b[128] = L"";
    st = 0;
    const int n = u.formatta(f, t, b, 128, nullptr, &st);
    u.chiudiFormato(f);
    if (st > 0 || n <= 0 || n >= 128) return {};   // (n >= 128: troncato, U_BUFFER_OVERFLOW_ERROR e' > 0 comunque)
    std::wstring s(b, (size_t)n);
    if (!s.empty()) s[0] = (wchar_t)towupper(s[0]);
    return s;
}
}  // namespace dettaglio

// Il mese costruito da zero (oggi, calendario, lingua dell'interfaccia "it-IT"/"en-US")
// TLauncher extension: preserve native month navigation; the optional fixed
// instant is used only by hosted fixtures and never changes the system clock.
inline Mese costruisci(int calid, const wchar_t* lingua, int offset = 0, UDate istante = NAN) {
    Mese r;
    const char* chiave = chiaveIcu(calid);
    const Icu& u = icu();
    if (!chiave || !u.ok) return r;
    // locale di ICU: "it_IT@calendar=persian" (nomi dei mesi nella lingua di FluidWin, come il resto del pannello)
    char loc[64] = "";
    {
        char l[16] = "";
        for (int k = 0; k < 15 && lingua[k]; k++) l[k] = lingua[k] == L'-' ? '_' : (char)lingua[k];
        wsprintfA(loc, "%s@calendar=%s", l, chiave);
    }
    UErrorCode st = 0;
    UCalendar* c = u.apri(nullptr, -1, loc, 0 /* UCAL_TRADITIONAL: quello del locale, cioe' @calendar */, &st);   // ora attuale, fuso del sistema
    if (!c || st > 0) { if (c) u.chiudi(c); return r; }
    st = 0;
    if (std::isfinite(istante)) u.impostaMillis(c, istante, &st);
    const UDate adesso = u.millis(c, &st);
    r.oggi = u.leggi(c, UCAL_DATE, &st);
    u.imposta(c, UCAL_DATE, 1);
    u.imposta(c, UCAL_HOUR_OF_DAY, 12);   // mezzogiorno: niente sorprese col cambio dell'ora a mezzanotte
    if (offset) { u.aggiungi(c, UCAL_MONTH, offset, &st); r.oggi = 0; }
    const UDate vista = u.millis(c, &st);
    const int dow = u.leggi(c, UCAL_DAY_OF_WEEK, &st);   // 1 = domenica ... 7 = sabato
    r.giorni = u.limite(c, UCAL_DATE, UCAL_ACTUAL_MAXIMUM, &st);
    u.chiudi(c);
    if (st > 0 || dow < 1 || dow > 7 || r.giorni < 1 || r.giorni > 31 || (!offset && (r.oggi < 1 || r.oggi > r.giorni))) return r;
    r.lunediPrimo = (dow + 5) % 7;
    const bool zh = lingua[0] == L'z';
    r.titolo = dettaglio::formatta(u, loc, zh ? (conEra(calid) ? L"Gy年M月" : L"y年M月")
                                                           : (conEra(calid) ? L"LLLL y G" : L"LLLL y"), vista);
    // (stesso ordine della data lunga gregoriana del pannello: "giovedi' 19 novembre", "Thursday, November 19")
    const bool en = lingua[0] == L'e';
    r.dataLunga = dettaglio::formatta(u, loc, zh ? (conEra(calid) ? L"Gy年M月d日 EEEE" : L"y年M月d日 EEEE")
                                               : en ? (conEra(calid) ? L"EEEE, MMMM d, y G" : L"EEEE, MMMM d, y")
                                                    : (conEra(calid) ? L"EEEE d MMMM y G" : L"EEEE d MMMM y"), adesso);
    r.ok = !r.titolo.empty();
    return r;
}

// Il mese di oggi nel calendario dell'utente, rifatto solo quando cambiano giorno, calendario o lingua (il pannello si
// ridisegna a ogni fotogramma delle animazioni)
inline const Mese& meseUtente(const wchar_t* lingua) {
    static Mese m;
    static int calidC = -1;
    static std::wstring linguaC;
    static WORD annoC = 0, meseC = 0, giornoC = 0;
    SYSTEMTIME t; GetLocalTime(&t);
    const int calid = tipoUtente();
    if (calid != calidC || linguaC != lingua || t.wYear != annoC || t.wMonth != meseC || t.wDay != giornoC) {
        m = costruisci(calid, lingua);
        calidC = calid; linguaC = lingua; annoC = t.wYear; meseC = t.wMonth; giornoC = t.wDay;
    }
    return m;
}

}  // namespace calendario
