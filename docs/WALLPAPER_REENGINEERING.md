# Phase 2 — native wallpaper re-engineering

Owner accepts c011c2b11b3d (Actions 45870002906), 11/11 UAT PASS.
The 2026-11-27 architecture update supersedes legacy module migration. Phase 3/4
remain gated. Accepted core, prior branches, candidates and production stay intact.

## Functional contract and audit

Reference: `src/main.rs`, `src/engine.rs`, `src/desktop.rs`, `src/wgl.rs`,
`src/palm_layout.rs`, `src/presentation.rs`, `src/weather.rs`, `src/solar.rs`, all
`src/themes/` and `prototype/themes.json`. These are specifications, not linked code.
The catalog has 16 scenes and scene-specific numeric/enumerated controls. Shader
interfaces include signed-distance fields around desktop icons, bounded actor
arrays, click effects, shared noise, clouds, image actors and palm sprites.
`cats` is CPU vector artwork, not a complete GPU scene: its fragment shader only
copies a CPU image. It requires an independent native implementation before parity
can be claimed. Shader-only compilation is insufficient to verify any scene.

Legacy ownership findings confirmed by source: shader compilation in
`engine::Prog::new/from_parts` can retain an already compiled vertex shader when
fragment compilation fails; failed program linking does not delete the program.
These cleanup paths are not carried over. The legacy desktop attachment contains
bounded synchronous Shell waits; the new attachment executes outside the core UI.
The old single-scene/panorama presentation is replaced with per-monitor viewports
in physical coordinates; no desktop-sized panorama texture is required.
Other suspected races/leaks are not treated as established defects without evidence.

Must preserve: all 16 scenes, interaction, customization, icons, weather (manual,
offline, shared live source), solar/shadows, FPS, pause/fullscreen/occlusion,
lock/display-off, monitor selection, independent/sync/panorama, hotplug and DPI.
Scene swaps and disabled state must release GPU resources. Errors must leave
native Shell, Dock and accepted effects operational.

## Renderer decision

D3D11 fits the accepted core but translating all audited GLSL and vector artwork
at once would introduce visual regressions. Phase 2 uses a newly owned C++ WGL
renderer for existing GLSL assets, without changing the core D3D device. No Rust
ABI is introduced. Only active scenes compile; programs,
textures, contexts, DCs and windows have explicit destruction on their owner thread.
No browser, legacy runtime, legacy module wrapper or additional desktop shell.

Windows 11 desktop composition cannot rely on WGL SwapBuffers into a layered
child window (the legacy source records this presentation constraint). The new
C++ presentation owner therefore uses standard WGL_NV_DX_interop with D3D11 and
DirectComposition when the driver supports it. That API pipeline is audited
algorithm reuse; no old Rust implementation is linked/copied. A distinct wallpaper
device is selected to match the GL adapter, so it cannot lock or reset the accepted
core's device. Unsupported interop uses a bounded three-slot asynchronous readback
queue and DIB presentation on the wallpaper thread, with its mode reported.

A cancellable wallpaper session owns its render thread, windows and context.
Core UI publishes bounded configuration/topology/weather/input snapshots and never
waits on frame rendering. Disabled defaults start no wallpaper thread or GL device.
Each monitor renders its own viewport in a shared scene coordinate system; negative
positions, portrait and mixed DPI are physical coordinates. Allocation limits apply
per surface and in aggregate. Weather uses the Dock's existing widget::Dati owner;
wallpaper does not issue another forecast/geocode request.

References: Microsoft [OpenGL pixel formats](https://learn.microsoft.com/en-us/windows/win32/opengl/pixel-formats),
[SwapBuffers](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-swapbuffers),
[session display power notification](https://learn.microsoft.com/en-us/windows/win32/power/power-setting-guids),
[session lock notification](https://learn.microsoft.com/en-us/windows/win32/termserv/wm-wtssession-change).

## Verification gate

Build and automated verification run only on GitHub-hosted Actions. Compare module
off/on against accepted baseline: startup, native Launcher/Settings/Notch, Guardian,
scene images, resources, hotplug/topology, pause and recovery. CI counters do not
establish GTX 1070 owner performance. No WALLPAPER TEST shortcut until the Phase 2
package is complete and passing. No local candidate execution or benchmark.

## Scene specifications and ownership

All rows share the bounded field/noise/GL owners, native input snapshot and
validated catalog parameter definitions. No legacy scene object is retained.

| Reference | Preserved behavior in new scene owner |
| --- | --- |
| meadow.rs, meadow-boy.js, meadow-cat.js | Icon cloud fields, moving clouds, click dandelions/puffs, boy atlas, native kite/ribbons/tether, kitten, show/hide controls. Atlas layout and kite fabric math are audited artwork reuse. |
| beach.rs | Icon shoreline, shallow/shoal/warp, palms/sun shadows, kayak target and birds, water ripples and sand marks. |
| maldives.rs | Turquoise shoreline, palms, fish schools/turtles/kayaks/birds and click response. |
| lake.rs | Icon banks and cloud reflection, koi, cast/wait/bite/catch/empty reel states, click ripples. |
| sky.rs | Day/dusk/night, clouds, birds/plane, moon/stars, daytime cloud puffs and nighttime click meteors. |
| ocean.rs | Shared clouds, boats/yachts/birds, waypoint attraction and ripples. |
| rainforest.rs | Rain/mist/wind, birds/fireflies, water ripple interaction. |
| mocchau.rs | Blossom/petals/mist/wind, birds and click petal burst. |
| tet_sakura.rs | Petals, lanterns, swallows and click burst. |
| cat_hill.rs | Bounded cats/poses/coats, pet happiness, yarn, icon avoidance, butterflies/leaves/tree shake. |
| stadium.rs | Two teams with per-team player count, passes and chasing, click target/shots/goals, field lighting/stripes. |
| aquarium.rs | Eight bounded goldfish, plants/light/bubbles, cast/approach/nibble/bite/hook/caught/flee states. |
| commute.rs | Time presets, moving/stopping bus/doors, horn/gust click feedback, wind/reeds/lights. |
| summer_porch.rs | Corgi poses/petting, fan stages, chime/bird hop, melon bites/refill, time and sunbeam controls. |
| cats.rs | Native GPU hill composition using audited cat artwork, moving/lazy cats/yarn/flowers/trees/sun controls. No old CPU vector renderer. |
| boxing_ring.rs | Position-dependent impacts, bounded springs/gloves, bags/tire, HUD/rounds, auto-spar, power/bounciness/style controls. |

Geometry uses read-only `IFolderView` queries on the existing Windows pool, one
coalesced cancellable job. It does not allocate or write memory inside Explorer.
The renderer receives the last complete geometry snapshot without waiting.
See Microsoft's [IFolderView::GetItemPosition](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifolderview-getitemposition)
and [GetSpacing](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifolderview-getspacing).

The scene simulations are re-engineered interpretations of those functional
contracts, not a claim of pixel-identical trajectories or certified owner parity.
CI proves nonconstant images, animation and a click-only difference at equal time
for every scene; owner comparison must still assess artwork, object hit regions,
mini-game timing and multi-monitor experience. Fullscreen/occlusion retain assigned
world state, while disabling/unassigning releases it. Topology reconstruction safely
rebuilds current scene resources and may reset actor trajectories.

Owner rejected Phase 2 candidate 027f437 despite technical CI success. The
subsequent 4cdbe4a run also revealed real composition defects on image inspection.
The correction and all-16 scene audit are recorded in `WALLPAPER_LAYER_AUDIT.md`.
Hosted verification and owner acceptance of the corrected candidate remain gates.
