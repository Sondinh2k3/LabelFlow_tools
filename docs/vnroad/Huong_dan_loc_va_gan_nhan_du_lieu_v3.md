# Hướng dẫn lọc và gán nhãn dữ liệu — phát hiện dị vật trên lòng đường

| Mục                 | Nội dung                                                                                                                                                                                                                 |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Phiên bản           | 3.1                                                                                                                                                                                                                      |
| Ngày                | 18/11/2026                                                                                                                                                                                                               |
| Thay đổi so với 3.0 | Công cụ gán CVAT → **LabelFlow** (mục 8); **chỗ chưa tô = Bỏ qua (255)** thay cho Khác (mục 8.5); script trong `tools/` (mục 8.5, 10.3); kiểm tra mới: Bỏ qua trong lòng đường, mép có khe (mục 10.3)                    |
| Phạm vi             | Lọc video, chọn ảnh, gán nhãn ảnh huấn luyện, gán nhãn video đánh giá, kiểm soát chất lượng                                                                                                                              |
| Người dùng          | Người gán nhãn, người duyệt, kỹ sư ML                                                                                                                                                                                    |
| Tài liệu liên quan  | Tài liệu kỹ thuật thuật toán phát hiện nghi vấn lấn chiếm lòng đường; Báo cáo 02 — Định nghĩa và tình huống lấn chiếm lòng đường; `huong_dan_thao_tac_labelflow.md` (bấm gì trong LabelFlow); `tools/README.md` (script) |
| Lưu ý               | Các ngưỡng và số lượng ảnh là giá trị khởi đầu; chỉnh lại sau đợt thử 30 ảnh và sau mỗi vòng                                                                                                                             |

---

## 1. Mục tiêu và nguyên tắc chung

### 1.1. Dữ liệu dùng để làm gì

Thuật toán trên AI box hoạt động theo một phép trừ: **lấy vùng lòng đường, trừ đi những gì mô hình chắc chắn thuộc nhóm được miễn (mặt đường, kết cấu cố định, xe các loại, người, động vật); phần còn lại là nghi vấn.** Vùng nghi vấn sau đó được lọc theo kích thước so với bề rộng lòng đường và xác nhận qua nhiều khung.

Mô hình PIDNet-S có hai đầu ra, nên mỗi ảnh cần hai nhãn:

| Đầu ra                | Dùng để làm gì trên box                                                                                                                | Học từ nhãn nào            |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | -------------------------- |
| P — 11 lớp nhìn thấy  | Cộng xác suất 8 lớp được miễn; điểm ảnh có tổng thấp là nghi vấn                                                                       | File 2: mặt nạ 11 lớp      |
| R — lòng đường đầy đủ | Xác định vùng lòng đường, kể cả phần bị xe và vật che; đo bề rộng lòng đường W(y) ở từng hàng ảnh — thước đo cho mọi ngưỡng kích thước | File 1: đa giác lòng đường |

R vừa xác định phạm vi tìm kiếm vừa là thước đo, nên **đa giác lòng đường quan trọng ngang với mặt nạ 11 lớp.**

### 1.2. Hai file nhãn cho mỗi ảnh

|                                  | File 1: đa giác lòng đường                            | File 2: mặt nạ 11 lớp        |
| -------------------------------- | ----------------------------------------------------- | ---------------------------- |
| Trả lời câu hỏi                  | Điểm này có thuộc lòng đường không, kể cả khi bị che? | Ở điểm này nhìn thấy cái gì? |
| Giá trị mỗi điểm ảnh             | Lòng đường / không (+ Bỏ qua)                         | Một trong 11 lớp (+ Bỏ qua)  |
| Cách vẽ                          | Một đa giác 6–12 điểm, vẽ xuyên qua xe và vật         | Tô từng vùng theo lớp        |
| Điểm ảnh trên xe máy             | Lòng đường                                            | Xe máy, xe đạp               |
| Điểm ảnh trên tảng đá giữa đường | Lòng đường                                            | Khác → **nghi vấn**          |
| Điểm ảnh trên vỉa hè             | Không                                                 | Vỉa hè / lề                  |

Một điểm ảnh có thể mang hai giá trị cùng lúc (“lòng đường” và “Xe máy, xe đạp”), nên phải tách thành hai file.

Vùng nghi vấn chuẩn tự suy ra từ hai file: **phần trong đa giác lòng đường có lớp Khác.** Không cần vẽ hộp, không cần gán loại dị vật.

### 1.3. Bộ lớp

| Nhóm                    | Lớp                                                                                                   | Trên box                     |
| ----------------------- | ----------------------------------------------------------------------------------------------------- | ---------------------------- |
| Được miễn (8 lớp)       | Mặt đường; Kết cấu cố định; Xe máy, xe đạp; Ô tô; Xe tải, xe buýt; Xe thô sơ, ba gác; Người; Động vật | Cộng xác suất cả nhóm        |
| Không được miễn (3 lớp) | Vỉa hè / lề; Nhà / tường; Khác                                                                        | Trong lòng đường là nghi vấn |

- **Xe, người, động vật tách theo loại** vì mỗi loại có hình dạng riêng. Gộp chung, mô hình dễ chỉ học nét của loại hay gặp và bỏ qua loại hiếm như xe ba gác, xe đẩy. Box cộng xác suất các lớp được miễn, nên mô hình nhầm giữa hai loại xe **không gây báo nhầm hay bỏ sót.**
- **Khác không tách theo loại** vì lớp này phải bao cả vật chưa từng xuất hiện trong dữ liệu.
- Nhãn chi tiết luôn gộp lại được bằng script (ví dụ thành 7 lớp để so sánh ở vòng 1); nhãn đã gộp thì không tách lại được nếu không gán lại.

### 1.4. Nguyên tắc chung

1. **Trong lòng đường, không chắc là mặt đường thì gán Khác.** Gán nhầm vật thành Mặt đường làm box bỏ sót, server không bao giờ thấy lại. Gán nhầm ngược lại chỉ thêm một báo nhầm cho server lọc.
2. **Nền ven đường gán Vỉa hè/lề hoặc Nhà/tường, không gán Khác.** Rãnh, mương, bậc, lề đất, cỏ, đất đắp là nền. Gán Khác làm mô hình học “cái gì lạ cũng là Khác” và báo nhầm dọc mép.
3. **Ở đoạn mép nhìn thấy, đa giác bám đúng mép mặt đường.** Chỉ vẽ rộng hơn khi mép bị che hoặc quá xa.
4. **Ranh giới Mặt đường với vật nằm trên nó cần chính xác nhất:** lệch tối đa khoảng 3 pixel ở vùng gần camera.
5. **Phía trên đường chân trời chỉ vẽ thô.** Box cắt bỏ phần này.
6. **Không chắc loại xe thì chọn loại gần nhất**, không mất thời gian cân nhắc. Ranh giới giữa xe và nền mới là chỗ cần chính xác.

### 1.5. Những việc không làm

- **Không gán loại dị vật.** Cây đổ, đống cát, nón, rào chắn đều là Khác; loại vật do server xác định. Chỉ xe, người, động vật mới tách theo loại.
- **Không bỏ qua vật nhỏ.** Gán mọi dị vật nhô lên rõ, kể cả vật nhỏ hơn ngưỡng β. β chỉ áp khi chạy thuật toán và khi chấm điểm.
- **Không vẽ hộp bao** cho ảnh train/val. Hộp bao chỉ dùng cho nhãn sự kiện video test (mục 9).
- **Không sửa video gốc** (đổi tên, cắt, nén lại). Mọi thao tác làm trên bản sao.

---

## 2. Quy trình tổng thể

```
 Bước 1  Kiểm kê video, đặt mã, chia tập (khóa tập test)
    │
 Bước 2  Trích khung và lọc ảnh tự động (script)
    │
 Bước 3  Chọn ảnh để gán nhãn ◄──────────────────────────────┐
    │                                                        │
 Bước 4–6  Tiền gán nhãn bằng máy → người sửa trên LabelFlow  │ vòng 2, 3:
    │      (đa giác lòng đường + mặt nạ 11 lớp)               │ chọn ảnh theo
 Bước 8  Kiểm soát chất lượng (gán đôi, script, duyệt)        │ lỗi đo được
    │                                                        │
    ▼                                                        │
 Huấn luyện → chạy thuật toán trên video test ───────────────┘
                     ▲
 Bước 7  Nhãn video test — làm song song, chỉ dùng để đánh giá
```

| Bước                                     | Đầu ra                                   |
| ---------------------------------------- | ---------------------------------------- |
| 1. Kiểm kê video, đặt mã, chia tập       | `videos.csv`, `clips.csv`, danh sách tập |
| 2. Trích khung và lọc tự động            | Kho ảnh ứng viên, `frames.csv`           |
| 3. Chọn ảnh để gán nhãn                  | Danh sách ảnh cần gán                    |
| 4–6. Tiền gán nhãn và sửa trên LabelFlow | Nhãn hai file cho mỗi ảnh                |
| 7. Nhãn video test                       | `test_events.csv`, `test_segments.csv`   |
| 8. Kiểm soát chất lượng                  | `qc_report.csv`, nhãn đã sửa             |

