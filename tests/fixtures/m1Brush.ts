// M1 capsule-distance paint kernel, unchanged through M4. Retained for exact-output
// regression and same-workload benchmarks; never imported into the production app.
import type { Point } from '../../src/lib/contracts';
export function m1PaintSegment(
  pixels: Uint8Array,
  width: number,
  height: number,
  a: Point,
  b: Point,
  radius: number,
  value: number,
) {
  const left = Math.max(0, Math.floor(Math.min(a.x, b.x) - radius)),
    top = Math.max(0, Math.floor(Math.min(a.y, b.y) - radius)),
    right = Math.min(width, Math.ceil(Math.max(a.x, b.x) + radius)),
    bottom = Math.min(height, Math.ceil(Math.max(a.y, b.y) + radius));
  const dx = b.x - a.x,
    dy = b.y - a.y,
    length = dx * dx + dy * dy;
  for (let y = top; y < bottom; y++)
    for (let x = left; x < right; x++) {
      const px = x + 0.5,
        py = y + 0.5,
        t = length
          ? Math.max(
              0,
              Math.min(1, ((px - a.x) * dx + (py - a.y) * dy) / length),
            )
          : 0;
      if (
        (px - a.x - t * dx) ** 2 + (py - a.y - t * dy) ** 2 <=
        radius * radius
      )
        pixels[y * width + x] = value;
    }
  return {
    x: left,
    y: top,
    width: Math.max(0, right - left),
    height: Math.max(0, bottom - top),
  };
}
