# Known issues and safety limits

Audit source `1d5d4c99`, 2026-11-21. Open until code and evidence close each issue.

| ID | Issue | Disposition |
|---|---|---|
| K01 | Unmanaged --dock child survives exit; separate config/logging | High, D9 |
| K02 | RefCell borrows cross synchronous Win32 calls | High, lifecycle/reentrancy foundation |
| K03 | Unpinned global Dock selects off-monitor HWND | High, D1 regression |
| K04 | Repeated per-bar enumeration/timers and Shell extraction in paint | Medium, D1/D2 event scheduler/cache |
| K05 | Cloaked/packaged identity/minimized monitor association incomplete | High, D1 |
| K06 | Work-area/DPI changes and stable monitor preference IDs absent | High, D5 |
| K07 | Stage lacks DWM previews/workspaces/placement restoration | High, D3 |
| K08 | Opaque UI, limited items, no real magnification/keyboard/reorder | D2 |
| K09 | Borderless is resize, no original placement saved | D4; never call true fullscreen |
| K10 | Settings links are not status widgets/native flyouts | D6, WINDOWS_DELEGATED |
| K11 | No safe third-party tray host/AppBar/recovery evidence | D7 Replacement BLOCKED; retain hybrid |
| K12 | No Dock native integration/dual-display/performance evidence | D8/D9 NATIVE_PENDING |

No baseline, installed binary or owner-profile changes. No taskbar hiding/Explorer termination. No merge without OWNER_NATIVE_ACCEPTED.
