# FluidWin integration audit

Internal continuation through `b9424e4`: original static/composition refusal, independent battery and event-driven Bluetooth headphone OSD scopes passed full hosted runs `45827021105`, `45830967493` and `45831194360`; exact source/package/PE 1.0.3 and checksums were retrieved. Strong headphone callback cancellation/admission and all three actual locale images are verified; runner had zero active render endpoints, so physical Bluetooth recognition/battery properties remain unverified. Notification access diagnosis observed successful GetAccessStatus with Denied (operation 7, HRESULT S_OK, access 2), not a package-identity error; no owner access conclusion follows. Original illustrated Weather card is VERIFIED_CI in full run 45836234845 / 8a41be3 with all five actual forecast glyphs in vi/en/zh; exact package verified and images inspected. Original inline city search/unified Control Center remain incomplete. Global physical volume keys are implemented through the existing hook/utilities permit with hosted verification pending. These scopes do not resolve owner border/pacing, full titlebar/lifecycle, badges/banners/cursors or overall acceptance.

Reference: [FluidWin v1.3.1](https://github.com/clooudfn/FluidWin/tree/2f773a8733b84c7dbee88fff6628d14e9ac802b9), peeled tag commit `2f773a8733b84c7dbee88fff6628d14e9ac802b9`. TLauncher baseline: `e9fa8ff52e773a2d5ff9b383af6973ca7281103a` (stable application code and updated showcase). The owner's successful run is evidence for the standalone upstream on this hardware; candidate evidence is collected separately.

## Source boundaries

| Upstream source | Ownership and reuse decision |
| --- | --- |
| `fluidwin.cpp`, `altrui.h` | Window events, foreign calls, backing snapshots, transition state and Guardian. Reuse timing/geometry independently from hooks and foreign mutation; guardian work extends the existing recovery owner. Audit HWND ownership, integrity, hung clients and recoverability before enabling mutation. |
| `genio.h`, `gpu.h`, `fabbriche.h`, `viva.h`, `stampa.h`, `pool.h`, `scala.h`, `raggio.h` | Genie strip/rounding shader, D3D11/DComp, WGC, PrintWindow and worker pool, DPI/corner helpers. The scale header is DPI utilities. Feed the existing TLauncher Stage capture, avoid a second frame pool; own swap chain on Dock thread and retain existing WGL wallpaper interop. |
| `dock.h`, `anteprime.h`, `jumplist.h`, `pila.h`, `widget.h` | Interactive Dock with nonactivating previews, per-app commands, stack/drag interactions and widget drawing. Reuse behavior with existing Dock item identities, layout and capture. Audit shell command support and cache bounds. |
| `panoramica.h`, `affianca.h`, `destro.h`, `menu.h`, `condividi.h` | Overview, snapping and shell context menu. Requires existing input and tracking coordination; preserve native behavior when an event or shell verb cannot be reproduced. |
| `launcher.h`, `ricerca.h`, `indice.h`, `onda.h` | Native app discovery and search, index worker, overlay edge shader. Reuse query/graphics algorithms; no independent catalog or index service. |
| `centro.h`, `mixer.h`, `musica.h`, `wifi.h`, `sistema.h`, `comodita.h`, `calendario.h` | Control Center configuration, Core Audio, media, WLAN, device/system controls, battery/calendar. Existing Control Center, weather and system stats remain owners. Run device work on demand; keep user sessions and Windows-owned state explicit. |
| `appunti.h`, `notifiche.h`, `banner.h`, `sensori.h`, `osd.h`, `suggerimento.h` | Clipboard, notifications, banners, hardware indicators, feedback. Capture is opt-in, retention bounded, no personal-data logging. Permission or policy failures must be visible. |
| `cursori.h`, `sfondi.h` | Cursor packs and shell wallpaper. Require provenance and exact rollback for cursor changes. TLauncher dynamic wallpaper remains the owner; no upstream media imported without asset provenance. |
| `impostazioni.h`, `testi.h`, `traduzioni.h`, `vetro.h` | Native settings, translation and glass drawing. Settings replacement requires keyboard, accessibility, localization and parity evidence. Maintain the existing Settings coordinator and configuration schema. |
| `notch.h`, `aggiorna.h` | Exclude Notch and updater. Audit independent shelf/media/timer/device widgets for reuse outside the Notch. |
| `compila.ps1`, `icona.*`, `sfondi/*`, `docs/gif/*` | Upstream GNU build uses D3D/DXGI/DComp, D2D/DWrite, shell, COM/WinRT, WIC and system libraries. Keep the current Actions/MSVC pipeline, and do not import logos, wallpapers or GIFs. |

Use an isolated source reference. Import qualified source and the original license only, attributed by file and commit. Preserve TLauncher's GPL and the original wallpaper credits. The table records architectural boundaries; the parity matrix records implementation and evidence separately.

## Native Settings decision

`impostazioni.h` paints its own settings rows, hover areas and navigation with Direct2D/DirectWrite and shared factories from `fabbriche.h`. Its `WM_KEYDOWN` case handles Escape and Up/Down section changes; this is not keyboard operation for every control. There is no Settings `WM_GETOBJECT` handler or UI Automation provider in that implementation. The upstream settings use per-user registry values. Its seven languages are Italian, English, Spanish, French, German, Portuguese and Arabic; TLauncher also requires Vietnamese and Simplified Chinese.

Keep the WebView2 Settings coordinator and profile JSON now. Reuse native drawing, factory lifetimes and metrics as appropriate, but require named accessible elements, complete keyboard interaction, scaling, and settings parity before the replacement becomes the default. Do not import the upstream registry store or startup toggle.

## Other compatibility findings

- `destro.h` obtains the active Shell view's `IContextMenu`, negotiates `IContextMenu2/3` and dispatches the selected verb. The later integration must preserve owner-drawn extension and cascading menu messages; when equivalent handling is unavailable use Windows's menu.
- `banner.h` changes per-app `ShowBanner` registry values. The copy/restore policy persists independently of the popup. Keep original settings and do not port the banner suppression by default; `notifiche.h` models permission and partial enumeration separately.
- `comodita.h` implements Keep Awake, silence and timers independently of the Notch; `widget.h` already uses Open-Meteo and bounded RSS sources. Integrate with TLauncher's existing time/weather owners.
- The cursor pack and wallpaper paths are not licensed by inference from the source license. This branch imports no upstream cursor pack, wallpaper, logo, or video. Cursor archive validation and original-theme recovery remain requirements for importing user packs.
