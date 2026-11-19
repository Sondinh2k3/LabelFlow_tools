# M1 schema 2 and interchange

The portable manifest remains `formatVersion: 1`. SQLite is now schema **2**. Opening an M0 schema-1 project first creates `.labelflow/backups/schema-1-<uuid>.sqlite` through SQLite online backup, then rebuilds media and creates annotation/class/layer/job tables inside one migration transaction. FK checks precede commit. Media IDs, paths and active-image references survive unchanged. M0 cannot open schema 2; use a copy of the migration backup with its matching original manifest if rollback is needed. There is no downgrade operation.

```text
Project/
  .labelflow/
    project.json, project.sqlite, project.lock
    masks/<sha256-of-PNG>.png       # authoritative, immutable
    thumbnails/<media-UUID>.png     # rebuildable cache
    backups/, logs/
  media/<source-sha256>.<extension> # optional managed originals
```

`0002_manual_labeling.sql` adds class namespaces/stable keys/export IDs, independent layers, normalized annotations, mask assets, image events/tags, split metadata, background jobs, canonical document snapshots and revisioned annotation operations. All writes use foreign keys and transactions, WAL and FULL synchronous mode. Before/after operation snapshots retain mask references; old assets must not be treated as disposable cache.

Vectors store floating-point canonical source pixels. Bboxes are positive rectangles with half-open coverage. Polygons store exterior vertices and zero or more holes; the initial UI draws/edits exterior rings, while persistence and validation preserve holes. Backend rejects self-intersections, intersecting holes, invalid classes and out-of-bounds geometry. Raster dimensions must exactly match EXIF-normalized source dimensions. The IPC raster encoding is alternating `[pixelValue, count, ...]`; PNG is authoritative on disk.

Cityscapes train IDs, immutable in `road-encroachment-v1`:

| IDs   | Classes in order                                       |
| ----- | ------------------------------------------------------ |
| 0–4   | road, sidewalk, building, wall, fence                  |
| 5–9   | pole, traffic light, traffic sign, vegetation, terrain |
| 10–14 | sky, person, rider, car, truck                         |
| 15–18 | bus, train, motorcycle, bicycle                        |
| 255   | ignore / uncertain                                     |

Candidate objects use a different namespace: 0 material_pile, 1 goods_cluster, 2 street_stall, 3 cart, 4 standalone_object. Generic projects have candidate class 0 object. Region layers (`road_extent`, `traffic_exclusion`, `suspected_region`) have no semantic class ID and use binary 0/1 masks. Eraser writes 255 in semantic layers and 0 in region layers. Semantic class 0 is road, **not** background.

Exports create `LabelFlow-export-<uuid>` under the selected parent:

- `images/{train,val,test}/<UUID>.png`: canonical pixels, independent of original EXIF metadata.
- `labels/{split}/<UUID>.txt` and `data.yaml`: YOLO detection only, normalized coordinates with 10 decimal places. Empty images have empty label files. Instance polygons are excluded with explicit warnings; no automatic polygon-to-box conversion.
- `masks/<layer>/{split}/<UUID>.png`: lossless 8-bit grayscale IDs. Semantic defaults to 255; absent binary region masks default to 0. Region polygon coverage uses pixel centers and union within that region only.
- `classes.json`: full profile mapping; `manifest.json`: dimensions, source reference, document revision, event/notes/review state, split/group, warnings, seed/ratios and SHA-256 checksums.

Splits hash seed + explicit split group; the source folder is the image-import default group. They are reproducible and never divide a group. Ratios describe desired group assignment, not guaranteed exact image percentages. Review empty-split warnings before training. The M1 format is not certified against a particular PIDNet loader.

YOLO detection import requires existing imported images and explicit mapping of every source class. Choose the labels directory or a dataset root with `labels/`; files match media UUIDs or unique source basenames. Semantic/region import accepts 8-bit grayscale or indexed PNG, requires matching dimensions and explicit mapping of every present ID. RGB, implicit resizing and unmapped IDs are rejected. Approved images are preserved; deliberately change review state before importing replacement labels.

**Backup DB + masks** uses online SQLite backup, copies all registered mask assets and the project manifest, verifies checksums and publishes `backup.json` last. Restore verifies that descriptor and requires a destination without `.labelflow`; it never overwrites a project. Backups retain absolute references to original media locations, including managed media in the original project. They are annotation backups, not a complete portable media archive: back up/copy originals separately before relocating or deleting a project.

After a crash, the OS lock releases automatically. Do not delete `project.lock`. On launch, outbox commands replay idempotently and interrupted jobs are marked failed with restart guidance. Asset inspection reports missing/corrupt files and preserves orphan PNG/staging files; no automatic deletion or speculative replacement. Linked-source remapping and a dedicated recovery management UI belong to later work.
