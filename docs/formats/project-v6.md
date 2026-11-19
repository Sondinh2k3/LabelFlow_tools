# Project schema 6 — advanced workflows

Manifest format remains 1. Transactional migration `0006_advanced_workflows.sql` backs up the previous database, preserves M0–M4 records and adds:

| Table                | Contents                                                                                                                                                            |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `media_fingerprints` | One rebuildable result per media/source hash/algorithm: dHash, preview brightness/sharpness, last error and scan time                                               |
| `propagation_links`  | Prediction/track/anchor identity and revision, source SHA, target, translation, registration error/margin, skipped-shape count, explicit verification revision/time |
| `snapshot_objects`   | SHA-256 keyed immutable complete annotation JSON, including exact RLE masks and instance geometries                                                                 |
| `dataset_snapshots`  | SHA-256 keyed immutable manifest, display name, count and creation time                                                                                             |

Snapshot UPDATE/DELETE triggers reject accidental mutation; no application delete endpoint is provided. Fingerprints are cache, snapshots and propagation audit are authoritative data. All are covered by the existing SQLite + external live-mask backup/restore. Snapshot masks are inline lossless RLE in frozen objects, so they do not depend on subsequently changed live-mask files. Original images/videos are not copied into the snapshot database. Older apps reject schema 6; downgrade is not supported. Migration backups do not contain later edits.

## Propagation prediction provenance

Canonical review JSON v1 gains optional `propagation`. Ordinary M4 JSON without it is unchanged. Files with this field require M5; an older importer will reject the unknown field. It contains `trackId`, `anchorId`, `anchorRevision`, `anchorSha256`, integer canonical `dx`/`dy`, `meanError`, `matchMargin` and `skippedShapes`. Model ID is reserved as `labelflow.translation-v1`, version `1`; missing/mismatched provenance is rejected.

Reimport into the matching project/image validates the current approved anchor, same-video frame geometry/timestamps and source identity, and resets visual verification. Verification is not a portable claim attached to a JSON file. Anchor edits, target edits, changed sources, approved targets, rejected or accepted predictions block verification/acceptance. Accept uses the existing atomic GT/operation/review save and leaves `needs_review`; reject never changes GT. Semantic fill is ignore 255, binary fill is 0. Existing human semantic pixels, regions and independent event labels are protected by the M4 acceptance rules.

The algorithm registers each target directly against the approved anchor using a grayscale image no larger than 160 pixels on its longest axis. Integer downsample factors map offsets back to canonical pixels; registration is coarse at high resolutions, not subpixel optical flow. Search radius is at most 12 preview pixels. A common interior sample window compares all offsets; low texture, ambiguous alternatives, error above 32 grayscale levels, boundary matches and incompatible frames fail visibly. Targets must be within 2 seconds, same video and canonical size; at most 10 per request, 40 eligible neighbors shown. Boxes clip; out-of-bounds vector polygons are skipped with a count. Mask holes/ignore survive translation. Events, notes, tags, groups and legal judgments are never propagated. No object-identity or occlusion tracking guarantee is made.

## Fingerprints and dashboard

`dhash9x8-luma64-v1` computes a 64-bit row-major left/right comparison on a 9×8 grayscale preview. Brightness is the mean of a 64×64 preview; sharpness is its interior discrete-Laplacian variance. Small-preview sharpness below 12 is only a low-detail candidate flag. Index jobs check source SHA, reuse successful results only for unchanged sources and otherwise decode one image at a time. Cached errors are retried. Force rebuild recomputes all previews. Up to 100,000 images per index/rank job; ranking returns at most 100 candidates, sorted by Hamming distance, source-hash equality, brightness difference and media UUID. Scope can be project or authoritative split group. These are last-scan results; a file changed outside LabelFlow is not detected until checked again.

Dashboard counts human review states, independent events, tags, groups/trips, index coverage/errors, low-detail candidates, pending predictions/propagation and versions. It does not infer annotation accuracy, remove duplicates, modify splits or replace the existing export quality validator. An identical perceptual hash is not proof of identical pixels.

## Immutable dataset versions

Create freezes the current approved-only/all selection, complete profile, trip definitions, per-image capture metadata, source SHA/dimensions/orientation, video provenance, exact annotation object hash, authoritative group and seeded split. Media/trips sort by UUID, tags sort by key; JSON uses deterministic serialization. Display name and creation time are outside content identity. The SHA-256 of `manifest.json` is the version ID. The exact same state/options in the same project returns the existing version; editing labels, metadata, source references or split settings creates another version. Object blobs deduplicate by exact JSON hash. This is reproducibility of exact state within a project, not semantic deduplication across independently imported projects.

Creation requires 1–100,000 eligible images, a name ≤160 bytes and seed ≤128 bytes. Each annotation object is ≤64 MiB. One transaction covers all frozen objects and the version; cancellation/failure rolls it back. No partial version or orphan object is published. Each verify checks object/manifest hashes, IDs, dimensions, group/video non-leakage and deterministic split membership. Optional source checking hashes the linked originals. Repaired media paths can be used on export if current media identity/hash/dimensions still match the frozen entry. The frozen manifest retains its original source reference for audit.

Reusing a content hash also compares its stored bytes; an already corrupt object/manifest is rejected instead of reused. Verification requires the immutable built-in profile definition to match exactly, and export restricts paths to normal relative components. A forged profile/layer path cannot escape the new staging folder.

Export creates a new `LabelFlow-snapshot-<id-prefix>-<uuid>` folder using verified staging/rename:

- `manifest.json`, `snapshot-id.txt`, `profile.json`.
- `images/<split>/<media-uuid>.png`: canonical source pixels, rechecked against original SHA before decoding.
- `annotations/<media-uuid>.json`: frozen full document, never the newer live document.
- `masks/<layer>/<split>/<media-uuid>.png`: lossless 8-bit semantic train IDs or independent binary regions. Only explicitly present dense/vector region layers are emitted. Missing dense labels remain absent, not fabricated negative masks; an explicit all-zero region mask still exports.
- `checksums.json`: exact hashes for all other exported files.

Instance boxes/polygons/holes/RLE remain lossless annotation JSON; this frozen format does not claim direct COCO/YOLO training compatibility. Source media stay linked until export. Missing/changed sources fail; no newer live image or label is substituted. Cancellation/failure preserves older versions/exports and may leave a diagnostic staging folder. Backup/restore restores full project state; snapshot export is a frozen dataset format, not a live-project restore operation.
