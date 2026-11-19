# Hướng dẫn thao tác LabelFlow — gán nhãn dị vật lòng đường

| Mục         | Nội dung                                                                                                                               |
| ----------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Áp dụng cho | `Huong_dan_loc_va_gan_nhan_du_lieu_v3.md` bản 3.1 (11 lớp + Bỏ qua, đa giác lòng đường)                                                |
| Ngày        | 18/11/2026 (thay `Huong_dan_gan_nhan_bang_LabelFlow.md` bản 16/11: bỏ lớp `bo_qua`, chỗ chưa tô = Bỏ qua, script chuyển sang `tools/`) |
| Người dùng  | Trưởng nhóm, người gán nhãn, người duyệt, kỹ sư ML                                                                                     |
| Script      | `tools/` (xem `tools/README.md`). Trong gói phát hành: `vnroad\tools\`                                                                 |
| Kiểm chứng  | `bao_cao_kiem_tra_labelflow.md` (dự án `test_vong_1`, 18/11/2026)                                                                      |
| Lưu ý       | Giao diện LabelFlow là **tiếng Anh**; tên nút trong tài liệu viết **in đậm**, đúng như trên màn hình                                   |

Tài liệu này nói **bấm gì, ở đâu, theo thứ tự nào**. Quy tắc gán (cái gì là Khác, mép đặt ở đâu…) theo Hướng dẫn v3; số mục dạng "(v3 §7.3)" là mục trong tài liệu đó.

---

## 0. Ba điều đã kiểm chứng, phải nhớ

| #   | Hành vi của LabelFlow                                                                                                                                                                 | Hệ quả khi gán                                                                                                                                                                          |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | **Pixel chưa tô xuất ra 255 = Bỏ qua** (không phải Khác). Cục tẩy (`E`) cũng trả pixel về 255                                                                                         | Khe hở nhỏ quanh vật không gây hại. Nhưng **vật quên tô cũng thành Bỏ qua**: mô hình không học được nó. Trong lòng đường không được để trống chỗ nào; trời, cây, nhà cũng phải tô (thô) |
| 2   | **Tô sau ghi đè tô trước** (cọ, đa giác tô, SAM _Apply mask_). Chỉ SAM _Apply to unlabeled only_ (Shift+Enter) là không đè                                                            | Tô **Mặt đường phủ rộng trước**, tô vật **sau**. Muốn đổi lớp một vùng thì tô đè bằng lớp đúng, không cần tẩy                                                                           |
| 3   | **Ignore template áp cho mọi ảnh chưa duyệt có cùng kích thước** trong project (không chọn được theo camera); bỏ qua ảnh chưa có mặt nạ semantic; tô sau có thể đè lên vùng đã Bỏ qua | **Mỗi camera một project.** Áp template **sau** khi nạp nhãn sơ bộ, và áp lại một lần trước khi chuyển duyệt                                                                            |

**LabelFlow không làm** (dùng công cụ khác): kiểm kê video, chia tập (`videos.csv`); trích khung + cắt phần trời (FFmpeg, mục 1.5); lọc mờ/trùng, chọn ảnh theo đa dạng (script riêng); nhãn đoạn `test_segments.csv` (bảng tính); làm mờ mặt, biển số (trước khi đưa ảnh vào); huấn luyện.

---

## 1. Chuẩn bị một lần

### 1.1. Cài LabelFlow trên mỗi máy gán nhãn

1. Giải nén `LabelFlow-windows-x64.zip`, chạy `LabelFlow.exe`. Máy cần WebView2 (Windows 10/11 thường có sẵn).
2. Project phải nằm trên **ổ cứng của máy**, không đặt trên ổ mạng. Ảnh nguồn nên chép về máy.

### 1.2. Cài SAM (cắt vật bằng vài click), mỗi máy một lần

PowerShell tại thư mục chứa `LabelFlow.exe`:

```powershell
python -m venv ai-venv
ai-venv\Scripts\python.exe -m pip install -r python-requirements.txt

New-Item -ItemType Directory -Force E:\models\mobile_sam
curl.exe -L -o E:\models\mobile_sam\mobile_sam.encoder.onnx https://huggingface.co/nrl-ai/samexporter-onnx-models/resolve/main/mobile_sam/mobile_sam.encoder.onnx
curl.exe -L -o E:\models\mobile_sam\mobile_sam.decoder.onnx https://huggingface.co/nrl-ai/samexporter-onnx-models/resolve/main/mobile_sam/mobile_sam.decoder.onnx
ai-venv\Scripts\python.exe make-sam-manifest.py E:\models\mobile_sam --name "MobileSAM" --license "Apache-2.0"
```

Trong LabelFlow: panel phải → **Click-to-mask (SAM)** → **Choose SAM model…** → chọn `ai-venv\Scripts\python.exe`, rồi `E:\models\mobile_sam\sam.json` → **Start SAM**, chờ **Ready**. Trên CPU: nạp mô hình ~5 s, mỗi ảnh mới ~1–1,5 s, mỗi click ~0,1 s.

### 1.3. Cài Python cho `tools/` (máy kỹ sư ML)

Tại thư mục chứa `tools\`:

```powershell
python -m venv .venv
.venv\Scripts\python.exe -m pip install -r tools\requirements.txt
.venv\Scripts\activate
```

Các lệnh `python tools\...` trong tài liệu chạy trong môi trường này.

### 1.4. Cấu trúc thư mục làm việc

Theo v3 §11:

```
E:\vnroad\
  manifest\        videos.csv, clips.csv, frames.csv, test_events.csv, test_segments.csv
  ignore\          mặt nạ Bỏ qua cố định theo camera (PNG, trắng = bỏ qua)
  batches\         ảnh giao cho người gán, mỗi video một thư mục con (mục 1.6)
  prelabel\        nhãn sơ bộ, PNG trùng tên ảnh
  labelflow_export\  thư mục xuất của LabelFlow, mỗi người gán một
  data\            do tools\convert_labelflow.py sinh: images\ masks\ road\ edge\ list\ ...
