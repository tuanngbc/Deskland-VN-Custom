# TLauncher v1.0.3 — FluidWin parity ledger

**Correction 2026-11-26:** Owner tested and rejected both `442861d` and `f3bf1f5`. Motion quality FAIL, colored borders UNRESOLVED, full feature parity INCOMPLETE. The older CI observations below remain historical evidence, not acceptance. Current implementation work and hosted run results are recorded in `FLUIDWIN_CORRECTION_EXECUTION.md`. No new comprehensive candidate is approved for owner handoff.

`NOT_STARTED` in Implementation describes missing FluidWin integration, not the absence of every similar pre-existing TLauncher feature. `IMPLEMENTED` means a real runtime route exists; it does not imply `VERIFIED_CI` or `VERIFIED_OWNER`. `BLOCKED_WITH_EVIDENCE` must cite an observed failure and attempted remedy. Only Dynamic Notch and Auto Update are `EXCLUDED_BY_OWNER`. For delivered motion, the owner evidence is **REJECTED**, superseding the older NOT_VERIFIED labels.

Upstream reference: v1.3.1 / `2f773a8733b84c7dbee88fff6628d14e9ac802b9`. The code license is MIT (Copyright © 2026 Clooudy); existing TLauncher code and assets retain their respective notices. The matrix describes the v1.0.3 work, not whether a similar feature already exists in v1.0.2.

