# TLauncher parity — native migration

All stable 1.0.2 implementations and the rejected integration work are preserved
in Git. None of the following is removed from the final 1.0.3 scope. The Phase 1
package intentionally contains only the native foundation, pending owner gate.

| Stable feature group | Native integration status | Phase |
| --- | --- | --- |
| 16 interactive wallpaper scenes/OpenGL/FPS/occlusion/lock/pause/resources | NOT_STARTED | 2 |
| Panorama/synchronized/independent displays/hotplug/mixed DPI/portrait | NOT_STARTED | 2 |
| Weather/manual/offline/live/solar/shadows/rain/snow/cloud/wind | NOT_STARTED | 2 |
| Stage grouping/cards/perspective/flat/monitor placement/Peek | NOT_STARTED | 3 |
| Compact/Fullscreen Spotlight/Alt+Space/F6/folders/drag/names/icons/favorites/pins/portable apps/hidden/cache | NOT_STARTED | 3 |
| Two desktop clocks/format/monitor/opacity/click-through/lock | NOT_STARTED | 4 |
| Screenshot editor/crop/arrows/text/numbers/pixelation/redaction/copy/PNG/JPG | NOT_STARTED | 4 |
| MP4 H.264 recorder/region/30–60 FPS/system audio/pause/output/shortcuts | NOT_STARTED | 4 |
| CPU/GPU/RAM/network indicators and native controls | NOT_STARTED | 4 |
| Backup/schema/validation/idempotent migration/rollback/unknown fields/assets | NOT_STARTED (production untouched) | 4 |
| Full Vietnamese/English/Simplified Chinese Settings parity | NOT_STARTED (upstream languages preserved; extension pending) | 4–5 |

Do not start these integrations before the hard Phase 1 owner acceptance gate.
Only Auto Update is excluded; Dynamic Notch is now included.
