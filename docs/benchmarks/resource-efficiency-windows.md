# Tối ưu tài nguyên sau M5 — Windows

Đo trên máy phát triển hiện tại, với Tauri/WebView2 release và frontend đóng gói. Các số đo là bằng chứng cho workload cụ thể, không phải cam kết cho mọi ảnh/máy hoặc chứng nhận toàn bộ performance target trong master plan.

## Cách đo

- Giữ executable M5 ban đầu cùng SHA-256 ở `.cache/resource-baseline/receipt.json` và `labelflow-production.exe`; bản benchmark M5 chỉ thêm cổng CDP loopback và dùng frontend M5 đã đóng gói riêng.
- `resource_efficiency::prepare_resource_benchmark` tạo project riêng: 4 PNG RGB 1920×1080, 20 bbox, một semantic mask dạng khối với nhiều đoạn RLE và một region mask. Mỗi lượt so sánh dùng project mới, cùng thuật toán/giá trị pixel; tên ID/path có thể khác.
- `node scripts/resource-desktop-benchmark.mjs baseline|optimized` chạy native app bằng harness, cùng cửa sổ 1280×820. Cả hai bản đều có CDP phục vụ đo; executable giao cho người dùng được build lại theo cấu hình production không có CDP.
- Navigation IPC: preload trước, đo `select_media` + `load_document`, bỏ 4 lượt đầu, lấy 10 mẫu. Đây không phải latency hiển thị canvas.
- Selection UI: bấm ảnh trong thư viện, chờ filename/canvas, bitmap sẵn sàng, Saved và hai animation frame; bỏ 2 lượt đầu, lấy 10 mẫu. Bao gồm overhead Playwright và lịch vẽ của màn hình.
- Stroke-to-Saved: 60 nét brush, 10 lượt warmup + 50 mẫu; gồm gửi pointer, journal IndexedDB strict, IPC, Rust/SQLite và quan sát DOM Saved. Không phải thời gian riêng của kernel brush hoặc FPS.
- Thu gom rác JS trước các lần chụp bộ nhớ. Private bytes là tổng commit riêng của cây tiến trình app + WebView2; working-set cộng dồn có thể đếm trùng trang chia sẻ. `backingStorageSize` do V8/CDP báo gồm bộ nhớ backing bên ngoài JS heap, không phải tổng RAM ứng dụng.
- CPU idle là mẫu ngắn, tính thời gian CPU tăng của cây tiến trình trong khoảng nghỉ, theo phần trăm một logical core. Không phải đo điện năng hay mức sử dụng CPU kéo dài.
- Kiểm tra SHA-256 bốn ảnh nguồn trước/sau, số revision, vector và event độc lập. Không chạy inference hoặc video trong workload này.

## Kết quả

Số liệu native trước/sau lưu trong `.cache/resource-benchmark/baseline.json` và `optimized.json`; bản sao bằng chứng tại [resource-efficiency-results.json](resource-efficiency-results.json). Cặp đo cuối chạy lần lượt khi không có build/test nặng chạy nền, mỗi bản dùng project mới. Sau loop đo, bản tối ưu còn kiểm tra undo/redo byte-exact và dọn cache qua giao diện native; các thao tác bổ sung này không tính vào các mẫu hiệu năng.

| Chỉ số                                          |    M5 trước tối ưu |         Sau tối ưu |                           Thay đổi |
| ----------------------------------------------- | -----------------: | -----------------: | ---------------------------------: |
| Private bytes sau 60 nét, toàn cây app/WebView2 |         505,23 MiB |         396,61 MiB |                         Giảm 21,5% |
| V8 backing storage sau 60 nét                   |          83,39 MiB |          22,76 MiB |                         Giảm 72,7% |
| Vẽ đến Saved, p50 / p95 (50 mẫu)                | 167,24 / 197,06 ms | 104,73 / 139,35 ms |                 Giảm 37,4% / 29,3% |
| Chọn ảnh đến canvas, p50 / p95 (10 mẫu)         | 145,79 / 361,37 ms | 132,00 / 152,70 ms |          p50 giảm 9,5%; p95 ít mẫu |
| Navigation IPC, p50 / p95 (10 mẫu)              | 565,00 / 622,79 ms | 551,12 / 570,48 ms |                  Chỉ cải thiện nhẹ |
| Private bytes khi mới mở ảnh, trước workload    |         299,25 MiB |         295,73 MiB |                    Gần tương đương |
| CPU trong mẫu nghỉ 3,5 giây, % một core         |                 0% |              0,88% | Mẫu ngắn, không kết luận khác biệt |
| Launch đến ảnh đầu sẵn sàng, 1 mẫu              |           1.009 ms |           1.859 ms |  Chưa chứng minh cải thiện startup |
| EXE release dùng đo (có CDP)                    |    17.901.568 byte |    15.107.584 byte |                         Giảm 15,6% |

Số private bytes gồm cả GPU process của WebView2; không chỉ đo JS heap. GPU process sau workload: 252.473.344 → 199.331.840 byte, renderer process: 198.238.208 → 139.247.616 byte. Tổng working set ước lượng cộng dồn cũng được giữ trong JSON nhưng không dùng làm con số RAM duy nhất vì có trang chia sẻ.

Một lượt thử trước khi sửa vòng đời canvas đã giảm V8 backing storage nhưng tăng tổng private bytes lên 610.492.416 byte. Đã xác định cache bị hủy sau `lostpointercapture` thông thường, canvas không còn dùng chưa được giải phóng backing chủ động; đã sửa và đo lại cả cặp. Receipt chẩn đoán còn tại `.cache/resource-benchmark/optimized-before-canvas-release.json`.

