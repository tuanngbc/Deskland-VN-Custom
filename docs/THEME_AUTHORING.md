# Tạo theme cho tuanpc-desk

Gói tuanpc-desk có nhắc tới `CONTRIBUTING.md`. Tệp đó không được đóng trong
ZIP anh gửi, nhưng hướng dẫn gốc trên kho tuanpc-desk mô tả đúng cấu trúc
theme: một trang web, shader (nếu dùng WebGL), đăng ký trong `themes.json`, bản
dịch và thumbnail. Bản Preview C# này dùng phần web đó; không cần Rust để tạo
theme mới.

## Cấu trúc tối thiểu

```text
prototype/my-theme.html        # trang renderer
prototype/my-theme.frag        # tùy chọn: fragment shader WebGL2
prototype/thumbs/my-theme.jpg  # thumbnail 1024×576
prototype/themes.json          # tên, mô tả, tham số, page
prototype/i18n/vi.json         # tên và tham số tiếng Việt
prototype/i18n/en.json         # bản tiếng Anh
```

Mẫu nên bắt đầu từ `prototype/aquarium.html` nếu dùng WebGL hoặc
`prototype/cats.html` nếu dùng Canvas 2D. Giữ các phần hạ tầng sẵn có:

- nạp `world-contract.js` và `theme-runtime.js`;
- nhận `settings`, `icons`, `pause` và `reset` từ host;
- gọi `DeskWorld.configure(m.contract)`, `DeskWorld.frameDue(now)` và
  `DeskWorld.noteDraw()` trong vòng lặp;
- dùng `getBoundingClientRect()` khi tính kích thước canvas; không cố định
  0.72 hoặc 0.85 cho canvas của wallpaper;
- khi đang pause vẫn vẽ lại một frame nếu icon hoặc tham số vừa thay đổi;
- không đọc file riêng tư, không gọi mạng, không hook bàn phím và không tạo
  màn hình khóa giả.

## Dùng môi trường theo giờ hệ thống

`world-contract.js` cung cấp `DeskWorld.environment`. Renderer có thể đọc giá
trị này trong lúc vẽ; dữ liệu được làm mới tối đa một lần mỗi giây nên không
được gọi mạng và không làm tăng chi phí vòng lặp 30 fps.

```js
const e = DeskWorld.environment;
const daylight = e.daylight;       // 0..1, bình minh đến ban ngày
const sun = e.sunElevation;        // âm khi dưới đường chân trời
const azimuth = e.sunAzimuth;      // hướng mặt trời, độ
const wind = e.wind, gust = e.gust; // gió nền và cơn gió nhẹ, 0..1
const windDir = e.windDirection;   // hướng gió, độ
const clouds = e.cloudiness;       // mây ổn định theo ngày, 0..1
const rain = e.rain, snow = e.snow; // lượng mưa / tuyết mô phỏng, 0..1
const storm = e.storm;              // giông gió, 0..1
```

Theme nên dùng các giá trị này để điều chỉnh màu, ánh sáng, tốc độ chuyển
động hoặc hướng hạt ngay trong `draw()`. Không nên dựng lại texture, bố cục
icon hay toàn bộ shader mỗi giây. Nếu theme có lựa chọn thời điểm thủ công,
giữ lựa chọn đó và chỉ dùng môi trường khi giá trị là “Tự động”. Môi trường
hiện là mô phỏng offline; thời tiết ngoài đời thật chỉ nên bổ sung sau bằng
tùy chọn rõ ràng và bộ nhớ đệm.

## Đăng ký tham số

Thêm theme vào mảng `themes` trong `prototype/themes.json`. Mỗi tham số số có
`min`, `max`, `step`, `default`; tham số lựa chọn dùng `options` và `default`
là chỉ số. Tên nhóm, nhãn, mô tả và lựa chọn phải có mục tương ứng trong
`i18n/vi.json` và `i18n/en.json`.

```json
{
  "id": "my-theme",
  "name": "Vườn đèn đom đóm",
  "desc": "Một khu vườn nhỏ quanh các biểu tượng.",
  "page": "my-theme.html",
  "status": "ready",
  "product": "free",
  "groups": [{
    "name": "Ánh sáng",
    "params": [{"key":"glow","min":0,"max":1,"step":0.05,"default":0.6,"format":"percent"}]
  }]
}
```

## Kiểm tra

```powershell
node tests\renderer-contract.cjs
powershell -ExecutionPolicy Bypass -File host\test.ps1
node prototype\serve.mjs
```

Mở `http://localhost:5178/settings.html`, chọn theme, đổi kích thước cửa sổ,
thử icon, pause và các nút chia sẻ. Sau khi đạt kiểm tra, chạy native acceptance
trên profile cách ly trước khi đưa theme vào bản phát hành.

Thumbnail trong `prototype/thumbs/` là tài sản thiết kế thủ công; không tự động
ghi đè thumbnail hiện có. Tác phẩm mới phải là tài sản nguyên bản hoặc có giấy
phép tương thích GPL-3.0 và ghi nguồn trong `THIRD_PARTY_NOTICES.md`.


