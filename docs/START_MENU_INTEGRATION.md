# Start riêng của Deskland

Log build 3a1aebb có Start và tray native, không có dòng watchdog báo ẩn thành công. Khi ẩn chưa có hiệu lực, nút trên Dock theo đường Explorer. Log này không xác định lý do chế độ ẩn chưa bật.

Mặc định mới: Start, nút khay và lịch dùng panel Deskland dù Windows taskbar đang hiện hoặc vừa được khôi phục. Giao diện Windows chỉ dùng khi người dùng chọn và không có takeover. Các phím hệ thống vẫn do Windows quản lý.

Start dùng launcher native: shortcut Win32, Shell AppsFolder cho MSIX/UWP, biểu tượng Shell, ô tìm, menu chuột phải và bàn phím. Pinned theo thứ tự Dock, rồi mục gần đây của Deskland, rồi danh mục app. Lệnh ghim giữ đường dẫn, tham số và thư mục làm việc. Bấm một lần trong Start; Spotlight giữ thao tác trước. Start bám cạnh Dock và xử lý tọa độ âm cùng DPI theo màn đích.

Lỗi PandaTV do người dùng nhận đường dẫn có dấu thoát Markdown. Bản mới chỉ gợi ý sửa khi kiểm tra thấy tệp đích; người dùng chọn áp dụng rồi ghim lại. Bài thử không chạy PandaTV.

Cần UAT trực tiếp hai màn hình trong chế độ ẩn trước khi nghiệm thu.
