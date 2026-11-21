# Visual acceptance evidence

Status: **VISUAL_PENDING**. The owner rejected the previous GDI appearance. M1 Settings preview is a placement diagram and does not certify the Dock or Stage appearance.

Use the [macOS Dock controls](https://support.apple.com/en-us/guide/mac-help/mchlp1119/mac) as a behavior reference. Build an original native design with Windows app icons and no Apple artwork. At 100%, 125%, 150% and 200% DPI, capture a landscape and a portrait screen on light and dark wallpapers. Compare capsule proportions, corners, sheen, shadow, icon clarity, spacing, state indicators, pointer magnification and input, hiding/reveal, thumbnails and group transitions.

The isolated probe compares the documented [DWM Desktop Acrylic backdrop](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwm_systembackdrop_type) to a premultiplied alpha capsule over the same sharp reference stripes. The latter provides real desktop transparency, fractional edges and a soft shadow, but does not blur the desktop. Do not label a successful DWM attribute call or the fallback surface as proof of true blur. Verify by examining the actual rendered pixels and measure before choosing the production path.

Stage still needs [DWM thumbnail relationships](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmregisterthumbnail), fallback for unavailable previews and workspace behavior. Required evidence is actual Windows screenshots and motion capture, scoped to the trial fixture. VISUAL_PASS requires owner review of that evidence; OWNER_NATIVE_ACCEPTED remains separate.
