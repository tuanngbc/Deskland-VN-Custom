// Lingua dell'interfaccia di FluidWin, scelta nelle impostazioni (al primo avvio quella di Windows). Solo i testi che
// si vedono cambiano lingua; il codice resta in italiano.
// Uso: testi::t(L"Impostazioni", L"Settings"), letto al momento di disegnare: cambiando lingua basta ridisegnare.
// Italiano e inglese stanno nel codice; le altre lingue in traduzioni.h, cercate con il testo inglese. Un testo senza
// traduzione resta in inglese (con --log lo si legge nel registro, una volta per testo: vedi mancante).
// Arabo: si legge da destra a sinistra. I formati di testo prendono la direzione RTL ma restano allineati dove li ha
// messi il disegno (vedi allinea): l'interfaccia non si capovolge, si legge bene il testo.
#pragma once
#include "tlauncher_identity.h"
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <dwrite.h>
#include <atomic>
#include <cwchar>
#include <string_view>
#include <unordered_map>
#include <unordered_set>
#include <mutex>
#include <functional>
#include <string>

namespace testi {

enum Lingua { ITALIANO = 0, INGLESE = 1, SPAGNOLO = 2, FRANCESE = 3, TEDESCO = 4, PORTOGHESE = 5, ARABO = 6, N_LINGUE = 7 };

// atomica: la leggono anche i thread di lavoro (aggiornamenti, widget) mentre l'interfaccia la cambia (S12)
inline std::atomic<int>& lingua() { static std::atomic<int> l{ITALIANO}; return l; }
inline int attuale() { const int l = lingua(); return l >= 0 && l < N_LINGUE ? l : INGLESE; }
inline bool inglese() { return attuale() == INGLESE; }
inline bool italiano() { return attuale() == ITALIANO; }
inline bool rtl() { return attuale() == ARABO; }

// nome di ogni lingua nella lingua stessa (scelta nelle impostazioni)
inline const wchar_t* nome(int l) {
    static const wchar_t* n[N_LINGUE] = {L"Italiano", L"English", L"Español", L"Français", L"Deutsch", L"Português", L"العربية"};
    return l >= 0 && l < N_LINGUE ? n[l] : n[INGLESE];
}

// registro dei testi senza traduzione (lo collega fluidwin.cpp con --log)
inline std::function<void(const wchar_t*)>& registro() { static std::function<void(const wchar_t*)> r; return r; }

}  // namespace testi

#include "traduzioni.h"

