# Tối ưu tốc độ và tài nguyên sau M5

Đợt này giữ kiến trúc module M0–M5 và schema 6. Không thêm dependency runtime, framework canvas, server hoặc model. Mục tiêu là giảm công việc thừa và giới hạn phần cache có thể tạo lại, đồng thời giữ chính xác nhãn và cơ chế phục hồi.

## Những thay đổi

- Canvas chỉ đổi backing size khi kích thước/DPR thực sự thay đổi; mọi invalidation trong cùng frame dùng một callback vẽ. Brush gộp vùng dirty, chỉ colorize/upload một lần mỗi frame; buffer clone từ mask immutable được phép dùng lại RGBA ngoài vùng dirty đã chứng minh không đổi.
- Giữ cache raster sau khi pointer capture kết thúc bình thường; giải phóng backing canvas không còn dùng khi đổi ảnh/ẩn layer/dispose. Thay đổi này được đo ở cả GPU process WebView2 và renderer, bên cạnh JS backing storage.
- Undo dùng các tile XOR 64×64 có thay đổi, đảo được theo cả hai chiều. Không giữ cả raster cho từng nét. Hint vùng brush gắn với buffer nguồn, được tiêu thụ ngay để tránh giữ chuỗi raster cũ. Import/replacement không có hint dùng so sánh đầy đủ. Raster serialization dùng WeakMap theo buffer immutable; sửa notes/vector không mã hóa lại mask không đổi.
- Rust xác thực độ phủ/giá trị RLE trước khi dùng bảng tra class ID 256 phần tử. Tài sản mask đã có chỉ cần xác minh checksum và được tái sử dụng; raster mới giải nén khi thực sự cần encode PNG. Revision, idempotency, transaction, operation log và journal strict được giữ nguyên.
- Main/review dùng chung decoded image pool, tối đa 2 entry và khoảng 32 MiB RGBA cho cache. Ảnh active lớn vẫn được mở; ảnh preload bị chặn nếu không vừa ngân sách. Chỉ preload một hàng xóm sau khi UI đã có thời gian vẽ; các lần save không đổi thông tin filter không làm refresh/preload lại thư viện.
- PNG tĩnh đúng orientation được hiển thị trực tiếp sau kiểm tra SHA nguồn. JPEG/WebP, ảnh xoay và APNG vẫn tạo preview PNG canonical tĩnh. Không giữ bản sao PNG đầy đủ khi không cần.
- Thumbnail thư viện dùng JPEG quality 78; nhãn và full preview giữ lossless. Cache có ngân sách thumbnail 32 MiB, image preview 128 MiB, video preview 128 MiB, trim theo đợt. Thumbnail PNG cũ được thay khi tạo JPEG tương ứng; cache cũ còn lại có thể dọn thủ công.
- Release chỉ tối ưu kích thước crate ứng dụng bằng `opt-level="s"`; dependency xử lý ảnh/SQLite vẫn giữ tối ưu mặc định. Không bật LTO toàn dependency hoặc tăng số job biên dịch gây tốn RAM/disk trên máy hiện tại.

## Sử dụng

Mở **Dataset tools → Storage & performance** để xem các nhóm dung lượng project. Đánh dấu **Rebuild thumbnails and previews when needed**, rồi **Clear derived cache** để thu hồi cache được nhận diện. Các ảnh đang hiển thị vẫn dùng được; thumbnail/preview được tạo lại khi cần. Nhãn, version, lịch sử, backup, ảnh nguồn và frame đã import không thuộc cache được xóa.

Chạy bản portable đã build hoặc `src-tauri/target/release/labelflow.exe` khi sử dụng bình thường. `npm.cmd run desktop:dev` dành cho phát triển và có thêm Vite, watcher, source map, toolchain. Không đóng gói `.tools`, `node_modules`, `.cache` hoặc `src-tauri/target/debug` cùng ứng dụng. Giữ Python/model/FFmpeg riêng nếu cần những tính năng tùy chọn đó.

## Kiểm tra và bằng chứng

Xem [đo trước/sau trên Windows](benchmarks/resource-efficiency-windows.md) và [ADR 0007](decisions/0007-resource-efficiency.md). Các bài mới kiểm tra undo byte-exact với tile mép/layer thêm-xóa/resize, 100 nét nhỏ 1080p, RLE reuse, decoded pool, canvas allocation, dirty upload, filter refresh, APNG, nguồn thay đổi, cache hỏng, quota và cleanup bảo toàn nhãn/lịch sử/backup.

Kết quả cuối, executable và các hạn chế đo được được ghi trong báo cáo benchmark; không khẳng định đã đạt tối ưu tuyệt đối hay mọi target của master plan.

Workload native cuối: private bytes sau 60 nét giảm khoảng **21,5%**, vẽ đến Saved p50 giảm **37,4%**, EXE giảm **15,6%**. Thumbnail của 20 ảnh thật giảm **84,1%**. Startup chưa có bằng chứng cải thiện; số đo ngắn không thay thế benchmark 4K, sử dụng nhiều giờ hoặc toàn bộ dataset.
