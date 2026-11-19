# LabelFlow — Codex Master Implementation Plan

> Status: Approved architecture baseline / implementation specification
> Language: Vietnamese (technical identifiers in English)
> Product: offline, desktop-first computer-vision labeling software for Windows and Ubuntu.
> Primary use case: road-encroachment dataset (PIDNet-S semantic segmentation, Road Extent, traffic exclusion, suspected regions, optional detection).

## 0. Instructions for Codex (READ FIRST)

You are the implementing engineer. Work milestone-by-milestone. Do **not** attempt to deliver the entire product in one unreviewed code generation pass.

1. Inspect the existing repository before changing anything. If empty, initialize the project using the stack below.
2. Read this document in full; build an explicit checklist from the acceptance criteria.
3. At each milestone: implement production code, tests, sample fixtures, README and migration notes; run checks; summarize changed files and limitations.
4. Do not silently reinterpret annotation semantics or supported export formats.
5. No mandatory account, telemetry, cloud sync, API server, or network connection.
6. Original media must remain unchanged. Any operations that can remove data must require deliberate confirmation.
7. Build predictable keyboard/mouse interactions and correct coordinate mapping before UI polishing.
8. Default to minimal dependencies. Audit dependency licenses; do not embed third-party model weights or FFmpeg binaries without verifying distribution rights.
9. Prefer a simple working implementation to a complex incomplete abstraction. Keep extension interfaces for later AI/video features.
10. If a requirement needs input, adopt the specified default; record any deviations in `docs/decisions/`.
11. Never claim benchmarks that have not been measured. Run repeatable tests on the target machine.
12. After each milestone, STOP and report to the user for review before proceeding to the next milestone, unless the user explicitly authorizes continuous execution.

## 1. Product boundaries

### 1.1 Goals
- Fast local annotation of images and selected video frames, with detection bounding boxes, semantic segmentation, and instance segmentation.
- Robust editing: box, polygon, raster brush/eraser, layer and class selection, undo/redo, autosave, zoom/pan, keyboard shortcuts.
- Usable with tens of thousands of media records without loading all media at startup.
- Reliable imports/exports and image-level validation before training.
- Project profiles; built-in `Road Encroachment` profile is a configuration, not hardcoded into core data structures.
- Optional AI prelabeling integrated as a separate worker after manual workflows are proven.

### 1.2 Explicitly out of scope for MVP
- Cloud, sharing, accounts, multi-user editing, browser access, collaboration server.
- Training neural networks in the app.
- Full video object tracking, optical flow, mask propagation and active learning.
- Automatic legal adjudication of whether road encroachment has occurred.
- Real-time Jetson deployment; LabelFlow runs on development computers, not required on the edge device.

## 2. Technology choices

- Tauri 2 (Rust backend) + React + TypeScript + Vite.
- Tailwind CSS for UI; lightweight state store (Zustand if beneficial), no heavy multipurpose component framework by default.
- SQLite via Rust `sqlx` or `rusqlite`, with migrations, foreign keys, WAL (when appropriate), and transactions. Frontend must not issue arbitrary SQL.
- Rendering: HTML Canvas2D as initial baseline. A layered renderer with original image, vector overlays, raster mask overlay, handles, and temporary interaction layer. Evaluate WebGL only if profiled bottlenecks justify it.
- Rust services for filesystem, image metadata, thumbnail generation, project management, import/export, validation and background jobs.
- FFprobe/FFmpeg (user-installed or permission-compliant packaged binary) via Rust subprocess for video metadata and frame extraction. No shell string interpolation; pass argument vectors.
- Optional Python AI worker, launched locally by Tauri. Define versioned JSON-lines IPC over stdio with request IDs and cancellation. No localhost port required.
- Unit tests: Vitest (TS) and Rust tests; Playwright/Tauri desktop smoke tests where feasible.
- Development target: Windows x64 first; Ubuntu x64 next. Cross-platform paths, font scaling and file permissions tested.

## 3. High-level architecture