**Ba vòng gán nhãn.** Vòng 1 khoảng 250 ảnh, đồng thời là vòng kiểm chứng phương án (mục 12.1). Vòng 2 và 3 mỗi vòng 400–600 ảnh, chọn theo lỗi của mô hình vòng trước. Tổng 1.500–2.000 ảnh là ước lượng; dừng sớm khi recall sự kiện tăng dưới khoảng 1 điểm sau một vòng.

---

## 3. Bước 1 — Kiểm kê video, đặt mã và chia tập

Chia tập **theo video gốc, không theo clip hay khung**, và khóa tập test trước khi làm việc khác. Hai khung cách nhau vài giây trong cùng video gần như giống hệt; một khung ở train, một khung ở test thì điểm test sẽ cao giả.

### 3.1. Bảng kê video gốc

Mỗi video gốc một dòng trong `videos.csv`:

| Cột                         | Ví dụ                      | Dùng để                                           |
| --------------------------- | -------------------------- | ------------------------------------------------- |
| `video_id`                  | VID_016                    | Khóa liên kết mọi file khác                       |
| `ngay_quay`, `buoi`         | 2026-11-06, sáng           | Cân bằng ánh sáng                                 |
| `tuyen`                     | Nguyễn Trãi – Thanh Xuân   | Chia tập theo tuyến                               |
| `thoi_tiet`                 | nắng / âm u / sau mưa      | Lọc ảnh cho giai đoạn sau                         |
| `thiet_bi`, `vi_tri_camera` | điện thoại, kính lái, thấp | Camera thấp khác góc camera xe buýt               |
| `co_gps`                    | có / không                 | Báo nhầm tính theo phút hay theo km               |
| `ty_le_cat_tren`            | 0,30                       | Tỷ lệ cắt phần trời, phải giống lúc chạy trên box |
| `tap`                       | train / val / test         | Kết quả chia tập                                  |
| `ghi_chu`                   | lóa nắng nhiều             |                                                   |

### 3.2. Chuẩn hóa các clip hiện có

Hiện có 191 clip cắt từ khoảng 20 video gốc, tổng 47 phút. Trước khi chia tập:

1. Gắn mỗi clip với video gốc của nó trong `clips.csv`.
2. Gộp 8 cặp clip trùng thời gian thành một.
3. Chuyển file gốc VID_011.mp4 ra khỏi thư mục clip.
4. Tìm lại video gốc đầy đủ và sao lưu. Clip chỉ có đoạn quanh vật; thiếu đoạn đường bình thường thì không đo được báo nhầm.

### 3.3. Chia tập

| Tập   | Tỷ lệ video gốc        | Dùng để                              | Quy tắc                                                                  |
| ----- | ---------------------- | ------------------------------------ | ------------------------------------------------------------------------ |
| Train | Khoảng 75%             | Huấn luyện                           | Chỉ lấy ảnh từ các video này                                             |
| Val   | Khoảng 10% (2 video)   | Chọn checkpoint; đo sai số mép của R | Khác tuyến với train nếu được; gán nhãn hai file như train               |
| Test  | Khoảng 15% (3–4 video) | Báo cáo kết quả cuối                 | Khóa ngay; không dùng để chọn ảnh hay chỉnh tham số; gán nhãn theo mục 9 |

Yêu cầu với tập test:

- Có vật ở cả ba vị trí: giữa làn, sát mép, bị che một phần.
- Có ít nhất **20–30 phút đường không có dị vật**, để đo báo nhầm mỗi phút.
- Phủ càng nhiều tình huống ở mục 9.1 càng tốt: nút giao, xe rẽ, dừng đèn, đông xe, lề đất.
- Có cả loại dị vật không xuất hiện trong train, để đo khả năng bắt vật lạ.
- VID_016–018 (lóa, mờ, camera thấp) để ở train làm mẫu khó, không đưa vào test.
- Khi có video từ camera xe buýt thật, tách thêm một tập test riêng cho camera này.

### 3.4. Thu bổ sung dữ liệu

Dữ liệu hiện lệch mạnh: 84/125 tình huống là vật liệu xây dựng, chỉ 4 rào chắn, nón, biển tạm và 2 cây đổ. Hai nhóm sau tạm để ở train; recall của chúng báo cáo là “chưa đủ mẫu”. Thứ tự ưu tiên thu bổ sung:

1. Cây đổ, cành gãy; rào chắn, nón, biển tạm — quay ở tuyến mới để đưa thẳng vào test.
2. Nút giao, vòng xuyến, đoạn xe rẽ.
3. Đường lề đất, đường sát nhà dân, đường đang thi công.
4. Xe ba gác, xe đẩy, xe bò, xe tự chế, động vật trên đường — lớp được miễn hiếm.
5. Đoạn đường không có dị vật, đủ dài cho tập test.
6. Video từ camera gắn trên xe buýt thật.

### 3.5. Dữ liệu cá nhân

Video có mặt người và biển số xe, thuộc phạm vi Luật Bảo vệ dữ liệu cá nhân có hiệu lực từ 1/1/2026. Làm mờ mặt và biển số trước khi gửi ảnh cho người gán nhãn bên ngoài; lưu trữ có kiểm soát truy cập.

---

## 4. Bước 2 — Trích khung và lọc ảnh tự động

Chạy hoàn toàn bằng script, chỉ trên video train và val (video test giữ nguyên để chạy cả thuật toán). 47 phút × 2 khung/giây ≈ 5.700 khung trước khi lọc.

| Thao tác                  | Cách làm                                                                                             | Ngưỡng khởi đầu                                   | Lý do                                                                |
| ------------------------- | ---------------------------------------------------------------------------------------------------- | ------------------------------------------------- | -------------------------------------------------------------------- |
| 1. Trích khung            | 2 khung/giây; xe đứng yên chỉ giữ 1 khung cho cả đoạn. Trích từ file gốc, không từ bản quay màn hình | —                                                 | Video hành trình rất trùng lặp; bản quay màn hình có thanh giao diện |
| 2. Cắt phần trời          | Theo `ty_le_cat_tren` của video                                                                      | Theo bảng kê                                      | Phải giống hệt cách box cắt                                          |
| 3. Loại ảnh mờ            | Phương sai Laplacian trên nửa dưới ảnh                                                               | Gán tay 100 ảnh nét/mờ, chọn ngưỡng tách tốt nhất | Ngưỡng phụ thuộc camera                                              |
| 4. Loại phơi sáng sai     | Độ sáng trung bình; tỷ lệ pixel cháy sáng                                                            | Dưới 40 hoặc trên 220 (thang 0–255); hơn 30% cháy | Ảnh tối gắn nhãn “đêm”, không xóa                                    |
| 5. Loại trùng trong video | Perceptual hash so với khung đã giữ                                                                  | Hamming ≤ 6                                       | Không gán hai lần cùng một cảnh                                      |
| 6. Loại trùng giữa video  | Embedding DINOv2 hoặc CLIP                                                                           | Cosine > 0,95                                     | Cùng đoạn đường có thể quay nhiều lần                                |
| 7. Loại bố cục hỏng       | Xem nhanh ảnh thu nhỏ: camera bị che, phản chiếu lớn, góc quá lệch                                   | —                                                 | Góc quá khác camera xe buýt làm mô hình học sai                      |
| 8. Ghi lý do loại         | Ghi vào `frames.csv`                                                                                 | —                                                 | Khi cần ảnh đêm, ảnh mưa thì lọc lại                                 |

**Không loại ảnh chỉ vì “khó”.** Ngược sáng nhẹ, đường ướt, bóng cây loang lổ, đông xe vẫn giữ: box gặp chúng hằng ngày. Chỉ loại ảnh không dùng được.

---

## 5. Bước 3 — Chọn ảnh để gán nhãn (vòng 1)

Vòng 1 gán khoảng **250 ảnh**: 40% có dị vật, 60% chọn theo độ đa dạng. Với video lái xe, chọn theo độ đa dạng cho kết quả tốt hơn lấy đều theo thời gian [1].

| Nhóm ảnh                                     | Cách chọn                                                                    | Số lượng                             | Lý do                                               |
| -------------------------------------------- | ---------------------------------------------------------------------------- | ------------------------------------ | --------------------------------------------------- |
| Có dị vật                                    | Từ danh mục 125 tình huống; mỗi vật 2–3 khung ở cự ly xa, vừa, gần           | Khoảng 100                           | Mô hình học không tô vật thành mặt đường            |
| — vật liệu xây dựng                          | Chọn đa dạng: cát, gạch, đá, phế thải; giữa làn và sát mép; đống dài dọc mép | Tối đa khoảng 50                     | Tránh mô hình chỉ giỏi nhận đống cát                |
| — cây đổ, rào chắn, nón, hàng hóa, rác, khác | Lấy hết, mỗi vật 3 khung                                                     | Phần còn lại                         | Bù lệch nhóm                                        |
| Đa dạng                                      | Embedding DINOv2 toàn kho, chạy k-center greedy (FiftyOne Brain)             | Khoảng 150                           | Phủ kiểu đường, ánh sáng, mật độ xe                 |
| — bắt buộc có                                | Ảnh đường bình thường và ảnh dễ nhầm (xe đỗ, xe đẩy, cọc tiêu, ổ gà)         | Ít nhất 40                           | Dạy mô hình điều không phải dị vật                  |
| — ảnh đường thoáng                           | Ít xe, thấy rõ mép đường cả hai bên                                          | Ít nhất 30 (tính trong nhóm đa dạng) | Làm nền để dán xe, người khi huấn luyện R (mục 5.2) |

