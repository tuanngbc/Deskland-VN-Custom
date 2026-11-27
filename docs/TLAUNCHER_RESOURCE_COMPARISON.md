# Resource comparison — no unmeasured claims

| Workload | CPU/RAM/GPU/startup/frame pacing |
| --- | --- |
| Untouched FluidWin 1.3.1 | Owner reports good experience; comparable instrumented values NOT_VERIFIED |
| Accepted native c011c2b11b3d | Owner functional acceptance 11/11; comparable owner measurements PENDING |
| Native + Wallpaper | Hosted serial accepted-baseline/disabled/enabled integration and exact main+Guardian sampling; use final run's `accepted-baseline-comparison.json` and three resource JSONs. Physical GPU/owner measurements PENDING |
| Native + Stage | WAITING_PHASE_3 |
| Native + Spotlight | WAITING_PHASE_3 |
| Full TLauncher | WAITING_PHASE_5 |

Targets remain 100–150 MB ordinary RAM, ~1% idle CPU, ~3% GPU; these are not results.
Count main + detached Guardian + helpers. Compare equal cold/warm startup,
Exclude user applications opened from the Dock from the TLauncher total; the
sampler restricts descendant membership to packaged binaries and cursor archive
helpers, alongside its separately identified Guardian.
first Dock/input/Launcher/animation/Settings, fullscreen, both displays, idle and
covered workloads. Do not trim memory, change visual quality or introduce delays.

Owner sampler is opt-in, no synthetic workload. RAM private working set is resident
private pages; commit/private bytes is separate. Shared working-set sums can double
count. CPU is normalized across logical processors. GPU is summed across owned PIDs
per physical engine then busiest engine per adapter, not a sum of unrelated engines.
Unavailable counters remain null. DWM and sampler overhead require separate Task
Manager observation; do not attribute whole-machine GPU load solely to TLauncher.

The hosted comparison downloads and verifies the exact c011c2b11b3d artifact,
runs one shell at a time with fresh isolated profiles, records first-observed and
warm message-window startup and bounded UI message round-trip latency. It does
not purge OS caches, so first-observed is not proof of cold startup. Enabled
wallpaper uses pinned hosted-only Mesa software GL: its CPU, RAM and frame pacing
cannot establish GTX 1070 hardware performance. Software DLLs are never packaged.
