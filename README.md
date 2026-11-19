# LabelFlow

Ứng dụng desktop gán nhãn ảnh và video **offline**, ưu tiên Windows: Tauri 2 + React + TypeScript + Rust + SQLite. Không cần cloud, server hay tài khoản; AI là tùy chọn và chạy trên CPU của máy.

## Tính năng

- **Ba loại nhãn độc lập:** semantic (mỗi pixel một lớp, bộ lớp tự định nghĩa theo project), region (vùng nhị phân như lòng đường), object (bbox, polygon, mask).
- **Gán nhanh:** SAM click ra mặt nạ (MobileSAM/SAM/SAM-HQ ONNX), đa giác tô semantic, nhập nhãn sơ bộ hàng loạt từ thư mục PNG, Ignore template theo camera, phím tắt chọn lớp, Done/Approve → next.
- **Làm việc nhóm:** trạng thái review (`needs_review` → `approved`/`rejected`), thảo luận ca khó qua thư mục dùng chung (không cần server).
- **Video:** lập chỉ mục PTS, trích khung chính xác, nhóm chia tập theo video/chuyến đi.
- **Xuất dataset:** semantic + region PNG, YOLO detection/segmentation, COCO; chia tập theo nhóm, có manifest và checksum; phiên bản dataset bất biến.
- **Công cụ dataset:** so sánh dự đoán với nhãn, hàng đợi review, tìm ảnh gần trùng, lan truyền nhãn theo video, dashboard chất lượng.
- **An toàn dữ liệu:** ảnh nguồn không bao giờ bị sửa, lưu tự động có journal, sao lưu/khôi phục, ảnh đã duyệt được bảo vệ khỏi ghi đè.

## Tải và chạy

Dùng bản portable Windows x64 (`LabelFlow-windows-x64.zip`, trên trang Releases): giải nén, chạy `LabelFlow.exe`. Cần WebView2 runtime (Windows 10/11 thường có sẵn). Python/mô hình/FFmpeg chỉ cần khi dùng SAM, AI gợi ý hoặc video.

## Tài liệu

| Tài liệu                                                                   | Nội dung                                                                                                                                                                                                                                                 |
| -------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [HUONG_DAN_SU_DUNG.md](HUONG_DAN_SU_DUNG.md)                               | **Hướng dẫn sử dụng**: tạo project, công cụ vẽ, phím tắt, SAM, nhãn sơ bộ, duyệt, xuất, video, AI, sao lưu                                                                                                                                               |
| [docs/vnroad/](docs/vnroad/)                                               | Dự án dị vật lòng đường (PIDNet-S): [quy tắc gán nhãn v3.1](docs/vnroad/Huong_dan_loc_va_gan_nhan_du_lieu_v3.md), [thao tác trong LabelFlow](docs/vnroad/huong_dan_thao_tac_labelflow.md), [báo cáo kiểm tra](docs/vnroad/bao_cao_kiem_tra_labelflow.md) |
| [tools/README.md](tools/README.md)                                         | Script Python cho dự án trên: chuyển đổi, kiểm tra chất lượng, ảnh chồng nhãn, nhãn sơ bộ                                                                                                                                                                |
| [worker/README.md](worker/README.md)                                       | Môi trường Python, manifest mô hình và giao thức của AI worker                                                                                                                                                                                           |
| [docs/decisions/](docs/decisions/)                                         | Quyết định kiến trúc (ADR 0001–0010)                                                                                                                                                                                                                     |
| [docs/formats/](docs/formats/)                                             | Định dạng project/schema (v1–v6), migration, export                                                                                                                                                                                                      |
| [docs/milestones/](docs/milestones/), [docs/benchmarks/](docs/benchmarks/) | Báo cáo nghiệm thu M0–M5, số đo hiệu năng                                                                                                                                                                                                                |
| [docs/resource-efficiency.md](docs/resource-efficiency.md)                 | Tối ưu RAM, dung lượng, tốc độ                                                                                                                                                                                                                           |
| [docs/architecture/third-party.md](docs/architecture/third-party.md)       | Thành phần bên thứ ba và giấy phép                                                                                                                                                                                                                       |
| [docs/master-plan.md](docs/master-plan.md)                                 | Kế hoạch phát triển gốc                                                                                                                                                                                                                                  |

## Phát triển

### Yêu cầu

