// Lavori brevi nel pool di thread di Windows invece di un thread nuovo ogni volta (foto delle finestre, finestre
// appena nate, lavori del dock): prima a riposo c'erano ~150 thread. Un lavoro che puo' restare fermo a lungo (una
// foto di un'app che non risponde) lo dice al pool, che ne aggiunge altri se servono.
#pragma once
#include "tlauncher_identity.h"
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

namespace pool {

// f(p) nel pool, con la priorita' indicata (rimessa com'era alla fine). false = non partito (allora niente f)
inline bool lancia(DWORD (WINAPI* f)(LPVOID), void* p, int priorita = THREAD_PRIORITY_NORMAL, bool lungo = false) {
    struct Lavoro { DWORD (WINAPI* f)(LPVOID); void* p; int priorita; bool lungo; };
    Lavoro* l = new Lavoro{f, p, priorita, lungo};
    const BOOL ok = TrySubmitThreadpoolCallback([](PTP_CALLBACK_INSTANCE ist, PVOID x) {
        Lavoro* l = (Lavoro*)x;
        if (l->lungo) CallbackMayRunLong(ist);
        const HANDLE t = GetCurrentThread();
        const int prima = GetThreadPriority(t);
        if (l->priorita != prima) SetThreadPriority(t, l->priorita);
        l->f(l->p);
        if (l->priorita != prima) SetThreadPriority(t, prima);
        delete l;
    }, l, nullptr);
    if (!ok) delete l;
    return ok != FALSE;
}

}  // namespace pool

// Oggetti consegnati da un thread di lavoro alla UI con PostMessage(..., lParam = puntatore). Chi posta li registra,
// chi riceve li riprende: un messaggio con un puntatore mai consegnato (un altro programma che posta lo stesso numero
// di messaggio, o un duplicato) viene ignorato invece di usare e liberare un indirizzo qualsiasi (R20261121-001416-S1,
// R20261121-002419-S2, R20261121-002818-S1 e S2)
// Ogni puntatore e' legato alla finestra e al numero di messaggio con cui e' partito: un oggetto in viaggio verso un
// altro destinatario (o con un altro tipo) non passa il controllo di chi lo riceve (R20261121-115814-S1 conferma).
#include <mutex>
#include <new>
#include <unordered_map>
namespace consegna {
struct Destino { HWND h; UINT msg; };
struct Registro { std::mutex m; std::unordered_map<const void*, Destino> s; };
inline Registro& registro() { static Registro* r = new Registro; return *r; }   // (mai distrutto: thread tardivi)
// nel destinatario: true se p era stato consegnato davvero a h con msg (e da ora non lo e' piu')
inline bool riprendi(HWND h, UINT msg, const void* p) {
    if (!p) return false;
    std::lock_guard<std::mutex> g(registro().m);
    auto it = registro().s.find(p);
    if (it == registro().s.end() || it->second.h != h || it->second.msg != msg) return false;
    registro().s.erase(it);
    return true;
}
// false = non consegnato (PostMessage fallito o memoria finita per il registro): allora libera chi posta
inline bool posta(HWND h, UINT msg, WPARAM w, const void* p) {
    if (!p) return false;
    try { std::lock_guard<std::mutex> g(registro().m); registro().s[p] = Destino{h, msg}; }
    catch (const std::bad_alloc&) { return false; }   // (R20261121-115445-S5)
    if (PostMessageW(h, msg, w, (LPARAM)p)) return true;
    riprendi(h, msg, p);
    return false;
}
}  // namespace consegna
