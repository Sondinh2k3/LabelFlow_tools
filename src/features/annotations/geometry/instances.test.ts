import { describe, expect, it } from 'vitest';
import type { Geometry } from '../../../lib/contracts';
import { hitShape, moveShape, validPolygonRings } from './shapes';
import { decodeDocument, encodeDocument } from '../history/session';
const poly = {
  points: [
    { x: 1, y: 1 },
    { x: 5, y: 1 },
    { x: 5, y: 5 },
    { x: 1, y: 5 },
  ],
  holes: [
    [
      { x: 2, y: 2 },
      { x: 4, y: 2 },
      { x: 4, y: 4 },
      { x: 2, y: 4 },
    ],
  ],
};
describe('M2 geometry preserves instance semantics', () => {
  it.each([
    { x: 1.5, y: 1.5, hit: true },
    { x: 3, y: 3, hit: false },
    { x: 8, y: 3, hit: true },
    { x: 6, y: 3, hit: false },
  ])('disconnected union and holes at $x,$y', ({ x, y, hit }) => {
    const g: Geometry = {
      type: 'multiPolygon',
      polygons: [
        poly,
        {
          points: [
            { x: 7, y: 1 },
            { x: 9, y: 1 },
            { x: 9, y: 5 },
            { x: 7, y: 5 },
          ],
          holes: [],
        },
      ],
    };
    expect(hitShape({ x, y }, g)).toBe(hit);
  });
  it('moves all components together and clips translation at the image boundary', () => {
    const g: Geometry = { type: 'multiPolygon', polygons: [poly] };
    expect(moveShape(g, 100, 100, 10, 10)).toEqual({
      type: 'multiPolygon',
      polygons: [
        {
          points: poly.points.map((p) => ({ x: p.x + 5, y: p.y + 5 })),
          holes: poly.holes.map((h) =>
            h.map((p) => ({ x: p.x + 5, y: p.y + 5 })),
          ),
        },
      ],
    });
    expect(g.polygons[0]!.points[0]).toEqual({ x: 1, y: 1 });
  });
  const mask: Geometry = {
    type: 'mask',
    width: 4,
    height: 3,
    runs: [0, 1, 1, 2, 0, 5, 1, 1, 0, 3],
    iscrowd: true,
  };
  it.each([
    { x: 0, y: 0, hit: false },
    { x: 1, y: 0, hit: true },
    { x: 0, y: 2, hit: true },
    { x: 4, y: 2, hit: false },
    { x: -1, y: 0, hit: false },
  ])('binary RLE hit test at $x,$y', ({ x, y, hit }) =>
    expect(hitShape({ x, y }, mask)).toBe(hit),
  );
  it('keeps imported pixel geometry fixed during vector move', () =>
    expect(moveShape(mask, 1, 1, 4, 3)).toBe(mask));
  it('retains iscrowd, disconnected rings and exact mask runs across history serialization', () => {
    const doc = {
      mediaId: 'frame',
      revision: 1,
      reviewStatus: 'needs_review' as const,
      eventLabel: 'uncertain' as const,
      notes: 'review',
      splitGroup: 'video:one',
      annotations: [
        {
          id: 'object',
          layerId: 'candidate_objects',
          classId: 'candidate-0',
          geometry: mask,
          source: 'imported' as const,
        },
      ],
      masks: [],
    };
    expect(encodeDocument(decodeDocument(doc), 1)).toEqual(doc);
  });
});

it('rejects vertex edits that strand or intersect a polygon hole', () => {
  expect(validPolygonRings(poly.points, poly.holes)).toBe(true);
  expect(
    validPolygonRings(poly.points, [
      poly.holes[0]!.map((p) => ({ ...p, x: p.x + 4 })),
    ]),
  ).toBe(false);
  expect(validPolygonRings(poly.points, [poly.holes[0]!, poly.holes[0]!])).toBe(
    false,
  );
});
