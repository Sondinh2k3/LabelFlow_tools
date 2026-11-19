use super::{annotation::*, Result};
use crate::services::annotation as a;

pub fn mask_pixels(width: u32, height: u32, runs: &[u32]) -> Result<Vec<u8>> {
    let pixels = a::unpack(&RasterMask {
        layer_id: String::new(),
        width,
        height,
        runs: runs.to_vec(),
    })?;
    if pixels.iter().any(|v| *v > 1) {
        return Err(a::invalid("Instance masks must be binary."));
    }
    if !pixels.contains(&1) {
        return Err(a::invalid("Instance mask is empty."));
    }
    Ok(pixels)
}
pub fn polygon_contains(p: &Point, points: &[Point], holes: &[Vec<Point>]) -> bool {
    a::inside(p, points) && !holes.iter().any(|hole| a::inside(p, hole))
}
pub fn rasterize(g: &Geometry, width: u32, height: u32) -> Result<Vec<u8>> {
    a::validate_geometry(g, width, height)?;
    if let Geometry::Mask {
        width,
        height,
        runs,
        ..
    } = g
    {
        return mask_pixels(*width, *height, runs);
    }
    let mut pixels = vec![0; width as usize * height as usize];
    let [x, y, w, h] = bounds(g)?;
    for py in (y.floor().max(0.0) as u32)..((y + h).ceil().min(f64::from(height)) as u32) {
        for px in (x.floor().max(0.0) as u32)..((x + w).ceil().min(f64::from(width)) as u32) {
            let p = Point {
                x: f64::from(px) + 0.5,
                y: f64::from(py) + 0.5,
            };
            let hit = match g {
                Geometry::Bbox {
                    x,
                    y,
                    width,
                    height,
                } => p.x >= *x && p.y >= *y && p.x < x + width && p.y < y + height,
                Geometry::Polygon { points, holes } => polygon_contains(&p, points, holes),
                Geometry::MultiPolygon { polygons } => polygons
                    .iter()
                    .any(|poly| polygon_contains(&p, &poly.points, &poly.holes)),
                Geometry::Mask { .. } => unreachable!(),
            };
            pixels[py as usize * width as usize + px as usize] = u8::from(hit);
        }
    }
    Ok(pixels)
}
pub fn bounds(g: &Geometry) -> Result<[f64; 4]> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    match g {
        Geometry::Bbox {
            x,
            y,
            width,
            height,
        } => return Ok([*x, *y, *width, *height]),
        Geometry::Mask {
            width,
            height,
            runs,
            ..
        } => {
            let pixels = mask_pixels(*width, *height, runs)?;
            for (i, &value) in pixels.iter().enumerate() {
                if value == 1 {
                    let x = (i % *width as usize) as f64;
                    let y = (i / *width as usize) as f64;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x + 1.0);
                    max_y = max_y.max(y + 1.0);
                }
            }
        }
        _ => {
            let points: Vec<&Point> = match g {
                Geometry::Polygon { points, .. } => points.iter().collect(),
                Geometry::MultiPolygon { polygons } => {
                    polygons.iter().flat_map(|p| p.points.iter()).collect()
                }
                _ => unreachable!(),
            };
            for p in points {
                min_x = min_x.min(p.x);
                min_y = min_y.min(p.y);
                max_x = max_x.max(p.x);
                max_y = max_y.max(p.y);
            }
        }
    }
    if !min_x.is_finite() || max_x <= min_x || max_y <= min_y {
        return Err(a::invalid("Geometry has no positive extent."));
    }
    Ok([min_x, min_y, max_x - min_x, max_y - min_y])
}

// COCO RLE scans columns first, starts with background, unlike LabelFlow value/count RLE.
pub fn coco_counts(pixels: &[u8], width: u32, height: u32) -> Vec<u32> {
    let mut counts = vec![0];
    let mut last = 0;
    for x in 0..width as usize {
        for y in 0..height as usize {
            let value = pixels[y * width as usize + x];
            if value != last {
                counts.push(0);
                last = value;
            }
            *counts.last_mut().unwrap() += 1;
        }
    }
    counts
}
pub fn decode_coco_counts(counts: &[u32], width: u32, height: u32) -> Result<Vec<u8>> {
    let len = u64::from(width) * u64::from(height);
    if len == 0
        || len > crate::services::media::MAX_PIXELS
        || counts.len() as u64 > len * 2 + 1
        || counts.iter().map(|&n| u64::from(n)).sum::<u64>() != len
    {
        return Err(a::invalid("COCO RLE dimensions/count coverage is invalid."));
    }
    let mut pixels = vec![0; len as usize];
    let mut index = 0usize;
    for (run, &count) in counts.iter().enumerate() {
        for _ in 0..count {
            pixels[(index % height as usize) * width as usize + index / height as usize] =
                (run % 2) as u8;
            index += 1;
        }
    }
    Ok(pixels)
}
pub fn compress_coco(counts: &[u32]) -> String {
    let mut out = String::new();
    for (i, &count) in counts.iter().enumerate() {
        let mut value = i64::from(count);
        if i > 2 {
            value -= i64::from(counts[i - 2]);
        }
        loop {
            let mut byte = (value & 31) as u8;
            value >>= 5;
            let more = if byte & 16 != 0 {
                value != -1
            } else {
                value != 0
            };
            if more {
                byte |= 32;
            }
            out.push(char::from(byte + 48));
            if !more {
                break;
            }
        }
    }
    out
}
pub fn decompress_coco(text: &str, max_pixels: u64) -> Result<Vec<u32>> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let mut value = 0i64;
        let mut shift = 0;
        loop {
            let byte = *bytes
                .get(i)
                .ok_or_else(|| a::invalid("Truncated compressed COCO RLE."))?;
            if !(48..=111).contains(&byte) || shift > 55 {
                return Err(a::invalid("Invalid compressed COCO RLE."));
            }
            i += 1;
            let part = byte - 48;
            value |= i64::from(part & 31) << shift;
            shift += 5;
            if part & 32 == 0 {
                if part & 16 != 0 {
                    value |= -(1i64 << shift);
                }
                break;
            }
        }
        if out.len() > 2 {
            value += i64::from(out[out.len() - 2]);
        }
        if value < 0 || value as u64 > max_pixels {
            return Err(a::invalid("COCO RLE run exceeds dimensions."));
        }
        out.push(value as u32);
        if out.len() as u64 > max_pixels * 2 + 1 {
            return Err(a::invalid("Too many COCO RLE runs."));
        }
    }
    Ok(out)
}
