# FluidWin parity — native foundation

Status vocabulary: NOT_STARTED, IMPLEMENTED, VERIFIED_CI, VERIFIED_OWNER,
BLOCKED_WITH_EVIDENCE, EXCLUDED_BY_OWNER. A retained source implementation is not
proof of successful hardware behavior. Owner gate is PENDING.

| Upstream subsystem (source audit) | Current status |
| --- | --- |
| Entry/message loop/window lifecycle, tracking, native transition/reversal/focus | IMPLEMENTED (complete `fluidwin.cpp`) |
| Genie/Drop/open/close/maximize/restore/Jelly/live GPU contents | IMPLEMENTED (original pipeline) |
| Dock identities/pins/reorder/hide/jump lists/recent commands/Downloads/Recycle/drag/show desktop/four edges/follow/fullscreen | IMPLEMENTED (`dock.h`, `jumplist.h`, `pila.h`, `anteprime.h`) |
| Native Launcher/search/catalog/icons/uninstall/context/keyboard | IMPLEMENTED (`launcher.h`, `ricerca.h`, `indice.h`, `onda.h`) |
| Desktop/Explorer Shell extensions/rename/share/open-with/scroll/keyboard | IMPLEMENTED (`destro.h`, `menu.h`, `condividi.h`, `vetro.h`) |
| Snap/edge/hover/Win+Z/arrows/Overview/topmost/maximized drag | IMPLEMENTED (`affianca.h`, `panoramica.h`, core hooks) |
| Native Control Center/audio/mixer/outputs/mic/Wi-Fi/Bluetooth/brightness/theme/comfort/timer/calendar/weather/news | IMPLEMENTED (`centro.h`, `mixer.h`, `wifi.h`, `comodita.h`, `widget.h`, `sistema.h`) |
| Notifications/banners/badges/silence/OSD/device indicators | IMPLEMENTED (`notifiche.h`, `banner.h`, `osd.h`, `sensori.h`) |
| Dynamic Notch/music/art/timer/shelf/alerts/fullscreen/fixed/activity | IMPLEMENTED (`notch.h`), INCLUDED |
| True Notch disable/re-enable/persistence/resource release | IMPLEMENTED (new independent lifetime switch) |
| Cursors/recovery/appearance/corners/static wallpaper/diagnostics | IMPLEMENTED (original sources; static wallpaper writes blocked in UAT) |
| Detached Guardian/taskbar/styles/opacity/transitions/Snap/cursors/topmost/emergency/recovery | IMPLEMENTED (original lifecycle, separate namespace) |
| Native Settings/localization seven upstream languages/RTL | IMPLEMENTED; full vi/zh parity remains consolidation scope |
| Auto Update | EXCLUDED_BY_OWNER (source retained for provenance; not included, linked or packaged) |
| Scale effect extension | NOT_STARTED (optional integration only if upstream quality preserved) |

Complete headers and three embedded upstream wallpaper assets are retained; no
FluidWin logo/showcase media is presented as TLauncher. Asset provenance is in
THIRD_PARTY_NOTICES. Actual visuals, physical multi-monitor and whole-process
performance await owner acceptance; no VERIFIED_OWNER claims are made.
