# tools/ — chuyển đổi và kiểm tra nhãn LabelFlow (dự án dị vật lòng đường)

Theo `Huong_dan_loc_va_gan_nhan_du_lieu_v3.md` bản 3.1. Thao tác trong app: `huong_dan_thao_tac_labelflow.md`.

| File                      | Việc                                                                                                     |
| ------------------------- | -------------------------------------------------------------------------------------------------------- |
| `lop.yaml`                | Nguồn duy nhất: ID 11 lớp + 255, nhóm được miễn, màu, gộp 7 lớp, cách đọc file xuất LabelFlow, ngưỡng QC |
| `convert_labelflow.py`    | Thư mục xuất LabelFlow → `images/ masks/ road/ edge/ list/`                                              |
| `check_labels.py`         | Kiểm tra nhãn → `qc_report.csv` (mã thoát 1 nếu có lỗi chặn)                                             |
| `overlay.py`              | Ảnh chồng nhãn để duyệt nhanh (vùng 255 trong lòng đường tô hồng đậm)                                    |
| `agreement.py`            | Độ trùng khớp giữa hai người gán (hiệu chuẩn, gán đôi)                                                   |
| `test_events.py`          | Điền `hop_bao`, `muc_chiem`… cho `test_events.csv` của video test                                        |
| `prelabel_mask2former.py` | Nhãn sơ bộ vòng 1 bằng Mask2Former Vistas, chạy ngoài app (máy GPU)                                      |
| `lf_common.py`            | Hàm dùng chung (không chạy trực tiếp)                                                                    |
| `requirements.txt`        | Thư viện Python                                                                                          |

## Cài đặt (một lần)

```
python -m venv .venv
.venv\Scripts\python -m pip install -r tools\requirements.txt
```

`prelabel_mask2former.py` cần thêm `torch transformers` và chạy trên máy có GPU.

## Quy trình

```
:: 1. Trong LabelFlow: Export dataset -> "Semantic + region PNGs", "Approved images only",
::    Train 100 %, Val 0 % (chia tập để script làm theo videos.csv). Ra thư mục LabelFlow-export-<uuid>.

:: 2. Chuyển đổi: truyền MỌI thư mục xuất (mỗi người gán một) trong cùng một lệnh
python tools\convert_labelflow.py E:\vnroad\labelflow_export\LabelFlow-export-xxxx [...] --out E:\vnroad\data ^
       --videos E:\vnroad\manifest\videos.csv --chi-duyet --ignore-dir E:\vnroad\ignore

:: 3. Kiểm tra
python tools\check_labels.py E:\vnroad\data

:: 4. Xem ảnh có cảnh báo
python tools\overlay.py E:\vnroad\data --qc E:\vnroad\data\qc_report.csv --chi-loi
```

Ảnh `chan` (lỗi chặn) và `canh_bao` đều trả người gán sửa trên LabelFlow, rồi xuất lại và chạy lại từ bước 2.
`--out` nên là thư mục riêng cho kết quả script: mỗi lần chạy, file của ảnh không còn trong lần xuất này bị xóa khỏi `images/ masks/ road/ edge/`, và `overlay.py` xóa JPG cũ trong `overlay/` (trừ khi có `--out` hoặc `--anh`).

Ba script trên chạy song song nhiều tiến trình khi có từ 8 ảnh trở lên (mặc định tối đa 8; `-j 1` để chạy tuần tự).

## Quy ước đầu ra

| Thư mục                                                    | Giá trị                                                                                                                                                                                                                         |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `masks/` (file 2)                                          | 0–10 theo `lop.yaml`; **255 = Bỏ qua**, gồm cả pixel chưa tô và vùng Ignore template. Lớp cũ `bo_qua` (ID 11) cũng thành 255                                                                                                    |
| `road/` (file 1)                                           | 1 = trong Road Extent, 0 = ngoài, 255 = mặt nạ camera trong `--ignore-dir`. Vùng dừng cách khung ảnh ≤ 8 px được kéo ra sát khung (`labelflow.bam_khung_px`)                                                                    |
| `edge/`                                                    | 0 không phải cạnh; 1 cạnh nhìn thấy; 2 **cạnh bị che** (phía trong là xe, người, động vật, Khác); 3 cạnh không xác định (phía trong là Vỉa hè, Nhà, Kết cấu hoặc 255, thường là đa giác lệch mép). Bỏ phần cạnh trùng khung ảnh |
| `list/*.lst`                                               | `images/X.png masks/X.png road/X.png`; ảnh tập test vào `test.lst`                                                                                                                                                              |
| `lop.yaml`, `convert_log.csv`, `qc_report.csv`, `overlay/` | Bản sao cấu hình; ảnh nào chuyển/bỏ/lỗi; kết quả kiểm tra; ảnh chồng nhãn                                                                                                                                                       |