### 5.1. Ràng buộc phủ tình huống

Mỗi tình huống dưới đây có ít nhất 10 ảnh (nếu kho có):

| Tình huống                                | Mô hình cần học được gì                                           |
| ----------------------------------------- | ----------------------------------------------------------------- |
| Đường có bó vỉa                           | Trường hợp chuẩn                                                  |
| Đường lề đất, không bó vỉa                | Mép mờ; không gán nhầm lề thành Khác                              |
| Đường sát nhà dân, không vỉa hè           | Mép là chân tường; hàng hóa bày sát mép                           |
| Nút giao, ngã ba, vòng xuyến              | Đảo giao thông, chân cột đèn tín hiệu là Kết cấu cố định          |
| Đường cong                                | R theo được đường cong                                            |
| Đông xe máy, kẹt xe                       | R đoán lòng đường sau hàng xe                                     |
| Xe to ngay phía trước                     | R đoán lòng đường khi bị che diện rộng                            |
| Xe ba gác, xe đẩy, xe thô sơ chở hàng     | Phân biệt xe chở hàng (được miễn) với đống hàng trên đường (Khác) |
| Đường đang thi công                       | Nhiều vật thật, dễ lẫn với nền                                    |
| Đường ướt, vũng nước, bóng cây loang lổ   | Phản chiếu và bóng là Mặt đường                                   |
| Trời âm u, hoàng hôn (khoảng 10% tổng số) | Không quá nhạy với ánh sáng                                       |

Ràng buộc khác:

- Tối đa 15 ảnh mỗi video gốc.
- Vòng 1 chỉ gán ảnh ban ngày.
- **Mỗi lớp được miễn có mặt trong ít nhất 30 ảnh.** Lớp nào kho chưa đủ (thường là Xe thô sơ, Động vật) thì ghi vào danh sách thu bổ sung (mục 3.4).

### 5.2. Vì sao cần ảnh đường thoáng

Phần lòng đường sau xe là chỗ R dễ sai nhất, và nhãn ở đó do người đoán. Khi huấn luyện, script dán xe, người cắt từ các ảnh đã gán lên ảnh đường thoáng. Nhãn lòng đường phía sau vật dán là **chính xác tuyệt đối**, vì ảnh gốc nhìn thấy toàn bộ mép đường. Người gán không phải làm thêm gì ngoài gán đúng các ảnh này.

---

## 6. Bước 4 — Quy tắc vẽ đa giác lòng đường (file 1)

Vẽ đa giác xuyên qua mọi xe, người và dị vật, như thể con đường trống. Mỗi ảnh một đa giác, 6–12 điểm; dùng nhiều điểm hơn ở đường cong và nút giao.

### 6.1. Quy tắc theo tình huống

| Tình huống                 | Cách vẽ                                                                                                                                                        | Lý do                                              |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| Đường có bó vỉa            | Từ mép bó vỉa bên này sang mép bó vỉa bên kia. Không tách làn đỗ xe                                                                                            | Dị vật ở chỗ xe đỗ sát lề vẫn phải bắt             |
| Mép bị xe hoặc vật che     | Nối thẳng từ đoạn mép nhìn thấy phía trước sang đoạn nhìn thấy phía sau                                                                                        | R phải học đoán mép sau vật che                    |
| Đường không bó vỉa, lề đất | Mép là mép mặt đường cứng (nhựa, bê tông); không tính phần đất bị xe nén                                                                                       | Ranh giới nhìn thấy được, nhất quán giữa người gán |
| Đường sát nhà dân          | Mép là chỗ mặt đường cứng gặp chân tường, bậc thềm hoặc sân nhà                                                                                                | Hàng hóa bày trước nhà nằm sát mép này             |
| Có rãnh, mương ở mép       | Mép là mép mặt đường cứng; rãnh nằm ngoài đa giác                                                                                                              | Rãnh là nền ven đường                              |
| Mép bị bùn đất phủ         | Kẻ theo mép đoán được từ đoạn trước và sau. Đoạn không đoán được: **tạm thời** vẽ theo mép đoán tốt nhất; cách đánh dấu Bỏ qua cho đoạn này chưa chốt (mục 13) | Nhãn đoán bừa gây nhiễu                            |
| Có dải phân cách giữa      | Bao cả hai chiều và dải phân cách                                                                                                                              | Dải phân cách là Kết cấu cố định ở file 2          |
| Ngã ba, ngã tư             | Mở rộng theo mặt đường của đường ngang, tới hết phần nhìn thấy                                                                                                 | Dị vật ở miệng ngã rẽ vẫn trên lòng đường          |
| Đảo giao thông, vòng xuyến | Bao cả đảo                                                                                                                                                     | Đảo là Kết cấu cố định ở file 2                    |
| Phía xa                    | Dừng ở chỗ không còn phân biệt được mép đường; không chắc thì vẽ rộng hơn                                                                                      | Phía xa vật quá nhỏ, ảnh hưởng ít                  |

### 6.2. Quy tắc khớp mép (bắt buộc)

Ở đoạn mép **nhìn thấy được**, cạnh đa giác phải trùng ranh giới Mặt đường / Vỉa hè (hoặc Mặt đường / Nhà) trên mặt nạ 11 lớp, **lệch tối đa khoảng 3 pixel**. Script kiểm tra ở mục 10.3.

Lý do: thuật toán đo bề rộng lòng đường từ R, và mọi ngưỡng kích thước tính theo bề rộng đó. Hai file lệch nhau ở mép thì mô hình học đường biên nhiễu: thước đo sai, và dải mảnh dọc mép bị coi là nghi vấn.

Cách làm nhanh nhất: tô Mặt đường và cắt vật trước (mục 8.4), rồi vẽ đa giác **bám theo đường biên Mặt đường đã có**, chỉ tự kẻ ở đoạn bị che. Đỉnh ở mép ảnh: click hơi ra ngoài ảnh để đỉnh dính đúng mép (script cũng kéo vùng hở ≤ 8 px sát khung ra tới khung).

Người gán **không** cần:

- vẽ dải Bỏ qua dọc mép — script huấn luyện tự đặt dải Bỏ qua rộng 4 pixel hai bên cạnh đa giác;
- đánh dấu đoạn nào bị che — script tự xác định (mục 8.5).

---

## 7. Bước 5 — Quy tắc mặt nạ 11 lớp (file 2)

### 7.1. Bảng lớp

| ID  | Lớp               | Gán vào lớp này                                                                                                                                          | Không gán vào lớp này                                   | Trên box                     |
| --- | ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------- | ---------------------------- |
| 0   | Mặt đường         | Mặt đường xe chạy nhìn thấy; vạch sơn, nắp ga, gờ giảm tốc, vết vá, vết nứt, ổ gà; bùn mỏng, nước loang, vũng nước; bóng đổ; que, giấy, cành nhỏ nằm dẹt | Vật nhô lên rõ; lề đất, cỏ                              | Được miễn                    |
| 1   | Vỉa hè / lề       | Vỉa hè, bó vỉa, lề đất, lề cỏ, rãnh, mương, bậc, gờ đất, đất đắp ven đường, bãi đất trống sát đường, sân trước nhà                                       | Phần mặt đường cứng xe vẫn chạy                         | Không được miễn              |
| 2   | Nhà / tường       | Mặt tiền nhà, tường bao, cổng, hàng rào cố định; vẽ thô, đúng ở chân tường                                                                               | Rào tôn, rào tạm công trình đặt trên đường (→ Khác)     | Không được miễn              |
| 3   | Kết cấu cố định   | Dải phân cách cứng, đảo giao thông, cọc tiêu cố định, chân cột đèn tín hiệu và chân cột biển báo nằm trong lòng đường                                    | Barie, rào nhựa di động, nón giao thông (→ Khác)        | Được miễn                    |
| 4   | Xe máy, xe đạp    | Xe máy, xe đạp, xe đạp điện, xe máy điện; người ngồi trên xe; hàng còn trên xe                                                                           | Xe ba gác máy, xe lôi (→ Xe thô sơ)                     | Được miễn                    |
| 5   | Ô tô              | Xe con, xe 7 chỗ, xe bán tải, xe van, taxi                                                                                                               | Xe có thùng hàng (→ Xe tải)                             | Được miễn                    |
| 6   | Xe tải, xe buýt   | Xe tải mọi cỡ, container, rơ moóc, xe buýt, xe khách từ 16 chỗ; máy xúc, xe lu, cần cẩu                                                                  | —                                                       | Được miễn                    |
| 7   | Xe thô sơ, ba gác | Xe ba gác (cả ba gác máy), xe lôi, xe đẩy, xe kéo tay, xích lô, xe bò, xe tự chế                                                                         | Đồ người đang gánh, vác (→ Người)                       | Được miễn                    |
| 8   | Người             | Người đi bộ, đứng, ngồi trên đường; cả đồ người đó đang gánh, vác, cầm                                                                                   | Người ngồi trên xe (→ lớp của xe)                       | Được miễn                    |
| 9   | Động vật          | Chó, mèo, gà, gia súc còn sống                                                                                                                           | Xác động vật (→ Khác)                                   | Được miễn                    |
| 10  | Khác              | Mọi dị vật nhô lên rõ: cây và cành đổ, nón, rào chắn tạm, đống cát, gạch, đá, hàng rơi, bao rác, bàn ghế. Cả cây đứng, trời, cột điện, biển báo trên cột | Vật nhỏ, dẹt (→ Mặt đường); nền ven đường (→ Vỉa hè/lề) | Trong lòng đường là nghi vấn |
| 255 | Bỏ qua            | Phản chiếu kính, taplo, mui xe; vùng quá xa không phân biệt được; thanh giao diện trình phát                                                             | Vùng trong lòng đường ở gần camera                      | Không tính loss              |

