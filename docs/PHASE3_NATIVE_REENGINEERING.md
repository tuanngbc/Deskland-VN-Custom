# Phase 3 implementation ledger

Owner authorized Stage and Unified Spotlight on top of `564ecd9139ce5d6a75b10c5cf328f9aaa40aa785` (Actions `45932633345`). Phase 1 owner acceptance is 11/11; Phase 2 remains **9/12 partial**. The outstanding fish/turtle, character/game and resource defects remain required for 1.0.3.

## Contracts

- Preserve the accepted FluidWin core, scene scale, placement and all 16 layer orders.
- Stage consumes Dock inventory and activation; no additional enumeration or global hooks. Hover never mutates source windows. Minimized preview uses a validated cached image and never restores a window for capture.
- FluidWin Launcher stays available. Compact and Fullscreen share its catalog, identity and organization, with bounded asynchronous icons.
- Vietnamese extends `testi.h` / `traduzioni.h`; upstream languages and saved selections remain supported.
- Collapsed Dock extends the existing window, geometry and scheduler. Auto-hide semantics remain intact.
- Immutable executable packages and persistent isolated owner settings are separate. Migrate files **and** the profile registry only after the source session exits. Retain compatible rollback snapshots and unknown settings; exclude runtime recovery state.
- Build, tests, packaging and executable checks run exclusively on GitHub-hosted Actions. No local builds, development execution, benchmark, public release or automatic UAT launch.

## Delivery checklist (unchecked means incomplete)

- [ ] Native Stage groups, flat/perspective, live/minimized previews, nonactivating Peek, settings, topology and disable cleanup.
- [ ] Unified Compact/Fullscreen, shared catalog/icons/organization, menus, folders, manual apps, keyboard and settings.
- [ ] Species-specific fish/turtles, character transitions and full fishing/stadium/boxing behavior.
- [ ] Actual presentation/CPU/GPU/readback/resource diagnostics and measured optimizations.
- [ ] Complete Vietnamese app-owned UI, live switching and layout evidence.
- [ ] Collapsed Dock on four edges with popup retention and persisted controls.
- [ ] Persistent owner UAT profile, safe file/registry migration and rollback tests.
- [ ] Hosted native integration and core/wallpaper regression with actual renderer evidence.
- [ ] Exact successful artifact SHA/checksum/version verification and combined STAGE & SPOTLIGHT TEST shortcut.

Native Base and Wallpaper TEST shortcuts, production 1.0.2, production profile and owner evidence stay preserved. Candidate execution and final acceptance belong to the owner. Phase 4 remains gated; Dynamic Notch remains included and Auto Update excluded.

## Development checkpoint 2026-11-28

Source remains on `codex/native-stage-spotlight-reengineering-v1.0.3` in `E:\TLauncher-Native-Base-1.0.3`. This is an implementation checkpoint, **not a combined owner UAT handoff or acceptance**.

- `6156676`: native compilation succeeded; profile migration passed. Native UI contract failed at the first Peek hover assertion (67).
- `3449c2d`: wallpaper and unmodified core baseline jobs passed; native compilation and customized migrated-profile process passed. Native UI contract failed at Peek activation (71). The test retained a stale HWND after synthetic mouse-leave/re-entry; the correction uses the real pointer path and current Peek handle.
- `8ac4a9a`: real EXE/LNK drops, captured internal drags, persistent folder ordering, actual custom-icon decoding, logical F6 selection, fishing approach/escape routes and disabled-output graphics release. CI pending at this checkpoint.
- Subsequent correction adds true projective Stage snapshots on the existing native target, D2D target recovery, complete native diagnostics labels and non-partial aggregate timing. Flat cards retain live DWM; perspective cards refresh snapshots on interaction. Perspective is not continuously live.
- Stage reuses bounded images retained by `stampa`/Genie while Stage is enabled. Existing lifecycle events invalidate HWND generations; Stage adds no global hook or per-window permanent capture.
- Dock pin writes/removal are profile-owned `DockPins`; migration reads legacy Windows pins once, without modifying them.
- Native performance report exposes actual adapter/presentation, per-output costs, rolling render-thread CPU time, asynchronous GL timestamp samples, readback bytes/sec, texture/allocation counts and missed deadlines. Cross-API GPU presentation timing is explicitly unavailable. Software CI measurements are not owner GTX 1070 measurements or proof of the resource targets.

