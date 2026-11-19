# ADR 0006 — M5 advanced workflows

M5 checklist from the master plan:

- [x] Profile and optimize brush/render paths; record p50/p95 against an executable reproduction of the M1 algorithm on the same fixtures. Preserve the earlier M1 functional timings without treating them as pure renderer timings.
- [x] Video translation propagation/tracking produces independent suggestions, with provenance and explicit visual verification before acceptance. Preserve approved GT and independent events; test cancellation, stale source/target revisions and failed registration.
- [x] Rebuildable perceptual fingerprints, candidate near-duplicate ranking and a quality dashboard; never delete media, merge labels or alter groups based on similarity.
- [x] Immutable, content-addressed dataset version snapshots, frozen annotation objects/profile/metadata and deterministic trip/video split manifests; verify and export frozen data without editing the live dataset.
- [x] Migration/backup, unit/integration/UI/native tests, fixtures, documentation and release build.

No new runtime dependency, model, cloud or server is required. Canvas2D stays the default; WebGL is optional in the specification and will only be introduced with evidence that it is needed.

Propagation uses bounded global translation registration against an approved anchor frame, not unrestricted optical flow or a neural tracker. Each target is registered directly against the anchor to avoid chained drift. Only same-video, same-size frames within a short timestamp window are eligible. Poor texture, ambiguous matches, excessive photometric error and out-of-range frames fail visibly. Canonical masks shift with semantic ignore/binary background fill. Boxes clip at image edges; vector polygons outside bounds are skipped with an explicit count. Events/tags are never propagated. Provenance records anchor revision, target base revision, translation and registration error. This does not claim object identities through occlusion or perspective deformation.

Perceptual dHash and low-resolution brightness/sharpness are candidate-ranking aids, not proof of duplicate images or annotation quality. Full-source SHA remains authoritative. Indexing is explicit, cancellable and one image at a time; errors are retained and shown. Dashboard aggregates metadata rather than decoding all masks during render.

Snapshots store immutable manifests and content-addressed annotation JSON objects in SQLite, so existing project backup/restore includes all frozen labels. They record source hashes and dimensions rather than duplicating all images. Frozen dataset export rechecks source hashes and exports canonical images, exact semantic/region PNGs and lossless annotation JSON; it never substitutes newer live labels. Stable manifest content determines the snapshot ID; display name and creation time do not change identity. Repeating unchanged inputs returns the same version. Export publication uses verified staging/rename and never overwrites an older export. Missing/changed sources fail rather than silently changing a version.

No destructive snapshot/media deletion is added. Existing models and human review remain independent. Performance reports must distinguish operation benchmarks from end-to-end desktop latency and state all unmeasured targets.
