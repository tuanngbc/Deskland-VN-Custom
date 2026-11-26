# 第三方依赖 / Third-party dependencies

## Microsoft Edge WebView2

Windows 宿主使用 Microsoft Edge WebView2 SDK 与 WebView2 Runtime。构建脚本从 NuGet 获取开发库，本仓库不提交 SDK 二进制文件。

- 包：`Microsoft.Web.WebView2`，构建版本 `1.0.4191.47`。
- 包与许可信息：https://www.nuget.org/packages/Microsoft.Web.WebView2/1.0.4191.47
- 文档：https://learn.microsoft.com/microsoft-edge/webview2/

WebView2 及 Windows / .NET Framework 组件适用各自的许可条款，不因本项目的 GPL-3.0 许可而变更。

The host uses the Microsoft Edge WebView2 SDK and Runtime. SDK binaries are downloaded by the build script and are not checked into this repository. Microsoft components retain their respective licenses.

## NSIS

Windows 安装包使用 NSIS 3.13 制作。便携编译器仅在构建时下载，不作为项目源码提交。NSIS 及其压缩模块适用各自许可，详见 https://nsis.sourceforge.io/License 。安装包附带项目对应源码 ZIP。

## FluidWin

Copyright (c) 2026 Clooudy. MIT License. Genie mesh geometry and shader from
`genio.h`, Drop geometry and timing from `fluidwin.cpp`, Spotlight
calculator grammar from `ricerca.h`, and Windows Search integration from
`indice.h`, and clipboard history logic from `appunti.h`, at
[FluidWin v1.3.1, commit 2f773a8](https://github.com/clooudfn/FluidWin/tree/2f773a8733b84c7dbee88fff6628d14e9ac802b9).
See [the original license](third_party/fluidwin/LICENSE). No FluidWin executables,
wallpapers, cursors, logos, or showcase media are bundled.

Desktop/Explorer context menus directly reuse `destro.h`, `menu.h`, `condividi.h`,
`fabbriche.h`, `raggio.h`, `testi.h` and `traduzioni.h` from the same reference.
Unmodified source hashes are recorded in `third_party/fluidwin/upstream/IMPORT.json`;
TLauncher integration lives in `shell_menu_bridge.cpp` and `src/dock/shell_menu.rs`.

Further native integrations reuse original `affianca.h`, `anteprime.h`,
`jumplist.h`, `pila.h`, `vetro.h`, `mixer.h`, `sistema.h`, `musica.h`,
`comodita.h`, `calendario.h`, `notifiche.h`, `onda.h` and `widget.h`, plus
selected window-state and Dock behaviors from `fluidwin.cpp` and `dock.h`.
These remain covered by the same Copyright (c) 2026 Clooudy and MIT license.
Original Git blobs/byte hashes, scoped adapters and local changes are recorded
in `third_party/fluidwin/upstream/IMPORT.json` and
`third_party/fluidwin/ADAPTATIONS.json`. No upstream standalone service,
executable or additional media license is implied by these source imports.