**Về lớp Khác.** Khác là lớp “còn lại”: điểm ảnh không thuộc 10 lớp kia và không phải Bỏ qua thì là Khác. Nhưng Khác **phải được tô**: trong LabelFlow chỗ để trống xuất ra là Bỏ qua (255), không tự thành Khác (mục 8.5). **Trong lòng đường, Khác càng là việc phải chủ động làm**: nhãn sơ bộ của máy thường tô đống cát, tấm bạt thành Mặt đường; người gán không cắt ra thì nó vẫn là Mặt đường.

### 7.2. Nguyên tắc khi vẽ

1. **Ngưỡng “nhô lên rõ”:** vật có mặt đứng hoặc bóng đổ nhìn thấy được, và lớn hơn khoảng một cục gạch, thì là Khác. Nhỏ hơn thì gộp vào Mặt đường.
2. **Đặt lên hay thuộc về nền:** ở mép đường, vật được đặt lên (đống, bao, thùng, rào) là Khác; phần thuộc về nền (lề, rãnh, gờ đất, cỏ mọc) là Vỉa hè/lề. Dấu hiệu của vật đặt lên: hình đống hoặc khối, mép rõ, màu hoặc chất liệu khác hẳn phần lề xung quanh.
3. **Người và xe:** người ngồi trên xe (lái hoặc chở) gán cùng lớp với xe, vẽ chung một vùng. Người đứng dưới đất đẩy, kéo xe gán Người, tách khỏi xe.
4. **Vẽ từ xa đến gần:** vẽ mặt đường và vỉa hè trước, rồi vật phía trước đè lên. Mỗi ranh giới chỉ vẽ một lần.
5. **Vật bị che hoặc bị cắt mép ảnh:** chỉ vẽ phần nhìn thấy. Phần bị che đã có đa giác lòng đường lo.
6. **Phía trên đường chân trời:** trời và cây là một đa giác Khác; dãy nhà là một đa giác Nhà/tường. Vẫn phải tô: để trống thì thành Bỏ qua.
7. **Camera cố định:** vùng mui xe, khung kính dùng một mặt nạ Bỏ qua chung cho mọi ảnh của camera đó: **Ignore template** của LabelFlow (mỗi camera một project) và cùng PNG đó trong `ignore/` cho script (mục 8.5).

### 7.3. Các ca khó

| Ca                                                                | Gán                                                              | Lý do                                                 |
| ----------------------------------------------------------------- | ---------------------------------------------------------------- | ----------------------------------------------------- |
| Hàng còn trên xe / hàng đã rơi xuống đường                        | Lớp của xe / Khác                                                | Hàng rơi là dị vật                                    |
| Xe ba gác chở vật liệu đỗ bên đường                               | Xe thô sơ, ba gác (cả xe và hàng trên xe)                        | Phần đã đổ xuống đất là Khác                          |
| Người đẩy xe đẩy; người gánh hàng rong                            | Người đẩy: Người; xe đẩy: Xe thô sơ; người gánh: Người (cả gánh) | Ngoài phạm vi, được miễn                              |
| Xe hỏng, xe đỗ lâu ngày trên đường                                | Lớp của xe                                                       | Xe ở mọi trạng thái đều được miễn                     |
| Máy xúc, xe lu đang thi công                                      | Xe tải, xe buýt                                                  | Xe được miễn; vật liệu, rào quanh công trường là Khác |
| Xe ở xa hoặc bị che, không rõ loại                                | Loại gần nhất                                                    | Nhầm giữa các loại xe không ảnh hưởng box             |
| Đồ bày bán đặt xuống đường; người bán ngồi cạnh                   | Đồ: Khác; người: Người                                           | Đồ đặt trên đường vẫn chiếm dụng lòng đường           |
| Nón giao thông, barie, rào tôn công trình                         | Khác                                                             | Vật tạm, nhóm GĐ1; tính cả khi đặt hợp pháp           |
| Một nón giao thông đơn lẻ, nhỏ                                    | Khác (vẫn gán)                                                   | Nhãn không phụ thuộc ngưỡng β                         |
| Cọc tiêu cố định, dải phân cách bê tông, đảo giao thông           | Kết cấu cố định                                                  | Không miễn thì báo nhầm mỗi chuyến                    |
| Cột đèn tín hiệu, cột biển báo trong lòng đường                   | Phần chân: Kết cấu cố định; phần cột phía trên: Khác             | Phần chạm đất quyết định có báo nhầm hay không        |
| Rạp cưới, rạp tang dựng trên lòng đường                           | Khác                                                             | Box vẫn bắt, server xếp loại                          |
| Xác động vật                                                      | Khác                                                             | Vật cản thật; server lọc                              |
| Vũng nước, bóng cây, vết dầu, bùn mỏng, phản chiếu trên đường ướt | Mặt đường                                                        | Không nhô lên; gán Khác gây báo nhầm hàng loạt        |
| Đống đất, cát màu giống mặt đường                                 | Khác, vẽ kỹ theo bóng và mép đống                                | Ca dễ bỏ sót nhất                                     |
| Bạt, tấm tôn nằm phẳng                                            | Khác nếu thấy mép hoặc nếp gấp                                   | Không chắc thì gán Khác                               |
| Dãy vật liệu dài dọc mép                                          | Khác cho toàn bộ dãy; đa giác vẫn vẽ tới mép đường thật phía sau | Ca phổ biến nhất trong dữ liệu                        |
| Rãnh thoát nước hở, mương, nắp rãnh ở mép                         | Vỉa hè/lề (nắp ga giữa đường: Mặt đường)                         | Nền ven đường, có mặt suốt tuyến                      |
| Lề đất gồ ghề, đất đắp, cỏ mọc ven đường                          | Vỉa hè/lề                                                        | Như trên                                              |
| Đống đất, phế thải đổ trên lề, lấn ra đường                       | Khác                                                             | Có hình đống, khác hẳn lề xung quanh                  |
| Tán cây, biển chìa ra phía trên lòng đường                        | Khác                                                             | Đúng với những gì nhìn thấy                           |
| Xe đỗ trên vỉa hè; người ngồi bán trên vỉa hè                     | Lớp của xe; Người                                                | Vẫn gán đúng lớp để mô hình học nhất quán             |

### 7.4. Ví dụ áp dụng

Ảnh mẫu: đường bùn đất, hai xe máy, một tảng đá giữa đường, xe tải phía xa, bao rác bên lề trái.

- Đa giác lòng đường: từ mép trái (chỗ mặt đường giáp lề đất có vũng nước) sang mép phải, kéo lên tới chỗ xe tải, vẽ xuyên qua hai xe máy và tảng đá.
- Tảng đá giữa đường → Khác; nằm trong đa giác nên là vùng nghi vấn chuẩn.
- Lớp bùn đất, vệt nước loang, que và cành nhỏ nằm dẹt → Mặt đường.
- Vũng nước lớn bên trái: phần trên lề đất → Vỉa hè/lề; phần tràn lên mặt đường → Mặt đường.
- Bao rác đen bên lề trái → Khác; nằm ngoài đa giác nên không tạo vùng nghi vấn.
- Xe máy kèm người lái → Xe máy, xe đạp; xe tải phía xa → Xe tải, xe buýt.
- Dãy nhà phía xa → Nhà/tường; cây, trời → Khác; vẽ thô.
- Vùng phản chiếu tối góc dưới bên phải, thanh giao diện trình phát → Bỏ qua.

---

## 8. Bước 6 — Công cụ và thao tác

