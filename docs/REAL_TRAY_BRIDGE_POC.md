# Real tray bridge: current state

**BLOCKED** for a collector that both coexists with Explorer and retrieves external notification icons while the native taskbar remains hidden. No permitted backend produced verified existing icons, subsequent events and callbacks. The prior read-only probe is useful negative evidence. The [cooperative POC](SYSTEM_TRAY_POC.md) only validates its own peer and is not repeated as external-app evidence.

Deskland keeps the caret as Control Center and keeps minimized windows separate from notification icons. There are no newly simulated tray rows. If an isolated disposable machine becomes available, first verify coexistence and recovery with actual third-party applications before integrating an adapter. A proposed adapter boundary separates discovery/snapshot, new/updated/deleted items, and gestures with owner/identity revalidation. Session disconnect would clear all stale entries. Do not publish an external icon action until its event and native menu are proven.

## Windows 11 build 26200 follow-up

`tests/trace-explorer-tray-uia.ps1` matches shell-owned HWNDs and walks both UI Automation views, with bounded depth and node count. It records only counts, shell geometry and errors. It never opens overflow, changes visibility, invokes an application or reads its name. Run it in Windows PowerShell from a normal desktop session, giving it an output file in an isolated directory.

The 2026-11-23 read-only trace found:

| Shell surface | Raw / Control descendants | Buttons | Invoke | Errors |
|---|---:|---:|---:|---:|
| Primary taskbar, offscreen except for auto-hide edge | 3 / 3 | 0 | 0 | 0 |
| Secondary taskbar, offscreen except for auto-hide edge | 2 / 2 | 0 | 0 | 0 |
| Modern XAML overflow HWND, present but not visible | 0 / 0 | 0 | 0 | 0 |
| Legacy overflow HWND | Absent | — | — | — |

Both taskbar HWNDs report a visible Win32 style. Their geometry shows they are **parked by Windows auto-hide**. The older `taskbar-p0-existing-uia.json` also reported visible HWND styles and didn't record geometry. Neither trace is proof of behavior after a deliberate Deskland hide. The new trace proves that the available accessibility trees do not deliver an inventory in the observed state. It does not assert that a manually opened Explorer panel has no UIA nodes.

1. **Existing icons and tooltips:** No application button in either tree, including the unmounted XAML overflow. No source of icon keys, pixel data, or tooltip text in this state.
2. **Add, change, delete:** No validated UIA provider for the icon inventory and no corresponding external NIM event observer. Documented `Shell_NotifyIcon` is an application registration API, and `Shell_NotifyIconGetRect` takes an existing identity; neither supplies a listener or an enumeration.
3. **Clicks and native menu:** No external HWND/identifier/notification version was obtained. UIA had no Invoke target here; even if an element appears when Explorer opens, an accessibility invoke is not evidence for the native right-click menu or versioned callbacks. Those gates are untested.
4. **Windows 11 and app types:** The XAML surface and absent legacy overflow are observed. No inference about modern or packaged app behavior is made without an external icon inventory.

The pinned [Seelen](REAL_TRAY_REFERENCE_AUDIT.md), ManagedShell and RetroBar sources were checked again. RetroBar delegates to ManagedShell rather than using an independent collector. The ManagedShell compatibility path depends on Explorer's toolbar representation and alters autotraying during collection; its alternate host is a different shell receive path. Seelen uses a shell hook and has separate icon enumeration/initialization. None is a drop-in backend for the present owner trial.

Next proof requires a disposable VM with an independent Explorer session. Snapshot before the trial, run three independently installed applications with real tray icons, capture isolated traces for existing/add/change/delete and gestures, compare with Explorer visible/parked/hidden, and verify bridge exit and Explorer recovery. Revert the VM snapshot if Explorer visibility or registrations do not recover. License review and separate owner approval would precede any shell-affecting candidate on the owner machine. This round does not ship an adapter.
