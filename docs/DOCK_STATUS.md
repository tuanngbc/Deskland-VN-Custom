# Deskland Rust Dock + Stage Manager — execution status

Checkpoint: 2026-11-21 02:10 UTC. Development only on `codex/rust-dock`, [Draft PR #1](https://github.com/tuanngbc/Deskland-VN-Custom/pull/1).
Accepted source baseline remains `a604519acaf4d5881b9e13e5476d52f97ac29c0f` on `codex/rust-test`, clean. Owner subsequently authorized isolated native smoke, fixes and CI, without production replacement or merge. See [native report](DOCK_NATIVE_SMOKE_REPORT.md). Installed EXE bytes were preserved; their source lineage lacks a BUILD-COMMIT marker and is not independently confirmed.

## Verified source and CI

- D0 docs `45bacf4`: [44844337708](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44844337708), CI_PASS.
- Initial integration `16b73d0`: [44845761358](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44845761358), CI_FAIL (DWM attribute i32/u32 binding). Fixed in `3526fcc`.
- Foundation `3526fcc4b470f8f0191868eb043498e3a5a33929`: [44846159435](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44846159435), CI_PASS.
- At 3526fcc: 47 Rust passed, six ignored tests each explicitly run and passed (five prior native cases plus new integrated Dock/Stage lifecycle). Eight Dock model/geometry tests are included in the 47 and rerun separately. JS renderer 32, shortcuts seven, Dock Settings handlers four: all passed.
- Runner is Windows Server 2025, not owner Windows 11 dual-monitor acceptance. No numerical CPU/GPU/FPS improvement is claimed.
- Shell recovery increment `390bacb2d2be77fb81c544528a7fb8d20730293b`: [44846968455](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44846968455), CI_PASS. 47 Rust plus seven separately invoked native tests passed, JS 32+7+4. Includes documented shell-message routing, hidden-controller cleanup and prompt owner-loop wakeup; actual Explorer restart remains NATIVE_PENDING.
- Fullscreen/desktop regression `4b7500c`: [44847644244](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44847644244), CI_PASS. Exact package/checksums verified; original EXE not executed because Startup migration makes an unisolated trial unsafe.
- Trial isolation `cdc9dcf`: [44850847554](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44850847554), CI_PASS (49 ordinary Rust, seven separately invoked native tests, JS 32+7+4, release persistence guard check). Native smoke caught a Settings/Dock startup deadlock.
- Own-process title exclusion `fa93c16cec223bb54bc2d75906341a3ae5a39a59`: [44852504957](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44852504957), CI_PASS (49 ordinary Rust, eight explicitly invoked native tests including visible owner-window regression, JS 32+7+4, release isolation check). Artifact 14686511095 was the repaired binary actually tested on owner Windows 11.

## Actual implementation

- One managed native UI thread inside `tuanpc-desk.exe`, with readiness, command channel and partial configuration edits. No unmanaged Dock child or separate configuration app. Main joins it before removing its tray/config owner.
- Existing Settings has a Dock & Stage Manager page. Existing native fallback and Deskland tray also have independent wallpaper/Dock/Stage controls. Enabled modules start through normal Deskland startup, and new modules default off.
- One canonical settings.json. Legacy dock.json pins/order, scope and side migrate only if canonical Dock config is absent; old file retained. Atomic writes preserve existing and unknown fields and retain malformed existing files on failure.
- Wallpaper can release its renderer independently; widget startup/Settings detection works without wallpaper windows. Real renderer disable/re-enable acceptance still pending.
- Window events coalesce through an out-of-context hook; one 5-second recovery timer replaces per-monitor polling. Unchanged items do not repaint; cached Shell icons bounded and released. Monitor detection reused from main; work-area changes reconciled. Suspend hides overlays.
- Running-app grouping remains EXE-based. D1 locality regression fixed for both pinned/unpinned global apps; DWM-cloaked windows excluded, iconic windows use their restored monitor. Foreign show-state requests asynchronous.
- Stage has live DWM previews, protected/minimized fallbacks, session-local window groups and per-display layout controls. The owner-host fixture verified basic switching and placement recovery at d708b09. Visual acceptance, monitor changes and exact Z-order remain open.
- Windows taskbar/tray preserved. Wi-Fi, Bluetooth, sound, notifications and taskbar entries are WINDOWS_DELEGATED Settings links, not native system-status flyouts or third-party tray hosting.
- Explicit --trial-dir protects real Startup/saver/lock/wallpaper changes and uses a separate marked profile. Windows 11 smoke verified eight module combinations in native Settings, selected WebView controls, negative-coordinate two-monitor HWND geometry, preserved module HWNDs, saved startup and normal cleanup. This is partial acceptance, not D1–D9 completion. No performance improvement is established.

## Milestone gates

| Milestone | Implementation / automated evidence | Owner native |
|---|---|---|
| D0 architecture audit | CI_PASS at 45bacf4; resource/lifecycle review and benchmark protocol documented | NATIVE_PENDING measurement |
| D1 window engine | IN_PROGRESS; events/filter/local selection tested; AUMID/packaged identity, multi-window picker/reorder missing | NATIVE_PENDING |
| D2 Dock UI | IN_PROGRESS; alpha capsule, cached icons, magnification, auto-hide rail, keyboard entry and pin reorder; true blur and interaction polish pending | NATIVE_PENDING |
| D3 Stage | IN_PROGRESS; DWM previews and explicit groups verified on native fixtures; polish, richer editing and cross-display recovery pending | NATIVE_PENDING |
| D4 controls | IN_PROGRESS; min/max/restore/snap/resize; no reversible saved geometry/quadrants | NATIVE_PENDING |
| D5 multi-monitor/DPI | IN_PROGRESS; negative geometry and work-area handling; independent targets and Stage layouts verified at 96 DPI; mixed DPI and hotplug hardening pending | NATIVE_PENDING |
| D6 system | IN_PROGRESS; current entries WINDOWS_DELEGATED, further states PENDING | NATIVE_PENDING |
| D7 Compatibility | IN_PROGRESS; Windows taskbar retained | NATIVE_PENDING |
| D7 Replacement | BLOCKED; supported tray parity/AppBar/watchdog recovery unresolved | NATIVE_PENDING |
| D8 hardening/RC | IN_PROGRESS; 100-toggle handle counts still under investigation, no RC | NATIVE_PENDING |
| D9 integrated product | IN_PROGRESS; common executable/config/Settings/lifecycle foundation CI_PASS; independent controls and basic multi-display geometry verified; lifecycle coverage and owner gate pending | NATIVE_PENDING |

Native lifecycle CI verifies same PID, independent Dock/Stage toggles without replacing the other module HWND, suspension and destroyed overlays. Owner smoke at fa93c16 additionally observed basic integration, persistence and normal cleanup. Windows autostart, actual Explorer restart, GPU budget, mixed-DPI hardware, overlay app actions and long-run performance remain pending. First clock-on/WebView hang remains an investigation item. No OWNER_NATIVE_ACCEPTED and no merge.

Next: auditor review of [native smoke evidence and remaining checklist](DOCK_NATIVE_SMOKE_REPORT.md); reproduce clock-on/WebView case, then remaining overlay actions/performance/recovery. Finish D1 identity/multi-window model, D9 per-display/appearance/behavior in D2/D5, D3/D4/D6 and D7 safety review, then D8/RC. The milestone table's NATIVE_PENDING refers to each full gate; the report records narrower PASS cases.
See [architecture](DOCK_ARCHITECTURE.md), [acceptance matrix](DOCK_ACCEPTANCE_MATRIX.md), [known issues](DOCK_KNOWN_ISSUES.md), [handoff](DOCK_HANDOFF.md).


## Owner rebuild — M1 checkpoint

- [c2a589b](https://github.com/tuanngbc/Deskland-VN-Custom/commit/c2a589b3be946503f84ed5b81d6bef09d2ffe141): separate Dock/Stage All, Primary, Selected and Custom display targets; actual HWND filtering; WebView layout preview and native Settings fallback. Application scope retains its previous meaning. When no requested display remains connected, the runtime uses the primary temporarily and does not rewrite the saved IDs.
- [Windows CI 44856277842](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44856277842): CI_PASS — 50 ordinary Rust tests, nine individually invoked Windows tests including native display filtering and the existing title-read deadlock regression, JS 32+7+6, release isolation check and one x64 package. [Artifact 14688380242](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44856277842/artifacts/14688380242).
- Owner-host c2a589b: primary-only Dock plus all-display Stage geometry observed on the real landscape/portrait desktop. Actual WebView Settings and its display layout were captured. Device names appeared generic; [6ca3997](https://github.com/tuanngbc/Deskland-VN-Custom/commit/6ca3997f262eaa86c15e888843ea0eedbd8aaac1) queries DisplayConfig friendly names and paths, with an interface-name fallback. Verify that exact artifact separately.
- Gate remains open for explicit Stage-on-secondary only and Custom interactions, Win32 fallback interaction, persistence across a full relaunch, hotplug, and mixed DPI. These cases are not inferred from UI or CI. M2 proof is separate from the production render path; M3 motion and M4 previews/groups remain open. D7 remains blocked. No owner action until a candidate has visual and native evidence.

## Owner rebuild — current visual and window-management checkpoint

- M1: [6ca3997](https://github.com/tuanngbc/Deskland-VN-Custom/commit/6ca3997f262eaa86c15e888843ea0eedbd8aaac1), [Windows CI](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44857296325). Owner-host verified primary-only Dock and secondary-only Stage before and after relaunch; device names, resolutions and negative geometry observed. Live Custom/Win32 edits, DPI changes and physical hotplug remain unverified.
- M2: [f63d7f7 glass probe](https://github.com/tuanngbc/Deskland-VN-Custom/commit/f63d7f72ccf54c49e1eb81b0aa22499c3321fb46), [Windows CI](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44858071402). DWM requests succeeded but the backdrop rendered opaque with Windows transparency disabled. True alpha blending was visible on the owner host; blur was not established. [9512260](https://github.com/tuanngbc/Deskland-VN-Custom/commit/95122607a6f2401e5ad9d7fd2c0f11c1d7210b27), [Windows CI and package](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44859178929): layered capsule, configurable magnification and off-thread Shell icon atlas. Owner-host captured the actual Dock and used it to open Spotlight.
- M3: [863d8f0](https://github.com/tuanngbc/Deskland-VN-Custom/commit/863d8f04bd12a2ee8c7865265829f75192f2e939), [Windows CI](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44859601177). Auto-hide rail, keyboard entry via tray, and pin drag reorder implemented. Native interaction and motion pacing still need verification; slide/reveal effects and accessibility are incomplete.
- M4: [c4821cb](https://github.com/tuanngbc/Deskland-VN-Custom/commit/c4821cb53473f327d35e6b5da5f129d421f09aa2), [Windows CI](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44860070425). Added bounded DWM relationships and fail-closed fallback; subsequent d708b09 native test observed content changing in place. [3afc044](https://github.com/tuanngbc/Deskland-VN-Custom/commit/3afc044445f03cd4ffe8992c3b3761ee0e8b3692) adds session-local groups and placement recovery on the development branch. [Windows CI PASS](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44872383035): 54 regular Rust tests, 10 explicitly invoked Windows tests, JS 32+7+7, and release isolation check. The foreign-window test exercised quick Stage OFF before minimize delivery, group switching, unchanged pre-minimized/other-group windows, and normal/maximized placement restoration on the runner. That revision did not certify GUI group switching or Z-order; later evidence is in the native report.
- M5/M6: no matched performance PASS, no visual/owner PASS, no release candidate. PR [#1](https://github.com/tuanngbc/Deskland-VN-Custom/pull/1) stays Draft; stable a604519 and installed bytes are unchanged.

- Follow-up [4eb069c](https://github.com/tuanngbc/Deskland-VN-Custom/commit/4eb069c248b5393009e9b8a65bfceedfecbe32ea) adds separate Stage side, scale and spacing per display in both Settings paths. Its [Windows run](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44872708910) reproduced an intermittently stranded minimize on fast Stage OFF. [d708b09](https://github.com/tuanngbc/Deskland-VN-Custom/commit/d708b0967591ea403bbcd817f313713c2bdea4b9) waits for observed minimize before restoring. [Windows CI PASS](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44873346276) repeated the foreign-window regression three times, including fast Stage OFF, window switching and normal cleanup.