```
React Desktop UI
  -> Workspace/Canvas/Media Browser/Timeline/Review/Settings
  -> Typed frontend services + ViewState
  -> Tauri invoke() / typed event contracts
Rust backend
  -> ProjectService / MediaService / AnnotationService
  -> ExportService / ImportService / QualityService / JobService
  -> SQLite project DB + files (source links, thumbnails, masks)
  -> Background workers (thumbnail, decode, export, frame extraction)
Optional AI worker
  -> model adapters (ONNX segmentation, PIDNet, YOLO, prompt-based masks)
  -> predicts candidate annotations, never writes approved labels directly
```

Boundaries:
- Canvas draws viewport images and annotation overlays; DB/file IO never executes directly in React render cycle.
- Annotation geometry transformations are pure TypeScript libraries, unit-tested independently of UI.
- Backend validates all records, paths, dimensions, class references and transaction consistency.
- Background jobs communicate progress through events, include cancellation and failure recovery.

## 4. Proposed repo structure

```
labelflow/
  README.md
  package.json
  src/
    app/                 # app shell, routes, settings, command map
    features/projects/
    features/media/
    features/annotations/
      canvas/            # renderers, viewport, hit tests
      tools/             # select, bbox, polygon, brush, eraser, pan
      geometry/          # pure transforms, clipping, contours, polygon tests
      history/           # undo/redo commands
    features/classes/
    features/video/
    features/review/
    features/export/
    features/quality/
    features/ai/
    components/
    lib/contracts/       # typed Rust IPC contracts
  src-tauri/
    src/
      commands/
      services/
      database/
      repositories/
      domain/
      jobs/
      export/
      import/
      video/
      ai/
      security/
    migrations/
    capabilities/
  tests/fixtures/
  docs/
    architecture/
    decisions/
    formats/
    milestones/
    benchmarks/
```

## 5. Annotation semantics — non-negotiable

### 5.1 Image coordinate convention
- Store all geometry in **original image pixel coordinates** (float for vector shapes). Origin at top-left, x increases rightward, y downward.
- Rectangle: `(x, y, width, height)`, positive width/height. Rectangle covered area uses half-open bounds `[x, x+w) × [y, y+h)`.
- Masks have integer dimensions **exactly** matching the source image at export. No implicit resizing.
- Canvas view transform: `screenPoint = imagePoint * zoom + pan`; invert for pointer input, accounting for CSS pixels and device pixel ratio.
- Crop/display/EXIF orientation must be normalized consistently. Record original file orientation and canonical decoded pixel dimensions; exported masks align with canonical image exported to training dataset.
- Clip shapes to legal image bounds; avoid off-by-one rounding at borders.

### 5.2 Annotation types
- `bbox`: category + rectangle; one object instance.
- `polygon`: category + exterior ring and zero or more holes (canonical model supports holes even if initial editor UI does not).
- `raster_mask`: category + lossless binary mask asset, bounding rectangle, origin and revision.
- `semantic_layer`: a mutually exclusive per-pixel class-ID raster with explicit `ignore_id` for unlabeled/ambiguous pixels.
- `region`: geometry + purpose (e.g. `road_extent`, `suspected_region`), independently of semantic class IDs.
- `image_event`: image/video-level categorical assessment (`positive`, `negative`, `uncertain`) and optional notes.
- `source`: `manual`, `imported`, `model_suggestion`, `model_approved`.
- `review_status`: `unlabeled`, `in_progress`, `needs_review`, `approved`, `rejected`.

Important: Semantic segmentation classes and instance segmentation objects cannot simply be treated as interchangeable. Separate data model and export paths are required.

### 5.3 Semantic layers / overlaps
- Named layers may overlap (e.g. `road_extent` and `traffic_exclusion`), whereas a particular exported semantic class-ID mask represents **one** class or ignore value per pixel.
- Define layer stacking and explicit semantic conflict resolution. Do not silently convert overlapping pixel regions into class IDs by screen z-order.
- Default semantic raster classes for Cityscapes are **train IDs 0..18**, with ignore value **255**, using a documented immutable mapping. Do not confuse raw Cityscapes label IDs with train IDs.
- Keep `road_extent` separate from visible `road` segmentation. Road Extent is an inferred corridor annotation that can include occluded road; never auto-relabel it as Cityscapes `road`.
- `traffic_exclusion` is a separate editable region/mask (possibly generated from person and vehicle predictions). Do not assume a single `vehicle` class covers every relevant traffic category.
- `suspected_region` describes algorithm suspicion, not confirmed unlawful encroachment; maintain human event labels separately.

