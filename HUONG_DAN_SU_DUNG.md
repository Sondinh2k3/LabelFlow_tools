# Hướng dẫn sử dụng LabelFlow

Tài liệu cho người gán nhãn, người duyệt và người chuẩn bị dữ liệu. Giao diện LabelFlow là tiếng Anh; tên nút trong tài liệu viết **in đậm**, đúng như trên màn hình.

Gán nhãn cho dự án dị vật lòng đường (11 lớp + đa giác lòng đường) có hướng dẫn riêng chi tiết hơn: [docs/vnroad/huong_dan_thao_tac_labelflow.md](docs/vnroad/huong_dan_thao_tac_labelflow.md).

## Mục lục

1. [Cài đặt và chạy](#1-cài-đặt-và-chạy)
2. [Tạo project và nhập ảnh](#2-tạo-project-và-nhập-ảnh)
3. [Màn hình gán nhãn và phím tắt](#3-màn-hình-gán-nhãn-và-phím-tắt)
4. [Các loại nhãn và công cụ vẽ](#4-các-loại-nhãn-và-công-cụ-vẽ)
5. [SAM: click ra mặt nạ](#5-sam-click-ra-mặt-nạ)
6. [Nhãn sơ bộ, Ignore template, nhập nhãn có sẵn](#6-nhãn-sơ-bộ-ignore-template-nhập-nhãn-có-sẵn)
7. [Duyệt nhãn và thảo luận nhóm](#7-duyệt-nhãn-và-thảo-luận-nhóm)
8. [Xuất dataset](#8-xuất-dataset)
9. [Video](#9-video)
10. [AI gợi ý (tùy chọn)](#10-ai-gợi-ý-tùy-chọn)
11. [Công cụ dataset nâng cao](#11-công-cụ-dataset-nâng-cao)
12. [Sao lưu, dung lượng, giới hạn](#12-sao-lưu-dung-lượng-giới-hạn)

---

## 1. Cài đặt và chạy

1. Giải nén `LabelFlow-windows-x64.zip` (bản portable), chạy `LabelFlow.exe`. Không cần Node, Rust, server hay tài khoản; chạy offline.
2. Máy cần **WebView2 runtime** (Windows 10/11 thường có sẵn).
3. Project phải nằm trên **ổ cứng của máy**, không đặt trên ổ mạng.

Thành phần tùy chọn, cài riêng khi cần: Python + MobileSAM cho SAM (mục 5), FFmpeg/FFprobe cho video (mục 9), Python + mô hình ONNX cho AI gợi ý (mục 10).

Lần mở sau, LabelFlow tự mở project và ảnh gần nhất. `LabelFlow.exe --home` bỏ qua việc tự mở; `LabelFlow.exe --project <thư mục>` mở project chỉ định.

## 2. Tạo project và nhập ảnh

### 2.1. Tạo project

1. Màn hình Home → chọn **Project profile** (ví dụ **Road Encroachment**: có sẵn layer Semantic, Road Extent, Traffic exclusion, Suspected region, Candidate objects).
2. Đặt tên.
3. **Semantic classes**: **Cityscapes (19 train IDs)** (mặc định) hoặc **Custom list**: mỗi dòng một lớp, ID theo thứ tự (dòng đầu = 0), có thể thêm màu, ví dụ `mat_duong #804080`. Lớp `255 · Ignore / uncertain` luôn có sẵn. Ứng dụng nhớ danh sách gần nhất; cả nhóm nên dùng **cùng một danh sách** để ID khớp nhau.
4. **Create project** → chọn thư mục có quyền ghi và chưa chứa `.labelflow`.

Mở project cũ: schema được sao lưu rồi nâng lên bản mới nhất; profile và ID ảnh không đổi.

**Sửa bộ lớp sau khi tạo:** chọn layer → **Edit classes** để thêm, đổi tên, đổi màu. Lớp chỉ xóa được khi chưa nhãn nào dùng. ID không bao giờ bị đánh lại; lớp mới xuất hiện ngay trong file xuất và phím tắt. Không đổi thứ tự lớp sau khi đã bắt đầu gán.

### 2.2. Nhập ảnh

- **Import images**, **Import folder** hoặc kéo thả ảnh/thư mục. Mặc định **liên kết** tới ảnh gốc (linked); bật **Copy into project** để chép ảnh vào project. Ảnh nguồn không bao giờ bị sửa.
- Có thanh tiến trình, nút hủy và báo cáo ảnh lỗi, ảnh trùng.
- **Nhóm chia tập** (Dataset split group) mặc định là **thư mục chứa ảnh**. Muốn chia tập theo video/chuyến đi thì đặt ảnh mỗi video vào một thư mục con riêng trước khi nhập, hoặc sửa ô **Dataset split group** ở panel phải.
- Ảnh nguồn bị di chuyển: chọn ảnh → **Repair source** → chọn đúng file gốc (phải trùng SHA-256, kích thước, chiều xoay). Nhãn được giữ nguyên.

## 3. Màn hình gán nhãn và phím tắt

| Khu vực    | Có gì                                                                                                                                                                                                 |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Panel trái | Danh sách ảnh, tìm kiếm, **Review filter**, lọc theo nhóm                                                                                                                                             |
| Giữa       | Ảnh và lớp phủ nhãn                                                                                                                                                                                   |
| Panel phải | **LAYERS** (chọn layer, bật/tắt, **Overlay opacity**), **CLASSES**, **Click-to-mask (SAM)**, **ANNOTATIONS**, **Review state**, **Notes**, **Dataset split group**, **Ask the team about this image** |

Kéo vạch phân cách để đổi độ rộng panel; `Tab` ẩn/hiện panel.

| Phím                         | Tác dụng                                                                                             |
| ---------------------------- | ---------------------------------------------------------------------------------------------------- |
| `1`–`9`                      | Chọn lớp thứ 1–9 (ID 0–8)                                                                            |
| `Alt+1`–`Alt+9`              | Chọn lớp thứ 10–18                                                                                   |
| `C`                          | Tìm lớp theo tên                                                                                     |
| `V`                          | Chọn, di chuyển, kéo đỉnh/handle để sửa hình                                                         |
| `B`                          | Hộp bao (bbox)                                                                                       |
| `P`                          | Đa giác: click từng đỉnh; Enter hoặc click lại đỉnh đầu để đóng; Backspace xóa đỉnh vừa đặt; Esc hủy |
| `M` / `E`                    | Cọ / tẩy; `[` `]` thu nhỏ / phóng to cọ                                                              |
| `S`                          | SAM (mục 5)                                                                                          |
| `H`, Space + kéo, chuột giữa | Di chuyển ảnh                                                                                        |
| Lăn chuột / `0` / `=`        | Zoom tại con trỏ / vừa màn hình / 100%                                                               |
| `L`                          | Ẩn/hiện lớp phủ                                                                                      |
| `←` / `→`                    | Ảnh trước / sau                                                                                      |
| `Ctrl+Z` / `Ctrl+Shift+Z`    | Hoàn tác / làm lại (trong phiên sửa ảnh hiện tại)                                                    |
| `Ctrl+Enter`                 | **Done → next**: đánh dấu `needs_review`, sang ảnh tiếp                                              |
| `Ctrl+Shift+Enter`           | **Approve → next**: duyệt, sang ảnh tiếp                                                             |
| `Ctrl+E`                     | **Export dataset**                                                                                   |
| `?`                          | Bảng phím tắt đầy đủ                                                                                 |

**Lưu tự động:** chờ chữ **Saved**. Nếu lưu lỗi (**Failed — retry**), ảnh và bản nháp được giữ, chuyển ảnh bị chặn; khắc phục nguyên nhân rồi bấm **Retry save**.

## 4. Các loại nhãn và công cụ vẽ

| Loại layer                                       | Ví dụ                                            | Công cụ                                  | Giá trị khi xuất                      |
| ------------------------------------------------ | ------------------------------------------------ | ---------------------------------------- | ------------------------------------- |
| **Semantic** (mỗi pixel một lớp)                 | Cityscapes semantic                              | Cọ `M`, tẩy `E`, đa giác tô `P`, SAM `S` | PNG class-ID; **pixel chưa tô = 255** |
| **Region** (vùng nhị phân, độc lập với semantic) | Road Extent, Traffic exclusion, Suspected region | Đa giác `P`, cọ, tẩy                     | PNG 0/1                               |
| **Object** (đối tượng)                           | Candidate objects                                | Bbox `B`, đa giác `P`                    | YOLO / COCO                           |

Lưu ý khi tô semantic:

- **Tô sau ghi đè tô trước** (cọ, đa giác tô, SAM _Apply mask_). Vẽ từ xa đến gần: nền trước, vật phía trước sau. Muốn đổi lớp một vùng thì tô đè bằng lớp đúng.
- **Tẩy trả pixel về 255** (chưa tô / Ignore). Chọn lớp **Ignore** để chủ động tô vùng 255.
- Đa giác tô trên layer Semantic tô kín mọi thứ bên trong (một bước undo).
- Click đỉnh **hơi ra ngoài ảnh** (trong khoảng 12 px) thì đỉnh tự dính đúng mép ảnh.

## 5. SAM: click ra mặt nạ

### 5.1. Cài một lần mỗi máy

SAM chạy trong một tiến trình Python riêng trên CPU. Hỗ trợ ONNX của SAM / MobileSAM / SAM-HQ theo chữ ký decoder chính thức của segment-anything (chưa hỗ trợ SAM 2). MobileSAM nhẹ và nhanh nhất trên CPU.

PowerShell tại thư mục chứa `LabelFlow.exe` (bản portable có sẵn `python-requirements.txt` và `make-sam-manifest.py`):

```powershell
python -m venv ai-venv
ai-venv\Scripts\python.exe -m pip install -r python-requirements.txt

New-Item -ItemType Directory -Force E:\models\mobile_sam
curl.exe -L -o E:\models\mobile_sam\mobile_sam.encoder.onnx https://huggingface.co/nrl-ai/samexporter-onnx-models/resolve/main/mobile_sam/mobile_sam.encoder.onnx
curl.exe -L -o E:\models\mobile_sam\mobile_sam.decoder.onnx https://huggingface.co/nrl-ai/samexporter-onnx-models/resolve/main/mobile_sam/mobile_sam.decoder.onnx
ai-venv\Scripts\python.exe make-sam-manifest.py E:\models\mobile_sam --name "MobileSAM" --license "Apache-2.0"
```

Từ mã nguồn: dùng `worker/requirements.txt` và `scripts/make-sam-manifest.py`. Mô hình khác (SAM ViT-B, SAM-HQ) tự xuất bằng `samexporter` (encoder `--use-preprocess`, decoder `--return-single-mask`) rồi tạo manifest như trên. Kiểm tra giấy phép của trọng số bạn dùng.

### 5.2. Dùng

1. Panel phải → **Click-to-mask (SAM)** → **Choose SAM model…** → chọn `python.exe` của môi trường trên (lần đầu), rồi `sam.json` → SAM khởi động, chờ **Ready**.
2. Chọn lớp, bấm `S`. **Kéo khung** quanh vật, hoặc click trái vào vật; click trái thêm phần thiếu, chuột phải hoặc Alt+click bỏ phần thừa.
3. **Enter** = ghi đè bằng lớp đang chọn; **Shift+Enter** = chỉ ghi vào pixel chưa tô; Esc = hủy.

Trên CPU: nạp mô hình khoảng 4–5 s, mỗi ảnh mới khoảng 1–1,5 s (tự chạy khi chọn công cụ, nhớ 4 ảnh gần nhất), mỗi click khoảng 0,1 s.

## 6. Nhãn sơ bộ, Ignore template, nhập nhãn có sẵn

### 6.1. Nhập mặt nạ hàng loạt (Import mask folder)

Chạy mô hình lớn ngoài app, lưu mỗi ảnh một PNG class-ID **trùng tên ảnh** (hoặc theo media ID). Trong LabelFlow:

1. Chọn layer cần nạp (Semantic hoặc một layer region).
2. **Import mask folder** → chọn thư mục.
3. **Confirm class mapping**: giá trị trong PNG → lớp project (ID trùng nhau được điền sẵn).
4. **Merge with existing labels**: **Fill only unlabeled pixels (keep human labels)** (mặc định) hoặc **Replace the layer**.
5. **Import masks**, đọc báo cáo: số ảnh khớp, file bị bỏ (thường do khác tên hoặc kích thước).

Ảnh đã **approved** không bao giờ bị đổi.

### 6.2. Ignore template

Một PNG trắng/đen đúng kích thước ảnh, **trắng = vùng Ignore cố định** (mui xe, taplo, khung kính). Thanh công cụ editor → **Ignore template** → chọn PNG → xác nhận. Áp cho **mọi ảnh chưa duyệt có cùng kích thước** trong project và đã có mặt nạ semantic; vì vậy mỗi camera nên một project. Làm sau khi nạp nhãn sơ bộ.

### 6.3. Nhập nhãn từ định dạng khác

- **Import class-ID PNG** (một ảnh), **Import YOLO labels**, **Import COCO**, **Import YOLO segmentation**: luôn yêu cầu ánh xạ lớp tường minh; chỉ nhận PNG 8-bit grayscale/indexed (không suy diễn màu RGB).
- COCO hỗ trợ bbox, polygon (nhiều phần, có lỗ), RLE nén/không nén. Ghép ảnh theo media UUID hoặc tên file duy nhất; từ chối tên mơ hồ và sai kích thước. Nhập thêm không thay nhãn cũ; nhập lặp cùng nội dung không tạo bản trùng.
- Đổi trạng thái review khỏi `approved` trước khi nhập đè nhãn.

## 7. Duyệt nhãn và thảo luận nhóm

### 7.1. Trạng thái và duyệt

Trạng thái mỗi ảnh: `unlabeled` → `in_progress` → `needs_review` → `approved` hoặc `rejected`.

1. Người gán xong ảnh: `Ctrl+Enter` (**Done → next**).
2. Người duyệt: **Review filter** → `needs_review`; đạt → `Ctrl+Shift+Enter` (**Approve → next**); không đạt → **Review state** = `rejected`, ghi lý do vào **Notes**.
3. Ảnh `approved` được bảo vệ: nhãn sơ bộ, Ignore template, AI gợi ý không đè lên.

LabelFlow không cho hai người cùng ghi một project: mỗi người gán một project, người duyệt mở project đó (hoặc nhận **Backup DB + masks** rồi **Restore backup**).

### 7.2. Thảo luận nhóm (không cần server)

1. Cả nhóm dùng chung một thư mục: ổ mạng LAN hoặc thư mục OneDrive / Google Drive / Dropbox / Syncthing.
2. Trong project: **Team discussion** → nhập tên hiển thị → **Choose shared folder** (mỗi máy chọn đường dẫn của mình tới cùng thư mục).
3. Gặp ca khó: panel phải → **Ask the team about this image** → viết câu hỏi → tùy chọn **Pin a spot** (click lên ảnh), **Share my current labels** → **Post question**.
4. Đồng nghiệp thấy badge "N new" trên thanh công cụ và chấm cam trên ảnh thu nhỏ; xem câu hỏi kèm nhãn và điểm ghim, trả lời, **Mark resolved** / **Reopen**, hoặc **Open image in editor** nếu project của họ có cùng ảnh (khớp theo SHA-256).

Mỗi người chỉ ghi file của riêng mình nên công cụ đồng bộ không xung đột. Thảo luận không tự sửa nhãn.

## 8. Xuất dataset

`Ctrl+E` hoặc **Export dataset**:

| Tùy chọn                            | Ý nghĩa                                                                                                                                                       |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Export format**                   | **YOLO detection**, **Semantic + region PNGs**, **YOLO detection + semantic/region PNGs**, YOLO segmentation, COCO                                            |
| **Approved images only**            | Chỉ xuất ảnh đã duyệt                                                                                                                                         |
| Seed, **Train %**, **Validation %** | Chia tập theo nhóm (Dataset split group): mọi ảnh cùng nhóm vào cùng một tập; tập nào nhận nhóm nào do băm (seed, nhóm) quyết định, nên tỷ lệ chỉ là mục tiêu |

Mỗi lần xuất tạo một thư mục mới `LabelFlow-export-<uuid>`:

```
manifest.json          danh sách ảnh (id, tên gốc, SHA-256, tập, nhóm, trạng thái, notes), cảnh báo, checksum
classes.json           bộ lớp và layer
semantic-stats.json    số pixel, số ảnh mỗi lớp; ảnh chưa có mặt nạ; ảnh có hơn 10% Ignore
images/<tập>/<id>.png
masks/<layer>/<tập>/<id>.png      semantic: class-ID, 255 = Ignore/chưa tô; region: 0/1
labels/<tập>/<id>.txt, data.yaml  (định dạng YOLO)
```

Tên file là UUID của ảnh trong LabelFlow; tên gốc nằm ở `manifest.json → images[].source`. Đọc kỹ cảnh báo trong báo cáo xuất (lớp không có pixel, ảnh chưa có mặt nạ, tập rỗng khi có ít nhóm). **Preview grouped splits** xem trước số ảnh mỗi tập; **Validate source pixels and annotations** kiểm tra sâu.

## 9. Video

1. Cài FFmpeg/FFprobe riêng (ứng dụng không tự tải). **Video workspace** → dùng PATH hoặc **Locate FFmpeg tools** chọn thư mục có cả `ffmpeg.exe` và `ffprobe.exe`.
2. **Import video** (MP4/MOV/MKV/WebM/AVI), chờ lập chỉ mục PTS.
3. **Show exact frame**, **Previous/Next frame**, thanh thời gian hoặc **Seek exact frame** để chọn khung chính xác; trích khung đang chọn (**Extract selected frame**) hoặc theo khoảng In/Out/Interval, PNG hoặc JPEG. Nhập lại cùng video/khung giữ nguyên ID và nhãn.
4. **Video / trip split group** → **Save group for all video frames**: mọi khung của video luôn cùng một tập khi xuất.
5. **Repair video source** định vị lại video gốc theo SHA-256.

Khung trích ra là khung nguyên (không cắt). Giới hạn: tối đa 100.000 khung/video, 10.000 khung/lần trích, 64 GiB/video; khung ở xa đầu clip có thể trích chậm.

## 10. AI gợi ý (tùy chọn)

Không có Python/mô hình vẫn dùng đầy đủ chức năng gán nhãn và xuất. Cài môi trường và manifest mô hình: [worker/README.md](worker/README.md).

1. Chọn một ảnh → **AI assistance** → **Choose Python & model manifest** → **Start / restart worker** (kiểm tra SHA, bảng lớp, input/output và chạy thử).
2. **Generate suggestion**, bật/tắt lớp phủ gợi ý, xem model/version/license/confidence.
3. **Accept suggestion**: thêm hộp mới hoặc chỉ điền semantic vào pixel 255; giữ nhãn thủ công; ảnh chuyển `needs_review`. **Reject suggestion** chỉ đổi trạng thái gợi ý.
4. **Cancel AI request**, **Stop / unload worker** để giải phóng mô hình.

Hỗ trợ ONNX CPU: semantic (ví dụ adapter `pidnet_semantic`: số kênh đầu ra = số lớp semantic của project, theo thứ tự ID) hoặc detection đã NMS với schema tường minh. Ứng dụng không tự tải trọng số.

## 11. Công cụ dataset nâng cao

| Chức năng             | Ở đâu                                     | Dùng để                                                                                                                                                                   |
| --------------------- | ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Road review           | **Road / dataset review**                 | So dự đoán với nhãn (Ground truth / Prediction / Both / Disagreement, IoU), **Import prediction JSON**, accept/reject dự đoán, hàng đợi review, **Export review metrics** |
| Trips, tags           | **Trips**, **Tags & capture**             | Gán chuyến đi, camera, thời gian, GPS, tag (hard negative, thời tiết, ánh sáng, che khuất)                                                                                |
| Quality dashboard     | **Dataset tools → Quality dashboard**     | Thống kê review, event, tag, nhóm, độ phủ chỉ mục                                                                                                                         |
| Similar images        | **Dataset tools → Similar images**        | Lập chỉ mục dấu vân ảnh, xếp hạng ảnh gần trùng (trong nhóm hoặc toàn project); không tự xóa                                                                              |
| Video propagation     | **Dataset tools → Video propagation**     | Từ một khung đã duyệt, gợi ý nhãn cho tối đa 10 khung lân cận (±2 s) theo dịch chuyển toàn cục; phải xem và **Mark visually verified** trước khi accept                   |
| Dataset versions      | **Dataset tools → Dataset versions**      | Tạo phiên bản bất biến (seed, tỷ lệ, approved-only), **Verify labels and sources**, **Export frozen dataset**                                                             |
| Storage & performance | **Dataset tools → Storage & performance** | Xem dung lượng, dọn ảnh thu nhỏ/preview có thể tạo lại                                                                                                                    |

Dự đoán nhập vào không bao giờ ghi đè nhãn đã duyệt; các bước accept luôn có kiểm tra revision.

## 12. Sao lưu, dung lượng, giới hạn

- **Backup DB + masks**: sao lưu CSDL, manifest và mặt nạ. **Ảnh/video nguồn phải sao lưu riêng.** **Restore backup** ở Home chỉ ghi vào thư mục chưa có `.labelflow`.
- Dataset versions được sao lưu cùng project nhưng không thay thế backup.
- Giới hạn: ảnh nguồn tối đa 64 MiB, 24 megapixel; project trên ổ cục bộ; ảnh nguồn liên kết phải còn truy cập được để sửa nhãn. File khóa còn lại sau khi đóng app là bình thường.
- Chưa có: giao diện tiếng Việt, installer, SAM 2, gộp nhãn giữa nhiều người, nhãn theo đoạn thời gian của video, huấn luyện mô hình trong app.
