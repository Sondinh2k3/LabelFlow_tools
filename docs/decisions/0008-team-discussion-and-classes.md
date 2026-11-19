# ADR 0008 — Thảo luận nhóm qua thư mục chia sẻ và class tùy chỉnh

Ngày: 2026-11-10 · Trạng thái: chấp nhận

## Bối cảnh

- Profile chỉ có class cố định; profile Generic có đúng một class `object`, nên không dùng được cho dataset mới.
- Nhiều người gán nhãn cần hỏi ý kiến nhau về các ca khó. Ứng dụng phải giữ offline, không server, không tài khoản; project SQLite vẫn single-writer.

## Quyết định

### Class đối tượng do project quản lý

- Class của lớp instance (namespace `candidate`: bbox/polygon/mask) đọc từ bảng `classes` của project. Cityscapes semantic (train ID 0–18, ignore 255) và layers vẫn cố định.
- Thêm class cấp export ID trống nhỏ nhất; đổi tên/màu không đổi ID. Xóa chỉ khi không nhãn nào đã lưu dùng class đó (kiểm tra bảng `annotations`, tài liệu nhãn và FK).
- Không cần migration: bảng `classes` có từ schema 2. Mọi chỗ trước đây gọi `profile::definition` giờ gọi `profile::current(session)`.
- Snapshot đóng băng profile của nó. Kiểm tra snapshot vẫn yêu cầu layers khớp chính xác (layer id dùng trong đường dẫn export) nhưng cho phép class thay đổi sau đó.

### Thảo luận nhóm qua thư mục chia sẻ

- Mỗi người chọn một thư mục chung (ổ mạng LAN, OneDrive, Google Drive, Dropbox, Syncthing). Đường dẫn lưu theo từng máy/project trong `team.json` ở thư mục cấu hình người dùng, cùng user ID ngẫu nhiên và tên hiển thị.
- Bố cục:

  ```
  <team>/labelflow-team.json          marker
  <team>/entries/<user-id>/<id>.json   comment hoặc status, bất biến, chỉ tác giả ghi
  <team>/previews/<sha256>.jpg         preview ≤1600 px, ghi một lần theo nội dung
  ```

- Không có hai người ghi cùng một file nên công cụ đồng bộ không tạo conflict. Trạng thái thread = status entry mới nhất (open/resolved).
- Thread tham chiếu ảnh bằng SHA-256 nội dung, nên khớp giữa các project/máy khác nhau dù đường dẫn khác. Comment đầu có thể kèm pin (tọa độ ảnh canonical) và nhãn vector (≤300 hình, ≤64 điểm/polygon).
- File từ người khác được coi là không tin cậy: giới hạn kích thước, kiểm tra UUID/SHA/độ dài, thư mục tác giả phải khớp `author.id`, file `.tmp`/lỗi bị bỏ qua có cảnh báo. Preview chỉ nhận JPEG ≤6 MiB, trả về dạng data URL.
- IO thư mục chia sẻ chạy ngoài khóa project nên ổ mạng chậm không chặn autosave. Frontend kiểm tra 20 giây/lần khi cửa sổ đang hiển thị; cache chỉ parse file mới/đổi.

## Hệ quả

- Không đồng bộ nhãn giữa nhiều người; mỗi người làm trên project riêng (hoặc chia tập ảnh), sau đó gộp qua export/import COCO/YOLO. Thảo luận là kênh phối hợp.
- Thời gian hiển thị phụ thuộc công cụ đồng bộ; đồng hồ máy lệch có thể đảo thứ tự comment gần nhau.
- Không có xóa/sửa comment (append-only); có thể mở lại thread đã resolved.