### 5.4 Dataset exports
- YOLO detection: each row `<class cx cy w h>`, normalized to `[0,1]`, per-image `.txt`.
- YOLO instance segmentation: each object row `<class x1 y1 x2 y2 ...>`, normalized polygon points, min 3 vertices; holes and disconnected components require explicit supported approximation or an export warning, never silent data loss. If lossless conversion impossible, mark export failed for that annotation or require explicit override.
- COCO detection/instance segmentation: category IDs, image dimensions, bboxes, segmentation polygons or RLE as appropriate; validate JSON references and geometry.
- Semantic segmentation: lossless indexed/8-bit PNG when <=255 class IDs, exact W×H; `ignore_id=255`, plus class mapping and train/val/test manifest.
- Road extent/suspected/exclusion: binary PNG per logical region/layer, separate from Cityscapes semantic mask; provide schema manifest.
- All exports write to temporary destination then verify manifest/checksums and atomically publish or use safe rename strategy; do not corrupt prior export on failure.
- Export report documents warnings, class mapping, skipped records and split membership.
- Check third-party training repository's expected mask semantics before claiming direct compatibility with a PIDNet training configuration.

## 6. Project format and data safety

### 6.1 Example portable layout

```
MyRoadProject/
  .labelflow/
    project.json             # format_version, UUID, profile, project settings
    project.sqlite           # authoritative annotation records, revisions
    thumbnails/              # discardable cache
    masks/                   # lossless raster assets, referenced by DB
    backups/                 # optional timestamped DB + manifest backup
    logs/
  media/                     # optional copied/ingested assets
  exports/                   # user-controlled output path
```

- Support **linked** external media by default and **copied/managed** media as a user choice. Document that linked sources can become unavailable. Do not overwrite source files.
- Store media paths relative to project where possible; for external linked sources, keep resolvable URI/path and a remap workflow.
- Project is single-writer: acquire a lock; on another running instance prompt read-only or reject, do not risk concurrent writes.
- Thumbnail cache is rebuildable; annotations and masks are not cache and must be backed up together.
- Recover from missing source files, broken media references, corrupted cache and interrupted thumbnail jobs gracefully.
- Filesystem changes must be tracked with DB transactions and safe file staging; reconcile orphan mask files and missing referenced masks at reopen.
- Database migrations require schema versioning and backup before risky migration.

### 6.2 Minimum relational schema

`projects(id, name, profile_id, schema_version, created_at, updated_at)`
`media(id, project_id, type, path, import_mode, content_sha256, width, height, orientation, timestamp, gps_json, source_video_id, frame_index, frame_time_ms, split_group_id, review_status, created_at)`
`classes(id, project_id, namespace, stable_key, export_id, display_name, color, is_ignore, sort_order)`
`layers(id, project_id, stable_key, layer_type, display_name, visible, opacity, export_role)`
`annotations(id, media_id, layer_id, class_id nullable, geometry_type, geometry_json, mask_asset_id nullable, source, confidence nullable, review_status, revision, created_at, updated_at)`
`mask_assets(id, relative_path, sha256, width, height, encoding, created_at)`
`image_events(id, media_id, event_type, notes, reviewed_at)`
`media_tags(id, media_id, tag)`
`dataset_splits(media_id, split_name, split_group_id, assignment_method, seed)`
`jobs(id, job_type, params_json, status, progress, error, created_at, updated_at)`
`annotation_operations(id, media_id, annotation_id nullable, operation, before_json, after_json, created_at)`

Implement appropriate CHECK constraints, FK constraints, indices, timestamps, and unique IDs. This is a conceptual schema; normalization can improve it, but not weaken guarantees.

### 6.3 Autosave and recovery
- UI changes immediately update view state and schedule persistence via a bounded debounce (~300–600 ms). Save immediately on image change/window close when possible; never rely on close event alone.
- Use a write queue with operation ordering; carry revision and reject stale updates.
- Maintain a small durable local journal or transactional operation log so a crash during pending debounce does not silently lose edits; reconcile on next launch.
- Bulk brush strokes are committed as one undoable command upon pointer release; long strokes can checkpoint internally if needed.
- Revision/conflict errors should surface visibly. Never silently overwrite a newer revision.
- Autosave status: `Saving...`, `Saved`, `Failed – retry`; retry with bounded backoff, prevent navigation-based data loss.
- Backup/restore tests include database **and** externally stored masks.

