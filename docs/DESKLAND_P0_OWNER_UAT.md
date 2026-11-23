# Owner UAT: default perspective and real tray

Task: `DESKLAND-STAGE-PERSISTENT-PERSPECTIVE-AND-REAL-TRAY-P0`. Baseline [71e320a](https://github.com/tuanngbc/Deskland-VN-Custom/commit/71e320a1d7477ab39829c04eb715bdeaa0b0d40a); review stays in draft [PR #1](https://github.com/tuanngbc/Deskland-VN-Custom/pull/1). Take the Windows x64 artifact only from a passing workflow for the exact intended commit and verify BUILD-COMMIT and SHA256 before preparing a profile.

- [Stage architecture](STAGE_DEFAULT_PERSPECTIVE_ARCHITECTURE.md), [cache lifecycle](STAGE_PREVIEW_CACHE_LIFECYCLE.md), [minimize and restore](STAGE_MINIMIZE_RESTORE_ACCEPTANCE.md), [height settings](STAGE_CARD_HEIGHT_SETTINGS.md).
- [Tray references](REAL_TRAY_REFERENCE_AUDIT.md), [bridge disposition](REAL_TRAY_BRIDGE_POC.md), [licensing and shell](REAL_TRAY_LICENSE_AND_SAFETY.md), [tray compatibility](REAL_TRAY_COMPATIBILITY_MATRIX.md).

The Stage code is in the product path. Software tests and explicitly reported Win32 fixture results are not owner acceptance. Both rail directions and DPI settings, ordinary apps, multi-window groups and resource usage with real perspective still need user-session evidence. The previous DWM measurements cannot substitute for that comparison. External tray is blocked; do not accept taskbar replacement based on the caret or the minimized window list.

Start from the isolated Desktop shortcut. Check three apps across the two screens; minimize and restore; try one, three and five windows in a group; open the sixth via All Windows; change heights and switch between perspective and DWM without restart. Use the companion shortcut to return to the stable build. Emergency taskbar recovery remains Ctrl+Alt+Shift+F12. Production and shell settings are not migrated by this trial. Owner visual and native approval remain pending.
