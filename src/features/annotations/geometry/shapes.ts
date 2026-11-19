import type { Geometry, Point } from '../../../lib/contracts';
export function clampPoint(p: Point, width: number, height: number): Point {
  return {
    x: Math.max(0, Math.min(width, p.x)),
    y: Math.max(0, Math.min(height, p.y)),
  };
}
export function boxFromPoints(
  a: Point,
  b: Point,
): Extract<Geometry, { type: 'bbox' }> {
  return {
    type: 'bbox',
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    width: Math.abs(a.x - b.x),
    height: Math.abs(a.y - b.y),
  };
}
const cross = (a: Point, b: Point, c: Point) =>
  (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
function on(a: Point, b: Point, p: Point) {
  return (
    Math.abs(cross(a, b, p)) < 1e-8 &&
    p.x >= Math.min(a.x, b.x) - 1e-8 &&
    p.x <= Math.max(a.x, b.x) + 1e-8 &&
    p.y >= Math.min(a.y, b.y) - 1e-8 &&
    p.y <= Math.max(a.y, b.y) + 1e-8
  );
}
export function segmentsIntersect(
  a: Point,
  b: Point,
  c: Point,
  d: Point,
): boolean {
  return (
    (cross(a, b, c) * cross(a, b, d) < 0 &&
      cross(c, d, a) * cross(c, d, b) < 0) ||
    on(a, b, c) ||
    on(a, b, d) ||
    on(c, d, a) ||
    on(c, d, b)
  );
}
export function validPolygon(points: Point[]): boolean {
  if (
    points.length < 3 ||
    points.length > 10000 ||
    points.some((p) => !Number.isFinite(p.x) || !Number.isFinite(p.y))
  )
    return false;
  let area = 0;
  for (let i = 0; i < points.length; i++) {
    const p = points[i]!,
      q = points[(i + 1) % points.length]!;
    if (Math.hypot(p.x - q.x, p.y - q.y) < 1e-8) return false;
    area += p.x * q.y - q.x * p.y;
    for (let j = i + 1; j < points.length; j++) {
      if (j === i + 1 || (i === 0 && j === points.length - 1)) continue;
      if (segmentsIntersect(p, q, points[j]!, points[(j + 1) % points.length]!))
        return false;
    }
  }
  return Math.abs(area) > 1e-6;
}
export function pointInRing(p: Point, points: Point[]): boolean {
  let inside = false;
  for (let i = 0; i < points.length; i++) {
    const a = points[i]!,
      b = points[(i + 1) % points.length]!;
    if (on(a, b, p)) return true;
    if (
      a.y > p.y !== b.y > p.y &&
      p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x
    )
      inside = !inside;
  }
  return inside;
}
export function hitShape(p: Point, g: Geometry): boolean {
  if (g.type === 'bbox')
    return (
      p.x >= g.x && p.y >= g.y && p.x < g.x + g.width && p.y < g.y + g.height
    );
  if (g.type === 'multiPolygon')
    return g.polygons.some((poly) => hitShape(p, { type: 'polygon', ...poly }));
  if (g.type === 'mask') {
    if (p.x < 0 || p.y < 0 || p.x >= g.width || p.y >= g.height) return false;
    const index = Math.floor(p.y) * g.width + Math.floor(p.x);
    let count = 0;
    for (let i = 0; i < g.runs.length; i += 2) {
      count += g.runs[i + 1]!;
      if (index < count) return g.runs[i] === 1;
    }
    return false;
  }
  return pointInRing(p, g.points) && !g.holes.some((h) => pointInRing(p, h));
}
export function moveShape(
  g: Geometry,
  dx: number,
  dy: number,
  width: number,
  height: number,
): Geometry {
  // Raster instance geometry remains pixel-exact; class assignment and deletion are supported.
  if (g.type === 'mask') return g;
  const polygons =
    g.type === 'multiPolygon' ? g.polygons : g.type === 'polygon' ? [g] : [];
  const points =
    g.type === 'bbox'
      ? [
          { x: g.x, y: g.y },
          { x: g.x + g.width, y: g.y + g.height },
        ]
      : polygons.flatMap((poly) => [...poly.points, ...poly.holes.flat()]);
  const xs = points.map((p) => p.x),
    ys = points.map((p) => p.y);
  dx = Math.max(-Math.min(...xs), Math.min(width - Math.max(...xs), dx));
  dy = Math.max(-Math.min(...ys), Math.min(height - Math.max(...ys), dy));
  const shift = (p: Point) => ({ x: p.x + dx, y: p.y + dy });
  if (g.type === 'bbox') return { ...g, x: g.x + dx, y: g.y + dy };
  const moved = polygons.map((poly) => ({
    points: poly.points.map(shift),
    holes: poly.holes.map((h) => h.map(shift)),
  }));
  return g.type === 'multiPolygon'
    ? { ...g, polygons: moved }
    : { ...g, ...moved[0]! };
}
export function boxToYolo(
  g: Extract<Geometry, { type: 'bbox' }>,
  width: number,
  height: number,
): number[] {
  return [
    (g.x + g.width / 2) / width,
    (g.y + g.height / 2) / height,
    g.width / width,
    g.height / height,
  ];
}
export function yoloToBox(
  values: number[],
  width: number,
  height: number,
): Extract<Geometry, { type: 'bbox' }> {
  const [cx, cy, w, h] = values as [number, number, number, number];
  return {
    type: 'bbox',
    x: (cx - w / 2) * width,
    y: (cy - h / 2) * height,
    width: w * width,
    height: h * height,
  };
}

export function validPolygonRings(points: Point[], holes: Point[][]): boolean {
  if (!validPolygon(points)) return false;
  for (let i = 0; i < holes.length; i++) {
    const hole = holes[i]!;
    if (!validPolygon(hole) || !hole.every((p) => pointInRing(p, points)))
      return false;
    for (const outer of [points, ...holes.slice(0, i)]) {
      if (
        hole.some((a, j) =>
          outer.some((b, k) =>
            segmentsIntersect(
              a,
              hole[(j + 1) % hole.length]!,
              b,
              outer[(k + 1) % outer.length]!,
            ),
          ),
        )
      )
        return false;
    }
    if (
      holes
        .slice(0, i)
        .some(
          (other) =>
            pointInRing(hole[0]!, other) || pointInRing(other[0]!, hole),
        )
    )
      return false;
  }
  return true;
}