namespace testi {

namespace dettagli {
// indice: testo inglese -> riga di traduzioni::RIGHE (fatto una volta, anche da un thread di lavoro: statica locale)
inline const std::unordered_map<std::wstring_view, int>& indice() {
    static const std::unordered_map<std::wstring_view, int> m = [] {
        std::unordered_map<std::wstring_view, int> x;
        x.reserve(sizeof(traduzioni::RIGHE) / sizeof(traduzioni::RIGHE[0]));
        for (int i = 0; i < (int)(sizeof(traduzioni::RIGHE) / sizeof(traduzioni::RIGHE[0])); i++) x.emplace(traduzioni::RIGHE[i][0], i);
        return x;
    }();
    return m;
}
inline void mancante(const wchar_t* en) {
    if (!registro()) return;
    static std::mutex m; static std::unordered_set<std::wstring> visti;
    {
        std::lock_guard<std::mutex> g(m);
        if (!visti.insert(en).second) return;
    }
    registro()(en);
}
}  // namespace dettagli

// il testo nella lingua scelta. Per le lingue in traduzioni.h il puntatore resta valido per sempre; senza traduzione
// torna en stesso (quindi vale finche' vale en)
inline const wchar_t* t(const wchar_t* it, const wchar_t* en) {
    const int l = attuale();
    if (l == ITALIANO) return it;
    if (l == INGLESE || !en || !*en) return en;
    if (it) for (const auto& r : traduzioni::PER_ITALIANO) if (!wcscmp(r[0], it)) return r[l - 1];
    const auto& m = dettagli::indice();
    const auto k = m.find(std::wstring_view(en));
    if (k == m.end()) { dettagli::mancante(en); return en; }
    const wchar_t* s = traduzioni::RIGHE[k->second][l - 1];
    return s && *s ? s : en;
}

// nome della lingua per date e orari (GetDateFormatEx). Arabo d'Egitto: calendario gregoriano (quello saudita e'
// l'egiriano, e le date non corrisponderebbero al resto di Windows)
inline const wchar_t* locale() {
    static const wchar_t* n[N_LINGUE] = {L"it-IT", L"en-US", L"es-ES", L"fr-FR", L"de-DE", L"pt-BR", L"ar-EG"};
    return n[attuale()];
}
// codice di due lettere (ricerca delle citta' del meteo)
inline const wchar_t* codice() {
    static const wchar_t* n[N_LINGUE] = {L"it", L"en", L"es", L"fr", L"de", L"pt", L"ar"};
    return n[attuale()];
}
// separatore dei decimali nei risultati dei calcoli
inline wchar_t virgola() { const int l = attuale(); return l == INGLESE || l == ARABO ? L'.' : L','; }

// lingua al primo avvio: quella di Windows se FluidWin la conosce, altrimenti inglese
inline int predefinita() {
    switch (PRIMARYLANGID(GetUserDefaultUILanguage())) {
        case LANG_ITALIAN: return ITALIANO;
        case LANG_SPANISH: return SPAGNOLO;
        case LANG_FRENCH: return FRANCESE;
        case LANG_GERMAN: return TEDESCO;
        case LANG_PORTUGUESE: return PORTOGHESE;
        case LANG_ARABIC: return ARABO;
        default: return INGLESE;
    }
}

// Testo dell'utente (una ricerca, un nome) dentro una frase: in arabo lo si isola (FSI ... PDI) cosi' l'ordine dei suoi
// caratteri non si mescola con quello della frase. Prima "7+5" dentro "Cerca “...” sul web" diventava "5+7"
// (i due caratteri come numeri: scritti nella stringa letterale GCC avvisa di un contesto bidirezionale non chiuso)
inline std::wstring isola(const std::wstring& s) {
    if (!rtl()) return s;
    // (LRE ... PDF: provato, gli isolati FSI ... PDI DirectWrite non li applica e "7+5" restava "5+7")
    std::wstring r(1, (wchar_t)0x202A);
    // i controlli di direzione dentro il testo (incollati) potrebbero chiudere l'incorporamento e capovolgere il resto
    // della frase: in questa copia, che serve solo a disegnare, si tolgono (LRE..RLO, LRI..PDI; R20261121-140048-S10)
    for (const wchar_t c : s) if (!((c >= 0x202A && c <= 0x202E) || (c >= 0x2066 && c <= 0x2069))) r += c;
    r += (wchar_t)0x202C;
    return r;
}

// Allineamento di un formato di testo, da usare al posto di SetTextAlignment. In arabo la direzione e' da destra a
// sinistra, e LEADING/TRAILING si scambiano: cosi' il testo resta dalla parte dove lo mette il disegno (pensato da
// sinistra a destra), ma la punteggiatura e i numeri dentro la frase seguono le regole dell'arabo
inline void allinea(IDWriteTextFormat* f, DWRITE_TEXT_ALIGNMENT al) {
    if (!f) return;
    if (rtl()) {
        f->SetReadingDirection(DWRITE_READING_DIRECTION_RIGHT_TO_LEFT);
        if (al == DWRITE_TEXT_ALIGNMENT_LEADING) al = DWRITE_TEXT_ALIGNMENT_TRAILING;
        else if (al == DWRITE_TEXT_ALIGNMENT_TRAILING) al = DWRITE_TEXT_ALIGNMENT_LEADING;
    }
    f->SetTextAlignment(al);
}

}  // namespace testi
