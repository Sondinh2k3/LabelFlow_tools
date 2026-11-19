# M5 fixtures

`translation.json` records the expected canonical translation and independent event semantics. Rust `advanced_workflows.rs::texture` generates four tiny source PNGs: a deterministic textured anchor, (+3,+2) target, (-2,+1) neighbor and a flat unregistrable image. Tests attach synthetic same-video timestamps; these are extracted-frame records for software acceptance, not real motion/optical-flow ground truth. Native helper copies all persistent fixture sources into `.cache/m5-acceptance` and never changes user datasets.

`tests/fixtures/m1Brush.ts` preserves the M1 capsule-distance kernel that remained unchanged through M4. M5 compares exact bytes on random/clipped/tangent strokes and benchmarks the same 1080p workloads with both implementations. It is test-only code and is not bundled in the desktop application.
