pub mod process;
pub mod tools;
use crate::{
    domain::{
        annotation::{ImportReport, Progress},
        video::*,
        ErrorCode, Result,
    },
    services::{annotation as a, media, project::ProjectSession},
};
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use std::{
    fs,
    io::Read,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tools::Toolchain;

pub fn source_sha(path: &Path) -> Result<String> {
    source_sha_cancel(path, &AtomicBool::new(false))
}
pub fn source_sha_cancel(path: &Path, cancel: &AtomicBool) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut file = fs::File::open(path)?;
    let mut buffer = [0; 65536];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(crate::domain::AppError::new(
                ErrorCode::Cancelled,
                "Source verification cancelled.",
            ));
        }
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
fn source_stamp(path: &Path) -> Result<(u64, std::time::SystemTime)> {
    let meta = fs::metadata(path)?;
    Ok((meta.len(), meta.modified()?))
}
fn number(value: Option<&serde_json::Value>) -> Option<f64> {
    value
        .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
        .filter(|v| v.is_finite())
}
fn integer(value: Option<&serde_json::Value>) -> Option<i64> {
    value.and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
}
fn rational(text: &str) -> Result<(i64, i64)> {
    let (a, b) = text
        .split_once('/')
        .ok_or_else(|| a::invalid("Video time base is missing."))?;
    let a = a
        .parse::<i64>()
        .map_err(|_| a::invalid("Invalid time base."))?;
    let b = b
        .parse::<i64>()
        .map_err(|_| a::invalid("Invalid time base."))?;
    if a <= 0 || b <= 0 || a > 1_000_000_000 || b > 1_000_000_000 {
        return Err(a::invalid("Unsupported video time base."));
    }
    Ok((a, b))
}
#[derive(Deserialize)]
struct ProbeFrames {
    frames: Vec<ProbeFrame>,
}
#[derive(Deserialize)]
struct ProbeFrame {
    best_effort_timestamp: Option<i64>,
    duration: Option<i64>,
    pkt_duration: Option<i64>,
}
pub fn probe(
    tools: &Toolchain,
    path: &Path,
    cancel: &AtomicBool,
) -> Result<(VideoInfo, Vec<FrameInfo>)> {
    let path = path.canonicalize()?;
    if !path.is_file() || fs::metadata(&path)?.len() > 64 * 1024 * 1024 * 1024 {
        return Err(a::invalid("Video must be a local file within 64 GiB."));
    }
    let hash = source_sha_cancel(&path, cancel)?;
    let arguments = vec![
        "-v".into(),
        "error".into(),
        "-protocol_whitelist".into(),
        "file,pipe".into(),
        "-select_streams".into(),
        "v:0".into(),
        "-show_streams".into(),
        "-show_format".into(),
        "-of".into(),
        "json".into(),
        path.to_string_lossy().into_owned(),
    ];
    let output = process::run(
        &tools.ffprobe,
        &arguments,
        cancel,
        2 * 1024 * 1024,
        Duration::from_secs(60),
    )?;
    let json: serde_json::Value =
        serde_json::from_slice(&output).map_err(|e| a::invalid(e.to_string()))?;
    let stream = json["streams"]
        .as_array()
        .and_then(|a| a.first())
        .ok_or_else(|| a::invalid("No video stream found."))?;
    let width = integer(stream.get("width"))
        .filter(|v| *v > 0)
        .ok_or_else(|| a::invalid("Missing video dimensions."))? as u32;
    let height = integer(stream.get("height"))
        .filter(|v| *v > 0)
        .ok_or_else(|| a::invalid("Missing video dimensions."))? as u32;
    if u64::from(width) * u64::from(height) > media::MAX_PIXELS {
        return Err(a::invalid("Video frames exceed the image pixel budget."));
    }
    let rotation = stream["side_data_list"]
        .as_array()
        .and_then(|list| list.iter().find_map(|item| integer(item.get("rotation"))))
        .or_else(|| integer(stream["tags"].get("rotate")))
        .unwrap_or(0) as i32;
    if rotation % 90 != 0 {
        return Err(a::invalid(
            "Only right-angle video rotations are supported; normalize this source externally.",
        ));
    }
    let (num, den) = rational(stream["time_base"].as_str().unwrap_or_default())?;
    let average_fps = stream["avg_frame_rate"]
        .as_str()
        .and_then(|v| rational(v).ok())
        .map_or(0.0, |(n, d)| n as f64 / d as f64);
    let arguments = vec![
        "-v".into(),
        "error".into(),
        "-protocol_whitelist".into(),
        "file,pipe".into(),
        "-select_streams".into(),
        "v:0".into(),
        "-show_frames".into(),
        "-show_entries".into(),
        "frame=best_effort_timestamp,duration,pkt_duration".into(),
        "-of".into(),
        "json".into(),
        path.to_string_lossy().into_owned(),
    ];
    let bytes = process::run(
        &tools.ffprobe,
        &arguments,
        cancel,
        64 * 1024 * 1024,
        Duration::from_secs(1800),
    )?;
    let raw: ProbeFrames = serde_json::from_slice(&bytes).map_err(|e| a::invalid(e.to_string()))?;
    if raw.frames.is_empty() || raw.frames.len() > 100_000 {
        return Err(a::invalid(
            "M2 indexes 1–100000 frames per video. Split longer clips before importing.",
        ));
    }
    let mut last_pts = None;
    for frame in &raw.frames {
        let pts = frame
            .best_effort_timestamp
            .ok_or_else(|| a::invalid("A source frame is missing its timestamp."))?;
        if pts.unsigned_abs() > 9_007_199_254_740_991 || last_pts.is_some_and(|last| pts < last) {
            return Err(a::invalid(
                "Source timestamps are non-monotonic or cannot be represented precisely.",
            ));
        }
        last_pts = Some(pts);
    }
    let first = raw.frames[0]
        .best_effort_timestamp
        .ok_or_else(|| a::invalid("Source frame has no presentation timestamp."))?;
    let scale = 1000.0 * num as f64 / den as f64;
    let mut frames = Vec::with_capacity(raw.frames.len());
    let mut previous = first;
    for (i, frame) in raw.frames.iter().enumerate() {
        let pts = frame
            .best_effort_timestamp
            .ok_or_else(|| a::invalid("A source frame is missing its timestamp."))?;
        if pts < previous || pts.unsigned_abs() > 9_007_199_254_740_991 {
            return Err(a::invalid(
                "Source timestamps are non-monotonic or cannot be represented precisely.",
            ));
        }
        previous = pts;
        let duration = raw
            .frames
            .get(i + 1)
            .and_then(|f| f.best_effort_timestamp)
            .map(|next| (next - pts) as f64 * scale)
            .or_else(|| {
                frame
                    .duration
                    .or(frame.pkt_duration)
                    .filter(|n| *n > 0)
                    .map(|n| n as f64 * scale)
            })
            .unwrap_or(if average_fps > 0.0 {
                1000.0 / average_fps
            } else {
                0.0
            });
        frames.push(FrameInfo {
            index: i as u32,
            pts,
            time_ms: (pts - first) as f64 * scale,
            duration_ms: duration,
        });
    }
    if source_sha_cancel(&path, cancel)? != hash {
        return Err(a::invalid(
            "Source changed during video indexing. Retry after recording/copying has finished.",
        ));
    }
    let last = frames.last().unwrap();
    let duration_ms = (last.time_ms + last.duration_ms)
        .max(number(json["format"].get("duration")).unwrap_or(0.0) * 1000.0);
    Ok((
        VideoInfo {
            id: uuid::Uuid::new_v4().to_string(),
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            path: path.to_string_lossy().into_owned(),
            sha256: hash.clone(),
            codec: stream["codec_name"].as_str().unwrap_or("unknown").into(),
            width,
            height,
            rotation,
            time_base_num: num,
            time_base_den: den,
            start_pts: first,
            duration_ms,
            average_fps,
            frame_count: frames.len() as u32,
            split_group: format!("video:{hash}"),
        },
        frames,
    ))
}
pub fn import(
    session: &mut ProjectSession,
    tools: &Toolchain,
    path: &Path,
    cancel: &AtomicBool,
) -> Result<VideoInfo> {
    let job = crate::jobs::start(&session.connection, "probe_video", &a::json(&path)?)?;
    let result = (|| {
        let (video, frames) = probe(tools, path, cancel)?;
        let existing: Option<String> = session
            .connection
            .query_row(
                "SELECT id FROM videos WHERE content_sha256=?1",
                [&video.sha256],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            return get(session, &id);
        }
        let tx = session.connection.transaction()?;
        tx.execute("INSERT INTO videos(id,project_id,path,content_sha256,codec,width,height,rotation,time_base_num,time_base_den,start_pts,duration_ms,average_fps,frame_count,split_group_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",params![video.id,session.project.id,video.path,video.sha256,video.codec,video.width,video.height,video.rotation,video.time_base_num,video.time_base_den,video.start_pts,video.duration_ms,video.average_fps,video.frame_count,video.split_group])?;
        {
            let mut insert=tx.prepare("INSERT INTO video_frames(video_id,frame_index,pts,time_ms,duration_ms) VALUES(?1,?2,?3,?4,?5)")?;
            for f in frames {
                insert.execute(params![video.id, f.index, f.pts, f.time_ms, f.duration_ms])?;
            }
        }
        tx.commit()?;
        Ok(video)
    })();
    finalize(session, &job, &result);
    result
}
fn finalize<T>(s: &ProjectSession, job: &str, result: &Result<T>) {
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
pub fn get(s: &ProjectSession, id: &str) -> Result<VideoInfo> {
    uuid::Uuid::parse_str(id).map_err(|_| a::invalid("Invalid video ID."))?;
    Ok(s.connection.query_row("SELECT id,path,content_sha256,codec,width,height,rotation,time_base_num,time_base_den,start_pts,duration_ms,average_fps,frame_count,split_group_id FROM videos WHERE id=?1",[id],|r|{let path:String=r.get(1)?;Ok(VideoInfo{id:r.get(0)?,name:Path::new(&path).file_name().unwrap_or_default().to_string_lossy().into_owned(),path,sha256:r.get(2)?,codec:r.get(3)?,width:r.get(4)?,height:r.get(5)?,rotation:r.get(6)?,time_base_num:r.get(7)?,time_base_den:r.get(8)?,start_pts:r.get(9)?,duration_ms:r.get(10)?,average_fps:r.get(11)?,frame_count:r.get(12)?,split_group:r.get(13)?})})?)
}
pub fn list(s: &ProjectSession) -> Result<Vec<VideoInfo>> {
    let mut stmt = s
        .connection
        .prepare("SELECT id FROM videos ORDER BY created_at,id")?;
    let ids = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ids.iter().map(|id| get(s, id)).collect()
}
pub fn frames(s: &ProjectSession, id: &str, offset: u32, limit: u32) -> Result<VideoPage> {
    if limit == 0 || limit > 1000 {
        return Err(a::invalid("Frame page limit must be 1–1000."));
    }
    let video = get(s, id)?;
    let mut stmt=s.connection.prepare("SELECT frame_index,pts,time_ms,duration_ms FROM video_frames WHERE video_id=?1 ORDER BY frame_index LIMIT ?2 OFFSET ?3")?;
    let frames = stmt
        .query_map(params![id, limit, offset], |r| {
            Ok(FrameInfo {
                index: r.get(0)?,
                pts: r.get(1)?,
                time_ms: r.get(2)?,
                duration_ms: r.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(VideoPage { video, frames })
}
pub fn frame(s: &ProjectSession, id: &str, index: u32) -> Result<FrameInfo> {
    Ok(s.connection.query_row("SELECT frame_index,pts,time_ms,duration_ms FROM video_frames WHERE video_id=?1 AND frame_index=?2",params![id,index],|r|Ok(FrameInfo{index:r.get(0)?,pts:r.get(1)?,time_ms:r.get(2)?,duration_ms:r.get(3)?}))?)
}
pub fn seek(s: &ProjectSession, id: &str, time_ms: f64) -> Result<FrameInfo> {
    if !time_ms.is_finite() || time_ms < 0.0 {
        return Err(a::invalid("Invalid seek time."));
    }
    let mut candidates = Vec::new();
    for clause in [
        "time_ms<=?2 ORDER BY time_ms DESC,frame_index ASC",
        "time_ms>=?2 ORDER BY time_ms ASC,frame_index ASC",
    ] {
        let sql = format!(
            "SELECT frame_index,time_ms FROM video_frames WHERE video_id=?1 AND {clause} LIMIT 1"
        );
        if let Some(value) = s
            .connection
            .query_row(&sql, params![id, time_ms], |r| {
                Ok((r.get::<_, u32>(0)?, r.get::<_, f64>(1)?))
            })
            .optional()?
        {
            candidates.push(value);
        }
    }
    let index = candidates
        .into_iter()
        .min_by(|a, b| {
            (a.1 - time_ms)
                .abs()
                .total_cmp(&(b.1 - time_ms).abs())
                .then_with(|| a.0.cmp(&b.0))
        })
        .ok_or_else(|| a::invalid("Video has no indexed frames."))?
        .0;
    frame(s, id, index)
}
pub fn repair(s: &mut ProjectSession, id: &str, path: &Path) -> Result<()> {
    let video = get(s, id)?;
    let path = path.canonicalize()?;
    if source_sha(&path)? != video.sha256 {
        return Err(a::invalid("Replacement video must match the exact original SHA-256. Frame identities and annotations were preserved."));
    }
    s.connection.execute(
        "UPDATE videos SET path=?1 WHERE id=?2",
        params![path.to_string_lossy(), id],
    )?;
    Ok(())
}
pub fn decode(
    tools: &Toolchain,
    video: &VideoInfo,
    frame: &FrameInfo,
    format: &str,
    quality: u8,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    if !["png", "jpeg"].contains(&format) || !(1..=100).contains(&quality) {
        return Err(a::invalid("Choose PNG or JPEG with quality 1–100."));
    }
    // Select by decoded frame index rather than FPS arithmetic: exact for VFR and B frames.
    let mut args = vec![
        "-v".into(),
        "error".into(),
        "-nostdin".into(),
        "-xerror".into(),
        "-threads".into(),
        "1".into(),
        "-protocol_whitelist".into(),
        "file,pipe".into(),
        "-i".into(),
        video.path.clone(),
        "-map".into(),
        "0:v:0".into(),
        "-an".into(),
        "-sn".into(),
        "-vf".into(),
        format!("select=eq(n\\,{})", frame.index),
        "-frames:v".into(),
        "1".into(),
        "-fps_mode".into(),
        "passthrough".into(),
        "-threads".into(),
        "1".into(),
        "-c:v".into(),
        if format == "png" { "png" } else { "mjpeg" }.into(),
    ];
    if format == "jpeg" {
        args.extend([
            "-q:v".into(),
            (31 - u32::from(quality) * 29 / 100).to_string(),
        ]);
    }
    args.extend(["-f".into(), "image2pipe".into(), "pipe:1".into()]);
    let bytes = process::run(
        &tools.ffmpeg,
        &args,
        cancel,
        128 * 1024 * 1024,
        Duration::from_secs(600),
    )?;
    let decoded = image::load_from_memory(&bytes)
        .map_err(|e| a::invalid(format!("Frame decode failed: {e}")))?;
    let expected = if video.rotation.rem_euclid(180) == 90 {
        (video.height, video.width)
    } else {
        (video.width, video.height)
    };
    if (decoded.width(), decoded.height()) != expected {
        return Err(a::invalid(
            "Decoded frame dimensions disagree with normalized video metadata.",
        ));
    }
    Ok(bytes)
}
pub fn selected_indices(s: &ProjectSession, options: &ExtractionOptions) -> Result<Vec<u32>> {
    if !["png", "jpeg"].contains(&options.format.as_str()) || !(1..=100).contains(&options.quality)
    {
        return Err(a::invalid("Choose PNG or JPEG with quality 1–100."));
    }
    let video = get(s, &options.video_id)?;
    let mut indices = options.indices.clone();
    if let Some(interval) = options.interval_ms {
        if !interval.is_finite()
            || interval < 1.0
            || !options.start_ms.is_finite()
            || !options.end_ms.is_finite()
            || options.start_ms < 0.0
            || options.end_ms < options.start_ms
            || options.end_ms > video.duration_ms + 1.0
        {
            return Err(a::invalid("Invalid interval extraction range."));
        }
        let count = ((options.end_ms - options.start_ms) / interval).floor() as usize + 1;
        if count > 10000 {
            return Err(a::invalid("Extract at most 10000 frames per job."));
        }
        for step in 0..count {
            indices.push(seek(s, &video.id, options.start_ms + step as f64 * interval)?.index);
        }
    }
    indices.sort_unstable();
    indices.dedup();
    if indices.is_empty()
        || indices.len() > 10000
        || indices.iter().any(|i| *i >= video.frame_count)
    {
        return Err(a::invalid("Select 1–10000 valid frames."));
    }
    Ok(indices)
}
pub fn extract(
    s: &mut ProjectSession,
    tools: &Toolchain,
    options: &ExtractionOptions,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<ImportReport> {
    let job = crate::jobs::start(&s.connection, "extract_video", &a::json(options)?)?;
    let result = (|| {
        let video = get(s, &options.video_id)?;
        if source_sha_cancel(Path::new(&video.path), cancel)? != video.sha256 {
            return Err(a::invalid(
                "Video source changed. Existing frames and annotations were preserved.",
            ));
        }
        let stamp = source_stamp(Path::new(&video.path))?;
        let indices = selected_indices(s, options)?;
        let total = indices.len() as u32;
        let mut report = ImportReport::default();
        let dir = s.root.join("media").join(format!("video-{}", video.id));
        crate::security::reject_symlink(&s.root.join("media"))?;
        crate::security::reject_symlink(&dir)?;
        fs::create_dir_all(&dir)?;
        for (i, index) in indices.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                report.cancelled = true;
                break;
            }
            let exists: bool = s.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM media WHERE source_video_id=?1 AND frame_index=?2)",
                params![video.id, index],
                |r| r.get(0),
            )?;
            if exists {
                report.duplicates += 1;
            } else {
                let frame = frame(s, &video.id, *index)?;
                let bytes = match decode(
                    tools,
                    &video,
                    &frame,
                    &options.format,
                    options.quality,
                    cancel,
                ) {
                    Ok(bytes) => bytes,
                    Err(e) if e.code == ErrorCode::Cancelled => {
                        report.cancelled = true;
                        break;
                    }
                    Err(e) => return Err(e),
                };
                if source_stamp(Path::new(&video.path))? != stamp {
                    return Err(a::invalid(
                        "Video source changed during extraction; completed frames were preserved.",
                    ));
                }
                let extension = if options.format == "png" {
                    "png"
                } else {
                    "jpg"
                };
                let path = dir.join(format!("frame-{index:08}.{extension}"));
                a::atomic_file(&path, &bytes)?;
                let (info, _) = media::load(&path)?;
                let relative = path
                    .strip_prefix(&s.root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                let extraction = serde_json::json!({"format":options.format,"quality":options.quality,"sourceSha256":video.sha256,"pts":frame.pts,"timeBase":[video.time_base_num,video.time_base_den],"selection":"decoded frame index; exact PTS index"});
                s.connection.execute("INSERT INTO media(id,project_id,type,path,import_mode,content_sha256,width,height,orientation,source_video_id,frame_index,frame_time_ms,frame_pts,split_group_id,extraction_json) VALUES(?1,?2,'image',?3,'copied',?4,?5,?6,1,?7,?8,?9,?10,?11,?12)",params![info.id,s.project.id,relative,info.sha256,info.width,info.height,video.id,index,frame.time_ms,frame.pts,video.split_group,a::json(&extraction)?])?;
                report.imported += 1;
            }
            crate::jobs::progress(&s.connection, &job, i as u32 + 1, total);
            progress(Progress {
                completed: i as u32 + 1,
                total,
                message: format!("Frame {index}"),
            });
        }
        Ok(report)
    })();
    if result.as_ref().is_ok_and(|r| r.cancelled) {
        crate::jobs::finish(&s.connection, &job, "cancelled", None);
    } else {
        finalize(s, &job, &result);
    }
    result
}
