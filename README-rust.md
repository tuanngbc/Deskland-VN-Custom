# tuanpc-desk — Phiên bản Rust Native (Win32 + OpenGL 3.3)

Phiên bản viết lại hoàn toàn bằng **Rust native (Win32 + OpenGL 3.3 qua WGL)** của tuanpc-desk, thay thế kiến trúc đa tiến trình C# + WebView2 thường trực để tối ưu triệt để RAM/CPU, bổ sung **tương tác chuột trực tiếp trên màn hình nền**, nâng tổng số chủ đề lên **16 chủ đề** và chuyển sang cơ chế **màn hình khóa thật 100% của Windows**.

---

## Điểm nổi bật của bản Rust Native

1. **Tiến trình nền gọn, tiện ích chạy khi cần:**
   - Không nuôi trình duyệt ngầm khi chạy hình nền: một tiến trình native. RAM và tải GPU thực tế phụ thuộc chủ đề, độ phân giải, số màn hình và driver.
   - Một ngữ cảnh OpenGL 3.3 dùng chung cho mọi màn hình; biên dịch shader & khởi tạo khung hình đầu tiên chỉ mất **~350–480 ms**.
2. **Đầy đủ 16 chủ đề thích ứng biểu tượng + Tương tác Click chuột riêng biệt:**
   - Tự động đọc vị trí biểu tượng màn hình nền (`SysListView32`), dựng trường khoảng cách (SDF) trên CPU và uốn lượn bờ biển, đồng cỏ, rạn cát, tán rừng, hàng rào sân vận động... ôm quanh các nhóm icon theo thời gian thực.
   - Mỗi chủ đề đều có phản hồi vật lý / hoạt ảnh riêng khi người dùng click chuột trái vào vùng trống trên màn hình nền.
3. **Bắt chuột thông minh & Adaptive FPS (`30 -> 50 FPS`):**
   - Sử dụng `WM_INPUT` (`RIDEV_INPUTSINK`) không chiếm quyền chuột của Windows, chỉ kích hoạt khi click gọn vào đúng vùng trống Desktop (`Progman` / `WorkerW` / `SysListView32`) và **tự động bỏ qua khi bấm vào biểu tượng (icon)** hoặc cửa sổ ứng dụng khác.
   - Bình thường giữ nhịp **30 FPS**; khi có tương tác hoặc hiệu ứng động, tăng tạm thời lên **50 FPS** rồi trở về **30 FPS** khi hiệu ứng lắng xuống.
4. **Cửa sổ Cài đặt Kép (WebView2 On-Demand + Win32 Native Fallback):**
   - Mở chính giao diện `prototype/settings.html` giàu trực quan qua WebView2 **chỉ khi cửa sổ Cài đặt đang mở**; đóng cửa sổ là giải phóng toàn bộ tiến trình `msedgewebview2.exe`.
   - Nếu máy không có WebView2 Runtime hoặc chạy độc lập một file `.exe` không kèm thư mục `prototype/`, ứng dụng tự động chuyển sang **cửa sổ Cài đặt Win32 thuần** (`src/settings.rs`) có đầy đủ chọn chủ đề, thanh trượt tham số và tùy chọn khóa màn hình.
5. **Bảo mật chuẩn hệ điều hành — Chỉ dùng khóa thật của Windows (`src/lock.rs`):**
   - Không hook bàn phím mức thấp, không chặn `Win+L`, không lưu/kiểm mã PIN tự chế.
   - Hỗ trợ khóa ngay (`LockWorkStation()`), trình bảo vệ màn hình cảnh động bảo mật (`tuanpc-desk.scr` với `ScreenSaverIsSecure=1`), phím tắt `Ctrl+Alt+L` và tự động đồng bộ cảnh hiện tại làm ảnh nền màn khóa Windows.
   - Tự động phát hiện và cho phép khôi phục lại `Win+L` nếu bản C# cũ từng ghi `DisableLockWorkstation=1`.
