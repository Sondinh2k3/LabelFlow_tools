# M1 Windows functional timings

Measured 2026-11-07 on Windows 10.0.26200 x64, Intel Core i5-12500H (16 logical processors), 15.64 GiB RAM. These are functional test timings, not an isolated performance benchmark. The app uses the locked Node 22.20.0 / Rust 1.99.1 toolchain and WebView2; desktop tests load embedded assets, not Vite.

| Scenario                                         | Result                                | Scope                                                                                                       |
| ------------------------------------------------ | ------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| 100 real images, complete Rust debug integration | 733.22 s                              | Decode, import, thumbnails, generated labels/masks, reopen, canonical export, checksums, backup and restore |
| 100 real images, final Rust release integration  | 49.70 s                               | Same complete pipeline; 100 distinct source SHA-256 values verified; source files unchanged                 |
| Box commands, 20 samples in packaged debug Tauri | p50 1456.87 ms; p95 1652.50 ms        | Playwright mouse gesture through backend revision observed by a subsequent native `load_document` call      |
| Brush command with 21 vectors visible            | 1400.32 ms                            | One 30-step gesture through saved revision observation; exact brush undo checked separately                 |
| Actual process restart                           | 100/100 documents and masks identical | Entire documents compared, including pixels, vectors, metadata and revisions                                |

The command timings include mouse-event delivery, polling intervals, strict journal/SQLite writes and decoding masks again for verification. A release build also ran during this validation session, so these samples are unsuitable for strict CPU/latency comparisons. They do not measure pure brush rendering latency, FPS, startup p50/p95 or idle memory. No claim is made that the master plan's <150 ms preloaded navigation / 60 FPS / <400 MB targets have been achieved.

Raw desktop results: `.cache/m1-acceptance/desktop.json`; source integration results: `.cache/m1-acceptance/latest.json`. The test uses 100 distinct real LostAndFound images authorized by the user, with generated smoke labels rather than human-reviewed ground truth. Scripts are documented in the README.

Before larger-library/video work, measure release builds in an idle environment, cold and warm caches separately, representative dense masks and 20-shape scenes, with frame/input latency, navigation p50/p95, total WebView process memory and 10k-image import throughput. Preserve source-video/trip grouping and compare against this functional baseline without interpreting these debug command timings as FPS.

Release integration command (set `LABELFLOW_REAL_DATASET` first as in README):

```powershell
node scripts/rust.mjs cargo test --release --offline --manifest-path src-tauri/Cargo.toml --test manual_labeling real_dataset_acceptance -- --ignored --exact --nocapture
npm.cmd run desktop:build
```
