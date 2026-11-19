# M0 project format and migration notes

Portable root layout:

```text
Project/
  .labelflow/
    project.json
    project.sqlite
    project.lock
    thumbnails/
    masks/
    backups/
    logs/
```

`project.json` format version 1 contains UUID, name and `generic-v1` profile. SQLite schema version 1 is tracked by `PRAGMA user_version` and the project row. Migration `src-tauri/migrations/0001_foundation.sql` creates `projects`, linked-image `media` and singleton `workspace`. IDs are UUIDs, timestamps UTC, dimensions positive, orientation 1..8, unique project/path, review states constrained and active-media references protected by foreign keys. Dimensions describe EXIF-normalized pixels, never screen coordinates.

New projects require an existing user-selected root without `.labelflow`. All other root contents are preserved. The metadata directory is exclusively created and the manifest is published last via a synced temporary file and rename. A crash during creation may leave an incomplete metadata directory: opening or recreating it is refused with a visible error; M0 has no automatic destructive cleanup. Preserve it and choose another folder.

Opening requires both manifest and database. Missing databases are never recreated. Newer schema/format versions, corrupt databases, mismatched identities and invalid foreign keys are rejected. Nonempty older schemas are backed up into `backups/schema-<version>-<uuid>.sqlite` before migration using the SQLite backup API, including committed WAL content; migration and schema version update share a transaction. M0's only migration creates the initial schema. No backwards downgrade is supported.

The operating-system lock is advisory against cooperating LabelFlow instances; external database editors must not edit a live project. A leftover `project.lock` after a crash is normal and must not be deleted. M0 rejects the second writer rather than offering read-only mode. WAL projects should be kept on a local filesystem.

Close LabelFlow before making a manual project backup. Copy the entire `.labelflow` directory, including any SQLite WAL/SHM if still present; copy any managed assets when those exist. External linked images must be backed up separately. M0 has no mask assets or user annotation edits yet. M1 must add transactional annotation revisions, durable operation journal, and tests restoring DB plus referenced lossless masks together.

The media reference is relative when inside the root and absolute for external sources. Reopening verifies SHA-256, dimensions and orientation; moved/missing/changed sources surface an error. Open the source again explicitly to update the M0 reference. A directory batch importer, relocation UI and copied media are future work. M0 media history is stored but only the active image is shown, without a browser for older records.
