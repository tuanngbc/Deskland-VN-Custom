# Real tray: compatibility matrix

| Gate | Status | Evidence |
|---|---|---|
| Owned fixture icon registration and versioned callback | Prior fixture-only PASS | [Cooperative tray POC](SYSTEM_TRAY_POC.md); excludes external apps and menus. |
| Existing third-party icons; new, updated or deleted icons | BLOCKED | Explorer with both taskbars parked by auto-hide exposed no usable inventory over read-only UI Automation. No alternative collector verified. |
| Tooltip, icon fidelity, left/right/double click and native app menu | NOT_TESTED | No actual external icon available to the Deskland panel. |
| Hidden/pinned state, local pin/reorder and overflow | NOT_TESTED | Needs verified external identity and pixel source first. |
| App restart, Explorer restart, hidden taskbar or two-monitor anchor | NOT_TESTED | No external bridge for Windows 11; do not infer from Dock recovery or test fixture. |
| Third-party app matrix; disposable VM | NOT_TESTED | No verified test environment with independent shell. |
| Owner acceptance | NOT_TESTED | Only the owner can accept. |
