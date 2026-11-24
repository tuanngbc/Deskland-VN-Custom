# Nhật ký thay đổi TLauncher

## 1.0.2 — Unified Spotlight & Premium Desktop Experience (2026-11-24)

**Added:** Gộp Spotlight thu gọn và thư viện toàn màn hình, cùng catalog Start/Dock. Cấu hình riêng phím tắt và Dock; mặc định Compact/Fullscreen. F6 giữ truy vấn; hỗ trợ phân trang, EXE/LNK, ghim, yêu thích, ẩn/khôi phục, đổi tên/icon và thư mục kéo thả.

**Improved:** Native HostBackdrop/Acrylic trên hệ thống tương thích, nền snapshot hoặc đặc dự phòng; ô tìm kính, chữ trắng ngà và DirectWrite grayscale. Chuyển động Spotlight, Start, Dock, menu, lịch và Trung tâm điều khiển dùng chính sách chung 0,25×–2,00×, mặc định 0,70×.

**Fixed:** Lọc theo đích shortcut, xác minh icon đúng nguồn; bỏ nền chữ nhật đục ở ô tìm toàn màn hình, giữ nhập/chọn text native; chuẩn bị khung đầu khi mở popup, xử lý Escape/reduced motion và chuyển hướng animation. Giữ hồ sơ và khởi động qua Task Scheduler như 1.0.1.

**Performance:** Cache icon RAM/đĩa giới hạn, nạp trước có giới hạn và chỉ làm mới mục bị đổi. Tái sử dụng graphics factories, giải phóng surface khi đóng Spotlight; giới hạn preview Stage và tạm dừng dựng hình khi màn hình phiên tắt.

**Known limitations:** Windows Default là hiệu ứng cửa sổ public. Scale không đạt owner UAT, giữ implementation ngoài hồ sơ thường; Genie và Stage Peek chưa triển khai. Kính và IME tùy hệ thống; mục tiêu RAM/GPU/CPU toàn ứng dụng chưa xác nhận.

[Xem ghi chú phát hành](docs/releases/v1.0.2.vi.md) · [English](docs/releases/v1.0.2.en.md) · [简体中文](docs/releases/v1.0.2.zh-CN.md)


## 1.0.1 — Khởi động cùng Windows, Dock sẵn sàng sớm hơn

- Tùy chọn **Khởi động cùng Windows** dùng Task Scheduler cho tài khoản hiện tại, chạy ngay khi người dùng đăng nhập, không thêm thời gian chờ và không yêu cầu quyền quản trị. TLauncher chuyển mục tự khởi động cũ đã bật sang cơ chế mới khi đăng ký thành công. Mục đã bị tắt trong Windows không được tự bật lại.
- Dock và Stage Manager khởi tạo trước bộ dựng hình nền. Nếu Explorer chưa sẵn sàng, TLauncher sẽ thử gắn lại hình nền sau; phần chờ này không còn chặn bước khởi tạo Dock. Các thành phần vẫn tuân theo cấu hình đã lưu.
- Bản Windows x64 có số phiên bản trong Cài đặt, thuộc tính EXE và `VERSION.txt`, cùng `BUILD-COMMIT.txt` và tệp SHA-256 để đối chiếu.

### Nâng cấp và quay lại

1. Thoát TLauncher cũ từ Dock hoặc khay hệ thống. Sao lưu thư mục cài đặt và `%USERPROFILE%\.deskland-rs`. Giải nén toàn bộ gói Windows x64; giữ `prototype/` và `assets/widget/` cạnh `TLauncher.exe`.
2. Mở `TLauncher.exe` một lần. Trong **Cài đặt → Hệ thống**, kiểm tra **Khởi động cùng Windows**. Nếu đổi thư mục cài đặt, hoặc lần đăng nhập kế tiếp ứng dụng chưa tự mở, tắt rồi bật lại công tắc từ bản mới để đăng ký đúng đường dẫn. Tác vụ có trong Task Scheduler và có thể không xuất hiện ở trang **Ứng dụng khởi động** của Windows.
3. Để quay về 1.0.0, tắt **Khởi động cùng Windows** trong 1.0.1 trước khi thoát để gỡ tác vụ mới. Khôi phục gói cũ, chạy `TLauncher.exe` và bật lại tùy chọn trong bản cũ nếu muốn. Giữ nguyên thư mục cấu hình trừ khi cần khôi phục bản sao lưu.

### Kiểm chứng

Thay đổi startup đã được chạy trên Windows 11 hai màn hình. Trong một lượt gọi trực tiếp từ Task Scheduler, Dock/Stage khởi tạo sau 132 ms và hình nền sẵn sàng sau 659 ms, tính từ lúc bắt đầu khởi tạo ứng dụng. Một lần khởi động lại sau đó xác nhận Task Scheduler tự mở TLauncher; Dock/Stage khởi tạo trước khi hình nền sẵn sàng. Đây là kết quả trên một máy, không phải thời gian tính từ lúc bấm nút nguồn hay cam kết trên máy khác.

## 1.0.0 — Bản phát hành đầu tiên

Hình nền động và chế độ đa màn hình, Dock, Stage Manager, Start, Spotlight, lịch và trung tâm điều khiển, hai đồng hồ desktop, phím tắt chụp/quay và Cài đặt tích hợp. Xem [ghi chú và ảnh minh họa của 1.0.0](https://github.com/tuanngbc/TLauncher/releases/tag/v1.0.0).
