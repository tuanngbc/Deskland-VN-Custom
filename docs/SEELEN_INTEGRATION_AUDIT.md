# Audit Seelen UI và các phương án đối chứng

| Dự án | Source revision đã đọc | Giấy phép repo |
|---|---|---|
| [Seelen UI](https://github.com/eythaann/Seelen-UI/tree/fae55d7510aa2503bd81dbc8f53b61677669342e) | fae55d7510aa2503bd81dbc8f53b61677669342e | AGPL-3.0 |
| [ManagedShell](https://github.com/cairoshell/ManagedShell/tree/855222035e927910ca53fd3215caef056337a048) | 855222035e927910ca53fd3215caef056337a048 | Apache-2.0 |
| [RetroBar](https://github.com/dremin/RetroBar/tree/545f9f2946bdd052236b4c6b730fdc73399e225b) | 545f9f2946bdd052236b4c6b730fdc73399e225b | Apache-2.0 |
| [Flow Launcher](https://github.com/Flow-Launcher/Flow.Launcher/tree/58b799721a343811d8c42046cb2b4d63de1b3da6) | 58b799721a343811d8c42046cb2b4d63de1b3da6 | MIT |

Deskland đối chứng: 3a1aebba13c6a730edbc1cee677c7b03b162a268; baseline a604519 không thay đổi.

| Mục | Deskland | Seelen | Quyết định |
|---|---|---|---|
| Win32/MSIX | Shortcut hai phạm vi, AppsFolder qua COM, icon từ Windows | Shortcut resolve AUMID/identity; WinRT PackageCatalog, AppListEntry, StartScreen; watcher thư mục/package | REIMPLEMENT_FROM_PUBLIC_APIS nếu cần làm mới index |
| Tìm, icon, mở | Xếp exact/prefix/token; Shell bitmap, mở shortcut/EXE/app đã cài | FlexSearch; icon cache; frontend gọi command Tauri | REFERENCE_ONLY, giữ launcher Rust |
| Ghim, gần đây, bàn phím | Pin Dock; lịch sử Deskland; context và keyboard | Favorites và folder; seed layout Start; bàn phím; chưa xác minh lịch sử Windows trong các file đã đọc | REUSE_DIRECTLY_CANDIDATE cho module Deskland của chính dự án |
| Khay/tooltip | Bảng riêng và khay Explorer tùy chọn; POC peer hợp tác | Backend Rust giữ identity, bitmap, tooltip/trạng thái; cập nhật và dọn entry mất owner | REFERENCE_ONLY |
| Tương tác | Windows xử lý nếu chọn khay gốc. Fixture kiểm tra riêng callback đã biết | TrayEntry gửi click trái/phải/double qua Tauri, backend chuyển callback; hover forwarding bị tắt trong UI đã đọc | REFERENCE_ONLY |
| Overflow/thứ tự | Không đổi trạng thái icon Windows | Trạng thái ghim và thứ tự đi qua thiết lập Explorer | REJECT trong phiên OWNER |
| Restart/đa màn | Watchdog/AppBar fail-open; UAT cần kiểm tra | Cleanup hook, nhận đăng ký lại và loại icon mất owner; chưa có dữ liệu native hai màn | PENDING |

Dependency map: App → Explorer → hook DLL → slu_ipc → backend Rust (identity, icon cache, state) → Tauri IPC → Svelte → backend → callback của app. Đọc [hook DLL](https://github.com/eythaann/Seelen-UI/blob/fae55d7510aa2503bd81dbc8f53b61677669342e/src/hook_dll/src/lib.rs), [backend tray](https://github.com/eythaann/Seelen-UI/tree/fae55d7510aa2503bd81dbc8f53b61677669342e/src/background/system_modules/system_tray/application/windows), [TrayEntry](https://github.com/eythaann/Seelen-UI/blob/fae55d7510aa2503bd81dbc8f53b61677669342e/src/ui/svelte/system-tray/TrayEntry.svelte). Shell_NotifyIcon, WM_COPYDATA và Win32 callbacks có tài liệu; cấu trúc trao đổi riêng, truy cập luồng Explorer và thiết lập nội bộ taskbar không phải hợp đồng công khai. Revision này có code khay dù upstream từng thông báo bỏ module.

ManagedShell là .NET (Common, Interop, WindowsTray, UWPInterop), có endpoint shell riêng, TaskbarCreated, đường đọc trạng thái Explorer qua COM và legacy toolbar. RetroBar WPF dùng chính ManagedShell, không phải công nghệ tray độc lập. Flow Launcher có plugin Win32/UWP, cache, reindex và watcher, hữu ích làm đối chứng cho Start; không phải backend tray.

Kiến nghị OPTION A: dùng launcher, Options, Control Center, Watchdog của Deskland. Không runtime hay tiến trình mới thường trực, không Tauri/Svelte. POC chỉ khởi động một peer khi gọi lệnh thử. Chưa đo CPU/RAM/GPU so với upstream. Không chọn Seelen làm desktop base; phương án phụ thuộc Shell chỉ đánh giá thêm trong VM và sau review giấy phép.
