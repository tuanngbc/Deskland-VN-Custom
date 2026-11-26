# Owner-gated migration (all v1.0.3)

Baseline before migration: old development branch
`codex/fluidwin-integration-v1.0.3` at `e887c52` (tested code `3f4197c`), main
`fa234a03a71807c86fb2587df3e6d1cd74708803`, tag `v1.0.2` object
`8bff471ae3729aa22b355a7058ef6a0a31fd191c`. No histories/tags/releases rewritten.
Remote owner main observed before handoff is
`e9fa8ff52e773a2d5ff9b383af6973ca7281103a`; the `v1.0.2` tag resolves to
`4652e7f5b0d76fc4a1c73328cf18547a410305f4`. Local main above is a stale local ref,
not the owner remote's current main. Neither ref is changed by the migration.
The third integration candidate is rejected per the new directive. Old evidence,
profiles and owner rejection records remain authoritative and preserved.

New branch: `codex/fluidwin-native-base-v1.0.3`, based on preserved old HEAD.

| Phase | Deliverable | Gate |
| --- | --- | --- |
| 0 | Untouched FluidWin reference build and technical evidence | Hosted CI |
| 1 | Branded native foundation, true Notch switch, updater removed, isolated TEST | **Stop for direct owner acceptance** |
| 2 | Wallpaper (16 themes), displays/panorama, weather/solar | Owner must first accept Phase 1 |
| 3 | Stage/Peek and Compact/Fullscreen Spotlight, shared services | Retain upstream Launcher |
| 4 | Clocks, screenshot editor, recorder, resource indicators, migration/localization | Preserve all stable features |
| 5 | Consolidation, full parity/recovery/performance/UAT | Owner acceptance before release |

Only Auto Update is excluded. Dynamic Notch is included, reversing the prior
exclusion. Do not resume the old integration engine or move missing work to 1.0.4.
No Phase 2 work before explicit owner acceptance of the Native Base TEST.