Người gán **không vẽ từ đầu**: mô hình lớn trên server tạo nhãn sơ bộ, người chỉ sửa. Vẽ tay từ đầu cần hơn 1,5 giờ mỗi ảnh ở Cityscapes [2]. Thời gian thực tế đo ở đợt thử 30 ảnh.

### 8.1. Công cụ

| Thành phần     | Cách làm                                                                                                                       | Lý do                                                                                  |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------- |
| Công cụ gán    | **LabelFlow**: ứng dụng desktop Windows, chạy offline. Mỗi người gán một project trên máy mình; gom dữ liệu bằng thư mục xuất  | Miễn phí; có đa giác, cọ, SAM, nhập mặt nạ hàng loạt, duyệt; dữ liệu không rời công ty |
| Công cụ hỗ trợ | SAM trong LabelFlow (MobileSAM ONNX [3], cài một lần mỗi máy): kéo khung quanh vật, click trái thêm, click phải bớt, Enter ghi | Cắt dị vật và xe khỏi mặt đường bằng vài click                                         |
| Script         | Thư mục `tools/` (Python): nhãn sơ bộ, chuyển đổi, kiểm tra, ảnh chồng nhãn, độ đồng thuận, nhãn sự kiện test                  | Phần LabelFlow không làm                                                               |
| Không dùng     | Layer **Traffic exclusion**, **Suspected region**; layer **Candidate objects** chỉ dùng cho nhãn sự kiện video test (mục 9.2)  | Không thuộc hai file nhãn                                                              |

Bấm gì, ở đâu: `huong_dan_thao_tac_labelflow.md`.

### 8.2. Khai báo nhãn trong LabelFlow

Profile **Road Encroachment**; **Semantic classes → Custom list** đúng 11 dòng `mat_duong`, `via_he_le`, `nha_tuong`, `ket_cau_co_dinh`, `xe_may_xe_dap`, `o_to`, `xe_tai_xe_buyt`, `xe_tho_so`, `nguoi`, `dong_vat`, `khac` (thứ tự = ID 0–10).

| Nhãn       | Layer, công cụ                                                                                                   | Khi xuất thành |
| ---------- | ---------------------------------------------------------------------------------------------------------------- | -------------- |
| 11 lớp     | Layer **Semantic**: đa giác tô, cọ, SAM                                                                          | File 2 (0–10)  |
| Bỏ qua     | Lớp có sẵn **`255 · Ignore / uncertain`** (không tạo lớp `bo_qua`); vùng cố định của camera: **Ignore template** | File 2: 255    |
| Lòng đường | Layer **Road Extent**: một đa giác                                                                               | File 1         |

- Mỗi lớp một màu riêng; chọn lớp bằng phím (`1`–`9` = lớp 0–8, `Alt+1` = Động vật, `Alt+2` = Khác): người duyệt nhìn màu là thấy lỗi.
- **Mỗi camera một project**: Ignore template áp cho mọi ảnh chưa duyệt có cùng kích thước trong project.
- **Nhập ảnh theo thư mục con mỗi video**: LabelFlow lấy thư mục chứa ảnh làm nhóm chia tập. Tập thật vẫn do `videos.csv` quyết định (mục 8.5).

### 8.3. Tiền gán nhãn

1. **Vòng 1:** `tools/prelabel_mask2former.py` chạy Mask2Former Swin-L Mapillary Vistas [4] trên server có GPU, ánh xạ về 11 lớp theo Phụ lục B, ghi mỗi ảnh một PNG trùng tên (0–10 và 255). Nạp vào LabelFlow: layer Semantic → **Import mask folder** → ánh xạ giữ nguyên → **Replace the layer**; sau đó áp Ignore template. Đa giác lòng đường vòng 1 vẽ tay.
2. **Vòng 2 trở đi:** dùng PIDNet-S của vòng trước. Mô hình sinh cả mặt nạ 11 lớp (nạp vào layer Semantic) lẫn lòng đường sơ bộ (đầu ra R, giá trị 0/1, nạp vào layer Road Extent), khớp bộ lớp hơn và không phụ thuộc mô hình học trên dữ liệu phi thương mại. Road Extent nạp vào là mặt nạ tô, không có đỉnh: sửa bằng cọ hoặc xóa rồi vẽ lại đa giác.

Lỗi thường gặp của nhãn sơ bộ, cần sửa:

- Đống vật liệu, đất đá bị tô thành mặt đường → cắt ra bằng SAM, gán Khác.
- Rãnh, lề đất bị tô thành Khác → gán Vỉa hè/lề.
- Cột đèn tín hiệu bị tô thành cột hoặc mặt đường → phần chân trong lòng đường gán Kết cấu cố định.
- Bóng cây đậm, vũng nước bị tô thành vật → gán Mặt đường.
- Mô hình vòng 1 không có lớp đảo giao thông, dải phân cách, nón giao thông → các vùng này luôn phải sửa tay.
- Xe ba gác, xe đẩy, xe tự chế bị tô thành ô tô hoặc lẫn với hàng hóa → sửa đúng loại, tách phần hàng đã đổ xuống đất thành Khác.

### 8.4. Thứ tự thao tác trên mỗi ảnh

Trong LabelFlow, **tô sau ghi đè tô trước** (cọ, đa giác tô, SAM). Vì vậy tô nền trước, vật sau:

| #   | Việc                                                                                                                           | Lý do làm ở bước này                                           |
| --- | ------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------- |
| 1   | Tô, sửa **Mặt đường** phủ rộng, cả dưới xe và vật                                                                              | Ranh giới quan trọng nhất; vật tô sau sẽ đè lên                |
| 2   | **Cắt mọi dị vật** bằng SAM, gán Khác; **xe, người, động vật** đúng loại, không lẫn vào Mặt đường                              | Lỗi phổ biến nhất của nhãn sơ bộ; lớp được miễn phải sạch      |
| 3   | Vẽ **đa giác lòng đường**: bám biên Mặt đường ở đoạn nhìn thấy, nối thẳng qua đoạn bị che                                      | Khớp mép tự nhiên, không phải căn lại                          |
| 4   | Kiểm tra **mép đường**: rãnh, lề, đất đắp là Vỉa hè/lề, tô **sát tới** Mặt đường; chân cột trong lòng đường là Kết cấu cố định | Tránh báo nhầm dọc mép; khe chưa tô làm không đo được khớp mép |
| 5   | Chân tường, mép vỉa hè ở chỗ nhìn thấy; phía trên đường chân trời tô thô; Bỏ qua cho vùng phản chiếu                           | Ngữ cảnh cho R; chỗ để trống thành Bỏ qua                      |
| 6   | **Tự kiểm tra**: lớp phủ đậm nhất, bật/tắt để thấy chỗ chưa tô; trong lòng đường không còn chỗ trống                           | Chỗ quên tô là Bỏ qua: mô hình không học được vật đó           |

Thiếu thời gian thì việc 1–3 bắt buộc phải xong; việc 4–6 làm nhanh hơn.

### 8.5. Xuất nhãn

1. Người duyệt chuyển ảnh đạt sang `approved`. Xuất từ LabelFlow: **Export dataset → Semantic + region PNGs**, **Approved images only**, Train 100 %, Val 0 %. Không dùng cách chia tập của LabelFlow (chia ngẫu nhiên theo băm nhóm).
2. Chạy `tools/convert_labelflow.py` với **mọi** thư mục xuất (mỗi người gán một), `--videos videos.csv`, `--chi-duyet`, `--ignore-dir ignore/`. Script đổi tên file về `<video_id>_<giây>.png` và chia tập theo cột `tap` của `videos.csv`; một video nằm ở hai tập thì dừng.
3. **File 2:** giá trị 0–10; **chỗ chưa tô = 255 (Bỏ qua)**, cùng giá trị với vùng Ignore. Khác bản 3.0 (khởi tạo bằng Khác) vì LabelFlow xuất pixel chưa tô là 255 và không phân biệt được với vùng Ignore. Chỗ quên tô vẫn không bao giờ thành Mặt đường, nhưng cũng không dạy mô hình gì; bù lại bằng kiểm tra "Bỏ qua trong lòng đường > 1%" (mục 10.3). Muốn quay lại cách cũ: `lop.yaml → labelflow.chua_to: 10` (khi đó vùng taplo cũng thành Khác).
4. **File 1:** Road Extent thành 1, nền 0. Chỗ vùng dừng cách khung ảnh ≤ 8 px được kéo ra sát khung (đỉnh click hơi lọt vào trong ảnh). Đoạn Bỏ qua trên đa giác chưa có cách làm (mục 13).
5. Áp mặt nạ Bỏ qua cố định của camera lên **cả hai file** (`--ignore-dir`). Ignore template trong LabelFlow chỉ tác động file 2.
6. Script tự **đánh dấu đoạn mép bị che**: dọc cạnh đa giác, chỗ nào phía trong mặt nạ 11 lớp không phải Mặt đường thì đoạn đó là đoạn đoán. Kết quả lưu vào `edge/`: 1 nhìn thấy, 2 bị che (phía trong là xe, người, động vật, Khác), 3 không xác định (phía trong là Vỉa hè, Nhà, Kết cấu hoặc Bỏ qua, thường do đa giác lệch mép). Dùng để giảm trọng số loss của R ở đoạn đoán và để đo sai số mép tách riêng (mục 9.4).
7. Khi tạo dữ liệu huấn luyện, script huấn luyện đặt 255 cho dải 4 pixel hai bên cạnh đa giác trong file 1.