## 7. Annotation engine details

### 7.1 Canvas stack
1. Image bitmap layer (fit, zoom and pan; use `createImageBitmap`/equivalent where reliable).
2. Semantic raster overlay layer (adjustable opacity).
3. Region/instance vector outline layer.
4. Selection handles and guides.
5. Temporary tool preview/cursor layer.

For performance, use incremental redrawing and image thumbnail preloading. Avoid React component per vertex, brush pixel or mask pixel. Keep viewport controls separate from persistent image geometry.

### 7.2 Tools and acceptance

**Select**: select shapes via click/hit test; move and resize boxes; move polygon vertices; delete selected annotation with confirmation policy; multi-select later.
**Bounding box**: drag in any direction, normalized rectangle; minimum size threshold; Escape cancels; classify by current active class.
**Polygon**: click vertices, close by first vertex or Enter; Backspace removes last vertex; Escape cancels; validation rejects <3 unique points / invalid shapes. Start with non-self-intersecting simple polygons; warn on self-intersection.
**Brush / Eraser**: pressure-independent circular footprint with size control; binary/class-ID edits on raster layer, zoom-aware cursor; preserve masks losslessly. Editing overlap semantics explicitly follows active layer rules.
**Pan/Zoom**: space-drag pan, Ctrl+wheel or wheel zoom per settings, zoom-to-pointer, fit image, reset view; no drift across repeated transformations.
**Undo/Redo**: command-pattern per media edit session, e.g. 100 operations configurable; persist completed changes; image navigation cannot lose latest state. Clarify whether undo history persists after relaunch (not required M1).
**Class assignment**: numeric hotkeys for common classes, search palette for many classes, last-used class remembered. Class stable IDs never implicitly renumber existing annotations.
**Overlay**: toggle image/mask outlines, opacity slider; deterministic compositing; semantic ignore pixels transparent in editing, preserved as 255 in exports.

### 7.3 Default shortcuts
- `V`: Select; `B`: Bounding box; `P`: Polygon; `M`: Brush; `E`: Eraser; `H` or Space: Pan.
- `Ctrl+Z` / `Ctrl+Shift+Z` (and platform alternative where appropriate): Undo/Redo.
- `←` / `→`: Previous/next image (only when not text editing; honor pending autosave).
- `1..9`: assign/select frequently used class; `C`: open class search.
- `[` and `]`: brush radius; `0`: fit image; `Esc`: cancel current action.
- `Tab`: toggle panels; `L`: toggle annotations; `Ctrl+E`: open export dialog.
- All shortcuts centralized, configurable later; focus rules prevent triggering while editing notes/search text.

## 8. UI specification

### 8.1 Main screens
1. **Project Home** — create, open/recent, recover, import, project profile.
2. **Annotation Workspace** — left media strip/list with thumbnail and filters, central canvas and tool strip, right class/layer/annotation panels, status bar.
3. **Video Workspace** — video preview, timeline, stepping, in/out frame extraction, frame selection and de-dup suggestions (M2).
4. **Dataset Review** — progress, `unlabeled/in_progress/needs_review/approved`, filters and quality warnings.
5. **Export Wizard** — format, class mapping, data split, preview counts, validate, export location.
6. **Settings** — shortcuts, theme, cache quota, optional AI worker, media locations.

### 8.2 UX requirements
- No modal to choose class for every object. Persistent current class.
- Support drag-and-drop images/folders; show progress and warnings for corrupt media.
- Show image filename, W×H, zoom, saving status, number of annotations and current review state.
- Restore last opened project/image/session and panel widths.
- Avoid destructive buttons next to routine navigation controls.
- Dark theme first; ensure high-contrast outlines on both bright and dark imagery.
- English UI strings initially, Vietnamese localization via string catalog if desired; never hardcode strings across components.

## 9. Road Encroachment project profile

