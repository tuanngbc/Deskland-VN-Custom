# Known issues and safety limits

Updated after owner smoke at `fa93c16`, CI 44852504957, 2026-11-21. See [native report](DOCK_NATIVE_SMOKE_REPORT.md). CI fixes and partial native PASS do not close the full owner gate.

| ID | Issue | Disposition |
|---|---|---|
| K01 | Old unmanaged child/config | Replaced by managed thread/canonical settings; isolated same-PID/toggle/cleanup CI_PASS. Autostart/Windows restart NATIVE_PENDING |
| K02 | Old RefCell reentrancy hazards | Snapshot/detached state before Win32; isolated lifecycle CI_PASS. Full menu/hung-app stress NATIVE_PENDING |
| K03 | Unpinned global activation locality | Regression CI_PASS at 3526fcc; interactive focus NATIVE_PENDING |
| K04 | Per-bar polling/paint extraction | Replaced by coalesced hooks, one recovery timer, change-only invalidation and bounded icon cache. First icon extraction still synchronous; D2 worker/cache refinement pending |
| K05 | Packaged/AUMID identity incomplete | High, D1; cloaked filtering and minimized monitor fallback implemented, native real apps pending |
| K06 | Dynamic DPI/stable per-monitor preferences incomplete | High, D5; work-area geometry now reconciled |
| K07 | Stage cross-monitor movement, exact Z-order and advanced workspace editing are not yet verified | High, D3 |
| K08 | Opaque UI, limited items, no real magnification/keyboard/reorder | D2 |
| K09 | Borderless is resize, no original placement saved | D4; never call true fullscreen |
| K10 | Settings links are not status widgets/native flyouts | D6, WINDOWS_DELEGATED |
| K11 | No safe third-party tray host/AppBar/recovery evidence | D7 Replacement BLOCKED; retain hybrid |
| K12 | No Dock native integration/dual-display/performance evidence | D8/D9 NATIVE_PENDING |
| K13 | Unisolated candidate Startup/profile mutations | P0 trial risk; cdc9dcf trial mode/release guard CI PASS. Never run trial without --trial-dir on owner's account |
| K14 | Visible own Settings deadlocks Dock startup/title enumeration | P1 fixed fa93c16, dedicated native CI and Win32/WebView owner retry PASS |
| K15 | First clock-on/WebView attempt hung during helper surface errors | P1 investigation; exact call site unproven. Clock-on + Settings stress needed |
| K16 | Initial Dock status can show raw dockStopped | P2 initial localization refresh investigation |

K12 clarification: fa93c16 owner smoke verifies basic same-PID/two-monitor geometry, eight toggle combinations, configuration preservation, saved startup and normal Exit. Overlay actions, mixed DPI/hotplug, real shell/login recovery, matched performance and long-run evidence remain pending. Short samples are not budget acceptance. Same-process integration provides no process crash isolation. A Shell icon handler could block the UI worker and delay joined shutdown: resolve/test before RC; no bounded fault-isolation guarantee.

No baseline/installed binary replacement. Owner state restored and verified, including reviewed runtime cache/image changes during production restarts. No taskbar hiding/Explorer termination. No merge without OWNER_NATIVE_ACCEPTED.


## Owner rebuild observations

- Generic monitor labels on the first Windows 11 Settings capture: c2a589b showed both real resolutions and primary state but named both "Generic PnP Monitor". DisplayConfig detection added in 6ca3997; recheck on the owner's hardware.
- Output identity is per-monitor device path where available. Remote/virtual outputs may fall back to their GDI source name, whose identity is not guaranteed across reconnect. Mirroring is one drawable desktop source and must not spawn duplicate overlays.
- First owner trial observed settings and real geometry, but coordinate injection into the WebView2 drop-down hit its separate Chrome window. Do not count that attempt as a successfully applied target. Recheck via native fixture or keyboard and record the saved mode and actual HWNDs.
- The old GDI Dock and Stage sidebar continue to run during M1. Native glass probe success cannot certify the Dock, and successful DWM API calls alone do not certify blur.

## Composition and Stage limitations

- Owner-host Windows transparency is off. DWM backdrop calls can succeed while rendering opaque. The current Dock alpha fallback genuinely blends with the desktop but does not blur it.
- Stage uses documented DWM thumbnails only when visibility, uncloaked status, and display affinity are verified. Windows may reject affinity queries for ordinary non-layered sources; those cases intentionally show a fallback rather than attempting another capture path.
- Workspace management is session-local. A hung/elevated window may reject or delay placement; normal Deskland exit submits restoration, but forced process termination and exactly preserving Windows focus/Z-order are not established. Multi-monitor moves and richer editing need separate verification.
- Initial status has a Vietnamese fallback before localization loads. Clock-on/WebView hang and Shell provider shutdown bounds still need owner-host evidence.