6. **Dừng dựng hình khi khuất hoặc khóa máy:**
   - Tự động tạm ngừng vẽ khi có cửa sổ phóng to / toàn màn hình che khuất màn hình nền, khi phiên Windows đang khóa (`WTSRegisterSessionNotification`), ở màn hình `Winlogon` / UAC, hoặc khi trình bảo vệ màn hình đang chạy.
7. **Biểu tượng ứng dụng nhúng sẵn & Menu Khay hệ thống tinh gọn:**
   - Nhúng trực tiếp icon đa độ phân giải (`16×16` đến `256×256`) và `VERSIONINFO` vào `tuanpc-desk.exe` qua `app.rc` + `build.rs` (hiển thị đồng bộ trên Explorer, Taskbar, Khay hệ thống và thanh tiêu đề Cài đặt).
   - Menu chuột phải ở khay hệ thống gọn gàng: chỉ hiển thị chủ đề đang chọn (`✓ Chủ đề: <Tên chủ đề>`) cùng các tác vụ nhanh, việc đổi chủ đề và tinh chỉnh chuyên sâu tập trung tại cửa sổ **Cài đặt…**.

---

## Xuất hình trực tiếp trên GPU

- Mặc định thử chia sẻ texture OpenGL với D3D11 qua `WGL_NV_DX_interop`, rồi đưa lên desktop bằng DXGI / DirectComposition. Khung hình thông thường không đi qua RAM, GDI hay bộ mã hóa video. Cảnh, thời tiết, ánh sáng và tương tác vẫn được dựng trực tiếp ở 30 FPS.
- Mỗi màn hình có một surface theo độ phân giải cảnh, Windows phóng lên kích thước vật lý. Màn bị che không nhận khung mới; khi mọi màn bị che hoặc phiên bị khóa thì dừng dựng hình. Surface cuối được giữ để tiếp tục hiển thị khi mở lại.
- Nếu driver không hỗ trợ hoặc thiết bị lỗi, tự chuyển sang GDI và ghi lý do vào log. Không cài thêm engine hay thay cài đặt đồ họa toàn hệ thống.
- `--present gpu` yêu cầu thử đường mới; `--present gdi` dùng đường cũ để đối chiếu. `--headless --test-gpu-output --test-output-resume --run-seconds 12` kiểm tra pixel, hướng ảnh, tạm dừng và tiếp tục trên GPU thật. Bài kiểm tra GPU báo lỗi nếu phải dùng GDI dự phòng. Kiểm tra này không thay thế việc đo tải khi hai desktop thật đang hiện.
- Cơ chế cache video thử nghiệm đã được gỡ vì không cải thiện tải trên máy kiểm thử. Không tự tạo thêm video cache.

## Chụp màn hình và quay desktop

- **Ctrl+Shift+S:** kéo chọn vùng chụp; Space chọn cả màn hình đang trỏ chuột; Esc hủy. Sau khi chọn, trình sửa ảnh mở ngay: cắt, mũi tên, khung, bút, chữ, số bước, pixel và tô kín. Enter copy rồi đóng; Ctrl+C copy, Ctrl+S lưu PNG/JPG, Ctrl+Z/Y hoàn tác/làm lại. Có nút ghim trên cùng.
- **Ctrl+Shift+R:** chọn vùng quay desktop, lưu MP4 H.264, 30 FPS mặc định (tùy chọn 60 FPS), thu tiếng máy hoặc không tiếng. Nhấn lại phím quay để dừng và lưu; Ctrl+Shift+P tạm dừng/tiếp tục. Bảng điều khiển cũng có các nút tương ứng. Không có webcam, mic hay chế độ game.
- Đổi phím tắt và tùy chọn tại **Cài đặt → Tiện ích**. Nếu ứng dụng khác đang giữ phím, Cài đặt báo xung đột để chọn phím khác.
- Ảnh lưu mặc định ở Pictures/tuanpc-desk, video ở Videos/tuanpc-desk trong hồ sơ người dùng. Không tự tải ảnh/video lên mạng.
- Hình nền giữ một tiến trình; mỗi tiện ích tạo một phiên của cùng EXE khi được gọi và thoát khi đóng. Bộ chụp, bộ mã hóa và âm thanh không được tạo lúc chạy nền. Quay dùng GDI và Media Foundation có trên Windows; tải CPU thực tế phụ thuộc vùng quay, FPS và bộ mã hóa của máy. Video có bảo vệ bản quyền có thể không được chụp.