Built-in `road-encroachment-v1` defines layers:
- `cityscapes_semantic` (19 train IDs, ignore 255): editable semantic class map; defaults must be documented.
- `road_extent` (binary region/mask): estimated full road corridor, including occluded portions where inferable.
- `traffic_exclusion` (binary region/mask): road parts occupied/occluded by traffic participants that algorithm must exclude.
- `suspected_region` (binary/instance regions): suspected occupancy output; not automatically ground truth of a violation.
- `candidate_objects` (bbox/instance): optional 5-class detector experiment:
  0 `material_pile`, 1 `goods_cluster`, 2 `street_stall`, 3 `cart`, 4 `standalone_object`.
- `image_event`: `positive/negative/uncertain`, optional comment.

Metadata: camera/video ID, route/trip, frame timestamp, GPS if available, weather/light/occlusion tags, source and review status. GPS is optional and never required for labeling.

**Recommended labeling protocol**:
1. Import continuous videos/images; keep original media and preserve timestamps/trip ID.
2. Select representative frames rather than every adjacent frame.
3. Label event outcome first (positive/negative/uncertain).
4. Create a `suspected_region` for positives where confidently visible; mark ambiguous pixels/regions as ignore/uncertain.
5. For a small high-quality subset, label semantic road/traffic masks and road extent; avoid requiring all dense masks on every positive image.
6. Include hard negatives (stopped vehicles, medians, trees, roadside objects, occlusions, road damage).
7. Split by video/trip/group rather than random neighboring frames; ensure no group crosses train/val/test.
8. Evaluate independent held-out clips with event recall and false triggers per km where distance metadata is available; do not claim that a single static segmentation IoU measures the complete application.

Review Mode (M4): import model predictions as separate overlays; switch predicted/ground truth, accept/reject suggestions, inspect disagreements. Never overwrite human-approved annotations with model output.

## 10. Video subsystem (M2)

- Probe video metadata: codec, width/height, duration, frame rate estimate, rotation, timebase; timestamps take precedence over naïve FPS arithmetic for variable-frame-rate sources.
- Timeline playback, frame stepping, time seek; preview frame decode is cancellable.
- Extract as lossless PNG or high-quality JPEG with explicit user choice; preserve source video ID, frame index when available, presentation timestamp (PTS), extraction configuration, image orientation.
- Modes: manual frame select; interval extraction (seconds); motion/blur/near-duplicate ranking only after basic pipeline works.
- Visual duplicate detection can use perceptual hashing as candidate ranking, not sole guarantee that two frames are identical.
- Extraction jobs report progress/cancel/error; never freeze UI.
- Frame navigation should preload adjacent frames and maintain stable annotation links.

## 11. AI integration contract (M3)

- AI component is optional; manual annotation functions without Python/models.
- Define versioned worker handshake: `protocol_version`, `capabilities`, available models, device/provider, input geometry requirements, output format.
- Operations: `health`, `list_models`, `load_model`, `predict_boxes`, `predict_semantic`, `predict_prompt_mask`, `cancel`, `unload`.
- Request includes request ID, image path or shared local resource reference, width/height, orientation, model version and parameters; response includes coordinate convention, class map, confidence and processing metadata.
- Prediction stored as a **suggestion layer** with model name/version and confidence; user review converts to approved annotation.
- Support no-GPU CPU fallback; GPU providers must be detected, not assumed; never make the app fail to launch because AI runtime is missing.
- Start with ONNX Runtime CPU; add hardware-specific providers only after tested on target machines.
- Prompt segmentation adapters may support points/boxes; model licensing and redistribution separately reviewed.
- Do not integrate with inference on Jetson device as a dependency of desktop functionality.

## 12. Import/export and quality control

### M1/M2 import
- Import JPEG/PNG/WebP with valid canonical dimensions. Unsupported/corrupt images are skipped with itemized report.
- Import YOLO detection with explicit YAML/class mapping; convert normalized coords to canonical pixels.
- Import YOLO instance polygon data after supported semantic check; do not interpret as pixelwise semantic segmentation.
- Import COCO JSON including segmentation polygons/RLE in M2.
- Import semantic PNG masks with explicit class-ID mapping, dimension validation and ignore semantics in M1/M2.
- Provide class-conflict remapping UI rather than assigning by implicit order.

