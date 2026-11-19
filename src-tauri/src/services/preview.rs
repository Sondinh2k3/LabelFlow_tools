use super::{annotation as a, library, project::ProjectSession};
use crate::domain::{ImageInfo, Result};
use std::{
    fs,
    io::{BufReader, Cursor, Read},
    path::PathBuf,
};
pub fn cached(s: &ProjectSession, id: &str) -> Result<(ImageInfo, PathBuf)> {
    uuid::Uuid::parse_str(id).map_err(|_| a::invalid("Invalid media ID."))?;
    let record = library::record(s, id)?;
    if record.sha256.len() != 64 || !record.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(a::invalid("Invalid source hash."));
    }
    // Validate content identity even when a preview is cached: a changed linked file cannot inherit labels.
    let source = library::resolve(s, &record.path)?;
    if crate::video::source_sha(&source)? != record.sha256 {
        return Err(a::invalid(
            "Linked source changed. Repair its original reference before editing.",
        ));
    }
    let info = ImageInfo {
        id: record.id.clone(),
        name: record.name.clone(),
        path: source.to_string_lossy().into_owned(),
        width: record.width,
        height: record.height,
        orientation: record.orientation,
        sha256: record.sha256.clone(),
    };
    // A verified, nonanimated PNG already in canonical orientation needs no copy.
    // APNG and transformed formats still use the canonical, static lossless preview.
    if record.orientation == 1 && static_png(&source, (record.width, record.height)) {
        return Ok((info, source));
    }
    let directory = super::storage::CacheKind::Previews.directory(s)?;
    crate::security::reject_symlink(&directory)?;
    fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{}-{}.png", record.id, record.sha256));
    crate::security::reject_symlink(&path)?;
    let mut added = 0;
    if !valid(&path, (record.width, record.height)) {
        let (_, image) = library::checked_load(s, &library::record(s, id)?)?;
        let mut output = Cursor::new(Vec::new());
        image
            .write_to(&mut output, image::ImageFormat::Png)
            .map_err(|e| a::invalid(e.to_string()))?;
        let bytes = output.into_inner();
        added = bytes.len() as u64;
        store(&path, &bytes)?;
    }
    super::storage::trim_cache(s, super::storage::CacheKind::Previews, &path, added)?;
    Ok((info, path))
}
fn static_png(path: &std::path::Path, dimensions: (u32, u32)) -> bool {
    (|| -> std::io::Result<bool> {
        let mut file = fs::File::open(path)?;
        let mut magic = [0; 8];
        file.read_exact(&mut magic)?;
        if magic != *b"\x89PNG\r\n\x1a\n" {
            return Ok(false);
        }
        let mut decoder = png::Decoder::new(BufReader::new(fs::File::open(path)?));
        decoder.set_limits(png::Limits {
            bytes: 128 * 1024 * 1024,
        });
        decoder.set_ignore_text_chunk(true);
        let reader = decoder.read_info().map_err(std::io::Error::other)?;
        let info = reader.info();
        Ok(info.animation_control.is_none() && (info.width, info.height) == dimensions)
    })()
    .unwrap_or(false)
}

pub fn valid(path: &std::path::Path, dimensions: (u32, u32)) -> bool {
    (|| -> Result<bool> {
        if fs::metadata(path)?.len() > 128 * 1024 * 1024 {
            return Ok(false);
        }
        let tag = path.with_extension("sha256");
        crate::security::reject_symlink(&tag)?;
        let mut expected = String::new();
        fs::File::open(tag)?
            .take(65)
            .read_to_string(&mut expected)?;
        Ok(expected.len() == 64
            && a::sha(&fs::read(path)?) == expected
            && image::image_dimensions(path).ok() == Some(dimensions))
    })()
    .unwrap_or(false)
}
pub fn store(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    let tag = path.with_extension("sha256");
    for target in [path, tag.as_path()] {
        crate::security::reject_symlink(target)?;
        if target.exists() {
            fs::remove_file(target)?;
        }
    }
    a::atomic_file(path, bytes)?;
    a::atomic_file(&tag, a::sha(bytes).as_bytes())?;
    Ok(())
}
