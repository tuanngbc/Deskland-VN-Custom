# Stage và Start sau phản hồi về a9e9859

Ở 96 DPI và kích thước mặc định, Stage dùng dải 252 px và card trước khoảng 186 × 103 px. Các lớp chồng lệch 12 px và không đổi bề rộng card khi thêm cửa sổ. Icon 31 px đè vào góc dưới khoảng 23 px, với phần còn lại nhô qua mép. Trái/phải đối xứng. Màn hẹp giới hạn bề ngang Stage ở 28% màn hình. DWM tiếp tục giữ tỷ lệ của cửa sổ nguồn và không vẽ đè lên icon.

| Phần | Trạng thái |
| --- | --- |
| Bố cục Stage ngang, góc icon, chỉ báo +N | Có trong ứng dụng. Kiểm tra trái/phải, DPI và không gian bấm từng lớp trong tests. |
| Phối cảnh Stage | Bài thử vẽ chữ và nội dung native GDI do chính fixture tạo vào tứ giác, qua phép chiếu đồng nhất và lấy mẫu song tuyến tính. Cạnh ngoài cao hơn, phía trong còn khoảng 75%; bên phải đảo hướng mà chữ vẫn đọc thuận. Bài thử gửi hình vào HWND thật rồi xuất đúng bộ đệm đã gửi. Nó không đọc desktop, không ghi cửa sổ người dùng, và không chạy trong sản phẩm. |
| Ảnh thực của app bên thứ ba theo phối cảnh | **PENDING.** Bản dùng thử vẫn dùng DWM phẳng. API DWM công khai chỉ nhận hình chữ nhật; phép thử bọc HWND xuyên tiến trình ở a9e9859 nhận lỗi tham số. |
| Start | Dùng bảng màu Dock, lớp vẽ bo góc AA, tìm kiếm có glyph, account phụ dưới tiêu đề, nút Cài đặt/Nguồn có biểu tượng và gợi ý. Giữ edit native và tất cả đường mở app hiện có. |
| Nghiệm thu | **PENDING.** Ảnh native CI là ảnh fixture, chưa chứng minh Stage phối cảnh với app thật, chất lượng hình trên desktop hai màn, hay CPU/GPU khi dùng hằng ngày. |

Nguồn ảnh có thể thử tiếp là [Windows.Graphics.Capture cho một HWND](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createforwindow). Hướng thận trọng là một frame theo sự kiện, với nguồn được chọn rõ ràng, kiểm tra capture affinity, không ghi ảnh ra đĩa, giới hạn bộ nhớ trước khi đưa vào phép chiếu và dọn tài nguyên khi đổi nhóm/ẩn Stage. Không dùng PrintWindow hay chụp desktop để thay thế nội dung được bảo vệ. [DWM thumbnail](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ns-dwmapi-dwm_thumbnail_properties) vẫn là đường tương thích an toàn.

Khi thử, mở 1/2/3/5/8 cửa sổ mẫu cùng app rồi đổi cạnh, chuyển sang màn dọc, kiểm tra đúng cửa sổ khi bấm lớp sau và thành viên thứ 6–8 trong menu. Kiểm tra Start sáng/tối, app ghim, tìm kiếm, context menu, nút nguồn chỉ mở menu, và thoát về production. Chưa đặt OWNER_VISUAL_ACCEPTED.
