import { describe, it, expect } from 'vitest';
import { colorLut, colorize } from './colorize';
describe('raster colorization', () => {
  it('preserves exact RGBA bytes and transparent ignore across all byte values', () => {
    const colors = Array.from({ length: 255 }, (_, id) => ({
      id,
      color: `#${id.toString(16).padStart(2, '0')}1234`,
    }));
    const table = colorLut(colors),
      pixels = Uint8Array.from({ length: 256 }, (_, i) => i),
      data = new Uint8ClampedArray(1024);
    colorize(pixels, 16, 16, data, table);
    for (let i = 0; i < 255; i++)
      expect([...data.slice(i * 4, i * 4 + 4)]).toEqual([i, 18, 52, 255]);
    expect([...data.slice(255 * 4)]).toEqual([0, 0, 0, 0]);
  });
  it('updates only dirty bounds and keeps all other bytes', () => {
    const data = new Uint8ClampedArray(4 * 4 * 4).fill(7);
    colorize(
      new Uint8Array(16).fill(1),
      4,
      4,
      data,
      colorLut([{ id: 1, color: '#a2e8bd' }]),
      { x: 1, y: 2, width: 2, height: 1 },
    );
    for (let i = 0; i < 16; i++)
      expect([...data.slice(i * 4, i * 4 + 4)]).toEqual(
        i === 9 || i === 10 ? [162, 232, 189, 255] : [7, 7, 7, 7],
      );
  });
});
