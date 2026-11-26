# TLauncher VN development notes

- Phase 1 native migration uses the complete C++ FluidWin foundation in `native/fluidwin/`; old Rust source remains preserved for owner-gated later extensions. Do not run two desktop cores.
- `prototype/` chứa giao diện cài đặt, thumbnail và shader dùng chung; phải được đóng gói cạnh `TLauncher.exe`.
- Người đóng góp có thể build trên máy riêng theo README-rust.md. Trên máy owner, build và kiểm thử chỉ chạy qua GitHub Actions.
- GitHub Actions ở `.github/workflows/build-rust-test.yml` là workflow phát hành duy nhất.
- Không đưa `target/`, EXE, log, profile cá nhân, credential hoặc dữ liệu ngoài repository vào git.
- Giữ nguyên `LICENSE`, `NOTICE`, `THIRD_PARTY_NOTICES.md`, `TRADEMARKS.md` và `third_party/deskland-vn/`.
- Ứng dụng đọc/ghi `%USERPROFILE%\\.deskland-rs`; không xóa cấu hình người dùng khi dọn build cũ.
- Mục tiêu nền là 20–30 FPS, tạm dừng khi bị che/khóa và tránh tạo tiến trình WebView2 thường trực.

## Chính sách build và kiểm thử bắt buộc (OWNER)

TLauncher development follows a GitHub Actions-only build and verification policy. The owner's machine is production-only. Local source editing and Git operations are permitted, but local compilation, dependency installation, executable testing, packaging, and development runtime execution are prohibited. All builds and automated verification must run on GitHub-hosted runners. Never use the owner's machine as a build fallback or self-hosted runner. Preserve the owner's production installation and configuration.

After a successful candidate CI run, automatically retrieve and verify the exact artifact (workflow SHA, archive checksum and package version). Prepare a separate candidate and isolated UAT profile under `%LOCALAPPDATA%\TLauncher-UAT`, and update one temporary Desktop TEST shortcut. Candidate execution requires the owner's explicit interaction with that shortcut. The UAT launcher must safely request production to exit, wait for the singleton to be free, start the isolated candidate, and restore the previously running production when testing ends. Never force-kill, overwrite production, modify production settings/startup, or run a candidate automatically. Do not change a running candidate. Clean redundant inactive UAT packages and temporary development worktrees when verified safe; preserve owner evidence and any running or unaccepted session.

## Triển khai production chính thức

Chỉ cập nhật khi owner đã chốt phiên bản và cấp quyền. Tải ZIP từ GitHub Releases, xác minh checksum, tag, commit và version, sao lưu installation và hồ sơ, rồi nâng cấp toàn bộ gói với đường quay lui. Có thể cập nhật tác vụ đăng nhập và khởi chạy bản chính thức nếu owner cho phép cụ thể. Chỉ dọn các bản TEST đã dừng sau khi kiểm tra bản chính thức. Quyền triển khai một phiên bản không áp dụng cho phiên bản sau.

Mục tiêu dài hạn: RAM 100–150 MiB, GPU khoảng 3%, CPU khoảng 1% trong sử dụng thông thường. Đo theo workload và phần cứng; giải phóng tài nguyên khi không dùng, không chạy render loop thừa và không dùng memory trimming để làm đẹp số liệu.
