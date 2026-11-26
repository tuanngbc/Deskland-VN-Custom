# Resource comparison — no unmeasured claims

| Workload | CPU/RAM/GPU/startup/frame pacing |
| --- | --- |
| Untouched FluidWin 1.3.1 | Owner reports good experience; comparable instrumented values NOT_VERIFIED |
| Branded native Phase 1 | Hosted functional smoke only; owner measurements PENDING |
| Native + Wallpaper | WAITING_PHASE_2 |
| Native + Stage | WAITING_PHASE_3 |
| Native + Spotlight | WAITING_PHASE_3 |
| Full TLauncher | WAITING_PHASE_5 |

Targets remain 100–150 MB ordinary RAM, ~1% idle CPU, ~3% GPU; these are not results.
Count main + detached Guardian + helpers. Compare equal cold/warm startup,
first Dock/input/Launcher/animation/Settings, fullscreen, both displays, idle and
covered workloads. Do not trim memory, change visual quality or introduce delays.

Owner sampler is opt-in, no synthetic workload. RAM private working set is resident
private pages; commit/private bytes is separate. Shared working-set sums can double
count. CPU is normalized across logical processors. GPU is summed across owned PIDs
per physical engine then busiest engine per adapter, not a sum of unrelated engines.
Unavailable counters remain null. DWM and sampler overhead require separate Task
Manager observation; do not attribute whole-machine GPU load solely to TLauncher.
