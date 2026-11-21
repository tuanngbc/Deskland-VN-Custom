# Visual acceptance evidence

Current P0 build and scoped native evidence: [Dock P0 verification](DOCK_P0_VERIFICATION.md). The full owner acceptance gates remain pending.

The [4b37064 Windows artifact](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44884664434/artifacts/14699144284) produced a native landscape crop showing Explorer, a custom shortcut, packaged Notepad and the compact status row at 96 DPI. It is evidence of normal size and real icon sources only. The open Windows on the user's desktop differ from the previous capture; do not use the new width as a matched before/after comparison. No video, higher physical DPI, authentication-free screen-reader review or complete portrait input matrix has been collected for this SHA.

Status: **VISUAL_PENDING**. The owner rejected the previous GDI appearance. M1 Settings preview is a placement diagram and does not certify the Dock or Stage appearance.

Use the [macOS Dock controls](https://support.apple.com/en-us/guide/mac-help/mchlp1119/mac) as a behavior reference. Build an original native design with Windows app icons and no Apple artwork. At 100%, 125%, 150% and 200% DPI, capture a landscape and a portrait screen on light and dark wallpapers. Compare capsule proportions, corners, sheen, shadow, icon clarity, spacing, state indicators, pointer magnification and input, hiding/reveal, thumbnails and group transitions.

The isolated probe compares the documented [DWM Desktop Acrylic backdrop](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwm_systembackdrop_type) to a premultiplied alpha capsule over the same sharp reference stripes. The latter provides real desktop transparency, fractional edges and a soft shadow, but does not blur the desktop. Do not label a successful DWM attribute call or the fallback surface as proof of true blur. Verify by examining the actual rendered pixels and measure before choosing the production path.

Stage now uses [DWM thumbnail relationships](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmregisterthumbnail), with fallbacks and explicit window groups. Native fixture evidence verifies content; sidebar composition and motion still require visual review. Required evidence is actual Windows screenshots and motion capture, scoped to the trial fixture. VISUAL_PASS requires owner review of that evidence; OWNER_NATIVE_ACCEPTED remains separate.

## Native render checkpoint

Actual native capsule and Spotlight activation were observed at 9512260. The separate glass probe at f63d7f7 rendered background stripes through per-pixel alpha, while the DWM candidates stayed opaque with the owner's existing transparency setting. The screenshots are private. Shape and alpha are observed; blur, shadow and contrast under different wallpapers, packaged icon coverage, motion pacing, Stage layout/interaction quality and owner approval stay pending. Native 3afc044 showed actual fixture thumbnails alongside safe fallback icons; a still of three sources does not establish motion quality or privacy coverage across app types.

## Floating Stage: native evidence before utility acceptance

The first rebuild [d2ba9e5](https://github.com/tuanngbc/Deskland-VN-Custom/actions/runs/44877602926) uses a layered card surface, an owned clipped DWM thumbnail target and an opaque per-card fallback when necessary. Source rectangle aspect is preserved. Two actual Windows 11 screenshots were taken in an isolated fixture trial at 2560×1080 and 1080×1920; the fixture-only portrait capture before/after demonstrates changing DWM content, with protected/minimized sources falling back to icons. This proves transparency around and through the material and retained functional previews. It does not prove acrylic blur or visual parity.

Compared with [Apple's Stage Manager behavior](https://support.apple.com/en-us/guide/mac-help/mchl534ba392/mac), standalone thumbnail groups and negative space are present. Windows Stage still shows recent cards in a vertical stack, uses a compact pager and preserves the existing context-menu grouping workflow. Full scale/slide transition, final typography, wallpaper comparisons, actual video, brightness/mixed DPI and owner review remain open. Do not mark VISUAL_PASS.

On owner Windows 11, the verified 099a4eb build displayed a separate utility area and real Wi-Fi, speaker and Settings glyphs on the landscape Dock. Menu and native volume-panel stills are private. Per-display placement and reconfiguration were observed at 96 DPI, including a relaunch. Overlay animation, glass comparison on wallpaper, all mixed DPI cases and two-display motion video remain pending. No visual sign-off.
