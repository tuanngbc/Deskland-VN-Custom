# P0 – nghiệm thu Deskland thay taskbar

## Bằng chứng vòng thử b66a5b9

- Trên Windows 11 build 26200, lượt UAT xác nhận ẩn xong rồi Start và khay mở local. Log sau đó có ShellRevealed; không đủ bằng chứng để gán sự kiện này cho click, phím Win hoặc con trỏ.
- Phép đọc sau UAT trả ABM_GETSTATE=1 (Windows auto-hide). Cả hai HWND của cùng Explorer mang WS_VISIBLE trong khi biên hình học đậu ở mép. Biên bê ra ngoài màn không đồng nghĩa thanh đầy đủ đang nhìn thấy.
- Settings cùng tiến trình bị bộ dò cửa sổ loại trừ để tránh WM_GETTEXT xuyên luồng. Hai chủ cửa sổ hiện công bố cùng một ID và tự báo vòng đời; Dock vẫn không dò toàn bộ HWND nội bộ.

## Thay đổi và giới hạn

| Gate | Trạng thái trước OWNER UAT | Cách kiểm tra |
|---|---|---|
| DESKLAND_SETTINGS_DOCK_ICON | HWND lifecycle fixture trên Windows CI; WebView2/fallback thực tế và hai màn chủ máy cần xác minh | Mở, thu nhỏ, restore qua Dock, kéo màn, đóng, ghim; kiểm tra chuyển sang fallback không nhân đôi |
| TASKBAR_EDGE_REVEAL_NATIVE | PENDING. Phân biệt thanh đậu/tạm hiện với Explorer đổi danh tính; không ngăn Explorer vẽ taskbar | Thử mép dưới ngang/dọc, còn và rời mép; đối chiếu taskbar-watchdog.jsonl |
| TASKBAR_ROLLBACK | PENDING trên bản tích hợp. Watchdog và hotkey khôi phục giữ nguyên | Compatibility, Ctrl+Alt+Shift+F12, tắt Dock và Exit; xác nhận cả hai thanh và không còn guard |
| WIN_KEY_DESKLAND_START_NATIVE | PENDING. Ctrl+Alt+Space là tùy chọn riêng trong Dock-only, phím Win vẫn do Windows xử lý | So sánh Win+E/D/R/I/Tab/Shift+S/L/X/mũi tên với baseline; xác nhận bàn phím và fullscreen trước khi xét Win độc lập |
| START_MENU_FUNCTIONAL | PARTIAL. HWND riêng, grid ghim, kéo thứ tự, ứng dụng, tìm, lịch sử chỉ từ Start, hành động hệ thống và bàn phím | Kiểm tra icon Win32/LNK/MSIX trên hai màn, bảng nguồn chỉ xem trong trial; chưa có folder, mới cài hay New Window theo capability từng app |
| THIRD_PARTY_TRAY_NATIVE | BLOCKED. Nút khay Windows dùng Explorer thật trong Compatibility; biểu tượng ứng dụng chưa được nhân bản | Kiểm tra menu gốc qua đường tương thích; cần VM được duyệt để kiểm tra bridge độc lập |
| OWNER_NATIVE_ACCEPTED | PENDING | OWNER duyệt sau native UAT |

Watchdog xác nhận việc ẩn ban đầu bằng trạng thái HWND. Sau đó chỉ với auto-hide ban đầu đang bật, nó nhận dạng thanh ngang đậu ngoài vùng hiển thị qua hình học và cho một lần trượt ở ngay sát mép kết thúc trong thời gian hữu hạn. Nếu thanh thực sự hiện lâu, xuất hiện khỏi mép hoặc Explorer đổi HWND/PID/TID, kết thúc Dock-only và khôi phục ngay hoặc sau khoảng chờ hữu hạn. Không dùng vòng ép ẩn, không gọi ABM_SETSTATE. Log trial lưu thời điểm, hình học, HWND/PID/TID, cursor và loại foreground (không lưu tiêu đề hay phím). ABM_SETSTATE điều chỉnh tùy chọn của Windows, không phải cơ chế dành riêng cho một phiên Dock.

Một low-level hook có thể quan sát phím trước khi Windows cập nhật trạng thái key và có thể bị Windows gỡ khi timeout. Không dùng cách chặn toàn bộ Win hoặc phát lại tổ hợp chưa được xác minh. Ctrl+Alt+Space được đăng ký sau watchdog ready và hủy khi chuyển Compatibility, suspend, Dock off hoặc Exit; Ctrl+Alt+Shift+F12 độc lập.

Nguồn: [AppBar state](https://learn.microsoft.com/en-us/windows/desktop/shell/abm-getstate), [AppBar settings](https://learn.microsoft.com/en-us/windows/win32/shell/abm-setstate), [keyboard callback](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc), [RegisterHotKey](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey), [notification API](https://learn.microsoft.com/en-us/windows/win32/shell/notification-area), [Seelen audit](SEELEN_INTEGRATION_AUDIT.md).