Bảng ID lớp, nhóm được miễn, cách gộp về 7 lớp, cách đọc file xuất LabelFlow và ngưỡng kiểm tra ghi trong một file duy nhất `tools/lop.yaml` (script chép một bản vào thư mục dữ liệu). Script xuất nhãn, script huấn luyện và cấu hình box đều đọc file này, để ba nơi không lệch nhau.

---

## 9. Bước 7 — Nhãn cho video test

Video test **không gán mặt nạ từng khung**. Có hai loại nhãn, đều gán theo thời gian trên video gốc.

### 9.1. Nhãn đoạn tình huống

Chia mỗi video test thành các đoạn liên tiếp; mỗi đoạn gắn một hoặc nhiều nhãn. Ghi vào `test_segments.csv` với các cột `video_id`, `giay_bat_dau`, `giay_ket_thuc`, `nhan` (nhiều nhãn cách nhau bằng `;`).

| Nhãn đoạn         | Định nghĩa                                 | Dùng để đo                                |
| ----------------- | ------------------------------------------ | ----------------------------------------- |
| `di_thang`        | Xe chạy, đường gần thẳng                   | Trường hợp chuẩn                          |
| `nut_giao`        | Đang đi qua ngã ba, ngã tư, vòng xuyến     | Báo nhầm ở góc vỉa hè; vật ở miệng ngã rẽ |
| `re`              | Xe đang rẽ, quay đầu                       | Vết đứt khi rẽ                            |
| `dung`            | Xe dừng (trạm, đèn đỏ, kẹt xe)             | Không gửi trùng                           |
| `dong_xe`         | Mặt đường bị xe che hơn khoảng một nửa     | R đoán lòng đường sau xe                  |
| `cong_truong_cho` | Đoạn có vật lạ dày đặc                     | Quy tắc đoạn bất thường                   |
| `anh_xau`         | Lóa nắng, ngược sáng, kính bẩn, ra vào hầm | Quy tắc đoạn bất thường                   |
| `le_dat`          | Đường không bó vỉa, mép là lề đất          | Báo nhầm dọc mép                          |
| `doc`             | Dốc cầu, cầu vượt, hầm                     | Thước đo khi xe chúi, dốc                 |
| `sach`            | Không có dị vật nào trong đoạn             | Báo nhầm mỗi phút; tỷ lệ khung bất thường |

### 9.2. Nhãn sự kiện dị vật

Mỗi dị vật một dòng trong `test_events.csv`:

| Cột                               | Ví dụ                                                          | Dùng để                                                                                                |
| --------------------------------- | -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `obj_id`, `video_id`              | OBJ_041, VID_020                                               | Đối chiếu với sự kiện box gửi lên                                                                      |
| `giay_xuat_hien`, `giay_bien_mat` | 12,5 – 19,0                                                    | Sự kiện của box phải rơi vào khoảng này                                                                |
| `giay_ro_nhat`                    | 16,0                                                           | Khung để vẽ hộp bao và đo mức chiếm                                                                    |
| `hop_bao`                         | x, y, w, h                                                     | Kiểm tra mặt nạ nghi vấn có trùng vật; bao toàn bộ phần nhìn thấy của vật                              |
| `da_giac_long_duong`              | Đa giác của khung rõ nhất                                      | Để script tính mức chiếm                                                                               |
| `muc_chiem`                       | 0,12                                                           | Script tính: bề rộng lớn nhất của phần vật nằm trong đa giác, chia cho bề rộng đa giác ở cùng hàng ảnh |
| `nhom`                            | cay_do / rao_non_bien / vat_lieu / hang_hoa / rac / rap / khac | Recall theo nhóm GĐ1                                                                                   |
| `vi_tri`                          | giua_lan / sat_mep / bi_che                                    | Ba vị trí hỏng vì lý do khác nhau                                                                      |
| `kieu_duong`                      | via_he / sat_nha / le_dat                                      | Kết quả theo kiểu đường                                                                                |
| `co_trong_train`                  | có / không                                                     | Khả năng bắt vật lạ chưa từng thấy                                                                     |

Hộp bao và đa giác của khung rõ nhất vẽ bằng LabelFlow trên khung trích ra từ video (hộp trên layer Candidate objects, đa giác trên Road Extent, `obj_id` ghi vào Notes); `tools/test_events.py` điền `giay_ro_nhat`, `hop_bao`, `da_giac_long_duong`, `muc_chiem`.

### 9.3. Quy tắc chấm điểm

- **Đã phát hiện:** vật có ít nhất một sự kiện của box trong khoảng `giay_xuat_hien`–`giay_bien_mat`, với mặt nạ nghi vấn trùng hộp bao của vật.
- **Mẫu số recall:** chỉ tính vật có `muc_chiem` ≥ β. Vật nhỏ hơn β báo cáo riêng.
- **Báo nhầm:** mọi sự kiện không trùng dòng nào trong `test_events.csv`; không cần gán tay. Tính trên số phút xe chạy (tổng thời lượng trừ các đoạn `dung`).
- **Báo cáo:** mọi chỉ số tách theo nhóm vật, vị trí, kiểu đường và nhãn đoạn tình huống.

### 9.4. Tập val

Tập val gán nhãn hai file như train (mục 6, 7) và dùng thêm để đo **sai số mép của R**: khoảng cách giữa biên vùng lòng đường box tính được và biên đa giác chuẩn, theo phần trăm bề rộng đường. Sai số này **báo cáo tách riêng đoạn mép nhìn thấy và đoạn mép bị che** (theo `edge/`, mục 8.5), vì đoạn bị che là nơi R dễ sai nhất. Người duyệt xem lại toàn bộ đa giác của tập val.

---

## 10. Bước 8 — Kiểm soát chất lượng

### 10.1. Các lớp kiểm tra

| Khi nào        | Việc cần làm                                                                                                                                                             | Tiêu chí đạt                                                                                          | Lý do                                       |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------- | ------------------------------------------- |
| Trước khi gán  | **Hiệu chuẩn:** gán chuẩn 20 ảnh có đủ nút giao, lề đất, vật sát mép, nhiều loại xe; mỗi người gán lại 20 ảnh này; so bằng `tools/agreement.py`                          | Trùng trên 95% pixel với bản chuẩn, tính trên 11 lớp, chỉ ở pixel cả hai bên đều đã tô (khác 255) [5] | Mọi người hiểu quy tắc giống nhau           |
| Trước vòng 1   | **Đợt thử 30 ảnh:** cả nhóm gán, ghi thời gian mỗi ảnh, gom ca tranh cãi                                                                                                 | Bổ sung ca tranh cãi vào mục 7.3                                                                      | Có số liệu lập lịch; vá lỗ hổng quy tắc sớm |
| Trong khi gán  | **Gán đôi 5% số ảnh** do hai người làm độc lập; đo riêng nhóm lề đất và nút giao (`tools/agreement.py`)                                                                  | Khoảng 96% pixel trùng [2]                                                                            | Phát hiện người gán bị lệch dần             |
| Trước khi xuất | **Người duyệt** xem từng ảnh `needs_review` trong LabelFlow; tập trung vào đa giác, ranh giới Mặt đường quanh xe và dị vật, vùng mép đường, chỗ chưa tô trong lòng đường | Đạt → `approved`; không đạt → `rejected` + ghi lý do vào Notes                                        | Ba chỗ này quyết định bỏ sót và báo nhầm    |
| Sau khi xuất   | **Script kiểm tra tự động** `tools/check_labels.py` (mục 10.3); ảnh cảnh báo xem bằng `tools/overlay.py --chi-loi`                                                       | Không có lỗi chặn; ảnh cảnh báo đã xem và sửa                                                         | Bắt lỗi mắt khó thấy                        |

Nếu độ đồng thuận ở nhóm đường lề đất thấp: gán Bỏ qua cho dải chuyển tiếp giữa mặt đường và lề đất, thay vì ép người gán chọn một mép.

### 10.2. Lỗi thường gặp khi duyệt

