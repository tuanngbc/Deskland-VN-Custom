# Deskland Rust Dock + Stage Manager — execution status

## Authoritative checkpoint — 2026-11-21 00:26 UTC (D0 + mandatory D9)

Inspected SHA: `1d5d4c99a868fd4a475b1a1e160976d19ea29d35`. D0 docs `45bacf4` also passed [CI 44844337708](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44844337708). Historical pending CI notes below are superseded by this evidence.
42 Rust tests passed (including seven Dock tests), five existing native tests separately passed. JS renderer 32, shortcuts seven. Runner is Windows Server 2025; no Dock-specific native GUI evidence.

| Milestone | Implementation | Native acceptance |
|---|---|---|
| D0 architecture | CI_PASS at 45bacf4 | NATIVE_PENDING (measurement protocol only) |
| D1 window engine | IN_PROGRESS; EXE grouping, seven model/geometry tests | NATIVE_PENDING |
| D2 Dock UI | IN_PROGRESS; opaque GDI scaffold, uncached Shell icons | NATIVE_PENDING |
| D3 Stage | IN_PROGRESS; app sidebar only | NATIVE_PENDING |
| D4 controls | IN_PROGRESS; no reversible placement | NATIVE_PENDING |
| D5 multi-monitor/DPI | IN_PROGRESS; negative coordinates tested, manifest PerMonitorV2 | NATIVE_PENDING |
| D6 system | IN_PROGRESS; Settings links WINDOWS_DELEGATED | NATIVE_PENDING |
| D7 Compatibility | IN_PROGRESS; Windows taskbar retained | NATIVE_PENDING |
| D7 Replacement | BLOCKED; no safe tray parity/recovery evidence | NATIVE_PENDING |
| D8 RC | NOT_STARTED; development artifact only | NATIVE_PENDING |
| D9 product integration | IN_PROGRESS; current unmanaged child/config do not meet gate | NATIVE_PENDING |

Next: D1 local/global app selection/filtering and D9 main-owned configuration, Settings, lifecycle and independent toggles. Build on GitHub only. No installed binary/profile/baseline changes or merge.

### D1/D9 foundation increment (IMPLEMENTED_UNVERIFIED until its own CI)

- Same-process managed Dock/Stage UI thread, explicit readiness, channel commands/partial edits, joined cleanup; standalone spawn removed. New modules default off.
- Canonical settings.json with one-time legacy pin/scope/side migration; independent Dock/Stage/wallpaper toggles in existing Settings, native fallback and tray. Atomic writes preserve unknown existing settings and refuse to overwrite malformed files.
- Reuse main monitor detection; work-area changes rebuild overlays. Out-of-context WinEvent notifications coalesce, one 5-second recovery timer replaces per-monitor polling. Unchanged items do not repaint, Shell icons cached/bounded/cleaned up. Suspension hides overlays.
- D1 fixes unpinned global activation locality, excludes DWM-cloaked windows, uses restored monitor identity for minimized HWNDs; foreign ShowWindow actions become asynchronous.
- New isolated native CI test covers same PID, independent Dock/Stage toggles, suspend and shutdown HWND cleanup; it is not owner dual-display acceptance. Additional options/migration/atomic-file and actual Settings-handler tests. Rust/Windows results pending this increment's CI; local JS 32+7+4 passed.
- D9 still incomplete: per-monitor appearance/size/position/auto-hide controls and full Explorer/Windows restart/renderer-toggle native acceptance remain pending. D1 packaged identity, multiple-window picker/reorder and other D2-D8 work remain open. No RC/merge/deployment.
See [architecture](DOCK_ARCHITECTURE.md), [acceptance](DOCK_ACCEPTANCE_MATRIX.md), [issues](DOCK_KNOWN_ISSUES.md), [handoff](DOCK_HANDOFF.md).

Baseline: accepted Rust `a604519` on `codex/rust-test`; development only on `codex/rust-dock`, PR #1 Draft. No merge and no Explorer replacement enabled.

## Implemented and CI-gated code
- Optional `--dock` process isolated from the wallpaper rendering loop; owner tray menu toggles Dock.
- Per-monitor dock windows, running applications via top-level HWND enumeration, application icons (Shell), pin/unpin, local/all-app grouping, window switching, fullscreen overlay suppression.
- Stage Manager **initial sidebar** on each screen: app groups shown on the side, active-window indication, click to switch, side left/right/off persisted to `dock.json`. **Not yet macOS-fidelity previews or real Stage grouping/minimize semantics**.
- Right-click Dock app window menu for minimize, maximize, restore, left/right tiling and borderless-sized window placement. Last item is **not true native fullscreen mode** and must not be described as such.
- Windows Settings links: Wi-Fi, Bluetooth, sound, notifications. **Not native Quick Settings flyouts or system tray parity**.
- Geometry tests including negative-coordinate monitors, Stage placements, preference JSON.
- GitHub Actions runs on `codex/rust-dock` to test/package as `tuanpc-desk-Rust-win-x64`.

