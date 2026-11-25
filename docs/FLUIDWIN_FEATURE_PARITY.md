# TLauncher v1.0.3 — FluidWin parity ledger

Upstream reference: v1.3.1 / `2f773a8733b84c7dbee88fff6628d14e9ac802b9`. The code license is MIT (Copyright © 2026 Clooudy); existing TLauncher code and assets retain their respective notices. The matrix describes the v1.0.3 work, not whether a similar feature already exists in v1.0.2.

| Feature | Upstream source | TLauncher owner / integration | Implementation | CI | Owner | Resource and compatibility |
| --- | --- | --- | --- | --- | --- | --- |
| Genie minimize/restore from Dock | genio.h griglia/SHADER; fluidwin.cpp curvaGenio/curvaRipristino | dock::window_effect, Stage WGC | IMPLEMENTED (experimental) | VERIFIED_CI (owned fixture rendered source pixels; full Dock/foreign-window lifecycle not verified) | NOT_VERIFIED | Bounded proxy; native overlap, protected and incompatible windows use Windows |
| Scale minimize/restore from Dock | scala.h is DPI support, not an effect | Existing dock::window_effect | IMPLEMENTED (existing experimental path retained) | VERIFIED_CI (two synthetic foreign HWNDs, capture timeout, placement, 100 proxy cycles) | NOT_VERIFIED | Same native overlap rule |
| Drop minimize/restore from Dock | fluidwin.cpp elementoRiduzione/cadutaChiude/cadutaApre | dock::window_effect and the same FluidWin mesh renderer | IMPLEMENTED (experimental) | VERIFIED_CI (owned fixture source and intermediate pixels) | NOT_VERIFIED | Bounded target path; Dock bounce not yet integrated |
| Interruption and reversal | fluidwin.cpp invertiTempo/inversaRipristino | Dock transition controller reverse from last submitted mesh geometry | IMPLEMENTED (Dock minimize → restore) | VERIFIED_CI (inverse geometry; full gestures and interruption coverage pending) | NOT_VERIFIED | Keep proxy on same Dock; fallback on identity/placement changes |
| Open, close, maximize, restore, Jelly and live contents | fluidwin.cpp; genio.h; viva.h; stampa.h | dock window motion and tracking | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Transient frame/mesh; verified state and live capture required |
| Shared D3D and factories, pool and cleanup | gpu.h; fabbriche.h; pool.h | shell_gpu owns one lazy native shell device; Dock, Stage and Spotlight hold leases; WGL wallpaper remains separate | IMPLEMENTED (D3D device); other consolidation pending | NOT_VERIFIED | NOT_VERIFIED | Protect immediate context; no sharing across incompatible WGL interop |
| Guardian and emergency restore | fluidwin.cpp guardiano; altrui.h | dock::taskbar_guard and state recovery | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | One helper, original state, identity and timeout |
| Dock multi-window previews and selection | anteprime.h; dock.h | dock::previews and stage_snapshot | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | On demand; minimized capture depends on cached frame |
| Dock running indicators and identity; new window/hidden apps/pins/commands/icons | dock.h | Dock identities, icon cache and shell commands | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | No duplicate cache or catalog |
| Jump Lists, recent files and commands | jumplist.h | Dock app menu | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Shell identity availability |
| Downloads and Recycle Bin, drag/drop | dock.h; pila.h | Dock stacks | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | On-demand listing and Shell recycle semantics |
| Show Desktop and restoration; Dock follow-display/top | dock.h; fluidwin.cpp | Dock multi-display manager | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Preserve existing independent layouts |
| Stage Peek and existing Stage Manager | anteprime.h; viva.h (patterns only) | dock::stage_peek with DWM and existing Stage cache, live Settings toggle and delay | IMPLEMENTED | NOT_VERIFIED | NOT_VERIFIED | TLauncher extension; cached fallback for permitted minimized windows |
| Alt+Tab Overview | panoramica.h | Dock window model and coordinated key handling | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Bounded thumbnails; Windows default remains available |
| Snap layouts, drag/keys/maximize controls | affianca.h; fluidwin.cpp | Dock work areas and window controller | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Native fallback; don't change global snap settings |
| Desktop and Explorer context menus | destro.h; menu.h; condividi.h | dock glass menu and shell access | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Preserve native shell extensions and UAC |
| Indexed file/folder search | indice.h cerca | Both Spotlight modes and shared query worker; current catalog fallback remains | IMPLEMENTED | VERIFIED_CI (build only; actual service/index pending) | NOT_VERIFIED | Windows Search availability, maximum 48 indexed matches per query, no duplicate local index |
| App/settings search | ricerca.h; launcher.h | Existing application catalog and system shortcuts | NOT_VERIFIED (upstream additions) | NOT_VERIFIED | NOT_VERIFIED | Preserve currently supported rows and app identities |
| Quick Math | ricerca.h Calcolo | Both Spotlight modes; explicit clipboard copy | IMPLEMENTED | VERIFIED_CI (parser/grammar/limits) | NOT_VERIFIED | No shell evaluation; maximum 200 ASCII bytes; results not persisted |
| Spotlight edge wave and app uninstall | onda.h; launcher.h | Spotlight UI and app management | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | User-confirmed Windows uninstall required |
| System controls, Wi-Fi, Bluetooth, battery, timers, silence | centro.h; wifi.h; sistema.h; comodita.h | Existing Control Center and stats | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | On-demand device work |
| Output selection and per-app mixer | mixer.h | Control Center Core Audio owner | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Endpoint/session lifetime handling |
| Media controls and art | musica.h | Control Center media panel | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Active sessions only |
| Clipboard history | appunti.h Cronologia | Opt-in Control Center text and supported DIB clipboard history | IMPLEMENTED | NOT_VERIFIED | NOT_VERIFIED | 4 MiB global cap, 5–40 entries; honors Windows privacy flags; clears on exit, lock, or disabling |
| Calendar, weather and RSS | calendario.h; centro.h; notch.h | Existing calendar and weather plus on-demand RSS | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | No duplicate weather service |
| Notifications, banners, badges and history | notifiche.h; banner.h | Single notification owner across Dock and Control Center | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Windows permission and policy limits |
| Custom cursors and import | cursori.h | Settings and Guardian | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Archive checks and exact original state |
| Always on Top, appearance and device indicators | fluidwin.cpp; raggio.h; sensori.h; osd.h | Window owner and Control Center | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Release only app-owned state |
| File Shelf | notch.h | Dock/Control Center shelf | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | Paths only; no file duplication |
| Static wallpaper audit and diagnostics | sfondi.h; fluidwin.cpp | Wallpaper and diagnostics owners | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | No unlicensed assets or personal data |
| Settings and translations | impostazioni.h; testi.h; traduzioni.h; vetro.h | Current Settings owner; assess native parity | NOT_VERIFIED | NOT_VERIFIED | NOT_VERIFIED | WebView2 stays until replacement matches accessibility and functionality |
| Dynamic Notch | notch.h | Excluded; independent utilities above remain in scope | EXCLUDED_BY_OWNER | — | — | — |
| Auto Update | aggiorna.h | Excluded | EXCLUDED_BY_OWNER | — | — | — |
