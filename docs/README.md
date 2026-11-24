# TLauncher: hướng dẫn mã nguồn

Mã nguồn phát hành nằm trong src, prototype và assets. Thư mục third_party/deskland-vn chứa mã và tài nguyên có nguồn gốc từ bản Deskland-VN; các tệp giấy phép ở thư mục gốc áp dụng cho việc phân phối.

| Chủ đề | Tài liệu |
| --- | --- |
| Chạy và phát triển bản Rust | [Hướng dẫn bản Rust](../README-rust.md), [Tạo chủ đề](THEME_AUTHORING.md) |
| Dock và bố cục cửa sổ | [Kiến trúc](DOCK_ARCHITECTURE.md), [Chế độ taskbar](DOCK_SHELL_MODES.md) |
| Stage Manager | [Ảnh phối cảnh](STAGE_PERSPECTIVE_RENDERER.md), [Vòng đời bộ đệm](STAGE_PREVIEW_CACHE_LIFECYCLE.md), [Chiều cao thẻ](STAGE_CARD_HEIGHT_SETTINGS.md) |
| Start và khay hệ thống | [Tích hợp Start](START_MENU_INTEGRATION.md), [Phạm vi tương thích của khay](REAL_TRAY_COMPATIBILITY_MATRIX.md) |
| Kiểm thử | [Trạng thái và bằng chứng Dock](DOCK_STATUS.md), [Báo cáo hiệu năng](DOCK_PERFORMANCE_REPORT.md), [Quy trình nghiệm thu Windows](OWNER_DAILY_USE_UAT.md) |

Các báo cáo theo đợt kiểm thử ghi nhận trạng thái tại commit được nêu trong báo cáo. Dùng kết quả CI của chính commit phát hành và kiểm tra trên máy thực để đánh giá bản mới. Ảnh fixture trong CI chỉ kiểm tra đúng trường hợp đã ghi, không thay thế việc dùng thử cả hai màn hình.


## Quy ước README đa ngôn ngữ

- [README.md](../README.md) tiếng Việt là bản nội dung chuẩn và là trang mặc định trên GitHub.
- Khi thay đổi tính năng đã phát hành, giới hạn, số liệu, phiên bản hoặc hướng dẫn cài đặt, kiểm tra và đồng bộ [README.en.md](../README.en.md) và [README.zh-CN.md](../README.zh-CN.md) trong cùng lần cập nhật. Giữ ngữ cảnh kiểm chứng và ghi nhận tác giả ở cả ba bản.
- Ba bản dùng chung ảnh tại `docs/images/` và logo tại `assets/branding/`. Dịch alt text, chú thích, mục lục, bảng và FAQ; không nhân bản ảnh. Giữ vùng `product-guide:start/end` quanh phần giới thiệu chi tiết.
- Kiểm tra liên kết giữa các bản, các mục lục và đường dẫn tài nguyên trên GitHub trước khi đưa lên nhánh chính.
