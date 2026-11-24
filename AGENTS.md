# TLauncher VN development notes

- Source chính là Rust native ở thư mục gốc (`src/`, `Cargo.toml`, `assets/`).
- `prototype/` chứa giao diện cài đặt, thumbnail và shader dùng chung; phải được đóng gói cạnh `TLauncher.exe`.
- Build kiểm tra: `cargo test --release`; build phát hành: `cargo build --release` trên Windows với Rust stable và Windows SDK.
- GitHub Actions ở `.github/workflows/build-rust-test.yml` là workflow phát hành duy nhất.
- Không đưa `target/`, EXE, log, profile cá nhân, credential hoặc dữ liệu ngoài repository vào git.
- Giữ nguyên `LICENSE`, `NOTICE`, `THIRD_PARTY_NOTICES.md`, `TRADEMARKS.md` và `third_party/deskland-vn/`.
- Ứng dụng đọc/ghi `%USERPROFILE%\\.deskland-rs`; không xóa cấu hình người dùng khi dọn build cũ.
- Mục tiêu nền là 20–30 FPS, tạm dừng khi bị che/khóa và tránh tạo tiến trình WebView2 thường trực.

## Chính sách build và kiểm thử bắt buộc (OWNER)

TLauncher development follows a GitHub Actions-only build and verification policy. The owner's machine is production-only. Local source editing and Git operations are permitted, but local compilation, dependency installation, executable testing, packaging, and development runtime execution are prohibited. All builds and automated verification must run on GitHub-hosted runners. Never use the owner's machine as a build fallback or self-hosted runner. Preserve the owner's production installation and configuration.

After a successful candidate CI run, automatically retrieve and verify the exact artifact (workflow SHA, archive checksum and package version). Prepare a separate candidate and isolated UAT profile under `%LOCALAPPDATA%\TLauncher-UAT`, and update one temporary Desktop TEST shortcut. Candidate execution requires the owner's explicit interaction with that shortcut. The UAT launcher must safely request production to exit, wait for the singleton to be free, start the isolated candidate, and restore the previously running production when testing ends. Never force-kill, overwrite production, modify production settings/startup, or run a candidate automatically. Do not change a running candidate. Clean redundant inactive UAT packages and temporary development worktrees when verified safe; preserve owner evidence and any running or unaccepted session.
