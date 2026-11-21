# Deskland Rust Dock + Stage Manager — execution status

Checkpoint: 2026-11-21 01:00 UTC. Development only on `codex/rust-dock`, [Draft PR #1](https://github.com/tuanngbc/Deskland-VN-Custom/pull/1).
Accepted baseline remains `a604519acaf4d5881b9e13e5476d52f97ac29c0f` on `codex/rust-test`. No owner profile/installed EXE change, candidate download/run or merge.

## Verified source and CI

- D0 docs `45bacf4`: [44844337708](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44844337708), CI_PASS.
- Initial integration `16b73d0`: [44845761358](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44845761358), CI_FAIL (DWM attribute i32/u32 binding). Fixed in `3526fcc`.
- Foundation `3526fcc4b470f8f0191868eb043498e3a5a33929`: [44846159435](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44846159435), CI_PASS.
- At 3526fcc: 47 Rust passed, six ignored tests each explicitly run and passed (five prior native cases plus new integrated Dock/Stage lifecycle). Eight Dock model/geometry tests are included in the 47 and rerun separately. JS renderer 32, shortcuts seven, Dock Settings handlers four: all passed.
- Runner is Windows Server 2025, not owner Windows 11 dual-monitor acceptance. No numerical CPU/GPU/FPS improvement is claimed.
- Shell recovery increment `390bacb2d2be77fb81c544528a7fb8d20730293b`: [44846968455](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44846968455), CI_PASS. 47 Rust plus seven separately invoked native tests passed, JS 32+7+4. Includes documented shell-message routing, hidden-controller cleanup and prompt owner-loop wakeup; actual Explorer restart remains NATIVE_PENDING.
- This following increment shares fullscreen detection between Dock and independent Stage, excludes desktop/Shell windows, and adds a regression so clicking desktop cannot hide Stage. It requires its own CI; consult actual HEAD/run before selecting artifacts.

## Actual implementation

- One managed native UI thread inside `tuanpc-desk.exe`, with readiness, command channel and partial configuration edits. No unmanaged Dock child or separate configuration app. Main joins it before removing its tray/config owner.
- Existing Settings has a Dock & Stage Manager page. Existing native fallback and Deskland tray also have independent wallpaper/Dock/Stage controls. Enabled modules start through normal Deskland startup, and new modules default off.
- One canonical settings.json. Legacy dock.json pins/order, scope and side migrate only if canonical Dock config is absent; old file retained. Atomic writes preserve existing and unknown fields and retain malformed existing files on failure.
- Wallpaper can release its renderer independently; widget startup/Settings detection works without wallpaper windows. Real renderer disable/re-enable acceptance still pending.
- Window events coalesce through an out-of-context hook; one 5-second recovery timer replaces per-monitor polling. Unchanged items do not repaint; cached Shell icons bounded and released. Monitor detection reused from main; work-area changes reconciled. Suspend hides overlays.
- Running-app grouping remains EXE-based. D1 locality regression fixed for both pinned/unpinned global apps; DWM-cloaked windows excluded, iconic windows use their restored monitor. Foreign show-state requests asynchronous.
- Stage remains an initial app sidebar. No DWM thumbnails, actual workspace switching/restoration or macOS visual acceptance.
- Windows taskbar/tray preserved. Wi-Fi, Bluetooth, sound, notifications and taskbar entries are WINDOWS_DELEGATED Settings links, not native system-status flyouts or third-party tray hosting.

## Milestone gates

| Milestone | Implementation / automated evidence | Owner native |
|---|---|---|
| D0 architecture audit | CI_PASS at 45bacf4; resource/lifecycle review and benchmark protocol documented | NATIVE_PENDING measurement |
| D1 window engine | IN_PROGRESS; events/filter/local selection tested; AUMID/packaged identity, multi-window picker/reorder missing | NATIVE_PENDING |
| D2 Dock UI | IN_PROGRESS; opaque GDI, cached icons; glass/motion/auto-hide/keyboard/drag/accessibility missing | NATIVE_PENDING |
| D3 Stage | IN_PROGRESS; app sidebar only, no real previews/workspaces | NATIVE_PENDING |
| D4 controls | IN_PROGRESS; min/max/restore/snap/resize; no reversible saved geometry/quadrants | NATIVE_PENDING |
| D5 multi-monitor/DPI | IN_PROGRESS; negative geometry and work-area handling; stable per-display options/DPI/hotplug hardening missing | NATIVE_PENDING |
| D6 system | IN_PROGRESS; current entries WINDOWS_DELEGATED, further states PENDING | NATIVE_PENDING |
| D7 Compatibility | IN_PROGRESS; Windows taskbar retained | NATIVE_PENDING |
| D7 Replacement | BLOCKED; supported tray parity/AppBar/watchdog recovery unresolved | NATIVE_PENDING |
| D8 hardening/RC | NOT_STARTED; development packages only, no RC | NATIVE_PENDING |
| D9 integrated product | IN_PROGRESS; common executable/config/Settings/lifecycle foundation CI_PASS; appearance/size/position/auto-hide/per-display controls still pending | NATIVE_PENDING |

Native lifecycle CI verifies same PID, independent Dock/Stage toggles without replacing the other module HWND, suspension and destroyed overlays. It does not prove Windows autostart, actual Explorer restart, wallpaper GPU cleanup, mixed-DPI hardware or long-run performance. No OWNER_NATIVE_ACCEPTED and no merge.

Next: finish D1 identity/multi-window model, continue D9 per-display/appearance/behavior in D2/D5, then D3/D4/D6 and D7 safety review, D8 regression/performance/RC. No owner test requested before suitable RC.
See [architecture](DOCK_ARCHITECTURE.md), [acceptance matrix](DOCK_ACCEPTANCE_MATRIX.md), [known issues](DOCK_KNOWN_ISSUES.md), [handoff](DOCK_HANDOFF.md).