- Tên file là tên ảnh gốc (lấy từ `manifest.json → images[].source`), không phải UUID của LabelFlow.
- Video của ảnh suy từ tên: `VID4_0611_00232.0.png` → `VID4_0611`.
- Chia tập: có `--videos` thì theo cột `tap`; không có thì theo `split` trong `manifest.json`. Một video nằm ở hai tập thì dừng và báo rò rỉ.
- Bỏ ảnh `unlabeled`, `rejected` và ảnh chưa có mặt nạ semantic (LabelFlow xuất toàn 255); `--chi-duyet` chỉ lấy `approved`. Ảnh trùng tên giữa các thư mục xuất: giữ bản trong thư mục đứng trước.
- `--ignore-dir`: mặt nạ cố định theo camera, tên `<video_id>.png` hoặc theo cột `mat_na_bo_qua` của `videos.csv`, trắng = bỏ qua; áp lên cả `masks/` và `road/`.
- Ảnh ra (`images/`) là liên kết cứng tới ảnh trong thư mục xuất nếu cùng ổ đĩa (nhanh, không tốn chỗ), khác ổ thì chép.
- Muốn quay lại quy ước cũ "chưa tô = Khác": đặt `labelflow.chua_to: 10` trong `lop.yaml`.

## Các kiểm tra trong `check_labels.py`

| Kiểm tra                                                                                                | Mức      | Ngưỡng (`lop.yaml → nguong_qc`)                           |
| ------------------------------------------------------------------------------------------------------- | -------- | --------------------------------------------------------- |
| masks chỉ có 0–10, 255; road chỉ có 0, 1, 255                                                           | chặn     | —                                                         |
| Đúng một vùng Road Extent; có pixel Mặt đường                                                           | chặn     | —                                                         |
| Ảnh và hai nhãn cùng kích thước                                                                         | chặn     | —                                                         |
| 255 nằm trong Road Extent (quên tô vật, khe hở), không tính dải 8 px sát khung ảnh                      | cảnh báo | > 1% diện tích Road Extent; in vị trí vùng lớn nhất       |
| Mặt đường nằm trong Road Extent                                                                         | cảnh báo | < 95%                                                     |
| Khớp mép: khoảng cách **vuông góc** từ biên Mặt đường/Vỉa hè (hoặc /Nhà) nhìn thấy tới cạnh Road Extent | cảnh báo | > 3 px ở > 10% số hàng                                    |
| Mép có khe 255 rộng hơn 3 px giữa Mặt đường và Vỉa hè/Nhà (không đo được khớp mép)                      | cảnh báo | > 10% số hàng                                             |
| Dải Khác mảnh sát cạnh Road Extent, chạy dọc mép                                                        | cảnh báo | dài > 1/3 chiều cao vùng đường, rộng < 5% bề rộng đa giác |
| Số ảnh có mặt của mỗi lớp được miễn (và Khác)                                                           | thống kê | < 30 ảnh                                                  |

Đo khớp mép theo phương ngang sẽ phóng đại lệch 3–4 lần ở cạnh xiên thoải (cạnh trái của đường nhìn từ camera), nên script dùng khoảng cách vuông góc tới cạnh đa giác.

## Công cụ khác

```
:: Độ đồng thuận: ghép ảnh theo SHA-256; Semantic chỉ tính pixel cả hai bên đã tô; Road Extent IoU
python tools\agreement.py <xuất_A> <xuất_B> --min 0.95

:: Nhãn sự kiện video test (xuất "YOLO detection + semantic/region PNGs"; obj_id trong Notes, trái sang phải)
python tools\test_events.py <xuất_test> E:\vnroad\manifest\test_events.csv

:: Nhãn sơ bộ vòng 1
python tools\prelabel_mask2former.py --tu-kiem                 :: kiểm tra bảng ánh xạ, không cần mô hình
python tools\prelabel_mask2former.py anh\ ra\ --xem-anh-xa     :: in ánh xạ theo config.json của mô hình
python tools\prelabel_mask2former.py anh\ ra\ --fp16
```

`prelabel_mask2former.py` ra một PNG một kênh cho mỗi ảnh, **trùng tên ảnh**, giá trị 0–10 và 255 (Ego Vehicle, Car Mount, Unlabeled). Trong LabelFlow: layer **Semantic** → **Import mask folder** → để nguyên ánh xạ → **Replace the layer** (mặc định của app là _Fill only unlabeled pixels_) → Import masks.

## Đã kiểm thử (18/11/2026)

- Bản xuất thật của project `test_vong_1` (bản sao, 73 ảnh, 5 ảnh đã gán), có và không có `--videos`.
- Dữ liệu tổng hợp: khe 255 trong lòng đường, dải Khác dọc mép, hai vùng Road Extent, giá trị lạ (13), lớp thừa trong `classes.json`, `bo_qua` 11 → 255, rò rỉ giữa tập, cạnh bị che.
- Bản tối ưu cho kết quả giống hệt bản trước trên 76 ca (ảnh thật, tổng hợp, ngẫu nhiên), trừ phần bám khung là thay đổi có chủ ý. Trên 120 ảnh 1920×1080 (16 nhân): convert 23 s → 5,5 s, check 76 s → 8 s, overlay 139 s → 12 s.
- `test_events.py` cho kết quả giống script cũ trên dữ liệu giả lập; `agreement.py` cho 100% khi so một bản xuất với chính nó.
- `prelabel_mask2former.py --tu-kiem` đạt; chưa chạy với mô hình thật (máy này không có GPU/mô hình).
