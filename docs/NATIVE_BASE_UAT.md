# TLauncher 1.0.3 — NATIVE BASE TEST

Đây là Phase 1: nền FluidWin C++ native, logo TLauncher, namespace/cấu hình TEST
riêng, Dynamic Notch và công tắc tắt thật. Auto Update bị loại hoàn toàn.
Wallpaper động, Stage và Spotlight TLauncher chưa tích hợp; vẫn giữ trong 1.0.3.

Mở **TLauncher 1.0.3 — NATIVE BASE TEST** trên Desktop. Thoát bản TEST cũ hoặc
FluidWin gốc trước; giữ cửa sổ khởi chạy mở. Launcher yêu cầu production 1.0.2
thoát bình thường, chờ singleton/Guardian, rồi mở native candidate với hồ sơ riêng.
Không chạy trực tiếp EXE. Thoát bằng menu Dock/Settings/khay TLauncher; launcher
chờ cả Guardian kết thúc rồi khôi phục production nếu trước đó đang chạy.
Khẩn cấp: **Ctrl+Alt+Shift+F12**, cơ chế Guardian gốc.

| Chức năng | Cách kiểm tra |
| --- | --- |
| Launcher | Bấm Windows hoặc nút Launcher trên Dock. Layout/search/icon/menu/uninstall giữ implementation FluidWin. Settings → Keyboard có thể trả phím Windows cho Start gốc. |
| Genie/Drop/Jelly | Settings → Animations. Chọn Genie hoặc Drop; thử minimize/restore từ Dock và nút tiêu đề, đảo chiều giữa chừng, mở/đóng, maximize/restore, kéo cửa sổ thường/phóng to. Giữ timing/easing gốc để đối chiếu FluidWin. |
| Menu Desktop/Explorer | Nhấp phải Desktop, khoảng trống Explorer và tệp/thư mục. Thử submenu, Open with, Rename, Share; Shift+nhấp phải dùng menu Windows. |
| Control Center/Settings | Biểu tượng cuối Dock mở Control Center gốc. Nhấp phải vùng trống Dock → Settings mở Settings native. Các tiện ích dùng giao diện upstream. |
| Dynamic Notch | Settings → Dynamic Notch: công tắc Dynamic Notch tắt/bật thật, **Show always** bật = luôn hiện; tắt = chỉ hiện khi có hoạt động. Thử âm lượng, nhạc, timer, file shelf, fullscreen; tắt rồi bật lại không restart. Lựa chọn được lưu. |
| Snap/Overview/Search | Win+Z, Win+mũi tên, kéo cạnh/góc, rê nút maximize; Alt+Tab tổng quan; Win+Tab tìm app/file/folder/settings/tính toán và sóng màu. |
| Tài nguyên | Chủ động mở **NATIVE-RESOURCE.cmd** cạnh EXE; lấy mẫu sử dụng bình thường 60 giây. JSON ở diagnostics. Tính main, Guardian và helper được nhận diện. Không tự benchmark. |

Kiểm tra trên ngang 2560×1080 và dọc 1080×1920: khởi động lạnh/ấm, lần mở Launcher
và Settings đầu, Genie ultrawide, Jelly portrait, Dock đổi màn hình, Snap và
fullscreen. Ghi rõ app/hiệu ứng/màn hình khi thấy viền màu lúc thu nhỏ/phục hồi.
Chưa kết luận nguyên nhân là capture indicator; không đổi Windows privacy/security.

UAT chặn thay đổi startup và hình nền Windows lâu dài. Không dùng profile production
hoặc registry FluidWin gốc. Một số chức năng phụ thuộc quyền Windows/hardware vẫn
có native fallback. Bản native Phase 1 giữ các ngôn ngữ upstream; bản dịch vi/zh
đầy đủ thuộc Phase 4, không được coi đã hoàn tất.

**CI PASS khác owner acceptance. Dừng tại Phase 1; chỉ tiếp tục Phase 2 sau khi
owner xác nhận nền tảng mượt và ổn định. Không publish release.**
