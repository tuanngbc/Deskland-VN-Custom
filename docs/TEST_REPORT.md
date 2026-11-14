# Báo cáo kiểm thử Deskland Preview 0.3.0 — tiếng Việt

Ngày kiểm tra: 14-11-2026. Bản chuyển 16 theme và giao diện tiếng Việt đã qua
kiểm tra tự động và chạy native trên hai màn hình thật. Không phát hành GitHub
Release hoặc merge main. Các giới hạn nghiệm thu hệ thống ở cuối báo cáo vẫn
được giữ rõ ràng.

## Bản Việt hoá 0.3.0

- `host/test.ps1`: **35 C# + 28 JavaScript = 63 PASS, 0 FAIL**. Bao gồm routing
  đa màn hình, lưu cấu hình, kiểm tra 16 trang theme, phụ thuộc cục bộ, bản dịch
  tham số và nhịp 30fps ở màn hình 60/75Hz.
- Lượt `artifacts/vn-theme-final2/acceptance/results.json`: **170 NATIVE PASS,
  0 FAIL**. WebView2 thật; không dùng trang preview mô phỏng để thay bằng chứng.
- Đủ 16 thẻ theme, 13 ảnh thu nhỏ giải mã được và 3 nền CSS từ gói tham khảo.
  Không ghi đè những thumbnail thủ công đã có trong kho.
- 16 theme × 3 chế độ Đồng bộ, Độc lập, Toàn cảnh. Đồng bộ/Độc lập có hai
  native surface; Toàn cảnh có một world 3640×1920. Kiểm tra khung hình tăng,
  không lỗi JavaScript, tham số mặc định khớp registry, nhận vị trí biểu tượng,
  đúng số cửa sổ và lớp nền nằm dưới biểu tượng desktop.
- Mỗi tổ hợp kiểm tra tạm dừng, gửi vị trí icon thử rồi khôi phục vị trí đã đọc
  từ Windows và xuất PNG. Không di chuyển icon thật của người dùng. Kiểm tra
  truyền giá trị tối đa của các điều khiển cho cả 16 theme.
- Xuất video thành công với Sân bóng, Mèo con và Bể cá vàng. Nhận dữ liệu video
  trực tiếp từ renderer; lượt tự động không ghi đè clipboard của người dùng.
- Kiểm tra Settings/khay tiếng Việt, lưu và đọc lại lựa chọn ngôn ngữ/theme.
  Có PNG native cho từng theme và màn hình trong thư mục evidence ở trên.
- Gói Windows 0.3.0 cài thử với `/ISOLATED`: **28 NATIVE PASS, 0 FAIL** ở
  `artifacts/vn-package-native/acceptance/results.json`. Bản ZIP giải nén chạy
  cùng lượt kiểm tra: **28 NATIVE PASS, 0 FAIL** ở
  `artifacts/vn-portable-native/acceptance/results.json`. Hai lượt này chạy Sân
  bóng/Bể cá vàng qua cả ba chế độ và xác nhận nhãn chia sẻ tiếng Việt.
- Đối chiếu SHA-256 đủ **77 tệp** của bản cài và ZIP; gói nguồn có **91 tệp**,
  gồm host, đủ 16 renderer, helper, shader và thông báo nguồn. Bản ZIP có thêm
  `Mo-Deskland.cmd`. Gỡ bản cài thử thành công, giữ đăng ký của bản cài sẵn.
  Khi đóng gói lại báo cáo này, tiếp tục đối chiếu tệp runtime với gói đã thử.

Môi trường hiện tại: Windows 11 build 26200; WebView2 157.0.4341.46; GTX 1070;
2560×1080 tại (0,0) và 1080×1920 tại (-1080,-685), cùng 96 DPI. Đọc được 45 icon
trên desktop chính. Canvas desktop dùng kích thước thực, tới DPR 2; khung xem
trước được giảm độ phân giải để giảm chi phí.

Mẫu khung hình cho mỗi theme chỉ dài khoảng một giây, có thể gồm khởi tạo cảnh.
Đa số gần 30fps; Thảo nguyên độ phân giải cao có mẫu khoảng 15–18fps khi khởi
tạo. Đây không phải kiểm thử hiệu năng dài hạn và không khẳng định mọi cảnh
đạt 30fps trên mọi máy. Số đo RAM/FPS của EXE Rust tham khảo không áp dụng cho
bản C# này.

Đã sửa qua các lượt thử: thiếu RNG cho cây dừa Maldives, thiếu kênh host native
ở Sân bóng, xuất ảnh dùng biến đếm khung hình không tồn tại ở một số trang,
và video Sân bóng không nhận timestamp. Lượt thăm dò `vn-theme-acceptance` có
lỗi trước khi sửa, chỉ dùng `vn-theme-final2` làm kết quả cuối cho 16 theme.

Chưa chuyển cơ chế Raw Input tương tác desktop, khóa màn hình và screensaver
của bản Rust. Tương tác chuột của theme dùng trong khung xem trước. Hot-plug,
mixed DPI, xoay màn hình, Explorer restart, sleep/resume và chạy nhiều giờ chưa
được nghiệm thu lại trong lượt Việt hoá này.

## Lịch sử checkpoint đa màn hình 0.2.0

Phần dưới ghi bằng chứng của bộ máy đa màn hình trước khi chuyển 16 theme.
Các số đếm và hiệu năng ở đây thuộc checkpoint đó.

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

Ở checkpoint 0.2.0, chưa hoàn tất các gate nêu trên và 11 theme còn khóa. Bản 0.3.0 đã thay hướng mở rộng cũ bằng việc chuyển 16 theme theo yêu cầu mới. Panorama pause riêng theo monitor và tối ưu vùng trống/world cực lớn vẫn là giới hạn. Các bước nghiệm thu hệ thống tiếp theo gồm hot-plug/primary/DPI/rotate, Explorer restart, sleep/lock/Win+D, video đường nối, soak test và thử integrated GPU; không tính các bước đó là đã hoàn thành trong bản Việt hoá.
