# Prototype khay có ứng dụng hợp tác

[Shell_NotifyIcon](https://learn.microsoft.com/en-us/windows/win32/shell/notification-area) dùng để app đăng ký, cập nhật và xóa icon của chính nó. API không liệt kê/cung cấp callback của mọi icon; UI Automation trong popup Explorer cũng không cung cấp luồng cập nhật ổn định cho tray độc lập.

POC chỉ chạy với marker hồ sơ thử và cờ --trial-tray-poc. Parent khởi chạy EXE cùng revision ở vai trò peer. Pipe stdio kế thừa, frame cố định version 1, hàng đợi một phần tử, timeout và cleanup đúng child. Peer sở hữu HWND và tự xử lý callback của nó. Kiểm tra thêm/cập nhật tooltip và icon, callback trái/double, dựng menu Win32 của peer, xóa và khởi chạy lại.

CI thử thêm/cập nhật/xóa icon trên runner Windows; registeredWithExplorer báo kết quả thật. Chạy local mặc định không thêm icon vào khay Windows. Số item menu chỉ chứng minh handler dựng menu, chưa chứng minh popup mở hay người dùng chọn mục. Lệnh --trial-tray-poc-demo mở một bảng thử riêng với icon mẫu, thay đổi trạng thái, click và menu chuột phải do peer sở hữu. Đây là bản demo tự nguyện, không mở trong phiên Deskland thường ngày. Chức năng demo cần thử tương tác riêng.

THIRD_PARTY_TRAY_PENDING / TASKBAR_REPLACEMENT_PENDING: chưa có UI khay app bên thứ ba trong Dock, chưa chuyển click/menu tới app không hợp tác, chưa kiểm tra tooltip khi rê chuột, thứ tự, overflow hay taskbar ẩn trên hai màn. Nếu cần hỗ trợ app bất kỳ, đánh giá thêm trong VM với yêu cầu OWNER_APPROVAL_FOR_SHELL_HOOK_TEST. Không thay đổi cấu hình Shell trong bản này.
