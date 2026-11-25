#include "index_upstream.h"

extern "C" void tlauncher_fluidwin_index(const wchar_t* query, void* user,
    void (*row)(void*, const wchar_t*, const wchar_t*, const wchar_t*, int)) {
    if (!query || !row) return;
    for (const auto& file : indice::cerca(query, 48, true)) {
        row(user, file.nome.c_str(), file.cartella.c_str(), file.percorso.c_str(), int(file.eCartella));
    }
}
