# Start and Stage reference audit

Baseline: [dbf054a](https://github.com/tuanngbc/Deskland-VN-Custom/commit/dbf054a08fd8df0cc6dc599d2689d4463df8eed9).
Revisions below were cloned and the root license was inspected. No external source files, fonts, artwork or runtime enter the application.

| Reference and revision | License at revision | Inspected code and visual source | Decision |
|---|---|---|---|
| [Seelen UI](https://github.com/eythaann/Seelen-UI/tree/fae55d7510aa2503bd81dbc8f53b61677669342e) | AGPL-3.0 | `src/ui/svelte/apps-menu`, `src/static/themes/default/styles/apps-menu.scss`; README's App Launcher screenshot is a different surface from Apps Menu | REFERENCE_ONLY: body/header/footer and interaction states |
| [WinLaunch](https://github.com/jensroth-git/WinLaunch/tree/139ebf85057e1fa05820452d57b8af5cfcb491d9) | MIT | `WinLaunch/MainWindow/Rendering.cs`; [project screenshot](https://winlaunch.org/images/previews/view.png) | ADAPT_ARCHITECTURE: icon-first grid and sparse chrome |
| [OpenDock](https://github.com/leemadov/OpenDock/tree/88cfd2280ba45860b3b0d6c4bd0d0a71a928d1a7) | MIT, separate Apache-2.0 glass port notice | `src/Launchpad.cs`, `src/Glass.cs`, `docs/demo.gif` | REFERENCE_ONLY: coherent material with tonal separation; reject screen capture and runtime reuse |
| [Wisp](https://github.com/SumirSeth/wisp/tree/38c1d4b619ffdbf4cbcd26fcf69c7bdfc0fbe830) | MIT | `src/render.rs`, `src/launch.rs`, `assets/screenshot-launcher.png` | REFERENCE_ONLY: retained DirectWrite formats/layouts; keep native edit/IME now |
| [StageDock](https://github.com/ImmanuelDeo/StageDock-Windows/tree/d8ca4de0b22e1c083b91aa00f344622d9f1bd370) | MIT | `cards.cpp::render3DWindowSlab`, `cards.hpp`, `platform.cpp`, `docs/preview.png` (illustration) | PORT_ALGORITHM: independently derived pinhole inverse. REJECT synchronous foreign PrintWindow and screen-copy fallback; neither demonstrates protected/minimized window support |
| [GNOME Stage Manager](https://github.com/itsDigvijaysing/gnome-stage-manager/tree/7bb5942e207df4553e5b9b740e3505d713ec9182) | GPL-3.0-or-later | `src/extension.js` (actor clone/tilt/hover) and `assets/sidebar_only.png` | REFERENCE_ONLY: grouped layers and hover; GNOME actor cloning is not a Win32 preview source |
| [Stage Manager for Windows](https://github.com/e1roy/stage-manager-for-windows/tree/2ab171f6fc64c8377a0013eadc34580acaa5eb73) | MIT | `stage_mgr_go/internal/app/{switch,stripview,carddrag}.go`, `docs/screenshot.png` | REFERENCE_ONLY: staged focus and explicit end/cancel; preserve native Workspace backend |
| [WacOS](https://github.com/SeimoDev/WacOS/tree/35402c5cde1b04697cce51f434bed825bfdf7a39) | GPL-3.0 | `StageManager/{StageManagerService,StripWindow}.cs` in `src/WacOS/Features` | REFERENCE_ONLY: preserve captured placement, identity and transition lifetimes |

| Component | Baseline | Reference | Design decision |
|---|---|---|---|
| Start material | Fixed-width GDI panel, sampled backdrop | OpenDock | Shared neutral palette, tonal depth, DWM corners; no extra runtime |
| Start grid | Fixed five-column pinned view; separate two-column app catalog; empty Recents reserved | WinLaunch, Seelen | Adaptive grid shared by pinned and catalog, text results for search, optional Recents, separate compact command bar |
| Typography | Segoe UI Variable with GDI gray AA | Wisp, Seelen | Keep native edit for IME; measure and size text, preserve shared fonts. DirectWrite migration needs a comparative render and cost check |
| Animation | Immediate state changes | WinLaunch | Keep input responsive and avoid indefinite repaint; prove bounded motion before wider rollout |
| Stage geometry | Landscape body, overlapping rectangular DWM content, corner icon | StageDock, GNOME | Compute projective map and rounded hit region from one camera; preserve source aspect and flat DWM fallback |
| Stage groups | HWND/PID/TID keys, per-monitor placements and AppBar lifecycle | WacOS, e1roy | Keep group and shell lifecycles; avoid switching windows just for capture |

The local comparison includes before renders and a clearly labeled independent 100% DPI prototype. The owner macOS reference was unavailable. OpenDock is an animated publisher demo; StageDock's published image is an illustration, not a reproduction of our native behavior. Reference images stay outside the repository.
