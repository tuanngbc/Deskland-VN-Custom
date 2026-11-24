# v1.0.2 showcase provenance

These unmodified PNGs are real Windows UI from [reference commit 3a3e512](https://github.com/tuanngbc/TLauncher/commit/3a3e51202f4cd34708ebd3d2cffca75b2f034f94), [Windows Actions run 45430738826](https://github.com/tuanngbc/TLauncher/actions/runs/45430738826). Release changes hide the Scale experiment and do not change Spotlight rendering. Test catalog, backdrop and typed text are synthetic; UI controls and renderer are production code.

| Image | Fixture file | Evidence |
| --- | --- | --- |
| spotlight-fullscreen-light.png | library-fixture-light.png | Fullscreen app grid, light theme, native library path; fixture backdrop. |
| spotlight-folder-dark.png | folder-fixture-dark.png | Open app folder, dark theme, native library path; fixture backdrop. |
| spotlight-edit-light.png | edit-fixture-light.png | Layout editing, light theme; fixture backdrop. |
| spotlight-search-light.png | actual-search-light-query.png | Actual native field, multilingual input and caret, light theme, compositor capture. |
| spotlight-search-dark.png | actual-search-dark-selection.png | Actual native field and selection, dark theme, compositor capture. |

The library shots do not demonstrate live blur over a user's windows. The published dual-monitor panorama in the parent folder is historical Windows 11 imagery, with desktop names/icons masked. There is no verified new full portrait, Compact or settings capture in this evidence set, and no motion recording; static images are used instead.

