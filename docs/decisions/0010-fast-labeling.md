# ADR 0010 — Gán nhãn nhanh: SAM click-ra-mặt-nạ, tiền nhãn hàng loạt, mặt nạ Ignore, QC khi xuất

Ngày: 2026-11-11 · Trạng thái: chấp nhận

## Bối cảnh

Quy trình thực tế (khoảng 1.500–2.000 ảnh, 2–3 người gán nhãn, nhiều vòng học chủ động) cần: dùng nhãn sơ bộ của mô hình lớn cho cả thư mục, cắt vật nhanh bằng SAM, loại vùng cố định (mui xe) và kiểm tra dataset trước khi train. Ứng dụng phải giữ offline, nhẹ và không có dependency runtime mới.

## Quyết định

### SAM click-ra-mặt-nạ

- Tiến trình Python **riêng** (`state.sam`), dùng chung supervisor `Runtime`/`Worker` và giao thức stdio JSON-lines với worker gợi ý; cả hai model có thể cùng chạy. Cấu hình ở `sam.json` trong thư mục cấu hình người dùng (ngoài project), tự dùng lại Python của AI nếu có.
- `worker/sam.py`: manifest `kind: sam_prompt` (encoder + decoder ONNX cạnh manifest, SHA-256 bắt buộc, kiểm tra trên đúng bytes đã nạp). Encoder HWC 0–255 (samexporter, tiền xử lý trong graph) hoặc NCHW chuẩn hóa; nhận dạng theo số chiều input; kích thước đầu vào đọc từ graph nếu cố định. Decoder theo chữ ký chính thức của segment-anything; khi có nhiều mask chọn theo IoU dự đoán. Chạy thử khi nạp.
- Embedding cache theo SHA-256 ảnh (LRU 4); click thêm điểm dùng lại low-res mask trước đó để hội tụ như demo SAM. Editor tự gửi yêu cầu encode khi chọn công cụ SAM.
- Frontend gửi tuần tự, chỉ giữ prompt mới nhất (click nhanh khi SAM bận gộp thành một yêu cầu); kết quả cũ bị bỏ theo số thứ tự và media ID. Rust kiểm tra prompt nằm trong ảnh và mask trả về (giá trị 0/1, tổng run = W×H) trước khi đưa cho editor.
- Mask SAM chỉ là preview. Enter ghi vào layer đang chọn như một lệnh undo thông thường: semantic = ID lớp (Shift+Enter chỉ ghi vào pixel 255), region = 1, object = instance mask kèm class. Không lưu gì riêng cho SAM.

### Tiền nhãn hàng loạt và mặt nạ Ignore

- `import/masks.rs`: quét thư mục PNG (đệ quy, từ chối symlink), khớp theo media ID hoặc tên file duy nhất, kiểm tra kích thước, thu tập giá trị mà **không giữ pixel trong bộ nhớ**; bước áp dụng giải mã lại từng file. Mỗi giá trị nguồn cần đúng một ánh xạ; giao diện điền sẵn ánh xạ đồng nhất cho ID có trong project.
- Hai chế độ: `fillUnlabeled` (chỉ pixel 255/0 đổi, giữ nhãn người) và `replace`. Ảnh approved luôn bị bỏ qua, kể cả khi được duyệt sau bước kiểm tra. Mỗi ảnh lưu qua đường lưu chuẩn (revision, operation log, asset), thành công/thất bại riêng, có progress/cancel.
- Mặt nạ Ignore: PNG khác 0 = Ignore, áp cho mọi ảnh chưa duyệt cùng kích thước (tùy chọn theo trip); ảnh chưa có mặt nạ semantic đã là 255 nên không bị ghi.

### QC khi xuất và thẻ

- Export semantic/all tính histogram mỗi ảnh một lần (cũng thay HashSet kiểm tra ID bằng bảng 256 phần tử), ghi `semantic-stats.json` (pixel, tỷ lệ, số ảnh theo lớp; ảnh trên 10% Ignore; ảnh không có mặt nạ) và thêm cảnh báo tóm tắt vào báo cáo.
- Thêm thẻ chụp: `weather:overcast`, `road:wet`, `road:construction`, `road:no_curb`, `road:unpaved`, `object:on_road`.

## Hệ quả

- Không thêm dependency npm/Cargo; Python/ONNX Runtime/model vẫn tùy chọn. Trọng số SAM do người dùng tự xuất và chịu trách nhiệm giấy phép.
- SAM 2 (chữ ký khác) chưa hỗ trợ. Tốc độ encoder phụ thuộc model và CPU; chưa có số đo trên model thật trong repo (fixture chỉ kiểm tra giao thức và tọa độ).
- Chưa có đồng bộ nhãn giữa nhiều người; gộp qua export như trước.
