# Acceptance matrix D0-D9

See the [visual redesign and utility metrics checkpoint](DOCK_VISUAL_REDESIGN.md) for the current candidate, exact Windows CI, and the scope of the native evidence.

Current P0 build and scoped native evidence: [Dock P0 verification](DOCK_P0_VERIFICATION.md). The full owner acceptance gates remain pending.

See the [current product gates and exact native evidence](DOCK_VISUAL_ACCEPTANCE.md#owner-host-windows-11-results). Older D0–D9 rows below describe broader milestone gates; scoped interaction and Settings PASS results do not close them.

The latest [daily-use gate table](DOCK_VISUAL_ACCEPTANCE.md#daily-use-functional-closure--4d64a26) records current Dock/Stage/Utilities/Window coverage. [Cold Settings and real trial renderer recovery](DOCK_VISUAL_ACCEPTANCE.md#daily-use-verification--settings-recovery-and-fixture-recording) were tested separately with Clock. No owner gate closed.

Vocabulary: NOT_STARTED, IN_PROGRESS, IMPLEMENTED_UNVERIFIED, CI_PASS, CI_FAIL, NATIVE_PENDING, NATIVE_PASS, OWNER_NATIVE_ACCEPTED, BLOCKED. CI_PASS never implies native acceptance.

2026-11-21 owner smoke at fa93c16 added partial native evidence: eight module combinations via Win32 Settings, selected WebView controls, same-PID/negative-coordinate geometry, saved configuration/startup and normal cleanup. See [case results](DOCK_NATIVE_SMOKE_REPORT.md). NATIVE_PENDING below still applies to each complete milestone; do not erase narrower PASS or promote it to whole-gate PASS.

| Gate | Required evidence | Remaining / existing evidence | Native |
|---|---|---|---|
| D0 | Lifecycle/resource/API audit, benchmark protocol, CI | CI_PASS at 45bacf4, run 44844337708 | NATIVE_PENDING |
| D1 | Identity/filtering/local activation, multiple windows, pin/order, events | Coalesced hooks, cloaked/minimized handling, local/global regression; EXE identity still incomplete | NATIVE_PENDING |
| D2 | Glass, crisp icons, magnification, auto-hide, keyboard/drag, idle budget | Premultiplied capsule, bounded Shell icon cache, computed spring, insertion guides and conservative alpha fallback. Dock idle workload has a scoped PASS; paired visual evidence required. | NATIVE_PENDING |
| D3 | Real previews, group switch/restore, privacy | d708b09 owner-host fixture verified content updates, one protected source, two groups and Stage OFF restoration; full gate pending | NATIVE_PENDING |
| D4 | Reversible snap/quadrants/move; explicit fullscreen limits | Basic controls; no saved geometry | NATIVE_PENDING |
| D5 | Negative coords/work-area/DPI/hotplug/stable display preferences | Pure geometry tests, manifest PerMonitorV2 | NATIVE_PENDING |
| D6 | Accurate states/controls with explicit classification | Native volume/calendar and hardware reads; network control, output selection and Bluetooth management delegated. | NATIVE_PENDING |
| D7 A | Windows tray retained, Explorer recovery | Explorer taskbar retained; AppBar registration/cleanup and recovery replay verified. Actual Explorer restart remains open. | NATIVE_PENDING |
| D7 B | Tray parity/AppBar/watchdog/restore | BLOCKED; supported tray strategy unresolved | NATIVE_PENDING |
| D8 | Regression/performance/leaks/RC hashes/rollback | Development package only, no RC | NATIVE_PENDING |
| D9 | Same EXE/config/Settings/autostart, independent modules, cleanup | Managed same-process runtime + common Settings/tray/config migration; CI lifecycle PASS at 3526fcc. Per-display appearance/behavior and full desktop acceptance pending | NATIVE_PENDING |

## D9 integration cases

1. Fresh profile defaults new modules off; wallpaper/themes/hotkeys/widgets unchanged.
2. Migrate pins/scope/side from dock.json once, preserve file; canonical settings always win. Malformed/read-only settings fail safely with visible error.
3. Existing Settings and tray enable Dock/Stage independently, update each other; no manual second app/command.
4. Wallpaper off while Dock/Stage stay on; restore theme unchanged. Dock off with Stage on, Stage off with Dock on.
5. Restart app/Windows restores configuration with no duplicate runtime. Saver/headless launches do not start Dock.
6. Exit, startup failure, crash, forced termination and 100 toggles leave no runtime/windows/timers/hooks/processes.
7. Explorer restart with wallpaper/clock/system monitor/Dock/Stage restores controls; Windows tray/taskbar stay available.
8. CI native integration tests use isolated temporary files/windows; interactive native acceptance separately pending.
9. One Windows x64 package includes all modules, Settings, existing themes/widgets/shortcuts, BUILD-COMMIT and checksum.

## Owner Windows 11 two-monitor RC checklist

Record resolution, refresh, orientation, primary, negative coordinates and scale; use landscape + portrait at different DPI. Swap primary, unplug/replug, change scale/taskbar position/work area live. Verify local/global multi-window selection (Win32 + packaged), minimized window restore, elevated/hung apps and fullscreen each screen. Test keyboard/focus, icons/glass/motion at 100/125/150/200% and reduce motion.

Stage moving previews must be real; independent groups restore minimized/maximized states and geometry. Cloaked/locked/protected content must not leak. Test rapid close/reopen, lock/unlock, saver, sleep/resume and monitor loss. Retain Windows Start/tray/Task Manager. Follow D8 benchmark, 100 toggles and long run. Save actual tested SHA/screenshots/traces. Only owner sets OWNER_NATIVE_ACCEPTED; do not merge beforehand.


## Visual rebuild milestones (separate from D9 smoke)

| Milestone | Verified | Remaining gate |
|---|---|---|
| M1 | c2a589b CI and owner-host HWND geometry for primary Dock/all-display Stage; real WebView layout captured | Exact latest artifact, secondary-only and Custom tests, Settings/Win32 persistence, hotplug and mixed DPI |
| M2 | Native glass/layered proof prepared; [visual notes](DOCK_VISUAL_ACCEPTANCE.md) | Hardware evidence and production integration, DPI/quality/icons |
| M3 | Existing basic activations only | Animated layout and hit testing, auto-hide, reorder, keyboard and feedback |
| M4 | Live DWM previews, two-group activation/restoration and separate 96-DPI side/scale observed | Full privacy matrix, richer group editing, display moves, mixed DPI and Z-order |
| M5 | Raw pre-refactor samples started | Matched 12-scenario results, animation times, 100 toggles and leak checks |
| M6 | No RC or owner visual/native acceptance | Native mixed-DPI/recovery/rollback and four-hour run |

## Updated evidence for M2–M4

| Milestone | Narrow verification | Not accepted yet |
|---|---|---|
| M2 | Owner-host alpha composite and real capsule still; Spotlight click | True blur, light/dark wallpaper matrix, DPI, animation pacing |
| M3 | Windows CI for rail, keyboard plumbing and reorder model | Native reveal/focus/drag, accessibility, transition quality |
| M4 | d708b09 CI + native source-content change, two groups, normal placement and Stage OFF | Full privacy matrix, mixed DPI, Z-order and display loss |

## Current utility capability boundaries

| Capability | Native behavior implemented | Delegated / remaining |
|---|---|---|
| Network | NATIVE_IMPLEMENTED: physical link state and Wi-Fi signal only when Windows returns it | WINDOWS_DELEGATED: network Settings; link does not prove internet reachability |
| Audio | NATIVE_IMPLEMENTED: default multimedia endpoint volume/mute; native Trackbar sends coalesced writes during drag and reads back the Windows endpoint | WINDOWS_DELEGATED: default output selection; external-volume callbacks and authorized endpoint-write verification pending |
| Bluetooth | NATIVE_IMPLEMENTED: adapter enumeration and known Classic connection observation | WINDOWS_DELEGATED: Bluetooth Settings; UNSUPPORTED: comprehensive LE/radio on-off status |
| Battery | NATIVE_IMPLEMENTED: percent, AC, charge, hide without a readable battery | WINDOWS_DELEGATED: Power Settings; real battery host pending |
| Date/calendar | NATIVE_IMPLEMENTED: local date/time at menu open and owned Win32 month view | No integration with calendar accounts or events |
| Notifications / Quick Settings | UNSUPPORTED: unread count or notifications from other apps | WINDOWS_DELEGATED: Windows panels |
| Deskland | NATIVE_IMPLEMENTED: commands reuse existing Spotlight, capture/record, taskbar stats, wallpaper, Stage and Settings | WINDOWS_DELEGATED: Explorer, Show Desktop, Task View |
| Utilities layout | Per-display enable/disable, leading/trailing button, menu details, order/visibility, reset; native Settings has essentials | Up to five individually pinned vector icons; narrow layouts keep overflow access. Always-visible inline live values and per-widget spacing/size/opacity pending. D7 third-party tray stays out of scope. |

Implementation labels do not certify native interaction or full feature acceptance. P4 and OWNER_NATIVE_ACCEPTED remain pending.
