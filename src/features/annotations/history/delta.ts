import type { EditDocument, Pixels } from './session';
import { rasterChanges } from '../geometry/changes';

type Header = Omit<Pixels, 'pixels'>;
interface Frame {
  state: Omit<EditDocument, 'masks'>;
  masks: Header[];
}
interface Tile {
  layerId: string;
  x: number;
  y: number;
  width: number;
  height: number;
  xor: Uint8Array;
}
interface Replacement {
  layerId: string;
  before?: Uint8Array;
  after?: Uint8Array;
}
export interface HistoryDelta {
  before: Frame;
  after: Frame;
  tiles: Tile[];
  replacements: Replacement[];
}
const frame = ({ masks, ...state }: EditDocument): Frame => ({
  state,
  masks: masks.map(({ layerId, width, height }) => ({
    layerId,
    width,
    height,
  })),
});
export function historyDelta(
  before: EditDocument,
  after: EditDocument,
): HistoryDelta {
  if (before.mediaId !== after.mediaId)
    throw Error('Cannot mix media in an edit session');
  const result: HistoryDelta = {
    before: frame(before),
    after: frame(after),
    tiles: [],
    replacements: [],
  };
  const previous = new Map(before.masks.map((m) => [m.layerId, m])),
    next = new Map(after.masks.map((m) => [m.layerId, m]));
  for (const layerId of new Set([...previous.keys(), ...next.keys()])) {
    const a = previous.get(layerId),
      b = next.get(layerId);
    if (a?.pixels === b?.pixels) continue;
    if (a && b && (a.width !== b.width || a.height !== b.height)) {
      result.replacements.push({
        layerId,
        before: a.pixels.slice(),
        after: b.pixels.slice(),
      });
      continue;
    }
    const mask = b ?? a!;
    const length = mask.width * mask.height;
    const old = a?.pixels ?? new Uint8Array(length),
      updated = b?.pixels ?? new Uint8Array(length);
    const hint = b && rasterChanges.get(b.pixels);
    // Consume once: retaining a strong source here would chain whole prior rasters.
    if (b) rasterChanges.delete(b.pixels);
    const bounds =
      hint && hint.source === a?.pixels
        ? hint.bounds
        : { x: 0, y: 0, width: mask.width, height: mask.height };
    for (
      let top = Math.floor(bounds.y / 64) * 64;
      top < Math.min(mask.height, bounds.y + bounds.height);
      top += 64
    )
      for (
        let left = Math.floor(bounds.x / 64) * 64;
        left < Math.min(mask.width, bounds.x + bounds.width);
        left += 64
      ) {
        const width = Math.min(64, mask.width - left),
          height = Math.min(64, mask.height - top);
        let xor: Uint8Array | null = null;
        for (let y = 0; y < height; y++) {
          const row = (top + y) * mask.width + left;
          for (let x = 0; x < width; x++) {
            const value = old[row + x]! ^ updated[row + x]!;
            if (value) {
              xor ??= new Uint8Array(width * height);
              xor[y * width + x] = value;
            }
          }
        }
        if (xor)
          result.tiles.push({ layerId, x: left, y: top, width, height, xor });
      }
  }
  return result;
}
export function applyHistory(
  current: EditDocument,
  delta: HistoryDelta,
  direction: 'before' | 'after',
): EditDocument {
  const target = delta[direction];
  if (current.mediaId !== target.state.mediaId)
    throw Error('History belongs to another image');
  const masks = target.masks.map((header) => {
    const replaced = delta.replacements.find(
      (r) => r.layerId === header.layerId,
    )?.[direction];
    if (replaced) return { ...header, pixels: replaced.slice() };
    const old = current.masks.find((m) => m.layerId === header.layerId),
      tiles = delta.tiles.filter((t) => t.layerId === header.layerId);
    if (
      !tiles.length &&
      old &&
      old.width === header.width &&
      old.height === header.height
    )
      return { ...header, pixels: old.pixels };
    const pixels =
      old?.pixels.slice() ?? new Uint8Array(header.width * header.height);
    for (const tile of tiles)
      for (let y = 0; y < tile.height; y++) {
        const row = (tile.y + y) * header.width + tile.x;
        for (let x = 0; x < tile.width; x++)
          pixels[row + x] = pixels[row + x]! ^ tile.xor[y * tile.width + x]!;
      }
    return { ...header, pixels };
  });
  return { ...target.state, revision: current.revision, masks };
}
export function historyBytes(entries: HistoryDelta[]): number {
  let bytes = 0;
  const arrays = new Set<number[]>();
  for (const entry of entries) {
    for (const tile of entry.tiles) bytes += tile.xor.byteLength;
    for (const r of entry.replacements)
      bytes += (r.before?.byteLength ?? 0) + (r.after?.byteLength ?? 0);
    for (const state of [entry.before.state, entry.after.state])
      for (const a of state.annotations)
        if (a.geometry.type === 'mask' && !arrays.has(a.geometry.runs)) {
          arrays.add(a.geometry.runs);
          bytes += a.geometry.runs.length * 8;
        }
  }
  return bytes;
}
