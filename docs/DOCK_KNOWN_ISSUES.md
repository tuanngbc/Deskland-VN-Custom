# Known issues and safety limits

Audit source `1d5d4c99`; foundation evidence `3526fcc`, CI 44846159435, 2026-11-21. CI fixes do not close the Windows 11 owner gate.

| ID | Issue | Disposition |
|---|---|---|
| K01 | Old unmanaged child/config | Replaced by managed thread/canonical settings; isolated same-PID/toggle/cleanup CI_PASS. Autostart/Windows restart NATIVE_PENDING |
| K02 | Old RefCell reentrancy hazards | Snapshot/detached state before Win32; isolated lifecycle CI_PASS. Full menu/hung-app stress NATIVE_PENDING |
| K03 | Unpinned global activation locality | Regression CI_PASS at 3526fcc; interactive focus NATIVE_PENDING |
| K04 | Per-bar polling/paint extraction | Replaced by coalesced hooks, one recovery timer, change-only invalidation and bounded icon cache. First icon extraction still synchronous; D2 worker/cache refinement pending |
| K05 | Packaged/AUMID identity incomplete | High, D1; cloaked filtering and minimized monitor fallback implemented, native real apps pending |
| K06 | Dynamic DPI/stable per-monitor preferences incomplete | High, D5; work-area geometry now reconciled |
| K07 | Stage lacks DWM previews/workspaces/placement restoration | High, D3 |
| K08 | Opaque UI, limited items, no real magnification/keyboard/reorder | D2 |
| K09 | Borderless is resize, no original placement saved | D4; never call true fullscreen |
| K10 | Settings links are not status widgets/native flyouts | D6, WINDOWS_DELEGATED |
| K11 | No safe third-party tray host/AppBar/recovery evidence | D7 Replacement BLOCKED; retain hybrid |
| K12 | No Dock native integration/dual-display/performance evidence | D8/D9 NATIVE_PENDING |

K12 clarification: isolated Windows CI lifecycle evidence exists at 3526fcc; owner dual-display/renderer-toggle/performance evidence is still absent. Same-process integration avoids orphans but provides no process crash isolation. A Shell icon handler could block the UI worker and delay a joined shutdown: resolve/test before RC, do not claim a bounded fault-isolation guarantee.

No baseline, installed binary or owner-profile changes. No taskbar hiding/Explorer termination. No merge without OWNER_NATIVE_ACCEPTED.
