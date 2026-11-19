use crate::{
    domain::{advanced::*, annotation::Progress, Result},
    services::{annotation as a, library, project::ProjectSession, review::queue},
};
use image::{imageops::FilterType, DynamicImage};
use rusqlite::params;
use std::{collections::BTreeMap, sync::atomic::AtomicBool};

pub const ALGORITHM: &str = "dhash9x8-luma64-v1";
// Fingerprints deliberately describe small canonical previews, not annotation quality.
pub fn describe(image: &DynamicImage) -> (u64, f64, f64) {
    let small = image.resize_exact(9, 8, FilterType::Triangle).to_luma8();
    let mut hash = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            hash = (hash << 1) | u64::from(small[(x, y)][0] > small[(x + 1, y)][0]);
        }
    }
    let gray = image.resize_exact(64, 64, FilterType::Triangle).to_luma8();
    let brightness = gray.as_raw().iter().map(|&p| f64::from(p)).sum::<f64>() / 4096.;
    let mut sum = 0.;
    let mut square = 0.;
    for y in 1..63 {
        for x in 1..63 {
            let v = f64::from(gray[(x - 1, y)][0])
                + f64::from(gray[(x + 1, y)][0])
                + f64::from(gray[(x, y - 1)][0])
                + f64::from(gray[(x, y + 1)][0])
                - 4. * f64::from(gray[(x, y)][0]);
            sum += v;
            square += v * v;
        }
    }
    let n = 62. * 62.;
    (hash, brightness, (square / n - (sum / n).powi(2)).max(0.))
}
pub fn index(
    s: &ProjectSession,
    force: bool,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<IndexReport> {
    let job = crate::jobs::start(&s.connection, "index_fingerprints", &a::json(&force)?)?;
    let result = (|| {
        let records = library::all(s)?;
        if records.len() > 100_000 {
            return Err(a::invalid("Fingerprint job exceeds 100,000 images."));
        }
        let mut report = IndexReport {
            indexed: 0,
            reused: 0,
            errors: vec![],
        };
        for (i, r) in records.iter().enumerate() {
            queue::check(cancel)?;
            let cached:bool=s.connection.query_row("SELECT EXISTS(SELECT 1 FROM media_fingerprints WHERE media_id=?1 AND source_sha256=?2 AND algorithm=?3 AND error IS NULL)",params![r.id,r.sha256,ALGORITHM],|r|r.get(0))?;
            let source_matches = cached
                && !force
                && crate::video::source_sha(&library::resolve(s, &r.path)?)
                    .is_ok_and(|sha| sha == r.sha256);
            if source_matches {
                report.reused += 1;
            } else {
                let result = library::checked_load(s, r).map(|(_, img)| describe(&img));
                queue::check(cancel)?;
                let (hash, brightness, sharpness, error) = match result {
                    Ok((h, b, v)) => {
                        report.indexed += 1;
                        (Some(format!("{h:016x}")), Some(b), Some(v), None)
                    }
                    Err(e) => {
                        let message = e.to_string().chars().take(1000).collect::<String>();
                        report.errors.push(format!("{}: {message}", r.name));
                        (None, None, None, Some(message))
                    }
                };
                s.connection.execute("INSERT INTO media_fingerprints(media_id,source_sha256,algorithm,dhash,brightness,sharpness,error) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(media_id) DO UPDATE SET source_sha256=excluded.source_sha256,algorithm=excluded.algorithm,dhash=excluded.dhash,brightness=excluded.brightness,sharpness=excluded.sharpness,error=excluded.error,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",params![r.id,r.sha256,ALGORITHM,hash,brightness,sharpness,error])?;
            }
            progress(Progress {
                completed: i as u32 + 1,
                total: records.len() as u32,
                message: r.name.clone(),
            });
        }
        Ok(report)
    })();
    queue::finish(s, &job, &result);
    result
}
pub fn rank(
    s: &ProjectSession,
    media_id: &str,
    scope: &str,
    max_distance: u32,
) -> Result<Vec<DuplicateCandidate>> {
    if !["project", "group"].contains(&scope) || max_distance > 64 {
        return Err(a::invalid("Invalid similarity scope or distance."));
    }
    let record = library::record(s, media_id)?;
    let group = crate::services::quality::group(s, media_id)?;
    let (hash,brightness):(String,f64)=s.connection.query_row("SELECT dhash,brightness FROM media_fingerprints WHERE media_id=?1 AND source_sha256=?2 AND algorithm=?3 AND error IS NULL",params![media_id,record.sha256,ALGORITHM],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|a::invalid("Index this image successfully before ranking candidates."))?;
    let hash = u64::from_str_radix(&hash, 16)
        .map_err(|_| a::invalid("Invalid cached fingerprint; rebuild index."))?;
    let mut q=s.connection.prepare("SELECT m.id,m.path,m.content_sha256,f.dhash,f.brightness,f.sharpness,CASE WHEN COALESCE(v.trip_id,r.trip_id) IS NOT NULL THEN 'trip:'||COALESCE(v.trip_id,r.trip_id) WHEN m.source_video_id IS NOT NULL THEN COALESCE(v.split_group_id,'video:'||m.source_video_id) ELSE COALESCE(NULLIF(m.split_group_id,''),m.content_sha256) END FROM media m JOIN media_fingerprints f ON f.media_id=m.id AND f.source_sha256=m.content_sha256 AND f.algorithm=?1 AND f.error IS NULL LEFT JOIN videos v ON v.id=m.source_video_id LEFT JOIN media_review_metadata r ON r.media_id=m.id WHERE m.id<>?2 ORDER BY m.id LIMIT 100001")?;
    let rows = q
        .query_map(params![ALGORITHM, media_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, f64>(4)?,
                r.get::<_, f64>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if rows.len() > 100_000 {
        return Err(a::invalid(
            "Similarity ranking exceeds 100,000 indexed images.",
        ));
    }
    let mut found = Vec::new();
    for (id, path, sha, h, b, v, g) in rows {
        if scope == "group" && g != group {
            continue;
        }
        let h = u64::from_str_radix(&h, 16)
            .map_err(|_| a::invalid("Invalid cached fingerprint; rebuild index."))?;
        let distance = (hash ^ h).count_ones();
        if distance <= max_distance {
            found.push(DuplicateCandidate {
                media_id: id,
                name: std::path::Path::new(&path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                distance,
                same_source_hash: sha == record.sha256,
                same_group: g == group,
                brightness: b,
                sharpness: v,
            });
        }
    }
    found.sort_by(|a, b| {
        a.distance
            .cmp(&b.distance)
            .then(b.same_source_hash.cmp(&a.same_source_hash))
            .then(
                (a.brightness - brightness)
                    .abs()
                    .total_cmp(&(b.brightness - brightness).abs()),
            )
            .then(a.media_id.cmp(&b.media_id))
    });
    found.truncate(100);
    Ok(found)
}
fn counts(s: &ProjectSession, sql: &str) -> Result<BTreeMap<String, u32>> {
    let mut q = s.connection.prepare(sql)?;
    let result = q
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<BTreeMap<_, _>, _>>()?;
    Ok(result)
}
pub fn dashboard(s: &ProjectSession) -> Result<Dashboard> {
    let scalar = |sql: &str| -> Result<u32> { Ok(s.connection.query_row(sql, [], |r| r.get(0))?) };
    let indexed:u32=s.connection.query_row("SELECT count(*) FROM media_fingerprints f JOIN media m ON m.id=f.media_id WHERE f.source_sha256=m.content_sha256 AND f.algorithm=?1 AND f.error IS NULL",[ALGORITHM],|r|r.get(0))?;
    let errors:u32=s.connection.query_row("SELECT count(*) FROM media_fingerprints f JOIN media m ON m.id=f.media_id WHERE f.source_sha256=m.content_sha256 AND f.algorithm=?1 AND f.error IS NOT NULL",[ALGORITHM],|r|r.get(0))?;
    let images = scalar("SELECT count(*) FROM media")?;
    Ok(Dashboard{images,review_counts:counts(s,"SELECT review_status,count(*) FROM media GROUP BY review_status")?,event_counts:counts(s,"SELECT COALESCE(e.event_type,'unassessed'),count(*) FROM media m LEFT JOIN image_events e ON e.media_id=m.id GROUP BY COALESCE(e.event_type,'unassessed')")?,tag_counts:counts(s,"SELECT tag,count(*) FROM media_tags GROUP BY tag")?,group_count:scalar("SELECT count(DISTINCT CASE WHEN COALESCE(v.trip_id,r.trip_id) IS NOT NULL THEN 'trip:'||COALESCE(v.trip_id,r.trip_id) WHEN m.source_video_id IS NOT NULL THEN COALESCE(v.split_group_id,'video:'||m.source_video_id) ELSE COALESCE(NULLIF(m.split_group_id,''),m.content_sha256) END) FROM media m LEFT JOIN videos v ON v.id=m.source_video_id LEFT JOIN media_review_metadata r ON r.media_id=m.id")?,trip_count:scalar("SELECT count(*) FROM trips")?,indexed,index_errors:errors,unindexed:images.saturating_sub(indexed+errors),low_detail_candidates:s.connection.query_row("SELECT count(*) FROM media_fingerprints f JOIN media m ON m.id=f.media_id WHERE f.source_sha256=m.content_sha256 AND f.algorithm=?1 AND f.error IS NULL AND f.sharpness<12",[ALGORITHM],|r|r.get(0))?,pending_predictions:scalar("SELECT count(*) FROM review_prediction_index WHERE status='pending'")?,pending_propagations:scalar("SELECT count(*) FROM propagation_links l JOIN review_predictions p ON p.id=l.prediction_id WHERE p.status='pending'")?,snapshots:scalar("SELECT count(*) FROM dataset_snapshots")?})
}