- Node.js 22.13+ (đã thử 22.20.0)
- Rust 1.99+ (đã thử 1.99.1) với `rustfmt`, `clippy`
- Windows: Visual Studio 2022 C++ Build Tools + Windows SDK, WebView2. Xem [prerequisites của Tauri](https://v2.tauri.app/start/prerequisites/).

### Chạy và build

```powershell
npm.cmd ci
rustup component add rustfmt clippy
npm.cmd run desktop:dev      # chạy bản phát triển (Vite hot reload)
npm.cmd run desktop:build    # build release: src-tauri/target/release/labelflow.exe
```

- Dùng `npm.cmd` trong PowerShell để tránh lỗi Execution Policy chặn `npm.ps1`.
- Các npm script gọi Rust qua `scripts/rust.mjs`: dùng toolchain trong `.tools/` nếu có, nếu không thì `cargo` trên PATH.
- `desktop:build` tạo executable, chưa có installer; chạy trực tiếp, không cần Node/Vite.
- `.cargo/config.toml` giới hạn build 2 job để không vượt bộ nhớ khi link nhiều test Tauri trên Windows (đổi bằng `CARGO_BUILD_JOBS`). Profile dev tắt incremental; bật bằng `CARGO_INCREMENTAL=1` nếu cần.

### Kiểm thử

```powershell
npm.cmd run check      # typecheck, lint, format, vitest, tooling, rustfmt, cargo test, clippy, build
npm.cmd run test:ui    # smoke UI trên trình duyệt (IPC giả lập)
npm.cmd run test:desktop
```

| Lệnh                                                    | Kiểm tra                                                                                          |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `test:desktop`                                          | Build bản debug có CDP loopback (cổng 9223), mở cửa sổ thật với Rust/SQLite rồi đóng. Chỉ Windows |
| `test:ai`                                               | AI worker Python với mô hình fixture (cần môi trường trong [worker/README.md](worker/README.md))  |
| `test:m2:acceptance`, `test:m2:desktop`                 | Video, COCO, YOLO segmentation (cần FFmpeg và gói Python nghiệm thu, xem dưới)                    |
| `test:m3:desktop`, `test:m4:desktop`, `test:m5:desktop` | Nghiệm thu desktop từng milestone                                                                 |
| `benchmark:m4`, `benchmark:m5`, `benchmark:resources`   | Số đo hiệu năng; kết quả trong `.cache/`                                                          |

- Các build smoke/benchmark có CDP. **Sau đó chạy lại `npm.cmd run desktop:build`** để có executable production không bật CDP.
- Không chạy Cargo build/test khi executable cùng đường dẫn đang mở.
- Gói Python cho nghiệm thu M2 (chỉ để kiểm thử, không cần để chạy app), tải một lần:

  ```powershell
  python -m pip install --target .cache/python Pillow==13.0.0 pycocotools==2.0.11 NumPy==2.5.4 PyYAML==6.0.3
  python scripts/prepare-m2-validation.py
  ```

- Nghiệm thu trên dataset thật (project riêng trong `.cache/m1-acceptance`, không sửa nguồn):

  ```powershell
  $env:LABELFLOW_REAL_DATASET = '<thư mục ảnh>'
  node scripts/rust.mjs cargo test --offline --manifest-path src-tauri/Cargo.toml --test manual_labeling real_dataset_acceptance -- --ignored --exact --nocapture
  ```

### Ubuntu 24.04

Chưa kiểm thử thực tế; CI chạy Windows và Ubuntu.

```bash
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev librsvg2-dev libayatana-appindicator3-dev patchelf
rustup component add rustfmt clippy
npm ci
npm run check
npm run desktop:dev
npx playwright install --with-deps chromium && CI=1 npm run test:ui
```

## Cấu trúc thư mục

| Vị trí                                                         | Nội dung                                                                       |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `src/app`, `src/components`                                    | Khung ứng dụng, chuỗi giao diện, phím tắt                                      |
| `src/features/*`                                               | Project, media, annotations (geometry, canvas, tools), review, video, ai, team |
| `src/lib`                                                      | DTO, bảng lệnh IPC, chuẩn hóa lỗi                                              |
| `src-tauri/src/commands`, `services`                           | IPC allowlist, điều phối ngoài UI thread                                       |
| `src-tauri/src/domain`, `repositories`, `database`, `security` | Domain, SQL, kiểm tra đường dẫn, khóa file                                     |
| `src-tauri/src/import`, `export`, `video`, `ai`                | Nhập/xuất, video (FFmpeg), AI worker                                           |
| `src-tauri/migrations`, `capabilities`, `permissions`          | Schema có phiên bản, quyền tối thiểu                                           |
| `src-tauri/tests`                                              | Test tích hợp Rust (test `#[ignore]` là nghiệm thu chạy tay)                   |
| `tests/fixtures`, `tests/ui`, `tests/worker`                   | Dữ liệu mẫu, smoke UI, test worker                                             |
| `worker/`                                                      | AI worker Python (ONNX Runtime CPU, SAM)                                       |
| `scripts/`                                                     | Launcher Rust, smoke/nghiệm thu/benchmark, `make-sam-manifest.py`              |
| `tools/`                                                       | Script Python cho dự án dị vật lòng đường                                      |
| `docs/`                                                        | Tài liệu (xem bảng Tài liệu)                                                   |

`.cache/`, `.tools/`, `node_modules/`, `src-tauri/target/` và `releases/` là dữ liệu cục bộ, không commit (xem `.gitignore`).