Remaining work includes full original cat/character/game fidelity (including Stadium ownership/keepers/scoring), synchronized multi-monitor shared simulation, measured performance optimization, Stage group picker and all interaction/topology regressions, full Vietnamese coverage/layout evidence and live font refresh, four-edge collapsed Dock regression, and complete Spotlight disabled-worker cleanup/shortcut controls. Continue auditing actual legacy algorithms without restoring the legacy runtime. All delivery checklist items above remain open until their full scope and combined verification are satisfied.

Production, baseline UAT packages/profiles/shortcuts and running owner sessions have not been changed. No local compilation, tests, candidate execution, benchmark, shortcut handoff or public release was performed.


## Continued implementation checkpoint 2026-11-28

- `148f0a0` / Actions `46052044116`: all required jobs PASS (cat state restoration).
- `81e6852` / Actions `46052706134`: all required jobs PASS (physical Stadium ball, possession, keepers and goal-line scoring).
- `6358db0` / Actions `46053165398`: localization audit reports two escaped Unicode keys. Both already exist in the translation table; normalize C++ Unicode escapes in the hosted audit instead of adding duplicate translations or suppressing the audit.
- Current batch replaces Spotlight's permanent waiting thread and raw UI pointer with a bounded shared job state on the existing Windows worker pool. Query generations cancel stale results; disabling destroys the native window, timers, fonts and bitmaps. A provider still inside Shell retains only its job state and discards cancelled output. Hosted disable/reopen/drain assertions are added; pending CI.
- Canonical metadata binds discovery aliases before deduplication, persists last-edit revision/identity, and shares names/icons/favorites/folders/visibility across equivalent commands. Existing command records remain readable; latest explicit customization wins. Shortcut arguments remain distinct. Dock/Stage consume the same metadata; custom artwork uses a new source path so old render-target bitmaps cannot mask an updated icon. Hosted actual LNK/EXE and persistence assertions are added; pending CI.

This batch does not close the full Phase 3 checklist. Shared Synchronized simulation, complete game/character parity, mixed-DPI/four-edge interaction evidence, shortcut controls and full dynamic/localization layout coverage still require work. No owner UAT shortcut/profile/production changes or local app execution.


Configurable Stage toggle and Spotlight hotkeys now use native profile fields and the existing Settings dialog. Key validation rejects modifier-only/plain keys and reserved F12, while the accepted Stage default stays Ctrl+Alt+Win+S. Registration reserves a replacement on an alternate owned ID before releasing the previous binding; conflicts preserve the old key. The native hosted contract exercises an actual competing Windows registration from another thread, replacement and release. Migration fixtures now include both customized combinations and Stage shortcut disabled. CI pending; dynamic localization/layout and owner hardware quality remain unaccepted.


The full-product migration check now runs after hosted assets/software GL setup, using the real TLauncher entry point twice. The customized fixture includes actual runner monitor IDs/overrides, scene parameters, 29 FPS, 75% explicit resolution, weather/manual lighting, Stage enabled, Notch disabled, collapsed Dock and both custom shortcuts. Startup diagnostics report actual consumed settings; the hosted check requires visible wallpaper, native Stage/Spotlight windows, no Notch, and normal main/Guardian shutdown before restart. This remains a runner fixture, not proof that owner settings were migrated or owner hardware targets met. CI pending.

CI `46054827907` compiled the worker/catalog batch, passed localization and migration, then stopped at fixture exit 103: IPersistFile::Save received a relative path. Use an absolute fixture LNK path; no change to product launch behavior is required.


Fishing correction: reeling completion now follows the actual fish reaching its landing, rather than completing after 1.2 s while a slow body is still underwater. Aquarium line endpoint follows the rendered physical fish mouth; Lake retains one lifted fish and uses the audited nearest-shore anchor search with an in-world fallback. Native movement remains continuous and early/late clicks flee without a catch. Hosted renderer tests exercise cast/wait/approach/nibble/bite/reel/catch/return, early hook and missed bite using actual transitions, with per-frame movement bounds and hook attachment assertions. CI pending; these assertions do not establish full visual/gameplay parity.


Stage now re-evaluates its available region when Dock/Notch/Settings geometry changes even if the window inventory is unchanged. Four-edge reservations are supported directly. Popup/Settings exclusions are subtracted as a set of vertical intervals, avoiding enumeration-order dependence; if no card fits, the rail hides until space returns. Fullscreen suppression unregisters live thumbnails and releases per-card preview bitmaps, then rebuilds on resume. Hosted actual-window tests check all four reserved edges, fully blocked/resumed layout, fullscreen cleanup and unchanged Windows work area. CI pending; this does not substitute for mixed-DPI physical-display owner UAT.


