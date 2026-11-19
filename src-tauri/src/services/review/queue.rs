use crate::{
    domain::{annotation::Progress, review::*, AppError, ErrorCode, Result},
    services::{annotation as a, project::ProjectSession},
};
use rusqlite::params;
use std::sync::atomic::{AtomicBool, Ordering};

pub fn page(
    s: &ProjectSession,
    filter: &ReviewFilter,
    offset: u32,
    limit: u32,
) -> Result<ReviewPage> {
    if limit == 0
        || limit > 100
        || (!filter.status.is_empty() && !a::valid_status(&filter.status))
        || (!filter.event.is_empty()
            && !["positive", "negative", "uncertain", "none"].contains(&filter.event.as_str()))
        || (!filter.tag.is_empty() && !TAGS.contains(&filter.tag.as_str()))
        || filter.trip_id.len() > 64
    {
        return Err(a::invalid("Invalid review queue filter."));
    }
    let sql="FROM media m LEFT JOIN image_events e ON e.media_id=m.id LEFT JOIN annotation_documents d ON d.media_id=m.id LEFT JOIN media_review_metadata r ON r.media_id=m.id LEFT JOIN videos v ON v.id=m.source_video_id LEFT JOIN trips t ON t.id=COALESCE(v.trip_id,r.trip_id) LEFT JOIN review_prediction_index p ON p.key=(SELECT key FROM review_prediction_index WHERE media_id=m.id ORDER BY created_at DESC,key DESC LIMIT 1) LEFT JOIN review_comparisons c ON c.prediction_key=p.key AND c.gt_revision=COALESCE(d.revision,0) WHERE m.project_id=?1 AND (?2='' OR m.review_status=?2) AND (?3='' OR e.event_type=?3 OR (?3='none' AND e.event_type IS NULL)) AND (?4='' OR EXISTS(SELECT 1 FROM media_tags tag WHERE tag.media_id=m.id AND tag.tag=?4)) AND (?5='' OR t.id=?5) AND (?6=0 OR c.disagreement=1)";
    let total = s.connection.query_row(
        &format!("SELECT count(*) {sql}"),
        params![
            s.project.id,
            filter.status,
            filter.event,
            filter.tag,
            filter.trip_id,
            filter.disagreements_only
        ],
        |r| r.get(0),
    )?;
    let mut q=s.connection.prepare(&format!("SELECT m.id,m.path,m.review_status,e.event_type,t.name,p.key,p.model_name,CASE WHEN c.scored=1 THEN c.disagreement END,c.prediction_key IS NOT NULL {sql} ORDER BY m.created_at,m.id LIMIT ?7 OFFSET ?8"))?;
    let rows = q
        .query_map(
            params![
                s.project.id,
                filter.status,
                filter.event,
                filter.tag,
                filter.trip_id,
                filter.disagreements_only,
                limit,
                offset
            ],
            |r| {
                let path: String = r.get(1)?;
                Ok(ReviewRow {
                    media_id: r.get(0)?,
                    name: std::path::Path::new(&path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    review_status: r.get(2)?,
                    event_label: r.get(3)?,
                    trip_name: r.get(4)?,
                    prediction_key: r.get(5)?,
                    model_name: r.get(6)?,
                    disagreement: r.get(7)?,
                    analyzed: r.get(8)?,
                })
            },
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(ReviewPage { items: rows, total })
}
pub fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(AppError::new(
            ErrorCode::Cancelled,
            "Road review operation cancelled. Completed comparisons remain consistent.",
        ));
    }
    Ok(())
}
pub fn keys(s: &ProjectSession) -> Result<Vec<String>> {
    let mut q = s.connection.prepare(
        "SELECT key FROM review_prediction_index ORDER BY media_id,created_at,key LIMIT 100001",
    )?;
    let keys = q
        .query_map([], |r| r.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if keys.len() > 100_000 {
        return Err(a::invalid("Review job exceeds 100,000 prediction limit."));
    }
    Ok(keys)
}
pub fn analyze(
    s: &ProjectSession,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<u32> {
    let job = crate::jobs::start(&s.connection, "review_analysis", "{}")?;
    let result = analyze_inner(s, cancel, |p| {
        crate::jobs::progress(&s.connection, &job, p.completed, p.total);
        progress(p);
    });
    finish(s, &job, &result);
    result
}
pub fn finish<T>(s: &ProjectSession, job: &str, result: &Result<T>) {
    match result {
        Ok(_) => crate::jobs::finish(&s.connection, job, "completed", None),
        Err(e) => crate::jobs::finish(
            &s.connection,
            job,
            if e.code == ErrorCode::Cancelled {
                "cancelled"
            } else {
                "failed"
            },
            Some(&e.to_string()),
        ),
    }
}
fn analyze_inner(
    s: &ProjectSession,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<u32> {
    let keys = keys(s)?;
    for (index, key) in keys.iter().enumerate() {
        check(cancel)?;
        let (summary, p) = super::predictions::load(s, key)?;
        let doc = a::load(s, &summary.media_id)?;
        let result = super::metrics::compare(&doc, &p, false)?;
        check(cancel)?;
        let scored =
            result.event_mismatch.is_some() || result.layers.iter().any(|l| l.evaluated_pixels > 0);
        s.connection.execute("INSERT INTO review_comparisons(prediction_key,media_id,gt_revision,disagreement,report_json,scored) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(prediction_key) DO UPDATE SET gt_revision=excluded.gt_revision,disagreement=excluded.disagreement,report_json=excluded.report_json,scored=excluded.scored,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",params![key,summary.media_id,doc.revision,result.has_disagreement,a::json(&result)?,scored])?;
        progress(Progress {
            completed: index as u32 + 1,
            total: keys.len() as u32,
            message: "Comparing predictions with human labels".into(),
        });
    }
    Ok(keys.len() as u32)
}