| Lỗi                                         | Dấu hiệu                                                                  | Hậu quả                                  |
| ------------------------------------------- | ------------------------------------------------------------------------- | ---------------------------------------- |
| Dị vật còn nằm trong Mặt đường              | Đống vật thể mang màu Mặt đường                                           | Mô hình học bỏ sót                       |
| Đa giác cắt qua đống vật sát mép            | Đống vật nằm ngoài đa giác dù ở trên đường                                | Vật không bao giờ thành nghi vấn         |
| Đa giác lệch mép Mặt đường ở đoạn nhìn thấy | Dải mảnh giữa biên đa giác và biên Mặt đường                              | Thước đo sai; báo nhầm dọc mép           |
| Rãnh, lề, đất đắp gán Khác                  | Dải Khác chạy dọc mép đường                                               | Báo nhầm lặp lại suốt tuyến              |
| Chân cột, đảo giao thông gán Khác           | Vùng Khác cố định ở nút giao                                              | Báo nhầm mỗi chuyến                      |
| Vũng nước, bóng cây gán Khác                | Vùng Khác phẳng, không có mặt đứng                                        | Báo nhầm hàng loạt                       |
| Hàng đã đổ xuống đường gán chung với xe     | Vùng xe kéo dài xuống mặt đường, quá thân xe                              | Mô hình học miễn cả đống hàng; bỏ sót    |
| Vật hoặc vùng trong lòng đường chưa tô      | Vùng hồng đậm trên ảnh chồng nhãn; cảnh báo "Bỏ qua chiếm …% Road Extent" | Mô hình không học vật đó                 |
| Khe chưa tô giữa Mặt đường và Vỉa hè        | Dải trống dọc mép; cảnh báo "hàng có khe chưa tô"                         | Không đo được khớp mép; R học biên nhiễu |
| Đa giác kéo quá phần đã tô ở phía xa        | Vùng trống giữa cuối Mặt đường và đỉnh trên của đa giác                   | Tăng tỷ lệ Bỏ qua trong lòng đường       |

### 10.3. Script kiểm tra tự động

`tools/check_labels.py` → `qc_report.csv`. Ngưỡng ghi trong `tools/lop.yaml` (`nguong_qc`).

| Kiểm tra                                                                                                                                                           | Mức      | Ngưỡng                                                                         |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------- | ------------------------------------------------------------------------------ |
| File 2 chỉ có giá trị 0–10 và 255; file 1 chỉ có 0, 1 và 255                                                                                                       | Chặn     | Bắt buộc                                                                       |
| Mỗi ảnh có đúng một vùng Road Extent liên thông và có pixel Mặt đường                                                                                              | Chặn     | Bắt buộc                                                                       |
| Ảnh và hai file nhãn cùng kích thước                                                                                                                               | Chặn     | Bắt buộc. Tỷ lệ cắt phần trời kiểm ở bước trích khung (script nhãn không biết) |
| Bỏ qua (255) nằm trong đa giác: quên tô vật, khe hở; không tính dải 8 px sát khung ảnh                                                                             | Cảnh báo | Quá 1% diện tích đa giác; in vị trí vùng lớn nhất                              |
| Phần Mặt đường nằm trong đa giác                                                                                                                                   | Cảnh báo | Dưới 95%                                                                       |
| Khớp mép: ở các hàng có biên Mặt đường/Vỉa hè (hoặc /Nhà) nhìn thấy, khoảng cách **vuông góc** tới cạnh đa giác (đo theo hàng ngang phóng đại 3–4 lần ở cạnh xiên) | Cảnh báo | Lệch quá 3 pixel ở hơn 10% số hàng                                             |
| Mép có khe chưa tô (255) rộng hơn 3 pixel giữa Mặt đường và Vỉa hè/Nhà (không đo được khớp mép)                                                                    | Cảnh báo | Hơn 10% số hàng                                                                |
| Dải Khác sát cạnh đa giác, chạy dài theo mép (dấu hiệu gán nhầm lề, rãnh)                                                                                          | Cảnh báo | Dài hơn 1/3 chiều cao vùng đường, rộng dưới 5% bề rộng đa giác                 |
| Số ảnh có mặt của mỗi lớp được miễn (và Khác)                                                                                                                      | Thống kê | Dưới 30 ảnh: bổ sung ảnh ở vòng sau                                            |
| Số ảnh đường thoáng (mục 5)                                                                                                                                        | Thống kê | Dưới 30 ảnh: bổ sung ở vòng sau. Chưa tự động (mục 13)                         |

Cảnh báo "> 10% Ignore" trong `semantic-stats.json` của LabelFlow chỉ để tham khảo.

---

## 11. Đầu ra và cấu trúc thư mục

```
vnroad/
  manifest/
    videos.csv            # bảng kê video gốc và tập (mục 3.1)
    clips.csv             # clip → video gốc
    frames.csv            # khung đã trích và lý do loại (mục 4)
    test_events.csv       # nhãn sự kiện dị vật (mục 9.2)
    test_segments.csv     # nhãn đoạn tình huống (mục 9.1)
  ignore/                 # mặt nạ Bỏ qua cố định theo camera (trắng = bỏ qua)
  labelflow_export/       # thư mục xuất gốc của LabelFlow, mỗi người gán một
  data/                   # do tools/convert_labelflow.py sinh; chạy lại thì ghi đè
    images/               # ảnh đã chọn, đã làm mờ mặt và biển số
    masks/                # file 2: PNG một kênh, 0–10 và 255
    road/                 # file 1: PNG một kênh, 0/1 và 255
    edge/                 # đoạn mép: 1 nhìn thấy, 2 bị che, 3 không xác định (mục 8.5)
    list/
      train.lst           # mỗi dòng: ảnh  masks  road
      val.lst
      test.lst            # chỉ khi có ảnh tập test
    lop.yaml              # bản sao tools/lop.yaml (mục 8.5)
    convert_log.csv       # ảnh nào được chuyển, bị bỏ, lỗi gì
    qc_report.csv         # kết quả script kiểm tra (mục 10.3)
    overlay/              # ảnh chồng nhãn để duyệt (tools/overlay.py)
tools/                    # script và lop.yaml
```

Tên file thống nhất: `<video_id>_<giây>.png`, ví dụ `VID_016_00125.5.png`. Ảnh và các file nhãn của cùng một khung có cùng tên.

---

## 12. Kiểm chứng sau vòng 1 và chọn ảnh vòng 2, 3

### 12.1. Kiểm chứng sau vòng 1

Vòng 1 dùng để xác nhận phương án trên dữ liệu thật trước khi gán tiếp. Đọc các chỉ số sau:

| Chỉ số                                                                    | Nếu kém thì                                                         |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| Sai số mép của R, đoạn nhìn thấy                                          | Xem lại quy tắc khớp mép, chất lượng đa giác                        |
| Sai số mép của R, đoạn bị che                                             | Thêm ảnh đông xe, xe to phía trước; tăng ảnh đường thoáng để dán xe |
| Recall theo nhóm vật và vị trí                                            | Thêm ảnh theo nguyên nhân bỏ sót (mục 12.2)                         |
| Báo nhầm mỗi phút theo nhãn đoạn tình huống                               | Thêm ảnh theo loại báo nhầm (mục 12.2)                              |
| So sánh bản 11 lớp với bản 7 lớp gộp (tạo bằng `lop.yaml`, không gán lại) | Giữ bản tốt hơn cho các vòng sau                                    |

### 12.2. Chọn ảnh theo lỗi

Chạy thuật toán trên video test, xếp mỗi ca bỏ sót vào đúng một nguyên nhân. Nguyên nhân chiếm nhiều nhất quyết định loại ảnh cần thêm.

| Nguyên nhân bỏ sót                                   | Ảnh cần thêm ở vòng sau                                                           |
| ---------------------------------------------------- | --------------------------------------------------------------------------------- |
| Vật nằm ngoài vùng lòng đường                        | Ảnh cùng kiểu đường (lề đất, sát nhà, đông xe che mép); ảnh vật dài dọc mép       |
| Vật bị miễn nhầm (mô hình gán vào một lớp được miễn) | Ảnh có loại vật đó; vật phẳng, màu xám, màu đất; đống hàng, bạt phủ dễ lẫn với xe |
| Bị lọc theo bề rộng                                  | Không cần thêm ảnh; xem lại ngưỡng β                                              |
| Không được xác nhận qua nhiều khung                  | Không cần thêm ảnh; xem lại quy tắc k/n                                           |

| Báo nhầm lặp lại nhiều nhất | Ảnh cần thêm ở vòng sau                             |
| --------------------------- | --------------------------------------------------- |
| Dọc mép đường               | Ảnh lề đất, rãnh, đất đắp, gán đúng Vỉa hè/lề       |
| Ở nút giao                  | Ảnh nút giao, đảo giao thông, chân cột đèn tín hiệu |
| Bóng, nước, phản chiếu      | Ảnh đường ướt, bóng cây, gán đúng Mặt đường         |
| Xe thô sơ, xe lạ, động vật  | Ảnh có loại xe hoặc động vật đó, gán đúng lớp       |

Cơ cấu mỗi vòng (400–600 ảnh):

1. Khoảng 40%: ảnh mô hình không chắc chắn (entropy trung bình cao trong vùng lòng đường và quanh mép).
2. Khoảng 40%: đa dạng (k-center greedy, loại trừ ảnh đã gán).
3. Khoảng 20%: ảnh nhắm vào lỗi theo hai bảng trên.

