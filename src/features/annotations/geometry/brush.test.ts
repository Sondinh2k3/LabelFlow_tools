import { expect, test } from 'vitest';
import { paintSegment } from './raster';
import { m1PaintSegment } from '../../../../tests/fixtures/m1Brush';
test('scanline brush exactly matches M1 capsules for subpixel, clipped, reversed and degenerate strokes', () => {
  let seed = 1234567;
  const random = () => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    return seed / 4294967296;
  };
  for (let i = 0; i < 800; i++) {
    const w = 67,
      h = 49,
      a = { x: random() * 90 - 10, y: random() * 65 - 10 },
      b = i % 7 ? { x: random() * 90 - 10, y: random() * 65 - 10 } : a,
      radius = i % 13 ? random() * 20 : 0,
      value = i % 2 ? 255 : 1;
    const baseline = new Uint8Array(w * h).fill(7),
      actual = baseline.slice();
    const bounds = m1PaintSegment(baseline, w, h, a, b, radius, value);
    expect(paintSegment(actual, w, h, a, b, radius, value)).toEqual(bounds);
    expect(actual).toEqual(baseline);
  }
});
test('pixel-center tangents, diagonal edges and horizontal/vertical bodies retain exact footprints', () => {
  for (const a of [
    { x: 0, y: 0 },
    { x: 0.5, y: 0.5 },
    { x: 8, y: 8 },
  ])
    for (const b of [{ x: 31, y: 31 }, { x: 0, y: 31 }, a, { x: 31, y: a.y }])
      for (const radius of [0, 0.5, 1, Math.SQRT2, 8, 64]) {
        const baseline = new Uint8Array(32 * 32),
          actual = baseline.slice();
        m1PaintSegment(baseline, 32, 32, a, b, radius, 1);
        paintSegment(actual, 32, 32, a, b, radius, 1);
        expect(actual).toEqual(baseline);
      }
});
