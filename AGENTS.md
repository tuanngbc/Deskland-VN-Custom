# tuanpc-desk VN development notes

- Source chính là Rust native ở thư mục gốc (`src/`, `Cargo.toml`, `assets/`).
- `prototype/` chứa giao diện cài đặt, thumbnail và shader dùng chung; phải được đóng gói cạnh `tuanpc-desk.exe`.
- Build kiểm tra: `cargo test --release`; build phát hành: `cargo build --release` trên Windows với Rust stable và Windows SDK.
- GitHub Actions ở `.github/workflows/build-rust-test.yml` là workflow phát hành duy nhất.
- Không đưa `target/`, EXE, log, profile cá nhân, credential hoặc dữ liệu ngoài repository vào git.
- Giữ nguyên `LICENSE`, `NOTICE`, `THIRD_PARTY_NOTICES.md`, `TRADEMARKS.md` và `third_party/deskland-vn/`.
- Ứng dụng đọc/ghi `%USERPROFILE%\\.deskland-rs`; không xóa cấu hình người dùng khi dọn build cũ.
- Mục tiêu nền là 20–30 FPS, tạm dừng khi bị che/khóa và tránh tạo tiến trình WebView2 thường trực.
