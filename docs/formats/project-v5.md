# Project schema 5 — M4 road review

Manifest format stays 1. Migration 0005 backs up schema 4 using the existing transactional migration runner, then adds:

- `trips`: stable UUID, name, route/camera metadata, optional measured distance, notes and revision.
- `videos.trip_id` / `trip_revision`: video-owned assignment and concurrency token. Frames inherit assignment without copying trip IDs into each frame's metadata.
- `media_review_metadata`: per-image capture fields, optional image trip/GPS pair and optimistic revision. Existing `media_tags` stores allowlisted hard-negative, weather, light and occlusion tags.
- `review_predictions`: immutable canonical prediction JSON, source identity, base document revision, model summary/event and pending/accepted/rejected audit fields. Acceptance updates the normal document and this status/operation/revision in one transaction.
- `review_comparisons`: derived numeric summaries by prediction key and GT revision, with an index for disagreement filtering. It is rebuildable and never used as a source of labels.
- Summary columns on `ai_suggestions`, backfilled once from existing JSON; a covering index supports existing M3 lists without parsing masks. `review_prediction_index` unifies M3/M4 keys for read-only review.

All existing media IDs, stable classes/layers, annotation revisions, image events, masks, video frames, operation logs and suggestions remain intact. Schema 1/2/3 migrations still run in order. Older applications reject schema 5; there is no automatic downgrade. Keep the complete project before upgrading. The pre-migration DB backup does not include edits made later.

Project backup/restore already covers all new authoritative data through SQLite plus existing mask assets. Original images/videos and model files are unchanged. Trip groups are resolved as `trip:<uuid>` before free-form groups; changing only tags does not alter an existing free-form split group. Dataset export uses the authoritative group for every media record. Explicitly unassigning a trip resets the group to `image:<uuid>` or `video:<uuid>`; it does not restore an older free-form group.

## Canonical prediction JSON v1

See [fixture/example](../../tests/fixtures/review/road-prediction.json). It is synthetic test data, not trained output; replace its sample UUID/hash with an imported image's actual identity. Dataset `manifest.json` now includes additive `sourceSha256` beside each image's `id`, `width` and `height`; this is the original source-file hash, not the exported canonical PNG checksum. Required fields:

- `schemaVersion: 1`, project `profileId`, existing `mediaId`, original `sourceSha256`, exact canonical `width` and `height`.
- `model`: stable `id`, human-readable `name`, `version`, optional model `sha256` and confidence in `[0,1]`.
- `eventLabel`: positive/negative/uncertain/null, independent of regions and objects.
- `annotations`: standard canonical shapes with UUIDs, stable layer/class IDs and legal geometry; import normalizes source to `model_suggestion` without changing GT. Region vectors cannot be boxes.
- `masks`: unique layer IDs, exact W×H, alternating value/count row-major `runs`. Semantic values are **profile train IDs**, including ignore 255, not M3 model channel indices or display colors. Region masks are binary 0/1. Missing layers mean unspecified, not background.

Files are UTF-8 (optional BOM), bounded to 32 MiB; each raster has at most one million RLE values. Import validates namespace, dimensions, source SHA, metadata and geometry. The native picker is the only frontend path ingress and checks that the selected image matches the file. There is no Python plugin, model execution or network request during import. Stored prediction review history displays the latest 100 entries per image.

## Review export

Each export creates `LabelFlow-review-<uuid>` through a temporary staging directory. `metrics.json` records policy, per-model confusion counts/recall/precision, per-class intersection/union/IoU, aggregate layer IoU and explicitly limited distance statistics. `metrics.csv` is an image-level summary; formula-like string values are escaped for spreadsheet display. JSON remains the exact metadata source.

`predictions/<key>.json` uses the importable format above. `ground-truth/<key>.json` contains the separate human document, capture metadata and numeric comparison. `checksums.json` covers all exported files. Full project backup/restore, not prediction import, restores human GT snapshots. Cancel/failure never publishes a partial destination or changes older exports; diagnostic staging is retained. Jobs are limited to 100,000 predictions and process full masks one image at a time.

Semantic comparison ignores human 255 pixels and scores model 255 as abstention/error on other human pixels. Region IoU scores foreground; empty-vs-empty foreground IoU is null, not fabricated as 1. Missing GT is unscored. Draft/uncertain rows are retained but excluded from the corresponding aggregate metric. Exact difference counts remain available even if a difference overlay exceeds the one-million-run display budget.
