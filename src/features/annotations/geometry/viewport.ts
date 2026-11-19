export interface Point {
  x: number;
  y: number;
}
export interface Size {
  width: number;
  height: number;
}
export interface Viewport {
  zoom: number;
  pan: Point;
}
export const MIN_ZOOM = 0.001;
export const MAX_ZOOM = 32;
export const imageToScreen = (p: Point, v: Viewport): Point => ({
  x: p.x * v.zoom + v.pan.x,
  y: p.y * v.zoom + v.pan.y,
});
export const screenToImage = (p: Point, v: Viewport): Point => ({
  x: (p.x - v.pan.x) / v.zoom,
  y: (p.y - v.pan.y) / v.zoom,
});
export function zoomAt(v: Viewport, pointer: Point, factor: number): Viewport {
  if (!Number.isFinite(factor) || factor <= 0) return v;
  const zoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, v.zoom * factor));
  const anchor = screenToImage(pointer, v);
  return {
    zoom,
    pan: { x: pointer.x - anchor.x * zoom, y: pointer.y - anchor.y * zoom },
  };
}
export function fitImage(image: Size, container: Size): Viewport {
  if (
    [image.width, image.height, container.width, container.height].some(
      (n) => !Number.isFinite(n) || n <= 0,
    )
  )
    return { zoom: 1, pan: { x: 0, y: 0 } };
  const zoom = Math.min(
    MAX_ZOOM,
    Math.max(
      MIN_ZOOM,
      Math.min(container.width / image.width, container.height / image.height) *
        0.9,
    ),
  );
  return {
    zoom,
    pan: {
      x: (container.width - image.width * zoom) / 2,
      y: (container.height - image.height * zoom) / 2,
    },
  };
}
export function pointerToScreen(
  client: Point,
  rect: { left: number; top: number; width: number; height: number },
  css: Size,
): Point {
  return {
    x: ((client.x - rect.left) * css.width) / rect.width,
    y: ((client.y - rect.top) * css.height) / rect.height,
  };
}
export function backingSize(css: Size, dpr: number): Size {
  return {
    width: Math.round(css.width * dpr),
    height: Math.round(css.height * dpr),
  };
}
