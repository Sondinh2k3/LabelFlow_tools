import { describe, it, expect } from 'vitest';
import {
  boxFromPoints,
  boxToYolo,
  yoloToBox,
  clampPoint,
  validPolygon,
  hitShape,
  moveShape,
  segmentsIntersect,
} from './shapes';
import { decodeRuns, encodeRuns, paintSegment } from './raster';
describe('canonical geometry and detection serialization', () => {
  for (let i = 1; i <= 120; i++)
    it(`1080p box round-trip ${i}`, () => {
      const box = boxFromPoints(
        { x: (i * 13.17) % 900, y: (i * 3.31) % 400 },
        { x: 1000 + ((i * 1.19) % 700), y: 500 + ((i * 2.23) % 550) },
      );
      const wire = boxToYolo(box, 1920, 1080).map((v) => Number(v.toFixed(10)));
      const restored = yoloToBox(wire, 1920, 1080);
      for (const key of ['x', 'y', 'width', 'height'] as const)
        expect(Math.abs(restored[key] - box[key])).toBeLessThan(0.000001);
    });
  it('normalizes all drag directions and clips borders', () => {
    for (const [a, b] of [
      [
        { x: 1, y: 2 },
        { x: 5, y: 8 },
      ],
      [
        { x: 5, y: 8 },
        { x: 1, y: 2 },
      ],
      [
        { x: 1, y: 8 },
        { x: 5, y: 2 },
      ],
      [
        { x: 5, y: 2 },
        { x: 1, y: 8 },
      ],
    ] as const)
      expect(boxFromPoints(a, b)).toEqual({
        type: 'bbox',
        x: 1,
        y: 2,
        width: 4,
        height: 6,
      });
    expect(clampPoint({ x: -10, y: 1100 }, 1920, 1080)).toEqual({
      x: 0,
      y: 1080,
    });
  });
  it('uses half-open box bounds', () => {
    const box = boxFromPoints({ x: 0, y: 0 }, { x: 10, y: 10 });
    expect(hitShape({ x: 0, y: 0 }, box)).toBe(true);
    expect(hitShape({ x: 10, y: 5 }, box)).toBe(false);
    expect(hitShape({ x: 5, y: 10 }, box)).toBe(false);
    expect(moveShape(box, 200, -50, 100, 100)).toEqual({ ...box, x: 90, y: 0 });
  });
  it('rejects degenerate, repeated, nonfinite and self-intersecting polygons', () => {
    for (const points of [
      [],
      [
        { x: 0, y: 0 },
        { x: 1, y: 1 },
      ],
      [
        { x: 0, y: 0 },
        { x: 1, y: 1 },
        { x: 2, y: 2 },
      ],
      [
        { x: 0, y: 0 },
        { x: 3, y: 3 },
        { x: 0, y: 3 },
        { x: 3, y: 0 },
      ],
      [
        { x: 0, y: 0 },
        { x: 3, y: 0 },
        { x: 3, y: 0 },
        { x: 0, y: 3 },
      ],
      [
        { x: NaN, y: 0 },
        { x: 3, y: 0 },
        { x: 0, y: 3 },
      ],
    ])
      expect(validPolygon(points)).toBe(false);
    expect(
      validPolygon([
        { x: 0, y: 0 },
        { x: 10, y: 0 },
        { x: 10, y: 10 },
        { x: 5, y: 5 },
        { x: 0, y: 10 },
      ]),
    ).toBe(true);
    expect(
      segmentsIntersect(
        { x: 0, y: 0 },
        { x: 10, y: 0 },
        { x: 5, y: 0 },
        { x: 20, y: 0 },
      ),
    ).toBe(true);
  });
  it('preserves holes through serialization, hit-testing and translation', () => {
    const polygon = {
      type: 'polygon' as const,
      points: [
        { x: 0, y: 0 },
        { x: 20, y: 0 },
        { x: 20, y: 20 },
        { x: 0, y: 20 },
      ],
      holes: [
        [
          { x: 5, y: 5 },
          { x: 15, y: 5 },
          { x: 15, y: 15 },
          { x: 5, y: 15 },
        ],
      ],
    };
    expect(hitShape({ x: 2, y: 2 }, polygon)).toBe(true);
    expect(hitShape({ x: 10, y: 10 }, polygon)).toBe(false);
    const moved = moveShape(polygon, 5, 4, 100, 100);
    expect(JSON.parse(JSON.stringify(moved))).toEqual(moved);
    expect(moved.type === 'polygon' && moved.holes[0]![0]).toEqual({
      x: 10,
      y: 9,
    });
  });
});
describe('lossless mutually exclusive rasters', () => {
  for (let seed = 0; seed < 20; seed++)
    it(`run encoding round-trip ${seed}`, () => {
      const data = Uint8Array.from({ length: 400 }, (_, i) =>
        (i * 17 + seed) % 20 === 19 ? 255 : (i + seed) % 19,
      );
      expect(
        decodeRuns({
          layerId: 'semantic',
          width: 20,
          height: 20,
          runs: encodeRuns(data),
        }),
      ).toEqual(data);
    });
  it('rejects malformed or incomplete rasters', () => {
    for (const runs of [
      [255],
      [256, 4],
      [0, 0],
      [0, 5],
      [0, 3],
      [-1, 4],
      [0, 1.5],
    ])
      expect(() =>
        decodeRuns({ layerId: 'x', width: 2, height: 2, runs }),
      ).toThrow();
  });
  it('uses pixel centers and a circular footprint without holes between sparse events', () => {
    const pixels = new Uint8Array(400).fill(255);
    paintSegment(pixels, 20, 20, { x: 2, y: 10 }, { x: 18, y: 10 }, 2, 13);
    for (let x = 2; x < 18; x++) expect(pixels[10 * 20 + x]).toBe(13);
    expect(pixels[0]).toBe(255);
    paintSegment(pixels, 20, 20, { x: 10, y: 10 }, { x: 10, y: 10 }, 1, 255);
    expect(pixels[10 * 20 + 10]).toBe(255);
  });
  it('clips brush at every border and does not mutate another layer', () => {
    const semantic = new Uint8Array(100).fill(255),
      region = new Uint8Array(100);
    for (const p of [
      { x: 0, y: 0 },
      { x: 10, y: 0 },
      { x: 0, y: 10 },
      { x: 10, y: 10 },
    ]) {
      const b = paintSegment(region, 10, 10, p, p, 3, 1);
      expect(b.x).toBeGreaterThanOrEqual(0);
      expect(b.y + b.height).toBeLessThanOrEqual(10);
    }
    expect([...semantic].every((p) => p === 255)).toBe(true);
    expect(region[0]).toBe(1);
    expect(region[99]).toBe(1);
    expect(region[55]).toBe(0);
  });
});
