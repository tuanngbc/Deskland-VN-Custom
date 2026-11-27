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