## Danh sách 16 Chủ đề & Cơ chế Tương tác Chuột

| # | Mã (`--theme`) | Bí danh (Alias) | Tên hiển thị | Mô tả cảnh & Tương tác khi Click vùng trống Desktop |
|---:|---|---|---|---|
| 1 | `meadow` | — | **Thảo nguyên** | Đồng cỏ xanh và mây trắng uốn quanh icon, bóng mây trôi dưới nắng. **Click:** Tạo cụm mây puff bồng bềnh bay lên hòa nhập vào tầng mây trời. |
| 2 | `beach` | — | **Bãi biển** | Bờ cát vàng, sóng ngọc lam vỗ bờ quanh đảo icon, thuyền kayak và hải âu. **Click:** Tạo vòng sóng lan tỏa trên nước, gọi thuyền kayak hướng mũi tới và hải âu nghiêng cánh. |
| 3 | `beach-turquoise` | `maldives` | **Đảo Maldives** | Đảo cát trắng giữa đầm phá san hô ngọc bích, đàn cá nhiệt đới và rùa biển. **Click:** Thả mồi thu hút đàn cá & rùa biển bơi tới, tạo gợn nước và gọi hải âu. |
| 4 | `lake` | — | **Hồ nước** | Mặt hồ tĩnh lặng, lá sen/hoa súng ôm bờ icon và đàn cá Koi bơi lội. **Click:** Mini-game câu cá thư giãn — click mặt hồ quăng phao câu, chờ cá đớp mồi giật phao rồi click lần nữa để kéo cá nhảy lên mặt nước. |
| 5 | `sky` | — | **Bầu trời** | Biển mây bồng bềnh đa tầng chuyển sắc Ngày / Hoàng hôn / Đêm (hoặc tự động theo giờ thực). **Click:** Ban ngày & hoàng hôn tạo cụm mây mới và gọi đàn chim bay qua; ban đêm phóng **sao băng** xẹt sáng bầu trời. |
| 6 | `ocean` | — | **Đại dương** | Đảo mây 3D nổi giữa đại dương xanh thẫm nhìn từ trên cao, thuyền buồm & du thuyền rẽ sóng. **Click:** Tạo vòng sóng định vị (waypoint) dẫn hướng cho thuyền buồm & du thuyền tiến về điểm chọn. |
| 7 | `rainforest` | — | **Rừng mưa nhiệt đới** | Tán rừng mưa nhiều tầng bao quanh trảng cỏ rêu icon, mưa rơi và sương mù. **Click:** Tạo gợn nước mưa, thu hút đàn đom đóm phát sáng tụ về và thúc chim nhiệt đới vỗ cánh. |
| 8 | `mocchau` | `plum-blossoms`, `plum` | **Hoa mận** | Rừng hoa mận trắng Mộc Châu ôm đồi cỏ xuân trong sương sớm. **Click:** Rung cành làm bung chùm cánh hoa mận trắng bay lả tả theo gió và gọi chim rừng bay lượn. |
| 9 | `tet-sakura` | `tet_sakura` | **Hoa anh đào Tết** | Sân xuân giấy dó ấm áp bao quanh bởi rừng đào hồng thắm, đèn lồng đỏ và bao lì xì. **Click:** Làm đung đưa đèn lồng / lì xì gần đó, tung cánh hoa đào xoay trong gió và gọi chim én mùa xuân. |
| 10 | `cat-hill` | `cat_hill` | **Đồi mèo nắng ấm** | Đồi cỏ nắng ấm với những chú mèo mập đi dạo, ngồi liếm lông và nằm ngủ thở đều. **Click:** Bấm trúng mèo để **vuốt ve** (mèo nảy lên vui vẻ); bấm lên thảm cỏ để thả **bướm vàng** thu hút mèo chạy theo vờn. |
| 11 | `stadium` | `football`, `soccer`, `pitch` | **Sân bóng sôi động** *(Mới)* | Sân vận động bóng đá nhìn từ trên cao (Ban ngày / Hoàng hôn / Đèn pha ban đêm), hai đội Đỏ – Xanh chạy chỗ chuyền bóng quanh chướng ngại icon. **Click:** Bấm trên sân để chuyền/lăn bóng tới vị trí đó cho cầu thủ bứt tốc đuổi theo; bấm vào khung thành để sút ghi bàn kèm hiệu ứng pháo sáng ăn mừng. |
| 12 | `aquarium` | `fish`, `fishing`, `goldfish`, `fishbowl` | **Bể cá vàng** *(Mới)* | Góc nhìn ngang qua bể kính ngập nắng với rạn cát vàng ươm uốn ôm biểu tượng, rong rêu đung đưa, bọt khí và đàn cá vàng tung tăng. **Click:** Bấm vùng nước để thả dây câu cá vàng (canh lúc phao giật mạnh hiện dấu `!` rồi click để kéo cá lên); bấm vào rạn cát để tung làn bụi cát lấp lánh. |
| 13 | `commute` | `bus`, `evening-bus`, `commute-bus` | **Chuyến xe chiều** *(Mới)* | Phong cảnh hoạt hình ven sông theo giờ thực, cầu vòm, phố lên đèn và xe buýt vàng chạy qua đồng lau. **Click:** Bấm mặt đường gọi xe dừng đón, bấm lên trời gọi chim / sao băng, bấm đồng cỏ tạo luồng gió thổi rạp lau. |
| 14 | `summer-porch` | `porch`, `corgi`, `corgi-porch`, `veranda` | **Hiên nhà ngày hạ** *(Mới)* | Hiên gỗ mái ngói âm dương kiểu Ghibli, đàn Corgi ngủ trưa lăn mình trước quạt bàn cổ điển, đĩa dưa hấu đỏ, sách mở và luống vườn xanh ôm quanh icon. **Click:** Bấm Corgi để gãi bụng bắn tim hồng, bấm quạt bàn đổi nấc gió, gõ chuông gió đồng, cắn dưa hấu hoặc thả bướm / đom đóm trên sân. |
| 15 | `cats` | `kittens`, `hillside-kittens` | **Mèo con trên đồi** *(Mới)* | Đồi cỏ trời xanh mây trắng, cây lá đung đưa, lá bay, bướm vàng và đàn mèo mập chân ngắn (10 bộ lông) đi dạo, liếm lông, ngủ cuộn tròn / nằm nghiêng / nằm ngửa thở phập phồng. Vẽ bằng **tiny-skia trên CPU** (`src/themes/cats/`, mỗi file `.rs` ứng với một file `prototype/cats-*.js`) thay vì shader, để giống hệt bản Canvas 2D. **Click:** Bấm mèo để vuốt ve (nhảy cẫng, chùm tim, mắt cười); bấm cây / lùm cây để rung rụng lá; bấm cỏ thả **cuộn len** cho 3 chú mèo gần nhất chạy tới vờn, khều lăn đi rồi đuổi theo; bấm trời gọi bướm. |
| 16 | `boxing-ring` | `boxing`, `ring`, `arcade-boxing`, `punch` | **Võ đài quyền anh** *(Mới)* | Phòng tập & võ đài quyền anh Arcade nâng cao góc nhìn 3/4, hai võ sĩ đối kháng kết hợp hệ lò xo giảm chấn Squash & Stretch, phóng găng đa vùng, khói tròn puff, sao choáng `@ @`, thanh máu HUD và thảm tập EVA ôm gọn icon. **Click:** Bấm nửa phải/trái đài để tung đòn búa đỉnh đầu, móc ngang má, đấm thẳng cằm hoặc đấm sườn cong chữ C; bấm bao cát treo, lốp xe & búa tạ hoặc đồng hồ hiệp đấu để tương tác đạo cụ. |