## Important limitations / nonclaims
- Existing rightmost "Tray" label currently launches taskbar settings. There is **no third-party tray icon host** yet.
- No DWM live thumbnails in Stage, actual magnification spring animations, smooth acrylic compositor, drag reordering, dynamic resize, virtual desktop parity, screen-reader support, real status-center flyouts, or taskbar replacement yet.
- No installation changes or replacing Explorer; taskbar stays visible and usable for rollback.
- Fullscreen movement currently uses window-position APIs, may differ from macOS fullscreen; avoid changing maximized/unmanaged windows without further tests.
- No Windows interactive dual-monitor evidence for new Dock/Stage features. CI does not verify true desktop interactions.
- Potential Shell/GDI Win32 reentrancy, process enumeration differences and resource usage require native tests.

## Required completion work
1. Implement proper window grouping, DWM thumbnails/live previews, stage drag/drop and geometry restoration without breaking fullscreen apps.
2. Replace GDI placeholder paint with GPU/composition-backed glass shell, real macOS 27-like dock magnification, spring transitions, reduced motion, icon labels, theme control.
3. Implement full app actions and keyboard/accessibility workflows, snapping, native Windows maximize/fullscreen, multi-monitor move, virtual desktops where supported.
4. Complete system tray interoperability (icons, menus, overflow) and native-compatible Wi-Fi, Bluetooth, sound, notifications, clock, power widgets; coordinate with existing System Monitor.
5. Develop compatibility/replacement mode switch only after safe taskbar restore watchdog, AppBar/explorer restart handling, recovery and owner native acceptance.
6. Run long-lived Windows 11 multi-display performance and safety acceptance before marking VERIFIED. Keep actions CI gates separate from owner native gates.

## CI and evidence
Latest deployment should be read from GitHub Actions at `https://github.com/tuanngbc/Deskland-VN-Custom/actions/workflows/build-rust-test.yml`; this document records feature scope, not a claimed CI PASS for unchecked commits.

### 2026-11-21 CI recovery
- GitHub Actions runs `44807055187` and `44807093091` failed at **Build Rust native app** for Stage increment (`e2c0efb`, `375b044`). Logs were not accessible through the connected GitHub text endpoint; do not assert this was the sole compiler failure.
- Static source review identified a missing `stage_side` field in the malformed-JSON `Preferences::load` fallback after extending the struct. Fixed in commit `bfa120ac41be5ea852ba54c297439c262549662a`.
- **CI verification pending** for that fix until its own workflow run completes; prior green `57b9d44` does not validate Stage changes. No native owner validation or merge.

### 2026-11-21 Stage per-monitor behavior follow-up
- Confirmed green GitHub Actions on commits `bfa120ac` (run [44814061528](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44814061528)) and `83f112e1` (run [44814088369](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44814088369)); these are **CI PASS**, not native feature acceptance.
- Commit `a8bc2e6d` aligns Stage application filtering and same-executable window counts with Dock's local-monitor/all-monitors setting; adds regression test for negative-coordinate secondary monitor and case-insensitive app grouping.
- This Stage filtering increment requires its own CI result. Native two-monitor, DWM thumbnail, taskbar parity, fullscreen game behavior remain **PENDING**. No Explorer replacement or merge.

### 2026-11-21 pinned app monitor-scope fix
- Verified previous Stage follow-up CI PASS: [44826611244](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44826611244) and [44826625961](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44826625961).
- Commit `a015a42fa22599593e0924d7b29d501ba0cea2cc` fixes pinned app activation on multi-monitor: prefer the HWND on the current screen, avoid switching to an off-screen window in local mode, count only local running windows, deduplicate case-insensitive pinned paths. A pinned executable without local window remains launchable.
- The right-side placeholder formerly labeled `Tray` is now honestly labeled `Taskbar`, because it opens Windows taskbar settings and does **not** host third-party notification icons.
- Added Rust regression test `pinned_app_respects_local_monitor_and_deduplicates`. **CI for this new commit is pending until its workflow finishes**. Native owner tests remain PENDING. No taskbar replacement, baseline modification or merge.

### 2026-11-21 Stage global-mode activation locality
- Verified previous pinned-app CI green: [44829967553](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44829967553) and [44829981171](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44829981171).
- Commit `7ec9267f4c9ee4fe88227005f19bc929499f64f0` corrects Stage Manager's global-mode selection: when the same executable has windows on multiple displays, clicking the Stage item prefers a window intersecting the current monitor, while keeping the total cross-monitor count. The previous implementation picked the first enumerated HWND, potentially switching focus to another monitor.
- Adds `global_stage_prefers_local_window_for_activation` Rust regression covering negative-coordinate secondary screen, case-insensitive path matching, local/global counts, and fallback to an app only running on another display.
- This commit requires its own CI gate; no native Windows two-monitor owner acceptance. No change to taskbar replacement, main, or baseline.
