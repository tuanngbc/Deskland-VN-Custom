# Deskland Preview — Tiếng Việt

Hình nền động thích ứng với vị trí biểu tượng desktop, hỗ trợ nhiều màn hình.
Bản 0.4.0 chuyển các trang theme từ gói cộng đồng **Deskland-VN 1.0.2** do
người dùng cung cấp vào host **C# + WebView2** của dự án này.

## Mở ứng dụng

Giải nén bản ZIP đầy đủ rồi mở `Mo-Deskland.cmd`, hoặc cài bằng bộ cài Windows.
Từ mã nguồn:

```powershell
powershell -ExecutionPolicy Bypass -File host\build.ps1
host\bin\Deskland.exe
```

Giữ vị trí tương đối của `host/` và `prototype/`. Nhấp đúp biểu tượng khay hệ
thống hoặc nhấn `Ctrl+Alt+H` để mở Cài đặt. Chọn **Thoát** ở khay hoặc chạy
`host\bin\Deskland.exe --exit` để trả desktop về hình nền Windows.

Tiếng Việt là mặc định cho cấu hình mới. Cấu hình đã có giữ lựa chọn cũ; có thể
chọn **Tiếng Việt** trong danh sách ngôn ngữ. Tiếng Anh và giao diện Trung giản
thể vẫn được giữ. Cấu hình, log và dữ liệu WebView2 ở
`%LOCALAPPDATA%\DesklandPreview`; không dùng chung dữ liệu với bản Rust.

## Chủ đề và đa màn hình

16 theme: Thảo nguyên, Bãi biển, Đảo Maldives, Hồ nước, Bầu trời, Đại dương,
Rừng mưa nhiệt đới, Hoa mận Mộc Châu, Hoa anh đào Tết, Đồi mèo nắng ấm,
Sân bóng, Bể cá vàng, Chuyến xe chiều, Hiên nhà ngày hạ, Mèo con và Võ đài
quyền anh. Mỗi theme có renderer và tham số thực trong gói nguồn.

- **Đồng bộ:** dùng chung theme và tham số trên các màn hình, giữ đúng tỷ lệ.
- **Độc lập:** chọn màn hình trên sơ đồ rồi chọn theme và tham số cho màn hình đó.
- **Toàn cảnh:** một cảnh lớn qua toàn desktop, kể cả màn hình có tọa độ âm.

Chọn **Xem trước trước khi áp dụng** để thử rồi bấm **Áp dụng**. Có nút tạm dừng,
tải lại, đặt lại tham số và khôi phục cấu hình hoạt động.
Các theme ngoài trời dùng chung môi trường offline theo giờ hệ thống: ánh sáng
chuyển dần qua bình minh, ban ngày, giờ vàng và ban đêm; mặt nước, cây, lá và
đom đóm nhận gió nền, cơn gió và hướng gió biến thiên theo ngày. Mỗi renderer
chỉ đọc một mẫu môi trường mỗi giây, nên mục tiêu 30fps và mức tiêu thụ thấp
được giữ nguyên. Theme có lựa chọn thời điểm thủ công vẫn ưu tiên lựa chọn đó.
Trong mục **Môi trường động**, có thể tắt riêng thay đổi theo thời gian hoặc
thời tiết. Bộ thời tiết gồm tự nhiên theo ngày, trời quang, nhiều mây, mưa nhẹ,
giông gió và tuyết rơi; mưa, tuyết và chớp được vẽ trên một lớp cục bộ nhẹ.
Mục **Màn hình chờ và tiết kiệm tài nguyên** tạo một vòng cảnh đã nén ở 6fps,
đăng ký `DesklandSaver.scr` với Windows và giải phóng WebView2 khi máy khóa,
tắt màn hình hoặc vào màn hình chờ. Khi quay lại, Windows yêu cầu đăng nhập và
Deskland dựng lại hình nền. Có thể dùng một frame làm ảnh nền màn khóa Windows;
đây là ảnh tĩnh theo giới hạn của Windows, không phải màn hình khóa giả.
Xem [hướng dẫn bản tiếng Việt](docs/VIETNAMESE_EDITION.md),
[hướng dẫn tạo theme](docs/THEME_AUTHORING.md),
[hướng dẫn đa màn hình](docs/MULTI_MONITOR.md) và [báo cáo kiểm thử](docs/TEST_REPORT.md).

## Yêu cầu và đóng gói

Windows 10/11 x64 và Microsoft Edge WebView2 Runtime. Biên dịch bằng trình C#
của .NET Framework có sẵn trên Windows. Lần build đầu cần mạng để lấy WebView2
SDK; bản ZIP đã chứa các thư viện runtime tương ứng. Xem trước trong trình
duyệt cần Node.js:

```powershell
node prototype\serve.mjs
# http://localhost:5178/settings.html
powershell -ExecutionPolicy Bypass -File host\test.ps1
powershell -ExecutionPolicy Bypass -File tools\build-installer.ps1
```

`dist/` chứa bộ cài, ZIP tiếng Việt và SHA-256. Cả hai gói kèm
`Deskland-source.zip` chứa mã nguồn tương ứng. Không tự phát hành release hoặc
merge vào main. Bộ cài dùng `%LOCALAPPDATA%\Programs\Deskland Preview`;
gỡ cài đặt giữ cấu hình người dùng.

## Phạm vi bản chuyển

Giữ bộ máy nhiều màn hình của Preview. Không sử dụng EXE Rust, bộ đo RAM/FPS
của Rust, chức năng khóa Windows, screensaver hoặc tự đổi ảnh màn khóa của gói
tham khảo. Theme có thao tác chuột dùng được trong khung xem trước; tương tác
Raw Input trên desktop của Rust chưa được chuyển. Chia sẻ ảnh/video dùng icon
mẫu và ô màu thay tên file, không chụp cửa sổ hay nội dung file của người dùng.

Các tình huống hot-plug, mixed DPI, xoay màn hình, Explorer restart,
sleep/resume và kiểm thử nhiều giờ vẫn cần nghiệm thu thêm; xem báo cáo.

## Nguồn và giấy phép

Copyright (C) 2026 Deskland authors. Dự án gốc:
[BLEZZ0102/Deskland](https://github.com/BLEZZ0102/Deskland).
Phần theme và tiếng Việt tham khảo từ bản cộng đồng
[nhhoang188/Deskland-VN](https://github.com/nhhoang188/Deskland-VN).

Giữ [GPL-3.0](LICENSE), [NOTICE](NOTICE),
[thông báo gốc của bản cộng đồng](third_party/deskland-vn/NOTICE),
[hồ sơ các file nhập](third_party/deskland-vn/IMPORT.json),
[giấy phép phụ thuộc](THIRD_PARTY_NOTICES.md) và [quy định tên/icon](TRADEMARKS.md).
Bản sửa đổi này không ngụ ý là bản phát hành chính thức hay được tác giả gốc bảo trợ.

## English

This Vietnamese port adapts the sixteen browser themes from the user-supplied
Deskland-VN 1.0.2 archive to the existing Deskland Preview C# / WebView2 host,
including Mirror, Independent and Panorama modes. New profiles default to
Vietnamese; existing language preferences are preserved. The reference Rust
executable and Windows lock-screen features are not included. See the source
notices, import hashes and test report linked above.
