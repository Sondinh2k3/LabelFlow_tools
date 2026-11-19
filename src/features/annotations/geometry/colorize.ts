import type { Bounds } from './raster';

/** Native-endian lookup entries write exactly the same RGBA bytes on every platform. */
export function colorLut(
  entries: readonly { id: number; color: string }[],
): Uint32Array {
  const bytes = new Uint8ClampedArray(256 * 4);
  for (const { id, color } of entries) {
    if (
      !Number.isInteger(id) ||
      id < 0 ||
      id > 255 ||
      !/^#[\da-f]{6}$/i.test(color)
    )
      throw Error('Invalid raster palette');
    const at = id * 4;
    bytes[at] = parseInt(color.slice(1, 3), 16);
    bytes[at + 1] = parseInt(color.slice(3, 5), 16);
    bytes[at + 2] = parseInt(color.slice(5, 7), 16);
    bytes[at + 3] = 255;
  }
  return new Uint32Array(bytes.buffer);
}
export function colorize(
  pixels: Uint8Array,
  width: number,
  height: number,
  rgba: Uint8ClampedArray,
  table: Uint32Array,
  bounds?: Bounds,
): void {
  const output = new Uint32Array(
    rgba.buffer,
    rgba.byteOffset,
    rgba.byteLength / 4,
  );
  const b = bounds ?? { x: 0, y: 0, width, height };
  for (let y = b.y; y < b.y + b.height; y++) {
    const end = y * width + b.x + b.width;
    for (let at = y * width + b.x; at < end; at++)
      output[at] = table[pixels[at]!]!;
  }
}
