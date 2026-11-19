use crate::{
    domain::{AppError, ErrorCode, ImageInfo, ImagePreview, Result},
    repositories,
    services::project::ProjectSession,
};
use base64::Engine;
use image::{ImageDecoder, ImageFormat, ImageReader, Limits};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Cursor, Read},
    path::{Component, Path},
};

pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 24_000_000;
fn invalid(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::InvalidImage, message)
}

pub fn load(path: &Path) -> Result<(ImageInfo, image::DynamicImage)> {
    let path = path.canonicalize()?;
    if path.to_str().is_none() {
        return Err(invalid("Image paths must be valid UTF-8."));
    }
    let source = File::open(&path)?;
    if !source.metadata()?.is_file() {
        return Err(invalid("Select a regular image file."));
    }
    let mut bytes = Vec::new();
    source.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(invalid("Image exceeds the M0 64 MiB source limit."));
    }
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| invalid(e.to_string()))?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP)
    ) {
        return Err(invalid("Supported image formats: JPEG, PNG, WebP."));
    }
    let mut limits = Limits::default();
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|e| invalid(e.to_string()))?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(invalid(
            "Image exceeds the M0 24 megapixel limit or has invalid dimensions.",
        ));
    }
    let orientation = decoder.orientation().map_err(|e| invalid(e.to_string()))?;
    let original_orientation = orientation.to_exif();
    let mut decoded =
        image::DynamicImage::from_decoder(decoder).map_err(|e| invalid(e.to_string()))?;
    decoded.apply_orientation(orientation);
    let info = ImageInfo {
        id: uuid::Uuid::new_v4().to_string(),
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        path: path.to_string_lossy().into_owned(),
        width: decoded.width(),
        height: decoded.height(),
        orientation: original_orientation,
        sha256: hash,
    };
    Ok((info, decoded))
}
pub fn decode(path: &Path) -> Result<ImagePreview> {
    let (info, decoded) = load(path)?;
    let mut encoded = Cursor::new(Vec::new());
    decoded
        .write_to(&mut encoded, ImageFormat::Png)
        .map_err(|e| invalid(e.to_string()))?;
    let data_url = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(encoded.into_inner())
    );
    Ok(ImagePreview { info, data_url })
}

pub fn open(session: &mut ProjectSession, path: &Path) -> Result<ImagePreview> {
    let mut preview = decode(path)?;
    let canonical = path.canonicalize()?;
    let stored = canonical
        .strip_prefix(&session.root)
        .unwrap_or(&canonical)
        .to_string_lossy()
        .into_owned();
    repositories::remember_image(
        &mut session.connection,
        &session.project.id,
        &mut preview.info,
        &stored,
    )?;
    Ok(preview)
}

pub fn restore(session: &ProjectSession) -> Result<Option<ImagePreview>> {
    let Some(saved) = repositories::active_image(&session.connection)? else {
        return Ok(None);
    };
    let path = Path::new(&saved.path);
    if !path.is_absolute()
        && path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(AppError::new(
            ErrorCode::InvalidProject,
            "Linked image contains an unsafe relative path.",
        ));
    }
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        session.root.join(path)
    };
    let mut preview = decode(&resolved)?;
    if preview.info.sha256 != saved.sha256
        || preview.info.width != saved.width
        || preview.info.height != saved.height
        || preview.info.orientation != saved.orientation
    {
        return Err(invalid(
            "The linked image has changed. Open the image deliberately to use its new contents.",
        ));
    }
    preview.info.id = saved.id;
    Ok(Some(preview))
}
