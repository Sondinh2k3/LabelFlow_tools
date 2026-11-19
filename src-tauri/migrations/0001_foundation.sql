CREATE TABLE projects (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK(length(trim(name)) BETWEEN 1 AND 120),
    profile_id TEXT NOT NULL,
    schema_version INTEGER NOT NULL CHECK(schema_version > 0),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE TABLE media (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id),
    type TEXT NOT NULL CHECK(type = 'image'),
    path TEXT NOT NULL,
    import_mode TEXT NOT NULL CHECK(import_mode = 'linked'),
    content_sha256 TEXT NOT NULL CHECK(length(content_sha256) = 64),
    width INTEGER NOT NULL CHECK(width > 0),
    height INTEGER NOT NULL CHECK(height > 0),
    orientation INTEGER NOT NULL CHECK(orientation BETWEEN 1 AND 8),
    review_status TEXT NOT NULL DEFAULT 'unlabeled' CHECK(review_status IN ('unlabeled', 'in_progress', 'needs_review', 'approved', 'rejected')),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE(project_id, path)
);
CREATE INDEX media_project ON media(project_id, created_at, id);
CREATE TABLE workspace (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    active_media_id TEXT REFERENCES media(id)
);
INSERT INTO workspace(singleton, active_media_id) VALUES (1, NULL);
