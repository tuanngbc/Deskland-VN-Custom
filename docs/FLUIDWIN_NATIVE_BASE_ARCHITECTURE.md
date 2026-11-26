# Native base, Phase 1

Owner directive 2026-11-27 replaces TLauncher-first integration. Application entry,
message loop, tracking, GPU, capture, input, Dock, Launcher, menus, Control Center,
native Settings and detached Guardian now come from the complete FluidWin 1.3.1
application in `native/fluidwin`, upstream commit
`2f773a8733b84c7dbee88fff6628d14e9ac802b9` (MIT, Copyright (c) 2026 Clooudy).

The original renderer, spring/easing constants and scheduling remain upstream.
There is one desktop core; the Rust runtime is not linked or packaged in Phase 1.
The full old source and branch remain available for later, owner-gated extensions.

Changes: TLauncher product/icon/version; audited window/property/message/mutex/input
identities; per-profile registry namespace and cursor storage; updater execution
removed; independent true-disable Notch switch alongside original fixed/activity
mode; UAT startup/wallpaper persistence guards. About retains source attribution.
`PROVENANCE.json` records imported hashes and categorized identity mappings.

Phase 1 is an isolated TEST executable. It refuses startup without a validated
profile marker. Launcher propagates that profile through inherited environment
to Guardian and GPU/DPI restarts. Profile registry:
`HKCU\Software\TLauncher\NativeBase\Profiles\<profile-name>`.
It never reads/writes original `Software\FluidWin` or stable `.deskland-rs`.
No production migration is performed. Upstream registry value names are retained
inside the new namespace to minimize changes; schema/migration to final unified
configuration remains Phase 4. Startup and static Windows wallpaper changes are
blocked in this UAT build. Security/privacy permissions remain controlled by Windows.

Notch disabled: window destroyed, drag-drop registration revoked, animation and
poll timers removed, DIB/Direct2D target/bitmaps/formats released. The original
media reader remains only as the shared Dock media source while Dock is active;
it is stopped when Dock closes. Control Center/sensor services retain ownership.
No new tracking/capture/global-hook/recovery engines are added.

Hosted UCRT64 GCC C++20/static runtime follows the upstream build recipe. Only
`.github/workflows/build-rust-test.yml` builds/packages; no local build, install,
runtime verification or benchmark is authorized. Phase 0 builds the exact untouched
upstream checkout separately. CI success is not owner visual/performance acceptance.
