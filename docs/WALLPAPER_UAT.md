# TLauncher 1.0.3 — WALLPAPER TEST

Phase 1 đã được owner nghiệm thu 11/11 trên c011c2b11b3d. Đây là Phase 2,
chờ owner nghiệm thu hình ảnh, tương tác và hiệu năng trên máy thật.

Bản 027f437 đã bị owner đánh FAIL về gallery/tương tác. Bản sửa cần kiểm tra
lại cả 16 cảnh; CI PASS không thay cho owner PASS. Xem `WALLPAPER_LAYER_AUDIT.md`
để đối chiếu lỗi xếp lớp và đặt vật thể đã phát hiện.

Thoát NATIVE BASE TEST đang mở bằng menu TLauncher và chờ cửa sổ khởi chạy
khôi phục production. Sau đó tự mở **TLauncher 1.0.3 — WALLPAPER TEST** trên
Desktop. Giữ cửa sổ khởi chạy mở. Không mở EXE trực tiếp hoặc hai bản TEST cùng
lúc. Thoát TEST bình thường để trở lại production 1.0.2 nếu trước đó đang chạy.
NATIVE BASE TEST được giữ nguyên để so sánh.

Nhấp phải vùng trống Dock → **Settings → Live Wallpaper**. Wallpaper được bật
sẵn với Beach, 25 FPS, Synchronized, độ phân giải đầy đủ. Chọn thumbnail để xem
trực tiếp trên Desktop; các tùy chỉnh bên dưới áp dụng ngay. Tab/Shift+Tab chuyển
điều khiển, Enter/Space kích hoạt; Up/Down đổi trang Settings, con lăn cuộn trang.

| Mục | Cách thử |
| --- | --- |
| Gallery và tương tác | Thử đủ 16 cảnh, nhấp trái vùng trống Desktop. Meadow có cậu bé, diều và mèo; biển có gợn sóng/dấu cát/thuyền; Sky có mây và sao băng ban đêm; các cảnh hoa có cánh hoa; Cats/Cat Hill có vuốt mèo và cuộn len. |
| Câu cá | Lake/Aquarium: click nước để thả câu, chờ phao báo cá cắn rồi click để kéo. Bấm sớm làm cá bỏ chạy hoặc thu câu. |
| Stadium | Click sân để đưa bóng tới điểm đó; click cầu môn để sút, theo dõi cầu thủ đuổi bóng và hiệu ứng ghi bàn. |
| Boxing Ring | Click võ sĩ để đấm theo vị trí, bao cát để làm đung đưa, lốp xe để đập búa, đồng hồ để sang hiệp mới. Có điều chỉnh lực/nảy và chế độ đấu tự động. |
| Summer Porch | Click Corgi để đổi tư thế; quạt đổi nấc; chuông gió rung; dưa hấu bị cắn rồi tự thêm lại; chim xanh nhảy. |
| Hai màn hình | Independent cho phép mỗi màn hình một cảnh và tùy chỉnh riêng; Synchronized dùng cùng cảnh/thời gian; Panorama nối một thế giới qua hai màn hình. Thử màn ngang 2560×1080 và dọc 1080×1920, đổi màn chính và kết nối lại. |
| Weather/Lighting | Chọn Clear/Rain/Snow/Clouds/Offline. Live dùng thành phố trong Control Center → Weather. Solar dùng tọa độ chung khi có; tắt Use current time để chọn giờ thủ công. Không có yêu cầu đổi Windows location/privacy. |
| Vòng đời | Pause giữ khung hình cuối. Live Wallpaper Off phải dừng renderer và giải phóng tài nguyên; bật lại không restart app. Thử khóa/mở máy, màn hình tắt/bật, fullscreen, che từng màn hình, đổi cảnh nhiều lần. |
| Native Core | So sánh Genie/Drop/Jelly, Launcher, menu, Dock, Notch, Snap và Control Center với NATIVE BASE TEST. Giữ timing/easing của nền đã nghiệm thu. Ghi ứng dụng và màn hình nếu thấy viền khi minimize/restore. |
| Tài nguyên | Khi muốn đo, tự mở **NATIVE-RESOURCE.cmd** trong thư mục candidate. Lấy mẫu sử dụng bình thường; JSON ở `diagnostics`. Đo tổng main + Guardian + helper của bản TEST. Không tự chạy benchmark hoặc gửi dữ liệu ra ngoài. |

GPU DirectComposition được ưu tiên khi driver hỗ trợ WGL/D3D interop. Nếu không,
renderer dùng readback/DIB có hàng đợi giới hạn. CI dùng GPU phần mềm nên không
chứng minh đường interop hoặc mức tiêu thụ GTX 1070. Mục tiêu RAM 100–150 MB,
CPU idle ~1%, GPU ~3% vẫn cần số đo thực tế; không coi đó là kết quả đã đạt.

Hồ sơ mới nằm trong `E:\TLauncher-UAT\profiles\tlauncher-native-trial-wallpaper-<commit>`.
Không đọc/ghi đè hồ sơ production, không đổi startup/security/privacy. Cấu hình
wallpaper có schema, kiểm tra giá trị, ghi nguyên tử, backup và giữ trường chưa biết.
Import/export và chuyển dữ liệu production tổng thể vẫn thuộc Phase 4.

Dynamic Notch vẫn có công tắc thật; Auto Update bị loại trừ. Stage Manager,
Unified Spotlight và các tiện ích còn thiếu vẫn trong phạm vi 1.0.3, chỉ triển khai
sau các gate owner tương ứng. Dừng tại nghiệm thu Phase 2; không publish release.
