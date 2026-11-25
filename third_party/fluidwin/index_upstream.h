// Copyright (c) 2026 Clooudy. MIT; see LICENSE.
// Adapted from indice.h, FluidWin v1.3.1 (2f773a8733b84c7dbee88fff6628d14e9ac802b9).
// File e cartelle dall'indice di ricerca di Windows (SearchFolderItemFactory, la stessa base della ricerca di Esplora
// file). Usato dalla ricerca rapida (ricerca.h) e dal launcher (launcher.h, issue #14: "la ricerca mostra solo le app").
// Si chiama da un thread di lavoro con COM inizializzato: l'indice puo' metterci qualche centinaio di millisecondi.
// Misure sul PC di sviluppo (19/11/2026): 30-220 ms; l'indice restituisce anche le cartelle. Con lo scopo su tutto
// l'indice (non solo la cartella dell'utente) si aggiungono i luoghi che l'utente ha indicizzato, e le voci del menu
// Start di ProgramData: quelle si tolgono, perche' sono le app che il launcher e la ricerca mostrano gia'.
#pragma once
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <shlobj.h>
#include <shobjidl.h>
#include <structuredquery.h>
#include <propsys.h>
#include <string>
#include <vector>
#include <cwctype>
#include <cwchar>

namespace indice {

struct File {
    std::wstring nome;       // nome da mostrare
    std::wstring cartella;   // cartella che lo contiene, con ~ al posto della cartella dell'utente
    std::wstring percorso;   // percorso completo, per aprirlo
    bool eCartella = false;
};

// Cose che non si cercano: cartelle di sistema e nascoste (AppData, .cache, .vscode...) e il menu Start (le app)
inline bool daSaltare(std::wstring p) {
    for (auto& c : p) c = (wchar_t)towlower(c);
    return p.find(L"\\appdata\\") != std::wstring::npos || p.find(L"\\.") != std::wstring::npos ||
           p.find(L"\\start menu\\") != std::wstring::npos;
}

// Al massimo quanti file e cartelle il cui nome contiene q. tuttoIndice: tutti i luoghi indicizzati; altrimenti solo
// la cartella dell'utente. Vuoto se l'indice non c'e' o non risponde
inline std::vector<File> cerca(const std::wstring& q, int quanti, bool tuttoIndice) {
    std::vector<File> out;
    if (q.empty() || quanti <= 0) return out;
    static const CLSID CLSID_Fabbrica = {0x14010e02, 0xbbbd, 0x41f0, {0x88, 0xe3, 0xed, 0xa3, 0x71, 0x21, 0x65, 0x84}};   // SearchFolderItemFactory
    static const IID IID_Fabbrica = {0xa0ffbc28, 0x5482, 0x4366, {0xbe, 0x27, 0x3e, 0x81, 0xe7, 0x8e, 0x06, 0xc2}};
    static const CLSID CLSID_Condizioni = {0xe03e85b0, 0x7be3, 0x4000, {0xba, 0x98, 0x6c, 0x13, 0xde, 0x9f, 0xa4, 0x86}};   // ConditionFactory
    static const IID IID_Condizioni2 = {0x71d222e1, 0x432f, 0x429e, {0x8c, 0x13, 0xb6, 0xda, 0xfd, 0xe5, 0x07, 0x7a}};
    static const IID IID_Condizione = {0x0fc988d4, 0xc935, 0x4b97, {0xa9, 0x73, 0x46, 0x28, 0x2e, 0xa1, 0x75, 0xc8}};
    static const PROPERTYKEY NOME = {{0xb725f130, 0x47ef, 0x101a, {0xa5, 0xf1, 0x02, 0x60, 0x8c, 0x9e, 0xeb, 0xac}}, 10};   // ItemNameDisplay
    ISearchFolderItemFactory* sf = nullptr; IUnknown* cf = nullptr; ICondition* cond = nullptr;
    if (FAILED(CoCreateInstance(CLSID_Fabbrica, nullptr, CLSCTX_INPROC_SERVER, IID_Fabbrica, (void**)&sf))) return out;
    if (SUCCEEDED(CoCreateInstance(CLSID_Condizioni, nullptr, CLSCTX_INPROC_SERVER, IID_Condizioni2, (void**)&cf)) &&
        SUCCEEDED(static_cast<IConditionFactory2*>(cf)->CreateStringLeaf(NOME, COP_VALUE_CONTAINS, q.c_str(), nullptr, CONDITION_CREATION_DEFAULT, IID_PPV_ARGS(&cond))) && cond) {
        sf->SetDisplayName(L"TLauncher");
        sf->SetCondition(cond);
        if (!tuttoIndice) {
            PIDLIST_ABSOLUTE profilo = nullptr;
            if (SUCCEEDED(SHGetKnownFolderIDList(FOLDERID_Profile, 0, nullptr, &profilo))) {
                IShellItemArray* scopo = nullptr;
                if (SUCCEEDED(SHCreateShellItemArrayFromIDLists(1, (PCIDLIST_ABSOLUTE_ARRAY)&profilo, &scopo))) { sf->SetScope(scopo); scopo->Release(); }
                CoTaskMemFree(profilo);
            }
        }
        wchar_t prof[MAX_PATH] = L"";
        const DWORD np = ExpandEnvironmentStringsW(L"%USERPROFILE%", prof, MAX_PATH);
        const size_t lp = np && np <= MAX_PATH ? wcslen(prof) : 0;
        IShellItem* ris = nullptr;
        if (SUCCEEDED(sf->GetShellItem(IID_PPV_ARGS(&ris)))) {
            IEnumShellItems* en = nullptr;
            if (SUCCEEDED(ris->BindToHandler(nullptr, BHID_EnumItems, IID_PPV_ARGS(&en)))) {
                IShellItem* it = nullptr;
                int visti = 0;
                // (al massimo 200 voci guardate: con molte voci da saltare non si scorre tutto l'indice)
                while ((int)out.size() < quanti && visti++ < 200 && en->Next(1, &it, nullptr) == S_OK) {
                    LPWSTR n = nullptr, perc = nullptr;
                    if (SUCCEEDED(it->GetDisplayName(SIGDN_NORMALDISPLAY, &n)) && SUCCEEDED(it->GetDisplayName(SIGDN_FILESYSPATH, &perc)) && n && perc) {
                        const std::wstring p = perc;
                        if (!daSaltare(p)) {
                            std::wstring cartella = p.substr(0, p.find_last_of(L'\\'));
                            if (lp && cartella.size() >= lp && _wcsnicmp(cartella.c_str(), prof, lp) == 0) cartella = L"~" + cartella.substr(lp);
                            SFGAOF a = 0;
                            it->GetAttributes(SFGAO_FOLDER | SFGAO_STREAM, &a);
                            out.push_back({n, cartella, p, (a & SFGAO_FOLDER) && !(a & SFGAO_STREAM)});
                        }
                    }
                    CoTaskMemFree(n); CoTaskMemFree(perc); it->Release();
                }
                en->Release();
            }
            ris->Release();
        }
    }
    if (cond) cond->Release();
    if (cf) cf->Release();
    sf->Release();
    return out;
}

}  // namespace indice
