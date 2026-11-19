# M5 Windows — M1 brush baseline and bounded renderer retention

Measured 2026-11-07 on the same Windows x64 reference machine as M1/M4: Intel Core i5-12500H, 16 logical processors, 15.64 GiB RAM, Windows 10.0.26200. Node 22.20.0, Rust 1.99.1; installed Edge 157 headless. No build, functional UI or Python test was intentionally running concurrently during sampling.

| 1920×1080 brush workload           | M1 p50 / p95 (ms) | M5 p50 / p95 (ms) |
| ---------------------------------- | ----------------- | ----------------- |
| 32 short moves, radius 16          | 0.30 / 0.60       | 0.10 / 0.30       |
| 4 long diagonal moves, radius 24   | 9.50 / 11.10      | 0.20 / 0.30       |
| 32 clipped eraser moves, radius 32 | 0.40 / 0.50       | 0.10 / 0.20       |

Each number measures a complete batch of moves in the indicated row, **not one pointer event or an end-to-end gesture**. Each implementation has 5 warmups and 50 samples, alternating M1 then M5; input arrays reset outside the timed section. Browser timer granularity is approximately 0.1 ms, so ratios for very short batches should be treated as approximate. Raw values are retained in [captured results](m5-results.json).

The M1 baseline is the exact capsule-distance kernel preserved in [m1Brush.ts](../../tests/fixtures/m1Brush.ts), unchanged through M4 before this optimization. Both kernels run in the same browser/process against the same canonical pixels and paths. M1 visits the bounding rectangle and projects each pixel onto the segment. M5 computes convex capsule scanline intervals, trims their edges with the original predicate to preserve floating-point/tangent semantics, then uses typed-array fills. Two unit tests compare exact output across 800 deterministic random cases plus edge/tangent/degenerate cases. The browser also compares every final pixel for each benchmark workload, including erasing labeled pixels to 255.

This isolates brush computation. It excludes raster upload, scene compositing, pointer scheduling, RLE encoding, journal writes and SQLite persistence. It demonstrates improvement over the M1 algorithm; it does not establish 60 FPS or autosave latency. Earlier [M1 desktop timings](M1-windows.md) include command/polling/verification overhead and are retained as functional evidence, not subtracted from these measurements. [M4 renderer/query measurements](M4-windows.md) remain separately scoped; they were not rerun in M5.

## Renderer cache retention

An additional real-Canvas2D exercise replaces 10 visible 128×96 instance masks through 50 generations. A reproduction of M4's geometry-keyed cache retains 500 backing canvases, below its 64 MiB eviction cap. M5 prunes geometries absent from the current visible document before drawing and retains only 10. The test uses actual canvas contexts, not an allocation estimate based only on record counts.

| Referenced resources                              | M4 retention policy      | M5      |
| ------------------------------------------------- | ------------------------ | ------- |
| Instance backing canvases                         | 500                      | 10      |
| Nominal RGBA backing bytes (`width × height × 4`) | 24,576,000               | 491,520 |
| Raster entries after hiding all layers            | Not measured in baseline | 0       |

These are referenced backing-store dimensions, **not OS working set, VRAM, allocator overhead or a measured whole-process memory reduction**. M4 already had a 64 MiB instance-cache cap; M5 removes stale entries earlier, preserves that cap and avoids retaining a single instance canvas larger than it. Hidden/removed raster layers are also released. Toggling a hidden layer may therefore require recolor/upload again. WebGL was not introduced: the measured brush work is small, and a new GPU path is not justified by this evidence.

## Reproduce and limits

```powershell
npm.cmd run benchmark:m5
```

The script runs only `m5-performance.spec.ts` with its explicit benchmark flag. Default functional UI testing skips this and the M4 benchmark. Local raw output: `.cache/m5-benchmark/renderer.json`. No production timing hook or dependency is added.

The warm browser microbenchmark does not measure startup, cold caches, loaded-library navigation, total WebView memory, simultaneous 20-shape input latency, or 10k-image snapshot/index throughput. The master plan's startup/navigation/RAM/FPS targets remain unproven. Native M5 acceptance separately tests the real Tauri/Rust/SQLite pipeline on small deterministic translation fixtures; it is functional acceptance rather than a performance measurement. A production road-video dataset is still needed to evaluate registration usefulness under motion, lighting and occlusion.

Build environment: an unconstrained parallel Cargo check hit Windows commit-limit error 1455 while mapping Tauri metadata. Project-local `.cargo/config.toml` now sets 2 build jobs; all checks were rerun without changing system/pagefile settings. Existing `profile.dev.incremental = false` remains. These build fixes are not included in runtime speedup claims.