> **Bật / tắt tương tác chuột:** Có thể bật hoặc tắt bất kỳ lúc nào ngay tại menu chuột phải ở khay hệ thống (**Tương tác chuột trên màn hình nền**) hoặc trong cửa sổ **Cài đặt**.

---

## Biên dịch & Chạy

Yêu cầu: **Rust toolchain (`rustup`, MSVC target `x86_64-pc-windows-msvc`)** và **Windows 10/11 SDK** (có sẵn khi cài Visual Studio Build Tools C++). Không cần kết nối mạng nếu cache Cargo đã sẵn sàng; trình biên dịch tự tìm `rc.exe` của Windows SDK để nhúng icon và manifest.

**Máy không cài được MSVC Build Tools** (cần quyền quản trị): build bằng bộ công cụ GNU của Rust. `build.rs` nhúng
`app.manifest` bằng crate `embed-manifest` và biên dịch `app.rc` (icon, VERSIONINFO) bằng `windres`; cần MinGW trong
`PATH` (ví dụ `winget install BrechtSanders.WinLibs.POSIX.UCRT --scope user`), rồi chép `WebView2Loader.dll`
(trong `target/release/build/webview2-com-sys-*/out/x64/`) ra cạnh exe để mở được cửa sổ Cài đặt. Cảnh báo
`.rsrc merge failure: multiple non-default manifests` là vô hại: manifest của dự án vẫn là tài nguyên số 1.

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
cargo +stable-x86_64-pc-windows-gnu build --release
```

```powershell
cd tuanpc-desk
cargo build --release

