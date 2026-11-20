# Dock native Rust — development status

Source branch: `codex/rust-dock`, based on accepted Rust `a604519`.

## Available in this increment
- Opt-in `--dock` process, toggled from existing wallpaper tray menu.
- Separate multi-monitor floating dock surfaces; wallpaper FPS/rendering loop is untouched.
- Running application discovery through top-level HWNDs; process executable-based grouping.
- Focus/restore running windows; launch pinned apps; pin/unpin persisted in `dock.json`.
- Global/local window filtering, display topology refresh, DPI-independent monitor positioning.
- Pause dock overlay on fullscreen applications to avoid game obstruction.
- Windows Settings links for Wi-Fi, Bluetooth, audio, notifications, plus Explorer launch.
- Rust geometry and preference tests in CI.

## Important safety status
**COMPATIBILITY MODE ONLY.** This does not replace Explorer's taskbar or provide a full
third-party notification-icon host. The rightmost "Tray" control opens Windows taskbar settings;
it is not a real icon collection. No code hides or kills Explorer/taskbar.
Native multi-display validation remains pending; success of GitHub Actions proves build/test
but not correct shell behavior on the owner's hardware.

## Next required gates
1. Native screen capture, window management, app icon extraction and drag-drop pin sorting.
2. System notification icon bridge with fail-open recovery and compatibility matrix.
3. AppBar reservation, configurable auto-hide, accessibility, taskbar restore watchdog.
4. Tray, flyouts (volume/network/notification), virtual desktop, focus-stealing safeguards.
5. Real two-monitor acceptance followed by opt-in replacement mode only when tested.
