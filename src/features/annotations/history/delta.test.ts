import { describe, expect, it } from 'vitest';
import { applyHistory, historyBytes, historyDelta } from './delta';
import { decodeDocument, encodeDocument, type EditDocument } from './session';
import { rasterChanges } from '../geometry/changes';
import { librarySummary } from './summary';

const document = (): EditDocument => ({
  mediaId: 'image',
  revision: 5,
  reviewStatus: 'in_progress',
  eventLabel: 'uncertain',
  notes: 'before',
  splitGroup: 'trip',
  annotations: [],
  masks: [],
});
const mask = (width: number, height: number, seed = 0) => ({
  layerId: 'semantic',
  width,
  height,
  pixels: Uint8Array.from(
    { length: width * height },
    (_, i) => (i * 31 + seed) % 256,
  ),
});
const withoutRevision = (doc: EditDocument) => ({ ...doc, revision: 0 });
describe('reversible tiled raster history', () => {
  it('round trips mixed bytes, odd edge tiles, metadata and multiple layers without mutating inputs', () => {
    for (const [w, h] of [
      [1, 1],
      [63, 65],
      [137, 129],
    ]) {
      const before = {
        ...document(),
        masks: [mask(w!, h!), { ...mask(w!, h!, 7), layerId: 'region' }],
      };
      const pixels = before.masks[0]!.pixels.slice();
      for (let i = 0; i < pixels.length; i += 17) pixels[i] = pixels[i]! ^ 255;
      const after = {
        ...before,
        notes: 'after',
        eventLabel: null,
        masks: [{ ...before.masks[0]!, pixels }, before.masks[1]!],
      };
      const delta = historyDelta(before, after);
      const undo = applyHistory({ ...after, revision: 99 }, delta, 'before');
      expect(withoutRevision(undo)).toEqual(withoutRevision(before));
      expect(undo.revision).toBe(99);
      expect(withoutRevision(applyHistory(undo, delta, 'after'))).toEqual(
        withoutRevision(after),
      );
      expect(before.masks[0]!.pixels).toEqual(mask(w!, h!).pixels);
      expect(undo.masks[1]!.pixels).toBe(before.masks[1]!.pixels);
    }
  });
  it('restores added, deleted, empty and resized masks independently of layer order', () => {
    const a = {
      ...document(),
      masks: [mask(80, 70), { ...mask(4, 3), layerId: 'deleted' }],
    };
    const b = {
      ...document(),
      masks: [
        { ...mask(20, 30), layerId: 'added' },
        mask(3, 5),
        { ...mask(2, 2), layerId: 'empty', pixels: new Uint8Array(4) },
      ],
    };
    const delta = historyDelta(a, b);
    const undo = applyHistory(b, delta, 'before');
    expect(undo).toEqual(a);
    expect(applyHistory(undo, delta, 'after')).toEqual(b);
  });
  it('keeps a single pixel edit to one tile and consumes trusted hints without retaining source chains', () => {
    const before = { ...document(), masks: [mask(1920, 1080)] };
    const pixels = before.masks[0]!.pixels.slice();
    pixels[100 * 1920 + 100] = 13;
    rasterChanges.set(pixels, {
      source: before.masks[0]!.pixels,
      bounds: { x: 100, y: 100, width: 1, height: 1 },
    });
    const after = { ...before, masks: [{ ...before.masks[0]!, pixels }] };
    const delta = historyDelta(before, after);
    expect(historyBytes([delta])).toBe(64 * 64);
    expect(rasterChanges.has(pixels)).toBe(false);
    expect(applyHistory(after, delta, 'before')).toEqual(before);
    expect(delta.before).not.toHaveProperty('pixels');
  });
  it('ignores hints referring to a different source buffer', () => {
    const before = { ...document(), masks: [mask(130, 130)] };
    const pixels = before.masks[0]!.pixels.slice();
    pixels[0] = 200;
    pixels[129 * 130 + 129] = 5;
    rasterChanges.set(pixels, {
      source: new Uint8Array(130 * 130),
      bounds: { x: 0, y: 0, width: 1, height: 1 },
    });
    const after = { ...before, masks: [{ ...before.masks[0]!, pixels }] };
    expect(applyHistory(after, historyDelta(before, after), 'before')).toEqual(
      before,
    );
  });
  it('reuses RLE for immutable masks during note/vector edits; replacements get fresh exact runs', () => {
    const before = { ...document(), masks: [mask(65, 67)] };
    const first = encodeDocument(before, 0);
    const second = encodeDocument({ ...before, notes: 'another' }, 1);
    expect(second.masks[0]!.runs).toBe(first.masks[0]!.runs);
    const pixels = before.masks[0]!.pixels.slice();
    pixels[0] = 255;
    const edited = { ...before, masks: [{ ...before.masks[0]!, pixels }] };
    const third = encodeDocument(edited, 2);
    expect(third.masks[0]!.runs).not.toBe(first.masks[0]!.runs);
    expect(decodeDocument(third).masks).toEqual(edited.masks);
  });
  it('refreshes the library only when filter-relevant metadata changes', () => {
    const doc = document(),
      key = librarySummary(doc);
    expect(librarySummary({ ...doc, notes: 'different', revision: 6 })).toBe(
      key,
    );
    expect(librarySummary({ ...doc, masks: [mask(1, 1)] })).not.toBe(key);
    expect(librarySummary({ ...doc, reviewStatus: 'approved' })).not.toBe(key);
    expect(librarySummary({ ...doc, eventLabel: 'negative' })).not.toBe(key);
    expect(librarySummary({ ...doc, splitGroup: 'other' })).not.toBe(key);
  });
});
