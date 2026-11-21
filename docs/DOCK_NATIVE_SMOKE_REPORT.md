# D9 owner Windows native smoke report

Date: 2026-11-21. Development branch `codex/rust-dock`, [Draft PR #1](https://github.com/tuanngbc/Deskland-VN-Custom/pull/1). Compatibility Mode only. No merge, production replacement or OWNER_NATIVE_ACCEPTED.

## Source and package chain

- Requested source: `4b7500c63ab156a76506243f66b2d3c01d639376`; [CI 44847644244](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44847644244), SUCCESS.
- Requested artifact `14684755056`, `tuanpc-desk-Rust-win-x64`: outer SHA256 `3e72e29e774a12fa6d5eeaf81ec3a9cc86ed7cca4e59389658f13553f6d12eda` verified against GitHub artifact metadata.
- Inner `tuanpc-desk-Rust-4b7500c-win-x64.zip`: SHA256 `be48147fe12b72f8654fb687fbd124300dab67a8639ebe18caf36f62135d2719` matches supplied `.zip.sha256`.
- Exact BUILD-COMMIT verified, EXE SHA256 `78b1a5fb4db899c260c25b4b9ea7c9e246eaeae396011bc1f56881bf7f4b003e`. EXE, prototype/settings.html, themes.json, assets/widget, README, README-rust, DOC-TRUOC and four legal notices verified present.
- **Original 4b7500c EXE was not executed.** Source preflight confirmed `settings::migrate_autostart()` rewrites the real HKCU Startup path; extraction to a separate directory is insufficient.
- Isolation repair: [cdc9dcf09bc50302576a65873144799726d8db76](https://github.com/tuanngbc/Deskland-VN-Custom/commit/cdc9dcf09bc50302576a65873144799726d8db76), [CI 44850847554](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44850847554). Any native results for this repair must be attributed to its own artifact, never to the original 4b7500c binary.
- Isolation CI PASS at cdc9dcf: 49 ordinary Rust tests and seven ignored native tests explicitly invoked and passed. Nine Dock model tests are included in the ordinary count and rerun, not additional unique tests. JS 32+7+4 PASS. The built release EXE exercised persistence guards and invalid-path rejection with real Registry/profile snapshots unchanged.
- cdc9dcf artifact `14685785831` verified: outer SHA256 `6f8fcf8d7896a334052a180fc4fba5076cf9be490e1920832a1f87c36d67fe0d`, inner ZIP `04b23d00e2e244d17c50630e7f1c7b11d17745effe7c44f79ee4e437b9cfa449`, EXE `bcfa0a5ec150b374fc9842baacb6a1592ce75ed68b9ba7961c962b78e363a33d`. Package content and BUILD-COMMIT checked before execution. Local release isolation check also exited 0 with real settings unchanged.

## Host and preservation

Windows 11 Pro 10.0.26200. NVIDIA GeForce GTX 1070 (driver 32.0.15.8253), AMD Radeon integrated graphics also enumerated.

| Display | Bounds x,y,width,height | Work area | Primary |
|---|---|---|---|
| Landscape | 0,0,2560,1080 | 0,0,2560,1032 | Yes |
| Portrait | -1080,-731,1080,1920 | -1080,-731,1080,1872 | No |

Before testing, private evidence captured process path/PID/command line, EXE hash, all profile files (402 files, approximately 128 MB), profile hash manifest, Startup and Desktop Registry snapshots, and individual typed values plus live saver SPI state. Registry backup is evidence, not a whole-hive rollback import. Personal paths, coordinates of weather location, profile data and desktop content are not uploaded to GitHub.

Initial production PID 12808. Installed EXE SHA256 `e541dc662e81b9acb78b073967cdacb45b126fc56ca93ea8f2816fcb5bc24539`. Stable checkout is clean at `a604519acaf4d5881b9e13e5476d52f97ac29c0f`, but there is no installed BUILD-COMMIT marker. The EXE differs from the a604519 artifact in [CI 44746584679](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44746584679) (artifact 14634026011, EXE hash `00003786bfdd5771ca5c5c43c46a765154a1b7f25962d568bb37d280435e90a4`). This is a provenance gap, not proof of a source difference: builds need not be byte-reproducible. Exact installed bytes must be preserved; installed source lineage cannot be marked PASS.

Trial protection: explicit `--trial-dir`, absolute scoped path, ownership marker, resolved-path check against the production profile, no C# profile fallback, common data_dir for Settings/launcher/WebView/cache, guards on Registry writes/deletes and saver/lock/wallpaper operations, no trial mini-tool child, no trial CLI forwarding to a running installed owner. Normal singleton remains shared. Normal startup without the trial argument retains existing behavior.

## Results

First native attempt cdc9dcf: PID 17440, fresh isolated profile. WebView Settings opened in 785 ms, first wallpaper frame in 801 ms, both displays DPR 1.00 (100%). Wallpaper ON, Dock OFF, Stage OFF were read from the actual saved trial settings. The computer-use helper grouped the clock and Settings surfaces, causing screenshot-bound targeting errors; no incorrect click result is treated as a successful test. The process later stopped responding, stopped its 10-second statistics and failed normal Exit. That first hang's exact call site is unproven.

Second attempt cdc9dcf: clock disabled only in the isolated fixture to remove the helper's surface ambiguity. Win32 Settings was visible and accessible, PID 4232. Clicking its actual Dock checkbox froze the UI before the new value was saved. Source review found a concrete same-process deadlock: the owner blocks in Runtime::new waiting for readiness; worker enumeration calls GetWindowTextW on the visible Settings HWND owned by that blocked thread. Own-process GetWindowTextW synchronously sends WM_GETTEXT. Existing CI had no visible owner Settings, so it missed this path.

Repair [fa93c16cec223bb54bc2d75906341a3ae5a39a59](https://github.com/tuanngbc/Deskland-VN-Custom/commit/fa93c16cec223bb54bc2d75906341a3ae5a39a59) skips all own-process HWNDs before title access and adds an explicit native owner-window regression in the sole build workflow. [CI 44852504957](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44852504957) PASS: 49 ordinary Rust tests, eight separately invoked native tests (including the new owner-window case), JS 32+7+4 and the release isolation check. This is not owner acceptance.

**Actually tested repair artifact:** `14686511095`, run 44852504957, BUILD-COMMIT fa93c16 exactly verified. Outer SHA256 `a667f2ac27c0d941552f10f080d9448022144c0d3975e7e3174d626592617353`; inner ZIP SHA256 `0f826fd242319a7cf2e7741517de0a609b9939b3217fa71ef14ccba86bc064a1`; EXE SHA256 `d195a3b5309b593c8ee437972408df57b7157c41da660cf6710ff1828521746e`. Checksums and required contents passed before retry.

Both hung attempts received normal --quit first, then were terminated only after that failed (15-second exit wait; first also observed unresponsive for minutes). Exact path and PID were checked before terminating each candidate. Production was restored between attempts; no Explorer termination, baseline edits or unscoped process kill. Trial WebView children were absent after recovery. On a subsequent production restart only its generated wallpaper.bmp and lockscreen.jpg changed; these two derived files were restored individually from backup while production was stopped before rerunning the exact-profile recovery verification. No config file or Registry import was used to conceal a trial write.

PASS applies only to an observed case, not the entire milestone.

| Case | Result | Evidence / limit |
|---|---|---|
| Package provenance, checksums, contents | PASS | Exact requested artifact and inner ZIP verified locally |
| 4b7500c current-account isolation | FAIL | Confirmed unconditional Startup migration; original EXE not run |
| Installed baseline source lineage | NOT_OBSERVED | Stable source confirmed; installed EXE lacks marker and differs from available CI bytes |
| D9-A startup/process/defaults | PASS (limited) | cdc9dcf fresh modules OFF. fa93c16 saved modules auto-start without --dock; actual overlay owner PID matches Deskland |
| D9-B WebView and Win32 Settings | PASS (tested controls, fa93c16) | Dock/Stage toggled from both real Settings implementations, WebView scope and side changes applied |
| D9-B 8 module combinations | PASS (fa93c16, Win32) | See actual states below; cdc9dcf failure retained above, not erased |
| D9-B owner configuration preservation | PASS (fixture) | A copy of owner settings/history retains theme, mode, all theme parameters, environment, widget position/style, shortcuts, launcher appearance/history, system-monitor options and an unknown sentinel after startup/save |
| D9-C Dock on two displays/negative coordinates | PASS (geometry and ownership) | Two native Dock HWNDs at DPI 96, correct work areas, one same-process worker; running-app icons visible in main-screen Dock capture |
| D9-C pin/unpin, app activation/minimize/restore, locality semantics | NOT_OBSERVED | Local/all setting applied; individual overlay app actions not reliably targetable by the available desktop helper |
| D9-D independent Stage and left/right | PASS (controls and geometry) | Stage with Dock OFF on both displays; left and right positions verified in actual HWND bounds |
| D9-D desktop-click visibility/fullscreen suppression/activation | NOT_OBSERVED | Model/CI evidence exists, insufficient owner interactive evidence |
| Stage DWM previews/workspaces | FAIL | Not implemented; sidebar does not satisfy this feature |
| D9-E module independence/normal Exit/restart | PASS (fa93c16, limited) | Actual HWNDs retained across independent toggles; three normal exits in 0.19/0.21/0.23 seconds; no candidate HWNDs afterwards; saved right/all-monitor settings and modules restored on restart |
| D9-E tray-menu control | NOT_OBSERVED | Existing tray code retained; tray menu not exercised in this run |
| D9-F resource sample | PASS (measurement only) | Short observations below, no budget/leak conclusion |
| D9-F GPU budget, leaks, DPI change/hotplug/recovery | NOT_OBSERVED | No matched-load/long-run or mixed-DPI acceptance |
| Explorer restart/logout/reboot | NOT_OBSERVED | Gated behind initial smoke and verified recovery |

Actual fa93c16 Win32 UI sequence (bits = Wallpaper, Dock, Stage):

| State | Visible overlays | Actual owner PID | Result |
|---|---:|---:|---|
| 100 | 0 | 8580 | PASS |
| 110 | 2 | 8580 | PASS |
| 111 | 4 | 8580 | PASS |
| 101 | 2 | 8580 | PASS |
| 001 | 2 | 8580 | PASS |
| 011 | 4 | 8580 | PASS |
| 010 | 2 | 8580 | PASS |
| 000 | 0 | 8580 | PASS |

Existing Stage HWNDs survived Dock-off and Wallpaper-off. Existing Dock HWNDs survived Stage-off. Both overlays OFF destroyed the hidden controller as well. Wallpaper restored after 000→100 without changing scene parameters, hotkeys or the fixture sentinel. The clock was disabled in this interactive fixture only. A later launch cloned the real owner configuration, with the clock enabled, and all preservation comparisons passed; no Settings was opened in that last launch.

WebView retry PID 17788: independent Stage, right side, all-monitor scope and Dock all applied from the existing Settings page. Normal Exit with WebView open and all three modules ON took 0.21 seconds. A later launch, PID 13356, restored all modules, right side and all-monitor scope using only --trial-dir (plus the existing render snapshot diagnostic); no --dock, no separate app. Actual dual-output panorama render captures were saved and inspected, including palms/shadows on both landscape/portrait outputs. These are renderer BMPs, not full desktop screenshots containing Dock/Stage.

Short resource observations (20–21 seconds each; percentages are process CPU normalized by total logical CPU capacity):

| Context | CPU % | Working set MiB | Private MiB | Own 3D-engine mean % | GDI | USER |
|---|---:|---:|---:|---:|---|---|
| Restored installed production before retry | 0.629 | 123.14 | 252.79 | 2.103 | 26→26 | 55→55 |
| fa93c16, default fixture, all modules, WebView open | 0.635 | 121.91 | 233.27 | sampled privately | 87→87 | 76→76 |
| fa93c16, owner config copy, all modules, Settings closed | 0.829 | 117.50 | 263.55 | 3.629 | 85→85 | 48→48 |

Ambient desktop occlusion/workload was uncontrolled, the first candidate fixture differs from production, and the owner-fixture launch also exercised render readback snapshots. WebView subprocess RAM is not included in the Deskland parent working set. These observations do **not** demonstrate a performance improvement, satisfy a total-GPU budget, isolate Dock cost or prove absence of long-run leaks. The GPU values are this PID's 3D engine counter, not total GPU/DWM load. Renderer logs showed approximately 30 FPS when visible and pause/resume entries when covered; visual smoothness is not thereby accepted.

## Findings and remaining work

| ID | Priority | Finding | Disposition |
|---|---|---|---|
| N01 | P0 | Original candidate can redirect installed Startup and touch the real profile/Windows state | Mitigated by cdc9dcf explicit trial isolation; original 4b7500c never executed; real-account normal non-trial candidate launch remains unsafe |
| N02 | P1 | Dock enable with visible same-process Settings deadlocks on WM_GETTEXT while owner awaits readiness | Fixed fa93c16; dedicated native CI regression PASS and owner Win32/WebView toggle retries PASS |
| N03 | P1 investigation | First cdc9dcf WebView/clock-on attempt became unresponsive during helper surface/targeting errors | Exact call site unproven. Do not claim fully resolved by N02; clock-on + WebView overlap/interaction stress still required |
| N04 | P2 | Initial Dock status displayed raw dockStopped before later host updates | Dictionary contains translation; likely asynchronous initial localization. Reproduce/fix status refresh in next UI pass |
| N05 | Evidence | Installed EXE source lineage not independently confirmed | Exact bytes preserved; stable source checkout confirmed; owner baseline identity marker required before future deployment |

The helper only listed the Settings top-level window, not independent tool-window targets for every overlay. Its screenshot/accessibility state could lag after changes, and a WebView select popup belonged to the WebView child process. Such rejected or misdirected actions were not counted as tests. Coordinate/keyboard retries with actual config/HWND verification succeeded for the recorded controls. A Windows network Settings page opened during one unreliable click; it was closed without changing a Windows setting. No test video or full two-screen desktop capture was collected. Available evidence is native Settings screenshots, one visible Dock surface, dual-output renderer captures, actual HWND/config records and private logs/counters.

## Evidence and next gate

Private evidence/backups/downloads are outside the source repository under the dedicated D9 test directory. Public report contains sanitized results only. Windows desktop automation is available; no owner credentials/security dialogs are automated.

Final recovery at 02:08–02:09 UTC: original EXE hash verified, all 402 original profile files matched their preflight hashes before production restarted, and scoped Startup/Desktop Registry values matched without importing keys. Two derived wallpaper/lockscreen images had refreshed during production restarts. Production's WebView cache also changed during a production-restored interval before the fa93c16 WebView test; its modified-file audit includes timestamps in that earlier interval. The modified cache was moved intact to the private backup directory while all relevant processes were stopped, then the verified original cache was copied back. No personal cache was deleted or uploaded. Runtime cache/image updates after the restored application runs are expected and are not a claim of immutable runtime data.

At final verification production PID 16356 ran the exact original EXE; real settings.json hash and Startup path matched preflight; candidate processes and trial WebView children were zero. Final candidate Exit had left no TuanpcDesk HWNDs before production restart. Stable E:\deskland checkout remained clean at a604519. Startup launch after actual Windows reboot, Explorer restart, logout, hotplug and DPI changes were deliberately not attempted: broad native smoke is still incomplete. Trial mode intentionally blocks saver/lockscreen/Windows Startup mutation, so it cannot certify those features.

Next auditor/native checklist: review N01/N02 changes and sanitized evidence; reproduce N03 with clock ON; verify overlay app activation/pin/unpin/locality on both screens, Stage desktop/fullscreen behavior, tray controls and a short real toggle recording; measure matched reveal/cover CPU/GPU including DWM and WebView children; test many toggles/long-run handle trends; then mixed DPI/primary/hotplug, safe Explorer recovery and finally login/reboot. Keep Draft PR and no baseline merge until the owner supplies OWNER_NATIVE_ACCEPTED.


## 2026-11-21 M1 output targeting trial

Source [c2a589b](https://github.com/tuanngbc/Deskland-VN-Custom/commit/c2a589b3be946503f84ed5b81d6bef09d2ffe141), [CI 44856277842](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44856277842), [artifact 14688380242](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44856277842/artifacts/14688380242). Exact BUILD-COMMIT, both ZIP digests, EXE and packaged settings/assets verified; release isolation preflight passed. Fresh marked profile with Wallpaper/Dock/Stage ON at launch, Dock Primary and Stage All, no manual `--dock`.

| Actual visible overlay | Bounds left,top,right,bottom | Display |
|---|---|---|
| Dock | 1104,955,1456,1025 | Primary landscape |
| Stage | 8,160,128,720 | Primary landscape |
| Stage | -1072,-571,-952,-11 | Secondary portrait |

All overlays had the same Deskland owner PID and 96 DPI. Only the internal controller was hidden. WebView Settings screenshot showed both physical resolutions, primary status and the predicted overlay layout; the screenshot does not certify appearance of the overlay renderer. A drop-down opened, but the target change was not observed. Stage Secondary-only, Custom, editing in Win32, persistence on relaunch, physical hotplug and mixed-DPI interaction stay pending.

Normal exit and production restart succeeded. Bytes of the production EXE, every file in the fresh backed-up profile and the scoped Registry snapshot matched before restoration; no Registry writes or production replacement were needed. Private raw readings and screenshot remain outside the repository. No user action required for this checkpoint.

## Display targeting and actual Dock appearance

- 6ca3997: on the owner host, Dock was on the primary 2560×1080 LG and Stage was on the secondary 1080×1920 portrait output at (-1080,-731). Real Deskland HWNDs matched the saved targets before and after a normal relaunch. Both were 96 DPI; this does not certify mixed DPI.
- f63d7f7: the controlled stripe probe rendered a true alpha composite. Both DWM backdrop calls returned success, yet the live background was opaque with Windows transparency disabled. No Windows theme/Registry setting was changed.
- 9512260: an owner-host screenshot showed the actual native capsule and mouse magnification; clicking the Dock search icon opened Deskland Spotlight. A still cannot verify pacing or transitions. These trials ran serially with production; normal exit and rollback checks verified installed EXE, production profile and scoped Registry.
- No Stage DWM preview pixels, manual group actions, auto-hide, reorder, hotplug, fullscreen recovery or mixed-DPI interaction are certified by these observations. Raw screenshots, device identifiers and trial data remain private.
