use crate::{
    domain::{advanced::*, annotation::*, Result},
    services::{
        annotation as a, library, profile,
        project::ProjectSession,
        quality,
        review::{metadata, queue},
    },
};
use rusqlite::params;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

fn options(o: &SnapshotOptions) -> Result<()> {
    if o.name.trim().is_empty()
        || o.name.len() > 160
        || o.name.chars().any(char::is_control)
        || o.seed.is_empty()
        || o.seed.len() > 128
    {
        return Err(a::invalid(
            "Snapshot needs a name (1–160 bytes) and split seed (1–128 bytes).",
        ));
    }
    crate::export::split_for("", &o.seed, o.train_percent, o.val_percent)?;
    Ok(())
}
pub fn list(s: &ProjectSession) -> Result<Vec<SnapshotSummary>> {
    let mut q=s.connection.prepare("SELECT id,name,image_count,created_at FROM dataset_snapshots ORDER BY created_at DESC,id DESC LIMIT 1000")?;
    let result = q
        .query_map([], |r| {
            Ok(SnapshotSummary {
                id: r.get(0)?,
                name: r.get(1)?,
                images: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(result)
}
fn summary(s: &ProjectSession, id: &str) -> Result<SnapshotSummary> {
    Ok(s.connection.query_row(
        "SELECT id,name,image_count,created_at FROM dataset_snapshots WHERE id=?1",
        [id],
        |r| {
            Ok(SnapshotSummary {
                id: r.get(0)?,
                name: r.get(1)?,
                images: r.get(2)?,
                created_at: r.get(3)?,
            })
        },
    )?)
}
pub fn create(
    s: &ProjectSession,
    o: &SnapshotOptions,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<SnapshotSummary> {
    options(o)?;
    let job = crate::jobs::start(&s.connection, "create_snapshot", &a::json(o)?)?;
    let result = (|| {
        let mut records = library::all(s)?;
        records.retain(|r| !o.approved_only || r.review_status == "approved");
        records.sort_by(|a, b| a.id.cmp(&b.id));
        if records.is_empty() || records.len() > 100_000 {
            return Err(a::invalid("A snapshot requires 1–100,000 eligible images."));
        }
        let mut manifest = SnapshotManifest {
            format_version: 1,
            project_id: s.project.id.clone(),
            profile: profile::current(s)?,
            seed: o.seed.clone(),
            train_percent: o.train_percent,
            val_percent: o.val_percent,
            approved_only: o.approved_only,
            trips: metadata::trips(s)?,
            images: Vec::new(),
        };
        manifest.trips.sort_by(|a, b| a.id.cmp(&b.id));
        // One transaction: cancellation/failure cannot publish a partial snapshot or orphan objects.
        let tx = s.connection.unchecked_transaction()?;
        for (i, r) in records.iter().enumerate() {
            queue::check(cancel)?;
            let source = library::resolve(s, &r.path)?;
            if crate::video::source_sha(&source)? != r.sha256 {
                return Err(a::invalid(format!(
                    "{}: source changed; snapshot not created.",
                    r.name
                )));
            }
            let group = quality::group(s, &r.id)?;
            let mut doc = a::load(s, &r.id)?;
            doc.split_group = group.clone();
            let json = a::json(&doc)?;
            if json.len() > 64 * 1024 * 1024 {
                return Err(a::invalid(
                    "A snapshot document exceeds 64 MiB; reduce raster complexity.",
                ));
            }
            let hash = a::sha(json.as_bytes());
            let inserted=tx.execute("INSERT INTO snapshot_objects(sha256,document_json) VALUES(?1,?2) ON CONFLICT(sha256) DO NOTHING",params![hash,json])?;
            if inserted == 0 {
                let existing: String = tx.query_row(
                    "SELECT document_json FROM snapshot_objects WHERE sha256=?1",
                    [&hash],
                    |r| r.get(0),
                )?;
                if existing != json {
                    return Err(a::invalid("Existing frozen object is corrupt; preserve the project and restore a complete backup."));
                }
            }
            manifest.images.push(FrozenImage {
                id: r.id.clone(),
                source: source.to_string_lossy().into(),
                source_sha256: r.sha256.clone(),
                width: r.width,
                height: r.height,
                orientation: r.orientation,
                document_sha256: hash,
                split: crate::export::split_for(&group, &o.seed, o.train_percent, o.val_percent)?
                    .into(),
                group,
                metadata: metadata::load(s, &r.id)?,
                source_video: quality::origin(s, &r.id)?,
            });
            progress(Progress {
                completed: i as u32 + 1,
                total: records.len() as u32,
                message: r.name.clone(),
            });
        }
        queue::check(cancel)?;
        let json = a::json(&manifest)?;
        let id = a::sha(json.as_bytes());
        let inserted=tx.execute("INSERT INTO dataset_snapshots(id,name,manifest_json,image_count) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO NOTHING",params![id,o.name.trim(),json,records.len()])?;
        if inserted == 0 {
            let existing: String = tx.query_row(
                "SELECT manifest_json FROM dataset_snapshots WHERE id=?1",
                [&id],
                |r| r.get(0),
            )?;
            if existing != json {
                return Err(a::invalid(
                    "Existing frozen manifest is corrupt; restore a complete backup.",
                ));
            }
        }
        tx.commit()?;
        summary(s, &id)
    })();
    queue::finish(s, &job, &result);
    result
}
pub fn manifest(s: &ProjectSession, id: &str) -> Result<SnapshotManifest> {
    if id.len() != 64 || !id.bytes().all(|v| v.is_ascii_hexdigit()) {
        return Err(a::invalid("Invalid snapshot ID."));
    }
    let json: String = s.connection.query_row(
        "SELECT manifest_json FROM dataset_snapshots WHERE id=?1",
        [id],
        |r| r.get(0),
    )?;
    if a::sha(json.as_bytes()) != id {
        return Err(a::invalid("Snapshot manifest checksum mismatch."));
    }
    let m: SnapshotManifest = a::parse(&json)?;
    if m.format_version != 1
        || m.project_id != s.project.id
        || m.profile.id != s.project.profile_id
        // Layers are fixed by the profile and used in export paths; object classes are frozen
        // into the version and may be renamed/added later without invalidating old versions.
        || a::json(&m.profile.layers)? != a::json(&profile::current(s)?.layers)?
        || m.profile.classes.len() > 512
        || m.profile.classes.iter().any(|c| {
            c.id.is_empty()
                || c.id.len() > 64
                || c.display_name.chars().count() > 64
                || !c.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        || m.images.is_empty()
        || m.images.len() > 100_000
    {
        return Err(a::invalid("Snapshot identity or format mismatch."));
    }
    crate::export::split_for("", &m.seed, m.train_percent, m.val_percent)?;
    Ok(m)
}
pub fn document(s: &ProjectSession, image: &FrozenImage) -> Result<Document> {
    let json: String = s.connection.query_row(
        "SELECT document_json FROM snapshot_objects WHERE sha256=?1",
        [&image.document_sha256],
        |r| r.get(0),
    )?;
    if json.len() > 64 * 1024 * 1024 || a::sha(json.as_bytes()) != image.document_sha256 {
        return Err(a::invalid("Snapshot annotation object checksum mismatch."));
    }
    let doc: Document = a::parse(&json)?;
    if doc.media_id != image.id || doc.split_group != image.group {
        return Err(a::invalid("Snapshot annotation identity/group mismatch."));
    }
    for mask in &doc.masks {
        if (mask.width, mask.height) != (image.width, image.height) {
            return Err(a::invalid("Snapshot mask dimensions mismatch."));
        }
        a::unpack(mask)?;
    }
    for shape in &doc.annotations {
        a::validate_geometry(&shape.geometry, image.width, image.height)?;
    }
    Ok(doc)
}
fn source_record(s: &ProjectSession, image: &FrozenImage) -> Result<MediaRecord> {
    let r = library::record(s, &image.id)?;
    if r.sha256 != image.source_sha256
        || (r.width, r.height, r.orientation) != (image.width, image.height, image.orientation)
    {
        return Err(a::invalid(
            "Live media identity differs from frozen snapshot; restore its original source.",
        ));
    }
    if crate::video::source_sha(&library::resolve(s, &r.path)?)? != image.source_sha256 {
        return Err(a::invalid(format!(
            "{}: frozen source missing or changed.",
            r.name
        )));
    }
    Ok(r)
}
pub fn verify(
    s: &ProjectSession,
    id: &str,
    sources: bool,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<SnapshotCheck> {
    let m = manifest(s, id)?;
    let mut splits = BTreeMap::new();
    let mut groups = HashMap::new();
    let mut videos = HashMap::new();
    let mut ids = HashSet::new();
    for (i, image) in m.images.iter().enumerate() {
        queue::check(cancel)?;
        uuid::Uuid::parse_str(&image.id).map_err(|_| a::invalid("Invalid frozen media ID."))?;
        if !ids.insert(&image.id)
            || image.metadata.media_id != image.id
            || image.split
                != crate::export::split_for(&image.group, &m.seed, m.train_percent, m.val_percent)?
        {
            return Err(a::invalid(
                "Snapshot has duplicate IDs or inconsistent split membership.",
            ));
        }
        if groups
            .insert(&image.group, &image.split)
            .is_some_and(|old| old != &image.split)
        {
            return Err(a::invalid("Snapshot group leaks across splits."));
        }
        if let Some(video) = image
            .source_video
            .as_ref()
            .and_then(|v| v["videoId"].as_str())
        {
            if videos
                .insert(video, &image.split)
                .is_some_and(|old| old != &image.split)
            {
                return Err(a::invalid("Snapshot video leaks across splits."));
            }
        }
        let doc = document(s, image)?;
        if m.approved_only && doc.review_status != "approved" {
            return Err(a::invalid("Unapproved document in approved-only snapshot."));
        }
        if sources {
            source_record(s, image)?;
        }
        *splits.entry(image.split.clone()).or_insert(0) += 1;
        progress(Progress {
            completed: i as u32 + 1,
            total: m.images.len() as u32,
            message: image.id.clone(),
        });
    }
    Ok(SnapshotCheck {
        id: id.into(),
        images: m.images.len() as u32,
        split_counts: splits,
        source_checked: sources,
    })
}
pub fn export(
    s: &ProjectSession,
    id: &str,
    parent: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<PathBuf> {
    let job = crate::jobs::start(&s.connection, "export_snapshot", &a::json(&id)?)?;
    let result = (|| {
        verify(s, id, false, cancel, |_| {})?;
        let m = manifest(s, id)?;
        let parent = parent.canonicalize()?;
        let nonce = uuid::Uuid::new_v4();
        let staging = parent.join(format!(".labelflow-snapshot-{nonce}.tmp"));
        let destination = parent.join(format!("LabelFlow-snapshot-{}-{nonce}", &id[..12]));
        fs::create_dir(&staging)?;
        let mut hashes = BTreeMap::<String, String>::new();
        let mut write = |relative: &str, bytes: &[u8]| -> Result<()> {
            if Path::new(relative).is_absolute()
                || !Path::new(relative)
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)))
            {
                return Err(a::invalid("Unsafe frozen export path."));
            }
            let path = staging.join(relative);
            fs::create_dir_all(path.parent().unwrap())?;
            a::atomic_file(&path, bytes)?;
            hashes.insert(relative.into(), a::sha(bytes));
            Ok(())
        };
        write("manifest.json", a::json(&m)?.as_bytes())?;
        write("snapshot-id.txt", id.as_bytes())?;
        write("profile.json", a::json(&m.profile)?.as_bytes())?;
        for (i, image) in m.images.iter().enumerate() {
            queue::check(cancel)?;
            let record = source_record(s, image)?;
            let (_, decoded) = library::checked_load(s, &record)?;
            let mut png = Cursor::new(Vec::new());
            decoded
                .write_to(&mut png, image::ImageFormat::Png)
                .map_err(|e| a::invalid(e.to_string()))?;
            write(
                &format!("images/{}/{}.png", image.split, image.id),
                &png.into_inner(),
            )?;
            let doc = document(s, image)?;
            write(
                &format!("annotations/{}.json", image.id),
                a::json(&doc)?.as_bytes(),
            )?;
            for layer in m
                .profile
                .layers
                .iter()
                .filter(|l| l.layer_type != "instance")
            {
                // An absent dense label is unspecified, not a negative mask.
                if !doc.masks.iter().any(|r| r.layer_id == layer.id)
                    && !doc.annotations.iter().any(|a| a.layer_id == layer.id)
                {
                    continue;
                }
                let mut pixels = doc
                    .masks
                    .iter()
                    .find(|r| r.layer_id == layer.id)
                    .map(a::unpack)
                    .transpose()?
                    .unwrap_or_else(|| {
                        vec![
                            if layer.layer_type == "semantic" {
                                255
                            } else {
                                0
                            };
                            image.width as usize * image.height as usize
                        ]
                    });
                for shape in doc.annotations.iter().filter(|a| a.layer_id == layer.id) {
                    let overlay = crate::domain::geometry::rasterize(
                        &shape.geometry,
                        image.width,
                        image.height,
                    )?;
                    for (p, v) in pixels.iter_mut().zip(overlay) {
                        *p |= v;
                    }
                }
                write(
                    &format!("masks/{}/{}/{}.png", layer.id, image.split, image.id),
                    &a::encode_png(image.width, image.height, &pixels)?,
                )?;
            }
            progress(Progress {
                completed: i as u32 + 1,
                total: m.images.len() as u32,
                message: record.name,
            });
            crate::jobs::progress(&s.connection, &job, i as u32 + 1, m.images.len() as u32);
        }
        for (relative, hash) in &hashes {
            queue::check(cancel)?;
            if a::sha(&fs::read(staging.join(relative))?) != *hash {
                return Err(a::invalid("Frozen export checksum verification failed."));
            }
        }
        a::atomic_file(
            &staging.join("checksums.json"),
            a::json(&hashes)?.as_bytes(),
        )?;
        queue::check(cancel)?;
        fs::rename(&staging, &destination)?;
        Ok(destination)
    })();
    queue::finish(s, &job, &result);
    result
}
