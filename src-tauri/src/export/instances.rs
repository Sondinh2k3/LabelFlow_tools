use crate::{
    domain::{annotation::*, geometry, Result},
    services::annotation as a,
};
pub fn yolo_seg_row(g: &Geometry, width: u32, height: u32, class: u8) -> Result<String> {
    a::validate_geometry(g, width, height)?;
    let Geometry::Polygon { points, holes } = g else {
        return Err(a::invalid("YOLO segmentation requires one polygon exterior; boxes, RLE and disconnected components cannot be exported losslessly."));
    };
    if !holes.is_empty() {
        return Err(a::invalid(
            "YOLO segmentation cannot represent polygon holes. Use COCO RLE.",
        ));
    }
    let mut values = vec![class.to_string()];
    for p in points {
        values.push(format!("{:.10}", p.x / f64::from(width)));
        values.push(format!("{:.10}", p.y / f64::from(height)));
    }
    Ok(values.join(" "))
}
pub fn coco_annotation(
    shape: &Annotation,
    category: u8,
    image_id: u32,
    id: u32,
    width: u32,
    height: u32,
) -> Result<serde_json::Value> {
    a::validate_geometry(&shape.geometry, width, height)?;
    let bbox = geometry::bounds(&shape.geometry)?;
    let (segmentation, area, iscrowd) = match &shape.geometry {
        Geometry::Bbox { width, height, .. } => (None, width * height, 0),
        Geometry::Polygon { points, holes } if holes.is_empty() => (
            Some(serde_json::json!([points
                .iter()
                .flat_map(|p| [p.x, p.y])
                .collect::<Vec<_>>()])),
            polygon_area(points),
            0,
        ),
        Geometry::MultiPolygon { polygons } if polygons.iter().all(|p| p.holes.is_empty()) => (
            Some(serde_json::json!(polygons
                .iter()
                .map(|p| p.points.iter().flat_map(|p| [p.x, p.y]).collect::<Vec<_>>())
                .collect::<Vec<_>>())),
            polygons.iter().map(|p| polygon_area(&p.points)).sum(),
            0,
        ),
        other => {
            let pixels = geometry::rasterize(other, width, height)?;
            let counts = geometry::coco_counts(&pixels, width, height);
            (
                Some(
                    serde_json::json!({"size":[height,width],"counts":geometry::compress_coco(&counts)}),
                ),
                pixels.iter().map(|v| f64::from(*v)).sum(),
                u8::from(matches!(other, Geometry::Mask { iscrowd: true, .. })),
            )
        }
    };
    let mut value = serde_json::json!({"id":id,"image_id":image_id,"category_id":category,"bbox":bbox,"area":area,"iscrowd":iscrowd,"labelflow":{"id":shape.id,"source":shape.source,"layerId":shape.layer_id}});
    if let Some(segmentation) = segmentation {
        value["segmentation"] = segmentation;
    }
    Ok(value)
}
fn polygon_area(points: &[Point]) -> f64 {
    points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let q = &points[(i + 1) % points.len()];
            p.x * q.y - q.x * p.y
        })
        .sum::<f64>()
        .abs()
        / 2.0
}
