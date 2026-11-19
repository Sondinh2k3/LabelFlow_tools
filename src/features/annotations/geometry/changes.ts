import type { Bounds } from './raster';

/** Optional trusted hints from the brush, tied to its immutable source buffer.
 * Imported/replaced masks have no hint and always receive a full comparison. */
export const rasterChanges = new WeakMap<
  Uint8Array,
  { source: Uint8Array; bounds: Bounds }
>();