Intermediate candidate: `442861d7ac77beaf247c7df3123bc92cef17ef70`, [Windows x64 Actions run 45524299059](https://github.com/tuanngbc/TLauncher/actions/runs/45524299059). Verified CI entries describe the listed runner coverage only. Owner UAT and release are pending.

| Feature | Upstream source | TLauncher owner / integration | Implementation | CI | Owner | Resource and compatibility |
| --- | --- | --- | --- | --- | --- | --- |
| Genie minimize/restore from Dock | genio.h griglia/SHADER; fluidwin.cpp curvaGenio/curvaRipristino | dock::window_effect, Stage WGC | IMPLEMENTED (experimental) | VERIFIED_CI (source and changed intermediate screen pixels; real Dock action on a separate fixture process) | NOT_VERIFIED | Bounded proxy; native overlap, protected and incompatible windows use Windows |
| Scale minimize/restore from Dock | scala.h is DPI support, not an effect | Existing dock::window_effect | IMPLEMENTED (existing experimental path retained) | VERIFIED_CI (two synthetic foreign HWNDs, capture timeout, placement, 100 proxy cycles) | NOT_VERIFIED | Same native overlap rule |
| Drop minimize/restore from Dock | fluidwin.cpp elementoRiduzione/cadutaChiude/cadutaApre | dock::window_effect and the same FluidWin mesh renderer | IMPLEMENTED (experimental) | VERIFIED_CI (source and changed intermediate screen pixels; real Dock action on a separate fixture process) | NOT_VERIFIED | Bounded target path; Dock bounce not yet integrated |
| Interruption and reversal | fluidwin.cpp invertiTempo/inversaRipristino | Dock transition controller reverse from last submitted mesh geometry | IMPLEMENTED (Dock minimize → restore) | VERIFIED_CI (Genie and Drop reverse the running transition through Dock activate, reuse the proxy, and preserve placement) | NOT_VERIFIED | Keep proxy on same Dock; fallback on identity/placement changes |
| Open, close, maximize, restore, Jelly and live contents | fluidwin.cpp; genio.h; viva.h; stampa.h | dock window motion and tracking | NOT_STARTED | NOT_STARTED | NOT_STARTED | Transient frame/mesh; verified state and live capture required |
| Shared D3D and factories, pool and cleanup | gpu.h; fabbriche.h; pool.h | shell_gpu owns one lazy native shell device; Dock, Stage and Spotlight hold leases; WGL wallpaper remains separate | IMPLEMENTED (D3D device); other consolidation pending | VERIFIED_CI (one device identity across UI/capture threads and no idle owner after releasing both leases) | NOT_VERIFIED | Protect immediate context; no sharing across incompatible WGL interop |
| Guardian and emergency restore | fluidwin.cpp guardiano; altrui.h | dock::taskbar_guard and state recovery | NOT_STARTED | NOT_STARTED | NOT_STARTED | One helper, original state, identity and timeout |
| Dock multi-window previews and selection | anteprime.h; dock.h | dock::previews and stage_snapshot | NOT_STARTED | NOT_STARTED | NOT_STARTED | On demand; minimized capture depends on cached frame |
| Dock running indicators and identity; new window/hidden apps/pins/commands/icons | dock.h | Dock identities, icon cache and shell commands | NOT_STARTED | NOT_STARTED | NOT_STARTED | No duplicate cache or catalog |
| Jump Lists, recent files and commands | jumplist.h | Dock app menu | NOT_STARTED | NOT_STARTED | NOT_STARTED | Shell identity availability |
| Downloads and Recycle Bin, drag/drop | dock.h; pila.h | Dock stacks | NOT_STARTED | NOT_STARTED | NOT_STARTED | On-demand listing and Shell recycle semantics |
| Show Desktop and restoration; Dock follow-display/top | dock.h; fluidwin.cpp | Dock multi-display manager | NOT_STARTED | NOT_STARTED | NOT_STARTED | Preserve existing independent layouts |
| Stage Peek and existing Stage Manager | anteprime.h; viva.h (patterns only) | dock::stage_peek with DWM and existing Stage cache, live Settings toggle and delay | IMPLEMENTED | VERIFIED_CI (live thumbnail with no activation or minimize and with cleanup) | NOT_VERIFIED | TLauncher extension; cached fallback for permitted minimized windows |
| Alt+Tab Overview | panoramica.h | Dock window model and coordinated key handling | NOT_STARTED | NOT_STARTED | NOT_STARTED | Bounded thumbnails; Windows default remains available |
| Snap layouts, drag/keys/maximize controls | affianca.h; fluidwin.cpp | Dock work areas and window controller | NOT_STARTED | NOT_STARTED | NOT_STARTED | Native fallback; don't change global snap settings |
| Desktop and Explorer context menus | destro.h; menu.h; condividi.h | original C++ modules via shell_menu bridge and shared win_key input hook | IMPLEMENTED (integration; fidelity gaps below) | NOT_STARTED (new CI pending) | NOT_STARTED | Shift bypass and native fallback; owner-drawn extensions and localization pending |
| Indexed file/folder search | indice.h cerca | Both Spotlight modes and shared query worker; current catalog fallback remains | IMPLEMENTED | VERIFIED_CI (build only; actual service/index pending) | NOT_VERIFIED | Windows Search availability, maximum 48 indexed matches per query, no duplicate local index |
| App/settings search | ricerca.h; launcher.h | Existing application catalog and system shortcuts | NOT_STARTED (upstream additions) | NOT_VERIFIED | NOT_VERIFIED | Preserve currently supported rows and app identities |
| Quick Math | ricerca.h Calcolo | Both Spotlight modes; explicit clipboard copy | IMPLEMENTED | VERIFIED_CI (parser/grammar/limits) | NOT_VERIFIED | No shell evaluation; maximum 200 ASCII bytes; results not persisted |
| Spotlight edge wave and app uninstall | onda.h; launcher.h | Spotlight UI and app management | NOT_STARTED | NOT_STARTED | NOT_STARTED | User-confirmed Windows uninstall required |
| System controls, Wi-Fi, Bluetooth, battery, timers, silence | centro.h; wifi.h; sistema.h; comodita.h | Existing Control Center and stats | NOT_STARTED | NOT_STARTED | NOT_STARTED | On-demand device work |
| Output selection and per-app mixer | mixer.h | Control Center Core Audio owner | NOT_STARTED | NOT_STARTED | NOT_STARTED | Endpoint/session lifetime handling |
| Media controls and art | musica.h | Control Center media panel | NOT_STARTED | NOT_STARTED | NOT_STARTED | Active sessions only |
| Clipboard history | appunti.h Cronologia | Opt-in Control Center text and supported DIB clipboard history | IMPLEMENTED | VERIFIED_CI (deduplication, count and byte budget; actual clipboard interaction needs owner UAT) | NOT_VERIFIED | 4 MiB global cap, 5–40 entries; honors Windows privacy flags; clears on exit, lock, or disabling |
| Calendar, weather and RSS | calendario.h; centro.h; notch.h | Existing calendar and weather plus on-demand RSS | NOT_STARTED | NOT_STARTED | NOT_STARTED | No duplicate weather service |
| Notifications, banners, badges and history | notifiche.h; banner.h | Single notification owner across Dock and Control Center | NOT_STARTED | NOT_STARTED | NOT_STARTED | Windows permission and policy limits |
| Custom cursors and import | cursori.h | Settings and Guardian | NOT_STARTED | NOT_STARTED | NOT_STARTED | Archive checks and exact original state |
| Always on Top, appearance and device indicators | fluidwin.cpp; raggio.h; sensori.h; osd.h | Window owner and Control Center | NOT_STARTED | NOT_STARTED | NOT_STARTED | Release only app-owned state |
| File Shelf | notch.h | Dock/Control Center shelf | NOT_STARTED | NOT_STARTED | NOT_STARTED | Paths only; no file duplication |
| Static wallpaper audit and diagnostics | sfondi.h; fluidwin.cpp | Wallpaper and diagnostics owners | NOT_STARTED | NOT_STARTED | NOT_STARTED | No unlicensed assets or personal data |
| Settings and translations | impostazioni.h; testi.h; traduzioni.h; vetro.h | Current Settings owner; assess native parity | NOT_STARTED | NOT_STARTED | NOT_STARTED | WebView2 stays until replacement matches accessibility and functionality |
| Dynamic Notch | notch.h | Excluded; independent utilities above remain in scope | EXCLUDED_BY_OWNER | — | — | — |
| Auto Update | aggiorna.h | Excluded | EXCLUDED_BY_OWNER | — | — | — |

## Runtime routes for correction work

| Integration | Trigger and Settings | Implementation and evidence | Remaining limits |
| --- | --- | --- | --- |
| Original FluidWin renderer baseline | Hosted workflow only; no application entry point | VERIFIED_CI: job in run 45733231963 built original reference and executed 66 full-canvas tint poses | Capture, hooks, Drop and Jelly are not covered by this first baseline probe; owner quality is not verified |
| Desktop / Explorer menus | Dock enabled; `dock.shellContextMenu`; normal right-click in recognized Shell views. Shift-right-click bypasses | IMPLEMENTED: exact `destro.h`, `menu.h`, `condividi.h` and dependencies; bridge dispatches actual Shell commands; one shared input hook. New compiled menu/submenu fixture pending | Full shell-extension message forwarding, Vietnamese/Chinese upstream-owned text, unified menu styling and teardown/resource evidence pending. Default off until owner opts in |
| Native-resolution motion capture | Existing minimize/restore Dock routes and motion selector | IMPLEMENTED: same WGC worker copies source texture GPU-to-GPU; native resolution, visible-frame bounds, dynamic adapter headroom; new complete-rendered-output fixture pending | Guardian / native-overlap gate still limits runtime paths; cache and minimized Peek integration need regression evidence |
| Borderless WGC request | Each existing WGC capture session | IMPLEMENTED: request `IsBorderRequired=false` as upstream does, recording request/result in capture timing | This is a candidate cause of colored borders, not a proven root cause or visual fix. OS permission/another capture client may keep the border visible |

The grouped rows still require decomposition into individual functions and routes before claiming full parity. In particular, existing Wi-Fi selection, calendar, weather, Stage and app organization must be audited separately from missing FluidWin additions. No feature is moved to 1.0.4.