```

`videos.csv` tối thiểu có `video_id` và `tap` (train/val/test); thêm `mat_na_bo_qua` nếu camera có mặt nạ cố định:

```csv
video_id,ngay_quay,buoi,tuyen,tap,ty_le_cat_tren,mat_na_bo_qua
VID2_0611,2026-11-06,sang,Nguyen Trai,train,0.30,cam_A.png
VID3_0611,2026-11-06,sang,Thanh Xuan,val,0.30,cam_A.png
VID4_0611,2026-11-06,chieu,Tuyen moi,test,0.30,
```

Cột `mat_na_bo_qua` trống thì script tìm `ignore\<video_id>.png`; không có file thì bỏ qua.

### 1.5. Trích khung, cắt phần trời, đặt tên

**Tên ảnh bắt buộc `<video_id>_<giây>.png`**, ví dụ `VID4_0611_00232.0.png`. Script lấy `video_id` là phần trước dấu `_` cuối cùng.

Ví dụ trích 2 khung/giây, cắt 30% phía trên (`ty_le_cat_tren = 0,30`), rồi đổi số thứ tự khung thành giây (khung n ứng với giây (n−1)/2):

```powershell
New-Item -ItemType Directory -Force E:\vnroad\frames_raw\VID2_0611
ffmpeg -i D:\video_goc\VID2_0611.mp4 -vf "fps=2,crop=iw:ih*0.70:0:ih*0.30" E:\vnroad\frames_raw\VID2_0611\%06d.png
python -c "import sys,pathlib; d=pathlib.Path(sys.argv[1]); v=sys.argv[2]; fps=float(sys.argv[3]); [p.rename(d/f'{v}_{(int(p.stem)-1)/fps:07.1f}.png') for p in sorted(d.glob('[0-9]*.png'))]" E:\vnroad\frames_raw\VID2_0611 VID2_0611 2
```

Tỷ lệ cắt phải **giống hệt AI box** (v3 §4). **Trình trích khung của LabelFlow không cắt ảnh**, không dùng cho ảnh train/val. Sau đó lọc mờ/sáng/trùng và chọn ảnh bằng script riêng (v3 §4, §5).

### 1.6. Chia ảnh cho người gán

- Vòng 1 khoảng 250 ảnh, tối đa 15 ảnh mỗi video gốc, chỉ ban ngày (v3 §5.1). Mỗi lớp được miễn có mặt trong **ít nhất 30 ảnh**; có **ít nhất 30 ảnh đường thoáng** (cách đánh dấu chưa chốt, v3 §13).
- **Mỗi người một project, mỗi camera một project** (LabelFlow không cho hai người cùng ghi một project; Ignore template áp theo kích thước ảnh). Mỗi ảnh giao một người, **trừ** 5% gán đôi và 20 ảnh hiệu chuẩn (mục 8).
- **Mỗi video một thư mục con**: LabelFlow lấy thư mục chứa ảnh làm nhóm chia tập.

  ```
  E:\vnroad\batches\batch_02_<tên>\
    VID2_0611\VID2_0611_00011.0.png ...
    VID5_0612\VID5_0612_00040.5.png ...
  ```

- Video tập **test** không đưa vào đây (v3 §3.3). Tập val gán như train.

---

## 2. Tạo project

### 2.1. Tạo project và bộ lớp

1. Home → **Project profile: Road Encroachment**.
2. Tên: `vong1_<camera>_<người gán>`.
3. **Semantic classes → Custom list**, dán **đúng 11 dòng** (thứ tự = ID; app tự thêm `255 · Ignore / uncertain` làm Bỏ qua):

   ```
   mat_duong #804080
   via_he_le #f423e8
   nha_tuong #464646
   ket_cau_co_dinh #ffc000
   xe_may_xe_dap #0000e6
   o_to #00008e
   xe_tai_xe_buyt #000046
   xe_tho_so #5a5aff
   nguoi #dc143c
   dong_vat #ff8c00
   khac #00ff00
   ```

   | ID  | Tên                         | Lớp v3            | Trên box                     |
   | --- | --------------------------- | ----------------- | ---------------------------- |
   | 0   | mat_duong                   | Mặt đường         | Được miễn                    |
   | 1   | via_he_le                   | Vỉa hè / lề       | Không miễn                   |
   | 2   | nha_tuong                   | Nhà / tường       | Không miễn                   |
   | 3   | ket_cau_co_dinh             | Kết cấu cố định   | Được miễn                    |
   | 4   | xe_may_xe_dap               | Xe máy, xe đạp    | Được miễn                    |
   | 5   | o_to                        | Ô tô              | Được miễn                    |
   | 6   | xe_tai_xe_buyt              | Xe tải, xe buýt   | Được miễn                    |
   | 7   | xe_tho_so                   | Xe thô sơ, ba gác | Được miễn                    |
   | 8   | nguoi                       | Người             | Được miễn                    |
   | 9   | dong_vat                    | Động vật          | Được miễn                    |
   | 10  | khac                        | Khác              | Trong lòng đường là nghi vấn |
   | 255 | Ignore / uncertain (có sẵn) | Bỏ qua            | Không tính loss              |

   Tên phải **đúng chính tả như trên**: `convert_labelflow.py` dừng nếu sai tên, sai thứ tự hoặc có lớp thừa. **Không** thêm dòng `bo_qua`. Màu tùy ý nhưng cả nhóm dùng chung (trùng `tools/lop.yaml` để ảnh chồng nhãn cùng màu).

4. **Create project** → chọn thư mục trống trên ổ cứng.

Sau khi bắt đầu gán, **không xóa hay đổi thứ tự lớp** trong **Edit classes** (ID = vị trí). Đổi màu thì an toàn.

**Project tạo theo hướng dẫn cũ** (có `11 · bo_qua`): Editor → **Edit classes** → _Semantic classes_ → dòng `11 bo_qua` → **Delete**. App chỉ cho xóa khi chưa ảnh nào dùng lớp này; nếu đã dùng thì để nguyên, `convert_labelflow.py` đổi 11 → 255.

### 2.2. Nhập ảnh

**Import folder** → chọn `batch_02_<tên>` (để mặc định _linked_; bật **Copy into project** nếu muốn project mang theo ảnh khi chép sang máy khác). Mở một ảnh, xem ô **Dataset split group** ở panel phải: phải là đường dẫn kết thúc bằng tên video. Ảnh đã nhập sai nhóm: gõ tên video (ví dụ `VID2_0611`) vào ô đó. Tập thật vẫn do `videos.csv` quyết định khi chạy script.

### 2.3. Ignore template (taplo, kính, mui xe của camera)

1. Lấy một khung của camera, vẽ mặt nạ **trắng = Bỏ qua, đen = còn lại**, đúng kích thước ảnh (ví dụ 1920×1080), lưu PNG vào `E:\vnroad\ignore\cam_A.png`.
2. Sau khi nạp nhãn sơ bộ (mục 3): thanh công cụ editor → **Ignore template** → chọn PNG → xác nhận số ảnh.
3. Trước khi chuyển duyệt một lô: bấm **Ignore template** lần nữa (chỉ đổi những pixel bị tô đè lên vùng đó; ảnh đã duyệt không bị đụng).
4. Ghi tên PNG vào cột `mat_na_bo_qua` của `videos.csv` cho mọi video của camera (hoặc chép thành `ignore\<video_id>.png`). `convert_labelflow.py --ignore-dir` dùng file này để đặt 255 cho **cả Road Extent**: Ignore template trong app chỉ đặt 255 cho semantic.

### 2.4. Không dùng

Layer **Traffic exclusion**, **Suspected region**, **Candidate objects** (có sẵn trong profile, không xóa được): không vẽ gì lên đó. Ngoại lệ: Candidate objects dùng cho nhãn sự kiện video test (mục 10).

---

## 3. Nhãn sơ bộ

### 3.1. Vòng 1: Mask2Former Mapillary Vistas (máy có GPU)

```
python tools\prelabel_mask2former.py --tu-kiem                   (kiểm tra bảng ánh xạ, không cần mô hình)
python tools\prelabel_mask2former.py <ảnh> <ra> --xem-anh-xa     (đối chiếu v3 Phụ lục B, một lần)
python tools\prelabel_mask2former.py <ảnh> <ra> --fp16
```

Cài thêm `torch transformers`. Kết quả: mỗi ảnh một PNG **trùng tên ảnh**, giá trị 0–10 và 255 (Ego Vehicle, Car Mount, Unlabeled). Lớp Vistas không có trong bảng → Khác. Script **chưa chạy với mô hình thật**: chạy thử 5 ảnh, mở xem trước khi chạy cả lô. Mô hình không có đảo giao thông, dải phân cách, nón; xe ba gác, xe đẩy dễ lẫn ô tô; Barrier gồm cả khối bê tông tạm: luôn sửa tay (v3 §8.3, Phụ lục B).

Nạp vào LabelFlow:

1. Chọn layer **Semantic** ở panel phải.
2. Thanh công cụ → **Import mask folder** → chọn thư mục `<ra>`.
3. Bảng ánh xạ: để nguyên (0→0 … 10→10, 255→255).
4. **Merge with existing labels: chọn "Replace the layer"** (mặc định của app là _Fill only unlabeled pixels_). Ảnh đã duyệt không bị đè.
5. **Import masks**, đọc báo cáo: số ảnh khớp, số file bị bỏ qua (thường do khác tên hoặc kích thước). Ảnh chuyển sang `in_progress`.
6. Áp Ignore template (mục 2.3).

Road Extent vòng 1 vẽ tay.

### 3.2. Vòng 2, 3: PIDNet-S của vòng trước

Chạy PIDNet-S, lưu hai thư mục PNG trùng tên ảnh: `pred_masks\` (0–10, đầu ra P) và `pred_road\` (0/1, đầu ra R, ngưỡng 0,5). Nạp lần lượt:

1. Layer **Semantic** → **Import mask folder** → `pred_masks\` → ánh xạ 0–10 → **Replace the layer** (ảnh mới) hoặc **Fill only unlabeled pixels** (ảnh người đã sửa một phần).
2. Layer **Road Extent** → **Import mask folder** → `pred_road\` → ánh xạ `1 → region` (0 là nền) → **Import masks**.

Road Extent nạp vào là **mặt nạ tô**, không có đỉnh: sửa bằng cọ `M` / tẩy `E` trên layer Road Extent, hoặc xóa và vẽ đa giác mới. Vẫn phải là **một vùng liền**.

**AI assistance** trong app không chạy được PIDNet-S hai đầu ra; dùng cách nạp hàng loạt ở trên.

---

## 4. Màn hình và phím tắt

| Khu vực    | Có gì                                                                                                                                                                                                                         |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Panel trái | Danh sách ảnh, tìm kiếm, **Review filter**                                                                                                                                                                                    |
| Giữa       | Ảnh và lớp phủ nhãn                                                                                                                                                                                                           |
| Panel phải | **LAYERS** (chọn layer, bật/tắt, **Overlay opacity**), **CLASSES** (dòng dạng `10 · khac`), **Click-to-mask (SAM)**, **ANNOTATIONS**, **Review state**, **Notes**, **Ask the team about this image**, **Dataset split group** |

**Chọn lớp** (khi con trỏ không nằm trong ô nhập chữ):

| Lớp             | Phím | Lớp            | Phím                                |
| --------------- | ---- | -------------- | ----------------------------------- |
| mat_duong       | `1`  | xe_tai_xe_buyt | `7`                                 |
| via_he_le       | `2`  | xe_tho_so      | `8`                                 |
| nha_tuong       | `3`  | nguoi          | `9`                                 |
| ket_cau_co_dinh | `4`  | dong_vat       | `Alt+1`                             |
| xe_may_xe_dap   | `5`  | **khac**       | **`Alt+2`**                         |
| o_to            | `6`  | Bỏ qua         | bấm dòng `255 · Ignore / uncertain` |

| Phím                         | Tác dụng                                                                                                                                                      |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `S`                          | SAM: kéo khung quanh vật; click trái thêm, chuột phải hoặc Alt+click bớt; **Enter** = ghi đè (_Apply mask_); Shift+Enter = chỉ ghi lên chỗ chưa tô; Esc = hủy |
| `P`                          | Đa giác: click từng đỉnh; Enter hoặc click lại đỉnh đầu để đóng; Backspace xóa đỉnh vừa đặt. Trên layer Semantic đa giác **tô đè mọi thứ bên trong**          |
| `V`                          | Chọn; kéo đỉnh đa giác Road Extent                                                                                                                            |
| `M` / `E`                    | Cọ / tẩy (tẩy = về 255); `[` `]` thu nhỏ / phóng to cọ                                                                                                        |
| `L`                          | Ẩn/hiện toàn bộ lớp phủ                                                                                                                                       |
| Space + kéo, `H`, chuột giữa | Di chuyển ảnh                                                                                                                                                 |
| Lăn chuột / `0` / `=`        | Zoom tại con trỏ / vừa màn hình / 100%                                                                                                                        |
| `Ctrl+Z` / `Ctrl+Shift+Z`    | Hoàn tác / làm lại                                                                                                                                            |
| `Ctrl+Enter`                 | **Done → next**: ảnh sang `needs_review`, mở ảnh tiếp                                                                                                         |
| `Ctrl+Shift+Enter`           | **Approve → next** (người duyệt)                                                                                                                              |
| `?`                          | Bảng phím tắt đầy đủ                                                                                                                                          |

---

## 5. Gán nhãn từng ảnh

Đặt **Overlay opacity** khoảng 45%. Nguyên tắc nền (v3 §1.4): **trong lòng đường, không chắc là mặt đường thì gán `khac`**; nền ven đường (rãnh, mương, bậc, lề đất, cỏ, đất đắp) là `via_he_le`, không gán `khac`; không chắc loại xe thì chọn loại gần nhất.

| #   | Việc                                            | Layer · công cụ                  | Cách làm                                                                                                                                                                                                                                                        |
| --- | ----------------------------------------------- | -------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | **Tô Mặt đường rộng**                           | Semantic · `P` hoặc `M`, lớp `1` | Phủ cả mặt đường, **cả dưới xe và vật**, lấn nhẹ sang vỉa hè. Vạch sơn, nắp ga, gờ giảm tốc, vết vá, ổ gà, vũng nước, bóng đổ, bùn mỏng, que/giấy/cành nhỏ nằm dẹt **đều là `mat_duong`**. Đa giác tô ghi đè mọi thứ bên trong, nên làm bước này **đầu tiên**   |
| 2   | **Cắt dị vật, xe, người bằng SAM, đè lên trên** | Semantic · `S`                   | Chọn lớp → **kéo khung** quanh vật → click trái thêm phần thiếu, click phải bỏ phần thừa → **Enter**. Không dùng Shift+Enter cho vật. Chi tiết ở 5.1                                                                                                            |
| 3   | **Vẽ Road Extent**                              | Road Extent · `P`                | Bám **biên Mặt đường** ở đoạn nhìn thấy; đoạn bị xe, vật che: **nối thẳng** qua. Chi tiết ở 5.2                                                                                                                                                                 |
| 4   | **Sửa mép**                                     | Semantic · `M`, lớp `2`          | Lề, rãnh, song chắn rác, bậc, dốc trước nhà, thảm nằm phẳng = Vỉa hè/lề, tô **sát tới mép Mặt đường** (không để dải trống). Chi tiết ở 5.3                                                                                                                      |
| 5   | **Nền phía trên, Bỏ qua**                       | Semantic · `P` thô               | Trời, cây, cột = một đa giác `khac`; dãy nhà = một đa giác `nha_tuong` (đúng ở chân tường). Vùng phản chiếu, thanh giao diện trình phát (nếu camera chưa có Ignore template) = dòng `255 · Ignore / uncertain`. **Không** dùng Bỏ qua cho lòng đường gần camera |
| 6   | **Tự kiểm tra**                                 | —                                | **Overlay opacity tối đa**, bấm `L` bật/tắt: trong lòng đường **không còn chỗ nào chưa có màu** (chỗ trống hiện ra ảnh gốc). Màu `khac` trong Road Extent chỉ được là dị vật thật. Cạnh Road Extent có đi qua đống vật sát mép không (phải bao vật vào trong)?  |
| 7   | **Chuyển duyệt**                                | —                                | Chờ chữ **Saved** (nếu **Failed — retry**: bấm **Retry save**, chưa chuyển ảnh). `Ctrl+Enter`                                                                                                                                                                   |

Bước 1–3 bắt buộc phải xong kỹ; bước 4–5 làm nhanh hơn nếu thiếu thời gian. Ca khó: ghi vào **Notes** hoặc **Ask the team about this image** (mục 8.3); tra bảng ca khó v3 §7.3.

### 5.1. Chọn lớp cho vật (bước 2)

| Vật                                                                                                                                                              | Lớp               | Phím    |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------- | ------- |
| Dị vật nhô lên rõ, to hơn cục gạch: đống cát/đất/đá, gạch, cây và cành đổ, nón, barie, rào tôn, hàng đã rơi, bao rác, bàn ghế, rạp dựng trên đường, xác động vật | `khac`            | `Alt+2` |
| Xe máy, xe đạp, xe điện; **người ngồi trên xe** và hàng còn trên xe                                                                                              | `xe_may_xe_dap`   | `5`     |
| Xe con, 7 chỗ, bán tải, xe van, taxi                                                                                                                             | `o_to`            | `6`     |
| Xe tải, container, rơ moóc, xe buýt, xe khách ≥ 16 chỗ, máy xúc, xe lu, cần cẩu                                                                                  | `xe_tai_xe_buyt`  | `7`     |
| Ba gác (cả ba gác máy), xe lôi, xe đẩy, xe kéo tay, xích lô, xe bò, xe tự chế (cả hàng còn trên xe)                                                              | `xe_tho_so`       | `8`     |
| Người đi bộ, đứng, ngồi trên đường; **người đẩy, kéo xe**; người gánh (cả gánh)                                                                                  | `nguoi`           | `9`     |
| Chó, mèo, gà, gia súc còn sống                                                                                                                                   | `dong_vat`        | `Alt+1` |
| Đảo giao thông, dải phân cách cứng, cọc tiêu cố định, **phần chân** cột đèn tín hiệu / cột biển báo trong lòng đường                                             | `ket_cau_co_dinh` | `4`     |

- **Không bỏ vật nhỏ** (một nón, một cục đá lớn hơn cục gạch): nhãn không phụ thuộc ngưỡng β (v3 §1.5). Vật nhỏ, dẹt (không có mặt đứng hay bóng) thì để `mat_duong`.
- Đống đất/cát **cùng màu mặt đường**: SAM hay lấy thiếu; zoom vào, theo **bóng đổ và mép đống**, chỉnh bằng cọ `M` lớp `khac`.
- **Hàng đã đổ xuống đường** (kể cả từ xe ba gác) → `khac`, không gộp với xe. Xe hỏng, xe đỗ lâu ngày, xe trên vỉa hè → vẫn lớp của xe.
- Mép bánh xe, chân người: không để lẫn Mặt đường; ranh giới xe ↔ mặt đường ≈ 3 px ở gần camera. Sai loại xe ít hại hơn sai ranh giới.

### 5.2. Vẽ Road Extent (bước 3)

1. **LAYERS** → **Road Extent** (công cụ tự chuyển sang `P`). Để layer Semantic vẫn hiện để bám theo biên Mặt đường.
2. Bắt đầu từ góc dưới bên trái, click đỉnh dọc biên Mặt đường / Vỉa hè bên trái, lên phía xa, sang phải, xuống. **Một đa giác**, 6–12 đỉnh; thêm đỉnh ở đoạn mép cong hoặc răng cưa cho đủ khít (±3 px). Phóng to ở mép.
3. **Đỉnh ở mép ảnh**: click **hơi ra ngoài ảnh** (trong 12 px), đỉnh tự dính đúng mép. Đỉnh đặt lọt vào trong làm vùng hở sát khung (thử nghiệm: hở 1–7 px ở cả 5 ảnh); script kéo phần hở ≤ 8 px ra sát khung, nhưng nên đặt đúng ngay từ đầu.
4. **Đoạn mép bị xe, vật che**: nối thẳng từ đoạn nhìn thấy phía trước sang đoạn nhìn thấy phía sau, **xuyên qua** xe, người, vật. Đống vật liệu dài sát mép: đa giác đi ở **mép đường thật phía sau** đống để đống nằm **trong** đa giác (v3 §7.3).
5. Phía xa: dừng ở chỗ không còn phân biệt được mép, và **không kéo quá phần Mặt đường đã tô** (vùng trống giữa cuối Mặt đường và đỉnh trên tính là Bỏ qua trong lòng đường). Phía trên chân trời chỉ vẽ thô.
6. Đóng: **Enter** hoặc click lại đỉnh đầu. Sửa đỉnh: `V`, kéo đỉnh. Vẽ lại: chọn trong **ANNOTATIONS** → xóa → vẽ lại.

Quy tắc mép theo tình huống: **v3 §6.1** (bó vỉa → từ bó vỉa sang bó vỉa; không bó vỉa → mép mặt đường cứng; rãnh nằm ngoài; dải phân cách, đảo giao thông nằm trong; ngã tư mở rộng theo đường ngang). **Mép bị bùn phủ, không đoán được**: tạm thời vẽ theo mép đoán tốt nhất (cách đánh dấu Bỏ qua chưa chốt, v3 §13). Người gán **không** vẽ dải Bỏ qua dọc cạnh đa giác và **không** đánh dấu đoạn bị che: script làm.

### 5.3. Mép đường, chân tường (bước 4)

1. Dọc hai mép đa giác: **rãnh, mương, bậc, lề đất, cỏ, gờ đất, đất đắp, bãi đất trống sát đường, sân trước nhà → `via_he_le`**, tô sát tới Mặt đường. Nhãn sơ bộ hay tô các vùng này thành Khác.
2. Vật **được đặt lên** lề (đống, bao, thùng, rào) vẫn là `khac`; đống đất, phế thải đổ trên lề lấn ra đường → `khac`.
3. Nón, barie, rào nhựa di động, rào tôn → `khac` (không phải kết cấu cố định). Phần thân cột phía trên chân → `khac`.
4. `nha_tuong`: tường, cửa cuốn, cổng; chỉ cần đúng ở **chân tường**.

---

## 6. Ví dụ: ảnh mẫu (đường bùn, 2 xe máy, tảng đá, xe tải, bao rác)

Theo v3 §7.4.

| #   | Layer       | Thao tác                                                                                                                                            |
| --- | ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Semantic    | `1` mat_duong, `P`: tô cả mặt đường, gồm lớp bùn, vệt nước, que/cành nằm dẹt, chỗ dưới 2 xe máy và tảng đá                                          |
| 2   | Semantic    | `Alt+2` khac, `S`: khung quanh tảng đá → Enter; bao rác bên lề trái → Enter (nằm ngoài đa giác nên không tạo nghi vấn)                              |
| 3   | Semantic    | `5`, `S`: từng xe máy kèm người lái → Enter. `7`: xe tải → Enter                                                                                    |
| 4   | Road Extent | `P`: từ mép trái (mặt đường giáp lề đất có vũng nước) → lên tới xe tải → sang mép phải → xuống; xuyên qua 2 xe máy và tảng đá; bao rác để **ngoài** |
| 5   | Semantic    | `2` via_he_le: lề đất và phần vũng nước trên lề, tô sát Mặt đường; `3` nha_tuong cho dãy nhà xa; một đa giác `khac` cho cây, trời                   |
| 6   | Semantic    | Dòng `255 · Ignore / uncertain`: vùng phản chiếu tối góc dưới phải, thanh giao diện trình phát                                                      |
| 7   | —           | Opacity tối đa, `L` kiểm tra không còn chỗ trống, chờ **Saved**, `Ctrl+Enter`                                                                       |

---

## 7. Người duyệt

1. Mở project của người gán (hoặc người gán gửi **Backup DB + masks** kèm thư mục ảnh, người duyệt **Restore backup** ở Home).
2. Panel trái → **Review filter** → `needs_review`.
3. Mỗi ảnh (opacity tối đa, `L`), xem theo thứ tự:
   1. Road Extent bám mép ở đoạn nhìn thấy? Đống vật sát mép có nằm **trong**? Đỉnh ở mép ảnh có chạm khung?
   2. Ranh giới Mặt đường quanh xe và dị vật; vật nào còn màu Mặt đường?
   3. Chỗ chưa tô trong lòng đường (hiện ra ảnh gốc khi opacity tối đa)?
   4. Vùng mép: dải `khac` chạy dọc mép (rãnh, lề gán nhầm)? Khe trống giữa Mặt đường và Vỉa hè? Chân cột, đảo giao thông gán `khac`? Vũng nước, bóng cây gán `khac`?
   5. Hàng đã đổ xuống đường có bị gộp vào xe?
4. Đạt → `Ctrl+Shift+Enter` (**Approve → next**). Ảnh đã duyệt bị khóa: nhãn sơ bộ và Ignore template nạp sau không đè lên.
5. Chưa đạt → **Review state** = `rejected`, ghi lý do vào **Notes**. Người gán lọc `rejected` để sửa rồi `Ctrl+Enter` lại.
6. Lỗi lặp lại → bổ sung vào bảng ca khó (v3 §7.3), báo cả nhóm. **Tập val**: xem lại **toàn bộ** đa giác (v3 §9.4). Bảng lỗi hay gặp: v3 §10.2.

---

## 8. Kiểm soát chất lượng trong nhóm

### 8.1. Hiệu chuẩn 20 ảnh (trước khi gán)

1. Trưởng nhóm tạo project `hieuchuan_chuan`, gán chuẩn 20 ảnh có đủ nút giao, lề đất, vật sát mép, nhiều loại xe; duyệt hết.
2. Mỗi người tạo project `hieuchuan_<tên>` (cùng 11 lớp), nhập **đúng 20 ảnh đó**, nạp cùng nhãn sơ bộ, gán độc lập.
3. Mỗi project xuất ra (mục 9), rồi so:

   ```
   python tools\agreement.py E:\vnroad\labelflow_export\hieuchuan_chuan E:\vnroad\labelflow_export\hieuchuan_<tên> --min 0.95
   ```

   Ghép ảnh theo SHA-256, in tỷ lệ pixel trùng của Semantic (chỉ tính pixel **cả hai bên đều đã tô**, khác 255) và IoU của Road Extent. Đạt khi trùng **trên 95%** (v3 §10.1). Ảnh dưới ngưỡng: mở hai bản cạnh nhau, thống nhất quy tắc.

### 8.2. Đợt thử 30 ảnh trước vòng 1

Cả nhóm gán 30 ảnh, **ghi thời gian mỗi ảnh**, gom ca tranh cãi bổ sung vào bảng v3 §7.3. Số liệu này dùng để lập lịch.

### 8.3. Gán đôi 5% và thảo luận ca khó

- Giao 5% số ảnh mỗi lô cho hai người (ưu tiên lề đất, nút giao). Mỗi lô xong chạy `agreement.py`; mốc khoảng 96%. Nhóm lề đất thấp: thống nhất gán Bỏ qua cho dải chuyển tiếp mặt đường ↔ lề đất (v3 §10.1).
- Khi gộp, `convert_labelflow.py` chỉ lấy **bản đầu tiên** của ảnh trùng tên theo thứ tự thư mục xuất trong lệnh, và ghi vào `convert_log.csv`. Đặt thư mục xuất của người duyệt lên trước.
- Thảo luận không cần server: cả nhóm dùng một thư mục đồng bộ (LAN, OneDrive…). Thanh công cụ → **Team discussion** → nhập tên → **Choose shared folder**. Ca khó: **Ask the team about this image** → viết câu hỏi → tùy chọn **Pin a spot**, **Share my current labels** → **Post question**. Người khác thấy badge "N new", trả lời, **Mark resolved**. Thảo luận không tự sửa nhãn.

---

## 9. Xuất và tạo bộ dữ liệu huấn luyện

1. Xuất từ mỗi project (`Ctrl+E`, **Export dataset**): **Semantic + region PNGs**, **Approved images only**, **Train % = 100, Validation % = 0** → `E:\vnroad\labelflow_export\`. Mỗi lần xuất tạo thư mục `LabelFlow-export-<uuid>` mới. Chia tập do script làm theo `videos.csv` (LabelFlow chia ngẫu nhiên theo băm nhóm).
2. Chạy, truyền **mọi** thư mục xuất của mọi người gán trong cùng một lệnh (thư mục `data\` được làm lại mỗi lần; ảnh không còn trong lần này bị xóa khỏi đó):

   ```
   python tools\convert_labelflow.py <xuất_1> <xuất_2> ... --out E:\vnroad\data --videos E:\vnroad\manifest\videos.csv --chi-duyet --ignore-dir E:\vnroad\ignore
   python tools\check_labels.py E:\vnroad\data
   python tools\overlay.py E:\vnroad\data --qc E:\vnroad\data\qc_report.csv --chi-loi
   ```

   Ba lệnh chạy song song nhiều tiến trình (mặc định tối đa 8, đổi bằng `-j`). Mỗi lệnh trả mã 1 nếu có lỗi chặn.

3. Đọc `convert_log.csv` (ảnh bị bỏ, lỗi chặn) và `qc_report.csv` (mở bằng Excel; cột `muc`: `dat`, `canh_bao`, `chan`, `thong_ke`, `thong_ke_thieu`). Ảnh có cảnh báo: xem `data\overlay\` (khung trên: nhãn + cạnh đa giác; khung dưới: vùng 255 trong lòng đường tô **hồng đậm**).
4. Ảnh `chan` hoặc `canh_bao`: người duyệt đổi về `rejected` kèm Notes, người gán sửa, duyệt lại, xuất lại, chạy lại bước 2.

| Thông báo                                                                    | Mức      | Sửa trong LabelFlow                                                                                            |
| ---------------------------------------------------------------------------- | -------- | -------------------------------------------------------------------------------------------------------------- |
| `bộ lớp semantic không khớp lop.yaml` (lớp thừa, sai tên, thiếu ID)          | Dừng     | Edit classes: đúng 11 lớp, đúng thứ tự (mục 2.1)                                                               |
| `Rò rỉ giữa tập`                                                             | Dừng     | Sửa cột `tap` trong `videos.csv`: một video một tập                                                            |
| `không tìm thấy video … trong videos.csv`                                    | Dừng     | Thêm video vào `videos.csv` hoặc sửa tên ảnh                                                                   |
| `giá trị semantic lạ`, `road_extent không nhị phân`, `kích thước … khác ảnh` | Chặn     | Mở ảnh kiểm tra; nạp lại nhãn sơ bộ đúng                                                                       |
| `có N vùng Road Extent (cần đúng 1)`                                         | Chặn     | Road Extent: một đa giác liền, xóa mảnh thừa                                                                   |
| `không có pixel Mặt đường`                                                   | Chặn     | Kiểm tra ảnh (có thể hỏng, nên loại)                                                                           |
| `Bỏ qua chiếm x% Road Extent …, vùng lớn nhất x,y,w,h=…`                     | Cảnh báo | Tô vật hoặc vùng trống ở tọa độ đó; vùng ở phía xa: tô tiếp Mặt đường hoặc kéo đỉnh trên của Road Extent xuống |
| `chỉ x% Mặt đường nằm trong Road Extent`                                     | Cảnh báo | Đa giác hẹp, hoặc lề bị tô nhầm `mat_duong`                                                                    |
| `lệch mép > 3 px ở x% số hàng …`                                             | Cảnh báo | Đặt lại đỉnh bám biên Mặt đường, thêm đỉnh ở đoạn cong                                                         |
| `N hàng có khe chưa tô (255) giữa Mặt đường và Vỉa hè/Nhà`                   | Cảnh báo | Tô Vỉa hè/lề sát tới Mặt đường                                                                                 |
| `dải Khác mảnh dọc mép …`                                                    | Cảnh báo | Rãnh, lề, đất đắp đang là `khac` → `via_he_le`                                                                 |
| `(thong ke) <lớp>: có mặt trong N ảnh, dưới 30`                              | Thống kê | Bổ sung ảnh có lớp đó ở vòng sau                                                                               |

Kết quả ở `E:\vnroad\data\`: `images\`, `masks\` (0–10, 255), `road\` (0/1, 255 = mặt nạ camera), `edge\` (1 nhìn thấy, 2 bị che, 3 không xác định), `list\train.lst`, `val.lst` (`test.lst` nếu có), `lop.yaml`, `convert_log.csv`, `qc_report.csv`. Mỗi dòng `.lst`: `images/X.png masks/X.png road/X.png`. Chi tiết: `tools/README.md`.

---

## 10. Nhãn cho video test (v3 §9)

Video test **không** gán mặt nạ từng khung. Dùng một project riêng `test_events`, cùng 11 lớp.

**Nhãn đoạn — `test_segments.csv` (v3 §9.1):** LabelFlow không có nhãn theo đoạn thời gian. Làm trong bảng tính: mở video trong **Video workspace** (xem giờ chính xác từng khung) hoặc trình phát bất kỳ; ghi `video_id, giay_bat_dau, giay_ket_thuc, nhan` (nhiều nhãn cách bằng `;`).

**Nhãn sự kiện — `test_events.csv` (v3 §9.2):**

1. Tạo `test_events.csv`, mỗi vật một dòng: `obj_id, video_id, giay_xuat_hien, giay_bien_mat, nhom, vi_tri, kieu_duong, co_trong_train`. Các cột `giay_ro_nhat, hop_bao, da_giac_long_duong, muc_chiem` để script điền.
2. Cài FFmpeg (một lần): **Video workspace** → **Locate FFmpeg tools** → chọn thư mục chứa `ffmpeg.exe`, `ffprobe.exe`.
3. **Import video** → chọn video test, chờ lập chỉ mục.
4. Mỗi vật: dùng thanh thời gian, **Previous/Next frame** tìm **khung rõ nhất** → **Extract selected frame**. Khung trích là **khung nguyên, chưa cắt phần trời**: nếu thuật toán so tọa độ trên ảnh đã cắt, trừ `ty_le_cat_tren × chiều cao` ở tọa độ y (`muc_chiem` không đổi).
5. Trên khung đó:
   1. **LAYERS** → **Candidate objects** → `B` → kéo hộp bao **toàn bộ phần nhìn thấy** của vật, kể cả phần ngoài lòng đường. Mỗi vật một hộp.
   2. **Road Extent** → `P` → vẽ như mục 5.2.
   3. **Notes**: `obj_id` của các hộp, cách nhau bằng dấu cách, **theo thứ tự hộp từ trái sang phải**. Ví dụ `OBJ_041 OBJ_042`.
   4. **Approve → next**.
6. **Export dataset** → **YOLO detection + semantic/region PNGs**, Train % = 100 → `E:\vnroad\labelflow_export\test_events_<ngày>`.
7. Chạy:

   ```
   python tools\test_events.py E:\vnroad\labelflow_export\test_events_<ngày> E:\vnroad\manifest\test_events.csv
   ```

   Script điền `giay_ro_nhat`, `hop_bao` (x, y, w, h), `da_giac_long_duong` (đường dẫn mặt nạ Road Extent trong thư mục xuất: giữ thư mục này cùng bộ dữ liệu) và `muc_chiem` = bề rộng lớn nhất của phần vật nằm trong đa giác chia bề rộng đa giác cùng hàng. Số hộp khác số `obj_id` trong Notes → khung bị bỏ qua và in ra để sửa.

Quy tắc chấm điểm (v3 §9.3) làm trong code đánh giá, dùng hai file CSV.

---

## 11. Fine-tune PIDNet-S từ dữ liệu này

LabelFlow dừng ở bước xuất. Phần huấn luyện làm trên server:

1. **Dữ liệu vào**: `data\list\train.lst`, `val.lst`; mỗi dòng: ảnh, `masks` (0–10, 255 = bỏ qua), `road` (0/1, 255 = bỏ qua). Không dùng `test.lst` để huấn luyện.
2. **Hai đầu ra, hai loss**: P (11 kênh) cross-entropy với `masks`, `ignore_index=255`; R với `road`, bỏ pixel 255. Loader đặt 255 cho dải 4 px hai bên cạnh đa giác (v3 §6.2) và giảm trọng số loss của R ở đoạn bị che (`edge` = 2).
3. **Dán xe, người** (v3 §5.2): cắt xe, người từ các ảnh đã gán (theo `masks`), dán lên ảnh đường thoáng; nhãn R phía sau vật dán chính xác tuyệt đối.
4. **Ảnh vào** là ảnh đã cắt phần trời giống box; resize ảnh song tuyến, nhãn **nearest**.
5. **Chọn checkpoint** trên `val.lst`; đo sai số mép của R tách đoạn nhìn thấy / bị che theo `edge\` (v3 §9.4).
6. **So 11 lớp với 7 lớp gộp** ở vòng 1: ánh xạ `gop7` trong `lop.yaml`, không gán lại (v3 §12.1).
7. **Vòng 2, 3** (400–600 ảnh): chọn ảnh theo v3 §12.2, nạp dự đoán theo mục 3.2, gán tiếp theo mục 5.

---

## 12. Giới hạn hiện tại và phần đã / chưa kiểm chứng

| Giới hạn                                                          | Cách xử lý                                                               |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------ |
| Giao diện tiếng Anh                                               | Dùng tài liệu này                                                        |
| Pixel chưa tô = 255, không có "khởi tạo bằng Khác"                | Quy ước chưa tô = Bỏ qua + cảnh báo "Bỏ qua trong lòng đường" (mục 0, 9) |
| Ignore template áp theo kích thước ảnh, chỉ cho semantic          | Mỗi camera một project; `--ignore-dir` cho cả Road Extent (mục 2.3)      |
| Layer vùng chỉ 0/1, không có "đoạn Bỏ qua" trên đa giác           | Chưa chốt (v3 §13)                                                       |
| LabelFlow chia tập theo thư mục ảnh, ngẫu nhiên theo băm          | `--videos videos.csv` (mục 9)                                            |
| Một project một người ghi                                         | Mỗi người một project; gộp bằng script                                   |
| Không cắt phần trời khi trích khung; không có nhãn đoạn thời gian | FFmpeg (mục 1.5); bảng tính (mục 10)                                     |
| AI assistance không chạy PIDNet-S hai đầu ra                      | Nạp dự đoán hàng loạt (mục 3.2)                                          |
| Ảnh nguồn tối đa 64 MiB, 24 megapixel                             | Không ảnh hưởng ảnh 1080p                                                |

**Đã kiểm chứng** (18/11/2026, bản sao dự án `test_vong_1`, xem `bao_cao_kiem_tra_labelflow.md`): bộ lớp; chưa tô = 255; Road Extent 0/1; tô sau ghi đè; phạm vi Ignore template; nhóm chia tập = thư mục; `tools/` chạy trên bản xuất thật và dữ liệu tổng hợp. Ngày 16/11/2026: tạo project, nạp nhãn sơ bộ, phím `1`–`9`, `Alt+1`–`Alt+9`, SAM, Done/Approve → next, xuất.

**Chưa kiểm chứng:** Mask2Former thật (máy thử không có mô hình/GPU); `agreement.py` trên hai project thật; quy trình Video workspace ở mục 10; nạp PIDNet-S vào Road Extent; tốc độ gán thực tế (đo ở đợt thử 30 ảnh); Việc 5 (ảnh `VID4_0611_00232.0.png` chưa gán).

---

## Phụ lục A — Thẻ thao tác một trang

**Mỗi ảnh:**

1. **Semantic** · `1` mat_duong · `P`: tô mặt đường **rộng**, cả dưới xe và vật.
2. **Semantic** · `S`: chọn lớp → kéo khung quanh vật → (click phải bỏ phần thừa) → **Enter**. Dị vật `Alt+2` · xe máy `5` · ô tô `6` · xe tải/buýt `7` · ba gác `8` · người `9` · động vật `Alt+1` · chân cột/đảo `4`. Mọi vật nhô lên, kể cả nhỏ. Hàng đã rơi → `khac`.
3. **Road Extent** · `P`: bám biên Mặt đường ở đoạn thấy; nối thẳng qua đoạn bị che; **một đa giác**; đỉnh ở mép ảnh click hơi ra ngoài; không kéo quá phần đã tô ở phía xa.
4. **Semantic** · `2` via_he_le · `M`: rãnh/lề/cỏ/đất đắp, tô **sát** Mặt đường, không để khe.
5. Trời, cây → một đa giác `khac`; nhà → `3`. Phản chiếu, thanh giao diện → dòng `255 · Ignore / uncertain`.
6. Opacity tối đa + `L`: **không còn chỗ trống trong lòng đường** → chờ **Saved** → `Ctrl+Enter`.

**Ghi nhớ:** chưa tô = Bỏ qua (mô hình không học) · tẩy = Bỏ qua · tô sau đè tô trước · trong lòng đường không chắc → khac · ngoài lòng đường, thuộc về nền → via_he_le · không chắc loại xe → loại gần nhất · nón, barie, rào tạm → khac.

## Phụ lục B — Lệnh dùng thường xuyên (kỹ sư ML)

```
:: Nhãn sơ bộ vòng 1 (máy GPU)
python tools\prelabel_mask2former.py <ảnh> <ra> --xem-anh-xa
python tools\prelabel_mask2former.py <ảnh> <ra> --fp16

:: Độ đồng thuận (hiệu chuẩn, gán đôi)
python tools\agreement.py <xuất_A> <xuất_B> --min 0.95

:: Tạo bộ dữ liệu + kiểm tra + xem ảnh lỗi
python tools\convert_labelflow.py <xuất_1> <xuất_2> ... --out E:\vnroad\data --videos E:\vnroad\manifest\videos.csv --chi-duyet --ignore-dir E:\vnroad\ignore
python tools\check_labels.py E:\vnroad\data
python tools\overlay.py E:\vnroad\data --qc E:\vnroad\data\qc_report.csv --chi-loi

:: Nhãn sự kiện video test
python tools\test_events.py <xuất_test> E:\vnroad\manifest\test_events.csv
```
