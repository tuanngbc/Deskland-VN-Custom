# Stage Manager và Start: phạm vi bản thử

Stage tự gom theo AUMID của cửa sổ, sau đó AUMID của ứng dụng đóng gói và đường dẫn executable. Host dùng chung không có AUMID được tách theo từng cửa sổ. Nhóm thủ công được ưu tiên và chỉ có hiệu lực trong phiên. Khi tùy chọn tất cả cửa sổ tắt, một cửa sổ bắc qua hai màn hình được xếp vào màn hình mà nó chiếm nhiều diện tích nhất.

DWM vẽ tối đa năm ảnh xem trước thật. Cửa sổ phía trước giữ tỷ lệ; các cửa sổ sau chỉ vẽ phần lộ ra. Menu chứa toàn bộ thành viên. Ảnh trong sản phẩm vẫn phẳng. Bài thử DirectComposition kiểm tra hai màu và cạnh nghiêng trên cửa sổ mẫu thuộc một tiến trình khác. Chỉ có HRESULT thành công hoặc CI xanh thì chưa đủ chứng minh có ảnh phối cảnh.

| Hạng mục | Trạng thái cần kiểm tra trên máy |
| --- | --- |
| Stage: 1, 2, 3, 4, 5 và 8 cửa sổ | Kiểm tra mỗi số lượng, hai cạnh, DPI khác nhau và màn dọc. Với 8 cửa sổ, chọn cửa sổ thứ 6–8 từ menu. |
| Stage: thao tác | Mở ảnh phía sau; bấm icon để kích hoạt nhóm; thử nút đóng khi rê chuột và menu Đóng nhóm với cửa sổ mẫu. Chọn Giải tán phải giữ các cửa sổ mở. |
| Stage: vòng đời | Đóng app, thu nhỏ, khôi phục, kéo qua màn khác, chuyển desktop ảo, fullscreen. Nhóm thủ công và lựa chọn tách nhóm tự động chưa lưu xuyên lần khởi động. |
| Stage: hình ảnh và hiệu năng | **PENDING** phối cảnh trong sản phẩm, chuyển động khi thành viên thay đổi, ảnh/video trên máy hai màn, và đo CPU/GPU/handle trên cùng fixture. |
| Start | Kiểm tra chiều cao theo nội dung, ghim và đổi thứ tự, lịch sử, gõ tìm rồi Enter, nền sáng/tối và neo ở từng cạnh Dock. Chỉ mở bảng nguồn; không gửi lệnh nguồn trong bản thử. |
| Khay | **BLOCKED** collector/callback app bên thứ ba chưa chứng minh trong VM. Chế độ tương thích vẫn có thể mở khay Explorer. |

Ma trận dưới đây ghi loại triển khai, không phải kết quả nghiệm thu.

| Chức năng taskbar | Phân loại | Giới hạn hiện tại |
| --- | --- | --- |
| Start, danh sách app, tìm kiếm, lịch sử | DESKLAND_NATIVE | Phiên nhập và lịch sử riêng, dùng Windows Shell để mở app. Chất lượng hình ảnh PENDING. |
| Ghim/bỏ ghim, các cửa sổ đang chạy, trạng thái active/minimize/attention, bộ chọn và menu app | DESKLAND_NATIVE | Trạng thái và phím điều hướng trên hai màn cần nghiệm thu lại. |
| Jump List | UNSUPPORTED | Danh sách cửa sổ đang mở không phải Jump List của ứng dụng. |
| Icon khay và menu gốc ứng dụng bên thứ ba | BLOCKED | Chưa có collector/callback đã kiểm chứng khi taskbar ẩn. Fallback mở Explorer thuộc Windows. |
| Đồng hồ/lịch | PARTIAL | Đồng hồ và lịch tuần trong Deskland; Notification Center thật do Windows cung cấp. |
| Wi-Fi, Bluetooth, âm thanh, pin và chỉ số máy | PARTIAL | Hiện dữ liệu theo thiết bị; một số thao tác chuyển sang Settings Windows. |
| Windows Settings, nguồn/phiên, Show Desktop, Task View | WINDOWS_DELEGATED | Một số hành động chỉ có ở chế độ tương thích; bản thử chặn thao tác nguồn. |
| Đa màn hình, bàn phím, hồi phục Explorer | PARTIAL | Có code cho từng màn, điều hướng và bảo vệ taskbar; cần kiểm tra thực tế cho bản này. |

Không thử hook/inject vào Explorer trên máy đang dùng. Không dùng fixture hợp tác để đánh dấu khay bên thứ ba PASS. Giữ phím thoát Ctrl+Alt+Shift+F12, hồ sơ thử cách ly và PR #1 ở trạng thái chưa merge.
