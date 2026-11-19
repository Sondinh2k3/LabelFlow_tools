# Project schema 3 and interoperability

Container format remains `formatVersion: 1`; SQLite `user_version` and `projects.schema_version` become 3. Opening schema 1/2 makes a `schema-<old>-<UUID>.sqlite` backup before migrating. Preserve the complete project, including `.labelflow/masks`, before using an older application: schema 3 is intentionally refused by schema-2 binaries.

`videos` stores source path/hash, codec, raw dimensions, right-angle rotation, integer time-base numerator/denominator, first PTS, duration, average FPS estimate, frame count and split group. `video_frames(video_id,frame_index)` stores exact integer PTS, normalized display time in milliseconds and frame duration. Index is zero-based presentation order, including B-frame reorder. Display time is `(PTS - first_PTS) * 1000 * num/den`; the original PTS is never reconstructed from FPS.

Extracted image records reference `videos(id)` and retain frame index, PTS, time in ms, effective group, canonical dimensions and JSON extraction settings. Frame files reside in `media/video-<UUID>/frame-<index>.png|jpg`. Duplicate `(video,index)` means the original existing media and annotation identity, even if a later extraction requests another format. To preserve labels, re-extraction does not replace an existing frame.

Canonical geometry wire discriminants:

- `bbox`: x/y/width/height, original canonical pixels.
- `polygon`: `points` exterior and `holes` rings.
- `multiPolygon`: `polygons` array of exterior/holes objects; raster coverage is union of components.
- `mask`: full-image width/height, binary value/count `runs` (row-major), `iscrowd` boolean. Stored document replaces runs with immutable PNG asset references carrying `annotationId`; load restores exact binary runs. This is distinct from semantic `masks[]` and uses no ignore value.

COCO RLE uses background-first alternating counts in **column-major** order. Both uncompressed count arrays and the official signed/delta compressed string are accepted; coverage must equal W×H. Imported masks preserve crowd state. COCO polygons are independent components, never interpreted as holes. Explicit repeated closing vertices are normalized. Unknown IDs, ambiguous filenames, mismatched dimensions, invalid geometry and crowd vectors lacking RLE are rejected.

Import source category IDs can differ from project IDs; the mapping dialog displays source ID/name and requires a target stable class. Already-approved images are skipped with a warning. Repeating identical imports is idempotent; edited/remapped input is a new import. Matching requires images imported first. No arbitrary paths from JSON are read.

COCO exports `annotations/instances_train.json`, `instances_val.json`, `instances_test.json`, with canonical PNGs referenced relative to export root, stable exported candidate category IDs 0..4, bbox, area, crowd state and segmentation. `labelflow` metadata retains canonical annotation ID, layer and provenance. Holes and instance masks use compressed RLE. Detection-only bboxes have no segmentation.

YOLO segmentation exports one normalized polygon row per supported instance. Unsupported geometry fails unless `skipUnsupported` is explicitly true; then the report/manifest lists skipped annotation IDs. It is not silently approximated. YOLO `data.yaml` omits a misleading `path: .`, allowing loaders to resolve train/val/test relative to the YAML file's location. Class mapping is fixed by stable project profile; custom class editing remains deferred.

All exports include canonical images, class/layer mapping, source-video/PTS metadata per image, split membership, warnings and checksums. Semantic/region sidecars remain separate PNGs with train IDs/ignore 255 or binary 0/1. Region rasterization unions each logical layer's components with its own existing raster; it never modifies semantic classes. Export staging is verified then renamed into a fresh destination. Failed staging remains for diagnosis and does not publish over a previous export.

Backups include DB and authoritative masks; original videos and images must be backed up separately. Linked source repair requires exact original SHA-256 (plus canonical dimensions/orientation for images), preserving IDs, PTS, masks, revisions and annotations. Preview caches are disposable and excluded from authoritative backup requirements.
