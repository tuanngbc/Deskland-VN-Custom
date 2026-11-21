# Dock and Stage architecture (D0 + D9)

Audit source: `1d5d4c99`, 2026-11-21. Accepted renderer: `a604519`.

## Existing lifecycle and resource audit

`src/dock.rs` is a Win32/GDI scaffold. `--dock` bypasses normal main initialization and logging, creates per-monitor popups and a message loop. The wallpaper tray spawns it without parent supervision. Exiting Deskland does not stop it. FindWindow is a racy singleton. Its separate dock.json competes with the canonical settings owner.

Every bar has an 850 ms timer refreshing all outputs: enumeration scales quadratically with monitor count. Paint retrieves/destroys Shell icons on every frame. Brushes/icons are generally released, but repeated extraction is costly. Refresh/create/destroy paths hold thread-local RefCell borrows across synchronous Win32 calls; callbacks can reenter and panic. Future WinEvent callbacks must queue/coalesce events, never mutate state inline.

Manifest already declares PerMonitorV2. Dock lacks dynamic WM_DPICHANGED; recreate ignores work-area-only changes. HWND filtering excludes tool/owned/invisible/untitled windows but not cloaked windows; identity is EXE only. Minimized geometry is not normalized. Unpinned global Dock may select an off-monitor window. Stage is an app sidebar, not real workspaces or live previews. Borderless command resizes only: not true fullscreen.

Development has not changed shaders, wallpaper presentation, launcher or system monitor. Topmost popups/enumeration/repainting still can affect composition. No measured performance improvement or regression is claimed.

## Mandatory D9 ownership

One product (`tuanpc-desk.exe`), existing Settings and one main-owned settings.json. Wallpaper, Dock and Stage have independent persisted enable flags. Existing Windows autostart restores enabled modules without --dock. Saver/headless instances do not create desktop modules.

Prefer a dedicated managed native UI thread for the first integration increment: no orphan process, duplicate autorun or configuration executable. Main owns its runtime and shutdown. Channels carry snapshots/edits; wake messages belong to the owning loop. Persist edits through main only. Do not hold state borrows during Win32 calls. Clean up hooks, timers, thumbnails and windows on their owning thread before joining. A thread provides no crash isolation; revisit process separation only with evidence.

If later isolating a Rust child, require readiness, health, bounded restart/backoff, explicit shutdown and kill-on-close Job Object assigned before execution. An unmanaged spawn(--dock) is prohibited.

Canonical versioned `dock` config: independent flags, app scope, pins/order, monitor preferences and appearance/behavior. Read legacy dock.json only when canonical config is absent; validate and bound input, preserve the old file, persist once via main. Malformed canonical config must not resurrect legacy enabled state. New modules default off. Preserve unknown existing fields and use atomic replacement so write failures retain valid settings. UI reports persistence/startup errors.

Settings/tray use the same owner and update one another. Disabling wallpaper must not quit other modules or lose themes/widgets/shortcuts; avoid recreating renderer for Dock-only changes. Lock suspends overlays/previews.

## Incremental boundaries

- Pure options/model modules: validation/migration/identity/grouping/geometry/order; deterministic tests without real profile access.
- Managed runtime: owning UI loop, readiness, command/events, cleanup and isolated native tests.
- Retain existing UI/collector initially; split at useful seams without rewriting baseline host.
- Reuse monitor/DPI helpers, launcher Shell/icon knowledge and system-monitor sampler. No second permanent WebView2 or expensive renderer.
- Documented DWM previews; hide/fallback for invalid, cloaked, locked or protected content. Validate HWND identity before actions and preserve placement. No arbitrary app style rewriting to imitate fullscreen. Documented virtual-desktop APIs only.

## D7 boundary

Shell_NotifyIcon registers this app's notification icon; it is not a third-party notification-icon hosting API. Replacement remains BLOCKED until supported tray strategy, AppBar/work-area negotiation, watchdog and restoration have evidence. Hybrid Compatibility Mode retains Windows taskbar/tray. No Explorer injection, forced termination or taskbar hiding.

## D8 measurement protocol (NATIVE_PENDING)

Same theme/time/weather, monitor arrangement/DPI, power/driver as baseline. Warm 60 seconds, sample at least 120 seconds, repeat three times. Record per-process CPU/private bytes/working set, GDI/USER handles/threads, per-process GPU engine and total DWM GPU separately, FPS and frame-time p50/p95/p99, startup readiness. Aggregate taskbar GPU is not process-only GPU.

Compare wallpaper only; idle Dock; hover/launch/Stage; two desktops exposed; both covered; one covered; lock/saver/suspend/resume; fullscreen each display; hotplug/mixed DPI. No repaint without changed frame. Verify pause/release when wallpaper disabled. Run 100 module toggles and 4 hours resource sampling. Preserve raw samples/hardware/commit. Numeric performance PASS requires evidence.
