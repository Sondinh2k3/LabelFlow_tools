import { expect, test } from 'vitest';
import { fillPolygon } from './raster';
import { pointInRing } from './shapes';
import type { Point } from '../../../lib/contracts';

test('polygon fill matches a per-pixel-center point-in-polygon reference', () => {
  let seed = 98765;
  const random = () => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    return seed / 4294967296;
  };
  const w = 61,
    h = 47;
  for (let i = 0; i < 400; i++) {
    const n = 3 + Math.floor(random() * 6),
      cx = random() * w,
      cy = random() * h;
    // Star-shaped around a center keeps polygons simple; some vertices fall outside.
    const points: Point[] = Array.from({ length: n }, (_, k) => {
      const angle = (k / n) * Math.PI * 2,
        r = 3 + random() * 30;
      return { x: cx + Math.cos(angle) * r, y: cy + Math.sin(angle) * r };
    });
    const actual = new Uint8Array(w * h).fill(255);
    fillPolygon(actual, w, h, points, 3);
    for (let y = 0; y < h; y++)
      for (let x = 0; x < w; x++) {
        const inside = pointInRing({ x: x + 0.5, y: y + 0.5 }, points);
        const p = actual[y * w + x];
        // Pixel centers exactly on an edge may go either way.
        if (inside) expect(p).toBe(3);
        else if (p === 3) {
          const near = points.some((a, j) => {
            const b = points[(j + 1) % n]!;
            const dx = b.x - a.x,
              dy = b.y - a.y,
              t = Math.max(
                0,
                Math.min(
                  1,
                  ((x + 0.5 - a.x) * dx + (y + 0.5 - a.y) * dy) /
                    (dx * dx + dy * dy),
                ),
              );
            return (
              Math.hypot(a.x + t * dx - x - 0.5, a.y + t * dy - y - 0.5) < 1e-6
            );
          });
          expect(near).toBe(true);
        }
      }
  }
});

test('polygon fill overwrites earlier classes and reports clipped bounds', () => {
  const pixels = new Uint8Array(10 * 10).fill(1);
  const bounds = fillPolygon(
    pixels,
    10,
    10,
    [
      { x: -5, y: 2 },
      { x: 4, y: 2 },
      { x: 4, y: 20 },
      { x: -5, y: 20 },
    ],
    7,
  );
  expect(bounds).toEqual({ x: 0, y: 2, width: 4, height: 8 });
  expect(pixels[2 * 10 + 0]).toBe(7);
  expect(pixels[2 * 10 + 3]).toBe(7);
  expect(pixels[2 * 10 + 4]).toBe(1);
  expect(pixels[1 * 10 + 0]).toBe(1);
});
