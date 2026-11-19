# Báo cáo kiểm tra LabelFlow — cấu hình dự án và hành vi xuất nhãn

| Mục            | Nội dung                                                                                                                                                                                                                                                                                                                                     |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Ngày           | 18/11/2026                                                                                                                                                                                                                                                                                                                                   |
| Dự án kiểm tra | `E:\vnroad\project_test1` (tên `test_vong_1`, profile `road-encroachment-v1`, schema 6)                                                                                                                                                                                                                                                      |
| Cách làm       | Chỉ đọc. DB (`project.sqlite` + `-wal` + `-shm`) và thư mục `.labelflow` được **sao chép** sang thư mục nháp; mọi truy vấn và lần xuất thử chạy trên bản sao. Không sửa DB gốc                                                                                                                                                               |
| Nguồn xác minh | (1) bảng `classes`, `layers`, `media`, `annotations`, `annotation_operations` của bản sao; (2) **xuất thật** bằng mã xuất của LabelFlow trên bản sao (test `src-tauri/tests/vnroad_export_copy.rs`); (3) mã nguồn LabelFlow (`src-tauri/src/export/mod.rs`, `src/features/annotations/tools/controller.ts`, `src-tauri/src/import/masks.rs`) |
| Ảnh minh họa   | `bao_cao_kiem_tra_labelflow_anh/` (5 ảnh chồng nhãn + `qc_report_5_anh_thu.csv`)                                                                                                                                                                                                                                                             |

**Tóm tắt**

1. Bộ lớp 0–10 đúng tên, đúng ID; Bỏ qua = 255 có sẵn. **Còn lớp thừa `11 · bo_qua`** (chưa ảnh nào dùng, xóa được ngay).
2. **Chia tập chưa theo video:** cả 73 ảnh (3 video VID2, VID3, VID4) cùng một nhóm là đường dẫn thư mục `batch_01`.
3. **Pixel chưa tô xuất ra 255, không phải 10.** Ảnh chưa có mặt nạ xuất toàn 255.
4. Road Extent xuất ra **0/1** (không có 255), một đa giác 6 đỉnh, vẽ xuyên qua xe.
5. **Tô sau ghi đè tô trước** với mọi công cụ (cọ, đa giác, SAM), trừ khi chủ động chọn _Apply to unlabeled only_ (Shift+Enter) của SAM.
6. Trên 5 ảnh đã gán: ranh giới SAM–Mặt đường gần như kín, nhưng có **khe 255 dọc mép Mặt đường/Vỉa hè**, **vùng 255 lớn quanh xe ở gần camera** (ảnh `VID2_0611_00014.0`), và Road Extent kéo xa hơn phần đã tô ở gần chân trời.
7. **Road Extent dừng cách khung ảnh 1–7 px** ở cả 5 ảnh (đỉnh click hơi lọt vào trong ảnh). `convert_labelflow.py` kéo phần hở ≤ 8 px ra sát khung.
8. **Việc 5 chưa làm được:** ảnh `VID4_0611_00232.0.png` vẫn ở trạng thái `unlabeled` (DB lưu lần cuối 17/11 08:51). Ngoài ra VID4_0611 thuộc **tập test** theo `videos.csv`.

---

## Việc 1 — Cấu hình dự án

### 1.1. Vị trí

