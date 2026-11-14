# Báo cáo kiểm thử và checkpoint

Trạng thái: **PARTIAL VERIFIED — NATIVE MULTI-MONITOR PENDING**. Ba mode đã chạy trên hai output thật; các tình huống thay đổi topology và lifecycle hệ thống bên dưới chưa đủ nghiệm thu. Không phát hành release hoặc merge main.

## Baseline và môi trường

Baseline commit `9818b577402ff476e1b59da07970c56472713126`, clone sạch trên branch `codex/deskland-multimonitor`. Baseline build PASS và EXE gốc chạy với profile riêng. Baseline chỉ tạo wallpaper trên màn hình chính. Có bản Preview 0.1.3 cài sẵn: giữ dữ liệu profile gốc; đóng tạm khi chạy nghiệm thu để hai host không tranh Z-order. Sau kiểm thử installer thường, bản cài gốc 0.1.3 được repair bằng installer có SHA khớp bản phát hành, để trả lại đăng ký/shortcut. Installer mới bổ sung `/ISOLATED` cho kiểm thử tiếp theo.

Windows 11 Pro build 26200, WebView2 157.0.4341.46, NVIDIA GeForce GTX 1070. Hai output do Windows báo: 2560×1080 primary tại (0,0), 1080×1920 secondary tại (-1080,-685). Cả hai 96 DPI; secondary 75Hz. World bounding rectangle: (-1080,-685,3640,1920). Không coi output hoặc DPI mô phỏng là phần cứng thật.

## Regression tự động

`host/test.ps1`: **33 C# + 10 JavaScript = 43 test**, gồm property roundtrip 1000 hình chữ nhật, topology giả lập 1–4 màn hình, origin âm, khoảng trống và bố trí lệch hàng, icon cắt qua mép, DPI/primary/resolution thay đổi vẫn giữ ID, hot-plug events và bỏ qua thay đổi trùng, settings migration/backup/future schema, scoped params, message validation, bounded retry, deterministic RNG và cú pháp renderer/settings. Chạy lại sau thay đổi source cuối; kết quả build và test được lưu trong artifacts, không commit binary/cache/profile.

## Native acceptance

Lượt `acceptance-release` ghi **34 NATIVE PASS**, 3 mẫu hiệu năng; restart thêm 3 NATIVE PASS. Kiểm tra dùng WebView2 thật, không dùng preview browser mock làm bằng chứng native.

| Test | Trạng thái | Bằng chứng / giới hạn |
|---|---|---|
| M01 một màn hình | AUTOMATED PASS; NATIVE NOT TESTED | Không rút màn hình thật để ép topology còn một output |
| M02 Mirror hai màn hình | NATIVE PASS | Hai native surfaces đúng kích thước, Beach và Meadow render |
| M03 Independent | NATIVE PASS | Theme khác nhau, scoped parameters, không reload renderer còn lại |
| M04 Panorama | NATIVE PASS | Một native world surface 3640×1920, 44 icon, không nhân đôi world |
| M05 ba/bốn màn hình | AUTOMATED PASS; HARDWARE UNAVAILABLE | Geometry/planning, không có native 3–4 output |
| M06 origin âm | NATIVE PASS | Secondary (-1080,-685) |
| M07 khác độ phân giải | NATIVE PASS | 1080×1920 và 2560×1080 |
| M08 mixed DPI | AUTOMATED PASS; NATIVE NOT TESTED | Hai output hiện tại cùng 96 DPI |
| M09 hot-plug | AUTOMATED PASS; NATIVE NOT TESTED | Event/identity/routing giả lập, chưa rút cáp |
| M10 đổi primary | AUTOMATED PASS; NATIVE NOT TESTED | Identity/topology giả lập |
| M11 khôi phục settings | NATIVE PASS | Save/load và restart process cùng profile |
| M12 chuyển mode | NATIVE PASS | Chuyển mode trong Settings thật, theme cả Beach và Meadow |
| M13 di chuyển icon | AUTOMATED PASS; NATIVE PARTIAL | Đọc 44 icon thật và adaptation; chưa di chuyển icon của owner |
| M14 Explorer restart | NOT TESTED | Chưa restart Explorer của owner |
| M15 per-monitor pause | NATIVE PASS | Manual pause giữ trạng thái màn hình còn lại; disposable fullscreen window chỉ pause output bị che |
| M16 Quit/restore | NATIVE PASS (process/log); visual pending | Thoát EXE và uninstall sạch; chưa chụp đối chiếu desktop trước/sau |
| M17 đường nối Panorama | NATIVE PARTIAL | Ảnh desktop thật và một world chung; chưa quay chuyển động qua mép hoặc đo pixel theo thời gian |
| M18 WebView2 crash | NATIVE PASS | Inject renderer crash và browser crash; wallpaper cùng Settings phục hồi |
| M19 startup/thoát nhiều lần | NATIVE PARTIAL | Chạy lại EXE và installed `--autostart`; chưa reboot/logon Windows |
| M20 resource leak | NOT TESTED dài hạn | Có snapshot 10 giây; chưa soak test nhiều giờ |

Lock/unlock, sleep/resume, Win+D, đổi DPI/resolution/rotate trong khi chạy chưa native tested. Computer-use helper lỗi khởi tạo nên thay bằng opt-in native acceptance chạy bên trong host. Không khẳng định đã click/quan sát trực tiếp mọi control hoặc thao tác desktop icons.

## Hiệu năng đo thực tế

Mỗi mẫu 10 giây, theme Meadow, Settings cũng đang mở. CPU là phần trăm tổng máy tính từ host + các process WebView2 của profile. RAM là tổng working set, có thể đếm lại shared pages; không phải GPU RAM hay private bytes.

| Mode | FPS mỗi renderer | CPU tổng máy | Tổng working set |
|---|---:|---:|---:|
| Mirror | 29.99 / 29.99 | 5.47% | 794.7 MiB |
| Independent | 29.96 / 30.06 | 4.49% | 804.8 MiB |
| Panorama | 30.00 | 2.46% | 701.4 MiB |

Chưa đo trên integrated GPU hoặc 3–4 màn hình, không tuyên bố mục tiêu hiệu năng đạt ở các môi trường đó. Lượt cũ có FPS âm do renderer/scene thay trong cửa sổ lấy mẫu: mẫu đó không hợp lệ và không dùng trong bảng này.

## Repair loop và bước tiếp theo

Đã sửa: per-window tranh slot sau DefView (một coordinator sắp thứ tự tất cả surfaces), settings restore không gửi lại params vào renderer đang giữ cùng theme, foreground Settings gây pause không cần thiết, kiểm thử params chưa chờ message async, snapshot restart ghi đè kết quả trước, khởi tạo script build mất BOM, thiếu `Themes.cs` trong corresponding source ZIP, main browser crash khiến Settings không tự phục hồi. Các test native có nhiễu foreground trước đó vẫn còn nguyên log trong artifacts; dùng kết quả mới sau retest.

Chưa hoàn tất các gate nêu trên. Phase 8 chưa bắt đầu; 11 theme locked giữ nguyên. Panorama pause riêng theo monitor và tối ưu vùng trống/world cực lớn là giới hạn hiện tại, cần owner xem xét khi nghiệm thu yêu cầu đầy đủ. Bước tiếp theo: native hot-plug/primary/DPI/rotate, Explorer restart, sleep/lock/Win+D, video đường nối, soak test và thử integrated GPU. Thực hiện trong session được owner cho phép đổi topology, giữ dữ liệu icon nguyên trạng.
