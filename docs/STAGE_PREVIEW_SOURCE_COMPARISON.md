# Stage preview sources

| Source | Decision | Limits |
|---|---|---|
| DWM Thumbnail | Keep for production fallback | Rectangular destination, no perspective warp. Current guards check identity, visibility, minimized/cloaked state and affinity; unresolved sources show non-content fallback |
| [Windows.Graphics.Capture per HWND](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createforwindow) | Test-only one-frame proof | Session/device/affinity checks; genuine window texture if supported, or record the exact failure; not a live stream |
| Event-triggered WGC snapshots | Candidate architecture; disabled | Require explicit user selection of each app window, an in-memory bounded cache and independent testing of background/occluded/minimized/protected windows. Need to invalidate on close, identity change, device loss, monitor change and disable |
| PrintWindow or desktop copy | Rejected for external windows | A synchronous window request can stall; screen-copy can copy private unrelated pixels and is not an allowable fallback |

The WGC test creates its own known HWND, verifies same-process/class identity and capture affinity, excludes desktop handles and uses no display capture. It keeps the Windows capture border, excludes the cursor, retries no protected source, reads one frame via a one-buffer pool, and closes the frame, session and pool. The budget is 512 × 320 and two seconds waiting for a frame event. Only the known synthetic window's projected result can be exported. A success proves the texture path on the run's graphics session, but not consent, GPU costs or lifetime behavior for product windows. See [Microsoft's capture lifecycle documentation](https://learn.microsoft.com/en-us/windows/apps/develop/media-authoring-processing/screen-capture).
