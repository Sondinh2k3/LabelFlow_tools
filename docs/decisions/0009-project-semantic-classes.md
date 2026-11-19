# ADR 0009 — Bộ lớp semantic theo project và tô đa giác semantic

Ngày: 2026-11-11 · Trạng thái: chấp nhận

## Bối cảnh

- Lớp semantic chỉ có 19 train ID Cityscapes cố định. Bài toán khác (ví dụ 8 lớp lòng đường Việt Nam: Lòng đường, Bó vỉa, Vỉa hè/lề, Phương tiện, Người, Động vật, Kết cấu cố định, Nền) không có chỗ đặt lớp và ID xuất bị lệch.
- Lớp semantic chỉ vẽ được bằng brush, chậm khi cần ranh giới thẳng hoặc vùng lớn.

## Quyết định

- Lớp semantic là dữ liệu của project, như lớp object (ADR 0008). Namespace `cityscapes`, layer `cityscapes_semantic` và class `cityscapes-ignore` (255) giữ nguyên tên để tương thích project, đường dẫn export và snapshot cũ; giao diện hiển thị layer là **Semantic**.
- Tạo project: mặc định seed 19 lớp Cityscapes như trước. Tùy chọn `semanticClasses` (mỗi dòng một tên, tùy chọn `#rrggbb`) thay bộ lớp, ID = thứ tự dòng (0..n-1, tối đa 255). Danh sách được kiểm tra trước khi tạo thư mục `.labelflow`.
- Seed chỉ chạy một lần mỗi namespace: lớp đã xóa không bị tạo lại khi mở project. Lớp Ignore luôn tồn tại và không sửa/xóa được.
- Thêm lớp lấy ID trống nhỏ nhất; đổi tên/màu không đổi ID. Xóa chỉ khi không mặt nạ semantic hiện tại nào chứa ID đó: quét các asset PNG được tham chiếu, dừng ở lần trùng đầu tiên. Chi phí chỉ trả khi xóa, không ảnh hưởng thao tác gán nhãn.
- Kiểm tra lưu/export tiếp tục dùng bộ lớp hiện tại của project (giá trị pixel phải thuộc bộ lớp hoặc 255). Thứ tự lớp của project mặc định giữ nguyên nên snapshot content ID cũ không đổi.
- Metrics semantic tính trên ID 0..254 và chỉ liệt kê lớp có mặt trong GT hoặc prediction; tổng hợp theo `class_id`, không theo vị trí.
- Adapter `pidnet_semantic` yêu cầu `classMap` bằng đúng danh sách lớp semantic của project theo thứ tự ID (không còn cố định 19 kênh).
- Tô đa giác trên lớp semantic: đóng polygon sẽ rasterize (quy tắc chẵn-lẻ, tâm pixel, quét theo hàng) vào mặt nạ hiện có bằng lớp đang chọn. Đây là một lệnh undo, dùng chung đường lưu/journal và cập nhật overlay theo vùng bao như brush. Không lưu vector cho semantic (vẫn là raster loại trừ lẫn nhau). Overlay tô lại toàn bộ một lần khi profile đổi (ví dụ đổi màu lớp).

## Hệ quả

- Không cần migration; bảng `classes` có từ schema 2.
- Các project của nhiều người gán nhãn phải dùng cùng danh sách để ID khớp; ứng dụng nhớ danh sách gần nhất trên máy để giảm sai sót.
- Chưa có nhập tiền nhãn hàng loạt, click-to-segment (SAM) hay mặt nạ Ignore dùng chung theo camera; đó là các bước tiếp theo.
