use crate::{
    domain::{review::*, AppError, ErrorCode, Result},
    services::{annotation as a, library, project::ProjectSession},
};
use rusqlite::{params, OptionalExtension};
fn bounded(value: &str, max: usize) -> bool {
    value.len() <= max && !value.chars().any(|c| c == '\0')
}
pub fn trips(s: &ProjectSession) -> Result<Vec<Trip>> {
    let mut q = s.connection.prepare(
        "SELECT id,name,route,camera_id,distance_km,notes,revision FROM trips ORDER BY name,id",
    )?;
    let rows = q
        .query_map([], |r| {
            Ok(Trip {
                id: r.get(0)?,
                name: r.get(1)?,
                route: r.get(2)?,
                camera_id: r.get(3)?,
                distance_km: r.get(4)?,
                notes: r.get(5)?,
                revision: r.get(6)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}
pub fn save_trip(s: &ProjectSession, mut trip: Trip) -> Result<Trip> {
    if trip.id.is_empty() {
        trip.id = uuid::Uuid::new_v4().to_string();
    }
    uuid::Uuid::parse_str(&trip.id).map_err(|_| a::invalid("Invalid trip ID."))?;
    if trip.name.trim().is_empty()
        || !bounded(&trip.name, 200)
        || !bounded(&trip.route, 512)
        || !bounded(&trip.camera_id, 200)
        || !bounded(&trip.notes, 4000)
        || trip.distance_km.is_some_and(|d| !d.is_finite() || d <= 0.)
    {
        return Err(a::invalid("Invalid trip metadata or distance."));
    }
    let changed = if trip.revision == 0 {
        s.connection.execute("INSERT INTO trips(id,name,route,camera_id,distance_km,notes,revision) VALUES(?1,?2,?3,?4,?5,?6,1)",params![trip.id,trip.name,trip.route,trip.camera_id,trip.distance_km,trip.notes])?
    } else {
        s.connection.execute("UPDATE trips SET name=?2,route=?3,camera_id=?4,distance_km=?5,notes=?6,revision=revision+1 WHERE id=?1 AND revision=?7",params![trip.id,trip.name,trip.route,trip.camera_id,trip.distance_km,trip.notes,trip.revision])?
    };
    if changed != 1 {
        return Err(AppError::new(
            ErrorCode::Conflict,
            "Trip changed; reload before saving.",
        ));
    }
    trip.revision += 1;
    Ok(trip)
}
pub fn load(s: &ProjectSession, media: &str) -> Result<Metadata> {
    library::record(s, media)?;
    let mut value=s.connection.query_row("SELECT m.id,COALESCE(r.revision,0),COALESCE(v.trip_id,r.trip_id),COALESCE(r.camera_id,''),COALESCE(r.route,''),COALESCE(r.captured_at,''),r.latitude,r.longitude,v.trip_revision FROM media m LEFT JOIN media_review_metadata r ON r.media_id=m.id LEFT JOIN videos v ON v.id=m.source_video_id WHERE m.id=?1",[media],|r|Ok(Metadata{media_id:r.get(0)?,revision:r.get(1)?,trip_id:r.get(2)?,camera_id:r.get(3)?,route:r.get(4)?,captured_at:r.get(5)?,latitude:r.get(6)?,longitude:r.get(7)?,video_trip_revision:r.get(8)?,tags:vec![]}))?;
    let mut q = s
        .connection
        .prepare("SELECT tag FROM media_tags WHERE media_id=?1 ORDER BY tag")?;
    value.tags = q
        .query_map([media], |r| r.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(value)
}
pub fn save(s: &mut ProjectSession, value: Metadata) -> Result<Metadata> {
    library::record(s, &value.media_id)?;
    let old = load(s, &value.media_id)?;
    if old.video_trip_revision != value.video_trip_revision {
        return Err(AppError::new(
            ErrorCode::Conflict,
            "Video trip changed; reload capture metadata before saving.",
        ));
    }
    let old_trip = old.trip_id;
    if !bounded(&value.camera_id, 200)
        || !bounded(&value.route, 512)
        || !bounded(&value.captured_at, 80)
        || value.tags.len() > TAGS.len()
        || value.tags.iter().any(|t| !TAGS.contains(&t.as_str()))
        || value
            .tags
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != value.tags.len()
        || value.latitude.is_some() != value.longitude.is_some()
        || value
            .latitude
            .is_some_and(|v| !v.is_finite() || !(-90. ..=90.).contains(&v))
        || value
            .longitude
            .is_some_and(|v| !v.is_finite() || !(-180. ..=180.).contains(&v))
    {
        return Err(a::invalid("Invalid road review metadata / tags / GPS."));
    }
    if let Some(id) = &value.trip_id {
        uuid::Uuid::parse_str(id).map_err(|_| a::invalid("Invalid trip ID."))?;
        let _: String = s
            .connection
            .query_row("SELECT id FROM trips WHERE id=?1", [id], |r| r.get(0))?;
    }
    let head: u32 = s
        .connection
        .query_row(
            "SELECT revision FROM media_review_metadata WHERE media_id=?1",
            [&value.media_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    if head != value.revision {
        return Err(AppError::new(
            ErrorCode::Conflict,
            "Metadata changed; reload before saving.",
        ));
    }
    let video: Option<String> = s.connection.query_row(
        "SELECT source_video_id FROM media WHERE id=?1",
        [&value.media_id],
        |r| r.get(0),
    )?;
    let tx = s.connection.transaction()?;
    tx.execute("INSERT INTO media_review_metadata(media_id,revision,trip_id,camera_id,route,captured_at,latitude,longitude) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(media_id) DO UPDATE SET revision=excluded.revision,trip_id=excluded.trip_id,camera_id=excluded.camera_id,route=excluded.route,captured_at=excluded.captured_at,latitude=excluded.latitude,longitude=excluded.longitude",params![value.media_id,head+1,if video.is_some(){None}else{value.trip_id.as_ref()},value.camera_id,value.route,value.captured_at,value.latitude,value.longitude])?;
    let group = value.trip_id.as_ref().map(|id| format!("trip:{id}"));
    if old_trip != value.trip_id {
        if let Some(video) = video {
            let fallback = format!("video:{video}");
            let group = group.as_deref().unwrap_or(&fallback);
            tx.execute(
                "UPDATE videos SET trip_id=?1,split_group_id=?2,trip_revision=trip_revision+1 WHERE id=?3",
                params![value.trip_id, group, video],
            )?;
            tx.execute(
                "UPDATE media SET split_group_id=?1 WHERE source_video_id=?2",
                params![group, video],
            )?;
        } else {
            let group = group.unwrap_or_else(|| format!("image:{}", value.media_id));
            tx.execute(
                "UPDATE media SET split_group_id=?1 WHERE id=?2",
                params![group, value.media_id],
            )?;
        }
    }
    // The form is an explicit replacement of this image's tag set, never an inference side effect.
    tx.execute(
        "DELETE FROM media_tags WHERE media_id=?1",
        [&value.media_id],
    )?;
    for tag in &value.tags {
        tx.execute(
            "INSERT INTO media_tags(id,media_id,tag) VALUES(?1,?2,?3)",
            params![uuid::Uuid::new_v4().to_string(), value.media_id, tag],
        )?;
    }
    tx.commit()?;
    load(s, &value.media_id)
}
