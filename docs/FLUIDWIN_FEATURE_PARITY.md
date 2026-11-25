# TLauncher v1.0.3 — FluidWin parity ledger

Upstream reference: v1.3.1 / `2f773a8733b84c7dbee88fff6628d14e9ac802b9`. The code license is MIT (Copyright © 2026 Clooudy); existing TLauncher code and assets retain their respective notices. The matrix describes the v1.0.3 work, not whether a similar feature already exists in v1.0.2.

| Feature | Upstream source | TLauncher owner / integration | Implementation | CI | Owner | Resource and compatibility |
| --- | --- | --- | --- | --- | --- | --- |
| Genie / Scale / Drop, Dock target, reversal | genio.h; fluidwin.cpp griglia/curvaRiduzione/curvaRipristino | dock::window_effect, Stage WGC | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Bounded proxy; protected and incompatible windows use Windows |
| Open, close, maximize, restore, Jelly and live contents | fluidwin.cpp; genio.h; viva.h; stampa.h | dock window motion and tracking | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Transient frame/mesh; verified state and live capture required |
| Shared D3D and factories, pool and cleanup | gpu.h; fabbriche.h; pool.h | Dock renderer, Stage capture and native text; separate WGL wallpaper | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Share when thread and adapter ownership match |
| Guardian and emergency restore | fluidwin.cpp guardiano; altrui.h | dock::taskbar_guard and state recovery | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | One helper, original state, identity and timeout |
| Dock multi-window previews and selection | anteprime.h; dock.h | dock::previews and stage_snapshot | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | On demand; minimized capture depends on cached frame |
| Dock running indicators and identity; new window/hidden apps/pins/commands/icons | dock.h | Dock identities, icon cache and shell commands | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | No duplicate cache or catalog |
| Jump Lists, recent files and commands | jumplist.h | Dock app menu | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Shell identity availability |
| Downloads and Recycle Bin, drag/drop | dock.h; pila.h | Dock stacks | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | On-demand listing and Shell recycle semantics |
| Show Desktop and restoration; Dock follow-display/top | dock.h; fluidwin.cpp | Dock multi-display manager | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Preserve existing independent layouts |
| Stage Peek and existing Stage Manager | anteprime.h; viva.h | dock::stage_rail; nonactivating preview | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | TLauncher extension; no hidden restore to acquire pixels |
| Alt+Tab Overview | panoramica.h | Dock window model and coordinated key handling | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Bounded thumbnails; Windows default remains available |
| Snap layouts, drag/keys/maximize controls | affianca.h; fluidwin.cpp | Dock work areas and window controller | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Native fallback; don't change global snap settings |
| Desktop and Explorer context menus | destro.h; menu.h; condividi.h | dock glass menu and shell access | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Preserve native shell extensions and UAC |
| Indexed app/file/folder/settings search | ricerca.h; indice.h; launcher.h | Unified Spotlight and existing application catalog | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Bounded results; canceled stale queries |
| Quick Math, edge wave, uninstall | ricerca.h; onda.h; launcher.h | Spotlight query and app management | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | No command eval; user-confirmed Windows uninstall |
| System controls, Wi-Fi, Bluetooth, battery, timers, silence | centro.h; wifi.h; sistema.h; comodita.h | Existing Control Center and stats | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | On-demand device work |
| Output selection and per-app mixer | mixer.h | Control Center Core Audio owner | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Endpoint/session lifetime handling |
| Media controls and art | musica.h | Control Center media panel | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Active sessions only |
| Clipboard history | appunti.h | Opt-in Control Center history | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Bounded memory, no logs or network |
| Calendar, weather and RSS | calendario.h; centro.h; notch.h | Existing calendar and weather plus on-demand RSS | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | No duplicate weather service |
| Notifications, banners, badges and history | notifiche.h; banner.h | Single notification owner across Dock and Control Center | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Windows permission and policy limits |
| Custom cursors and import | cursori.h | Settings and Guardian | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Archive checks and exact original state |
| Always on Top, appearance and device indicators | fluidwin.cpp; raggio.h; sensori.h; osd.h | Window owner and Control Center | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Release only app-owned state |
| File Shelf | notch.h | Dock/Control Center shelf | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Paths only; no file duplication |
| Static wallpaper audit and diagnostics | sfondi.h; fluidwin.cpp | Wallpaper and diagnostics owners | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | No unlicensed assets or personal data |
| Settings and translations | impostazioni.h; testi.h; traduzioni.h; vetro.h | Current Settings owner; assess native parity | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | WebView2 stays until replacement matches accessibility and functionality |
| Dynamic Notch | notch.h | Excluded; independent utilities above remain in scope | EXCLUDED_BY_OWNER | — | — | — |
| Auto Update | aggiorna.h | Excluded | EXCLUDED_BY_OWNER | — | — | — |
