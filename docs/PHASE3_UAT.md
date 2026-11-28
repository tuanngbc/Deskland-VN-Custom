# TLauncher 1.0.3 — STAGE & SPOTLIGHT TEST

Tài liệu cho bản TEST Phase 3 tổng hợp khi được bàn giao. CI PASS chưa phải owner nghiệm thu; các giới hạn thực tế phải đi kèm báo cáo của đúng candidate.

Mở lối tắt **TLauncher 1.0.3 — STAGE & SPOTLIGHT TEST** trên Desktop. Giữ cửa sổ khởi chạy mở. Launcher yêu cầu production thoát bình thường, chờ singleton/Guardian, rồi mở TEST với profile riêng. Khi TEST thoát, launcher khôi phục production nếu trước đó đang chạy. Không chạy trực tiếp EXE hoặc mở đồng thời hai desktop shell.

| Tính năng | Vị trí và cách dùng |
| --- | --- |
| Tiếng Việt | Settings → Language → Tiếng Việt. Đổi trực tiếp; đóng/mở TEST để kiểm tra lựa chọn được giữ. |
| Stage Manager | Settings → Stage Manager: bật, chọn màn hình/cạnh, thẻ phẳng hoặc phối cảnh, kích thước/khoảng cách/độ mờ. Nhấp thẻ để kích hoạt; cuộn trên nhóm hoặc nhấp phải để chọn cửa sổ cùng app. |
| Stage Peek | Bật trong Settings → Stage Manager. Rê thẻ và chờ preview; rời chuột để đóng. Cửa sổ thu nhỏ dùng ảnh hợp lệ đã lưu, không tự mở lại để chụp. |
| Spotlight | Settings → Launcher & Spotlight: chọn Compact hoặc Fullscreen. Alt+Space mở/đóng; F6 đổi chế độ; mũi tên chọn; Enter mở; Escape đóng. Phím mở có thể cấu hình. Launcher FluidWin và Start Windows vẫn là lựa chọn riêng. |
| Thư viện app | Fullscreen Spotlight: nhấp phải để thêm EXE/LNK, đổi tên/icon, yêu thích, ẩn/khôi phục, tạo/sửa thư mục hoặc ghim Dock. Kéo app để sắp xếp; xóa thư mục không gỡ cài đặt ứng dụng. |
| Dock thu nhỏ | Settings → Dock → Chế độ hiển thị → Thu nhỏ thành thanh. Rê vào thanh để mở; rời Dock và popup để thu lại. Thử cả bốn cạnh, Live Preview, menu và kéo thả. |
| Wallpaper | Settings → Hình nền: bật/tắt, chọn theme, tham số scene, Independent/Synchronized/Panorama, màn hình, thời tiết và giới hạn FPS. Boxing có lựa chọn tên cho giao đấu, kiểu găng và HUD. |
| Dynamic Notch | Settings → Dynamic Notch: bật/tắt thật, giữ lựa chọn qua restart. Auto Update được loại trừ. |
| Chẩn đoán | Settings → Diagnostics: xuất thông tin đường trình bày GPU/DIB, FPS, thời gian frame, readback và tài nguyên thực tế. Giá trị unavailable là chưa đo được. |

Profile TEST bền vững được giữ riêng tại `E:\TLauncher-UAT\profiles\tlauncher-native-trial-owner-1-0-3`; gói EXE thay đổi theo commit. Migration chỉ thực hiện sau khi phiên cũ thoát và lưu xong, gồm registry và tệp/asset cá nhân. Không dùng profile production. Không tự sửa security/privacy/startup Windows.

## Đo tài nguyên khi owner muốn

Sau khi tự mở TEST, mở **NATIVE-RESOURCE.cmd** cạnh EXE. Công cụ hỏi tên tình huống, lấy mẫu 60 giây sử dụng bình thường và lưu JSON vào `diagnostics`. Công cụ không tự đổi tính năng, đổi chất lượng/FPS hoặc tải dữ liệu lên mạng.

So sánh từng tình huống với cùng workload, màn hình, theme, FPS và resolution:

| Mã | Cấu hình owner tự chọn trước khi đo |
| --- | --- |
| A | Native Base đã nghiệm thu, dùng công cụ trong package tương ứng nếu có. |
| B | Chỉ Wallpaper. |
| C | Chỉ Stage. |
| D | Chỉ Spotlight. |
| E | Wallpaper + Stage. |
| F | Wallpaper + Spotlight. |
| G | Wallpaper + Stage + Spotlight. |
| H | Tắt Wallpaper, Stage và Spotlight. |

Báo cáo gồm RAM/private working set/private bytes, CPU, GPU theo adapter, dedicated/shared GPU memory, thread và handle của main, Guardian và helper được nhận diện. Bộ lấy mẫu không tính vào tổng app. Working set và GPU shared memory có thể đếm trùng phần dùng chung giữa tiến trình; counter không có dữ liệu giữ unavailable. Số đo software GL trên CI không chứng minh mục tiêu GTX 1070/RAM 100–150 MiB/GPU 3%/CPU 1% trên máy owner.

Để đối chiếu lỗi, ghi commit và ứng dụng/màn hình/thao tác gây lỗi. Giữ animation speed 0.70x theo thiết lập owner; không tự reset profile để làm phép đo đẹp hơn. Thử Genie/Drop/Jelly, menu Desktop/Explorer, Control Center, Notch, Snap, fullscreen, đổi màn hình và thu nhỏ/phục hồi cửa sổ. Chỉ owner quyết định PASS/FAIL và nghiệm thu Phase 3.

### FULL FEATURE TEST

- **Dock**: vào Settings → Dock để chọn màn hình, thanh thu nhỏ và ảnh nút Start. Trong Start, dùng menu chuột phải để ghim, kéo các mục ghim để đổi thứ tự.
- **Stage**: Settings → Stage Manager có nền rail/thẻ riêng và chế độ luôn hiện dành vùng cho Stage. Kiểm tra maximize, Snap và chuyển màn hình.
- **Đồng hồ**: Settings → Desktop clocks để cài đặt hai đồng hồ độc lập, dùng Classic để đối chiếu với 1.0.2.
- **Chụp/quay**: Ctrl+Shift+S mở chọn ảnh; Ctrl+Shift+R bắt đầu chọn vùng quay hoặc dừng và lưu. Space chọn cả màn hình dưới con trỏ; Esc hủy chọn; Ctrl+Shift+P tạm dừng/tiếp tục. Settings → Keyboard có 30/60 FPS, âm thanh và thư mục lưu.
- **Wallpaper**: thử thêm một shortcut cạnh mép nước; cây hợp lệ nên giữ vị trí và thuyền mắc bờ tìm lại nước.
- **Tài nguyên**: dùng NATIVE-RESOURCE.cmd trong gói TEST khi cần đo; công cụ không tự chạy.
