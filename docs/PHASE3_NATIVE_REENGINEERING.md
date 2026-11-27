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