**Điều kiện dừng:** recall sự kiện tăng dưới khoảng 1 điểm sau một vòng. **Tùy chọn từ vòng 3:** mô hình hiện tại gán nhãn giả cho ảnh chưa gán, chỉ giữ pixel có độ tin cậy cao, rồi trộn vào tập train.

---

## 13. Việc cần chốt trước khi bắt đầu

| Câu hỏi                                                                                                                                                                      | Ảnh hưởng tới                                             |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------- |
| Ngưỡng β: vật chiếm bao nhiêu bề rộng đường thì tính là lấn chiếm                                                                                                            | Mẫu số recall (mục 9.3); không ảnh hưởng cách gán nhãn    |
| Video đã quay có GPS không                                                                                                                                                   | Báo nhầm tính theo phút hay theo km                       |
| Video gốc đầy đủ nằm ở đâu, đã sao lưu chưa                                                                                                                                  | Chia tập; đo báo nhầm                                     |
| Tỷ lệ cắt phần trời và kích thước đầu vào (1024×512)                                                                                                                         | Trích khung phải cắt giống box                            |
| Ai gán nhãn, bao nhiêu người, có thuê ngoài không                                                                                                                            | Lịch vòng 1; làm mờ mặt, biển số                          |
| Lịch thu bổ sung cây đổ, rào chắn, nón, nút giao, xe thô sơ                                                                                                                  | Đo recall riêng từng nhóm; đủ mẫu lớp hiếm                |
| Khi nào có video từ camera xe buýt thật                                                                                                                                      | Tập test riêng cho camera này                             |
| Giữ quy ước **chỗ chưa tô = Bỏ qua (255)** thay cho Khác (mục 8.5)                                                                                                           | Chỗ quên tô; tham số `labelflow.chua_to` trong `lop.yaml` |
| Đoạn mép bị bùn phủ không đoán được (mục 6.1): (a) bỏ quy tắc, vẽ theo mép đoán tốt nhất; hoặc (b) cho dùng layer Traffic exclusion riêng cho việc này và bổ sung vào script | File 1                                                    |
| Cách đánh dấu ảnh đường thoáng (mục 5), ví dụ ghi `duong_thoang` vào Notes để script đọc                                                                                     | Thống kê đường thoáng; dán xe, người khi huấn luyện R     |

---

## Phụ lục A — Bảng tra nhanh cho người gán nhãn

**Thứ tự trên mỗi ảnh:** tô Mặt đường rộng → SAM cắt dị vật, xe, người, động vật → vẽ đa giác bám mép → sửa mép (Vỉa hè/lề tô sát Mặt đường) → chân tường, phía trên chân trời, Bỏ qua → tự kiểm tra (không còn chỗ trống trong lòng đường) → `needs_review`.

| Thấy gì                                                                    | Gán gì                                                       |
| -------------------------------------------------------------------------- | ------------------------------------------------------------ |
| Vật nhô lên trên mặt đường, to hơn cục gạch                                | Khác                                                         |
| Vật dẹt, nhỏ; vũng nước; bóng; vết dầu                                     | Mặt đường                                                    |
| Không chắc là mặt đường hay vật (trong lòng đường)                         | Khác                                                         |
| Rãnh, mương, bậc, lề đất, cỏ, đất đắp ven đường                            | Vỉa hè / lề                                                  |
| Đống đất, phế thải đổ trên lề lấn ra đường                                 | Khác                                                         |
| Xe máy, xe đạp, xe điện (cả người ngồi trên xe)                            | Xe máy, xe đạp                                               |
| Xe con, xe 7 chỗ, bán tải, xe van                                          | Ô tô                                                         |
| Xe tải, container, xe buýt, xe khách lớn, máy công trình                   | Xe tải, xe buýt                                              |
| Ba gác, xe lôi, xe đẩy, xích lô, xe bò, xe tự chế                          | Xe thô sơ, ba gác                                            |
| Hàng còn trên xe                                                           | Lớp của xe                                                   |
| Hàng đã rơi, đổ xuống đường                                                | Khác                                                         |
| Người đi bộ, đứng, đẩy xe, gánh hàng                                       | Người                                                        |
| Vật nuôi, gia súc còn sống                                                 | Động vật                                                     |
| Dải phân cách, đảo giao thông, cọc tiêu cố định, chân cột trong lòng đường | Kết cấu cố định                                              |
| Nón, barie, rào tạm, rào tôn công trình                                    | Khác                                                         |
| Trời, cây đứng, cột điện, biển trên cột                                    | Khác (vẽ thô, nhưng phải tô)                                 |
| Nhà, tường, cổng                                                           | Nhà / tường (vẽ thô, đúng chân tường)                        |
| Mui xe, taplo, phản chiếu kính, thanh giao diện                            | Bỏ qua (Ignore template hoặc lớp `255 · Ignore / uncertain`) |

**Đa giác lòng đường:** có bó vỉa thì từ bó vỉa sang bó vỉa; không bó vỉa thì theo mép mặt đường cứng; vẽ xuyên qua xe và vật; đoạn nhìn thấy bám đúng biên Mặt đường (lệch không quá 3 pixel); đỉnh ở mép ảnh click hơi ra ngoài; không chắc thì vẽ rộng.

---

## Phụ lục B — Ánh xạ lớp Mapillary Vistas về 11 lớp (tiền gán nhãn vòng 1)

Mô hình Mask2Former Swin-L Mapillary Vistas dùng bộ 65 lớp của Vistas bản 1.2 (cộng Unlabeled); bộ này không có lớp đảo giao thông, dải phân cách, nón giao thông. Bảng dưới đã cài trong `tools/prelabel_mask2former.py`; chạy `--xem-anh-xa` để đối chiếu với `id2label` trong `config.json` của mô hình.

| Lớp dự án         | Lớp Mapillary Vistas                                                                                                                       |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Mặt đường         | Road, Lane Marking - General, Lane Marking - Crosswalk, Crosswalk - Plain, Manhole, Catch Basin, Pothole, Bike Lane, Service Lane, Parking |
| Vỉa hè / lề       | Sidewalk, Curb, Curb Cut, Pedestrian Area, Terrain, Sand                                                                                   |
| Nhà / tường       | Building, Wall, Fence, Bridge, Tunnel                                                                                                      |
| Kết cấu cố định   | Guard Rail, Barrier                                                                                                                        |
| Xe máy, xe đạp    | Motorcycle, Bicycle, Motorcyclist, Bicyclist, Other Rider                                                                                  |
| Ô tô              | Car                                                                                                                                        |
| Xe tải, xe buýt   | Truck, Bus, Trailer, Caravan, On Rails                                                                                                     |
| Xe thô sơ, ba gác | Wheeled Slow, Other Vehicle                                                                                                                |
| Người             | Person                                                                                                                                     |
| Động vật          | Ground Animal, Bird                                                                                                                        |
| Bỏ qua            | Ego Vehicle, Car Mount, Unlabeled                                                                                                          |
| Khác              | Tất cả lớp còn lại, gồm Sky, Vegetation, Pole, Utility Pole, Street Light, Traffic Light, Traffic Sign, Trash Can, Billboard, Banner       |

Cần sửa tay sau ánh xạ:

- Barrier gồm cả khối bê tông tạm của công trình → chuyển thành Khác.
- Chân cột đèn tín hiệu trong lòng đường (máy gán Pole) → Kết cấu cố định.
- Đảo giao thông, dải phân cách (máy thường gán Sidewalk hoặc Curb) → Kết cấu cố định.

---

## Nguồn tham khảo

1. [Mittal và cộng sự — Best Practices in Active Learning for Semantic Segmentation](https://arxiv.org/pdf/2302.04075)
2. [Cordts và cộng sự — The Cityscapes Dataset for Semantic Urban Scene Understanding](https://arxiv.org/pdf/1604.01685)
3. [Zhang và cộng sự — Faster Segment Anything: Towards Lightweight SAM for Mobile Applications (MobileSAM)](https://arxiv.org/abs/2306.14289)
4. [Mask2Former Swin-L Mapillary Vistas semantic (Hugging Face)](https://huggingface.co/facebook/mask2former-swin-large-mapillary-vistas-semantic/tree/main)
5. [Varma và cộng sự — IDD: A Dataset for Exploring Problems of Autonomous Navigation in Unconstrained Environments](https://cseweb.ucsd.edu/~mkchandraker/pdf/wacv19_idd.pdf)
6. [Chen và cộng sự — Understanding the Impact of Label Granularity on CNN-based Image Classification](https://arxiv.org/pdf/1901.07012)
7. [Hong và cộng sự — Towards Understanding the Effect of Pretraining Label Granularity](https://arxiv.org/abs/2303.16887)
8. [Breitenstein, Fingscheidt — Amodal Cityscapes](https://arxiv.org/pdf/2206.00527)
9. [Luật Bảo vệ dữ liệu cá nhân có hiệu lực từ 1/1/2026 (Rajah & Tann)](https://www.rajahtannasia.com/viewpoints/new-law-on-personal-data-protection-comes-into-operation-on-1-january-2026/)