# Chạy hình nền (mặc định đọc chủ đề lần trước hoặc beach, 20 FPS cơ sở)
.\target\release\tuanpc-desk.exe

# Hoặc chỉ định chủ đề / tốc độ khung hình ngay từ dòng lệnh
.\target\release\tuanpc-desk.exe --theme stadium --fps 24

# Mở cửa sổ Cài đặt của tiến trình đang chạy (hoặc khởi động kèm Cài đặt)
.\target\release\tuanpc-desk.exe --settings

# Mở thẳng cửa sổ Cài đặt Win32 thuần (bỏ qua WebView2)
.\target\release\tuanpc-desk.exe --settings-win32

# Thoát tiến trình đang chạy nền
.\target\release\tuanpc-desk.exe --quit
```

### Đầy đủ tham số dòng lệnh (CLI)

| Tham số | Chức năng |
|---|---|
| `--theme <id>` | Khởi chạy với chủ đề chỉ định (hỗ trợ cả mã chính thức lẫn alias trong bảng trên). |
| `--mode mirror\|independent\|panorama` | Chọn chế độ nhiều màn hình ngay khi khởi chạy; chế độ này cũng có thể đổi trong Cài đặt. |
| `--fps <số>` | Đặt tốc độ khung hình cơ sở khi nghỉ (mặc định `20`; tự nâng lên `50` khi đang tương tác). |
| `--settings` | Mở cửa sổ Cài đặt WebView2 (tự lùi về Win32 nếu thiếu WebView2/prototype). |
| `--settings-win32` | Buộc mở giao diện Cài đặt Win32 cổ điển siêu nhẹ. |
| `--saver` (hoặc `/s`) | Chạy thử ngay chế độ trình bảo vệ màn hình toàn màn hình. |
| `--quit` | Gửi tín hiệu đóng an toàn tới tiến trình `tuanpc-desk.exe` đang chạy và dọn icon khay. |
| `--snapshot <thư_mục>` | Xuất ảnh chụp khung hình GPU (`.bmp`) ra thư mục chỉ định sau 4 giây. |
| `--exit-after-snapshot` | Tự động thoát ngay sau khi chụp xong `--snapshot` (dùng cho kiểm thử tự động). |
| `--test-click` | Tự động mô phỏng thao tác click trên cảnh để kiểm thử hiệu ứng tương tác. |
| `--headless` | Dựng cảnh ẩn ngoài màn hình (không gắn vào `Progman`/`WorkerW`, không tạo icon khay). |
| `--present gpu\|gdi\|gl` | Mặc định thử GPU trực tiếp và dự phòng bằng GDI. `gdi` ép đường cũ; `gl` dùng SwapBuffers trên máy có driver tương thích. |
| `--webview-debug` | Mở cổng Remote Debugging `9222` cho cửa sổ cài đặt WebView2. |

- **Tệp cấu hình:** Lưu độc lập tại `~/.deskland-rs/settings.json` (lần chạy đầu tiên nếu chưa có sẽ tự kế thừa cấu hình từ `%LOCALAPPDATA%\DesklandPreview\settings.json` của bản C# mà không ghi đè dữ liệu cũ).
- **Nhật ký hoạt động:** Ghi tại `tuanpc-desk.log` cạnh file `.exe` (hoặc `~/.deskland-rs/tuanpc-desk-saver.log` ở chế độ `.scr`), tự in thống kê RAM và tốc độ khung hình mỗi 10 giây.

---

## Kiến trúc Hiển thị & Tương thích Windows 11 24H2+

- **Tự động nhận diện mô hình Desktop:** Hỗ trợ cả kiến trúc **Raised Desktop (`Progman` + cửa sổ con phân lớp `WS_EX_LAYERED`)** trên Windows 11 24H2+ lẫn kiến trúc **`WorkerW` cổ điển** trên Windows 10 / 11 23H2, hỗ trợ đa màn hình khác DPI (`Per-Monitor DPI Aware V2`) và tự dựng lại khi cắm/rút màn hình (`WM_DISPLAYCHANGE`).
- **Đường ống kết xuất (`--present gl` mặc định):**
  1. OpenGL 3.3 vẽ shader vào FBO ở độ phân giải tối ưu **72%** (giống bản gốc để giảm tải fill-rate GPU tích hợp).
  2. `glBlitFramebuffer(..., GL_LINEAR)` phóng mượt trên GPU lên kích thước thật của từng màn hình.
  3. Đọc bất đồng bộ qua **PBO (Pixel Buffer Object)** không gây nghẽn pipeline GPU → đẩy thẳng lên cửa sổ hình nền bằng `SetDIBitsToDevice`.
  4. Giải quyết triệt để lỗi màn hình đen của `SwapBuffers` trên cửa sổ con `WS_EX_LAYERED` của Windows 11 24H2+.

---

## Hiệu năng Đo Thực Tế

1. **So sánh trực tiếp với bản C# + WebView2** *(Intel UHD 770, 2 màn hình `1920×1080`)*:

| Phiên bản / Cấu hình | Số tiến trình | Private RAM | Working Set | GPU Shared | CPU (1 nhân) |
|---|---:|---:|---:|---:|---:|
| Bản C# + WebView2 (`beach`) | 11 | ~620 MB | ~1.210 MB | ~250 MB | ~15% |
| Bản C# + WebView2 (`meadow`) | 11 | ~650 MB | ~1.240 MB | ~266 MB | ~20% |
| **Bản Rust (`tuanpc-desk`, `--present gdi` 2 màn Full HD)** | **1** | **~97,7 MB** | **~124,2 MB** | **~97,9 MB** | **~14%** |

2. **Đo thực tế trên 1 màn hình Full HD với chủ đề phức tạp (`stadium` — 12 cầu thủ AI + khán đài + bóng)** *(chạy nền hàng ngày)*:
   - **Private RAM:** `~64,9 MB` (`Working Set ~95,4 MB`).
   - **CPU trung bình khi nghỉ (20 FPS):** `~0,7%` tổng toàn hệ thống (`~1,22 ms` thời gian CPU mỗi khung hình).
   - **Khi khóa máy hoặc mở ứng dụng toàn màn hình:** **`0,0% CPU`**, **`0 FPS`**.

---

## Khác biệt chủ đích so với bản C# / Web

- Không port các lớp ảnh PNG/Canvas phụ của 2 chủ đề đầu (`beach-palms.js`, `meadow-boy.js`, `meadow-cat.js`) và tính năng xuất GIF/PNG chia sẻ mạng xã hội (`share.js`); giao diện Cài đặt tự động ẩn các tùy chọn này qua cờ `caps` để đồng nhất với cảnh thực tế trên Desktop.
- Không vẽ đè đồng hồ Glassmorphism giả lập lên màn hình khóa và không chặn `Win+L`, đảm bảo an toàn bảo mật tuyệt đối thông qua màn hình đăng nhập thật của Windows (`Winlogon` / Windows Hello / PIN hệ thống).
