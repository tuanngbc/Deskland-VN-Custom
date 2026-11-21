# Durable development handoff

Current P0 build and scoped native evidence: [Dock P0 verification](DOCK_P0_VERIFICATION.md). The full owner acceptance gates remain pending.

The final hardening increment adds a live DWM fixture and a controlled performance harness. Use the [sampler instructions](DOCK_PERFORMANCE_REPORT.md#controlled-sampler-and-stage-allocation-check) and keep fixture artifacts private. Verify matching foreground, coverage, monitor state and GPU engine before comparing intervals. The 240-switch CI workload measures preview relationships and insertion-map growth, not UI gestures, long-run stability or GPU savings. The first controlled run correctly rejected mismatched wallpaper coverage. For the next pilot, use a confirmed uninterrupted two-display session with both wallpapers revealed; start with wallpaper-only and floating Dock idle, then run the full repeated matrix only after the pilot succeeds. The verified 4fecfd8 landscape overflow test ended with production restored, no remaining overlays and work areas restored on both displays. The dcd607e AppBar replay result is a CI test, not a native Explorer restart.

For the P0 follow-up, continue from remote HEAD, not the older handoff commits below. Owner smoke evidence for 5f8d2e8 and 4b37064 is under the private trial folders ending in their short SHAs. The 4b37064 release is the last owner-tested executable. Dddd812 adds an exact CI crash probe (real Explorer, primary monitor, test-owned child). Do not stop Explorer or rerun that probe on the owner desktop. Keep the production and baseline unchanged, PR Draft, and leave `OWNER_NATIVE_ACCEPTED` unset. Next native evidence: actual Explorer restart in a recoverable Windows 11 session, alternate primary/mixed DPI, real app maximize and fullscreen paths, hover, utility navigation and repeatable resource sampling.

Checkpoint 2026-11-21 02:10 UTC. Only codex/rust-dock, [Draft PR #1](https://github.com/tuanngbc/Deskland-VN-Custom/pull/1), base codex/rust-test. Stable source a604519 remains clean in E:\deskland. No merge or production replacement; owner alone grants OWNER_NATIVE_ACCEPTED.

Latest owner attachment authorized download, isolated native smoke, fixes, push and GitHub-only builds. It superseded the former no-download/no-run restriction for that test; it did not authorize candidate deployment. Prior AI writer was paused. Fetch and verify HEAD before changes; never reset/rebase/force-push over another writer. Push only owner repository, never upstream origin.

## Exact build and evidence chain

1. Requested 4b7500c, [CI 44847644244](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44847644244), artifact 14684755056: checksums/BUILD-COMMIT/package passed, original EXE never run. Startup migration can redirect real Startup.
2. cdc9dcf adds marked --trial-dir storage, blocks Windows persistence/lock/saver changes, omits C# fallback, blocks trial child mini tools and CLI forwarding. [CI 44850847554](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44850847554) PASS: 49 Rust, seven explicitly invoked native tests, JS 32+7+4 and built release isolation checks. Owner smoke caught Settings/Dock deadlock; two hung attempts required scoped forced recovery after normal Exit failed.
3. Native-tested repair fa93c16cec223bb54bc2d75906341a3ae5a39a59, [CI 44852504957](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44852504957), artifact 14686511095. Skips own PID before GetWindowTextW; otherwise owner readiness wait and worker WM_GETTEXT deadlock. New visible owner-window regression. CI PASS: 49 ordinary Rust + eight separately invoked native tests, JS 32+7+4 and release isolation checks. Nine Dock model cases are included in 49 and rerun, not additional unique tests.
4. Later report commits are documentation only; never relabel fa93c16 native execution as execution of a later artifact. Full hashes and cases: [native report](DOCK_NATIVE_SMOKE_REPORT.md).

## Actual owner native scope

Windows 11 build 26200, GTX 1070, 2560x1080 primary landscape and 1080x1920 portrait at x=-1080/y=-731; both 100% DPI.

Observed fa93c16: Win32 Settings all eight Wallpaper/Dock/Stage combinations, existing module HWNDs retained for independent toggles, both OFF destroys controller. WebView independent Stage/Dock, all-monitor scope and right side applied. Two Dock + two Stage HWNDs belong to the Deskland process. Saved modules start without --dock. A copy of owner settings/history retains theme/mode/params/environment/widgets/hotkeys/launcher/system monitor/unknown sentinel. Dual-output panorama renderer captures inspected. Three normal Exits took 0.19/0.21/0.23 seconds; no candidate HWNDs before production restart, no trial WebView children afterwards.

Not accepted: overlay pin/unpin/application actions/locality semantics; Stage activation/desktop/fullscreen; tray interaction; mixed DPI/hotplug/primary changes; actual Explorer restart/login/reboot; long-run/leak/GPU budget/macOS visuals. At fa93c16 Stage was a sidebar; see newer d708b09 evidence below and in the native report. Helper targeted Settings but not every tool-window overlay; no full two-screen desktop capture or video. Short samples are uncontrolled and do not establish improvement. First cdc9dcf clock-on/WebView hang has no proven call site; do not close it merely because the Settings startup deadlock was fixed. Initial status can expose dockStopped.

## Recovery and safety

Private downloads, profile backup, cache evidence, screenshots, logs/counters and recovery scripts are outside git in the dedicated D9 test directory. Never publish without redaction. No local Rust/SDK install or build, Explorer kill, taskbar hiding or Windows security change.

Installed EXE bytes preserved. All 402 original profile files matched the verified backup immediately before final production restart. Scoped Startup/Desktop values matched without Registry imports. Derived images restored individually. Production WebView cache changed during a production-restored interval before retry; stopped-process cache moved intact to private evidence, verified original cache restored. Runtime images/cache may refresh after production runs. Final verification: correct production path/hash, settings.json hash and Startup; zero candidate/trial WebView processes. Installed EXE lacks BUILD-COMMIT and differs from available a604519 CI bytes: lineage gap, not proof of changed source. Never replace it to hide that gap.

## Next actions

1. Auditor reviews report and isolation/title-read fixes. Keep Draft and baseline untouched.
2. Reproduce clock ON + WebView interaction/overlap; fix initial status localization. Improve overlay accessibility/targetability; collect real two-screen app-action/video evidence.
3. D1 identity/AUMID/multi-window; D2/D5 appearance/per-display/DPI/behavior; D3 previews/workspaces; D4 reversible placement; D6 statuses. D7 replacement remains BLOCKED without safe tray/AppBar/recovery.
4. Matched reveal/cover measurements including DWM/WebView children, stress/long-run handles; then safe DPI/hotplug, Explorer recovery and login/reboot gates. Trial mode intentionally cannot certify Windows saver/Startup mutation.

Managed same-process threads are not crash isolation. First Shell icon extraction is synchronous and can delay joined shutdown; no bounded recovery claim. Sole build workflow .github/workflows/build-rust-test.yml. Preserve legal/third_party files; no binaries/logs/profiles/credentials in git. No merge/deployment without later owner authorization and OWNER_NATIVE_ACCEPTED.


## Owner visual rebuild checkpoint

Fetch the user repository before further edits; keep PR #1 Draft and stable a604519 unchanged. M1 landed in c2a589b and named display discovery in 6ca3997. See status/native report for exact artifact and verified scopes. The new `--trial-dock-displays` diagnostic writes inventory only inside the marked profile. Never upload its device paths; target IDs can contain per-device identifiers.

Latest matched-measurement attempt uses the pre-renderer fa93c16 release artifact in a fresh trial profile, native Dock/Wall settings, serial production/trial startup and private raw counters. Fixture coverage is sampled from the wallpaper log; a changed workload or missing GPU engine counters invalidates a performance comparison. Restore and verify production between groups of trials. Read [performance notes](DOCK_PERFORMANCE_REPORT.md) before starting a full benchmark or inferring any resource improvement.

The glass proof is opt-in on an isolated `--trial-dir` with the same singleton. It compares a documented DWM backdrop request and a true premultiplied capsule over the same high-contrast reference. Accepted HRESULTs are capability hints. The production GDI overlay does not select the proof. Resume with measured visual verification, then integrate an event-driven renderer, motion, and Stage thumbnails/groups; no visual or performance PASS yet.

## Next increment after reversible groups

Use the current [status](DOCK_STATUS.md) and exact-commit CI links instead of treating the original sidebar snapshot above as current. Workspaces are session-local and manage only explicitly enrolled, foreign application windows. d708b09 passed three CI foreign-window runs plus a native two-group switch/restore and changing DWM pixels from a benign fixture. Remaining: cross-display movement, UI group editing, mixed DPI, broader privacy/fallback coverage, and motion quality. Diagnose subsequent handle growth before proceeding to a release candidate. Keep proof separate from CI, keep the native trial profile isolated, and verify production rollback before leaving the desktop.

## Current handoff — floating Stage and utility host

Stage d2ba9e5 is the exact native-tested package, not the latest utility candidate. [CI](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44877602926) and [artifact](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44877602926/artifacts/14697175816) match; trial rollback verified EXE, profile and scoped Registry. On the test machine, Stage screenshots and diagnostics are in the private D9 evidence folder with suffix d2ba9e5-geometry. Do not publish raw logs, app names or display IDs. Run future builds on GitHub, verify both ZIPs and BUILD-COMMIT before native use, and return to production after the isolated trial.

Next: verify the exact utility package and both Settings paths, audio interaction on an authorized test endpoint, desktop/taskbar actions, compact layout, Stage motion and both-display video. Record device gaps and matched resource data rather than inferring results from CI. Keep draft and baseline unchanged.

The combined **CI-verified and smoke-tested** source is 099a4eb, Windows run 44879715526, artifact 14696479977. Use the evidence suffix 099a4eb-geometry for the scoped rollback, layout before/after/relaunch and cropped utility menu/slider. Native Settings smoke and read-only hardware observations are recorded in [status](DOCK_STATUS.md). The slider itself was not used to change OS volume. Next close Stage transition/video, Dock utility live/status controls and individual sizing, then benchmark with a stable fixture; do not repeat milestone claims based on a docs-only build.