| Thành phần    | Đường dẫn                                                                                                                                                |
| ------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Thư mục dự án | `E:\vnroad\project_test1\.labelflow\` (đọc từ `%APPDATA%\app.labelflow.desktop\session.json`)                                                            |
| SQLite        | `project.sqlite` (580 KB) + `project.sqlite-wal` (4,1 MB, phần lớn dữ liệu nằm ở đây, phải chép cùng)                                                    |
| Mặt nạ        | `.labelflow\masks\<sha256>.png` (88 file, PNG 8-bit, gồm cả các phiên bản cũ trong lịch sử). Không có thư mục `.labelflowmasks/` như mô tả trong yêu cầu |
| Ảnh           | Liên kết (linked), không chép vào dự án: `E:\vnroad\batches\batch_01\*.png`, 73 ảnh 1920×1080                                                            |

### 1.2. Bảng `classes` (namespace semantic `cityscapes`)

| ID xuất | stable_key                    | is_ignore | Đối chiếu bảng lớp                                                                                                 |
| ------- | ----------------------------- | --------- | ------------------------------------------------------------------------------------------------------------------ |
| 0       | mat_duong                     | 0         | Đúng                                                                                                               |
| 1       | via_he_le                     | 0         | Đúng                                                                                                               |
| 2       | nha_tuong                     | 0         | Đúng                                                                                                               |
| 3       | ket_cau_co_dinh               | 0         | Đúng                                                                                                               |
| 4       | xe_may_xe_dap                 | 0         | Đúng                                                                                                               |
| 5       | o_to                          | 0         | Đúng                                                                                                               |
| 6       | xe_tai_xe_buyt                | 0         | Đúng                                                                                                               |
| 7       | xe_tho_so                     | 0         | Đúng                                                                                                               |
| 8       | nguoi                         | 0         | Đúng                                                                                                               |
| 9       | dong_vat                      | 0         | Đúng                                                                                                               |
| 10      | khac                          | 0         | Đúng                                                                                                               |
| **11**  | **bo_qua**                    | **0**     | **Thừa.** Là lớp thường, xuất ra giá trị 11, không phải 255. Còn từ quy ước cũ (hướng dẫn LabelFlow bản v2/v3 đầu) |
| 255     | ignore ("Ignore / uncertain") | 1         | Đúng: đây là Bỏ qua. Lớp cố định của app, không đổi tên hay xóa được                                               |

Ngoài ra namespace `candidate` có 5 lớp (material_pile, goods_cluster, street_stall, cart, standalone_object) của layer Candidate objects; không ảnh hưởng mặt nạ semantic.

Lớp 11 hiện **không có pixel nào** trong mặt nạ đã lưu, nên app cho phép xóa.

### 1.3. Bảng `layers`

| id                    | layer_type | Dùng?       | Ghi chú                                                                                               |
| --------------------- | ---------- | ----------- | ----------------------------------------------------------------------------------------------------- |
| `cityscapes_semantic` | semantic   | Có (file 2) | Tên hiển thị "Cityscapes semantic" (trong editor hiện là "Semantic"); tên id cố định, không ảnh hưởng |
| `road_extent`         | **region** | Có (file 1) | Đúng yêu cầu                                                                                          |
| `candidate_objects`   | instance   | Không       | Lớp có sẵn của profile, không xóa được; đang trống                                                    |
| `traffic_exclusion`   | region     | Không       | Đang trống ở cả 73 ảnh                                                                                |
| `suspected_region`    | region     | Không       | Đang trống                                                                                            |

### 1.4. Nhóm chia tập (videoId)

| Kiểm tra                | Kết quả                                                                                                                                                                       |
| ----------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `media.split_group_id`  | **Cả 73 ảnh = `\\?\E:\vnroad\batches\batch_01`** (LabelFlow lấy thư mục chứa ảnh lúc nhập làm nhóm)                                                                           |
| `source_video_id`, trip | Trống (ảnh nhập dạng file, không qua Video workspace)                                                                                                                         |
| `dataset_splits`        | Trống (chưa xuất lần nào trên DB gốc)                                                                                                                                         |
| Hệ quả                  | Ba video VID2_0611, VID3_0611, VID4_0611 là **một nhóm**: khi xuất, cả 73 ảnh rơi vào cùng một tập; không rò rỉ nhưng cũng không chia được                                    |
| Cách LabelFlow chia     | `split = băm(seed, nhóm) mod 100` so với % train/val. Nhóm nào vào tập nào là **ngẫu nhiên theo băm**, không theo `videos.csv`. Với 3 video, có thể VID4 (test) rơi vào train |

### 1.5. Trạng thái gán

| Trạng thái     | Số ảnh                                                                   |
| -------------- | ------------------------------------------------------------------------ |
| `needs_review` | 5: `VID2_0611_00011.0`, `00013.0`, `00014.0`, `00014.5`, `00015.0`       |
| `unlabeled`    | 68, gồm cả `VID4_0611_00232.0`                                           |
| `approved`     | 0 → xuất với "Approved images only" sẽ báo _No images match this export_ |

### 1.6. Điểm sai lệch và cách sửa trên giao diện

| #   | Sai lệch                                               | Mức                          | Cách sửa                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| --- | ------------------------------------------------------ | ---------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Lớp thừa `11 · bo_qua`                                 | Nên sửa                      | Editor → panel lớp → **Edit classes** → mục _Semantic classes_ → dòng `11 bo_qua` → **Delete** → xác nhận. Dùng **255 Ignore / uncertain** cho mọi vùng Bỏ qua. (Nếu vẫn giữ, `convert_labelflow.py` đổi 11 → 255 theo `lop.yaml → labelflow.doi_id`.) Sau khi xóa, phím Alt+3 không còn gán lớp nào                                                                                                                                                      |
| 2   | Nhóm chia tập = thư mục, không phải video              | Phải sửa trước khi xuất thật | **Ảnh mới:** đặt khung của mỗi video vào thư mục con riêng trước khi nhập (`batch_02\VID2_0611\…`, `batch_02\VID3_0611\…`); LabelFlow tự lấy thư mục con làm nhóm. **73 ảnh hiện có:** gõ `VID2_0611` / `VID3_0611` / `VID4_0611` vào ô **Dataset split group** ở panel phải của từng ảnh, hoặc bỏ qua và để `convert_labelflow.py --videos videos.csv` chia tập (khuyến nghị, vì LabelFlow chia ngẫu nhiên theo băm nên không bảo đảm đúng `videos.csv`) |
| 3   | Ảnh thử VID4 thuộc tập test                            | Lưu ý                        | Theo Hướng dẫn v3 §3.3 và §9, video test không gán mặt nạ để huấn luyện. Dùng ảnh này để thử công cụ thì được; `convert_labelflow.py --videos` đưa nó vào `list/test.lst`, không vào train. Gán 10–20 ảnh huấn luyện từ VID2 (train)                                                                                                                                                                                                                      |
| 4   | Chưa ảnh nào `approved`                                | Lưu ý                        | Người duyệt chuyển ảnh sang `approved` trước khi xuất "Approved images only"                                                                                                                                                                                                                                                                                                                                                                              |
| 5   | Traffic exclusion, Suspected region, Candidate objects | Không sai                    | Không xóa được (thuộc profile). Không dùng; script bỏ qua                                                                                                                                                                                                                                                                                                                                                                                                 |
| 6   | Road Extent cách khung ảnh 1–7 px (trái, phải, đáy)    | Nên sửa khi gán              | Đỉnh ở mép ảnh: click **hơi ra ngoài ảnh** (trong 12 px), đỉnh tự dính đúng mép. Ảnh đã gán: `convert_labelflow.py` tự kéo phần hở ≤ 8 px ra sát khung (`lop.yaml → labelflow.bam_khung_px`); nếu không, khung ảnh bị coi là mép đường trong `edge/`                                                                                                                                                                                                      |

---

## Việc 2 — Hành vi xuất nhãn

Vì ảnh thử VID4 chưa được gán, tôi xuất thật **bản sao** của dự án bằng chính mã xuất của LabelFlow (định dạng _Semantic + region PNGs_, Train 100 %, không giới hạn approved) và phân tích 5 ảnh VID2 đã gán. Các kết luận về giá trị pixel được đối chiếu với mã nguồn, nên vẫn đúng cho ảnh VID4 khi gán xong. Phần kiểm tra trên đúng ảnh VID4 (vùng trống cố ý) làm lại khi có nhãn (xem Việc 5).

### 2.1. Cấu trúc thư mục xuất

```
LabelFlow-export-<uuid>/
  manifest.json            danh sách ảnh, tập, nhóm, trạng thái, cảnh báo, sha256 mọi file
  classes.json             profile: toàn bộ lớp (cả candidate) và layer
  semantic-stats.json      số pixel, tỷ lệ, số ảnh của từng lớp semantic
  images/<tap>/<media-uuid>.png                         ảnh (PNG, đã chuẩn hóa EXIF)
  masks/cityscapes_semantic/<tap>/<media-uuid>.png      file 2 (semantic)
  masks/road_extent/<tap>/<media-uuid>.png              file 1 (Road Extent)
  masks/traffic_exclusion/<tap>/<media-uuid>.png        không dùng (toàn 0)
  masks/suspected_region/<tap>/<media-uuid>.png         không dùng (toàn 0)