### Export wizard quality gates
- Zero-area/out-of-bounds bbox, polygon <3 points, self-intersection, missing class, invalid mask resolution, missing image, stale broken path.
- Unknown class IDs, unmapped ignore values, duplicate ID references, unsupported holes/multiple components when exporting YOLO polygons.
- Split leakage: same source video/trip group assigned across train/val/test.
- Validate export manifest: image-label pairs, hashes or counts, dimensions, class maps; run round-trip tests.
- Export images by copy/hardlink if safe and supported; avoid rewriting original data by default.
- Split ratios configurable, with seeded grouping by source video/trip or explicit group ID (never random frame-level split as default).

## 13. Performance targets (targets, NOT measured promises)

Test on defined Windows x64 reference hardware, 1080p imagery and 10k-image synthetic dataset:
- Startup: aim <2 s warm start; capture cold start separately.
- Manual labeling idle memory: aim <400 MB; include WebView/OS caveat.
- Image switch: aim <150 ms for preloaded 1080p images.
- Input interaction: aim 60 FPS for typical 20 shapes/image.
- Import 10k records: asynchronous and responsive, bounded memory.
- Autosave: no UI blocking > 50 ms under normal load; crash-recovery tested.
- No per-pixel React nodes and no duplicate 1080p uncompressed bitmaps retained without budget limits.
- Record actual p50/p95 values and machine specs in `docs/benchmarks/`.

## 14. Security, reliability and packaging

- Tauri capabilities least privilege; never enable blanket file system or process-shell permissions to frontend.
- Paths validated and canonicalized; work only on project/user-approved directories. Guard path traversal in ZIP/dataset import.
- Limit decompression bombs / gigantic image dimensions; decode and thumbnails on bounded worker queues.
- External FFmpeg launch by fixed executable path + argument array, never interpolated shell command.
- Project lock and clear recovery mode on unexpected termination.
- Periodic backup with retention policy, integrity check on open, and user-controlled restore.
- Packaging: MSI/NSIS (Windows) and AppImage/deb as supported (Ubuntu). Installer does not bundle AI models; verify licenses for shipped dependencies.
- No automatic network calls by default; optionally permit manual release checking only if separately approved.

## 15. Roadmap with enforceable acceptance criteria

### M0 — Foundation and technical spike
Deliver:
- Initialize Tauri 2 + React + TS + Rust; strict TypeScript, lint, formatting, Rust Clippy.
- SQLite migrations and typed Tauri command contract.
- Basic image canvas proof-of-concept (open one 1080p image, correct zoom/pan), project open/create.
- CI/local scripts for unit, typecheck and build.
Accept only when:
- Fresh clone builds and opens on Windows; Ubuntu build instructions documented.
- Canvas transforms round-trip accurately in test fixtures.
- DB migrations and project lock behave correctly.

### M1 — Usable manual labeling MVP (HIGHEST PRIORITY)
Deliver:
- Image/folder import, thumbnail grid, navigation and filters.
- Bbox, polygon, brush, eraser, select, zoom/pan, class/layer palette, undo/redo.
- Separate semantic mask and region layer support, mask assets, autosave and crash recovery.
- YOLO detection export and semantic PNG export; basic import/round-trip tests.
- Road profile baseline, event label, review status.
Accept only when:
- Label 100 real test images end-to-end offline without losing annotations.
- Close/reopen app; verify exact restored annotations and referenced masks.
- Pixel-aligned semantic PNG matches image dimensions and class IDs.
- YOLO detection boxes round-trip within defined tolerance of <1 pixel for 1080p fixtures.
- 100 automated pure geometry/serialization assertions or equivalent meaningful suite; all tests green.
- No freeze under representative brush strokes and media navigation.

### M2 — Video workflow and dataset interoperability
Deliver:
- FFprobe/video timeline/manual and interval frame extraction.
- Source video/PTS metadata and grouping for splits.
- YOLO segmentation + COCO detection/segmentation import/export, PNG semantic import.
- Dataset filters, approved/rejected review flow, QA validation and grouped split wizard.
- Missing file repair and large dataset incremental import.
Accept only when:
- Extract/select frames from several MP4 videos (including VFR fixture if available); timestamps accurate.
- COCO polygon/RLE and YOLO segmentation round-trip on representative valid fixtures; unsupported geometry explicitly warns.
- Train/val/test split contains no overlapping source-video groups.
- Export produces valid files accepted by parser/loader of selected training pipeline.

