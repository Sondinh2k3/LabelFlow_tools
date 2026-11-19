# ADR 0007 — Resource efficiency after M5

Optimize measured labeling workloads while preserving exact masks, undo/redo, durable journaling, source hashes and reviewed GT. No new runtime dependency, model, server, lossy annotation storage or automatic removal of annotation history/snapshots is permitted.

Work checklist:

- [x] Capture an M5 executable baseline and reproducible 1080p/native timings, process-tree memory and disk categories; distinguish build/test cache from shipped application size.
- [x] Avoid reallocating canvas backing stores on ordinary redraws; coalesce brush colorization/upload to animation frames.
- [x] Reuse immutable raster serialization and compact undo history with reversible pixel deltas, retaining memory limits and durable save ordering.
- [x] Remove redundant raster decode/validation work in Rust; preserve checksums, transaction/revision/idempotency checks and full durability.
- [x] Budget decoded images and predictive preload; avoid background work triggered by every save.
- [x] Smaller rebuildable thumbnails/caches, safe cache storage controls and clear accounting; never clean original media, labels, versions, backups or operation history automatically.
- [x] Evaluate application-only size build settings against speed, measure repeatably, and retain only worthwhile tradeoffs.
- [x] Run regression, crash/recovery and native acceptance; publish evidence and remaining limits with a fresh production executable.

Defaults favor a bounded working set and responsive ordinary labeling, not maximum speculative preloading. Cache misses may cost decode time; export/source integrity remains exact. Snapshot and audit history remain authoritative and explicitly excluded from cache cleanup. No statement that an absolute optimum or all master-plan performance targets have been reached is justified without measurements.

Canvas lifetime matters independently of JS heap: normal `lostpointercapture` after a completed stroke must not evict all raster caches. Dispose/prune explicitly zero obsolete canvas dimensions to release graphics backing stores. An early native measurement reduced V8 backing bytes but increased total private bytes; that result was retained as diagnostic evidence, not presented as an overall RAM improvement. Final results must include process-tree memory after graphics settlement.

Implementation and user-facing controls: [resource guide](../resource-efficiency.md). Workload, receipts, measured tradeoffs and remaining limits: [Windows benchmark](../benchmarks/resource-efficiency-windows.md).
