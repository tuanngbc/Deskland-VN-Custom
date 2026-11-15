# Deskland VN — Rust Native

Deskland VN là hình nền động native cho Windows, viết bằng Rust + Win32 + OpenGL. Đây là source chính của dự án; bản chạy nền không còn phụ thuộc vào host C# hay WebView2.

## Chạy bản đã build

Giữ `Deskland-VN.exe` và thư mục `prototype` cùng cấp rồi chạy EXE. Mở Cài đặt bằng `Deskland-VN.exe --settings`. Cấu hình nằm tại `%USERPROFILE%\\.deskland-rs\\settings.json`.

## Build từ source

GitHub Actions tự build trên Windows với Rust stable. Khi có Rust và Windows SDK cục bộ:

```powershell
cargo test --release
cargo build --release
.\\target\\release\\deskland-rs.exe
```

EXE phát hành được đổi tên thành `Deskland-VN.exe`; giữ `prototype/` bên cạnh để dùng giao diện cài đặt và thumbnail. `--headless --snapshot <thư_mục> --exit-after-snapshot` dùng để kiểm tra nhanh nhiều màn hình.

## Tính năng

- Một tiến trình native, mục tiêu 20–30 FPS và tự giảm tải khi bị che hoặc khóa máy.
- Đồng bộ, độc lập và panorama trên nhiều màn hình, kể cả tọa độ âm.
- 16 chủ đề, tiếng Việt/English, thời tiết theo vị trí và ánh sáng thay đổi theo giờ.
- Hỗ trợ screensaver và khóa màn hình Windows bằng cơ chế hệ điều hành.

Giấy phép và thông báo bản quyền nằm trong `LICENSE`, `NOTICE`, `THIRD_PARTY_NOTICES.md` và `TRADEMARKS.md`.
