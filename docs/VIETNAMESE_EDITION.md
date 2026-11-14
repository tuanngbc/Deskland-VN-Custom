# Deskland Preview — bản tiếng Việt 0.3.0

Bản này chuyển các trang theme từ gói Deskland-VN 1.0.2 do người dùng cung cấp
vào host C# + WebView2 của Deskland Preview, giữ ba chế độ đa màn hình hiện có.
Hướng thử nghiệm renderer đơn giản trước đó đã được hủy bằng commit revert;
không sử dụng lại các cảnh thử nghiệm đó.

## Chạy bản đã đóng gói

- Bản ZIP: giải nén toàn bộ rồi mở `Mo-Deskland.cmd`, hoặc `host/bin/Deskland.exe`.
- Bản cài đặt: chạy bộ cài 0.3.0 và mở Deskland Preview.
- Cấu hình mới dùng tiếng Việt. Cấu hình cũ giữ ngôn ngữ đã lưu; chọn
  **Tiếng Việt** trong danh sách **Language / Ngôn ngữ** nếu cần.
- Chọn theme trong Cài đặt. **Đồng bộ** dùng chung theme; **Độc lập** cho phép
  chọn theme theo màn hình; **Toàn cảnh** dùng một cảnh lớn qua toàn desktop.
- Nhấp đúp biểu tượng khay hệ thống để mở lại Cài đặt; chọn **Thoát** để trả
  desktop về hình nền Windows. Phím mở Cài đặt là `Ctrl+Alt+H`.
- Cấu hình và log: `%LOCALAPPDATA%\DesklandPreview`. Dữ liệu của gói tham khảo
  `.deskland-rs` được giữ riêng, không đọc hay chuyển tự động.

## 16 theme

Thảo nguyên, Bãi biển, Đảo Maldives, Hồ nước, Bầu trời, Đại dương, Rừng mưa
nhiệt đới, Hoa mận Mộc Châu, Hoa anh đào Tết, Đồi mèo nắng ấm, Sân bóng,
Bể cá vàng, Chuyến xe chiều, Hiên nhà ngày hạ, Mèo con và Võ đài quyền anh.

Các trang có hỗ trợ tương tác chuột sẽ nhận click trong khung xem trước.
Host hiện giữ các icon Windows nhận thao tác chuột trên desktop; cơ chế Raw
Input tương tác trực tiếp của bản Rust không thuộc bản chuyển giao này.
Các tính năng khóa máy, đổi ảnh màn khóa và screensaver của gói tham khảo
không được chuyển vào. Tên, icon và danh tính dữ liệu vẫn là Deskland Preview.

## Nguồn và kiểm chứng

Giữ GPL-3.0, copyright và thông báo sửa đổi từ hai dự án. Xem `NOTICE`,
`third_party/deskland-vn/NOTICE` và `third_party/deskland-vn/IMPORT.json`.
Không đóng gói file EXE của gói tham khảo. Bộ cài và bản ZIP chứa mã nguồn
tương ứng trong `Deskland-source.zip`.

Kết quả build, kiểm thử và giới hạn native được ghi trong `TEST_REPORT.md`.
Các số đo RAM/FPS trong README hoặc log của bản Rust không phải số đo của
bản C# này.
