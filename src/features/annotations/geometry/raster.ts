import type { Point, RasterMask } from '../../../lib/contracts';
export interface Bounds {
  x: number;
  y: number;
  width: number;
  height: number;
}
export function paintSegment(
  pixels: Uint8Array,
  width: number,
  height: number,
  a: Point,
  b: Point,
  radius: number,
  value: number,
): Bounds {
  const left = Math.max(0, Math.floor(Math.min(a.x, b.x) - radius)),
    top = Math.max(0, Math.floor(Math.min(a.y, b.y) - radius)),
    right = Math.min(width, Math.ceil(Math.max(a.x, b.x) + radius)),
    bottom = Math.min(height, Math.ceil(Math.max(a.y, b.y) + radius));
  const dx = b.x - a.x,
    dy = b.y - a.y,
    length = dx * dx + dy * dy;
  const len = Math.sqrt(length),
    nx = len ? (-dy / len) * radius : 0,
    ny = len ? (dx / len) * radius : 0;
  const corners = [
    { x: a.x + nx, y: a.y + ny },
    { x: b.x + nx, y: b.y + ny },
    { x: b.x - nx, y: b.y - ny },
    { x: a.x - nx, y: a.y - ny },
  ];
  const hit = (x: number, y: number) => {
    const px = x + 0.5,
      py = y + 0.5,
      t = length
        ? Math.max(0, Math.min(1, ((px - a.x) * dx + (py - a.y) * dy) / length))
        : 0;
    return (
      (px - a.x - t * dx) ** 2 + (py - a.y - t * dy) ** 2 <= radius * radius
    );
  };
  // A capsule is convex: each scanline is one interval. Compute its continuous
  // bounds from end disks + body, then use the original predicate at pixel edges
  // to keep subpixel/tangent behavior exact despite floating-point rounding.
  for (let y = top; y < bottom; y++) {
    const py = y + 0.5;
    let lo = Infinity,
      hi = -Infinity;
    for (const p of [a, b]) {
      const delta = py - p.y;
      if (Math.abs(delta) <= radius) {
        const span = Math.sqrt(Math.max(0, radius * radius - delta * delta));
        lo = Math.min(lo, p.x - span);
        hi = Math.max(hi, p.x + span);
      }
    }
    if (length)
      for (let i = 0; i < 4; i++) {
        const p = corners[i]!,
          q = corners[(i + 1) % 4]!;
        if (p.y === q.y) {
          if (py === p.y) {
            lo = Math.min(lo, p.x, q.x);
            hi = Math.max(hi, p.x, q.x);
          }
          continue;
        }
        const t = (py - p.y) / (q.y - p.y);
        if (t >= 0 && t <= 1) {
          const x = p.x + t * (q.x - p.x);
          lo = Math.min(lo, x);
          hi = Math.max(hi, x);
        }
      }
    if (lo > hi) continue;
    let start = Math.max(left, Math.floor(lo - 0.5) - 1),
      end = Math.min(right, Math.ceil(hi - 0.5) + 2);
    while (start < end && !hit(start, y)) start++;
    while (end > start && !hit(end - 1, y)) end--;
    if (start < end) pixels.fill(value, y * width + start, y * width + end);
  }
  return {
    x: left,
    y: top,
    width: Math.max(0, right - left),
    height: Math.max(0, bottom - top),
  };
}
/**
 * Fills a simple polygon (even-odd rule, pixel centers) with `value`. Scanline
 * crossings keep it O(rows × edges) with no per-pixel point-in-polygon test.
 */
export function fillPolygon(
  pixels: Uint8Array,
  width: number,
  height: number,
  points: readonly Point[],
  value: number,
): Bounds {
  let minY = Infinity,
    maxY = -Infinity,
    minX = Infinity,
    maxX = -Infinity;
  for (const p of points) {
    minY = Math.min(minY, p.y);
    maxY = Math.max(maxY, p.y);
    minX = Math.min(minX, p.x);
    maxX = Math.max(maxX, p.x);
  }
  const top = Math.max(0, Math.floor(minY)),
    bottom = Math.min(height, Math.ceil(maxY)),
    left = Math.max(0, Math.floor(minX)),
    right = Math.min(width, Math.ceil(maxX));
  const xs: number[] = [];
  for (let y = top; y < bottom; y++) {
    const py = y + 0.5;
    xs.length = 0;
    for (let i = 0, j = points.length - 1; i < points.length; j = i++) {
      const a = points[i]!,
        b = points[j]!;
      if (a.y > py !== b.y > py)
        xs.push(a.x + ((py - a.y) * (b.x - a.x)) / (b.y - a.y));
    }
    xs.sort((m, n) => m - n);
    const row = y * width;
    for (let k = 0; k + 1 < xs.length; k += 2) {
      // Pixel x is inside when its center x+0.5 lies in [xs[k], xs[k+1]).
      const start = Math.max(left, Math.ceil(xs[k]! - 0.5)),
        end = Math.min(right, Math.ceil(xs[k + 1]! - 0.5));
      if (start >= end) continue;
      pixels.fill(value, row + start, row + end);
    }
  }
  return {
    x: left,
    y: top,
    width: Math.max(0, right - left),
    height: Math.max(0, bottom - top),
  };
}
export function encodeRuns(pixels: Uint8Array): number[] {
  const runs: number[] = [];
  let value = -1,
    count = 0;
  for (const p of pixels) {
    if (p === value) {
      count++;
    } else {
      if (count) runs.push(value, count);
      value = p;
      count = 1;
    }
  }
  if (count) runs.push(value, count);
  return runs;
}
export function decodeRuns(mask: RasterMask): Uint8Array {
  const length = mask.width * mask.height;
  if (
    !Number.isSafeInteger(length) ||
    length <= 0 ||
    length > 24000000 ||
    mask.runs.length % 2
  )
    throw Error('Invalid raster dimensions');
  const result = new Uint8Array(length);
  let offset = 0;
  for (let i = 0; i < mask.runs.length; i += 2) {
    const value = mask.runs[i]!,
      count = mask.runs[i + 1]!;
    if (
      !Number.isInteger(value) ||
      value < 0 ||
      value > 255 ||
      !Number.isInteger(count) ||
      count <= 0 ||
      offset + count > length
    )
      throw Error('Invalid raster run');
    result.fill(value, offset, offset + count);
    offset += count;
  }
  if (offset !== length) throw Error('Incomplete raster');
  return result;
}
