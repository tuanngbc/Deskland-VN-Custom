# Deskland Rust Dock + Stage Manager — execution status

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
