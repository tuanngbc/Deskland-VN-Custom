# Deskland Preview: nhiều màn hình

Trong Settings, chọn một trong ba chế độ, không cần khởi động lại ứng dụng:

- **Đồng bộ / Synchronized**: một theme và tham số chung; mỗi màn hình có renderer đúng tỷ lệ riêng. Seed và mốc thời gian shader được dùng chung. Các vật thể mô phỏng theo icon riêng của mỗi màn hình có thể khác quỹ đạo.
- **Độc lập / Independent**: chọn màn hình trên sơ đồ, sau đó chọn theme và tham số của màn hình đó. Theme, tham số và trạng thái pause được lưu theo đường dẫn thiết bị màn hình, không theo số thứ tự hay tọa độ. Cấu hình của màn hình rút ra được giữ lại; màn hình mới dùng theme chung làm fallback.
- **Toàn cảnh / Panorama**: một mô phỏng chạy trên surface bao trọn virtual desktop. Windows cắt surface theo các output thật. Gốc tọa độ âm và các vùng trống giữa màn hình được giữ nguyên. Không kéo giãn ảnh của màn hình chính; không tạo một mô phỏng riêng lặp lại trên mỗi màn hình.

Thông số thao tác theo theme hiện có. **Preview before Apply** giữ các thay đổi theme/tham số/mode trong preview cho đến khi bấm **Apply**. Mặc định thay đổi được áp dụng và lưu tự động. **Reset theme** trả tham số theme về mặc định. **Restore last working** khôi phục snapshot đã Apply khi các renderer báo ready. Reload khởi tạo lại renderer đã lỗi và đặt lại giới hạn retry.

Dịch vụ icon đọc vị trí vật lý từ Shell một lần mỗi chu kỳ 500ms trên worker, chỉ phát khi snapshot thay đổi. Icon sát mép được giữ nguyên hình chữ nhật khi chuyển sang tọa độ local. Panorama nhận toàn bộ icon trong world. Dữ liệu UI và theme dùng virtual host WebView2 dùng chung cache.

Settings schema 2 migrate dữ liệu schema 1 sang Mirror, giữ theme, tham số và trường chưa biết. Lưu file qua temporary + atomic replace, giữ `.bak`; schema mới hơn không bị ghi đè. Log và settings mặc định ở `%LOCALAPPDATA%\DesklandPreview`. Profile cách ly không sửa startup.

```powershell
powershell -ExecutionPolicy Bypass -File host\build.ps1
host\bin\Deskland.exe
# Thoát, trả desktop về wallpaper Windows
host\bin\Deskland.exe --exit
# Regression
powershell -ExecutionPolicy Bypass -File host\test.ps1
# Native acceptance: đóng bản thử khác trước, giữ màn hình ở trạng thái ổn định
$env:DESKLAND_PROFILE_DIR = "$pwd\artifacts\native"
host\bin\Deskland.exe --native-test
```

Native acceptance ghi PNG từ renderer thật và ảnh desktop thật, `environment.json`, `results.json`. Để kiểm tra khôi phục qua process restart, chạy lại cùng profile với `$env:DESKLAND_TEST_STAGE='restart'`; kết quả được ghi riêng vào `restart-results.json`.

Lượt kiểm tra bản Việt hoá dùng `$env:DESKLAND_TEST_STAGE='themes'` để chạy cả
16 theme qua ba chế độ. Giá trị `package` chạy kiểm tra ngắn với Sân bóng và Bể
cá vàng qua ba chế độ từ thư mục đã giải nén/cài đặt. Hai lượt đều kiểm tra
thẻ theme, ảnh thu nhỏ và giao diện tiếng Việt; chỉ lượt `themes` ghi thử video.

Build installer bằng `tools\build-installer.ps1`. NSIS từ mirror phải khớp SHA-256 upstream; tải HTML thay ZIP không được bỏ qua kiểm tra. Installer có `/ISOLATED /D=<thư mục tuyệt đối>` để cài thử mà không đổi shortcut hoặc đăng ký của bản có sẵn. Khi chạy EXE cài thử, vẫn phải đặt `DESKLAND_PROFILE_DIR` riêng. Uninstall bảo toàn dữ liệu người dùng.

## Bản tiếng Việt 0.4.0

Chuyển 16 theme từ gói Deskland-VN 1.0.2 vào host hiện tại và Việt hoá Settings,
khay hệ thống, tên theme và tham số. Chi tiết ở [VIETNAMESE_EDITION.md](VIETNAMESE_EDITION.md).
Các theme nhận epoch, seed và tọa độ icon từ host. Toàn cảnh vẫn là một surface
world chung; các trang không tự nhân đôi cảnh theo màn hình.

## Giới hạn cần nghiệm thu tiếp

Panorama dùng một surface lớn nên vùng trống trong bounding rectangle vẫn có chi phí render. Vượt giới hạn canvas/GPU trên topology rất lớn chưa được nghiệm thu. Để giữ cảnh liên tục, automatic pause của Panorama chỉ dừng cả world nếu foreground che toàn world; pause riêng theo màn hình chỉ áp dụng ở Independent. Hot-plug/DPI/rotate/Explorer restart, lock/unlock và sleep/resume cần native retest trước khi coi là hoàn tất toàn bộ yêu cầu.

Tài liệu kỹ thuật tham chiếu: [WebView2 process failure](https://learn.microsoft.com/microsoft-edge/webview2/concepts/process-related-events), [Windows monitor device information](https://learn.microsoft.com/windows/win32/api/wingdi/ns-wingdi-displayconfig_target_device_name).
