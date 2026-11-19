import { describe, expect, it } from 'vitest';
import {
  backingSize,
  fitImage,
  imageToScreen,
  MAX_ZOOM,
  MIN_ZOOM,
  pointerToScreen,
  screenToImage,
  zoomAt,
} from './viewport';

describe('canonical image / CSS viewport mapping', () => {
  for (const zoom of [0.001, 0.125, 0.5, 1, 1.75, 8, 32]) {
    for (const point of [
      { x: 0, y: 0 },
      { x: 1919.5, y: 1079.5 },
      { x: 123.456, y: 987.654 },
      { x: -12, y: 2000 },
    ]) {
      it(`round trips ${JSON.stringify(point)} at ${zoom}`, () => {
        const view = { zoom, pan: { x: -317.125, y: 84.75 } };
        const restored = screenToImage(imageToScreen(point, view), view);
        expect(restored.x).toBeCloseTo(point.x, 8);
        expect(restored.y).toBeCloseTo(point.y, 8);
      });
    }
  }
  it('fits 1080p with centered margins', () => {
    const view = fitImage(
      { width: 1920, height: 1080 },
      { width: 960, height: 600 },
    );
    expect(view.zoom).toBe(0.45);
    expect(imageToScreen({ x: 960, y: 540 }, view)).toEqual({ x: 480, y: 300 });
  });
  it('fits portrait images and guards zero-size containers', () => {
    expect(
      fitImage({ width: 1080, height: 1920 }, { width: 900, height: 600 }).zoom,
    ).toBe(0.28125);
    expect(
      fitImage({ width: 0, height: 1 }, { width: 1, height: 1 }).zoom,
    ).toBe(1);
  });
  it('keeps the image pixel under the zoom pointer fixed', () => {
    const view = { zoom: 0.47, pan: { x: -58, y: 33 } };
    const pointer = { x: 299.2, y: 411.3 };
    const pixel = screenToImage(pointer, view);
    const changed = zoomAt(view, pointer, 2.3);
    expect(imageToScreen(pixel, changed).x).toBeCloseTo(pointer.x, 10);
    expect(imageToScreen(pixel, changed).y).toBeCloseTo(pointer.y, 10);
  });
  it('has no significant drift after 1000 reciprocal zoom cycles', () => {
    let view = { zoom: 1, pan: { x: -35.25, y: 89.5 } };
    for (let i = 0; i < 1000; i++) {
      view = zoomAt(
        zoomAt(view, { x: 333, y: 222 }, 1.1),
        { x: 333, y: 222 },
        1 / 1.1,
      );
    }
    expect(view.zoom).toBeCloseTo(1, 10);
    expect(view.pan.x).toBeCloseTo(-35.25, 8);
    expect(view.pan.y).toBeCloseTo(89.5, 8);
  });
  it('clamps zoom without moving the anchor and ignores invalid factors', () => {
    const initial = { zoom: 1, pan: { x: 0, y: 0 } };
    expect(zoomAt(initial, { x: 0, y: 0 }, 1e8).zoom).toBe(MAX_ZOOM);
    expect(zoomAt(initial, { x: 0, y: 0 }, 1e-8).zoom).toBe(MIN_ZOOM);
    for (const factor of [0, -1, NaN, Infinity])
      expect(zoomAt(initial, { x: 1, y: 2 }, factor)).toBe(initial);
  });
  for (const dpr of [1, 1.25, 1.5, 2, 3]) {
    it(`keeps pointer coordinates in CSS pixels at DPR ${dpr}`, () => {
      const css = { width: 800, height: 600 };
      expect(backingSize(css, dpr)).toEqual({
        width: 800 * dpr,
        height: 600 * dpr,
      });
      const pointer = pointerToScreen(
        { x: 450, y: 350 },
        { left: 50, top: 50, width: 800, height: 600 },
        css,
      );
      expect(pointer).toEqual({ x: 400, y: 300 });
      expect(
        screenToImage(pointer, { zoom: 0.5, pan: { x: 0, y: 30 } }),
      ).toEqual({ x: 800, y: 540 });
    });
  }
  it('accounts for CSS scaling independently from backing pixels', () => {
    expect(
      pointerToScreen(
        { x: 225, y: 170 },
        { left: 25, top: 20, width: 400, height: 300 },
        { width: 800, height: 600 },
      ),
    ).toEqual({ x: 400, y: 300 });
  });
});
