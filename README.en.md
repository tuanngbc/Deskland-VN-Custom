<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/branding/logo-dark.png">
    <source media="(prefers-color-scheme: light)" srcset="assets/branding/logo-light.png">
    <img src="assets/branding/logo-light.png" alt="TLauncher" width="460">
  </picture>
</p>

<p align="center"><strong>Your Windows. Your Workspace.</strong></p>

[Tiếng Việt](README.md) · **English** · [简体中文](README.zh-CN.md)

# TLauncher — Modern Desktop Workspace for Windows 11

TLauncher is a native Rust desktop workspace for Windows 11, combining a macOS-inspired Dock, Stage Manager, Spotlight search, interactive live wallpapers, and advanced multi-monitor customization in one integrated application.

[![Latest release](https://img.shields.io/github/v/release/tuanngbc/TLauncher?label=release)](https://github.com/tuanngbc/TLauncher/releases/latest) [![Windows 11 x64](https://img.shields.io/badge/Windows%2011-x64-0078D4)](https://github.com/tuanngbc/TLauncher/releases/latest) [![Rust](https://img.shields.io/badge/Rust-native-000000?logo=rust)](README-rust.md) [![GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue)](LICENSE) [![GitHub release downloads](https://img.shields.io/github/downloads/tuanngbc/TLauncher/total?label=release%20downloads)](https://github.com/tuanngbc/TLauncher/releases) [![GitHub stars](https://img.shields.io/github/stars/tuanngbc/TLauncher?label=stars)](https://github.com/tuanngbc/TLauncher/stargazers)

**[Download the latest release for Windows 11 x64](https://github.com/tuanngbc/TLauncher/releases/latest)** · [Changelog](CHANGELOG.md) · [Report an issue](https://github.com/tuanngbc/TLauncher/issues) · [Source code](https://github.com/tuanngbc/TLauncher)

## TLauncher in action

![TLauncher on a 1080×1920 portrait display and a 2560×1080 landscape display, showing the Beach panorama, desktop clocks, Dock, and Stage Manager cards](docs/images/showcase/dual-monitor-real.webp)

*Real Windows 11 capture. The screens retain their relative scale and placement, and each has its own Dock. Clusters of desktop icons still shape the islands. The Stage cards are visible; desktop icons and filenames have been masked. Some screenshots are from native test builds and show Vietnamese UI labels.*

## Contents

- [TLauncher in action](#tlauncher-in-action)
- [What is TLauncher](#what-is-tlauncher)
- [Features at a glance](#features-at-a-glance)
- [Features and screenshots](#features-and-screenshots)
- [System requirements](#system-requirements)
- [Install and upgrade](#install-and-upgrade)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Performance and resource use](#performance-and-resource-use)
- [Architecture and technologies](#architecture-and-technologies)
- [Privacy and security](#privacy-and-security)
- [FAQ](#faq)
- [Build from source](#build-from-source)
- [Feedback and contributions](#feedback-and-contributions)
- [Credits and upstream projects](#credits-and-upstream-projects)
- [License](#license)

## What is TLauncher

Pin and open apps with the Dock, organize windows with Stage Manager, find apps and files with Spotlight, and customize each display. The wallpaper, Dock, and Stage Manager can be turned on or off independently. TLauncher is a Windows desktop app and is unrelated to the Minecraft launcher of the same name.

## Features at a glance

| Area | Available in the public release |
| --- | --- |
| Window management | Dock, Stage Manager, Start, Spotlight, and Windows taskbar compatibility mode |
| Desktop | **16** interactive live themes, time-based lighting, weather by location, three display modes, and two clocks |
| Utilities | Control Center, monthly calendar, CPU/GPU/RAM/network indicators, screenshot editor, and MP4 screen recording |
| Personalization | Integrated Settings, per-display layouts, custom shortcuts, and launch at sign-in |

**Version 1.0.1:** When startup is enabled, Windows starts TLauncher at sign-in through Task Scheduler. Dock and Stage Manager initialize before the wallpaper. If Explorer is still starting, the wallpaper tries to attach again. See the [verification notes and test conditions](CHANGELOG.md#kiểm-chứng).

## What's new in v1.0.2

**Unified Spotlight** combines quick search and the app library. **Alt+Space** defaults to Compact and the Dock magnifier defaults to Fullscreen on the Dock's display. Set these independently or let Dock follow the shortcut. **F6** switches modes while retaining your query. Use **Esc** to leave a folder or dismiss Spotlight, click empty space to close Fullscreen, and scroll or use the keyboard to change pages.

The shared catalog finds Win32 apps in Start Menu and packaged apps in Windows AppsFolder, filters helper shortcuts, and searches apps, indexed files and system tools. Add **EXE/LNK** entries, rename or change icons, favorite, pin to Dock, hide/restore, reorder, or drag into folders in layout editing mode. Hiding or removing a manual entry does not uninstall it. A new profile can show the guide on first opening Settings; reopen it under **Shortcuts & Utilities → Unified Spotlight**.

**Materials and motion.** Fullscreen uses native HostBackdrop/Acrylic when available, a one-time in-memory blurred snapshot as a compatibility path, or a solid surface when transparency is disabled or High Contrast is active. Light/dark themes share the search field, ivory text, shadows and grayscale antialiasing. The native editor retains Windows input and selection. Global speed runs from **0.25× to 2.00×** in **0.05×** steps; new profiles start at **0.70×** and saved speeds are preserved. Spotlight, Start, Dock menus, Control Center and Calendar use motion suited to each panel; Dock magnification, reveal and auto-hide share the policy. System reduced motion is honored when configured.

**Resources.** Start, Spotlight and Dock share bounded source icon caches in memory and on disk. Selective invalidation and bounded prefetch avoid unnecessary extraction and preserve custom icons. First paint and icon quality still depend on the source. Spotlight reuses graphics factories and releases surfaces on close; Stage preview retention is bounded. Whole-app RAM (100–150 MiB), GPU (3%) and CPU (1%) targets have not been verified.

**Window transitions:** public builds use **Windows Default**. Scale failed owner acceptance and is hidden in the public release. Genie and Stage Peek are unavailable. Leave Windows minimize animation enabled. Glass and IME behavior can vary with the Windows build, graphics driver and input method.

| Fullscreen Spotlight library in the light theme | Spotlight app folder in the dark theme | Native Spotlight search field with selected text |
| --- | --- | --- |
| ![Fullscreen Spotlight library in the light theme](docs/images/showcase/v1.0.2/spotlight-fullscreen-light.png) | ![Spotlight app folder in the dark theme](docs/images/showcase/v1.0.2/spotlight-folder-dark.png) | ![Native Spotlight search field with selected text](docs/images/showcase/v1.0.2/spotlight-search-dark.png) |

*Actual Windows native fixture output on GitHub Actions, with example apps and a fixture backdrop. See [image provenance and limits](docs/images/showcase/v1.0.2/README.md).*

[Release notes](docs/releases/v1.0.2.en.md)



<!-- product-guide:start -->
## Features and screenshots

### Interactive live wallpapers

Choose from **16 themes**, from Beach, Meadow, and Aquarium to Stadium, Ocean, and Summer Veranda. Many scenes arrange elements around groups of desktop icons. Depending on the scene, clicks on empty space create ripples, attract fish, or add clouds; clicking an icon still opens the app. Choose a scene, adjust its settings, or create a layout under **Settings → Wallpaper**.

| Beach | Maldives | Meadow |
| --- | --- | --- |
| ![Waves in the Beach theme](docs/images/beach.webp) | ![Lagoon in the Maldives theme](docs/images/maldives.webp) | ![Clouds over the Meadow theme](docs/images/meadow.webp) |

*Previews from three included themes.*

### Weather lighting and solar shadows

Choose offline simulation or **Live weather by location** under **Settings → Weather & Lighting**. Search for a city or postal code to get cloud cover, rain, snow, and wind from Open-Meteo. You can also select clear, cloudy, rainy, stormy, or snowy conditions manually.

With **Time-based lighting** on, TLauncher calculates the sun’s position from the time and chosen coordinates. If no location is set, it uses the local system time. In scenes with tree shadows, such as Beach, the shadows change length and direction as the sun moves. Weather effects and time effects have separate switches.

![Weather settings and a morning, noon, and evening shadow diagram](docs/images/showcase/weather.webp)

*A sample city and a diagram showing shadow direction at different times of day.*

### Panoramas and multiple displays

Use **Synchronized** for the same theme on all displays, **Independent** to pick different scenes, or **Panorama** to span the scene using Windows display coordinates. Landscape and portrait layouts and mixed DPI scaling are supported. Pause wallpaper on individual screens, and choose the display, edge, size, and reserved space for the Dock or Stage Manager.

![Panorama settings with a landscape and a portrait display](docs/images/showcase/multi-monitor.webp)

*Example monitors at 100% and 125% scaling.*

### Two desktop clocks

The native clock shows the time, weekday, and date behind your windows. Unlock the clock to drag it, then lock it to pass clicks through. Customize its size, opacity, and individual lines. Add a second clock with its own size and position in **Settings → Clock**.

![Size and display settings for two clocks](docs/images/showcase/clocks.webp)

*The second clock is enabled on the example portrait screen.*

### macOS-inspired Dock

Pin apps, shortcuts, or commands with arguments. Open and switch windows from their icons. Customize icons, hover magnification, size, spacing, frosted glass, and light, dark, or system colors. The Dock can float, reserve work area, or hide when a window is maximized and return when you move the pointer to the edge. Set these options independently for each screen.

![Dock with pinned apps, hover magnification, and resource indicators](docs/images/showcase/dock.webp)

*Native test capture with a sample profile.*

### Stage Manager and window groups

Preview windows on perspective or flat cards on the left or right. The active card stands out. Use the two arrows to browse windows in a group. Configure the cards per page, card size, and gaps to the screen edge and work area. Version 1.0.0 smooths diagonal edges even during hover magnification. It was tested at **100%, 125%, and 150%** DPI scaling. Windows may prevent previews of protected windows.

![Perspective cards and examples of edge smoothing at several DPI settings](docs/images/showcase/stage.webp)

*Native test capture with sample windows.*


### Start and Spotlight

**Start** focuses the search box as it opens and shows pinned, recent, and frequently used apps, along with account and power options. In Dock mode, pressing the **Windows** key on its own can open TLauncher Start. **Spotlight** is a separate search box centered on the screen; open it with the magnifying glass or **Alt+Space**. Search for apps and indexed files, use the arrow keys to select a result, and press Enter to open it.

| Start | Spotlight |
| --- | --- |
| ![Start with search, pinned apps, and recent apps](docs/images/showcase/start.webp) | ![Spotlight search with a frosted glass background](docs/images/showcase/spotlight.webp) |

*Start from 1.0.0 testing; Spotlight from a native trial build before the product was renamed.*

### Control Center calendar and resource indicators

Open Control Center from the arrow on the Dock to adjust volume or access network and Bluetooth controls. Click the clock to open the calendar and browse months. The Dock can display CPU, GPU, and RAM usage and upload and download speeds. Resource readings and network/volume buttons have separate switches. Use **Windows compatibility** mode for full access to third-party tray icons.

| Monthly calendar | Dock indicators |
| --- | --- |
| ![TLauncher’s monthly calendar in dark mode](docs/images/showcase/calendar.webp) | ![Dock showing CPU, RAM, GPU, network, date, and time](docs/images/showcase/status.webp) |

*The calendar is from 1.0.0 testing; the indicators are from a Dock test capture and do not represent benchmark results.*

### Screenshots and screen recording

Capture a selected region or the entire screen under the pointer. The editor supports cropping, arrows, text, step numbers, pixelation, and solid redaction. Copy to the clipboard or save as PNG/JPG. Record a region to MP4 (H.264) at **30 FPS** by default or **60 FPS**, with optional system audio. You can pause and resume. This release does not record a microphone or webcam and has no dedicated game capture mode.

![Capture shortcuts and video quality settings](docs/images/showcase/capture-shortcuts.webp)

*Settings before starting a recording.*

### Settings and taskbar modes

One window groups Wallpaper, Displays, Weather & Lighting, Clock, Dock & Stage Manager, Shortcuts & Utilities, and System. The WebView2 Settings interface supports Tiếng Việt, English, and 简体中文; some native surfaces use separate UI strings. Explorer keeps running. Select **Windows compatibility** to use the original Start menu, taskbar, and tray.

![Settings with independent toggles for Wallpaper, Dock, and Stage](docs/images/showcase/dock-settings.webp)

*Version 1.0.0 Settings with a sample configuration.*
<!-- product-guide:end -->

## System requirements

- **Windows 11 x64** is the target for the public package; Windows 10 support is not guaranteed.
- An **OpenGL 3.3** driver for native wallpaper rendering. The direct GPU presentation path depends on the driver; GDI is the fallback.
- **Microsoft Edge WebView2 Runtime** for full Settings. If the runtime or `prototype/` is unavailable, basic Win32 settings are available.

## Install and upgrade

1. Download [TLauncher-1.0.2-win-x64.zip](https://github.com/tuanngbc/TLauncher/releases/download/v1.0.2/TLauncher-1.0.2-win-x64.zip) and optionally verify its [SHA-256](https://github.com/tuanngbc/TLauncher/releases/download/v1.0.2/TLauncher-1.0.2-win-x64.zip.sha256). **Source code** archives are for browsing or building the code.
2. Extract the whole archive; keep `prototype/` and `assets/widget/` beside the EXE. Run `TLauncher.exe`, then open Settings from Dock or tray to set Spotlight activation, theme, speed and startup.
3. Use **Alt+Space** to open, **F6** to switch modes and **Esc** to close. Right-click library items to manage, pin or hide. Use **Edit layout** for folders and **Hidden apps** in Settings to restore entries. Use **Exit** in the TLauncher menu, or **Ctrl+Alt+Shift+F12** to restore the Windows taskbar for the session.
4. **Upgrade from 1.0.1:** Exit the old app and back up the installation and `%USERPROFILE%\.deskland-rs`. Extract the full new bundle and keep the profile for wallpapers, displays, pins and shortcuts. If the EXE path changed, check **Settings → System → Start with Windows**; switch it off and on to update Task Scheduler if needed.
5. **Roll back to 1.0.1:** Disable startup in the new app, exit, restore the older bundle and the pre-upgrade profile backup if needed, and register startup again in the older app. Do not assume 1.0.1 understands newer library data; back it up before replacing any profile.

### Recommended Windows settings

- **Frosted glass (optional):** Windows 11 **Settings → Personalization → Colors → Transparency effects**, or **Accessibility → Visual effects** (`ms-settings:easeofaccess-visualeffects`). The compositor and driver determine the result. A solid fallback when transparency is off or High Contrast is active is expected.
- **Motion (optional):** Enable **Animation effects** under **Accessibility → Visual effects** if desired. Keep Windows minimize/maximize animation at its default. To restore it after experimenting, use **Win+R → SystemPropertiesPerformance.exe → Visual Effects**, enable **Animate windows when minimizing and maximizing**, then Apply. Leave font smoothing on if appropriate; no font change, macOS theme or third-party glass utility is needed.
- **TLauncher:** Start with animations on and **0.70×**, Compact by keyboard, Fullscreen from Dock, and Light, Dark or Follow System as preferred. Choose wallpaper quality for your driver. Startup at sign-in is optional.



## Keyboard shortcuts

| Default | Action |
| --- | --- |
| **Ctrl+Shift+S** | Capture a region; press Space to select the full screen under the pointer. |
| **Ctrl+Shift+R** | Select a recording area; press again to stop and save. |
| **Ctrl+Shift+P** | Pause or resume recording. |
| **Alt+Space** | Open Spotlight. |
| **Ctrl+Alt+L** | Lock the session through Windows. |
| **Ctrl+Alt+Shift+F12** | Restore the Windows taskbar for the current session. |

Change the Spotlight, capture, recording, and lock shortcuts in **Settings → Shortcuts & Utilities**.

## Performance and resource use

Wallpapers usually run at about **30 FPS** and may render faster briefly during interaction. An occluded wallpaper stops receiving new frames. If all wallpapers are covered or the session is locked, rendering pauses. WebView2 is used when Settings is open; capture and recording run on demand.

**Sample observation from v1.0.0:** Windows 11, Ryzen 5 4600G, GTX 1070, **2560×1080 + 1080×1920** displays, Beach in Panorama mode. Over **25.5 seconds / 18 samples**, logs showed **one occluded wallpaper**; the other ran at about **30 FPS**.

| Metric | Observation |
| --- | ---: |
| Main-process CPU, normalized across 12 logical threads | **0.54%** average |
| Private memory (private bytes) | **279 MiB** |
| Resident memory (working set) | **174–176 MiB** |
| TLauncher GPU 3D on the active engine | **1–2%** |
| DWM GPU 3D on the same engine | **0–1%** |

![Sample CPU, RAM, and GPU 3D readings with one wallpaper occluded](docs/images/showcase/performance.webp)

*GPU readings are per process and engine, rather than total GPU use. This snapshot is not a controlled comparison or a guarantee for two visible wallpapers, open Settings, or active recording.*


## Architecture and technologies

- **Rust and Win32** manage the desktop, windows, per-display DPI, and native utilities.
- **OpenGL 3.3 via WGL** renders the wallpaper. On supported systems, **Direct3D 11 and DirectComposition** present GPU frames; **GDI** is the fallback.
- **WebView2** provides full Settings on demand; a basic Win32 interface is also available.
- **Windows Graphics Capture** is used on supported capture paths; **Media Foundation** encodes MP4/H.264.
- **Windows Task Scheduler** registers per-user startup from 1.0.1 onward. See [the Rust guide](README-rust.md) and [architecture docs](docs/README.md).

## Privacy and security

Searching for a place sends a query to Open-Meteo’s geocoding service. Live weather sends the selected place’s coordinates to request the forecast; TLauncher does not retrieve the device’s location for this feature. Offline simulation is available. By default, screenshots and recordings are saved locally in `Pictures/TLauncher` and `Videos/TLauncher`. The user controls when an image is copied to the clipboard.

TLauncher uses Windows session lock and sign-in. The 1.0.1 startup task runs under the current account and does not need administrator rights.

## FAQ

**Why do I only see basic Settings?** Check for the WebView2 Runtime and the `prototype/` folder beside the EXE. Extract the full package if you previously copied only the executable.

**How do I get the Windows taskbar back?** Select **Windows compatibility**, or press **Ctrl+Alt+Shift+F12** to restore it for the current session. Explorer continues running while the Dock is active.

**Why isn’t TLauncher listed in Windows Startup apps?** Version 1.0.1 uses Task Scheduler. Use the switch in TLauncher Settings. If you moved the EXE, toggle the switch off and back on.

**Why can’t I preview or record some windows?** Windows may restrict protected content. Recording supports system audio. This release does not capture microphone audio or a webcam.

**Profile and rollback:** See Install and upgrade above.

## Build from source

On Windows, use stable Rust, the MSVC C++ build tools, and the Windows SDK. From the source directory:

```powershell
cargo test --release
cargo build --release
```

See the [Rust build guide](README-rust.md) for resource layout next to the executable. Release packages record the version in `VERSION.txt` and the source commit in `BUILD-COMMIT.txt`.

## Feedback and contributions

[Report a bug or suggest a feature](https://github.com/tuanngbc/TLauncher/issues) with your TLauncher and Windows versions, display layout if relevant, and steps to reproduce. Remove personal data from screenshots and logs. See the [development notes](AGENTS.md) and the [README translation policy](docs/README.md#quy-ước-readme-đa-ngôn-ngữ).

## Credits and upstream projects

- [BLEZZ0102/Deskland](https://github.com/BLEZZ0102/Deskland) — upstream project and the foundation for live wallpapers.
- [nhhoang188/Deskland-VN](https://github.com/nhhoang188/Deskland-VN) — ported themes and Vietnamese localization.
- [tuanngbc/TLauncher](https://github.com/tuanngbc/TLauncher) — [tuanngbc](https://github.com/tuanngbc) develops and maintains the TLauncher fork, including native Rust rendering, desktop improvements, the multi-monitor system (synchronized, independent, and panoramas based on Windows display coordinates), Dock, Stage Manager, Start, Spotlight, taskbar modes, Control Center, calendar, system monitoring, native clocks, screenshot and recording tools, shortcuts, weather and solar shadows, integrated Settings, and the Windows startup improvements.

See [NOTICE](NOTICE), the [original Deskland-VN notice](third_party/deskland-vn/NOTICE), and the [clock design credits](assets/widget/NOTICE.txt).

## License

TLauncher source code is released under [GNU GPL-3.0](LICENSE). See [third-party notices](THIRD_PARTY_NOTICES.md), [trademarks](TRADEMARKS.md), and the [logo guidelines](assets/branding/BRAND.md). TLauncher is an independent project and is not an official Apple or Microsoft product.
