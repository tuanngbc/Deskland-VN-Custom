# Durable development handoff

UTC checkpoint: 2026-11-21 01:00. Branch `codex/rust-dock`, [PR #1 Draft](https://github.com/tuanngbc/Deskland-VN-Custom/pull/1), base `codex/rust-test`. Accepted baseline `a604519acaf4d5881b9e13e5476d52f97ac29c0f` untouched in E:\deskland.

Owner assigned implementation/commit/push/CI in 21/11 handover, superseding old read-only request. Previous AI writer paused. D9 is mandatory: same Deskland executable/config/Settings/startup/lifecycle, independent modules, backwards compatibility and one bundle. No merge or production deployment without OWNER_NATIVE_ACCEPTED. No repeated owner tests before RC.

## Verified commits and tests

- `45bacf4`: D0 audit + five living docs. [44844337708](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44844337708) CI_PASS.
- `16b73d0`: D1/D9 foundation, [44845761358](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44845761358) CI_FAIL on DWM attribute cast.
- `3526fcc4b470f8f0191868eb043498e3a5a33929`: cast corrected and independent Stage HWND retention, [44846159435](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44846159435) CI_PASS. 47 Rust + six separately invoked native tests, JS 32+7+4 passed; Dock model tests eight included in 47.

Foundation: managed same-process UI thread in src/dock/runtime.rs; canonical options/migration in src/dock/options.rs; config_store atomic preserving writes; Settings/tray/fallback integration, independent wallpaper switch; coalesced hooks, cached icons, work-area reconciliation; local/unpinned selection + iconic/cloaked fixes. No separate Dock process or config writer. Default new modules off. Stage still sidebar only.

Shell recovery source `390bacb2d2be77fb81c544528a7fb8d20730293b` passed [44846968455](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44846968455): 47 Rust plus seven explicitly run native tests; JS 32+7+4 passed. Adds TaskbarCreated routing, session cleanup, owner-loop wakeup, hidden-controller assertion and isolated shell-message test. Wallpaper startup failure no longer exits other desktop utilities; wallpaper-off frees staging memory. Actual native Explorer restart NOT tested.

The following increment (commit containing this entry) fixes independent Stage incorrectly treating desktop foreground as fullscreen by sharing the Dock/Shell exclusion helper. Adds regression for virtual desktop spanning negative-coordinate monitors, own/Shell classes and actual fullscreen vs work-area maximize. Requires own CI, not 390bacb's PASS.

## Immediate next actions

1. Verify actual branch HEAD and latest increment CI; fix failures before PASS. Record actual tested SHA/run/artifact. No candidate EXE downloaded to owner PC.
2. D1 packaged/AUMID identity, proper multi-window model/selection/pin reorder and stale/hung HWND safeguards.
3. D2/D5 + remaining D9: per-display appearance/size/position/auto-hide/config controls and DPI/hotplug handling; actual glass/motion/keyboard/drag workflows.
4. D3 real DWM previews/workspaces/privacy and D4 reversible placement/controls.
5. D6 real classified statuses, D7 compatibility + supported tray/recovery strategy review, D8 full regression/performance/RC.

All Windows 11 owner dual-display/Windows autostart/actual Explorer restart/wallpaper toggle/performance/long-run acceptance NATIVE_PENDING. Same-process threads avoid orphan children, not process crash isolation. Initial Shell icon lookup remains synchronous; a blocking Shell handler may delay joined shutdown: refine/reuse launcher icon pipeline and stress-test before RC. D7 Replacement BLOCKED without safe tray parity; no private Shell hook, Explorer kill or taskbar hiding.

OWNER ACTION: NONE for current foundation. Do not call whole D1/D9 completed. Next session fetch and read this file + actual HEAD first. Push only owner repository, never upstream origin. No force/history reset/main/baseline edits. Build only through .github/workflows/build-rust-test.yml on GitHub; no local Rust/SDK install or build, no trial EXE download/run. Keep legal files and owner profile unchanged; no credentials/target/logs in git.
