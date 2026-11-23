# Ma trận nghiệm thu Shell

| Kịch bản | Kết quả cần thấy | Bằng chứng / giới hạn |
|---|---|---|
| Taskbar hiện hoặc đang chờ ẩn, nút Deskland | Start/khay mở giao diện riêng | Kiểm thử routing; cần thử thao tác owner |
| Cả hai taskbar đã ẩn | Start, Control Center và lịch mở bên đúng Dock mà không hiện taskbar | Chưa PASS native |
| Tương thích + chọn nút Windows | UI của Explorer có thể hiện taskbar | Đã dò được Start hai màn và overflow chính ở vòng trước; bản này cần UAT |
| Thoát hoặc tắt Dock, Ctrl+Alt+Shift+F12 | Windows taskbar quay lại, không guard mồ côi | Giữ watchdog, có test lease và cửa sổ mẫu; kiểm tra thực còn thiếu |
| Crash, Explorer restart, watchdog chết | Fail-open và có thể bật lại thủ công | Test thành phần đã có; kiểm tra chuỗi hai màn còn thiếu |
| Lock/session switch, đổi màn hình | Không làm mất cấu hình, hiển thị taskbar khi cần | Rút màn, đổi primary và session cần UAT hoặc VM |
| Peer thử thoát và mở lại | Dọn đúng peer, cấu hình dùng hằng ngày không thay đổi | Fixture release, không tương đương Explorer restart |
| Win/Win+X/Win+N | Windows xử lý, có thể hiện UI của Windows | Không chặn phím hệ thống |
| Ghim .lnk và lệnh có tham số | Giữ tham số và cwd; gợi ý đường dẫn hợp lệ | Test giao diện và ShellExecute với ứng dụng mẫu, không chạy PandaTV |

Bài UAT: chọn Giao diện Deskland, bật ẩn và kiểm tra cả hai taskbar; click Start rồi Paint/Notepad, thử menu ghim. Mở khay từng màn. Thử phím khôi phục; sau đó chọn Giao diện Windows khi taskbar hiện để đối chứng. Cuối cùng dùng shortcut quay lại bản ổn định. Không khởi động lại Explorer hoặc thay cấu hình hệ thống trên desktop đang dùng.

OWNER_NATIVE_ACCEPTED_PENDING.
