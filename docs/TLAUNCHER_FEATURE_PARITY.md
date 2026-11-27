# TLauncher parity — native re-engineering

All stable 1.0.2 implementations and the rejected integration work are preserved
in Git. None of the following is removed from the final 1.0.3 scope. Phase 1
was accepted by owner 11/11 on c011c2b11b3d, Actions 45870002906. Phase 2 uses
new C++ owners and audited artwork/math; no legacy runtime or module is linked.

| Stable feature group | Native integration status | Phase |
| --- | --- | --- |
| 16 interactive wallpaper scenes/WGL-D3D presentation/FPS/occlusion/lock/pause/resources | IMPLEMENTED; hosted renderer/session verification, owner UAT PENDING | 2 |
| Panorama/synchronized/independent displays/hotplug/mixed DPI/portrait | IMPLEMENTED; physical topology and tiled pixels verified in CI, physical hardware UAT PENDING | 2 |
| Weather/manual/offline/live/solar/shadows/rain/snow/cloud/wind | IMPLEMENTED; shared FluidWin weather data, solar/WMO contracts verified, live provider/hardware UAT PENDING | 2 |
| Native wallpaper gallery/scene controls/keyboard/MSAA | IMPLEMENTED; full candidate integration verification required before handoff | 2 |
| Stage grouping/cards/perspective/flat/monitor placement/Peek | NOT_STARTED | 3 |
| Compact/Fullscreen Spotlight/Alt+Space/F6/folders/drag/names/icons/favorites/pins/portable apps/hidden/cache | NOT_STARTED | 3 |
| Two desktop clocks/format/monitor/opacity/click-through/lock | NOT_STARTED | 4 |
| Screenshot editor/crop/arrows/text/numbers/pixelation/redaction/copy/PNG/JPG | NOT_STARTED | 4 |
| MP4 H.264 recorder/region/30–60 FPS/system audio/pause/output/shortcuts | NOT_STARTED | 4 |
| CPU/GPU/RAM/network indicators and native controls | NOT_STARTED | 4 |
| Backup/schema/validation/idempotent migration/rollback/unknown fields/assets | Wallpaper schema/atomic backup/unknown fields implemented; whole-app migration/import/export/rollback remains NOT_STARTED, production untouched | 4 |
| Full Vietnamese/English/Simplified Chinese Settings parity | NOT_STARTED (upstream languages preserved; extension pending) | 4–5 |

Phase 3 waits for Phase 2 owner acceptance; Phase 4 waits for Phase 3 acceptance.
Only Auto Update is excluded; Dynamic Notch is now included.