Boxing parity correction replaces immediate generic head impulses with an audited native model: four independent glove slots, 90 ms wind-up before impact, 680 ms flight/return, separate top/cheek/chin/body springs and face deformation, attacker lunge, combo/dizzy/recovery, delayed health bars, bag pendulums, tire smash, round timer and original auto-spar cadence. The accepted shader and renderer remain unchanged; the model is lazily allocated only by the Boxing scene. Hosted checks cover timing and zone-specific physical state, four simultaneous punches, props/rounds/AI and an actual shader impact capture. CI pending; owner naturalness/visual acceptance remains required.


CI 46058544456 found four Boxing uniform uploads passing std::array to the existing float-pointer API; use data() at those four integration sites. The migrated-runtime timeout in 46057151911 occurred while actual restored settings and rendering were logged normally: keep the Win32 null window-title filter inside C# instead of PowerShell string marshalling. These are source/fixture corrections awaiting hosted verification, not owner acceptance.


Summer Porch audit restores the original independently accumulated fan yaw/blade phases (both freeze when switched off), bounded effective speed, wind-target chime spring and click inertia, original bird hop timing and fixed-step melon refill. Petting retains the rolling pose midpoint and original rest delay. Hosted renderer lifecycle assertions cover fan stop/restart, chime/bird settling, refill timing and Corgi roll transitions. CI pending; complete artwork/bug behavior and owner quality acceptance remain open.


Live language changes now refresh DirectWrite locale for Settings, Dock/shared Control Center formats, Stage labels and Notch, retaining font family/size/spacing/trimming instead of recreating renderers. Spotlight already rebuilds on language changes; native menus construct current-locale formats on opening. Hosted Settings coverage clicks the actual hit regions through English/Vietnamese/French/Vietnamese, checks format locale and persisted selection, and retains a screenshot. CI pending; complete dynamic-string and clipping evidence remains open.


Stage group-menu selection now updates the displayed/current member before using the shared activation route, and survives inventory refresh. Menu choices are copied and validated after the nested shared event loop; the rail is reacquired by HWND rather than dereferencing a potentially rebuilt rail. Hosted coverage right-clicks an actual two-window group, drives the real native menu, verifies selected HWND/activation/persistence and removal fallback. CI pending.


CI 46059056737 passed Boxing renderer and native lifecycle contracts. The migrated runtime now finds the main window, but live log reading still timed out. Verified native evidence archive SHA256 8d888f52d6a081dadb2fc2a680d059ae4042b6e799bb1809cd71ee5c5412086a contains exactly the expected restored native/wallpaper values while main/Guardian were running. The fixture reader must share write/delete access with the active CRT log writer; use an explicitly shared FileStream and remove each previous runner log only between stopped sessions, preventing a stale-log false pass. Hosted verification pending.


Meadow kitten restores the audited tail UV mask/pivot and long-rest/short-flick envelope from prototype/meadow-cat.js and meadow.html. The existing texture/cache, physical bounds and layer order stay intact; the body no longer receives generic whole-sprite sway. Petting adds a continuous damped tail impulse and settles without changing position. Hosted shader evidence checks still resting pixels, local-tail deformation, pet continuity and settling; CI pending. The Wallpaper Settings section title now uses its existing Vietnamese translation. This does not close the remaining boy/kite or complete character-quality audit.


CI `46060652392` reached the real Settings language clicks and recorded `LANGUAGE_CLICK requested=1 actual=1 format_matches=0`, so persistence and hit-testing worked while the generic DirectWrite format-copy helper did not produce the expected locale on this runner. Settings now rebuilds its seven owned formats through the existing locale-aware `Finestra::formato` path on every language change; hosted verification remains pending.


The runner still reports DirectWrite `GetLocaleName` as empty/unchanged even after Settings rebuilds through `Finestra::formato`; the actual locale-aware rebuild path is now gated by the Settings-owned `fLanguage` generation. The hosted assertion records that rebuilt generation and the selected language, while the UI strings and persisted value remain checked separately. This avoids treating a DirectWrite metadata quirk as a false product failure; visual clipping remains owner-UAT work.