Startup là một mẫu chức năng, có tạo WebView2 profile riêng, không được dùng làm cold-start benchmark. Mẫu của bản tối ưu chậm hơn; cần đo lặp trên các máy/ổ đĩa thực trước khi kết luận nguyên nhân hoặc tối ưu riêng startup. Không suy ra 60 FPS hay UI-thread blocking dưới 50 ms từ thời gian Saved bất đồng bộ.

Unit test giữ đủ 100 nét nhỏ trên mask 1080p, raster history không quá 409.600 byte, sau khi tạo 110 nét và loại 10 nét cũ. Đây là bài kiểm tra bộ nhớ được tham chiếu bởi delta, không phải private bytes toàn app. Native undo/redo, source hashes, vectors/events và cleanup đều qua.

## Hồi quy

- TypeScript strict, ESLint, Prettier, Vitest, launcher tests, Rust fmt/test/Clippy và build được chạy bằng `npm.cmd run check`.
- 222 Vitest; 78 Rust mặc định, gồm lock/abrupt-exit/WAL/backup/migration và 4 bài resource mới; 6 launcher tests.
- 25 browser UI tests qua, 2 benchmark M4/M5 được skip mặc định; native benchmark và native M5 trên bản release qua, gồm verification/acceptance/snapshots/process restart.
- 9 bài Python và Rust AI supervisor thật qua; không thay model, không đo AI inference trong số liệu resource ở trên.

Native dialogs chọn file/thư mục và máy Linux chưa được nghiệm thu lại bằng đợt đo này. Các ignored test dataset/video khác không được tính là đã chạy lại nếu không nêu riêng.

## Bản production

Đã build lại bằng cấu hình production mặc định, không có CDP: `src-tauri/target/release/labelflow.exe`, **15.107.072 byte** (14,41 MiB). Bản sao chạy trực tiếp tại `releases/LabelFlow/LabelFlow.exe`, kèm README, checksum và dependency inventory; không kèm build cache, Node/Rust, Python, model hoặc FFmpeg.

`scripts/production-launch-smoke.mjs` đã chạy chính bản portable với project/profile thử nghiệm riêng, xác nhận cửa sổ native, yêu cầu đóng bình thường và exit code 0; cổng CDP 9223 không mở. Receipt tại `.cache/resource-benchmark/production-launch.json`. Đây là kiểm tra khởi động/đóng, không thay thế UI acceptance qua bản release smoke đã nêu ở trên. Checksum/build receipt của production được lưu cùng kết quả JSON.

## Thumbnail ảnh thật

20 PNG thật đầu tiên theo thứ tự tên trong thư mục dataset LostAndFound đã được người dùng cho phép. So sánh cùng thumbnail có bounding box 200×140, PNG cũ và JPEG quality 78 mới:

| Chỉ số                                            |        Giá trị |
| ------------------------------------------------- | -------------: |
| Tổng PNG cũ                                       |   815.367 byte |
| Tổng JPEG mới                                     |   129.608 byte |
| Giảm dung lượng                                   |          84,1% |
| Sai số tuyệt đối trung bình mỗi kênh, thang 0–255 |           4,09 |
| Hash ảnh nguồn trước/sau                          | Khớp cả 20 ảnh |

Hai thumbnail đầu được kiểm tra trực quan tại `.cache/resource-benchmark/thumbnail-before.png` và `thumbnail-after.jpg`. Đây là ảnh nhỏ để tìm ảnh trong thư viện; full preview, ảnh nguồn và PNG class-ID vẫn lossless. Không suy rộng sai số này cho toàn dataset hoặc ảnh khác.

## Dung lượng phát triển và dung lượng sử dụng

Mốc workspace ban đầu: `src-tauri/target` khoảng 8,11 GB, `.tools` 1,48 GB, `node_modules` 179 MB và `.cache` 2,03 GB. Đây là công cụ, dependency, symbol, test fixture và build cache của môi trường phát triển. Chúng không cần đi cùng executable khi dùng thông thường. Receipt là ảnh chụp tại thời điểm đo, các thư mục này tiếp tục thay đổi khi biên dịch/kiểm thử.

Project có thể tăng dung lượng vì ảnh managed, video frame đã trích xuất, lịch sử nhãn, dataset versions, backup và export. Cache cleaner không xóa các dữ liệu này. WebView2 runtime/profile và Python/model tùy chọn nằm ngoài executable; không coi kích thước EXE là toàn bộ footprint cài đặt.

## Giới hạn còn lại

- Một ảnh active lớn, các mask visible và thao tác review cần lượng bộ nhớ tương ứng; 32 MiB là ngân sách cho cache decoded ảnh, không phải hard cap toàn ứng dụng.
- Cache đĩa được trim theo đợt sau 30 giây hoặc sau khi tích lũy khoảng 1/8 ngân sách traffic mới; có thể vượt tạm thời trong đợt và giữ lại preview hiện hành lớn. Throttle tránh quét thư mục mỗi pointer/save/request.
- Lịch sử undo giữ tối đa 100 lệnh và khoảng 64 MiB raster payload, không bảo đảm mọi trường hợp đều undo được 100 nét. Nét thay đổi toàn ảnh đắt hơn nét nhỏ. Metadata/JS/DOM và queue journal có overhead riêng.
- Hash nguồn vẫn được kiểm tra khi chọn ảnh; recovery vẫn kiểm tra checksum tài sản nhãn. Không đổi các bảo vệ này lấy tốc độ. Ổ chậm, project có lịch sử lớn và RLE rất phân mảnh vẫn có thể chậm.
- Chưa đo cold-start p95, FPS liên tục, điện năng, workload 4K/24 MP, hàng giờ sử dụng, Linux hoặc GPU/model PIDNet thực tế.