### M3 — Optional AI assistance
Deliver:
- Python worker management + versioned IPC + cancel/restart.
- Import predictions as suggestions, overlay/approve/reject UI.
- First ONNX CPU model adapter; PIDNet semantic model adapter after actual output schema validation.
- Optional click-to-segment adapter only when a supported model is selected.
Accept only when:
- App works identically without Python/models installed.
- AI failure cannot corrupt manual annotations; cancellation leaves consistent state.
- Wrong class maps or dimensions reject prediction safely.
- No model prediction can overwrite approved manual annotation without explicit user action.

### M4 — Road encroachment expert workflow
Deliver:
- Road extent, traffic exclusion, suspicious region and independent event-level assessment.
- Model-vs-ground-truth review screen and disagreement filtering.
- Hard-negative tags, trip-based dataset management, metrics export.
Accept only when:
- Road extent and visible road masks remain separate across save/export/import.
- Positive/negative/uncertain labels and uncertain masks survive all round-trips.
- Predicted regions and approved ground truth displayed and stored independently.

### M5 — Optimization and advanced workflows
Deliver:
- Profiling-driven render/brush optimizations, optional WebGL.
- Video mask propagation/tracking suggestions, near-duplicate ranking, quality dashboards.
- Dataset version snapshots with reproducible split manifests.
Accept only when:
- Benchmarks measured p50/p95 against M1 baseline; speed or resource improvements documented.
- Label propagation remains suggestions and is verified before approval.
- Dataset snapshots are reproducible and non-destructive.

## 16. Test matrix

**Unit:** screen/image transforms, DPR mapping, clipping, bbox normalize/denormalize, polygon validation, raster brush updates, alpha compositing, serialization, class remap, split grouping.

**Integration:** create/open project, migrate/backup, import image, save annotation+mask, undo/redo, reopen/recover, export/import roundtrip, missing paths, corrupt images, process crashes.

**End-to-end:** keyboard-first bbox flow, polygon flow, brush flow, review states, project reopen, video extract, batch export, user-visible progress/cancel.

**Edge cases:** portrait images/EXIF, huge images, Unicode and Vietnamese filenames, Windows paths, relative/linked assets, interrupted copy, low disk space, invalid class map, mask with holes, empty background image, overlapping layers, duplicate frames, 0-byte media, repeated import, VFR video, read-only project directory.

**Manual evaluation:** 20 representative video frames from Vietnamese roads, positive/hard-negative cases, heavy occlusion, normal vehicles stopping, poorly visible road, uneven lighting and rough surface.

## 17. Deliverable format per milestone

Each milestone PR/update must contain:
1. Features implemented and demo usage instructions.
2. Changed-file list and architecture decisions.
3. Database migrations and compatibility notes.
4. Test commands and pass/fail counts.
5. Screenshots where UI changed.
6. Known limitations and measured performance (if benchmarked).
7. A concise next-milestone checklist.

## 18. First Codex execution prompt

Copy the following prompt verbatim into Codex:

> Read `LABELFLOW_CODEX_MASTER_PLAN.md` completely. You are building LabelFlow, an offline Windows-first desktop labeling app using Tauri 2 + React + TypeScript + Rust + SQLite. Do not implement the whole roadmap yet. First inspect the repository, create `docs/decisions/0001-architecture.md`, implement **M0 only** (app shell, project creation/open, database migration, image viewport zoom/pan, typed IPC, tests and build scripts), and demonstrate a runnable local result. Do not add cloud, server, authentication or bundled AI models. Pay special attention to canonical image pixel coordinates, non-destructive media access, safe path handling and typed backend errors. Run all tests/checks available, document blockers accurately and stop after M0 with a summary and suggested next steps.

## 19. References checked during planning (2026-11-06)
- Tauri architecture: https://v2.tauri.app/concept/architecture/
- Tauri plugins/capabilities: https://v2.tauri.app/plugin/
- YOLO instance segmentation representation: https://docs.ultralytics.com/datasets/segment/
- ONNX Runtime execution providers: https://onnxruntime.ai/docs/execution-providers/

These references are informational, not additional mandatory product dependencies. Validate versions and licenses at implementation time.
