import type {
  Annotation,
  Geometry,
  Point,
  Profile,
} from '../../../lib/contracts';
import type { EditDocument, Pixels } from '../history/session';
import type { Bounds } from '../geometry/raster';
import { colorLut, colorize } from '../geometry/colorize';

interface Cache {
  source: Uint8Array;
  profile: Profile;
  canvas: HTMLCanvasElement;
  context: CanvasRenderingContext2D;
  data: ImageData;
}
export class SceneRenderer {
  private instances = new Map<
    Geometry,
    { canvas: HTMLCanvasElement; color: string }
  >();
  private cache = new Map<string, Cache>();
  private palettes = new WeakMap<Profile, Map<string, Uint32Array>>();
  clear() {
    for (const item of this.cache.values()) this.release(item.canvas);
    for (const item of this.instances.values()) this.release(item.canvas);
    this.cache.clear();
    this.instances.clear();
    this.palettes = new WeakMap();
  }
  private release(canvas: HTMLCanvasElement) {
    // Drop native/graphics backing storage when its owner goes away, rather than
    // waiting for JS collection and a separate GPU resource collection cycle.
    canvas.width = 0;
    canvas.height = 0;
  }
  private colorTable(layerId: string, profile: Profile): Uint32Array {
    let palettes = this.palettes.get(profile);
    if (!palettes) {
      palettes = new Map();
      this.palettes.set(profile, palettes);
    }
    const cached = palettes.get(layerId);
    if (cached) return cached;
    const layer = profile.layers.find((l) => l.id === layerId)!;
    const entries =
      layer.layerType === 'semantic'
        ? profile.classes
            .filter((c) => c.namespace === layer.namespace && !c.isIgnore)
            .map((c) => ({ id: c.exportId, color: c.color }))
        : [{ id: 1, color: layer.color }];
    const table = colorLut(entries);
    palettes.set(layerId, table);
    return table;
  }
  dirty(mask: Pixels, profile: Profile, bounds?: Bounds, base?: Uint8Array) {
    let item = this.cache.get(mask.layerId);
    if (
      !item ||
      item.canvas.width !== mask.width ||
      item.canvas.height !== mask.height
    ) {
      if (item) this.release(item.canvas);
      const canvas = document.createElement('canvas');
      canvas.width = mask.width;
      canvas.height = mask.height;
      const context = canvas.getContext('2d')!;
      item = {
        source: mask.pixels,
        profile,
        canvas,
        context,
        data: context.createImageData(mask.width, mask.height),
      };
      this.cache.set(mask.layerId, item);
      bounds = undefined;
    } else if (item.source !== mask.pixels) {
      if (item.source !== base) bounds = undefined;
      item.source = mask.pixels;
    }
    // Class colors are editable: a new profile recolors the whole overlay once.
    if (item.profile !== profile) {
      bounds = undefined;
      item.profile = profile;
    }
    const b = bounds ?? { x: 0, y: 0, width: mask.width, height: mask.height };
    const table = this.colorTable(mask.layerId, profile);
    colorize(mask.pixels, mask.width, mask.height, item.data.data, table, b);
    item.context.putImageData(item.data, 0, 0, b.x, b.y, b.width, b.height);
  }
  draw(
    ctx: CanvasRenderingContext2D,
    doc: EditDocument,
    profile: Profile,
    zoom: number,
    visible: Set<string>,
    opacity: number,
    selected: string | null,
    preview: Geometry | null,
    polygon: Point[],
    cursor: Point | null,
    radius: number,
    brush: boolean,
    stroke: Pixels | null,
    transform: Annotation | null,
  ) {
    const liveInstances = new Set<Geometry>();
    for (const stored of doc.annotations)
      if (visible.has(stored.layerId))
        liveInstances.add(
          transform?.id === stored.id ? transform.geometry : stored.geometry,
        );
    for (const [geometry, item] of this.instances)
      if (!liveInstances.has(geometry)) {
        this.release(item.canvas);
        this.instances.delete(geometry);
      }
    const liveRasters = new Set(
      doc.masks.filter((m) => visible.has(m.layerId)).map((m) => m.layerId),
    );
    if (stroke && visible.has(stroke.layerId)) liveRasters.add(stroke.layerId);
    for (const [layer, item] of this.cache)
      if (!liveRasters.has(layer)) {
        this.release(item.canvas);
        this.cache.delete(layer);
      }
    ctx.save();
    ctx.globalAlpha = opacity;
    ctx.imageSmoothingEnabled = false;
    for (const layer of profile.layers) {
      if (!visible.has(layer.id)) continue;
      const mask =
        stroke?.layerId === layer.id
          ? stroke
          : doc.masks.find((m) => m.layerId === layer.id);
      if (!mask) continue;
      const cached = this.cache.get(layer.id);
      if (cached?.source !== mask.pixels || cached?.profile !== profile)
        this.dirty(mask, profile);
      ctx.drawImage(this.cache.get(layer.id)!.canvas, 0, 0);
    }
    ctx.globalAlpha = 1;
    for (const stored of doc.annotations) {
      if (!visible.has(stored.layerId)) continue;
      const shape = transform?.id === stored.id ? transform : stored;
      const color =
        profile.classes.find((c) => c.id === shape.classId)?.color ??
        profile.layers.find((l) => l.id === shape.layerId)?.color ??
        '#ffffff';
      this.shape(ctx, shape.geometry, color, zoom, shape.id === selected);
    }
    if (preview) {
      ctx.setLineDash([5 / zoom, 4 / zoom]);
      this.shape(ctx, preview, '#ffffff', zoom, false);
      ctx.setLineDash([]);
    }
    if (polygon.length) {
      ctx.beginPath();
      polygon.forEach((p, i) =>
        i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y),
      );
      if (cursor) ctx.lineTo(cursor.x, cursor.y);
      ctx.strokeStyle = '#ffffff';
      ctx.lineWidth = 1.5 / zoom;
      ctx.stroke();
      for (const p of polygon) {
        ctx.fillStyle = '#a2e8bd';
        ctx.fillRect(p.x - 3 / zoom, p.y - 3 / zoom, 6 / zoom, 6 / zoom);
      }
    }
    if (cursor && brush) {
      ctx.beginPath();
      ctx.arc(cursor.x, cursor.y, radius, 0, Math.PI * 2);
      ctx.strokeStyle = '#000000';
      ctx.lineWidth = 3 / zoom;
      ctx.stroke();
      ctx.strokeStyle = '#ffffff';
      ctx.lineWidth = 1 / zoom;
      ctx.stroke();
    }
    ctx.restore();
  }
  private shape(
    ctx: CanvasRenderingContext2D,
    g: Geometry,
    color: string,
    zoom: number,
    selected: boolean,
  ) {
    if (g.type === 'mask') {
      let cached = this.instances.get(g);
      if (!cached || cached.color !== color) {
        const canvas = document.createElement('canvas');
        canvas.width = g.width;
        canvas.height = g.height;
        const context = canvas.getContext('2d')!;
        const data = context.createImageData(g.width, g.height);
        const rgb = [1, 3, 5].map((i) => parseInt(color.slice(i, i + 2), 16));
        let pixel = 0;
        for (let i = 0; i < g.runs.length; i += 2) {
          const end = pixel + g.runs[i + 1]!;
          if (g.runs[i] === 1)
            for (let p = pixel; p < end; p++) {
              const at = p * 4;
              data.data[at] = rgb[0]!;
              data.data[at + 1] = rgb[1]!;
              data.data[at + 2] = rgb[2]!;
              data.data[at + 3] = 255;
            }
          pixel = end;
        }
        context.putImageData(data, 0, 0);
        cached = { canvas, color };
        const bytes = [...this.instances.values()].reduce(
          (n, item) => n + item.canvas.width * item.canvas.height * 4,
          0,
        );
        if (bytes + g.width * g.height * 4 > 64 * 1024 * 1024) {
          for (const item of this.instances.values()) this.release(item.canvas);
          this.instances.clear();
        }
        if (g.width * g.height * 4 <= 64 * 1024 * 1024)
          this.instances.set(g, cached);
      }
      ctx.save();
      ctx.globalAlpha = selected ? 0.7 : 0.4;
      ctx.drawImage(cached.canvas, 0, 0);
      ctx.restore();
      return;
    }
    ctx.beginPath();
    if (g.type === 'bbox') ctx.rect(g.x, g.y, g.width, g.height);
    else {
      const polygons = g.type === 'multiPolygon' ? g.polygons : [g];
      for (const ring of polygons.flatMap((poly) => [
        poly.points,
        ...poly.holes,
      ])) {
        ring.forEach((p, i) =>
          i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y),
        );
        ctx.closePath();
      }
    }
    ctx.strokeStyle = '#000000';
    ctx.lineWidth = (selected ? 4 : 3) / zoom;
    ctx.stroke();
    ctx.strokeStyle = color;
    ctx.lineWidth = (selected ? 2 : 1.5) / zoom;
    ctx.stroke();
    if (selected) {
      const handles =
        g.type === 'bbox'
          ? [
              { x: g.x, y: g.y },
              { x: g.x + g.width, y: g.y },
              { x: g.x + g.width, y: g.y + g.height },
              { x: g.x, y: g.y + g.height },
            ]
          : g.type === 'polygon'
            ? g.points
            : [];
      for (const p of handles) {
        ctx.fillStyle = color;
        ctx.fillRect(p.x - 3 / zoom, p.y - 3 / zoom, 6 / zoom, 6 / zoom);
        ctx.strokeStyle = '#111111';
        ctx.lineWidth = 1 / zoom;
        ctx.strokeRect(p.x - 3 / zoom, p.y - 3 / zoom, 6 / zoom, 6 / zoom);
      }
    }
  }
}
