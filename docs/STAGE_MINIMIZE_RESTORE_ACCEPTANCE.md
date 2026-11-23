# Stage: minimize/restore acceptance

On minimize the snapshot capture stamp is inactive; an existing allowed frame remains usable. An iconic HWND is still distinct from a closed HWND and the window model uses its restored monitor placement. On restore an old valid frame can paint immediately, while the changed capture stamp schedules one replacement. A closed HWND purges the source; any other group members keep their own frames. A window minimized before the first frame can use its app icon; Stage never restores a window just to request pixels.

The scheduler tests check source misses, expiry and partial layer occlusion. The Windows child-fixture test tries two external-process HWNDs that the test owns, verifies two independent frames and drives minimize, restore and close only on that child. Its recorded result is separate from workflow success. The owned WGC diagnostic checks the role of the HWND message pump and window capture policy separately.

Owner check: open 2–3 ordinary apps on each display. Wait for both Stage rails, then minimize and restore each app by the caption and by the Stage card. The card and app icon should remain, with its last available image until the permitted refresh arrives. Close one app and verify no ghost frame remains. Try both sides and different DPI. A protected preview should show its normal fallback.
