# v1.0.3 correction — full FluidWin scope

Owner instruction received 2026-11-26. Both candidates `442861d7ac77beaf247c7df3123bc92cef17ef70` and `f3bf1f5bcbf1b6eb8cbb62bcba1e9d88876de4bd` are **REJECTED**. Owner reports continuing colored window borders, insufficient smoothness, missing effects, Desktop/Explorer menus and utilities. Preserve these observations even when runner tests pass.

`OWNER_VISUAL_ACCEPTANCE=FAIL`, `ANIMATION_QUALITY=FAIL`, `COLORED_WINDOW_BORDER=UNRESOLVED`, `FLUIDWIN_FEATURE_PARITY=INCOMPLETE`.

The entire v1.0.3 master scope remains active. Only Dynamic Notch and Auto Update are excluded. No owner TEST handoff for an animation-only intermediate build. Internal artifacts are not accepted candidates and must not replace the Desktop shortcut. Production v1.0.2, its profile and startup remain untouched.

## Baseline gate

The sole workflow now has a separate hosted Windows baseline job. It checks out FluidWin commit `2f773a8733b84c7dbee88fff6628d14e9ac802b9`, compiles the unchanged application using its GNU toolchain/link libraries, and runs the original `genio::Motore` renderer without starting FluidWin's application entry point. Fixtures save the full canvas for white, black, RGB and premultiplied transparency across first/early/middle/near-Dock/final and reverse poses. Submission plus DWM flush timings are not measured display FPS. Missing hardware is explicitly BLOCKED, never visual PASS. This first probe does not cover capture, Drop, Jelly, hooks or Guardian; those remain separate work.

## Architecture correction

Baseline evidence: hosted job `fluidwin-baseline` in Actions run `45733231963`, source `a8764756e7210a9c8f3c7a38b1705157cb049dcd`. Original FluidWin application compiled; original renderer executed all 66 poses. The baseline archive digest is `8d4a87b79c855e79b3bcad0a78ce90b9585ee985f30c2c5d1ce3eb4a5a062e52`. It was retrieved and verified under the owner's UAT evidence directory, without executing its binaries locally. The independent TLauncher regression job is tracked separately. These tint fixtures do not establish capture quality, full upstream parity or owner acceptance.

The existing adapter reuses mesh/shader/Drop curves but retains CPU Stage capture, timer scheduling and native-animation fallback. It is not the full upstream engine. Compare original scheduling (`Present(1,0)` and an active message loop), `viva::Cattura` GPU copies and the complete window-event/handoff/recovery paths before replacing them. Preserve owner speed 0.70x.

Foreign mutation must have a recovery owner first. DWM transition suppression has no documented readable original state; do not assume FALSE is its original value. Layered style/alpha, identity, placement and focus require explicit recovery records and failure/termination coverage. A blocker on one mutation path does not stop independent menus, Dock or Control Center work.

## Evidence and release gates

CI evidence establishes only the listed fixtures on that runner. Colored-border root cause, rendered perimeter evidence, real presentation pacing, full motion features, Guardian/recovery, Desktop/Explorer menus, Dock previews/functions, utilities, accurate parity ledger, regression and resource checks are required before a comprehensive owner TEST. Owner acceptance remains FAIL until the owner confirms a later candidate. No publish, release tag, merge or production deployment is authorized.
