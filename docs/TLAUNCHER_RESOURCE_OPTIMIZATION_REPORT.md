# TLauncher 1.0.2 — resource and motion audit

Owner baseline: approximately 600 MB RAM reported in Task Manager. Exact counter, child processes, workload and private bytes were not recorded, so this is not a comparable measured baseline. Owner targets: 100–150 MiB aggregate working set, GPU about 3%, average CPU below 1% on Ryzen 5 4600G / GTX 1070 / 2560×1080 + 1080×1920. GitHub hosted Windows cannot certify these targets.

## Architecture and evidence

| Module | Before (source limits, not measured resident memory) | After | Evidence |
|---|---|---|---|
| Decoded shared icon sources | 24 MiB plus active Arc owners | 12 MiB, unchanged 128 MiB disk tier | bounded source LRU |
| Derived launcher rasters | 16 MiB plus references | 8 MiB; visible-page references only; smaller source tier and 24-item bounded prewarm | source/cache and visible page tests |
| Stage cached sources | 40 MiB, 80 entries, 360 px thumbnails | 12 MiB, same thumbnail resolution; bounded age | cache eviction tests |
| Stage projected cards | 28 MiB | 12 MiB, current output geometry | projection/native Stage fixtures |
| WGC device | 15 s idle retention | 3 s idle retention; one bounded capture job; no capture while hidden | WGC worker tests |
| Fullscreen foreground | DIB released on dismiss; DirectComposition detached | also release Direct2D target, explicit D3D context cleanup; page refs released | 100 native toggles and memory counters |
| Panorama | per-monitor intermediate scene texture even on direct GPU path | remove duplicate scene texture/FBO on direct GPU panorama; retain compatibility/test allocation and native output | source audit; existing panorama and presentation contracts |
| Settings | lazy WebView2; controller Close on HWND destruction | preserve existing explicit lifecycle | coordinator fixture; owner diagnostic verifies child-process memory |
| Wallpaper / clocks / monitoring | 30 FPS default; occluded outputs pause; GDI output buffers released after 10 s; widgets/monitor sampled on slower cadence | preserve visibility and scheduling, no resolution or FPS reduction | coverage and suspension tests |
| Capture/recording | session-scoped pipeline | preserve; recording tracked as separate workload | existing recording tests |

Limits are caps on tracked payloads, not measurements of process working set. The same Arc may be held in several maps; active references and allocator/driver memory can exceed a map's nominal cap. Actual 600 MB attribution remains unverified. No working-set trimming or process termination has been added. Wallpaper scene simulations, OpenGL/D3D driver residency, WGC staging/texture residency and WebView2 are probable substantial costs; the owner report does not establish which dominates.

### Menu alpha and typography

Confirmed source defect: GDI/Direct2D alpha is not a defined premultiplied alpha channel. Previously the menu's stretch-drawn background was used as a per-pixel compositing target without establishing opaque alpha; the text DC used an alpha-ignore Direct2D target on that same surface. The repair creates the frosted background once with explicit BGRA alpha = 255, clears the glyph buffer per row, composites grayscale coverage, and applies the rounded alpha mask last. It also initializes Layer DIB memory and synchronizes GDI. Native CI checks a poisoned buffer repaint and premultiplied invariants; exact owner stripe reproduction and visual acceptance remain pending. No claim that ClearType caused the reported stripes.

Fullscreen uses ivory labels with soft grayscale coverage shadows, a native EDIT field in a frosted rounded field, darker contrast tint on real HostBackdrop, semantic colors for normal themes and Windows system colors for High Contrast. The native edit retains caret and selection. CI synthetic backdrop is not owner visual acceptance. Default global motion is 0.70; saved 0.75 and other values are preserved.

### Window transitions

DWM thumbnails are live links to a source window; a successful registration/property update does not prove the pixels remain valid after minimize. See [DWM thumbnail overview](https://learn.microsoft.com/en-us/windows/win32/dwm/thumbnail-ovw), [update properties](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmupdatethumbnailproperties), [thumbnail properties](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ns-dwmapi-dwm_thumbnail_properties). Stage's WGC source is downsampled and intended for small previews. Reusing it for full-size minimize/restore would produce low-resolution or stale content, and a DWM proxy would compete with native Windows animation. No production custom animation is enabled until the backend can reliably prove usable frames, interruption, placement, activation and secure/unsupported-window fallback. Existing native minimize/restore remains the safe path.

## Owner measurement

Launch using the Desktop TEST shortcut. In the candidate directory double-click REPORT-RESOURCE.cmd, enter a scenario label, and use TLauncher normally for the sample interval (60 seconds). It writes a local diagnostics JSON, including the main and observed descendant processes, working set, private working set where available, private bytes, CPU, thread/handle counts, GPU engine counters and GPU process memory where supported. No screenshots, telemetry, installations or candidate execution by the diagnostic. Working-set totals may double-count shared pages; DWM is excluded and raw GPU engine counters are not a global GPU percentage. Child identity uses parent PID plus creation time. For accurate steady-state compare repeated equal scenarios, including desktop both displays, one/all covered, Compact and Fullscreen, Start, Settings before/during/after, Stage, animation, screenshot/recording, and long idle. For reference baseline use the previous candidate if the owner chooses to run it under the same conditions.

CI fixture evidence is distinct from the owner hardware gate. Until measured on owner hardware: RAM/GPU/CPU targets, module-level resident memory, full matrix and frame pacing are NOT_VERIFIED. OWNER_PERFORMANCE_ACCEPTANCE=PENDING; OWNER_VISUAL_ACCEPTANCE=PENDING; PUBLIC_RELEASE=NOT_PUBLISHED.
