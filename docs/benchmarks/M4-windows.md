# M4 Windows — measured query and raster improvements

Measured 2026-11-07 on Windows 10.0.26200 x64, Intel Core i5-12500H (16 logical processors), 15.64 GiB RAM. Locked toolchain: Node 22.20.0, Rust 1.99.1; browser benchmark uses installed Edge 157 headless. Cargo build and functional UI tests completed before sampling. Measurements use warm caches on this machine, not hardware-independent guarantees.

| Operation                                                       | Baseline p50 / p95 (ms)          | M4 p50 / p95 (ms) | Samples                               |
| --------------------------------------------------------------- | -------------------------------- | ----------------- | ------------------------------------- |
| M3 suggestion list, 100 rows                                    | 10.2849 / 11.4775                | 0.1332 / 0.1814   | 50 per implementation after 5 warmups |
| 1920×1080 raster replacement, RGBA conversion + Canvas2D upload | 7.90 / 14.20                     | 3.70 / 4.80       | 30 per implementation after 5 warmups |
| M4 queue count + first 40 rows, 10k metadata records            | No comparable prior review queue | 8.3596 / 8.9171   | 20 after 5 warmups                    |

Suggestion-list p50 is about 77× faster in this workload; raster p50 about 2.1×. Exact query results and every final RGBA byte are compared. Same-size raster replacement also asserts ImageData buffer reuse. These are localized operations, **not whole-app FPS, interaction latency, startup or RAM benchmarks**.

## Method and scope

`src-tauri/tests/road_review.rs::benchmark_m4_suggestion_listing` runs in release mode. The same SQLite database contains 128 stored suggestions, each with a 160,086-byte synthetic prediction JSON. The baseline uses the M3 `json_extract` query and previous ordering; M4 reads normalized summary columns with a covering index. Both include the existing media lookup and return the same 100 serialized summaries. Calls alternate baseline then optimized. The baseline runs against the current schema/indexes, so this isolates query/payload access rather than comparing two independently installed applications.

The queue measurement then adds 9,999 metadata-only media records, making 10,000 total, and calls the production page function with default filters, offset 0, limit 40. Each call checks count and page length. It is **not** a 10,000-image decode/import, all-filter stress test, large prediction-history analysis or navigation benchmark.

`tests/ui/performance.spec.ts` uses the actual M4 `SceneRenderer` and a test-local reproduction of the M3 raster path. Two 1080p semantic buffers alternate, including ignore 255. The baseline allocates canvas/ImageData/palette and writes four RGBA bytes per pixel; M4 reuses same-size resources and uses a native-endian uint32 palette. Both call Canvas2D `putImageData`. The measurement excludes document/RLE decoding, final transformed scene composition, mouse delivery and durable annotation writes. Unit tests separately cover transparency, all 256 pixel values and dirty rectangles.

No other build/test workload was intentionally run concurrently with the samples. Fixed call order, headless browser scheduling, cache warmth and OS activity can affect results; repeat on deployment hardware before setting performance targets. This short run does not measure sustained memory pressure. No machine-wide settings or priority were changed.

## Reproduce

```powershell
npm.cmd run benchmark:m4
```

The script sets `CARGO_INCREMENTAL=0`, runs the release Rust benchmark, then the dedicated browser benchmark. Raw local results are `.cache/m4-benchmark/{sqlite,renderer}.json`; [captured results](m4-results.json) preserve this report's run. The generated database/images remain test fixtures, separate from user datasets. Default `test:ui` skips the benchmark so functional tests do not mix with performance sampling.

The original [M1 timings](M1-windows.md) measured complete debug desktop commands with polling, SQLite commits and verification, with a concurrent build. Those numbers are retained as functional evidence; subtracting or comparing them to this isolated raster/query benchmark would be misleading. M5 still needs comparable end-to-end release workloads for brush input, navigation, startup and total WebView process memory.

Rust `[profile.dev] incremental = false` now limits Windows cache/PDB growth by default. A check using old incremental caches encountered LNK1318; obsolete build caches were removed and all checks rerun successfully. This configuration is a build-environment fix, not included in runtime speedup claims.