```

- **Tên file là UUID của ảnh trong LabelFlow**, không phải tên gốc. Tên gốc nằm ở `manifest.json → images[].source`. `convert_labelflow.py` đổi lại về tên gốc.
- **Không có `data.yaml`** ở định dạng này: app chỉ ghi `data.yaml` cho định dạng YOLO / YOLO-seg / "all".
- Mọi layer không phải instance đều được xuất, kể cả layer không dùng.
- App ghi chia tập vào bảng `dataset_splits` của dự án sau mỗi lần xuất (lần thử này ghi vào bản sao).

### 2.2. PNG semantic

PNG 8-bit một kênh (mode L), 1920×1080. Giá trị và số pixel của 5 ảnh:

| Ảnh               | 0     | 1    | 2     | 4    | 5    | 6    | 10   | **255**   |
| ----------------- | ----- | ---- | ----- | ---- | ---- | ---- | ---- | --------- |
| VID2_0611_00011.0 | 75,7% | 2,7% | 12,9% | 0,8% | 2,3% | 0,3% | 1,1% | **4,2%**  |
| VID2_0611_00013.0 | 74,4% | 1,8% | 3,3%  | 4,3% | 6,1% | 1,9% | 3,0% | **5,1%**  |
| VID2_0611_00014.0 | 69,8% | 0,3% | 2,6%  | 8,4% | 1,3% | —    | 0,1% | **17,5%** |
| VID2_0611_00014.5 | 73,3% | 2,2% | 10,4% | 1,4% | —    | 5,1% | 0,1% | **7,5%**  |
| VID2_0611_00015.0 | 74,3% | 2,8% | 11,4% | 2,4% | —    | —    | 2,0% | **7,1%**  |

**Kết luận: pixel chưa tô xuất ra 255, không phải 10.** Bằng chứng:

- Mã xuất khởi tạo raster semantic bằng 255 khi ảnh chưa có mặt nạ; khi có mặt nạ thì ghi nguyên giá trị đã lưu (`export/mod.rs`, dòng 289–300).
- Editor tạo raster mới cũng bằng 255; công cụ tẩy (eraser) ghi lại 255 (`controller.ts`, dòng 236–241, 380, 482).
- 68 ảnh chưa gán xuất ra **toàn 255**; app cảnh báo _"68 images have no semantic mask; they export as all Ignore (255)."_
- Pixel 255 trong 5 ảnh nằm đúng ở các vùng không tô: trời/cây phía trên, khe dọc mép, vùng quanh xe (mục 2.4).

Hệ quả: theo quy ước mới (255 = Bỏ qua), **vật quên tô trở thành Bỏ qua, không bị phạt khi huấn luyện**. Vì vậy kiểm tra "Bỏ qua trong Road Extent > 1%" là bắt buộc.

### 2.3. PNG Road Extent

| Thuộc tính           | Kết quả                                                                                                                                                                                                                            |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Giá trị              | **0 và 1** (không có 255). Mã: raster region khởi tạo 0, rồi OR từng đa giác (`export/mod.rs`, dòng 321–334)                                                                                                                       |
| Số vùng              | 1 ở cả 5 ảnh; đa giác 6 đỉnh, không có lỗ                                                                                                                                                                                          |
| Sát khung ảnh        | Không chạm khung: x nhỏ nhất 1–7, y lớn nhất 1075–1079 (ảnh 1920×1080). Phần hở 3.600–12.300 px mỗi ảnh, khoảng 2/3 trong đó đã tô Mặt đường, tức là người gán định vẽ tới khung                                                   |
| Xuyên qua xe         | **Có.** Đa giác là các đoạn thẳng nối đỉnh, không né xe. Dọc cạnh đa giác, 11–22% chiều dài nằm ngay cạnh xe/vật (đoạn bị che, `edge/` = 2) ở 4/5 ảnh; ví dụ `VID2_0611_00014.0`: cạnh trái cắt thẳng qua xe máy và đầu xe tải nhỏ |
| Phủ Mặt đường        | 98,9–99,5% pixel Mặt đường nằm trong Road Extent (đạt ngưỡng 95%)                                                                                                                                                                  |
| Độc lập với semantic | Đúng (manifest: _"regionCompositing: binary union … independent of semantic classes"_)                                                                                                                                             |

Vì file xuất không có 255, `road/` của dự án (0, 1, 255) chỉ có 255 khi áp mặt nạ camera (`convert_labelflow.py --ignore-dir`).

### 2.4. Ranh giới SAM – Mặt đường, khe hở, ghi đè

**Khe giữa vật (SAM) và Mặt đường: gần như không có.** Số pixel 255 nằm trong Road Extent và cách cả vật lẫn Mặt đường ≤ 2 px: 81, 73, 1, 93, 0 px ở 5 ảnh (không đáng kể).

**Nhưng có ba loại vùng 255 khác trong lòng đường:**

| Loại                           | Ảnh       | Mô tả                                                                                                                                                                                      |
| ------------------------------ | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Vùng quanh xe ở gần camera     | `00014.0` | Một vùng 255 khoảng 28.000 px (x=190–872, y=269–626) quanh xe máy và dưới đầu xe tải nhỏ, cộng viền 255 bao quanh xe máy. Đây là đúng kiểu lỗi "quên tô" mà quy ước 255 che đi             |
| Phía xa, gần chân trời         | 4/5 ảnh   | Road Extent kéo tới y≈70–90 nhưng phần đường xa chưa tô (vùng 5.000–13.000 px). Hướng dẫn v3 cho phép Bỏ qua vùng quá xa, nhưng vùng này góp phần lớn vào tỷ lệ "Bỏ qua trong Road Extent" |
| Khe dọc mép Mặt đường / Vỉa hè | 4/5 ảnh   | Giữa Vỉa hè và Mặt đường còn dải 255 rộng 5–30 px ở 114–257 hàng mỗi ảnh. Cạnh Road Extent nằm trong khe này, nên không xác định được biên nhìn thấy                                       |

Tỷ lệ 255 trong Road Extent, đo trên file xuất nguyên gốc: 0,90% / 1,14% / **2,49%** / 1,21% / 0,57% → 3 ảnh vượt ngưỡng 1%. Sau khi `convert_labelflow.py` bám khung và `check_labels.py` bỏ dải 8 px sát khung, số liệu ở mục 2.6.

**Tô sau ghi đè tô trước: Có**, với mọi công cụ:

- Mã: cọ, đa giác tô (`fillSemantic`, chú thích _"later shapes win"_) và SAM _Apply mask_ ghi thẳng giá trị lớp lên mọi pixel trong vùng. Chỉ SAM **Apply to unlabeled only** (Shift+Enter) giữ pixel đã có lớp (chỉ ghi lên 255).
- Lịch sử chỉnh sửa trong DB (so các mặt nạ liên tiếp đã lưu) có các chuyển đổi giữa hai lớp thật: Mặt đường → Vỉa hè 3.357 px, Nhà → Xe máy 1.768 px, Xe tải → Xe máy 1.321 px, Vỉa hè → Mặt đường 1.013 px, Xe máy → Ô tô 715 px. Không có bước "xóa về 255 rồi mới tô", nên lớp sau ghi đè trực tiếp lớp trước.
- Ignore template cũng ghi đè (đặt 255 lên mọi pixel trắng của template, kể cả pixel đã tô), nhưng chỉ áp cho ảnh **đã có** mặt nạ semantic và **chưa duyệt**.

Ghi chú về cách người gán đã làm: lịch sử không có chuyển đổi Mặt đường → xe (0 px). Người gán tô Mặt đường **quanh** xe chứ không tô phủ dưới xe rồi để SAM đè lên. Vì tô đè được, thứ tự đúng là Mặt đường phủ rộng trước, SAM sau (xem `huong_dan_thao_tac_labelflow.md`); cách này cũng xóa được khe quanh xe như ở ảnh `00014.0`.

### 2.5. Ba file JSON

**`manifest.json`**

| Khóa                                 | Nội dung                                                                                                                                                                                         |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `formatVersion`                      | 1                                                                                                                                                                                                |
| `ignoreId`                           | 255                                                                                                                                                                                              |
| `semanticIds`                        | "project semantic class export IDs (see classes.json, namespace cityscapes); 255 = ignore"                                                                                                       |
| `regionCompositing`                  | Region = hợp nhị phân của raster vùng và các đa giác, độc lập với semantic                                                                                                                       |
| `seed`, `trainPercent`, `valPercent` | Tham số chia tập của lần xuất                                                                                                                                                                    |
| `images[]`                           | Mỗi ảnh: `id` (UUID = tên file), `source` (đường dẫn gốc), `sourceSha256`, `width`, `height`, `split`, `group` (nhóm chia tập), `revision`, `reviewStatus`, `eventLabel`, `notes`, `sourceVideo` |
| `warnings`                           | Lần thử: (1) lớp không có pixel nào: 3, 7, 8, 9, 11; (2) 68 ảnh không có mặt nạ → toàn 255; (3) 1 ảnh có hơn 10% Ignore                                                                          |
| `checksums`                          | sha256 của mọi file đã ghi (367 file)                                                                                                                                                            |

**`classes.json`**: profile hiện tại: `id` = `road-encroachment-v1`, `classes[]` (mỗi lớp: `id`, `namespace`, `stableKey`, `exportId`, `displayName`, `color`, `isIgnore`; gồm cả 5 lớp candidate và lớp thừa `bo_qua` 11), `layers[]` (5 layer như mục 1.3). `convert_labelflow.py` đọc file này để kiểm tra bộ lớp trước khi chuyển.

**`semantic-stats.json`**: với mỗi lớp semantic: `pixels`, `share` (tỷ lệ trên toàn bộ pixel đã xuất, kể cả ảnh chưa gán), `images` (số ảnh có lớp đó); thêm `imagesWithoutSemanticMask` (68) và `imagesOverTenPercentIgnore` (`VID2_0611_00014.0.png`). Ngưỡng cảnh báo của app là **10% toàn ảnh**, không phải 1% trong Road Extent, nên không thay được `check_labels.py`.

### 2.6. Kết quả chạy `tools/` trên bản xuất thử

`convert_labelflow.py --videos videos.csv`: chuyển 5 ảnh (train), bỏ 68 ảnh chưa gán, không có lỗi chặn, bám khung cả 5 ảnh. `check_labels.py`: 0 đạt, 5 cảnh báo, 0 chặn (`bao_cao_kiem_tra_labelflow_anh/qc_report_5_anh_thu.csv`):

| Ảnh     | 255 trong Road Extent | Lệch mép > 3 px (khoảng cách vuông góc) | Hàng có khe 255 ở mép | Ghi chú               |
| ------- | --------------------- | --------------------------------------- | --------------------- | --------------------- |
| 00011.0 | 0,86%                 | 75% số hàng, trung vị 6 px              | 143                   |                       |
| 00013.0 | 0,95%                 | 90%, trung vị 10 px                     | 110                   |                       |
| 00014.0 | **2,47%**             | 57%, trung vị 4 px                      | 54                    | vùng 255 quanh xe máy |
| 00014.5 | **1,15%**             | 24%, trung vị 1 px                      | 141                   | vùng xa gần chân trời |
| 00015.0 | 0,57%                 | 38%, trung vị 3 px                      | 257                   |                       |

Nguyên nhân lệch mép (xem ảnh trong `bao_cao_kiem_tra_labelflow_anh/`): cạnh Road Extent là đường thẳng nối vài đỉnh, còn biên Vỉa hè tô tay thì răng cưa, nên hai bên lệch nhau 5–10 px vuông góc. Muốn đạt ±3 px thì phải vẽ đa giác **bám theo biên Mặt đường** với nhiều đỉnh hơn ở đoạn mép nhìn thấy, hoặc sửa biên Mặt đường cho thẳng theo cạnh đa giác.

---

## Việc 5 — Đối chiếu ảnh `VID4_0611_00232.0.png`

**Chưa thực hiện được:** trong DB (bản lưu cuối 17/11/2026 08:51) ảnh này ở trạng thái `unlabeled`, không có mặt nạ semantic và không có Road Extent. Không có thư mục xuất nào trên máy chứa ảnh này.

Ảnh này thuộc VID4_0611 = **tập test** trong `E:\vnroad\manifest\videos.csv`. Dùng để thử công cụ thì được, nhưng không đưa vào huấn luyện.

Khi đã gán xong, chạy:

```
python tools\convert_labelflow.py <thư mục xuất> --out <ra> --videos E:\vnroad\manifest\videos.csv
python tools\check_labels.py <ra>
python tools\overlay.py <ra> --anh VID4_0611_00232.0.png --qc <ra>\qc_report.csv
```

rồi đối chiếu ảnh overlay theo bảng dưới. Cột "Cách kiểm" cho biết dấu hiệu cần tìm trên overlay hoặc số liệu.

| Vùng                                                                                     | Lớp kỳ vọng                                                                                                                                    | Cách kiểm                                                                                                                                                                                                               |
| ---------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Mặt nhựa, vạch vàng đứt, vạch trắng hai bên, lá vụn và cành nhỏ nằm dẹt                  | Mặt đường (0)                                                                                                                                  | Toàn bộ mặt nhựa màu tím; vạch trắng bên trái **không** được tô Vỉa hè                                                                                                                                                  |
| Đống cành cây bên trái                                                                   | Khác (10), nằm trong Road Extent                                                                                                               | Màu xanh lá, nằm **trong** cạnh xanh lơ; đoạn cạnh trái sau đống cành có màu vàng (bị che)                                                                                                                              |
| Dải bê tông sát nhà, song chắn rác, bậc, dốc trước cửa, thảm xanh nằm phẳng, vỉa hè phải | Vỉa hè / lề (1)                                                                                                                                | Màu hồng nhạt; thảm xanh không được tô Khác                                                                                                                                                                             |
| Cửa cuốn, cổng, tường, biển gắn vào nhà                                                  | Nhà / tường (2)                                                                                                                                | Màu xám đậm                                                                                                                                                                                                             |
| Xe máy đỗ hai bên; người đi xe đạp (tô chung người và xe)                                | Xe máy, xe đạp (4)                                                                                                                             | Màu xanh dương; người đạp xe không tô Người (8)                                                                                                                                                                         |
| Xe con trắng cuối đường                                                                  | Ô tô (5)                                                                                                                                       |                                                                                                                                                                                                                         |
| Trời, cây, cờ, khung sắt                                                                 | Khác (10)                                                                                                                                      | Nếu để trống thì thành 255 (không gây hại cho lòng đường nhưng không đúng Hướng dẫn v3 §7.2)                                                                                                                            |
| Vùng mờ taplo, kính góc dưới phải                                                        | Bỏ qua (255)                                                                                                                                   | Nếu dùng Ignore template: góc dưới phải trong suốt trên overlay. Vùng này phần lớn nằm trong lòng đường, nên **sẽ làm tăng "255 trong Road Extent"**: phải loại vùng camera khỏi Road Extent (`--ignore-dir`, xem dưới) |
| Road Extent                                                                              | Mép trái theo mép mặt nhựa giáp dải bê tông (không theo vạch trắng); mép phải theo vỉa hè; xuyên qua đống cành; kéo tới chỗ còn thấy mép đường | `check_labels`: một vùng; Mặt đường trong Road Extent ≥ 95%; lệch mép ≤ 3 px; không cảnh báo dải Khác dọc mép (nếu dải bê tông bị tô Khác sẽ báo)                                                                       |
| Vùng trống cố ý trong lòng đường                                                         | 255                                                                                                                                            | Hiện màu hồng đậm ở khung dưới; nếu > 1% Road Extent thì `check_labels` báo cảnh báo kèm tọa độ                                                                                                                         |

Lưu ý về taplo: Ignore template của LabelFlow chỉ ghi 255 vào **semantic**, Road Extent vẫn là 1 ở đó. Khi đó `check_labels` sẽ đếm vùng taplo là "255 trong Road Extent". Cách xử lý: vẽ thêm mặt nạ camera `E:\vnroad\ignore\VID4_0611.png` (trắng = taplo/kính) và chạy convert với `--ignore-dir E:\vnroad\ignore`; script đặt 255 cho cả `masks/` và `road/` ở vùng đó, nên vùng này không còn nằm trong Road Extent.
