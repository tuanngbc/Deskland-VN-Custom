# Stage: default perspective

The former one-shot command rendered one group. It rejected a group when even one source was unavailable and discarded its bitmap on card/layout changes. The Stage refresh now builds targets for each visible card on each selected output, then runs one cross-output scheduler before linking per-layer DWM fallbacks.

For eligible layers, WGC obtains the first available window frame, omits the pointer, copies at most a 360px thumbnail into RAM and closes its capture session. The existing inverse pinhole mapper tilts both the outline and the UVs, bilinearly samples the frame, fits portrait sources with a matte and mirrors geometry (not text) on the right. Geometry and hit tests use the same projected silhouette. The shell's normal capture indicator remains under Windows control.

[StageDock-Windows d8ca4de](https://github.com/ImmanuelDeo/StageDock-Windows/tree/d8ca4de0b22e1c083b91aa00f344622d9f1bd370) was compared at `src/cards.cpp::render3DWindowSlab` (MIT, Y-axis rotation and textured projected slab) and `src/platform.cpp` (source handling). Deskland retains its independent renderer and does not copy the desktop to obtain a window image. Direct GPU warping might help if the bounded CPU raster proves expensive, but would add synchronization to the current layered presentation path. Measure first.

Settings can restore flat DWM immediately. The old synthetic DWM benchmarks force the previous layout; their data does not measure this change.
