import type {
  Annotation,
  Geometry,
  Point,
  Profile,
  SamPoint,
} from '../../../lib/contracts';
import type { EditDocument, Pixels } from '../history/session';
import {
  boxFromPoints,
  clampPoint,
  hitShape,
  moveShape,
  validPolygon,
  validPolygonRings,
} from '../geometry/shapes';
import { fillPolygon, paintSegment, type Bounds } from '../geometry/raster';
import { SceneRenderer } from '../canvas/renderer';
import { rasterChanges } from '../geometry/changes';
export type Tool =
  'select' | 'bbox' | 'polygon' | 'brush' | 'eraser' | 'pan' | 'sam';
export interface SamBox {
  x: number;
  y: number;
  width: number;
  height: number;
}
/** Current click-to-mask prompt; an empty prompt clears the preview. */
export interface SamPrompt {
  points: SamPoint[];
  box: SamBox | null;
}
export interface EditorConfig {
  document: EditDocument;
  profile: Profile;
  width: number;
  height: number;
  tool: Tool;
  layerId: string;
  classId: string | null;
  radius: number;
  visible: Set<string>;
  opacity: number;
  disabled: boolean;
  commit: (doc: EditDocument) => void;
  selection: (id: string | null) => void;
  error: (message: string) => void;
  /** Called whenever the SAM prompt changes (tool `sam`). */
  sam?: (prompt: SamPrompt) => void;
}
type Action =
  | { kind: 'bbox'; start: Point; end: Point }
  | { kind: 'samDrag'; start: Point; end: Point }
  | {
      kind: 'stroke';
      last: Point;
      mask: Pixels;
      dirty?: Bounds;
      changed?: Bounds;
    }
  | {
      kind: 'transform';
      start: Point;
      shape: Annotation;
      preview: Annotation;
      handle: number;
    };
export class EditorController {
  private config: EditorConfig | null = null;
  private action: Action | null = null;
  private polygon: Point[] = [];
  private cursor: Point | null = null;
  private selected: string | null = null;
  private renderer = new SceneRenderer();
  private listeners = new Set<() => void>();
  private zoom = 1;
  configure(config: EditorConfig) {
    if (
      this.config?.document.mediaId !== config.document.mediaId ||
      this.config?.layerId !== config.layerId ||
      this.config?.tool !== config.tool ||
      (config.disabled && this.hasDraft)
    ) {
      this.cancel();
    }
    this.config = config;
    if (
      this.selected &&
      !config.document.annotations.some(
        (a) => a.id === this.selected && a.layerId === config.layerId,
      )
    )
      this.select(null);
    this.invalidate();
  }
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private invalidate() {
    for (const l of this.listeners) l();
  }
  private select(id: string | null) {
    this.selected = id;
    this.config?.selection(id);
    this.invalidate();
  }
  selectId(id: string) {
    this.select(id);
  }
  get selectedId() {
    return this.selected;
  }
  private samPoints: SamPoint[] = [];
  private samBox: SamBox | null = null;
  private samMask: {
    pixels: Uint8Array;
    canvas: HTMLCanvasElement;
    bounds: Bounds;
  } | null = null;
  /** True while a SAM prompt or preview exists (Enter applies, Esc clears). */
  get hasSam() {
    return this.samPoints.length > 0 || !!this.samBox || !!this.samMask;
  }
  get samReady() {
    return !!this.samMask;
  }
  private emitSam() {
    this.config?.sam?.({ points: [...this.samPoints], box: this.samBox });
    this.invalidate();
  }
  private clearSam(notify: boolean) {
    const had = this.hasSam;
    this.samPoints = [];
    this.samBox = null;
    this.setSamMask(null);
    if (had && notify) this.config?.sam?.({ points: [], box: null });
  }
  /** Shows a SAM result (1 = object) as a translucent preview. */
  setSamMask(pixels: Uint8Array | null) {
    if (this.samMask) {
      this.samMask.canvas.width = 0;
      this.samMask.canvas.height = 0;
    }
    this.samMask = null;
    const c = this.config;
    if (pixels && c && pixels.length === c.width * c.height) {
      const canvas = document.createElement('canvas');
      canvas.width = c.width;
      canvas.height = c.height;
      const context = canvas.getContext('2d')!;
      const image = context.createImageData(c.width, c.height);
      const rgba = new Uint32Array(image.data.buffer);
      // Little-endian RGBA: cyan with full alpha; opacity is applied at draw time.
      const on = 0xffffd200;
      let x0 = c.width,
        y0 = c.height,
        x1 = -1,
        y1 = -1;
      for (let y = 0, i = 0; y < c.height; y++)
        for (let x = 0; x < c.width; x++, i++)
          if (pixels[i]) {
            rgba[i] = on;
            if (x < x0) x0 = x;
            if (x > x1) x1 = x;
            if (y < y0) y0 = y;
            if (y > y1) y1 = y;
          }
      context.putImageData(image, 0, 0);
      if (x1 >= 0)
        this.samMask = {
          pixels,
          canvas,
          bounds: { x: x0, y: y0, width: x1 - x0 + 1, height: y1 - y0 + 1 },
        };
      else {
        canvas.width = 0;
        canvas.height = 0;
      }
    }
    this.invalidate();
  }
  /** Right click / Alt+click: negative SAM point. Returns true when handled. */
  secondaryDown(point: Point): boolean {
    const c = this.config;
    if (!c || c.disabled || c.tool !== 'sam') return false;
    if (point.x < 0 || point.y < 0 || point.x > c.width || point.y > c.height)
      return true;
    this.samPoints.push({
      ...clampPoint(point, c.width, c.height),
      positive: false,
    });
    this.emitSam();
    return true;
  }
  /**
   * Writes the previewed SAM mask into the active layer as one undoable edit.
   * `onlyUnlabeled` keeps pixels that already have a semantic class.
   */
  applySam(onlyUnlabeled = false): boolean {
    const c = this.config,
      sam = this.samMask;
    if (!c || c.disabled || !sam) return false;
    const layer = c.profile.layers.find((l) => l.id === c.layerId)!;
    if (layer.layerType === 'instance') {
      if (!c.classId) {
        c.error('wrongTool');
        return false;
      }
      const runs: number[] = [];
      let value = sam.pixels[0]!,
        count = 0;
      for (const p of sam.pixels) {
        if (p === value) count++;
        else {
          runs.push(value, count);
          value = p;
          count = 1;
        }
      }
      runs.push(value, count);
      this.clearSam(true);
      this.shape({
        type: 'mask',
        width: c.width,
        height: c.height,
        runs,
        iscrowd: false,
      });
      return true;
    }
    const semantic = layer.layerType === 'semantic';
    const value = semantic
      ? (c.profile.classes.find((k) => k.id === c.classId)?.exportId ?? 255)
      : 1;
    const old = c.document.masks.find((m) => m.layerId === c.layerId);
    const pixels = old
      ? old.pixels.slice()
      : new Uint8Array(c.width * c.height).fill(semantic ? 255 : 0);
    const b = sam.bounds;
    for (let y = b.y; y < b.y + b.height; y++)
      for (let i = y * c.width + b.x, end = i + b.width; i < end; i++)
        if (sam.pixels[i] && (!onlyUnlabeled || !semantic || pixels[i] === 255))
          pixels[i] = value;
    if (old) rasterChanges.set(pixels, { source: old.pixels, bounds: b });
    const mask = {
      layerId: c.layerId,
      width: c.width,
      height: c.height,
      pixels,
    };
    this.renderer.dirty(mask, c.profile, b, old?.pixels);
    this.clearSam(true);
    this.commit({
      ...c.document,
      masks: [...c.document.masks.filter((m) => m.layerId !== c.layerId), mask],
    });
    return true;
  }
  get hasDraft() {
    return !!this.action || this.polygon.length > 0;
  }
  cancelPointer() {
    // Pointer capture is also lost normally after pointerUp has committed.
    // Releasing a completed gesture must not discard every raster backing store.
    if (!this.action) return;
    this.action = null;
    this.renderer.clear();
    this.invalidate();
  }
  dispose() {
    this.setSamMask(null);
    this.renderer.clear();
    this.listeners.clear();
    this.config = null;
    this.action = null;
    this.polygon = [];
  }
  cancel() {
    this.action = null;
    this.polygon = [];
    this.clearSam(true);
    this.renderer.clear();
    this.invalidate();
  }
  private commit(doc: EditDocument) {
    this.config!.commit({
      ...doc,
      reviewStatus:
        doc.reviewStatus === 'unlabeled' || doc.reviewStatus === 'approved'
          ? 'in_progress'
          : doc.reviewStatus,
    });
  }
  private shape(g: Geometry) {
    const c = this.config!;
    const shape: Annotation = {
      id: crypto.randomUUID(),
      layerId: c.layerId,
      classId:
        c.profile.layers.find((l) => l.id === c.layerId)?.layerType === 'region'
          ? null
          : c.classId,
      geometry: g,
      source: 'manual',
    };
    this.commit({
      ...c.document,
      annotations: [...c.document.annotations, shape],
    });
    this.select(shape.id);
  }
  pointerDown(point: Point): boolean {
    const c = this.config;
    if (!c || c.disabled || c.tool === 'pan') return false;
    // Polygon clicks just outside the image snap to its border, so a vertex can sit exactly on
    // the image edge (as in CVAT) instead of leaving a 1-px unlabeled strip.
    const margin = c.tool === 'polygon' ? 12 / this.zoom : 0;
    if (
      point.x < -margin ||
      point.y < -margin ||
      point.x > c.width + margin ||
      point.y > c.height + margin
    )
      return true;
    const p = clampPoint(point, c.width, c.height);
    const layer = c.profile.layers.find((l) => l.id === c.layerId)!;
    if (!c.visible.has(c.layerId)) {
      c.error('hiddenLayer');
      return true;
    }
    if (c.tool === 'sam') {
      if (layer.layerType !== 'region' && !c.classId) {
        c.error('wrongTool');
        return true;
      }
      this.action = { kind: 'samDrag', start: p, end: p };
    }
    if (c.tool === 'bbox') {
      if (layer.layerType !== 'instance' || !c.classId) {
        c.error('wrongTool');
        return true;
      }
      this.action = { kind: 'bbox', start: p, end: p };
    }
    if (c.tool === 'polygon') {
      if (
        (layer.layerType === 'semantic' || layer.layerType === 'instance') &&
        !c.classId
      ) {
        c.error('wrongTool');
        return true;
      }
      if (
        this.polygon.length >= 3 &&
        Math.hypot(p.x - this.polygon[0]!.x, p.y - this.polygon[0]!.y) *
          this.zoom <
          8
      )
        this.finishPolygon();
      else {
        if (this.polygon.length >= 10000) {
          c.error('invalidPolygon');
          return true;
        }
        this.polygon.push(p);
      }
    }
    if (c.tool === 'brush' || c.tool === 'eraser') {
      if (layer.layerType === 'instance') {
        c.error('wrongTool');
        return true;
      }
      const old = c.document.masks.find((m) => m.layerId === c.layerId);
      const pixels = old
        ? old.pixels.slice()
        : new Uint8Array(c.width * c.height).fill(
            layer.layerType === 'semantic' ? 255 : 0,
          );
      const mask = {
        layerId: c.layerId,
        width: c.width,
        height: c.height,
        pixels,
      };
      this.action = { kind: 'stroke', last: p, mask };
      this.paint(p);
    }
    if (c.tool === 'select') {
      const shape = c.document.annotations.find(
        (a) => a.id === this.selected && a.layerId === c.layerId,
      );
      const handles = shape
        ? shape.geometry.type === 'bbox'
          ? [
              { x: shape.geometry.x, y: shape.geometry.y },
              {
                x: shape.geometry.x + shape.geometry.width,
                y: shape.geometry.y,
              },
              {
                x: shape.geometry.x + shape.geometry.width,
                y: shape.geometry.y + shape.geometry.height,
              },
              {
                x: shape.geometry.x,
                y: shape.geometry.y + shape.geometry.height,
              },
            ]
          : shape.geometry.type === 'polygon'
            ? shape.geometry.points
            : []
        : [];
      const handle = handles.findIndex(
        (v) => Math.hypot(v.x - p.x, v.y - p.y) * this.zoom <= 8,
      );
      const hit =
        handle >= 0
          ? shape
          : [...c.document.annotations]
              .reverse()
              .find((a) => a.layerId === c.layerId && hitShape(p, a.geometry));
      this.select(hit?.id ?? null);
      if (hit && hit.geometry.type !== 'mask')
        this.action = {
          kind: 'transform',
          start: p,
          shape: hit,
          preview: hit,
          handle,
        };
    }
    this.invalidate();
    return true;
  }
  pointerMove(point: Point) {
    const c = this.config;
    if (!c) return;
    this.cursor = point;
    const p = clampPoint(point, c.width, c.height);
    const action = this.action;
    if (action?.kind === 'bbox' || action?.kind === 'samDrag') action.end = p;
    if (action?.kind === 'stroke') this.paint(p);
    if (action?.kind === 'transform') {
      const g = action.shape.geometry;
      let geometry: Geometry;
      if (action.handle >= 0 && g.type === 'bbox') {
        const opposite = [
          { x: g.x + g.width, y: g.y + g.height },
          { x: g.x, y: g.y + g.height },
          { x: g.x, y: g.y },
          { x: g.x + g.width, y: g.y },
        ][action.handle]!;
        geometry = boxFromPoints(opposite, p);
      } else if (action.handle >= 0 && g.type === 'polygon') {
        geometry = {
          ...g,
          points: g.points.map((v, i) => (i === action.handle ? p : v)),
        };
      } else
        geometry = moveShape(
          g,
          p.x - action.start.x,
          p.y - action.start.y,
          c.width,
          c.height,
        );
      action.preview = { ...action.shape, geometry };
    }
    this.invalidate();
  }
  private paint(p: Point) {
    const c = this.config!,
      action = this.action;
    if (action?.kind !== 'stroke') return;
    const layer = c.profile.layers.find((l) => l.id === c.layerId)!;
    const value =
      c.tool === 'eraser'
        ? layer.layerType === 'semantic'
          ? 255
          : 0
        : layer.layerType === 'region'
          ? 1
          : (c.profile.classes.find((k) => k.id === c.classId)?.exportId ??
            255);
    const bounds = paintSegment(
      action.mask.pixels,
      c.width,
      c.height,
      action.last,
      p,
      c.radius,
      value,
    );
    action.dirty = this.union(action.dirty, bounds);
    action.changed = this.union(action.changed, bounds);
    action.last = p;
  }
  private union(old: Bounds | undefined, bounds: Bounds): Bounds {
    if (old) {
      const x = Math.min(old.x, bounds.x),
        y = Math.min(old.y, bounds.y);
      return {
        x,
        y,
        width: Math.max(old.x + old.width, bounds.x + bounds.width) - x,
        height: Math.max(old.y + old.height, bounds.y + bounds.height) - y,
      };
    } else return bounds;
  }
  private flushStroke() {
    const action = this.action;
    if (action?.kind === 'stroke' && action.dirty) {
      this.renderer.dirty(
        action.mask,
        this.config!.profile,
        action.dirty,
        this.config!.document.masks.find(
          (m) => m.layerId === action.mask.layerId,
        )?.pixels,
      );
      action.dirty = undefined;
    }
  }
  pointerUp(point: Point) {
    const c = this.config;
    if (!c) return;
    this.pointerMove(point);
    this.flushStroke();
    const action = this.action;
    this.action = null;
    if (action?.kind === 'samDrag') {
      const box = boxFromPoints(action.start, action.end);
      if (Math.max(box.width, box.height) * this.zoom < 6)
        this.samPoints.push({ ...action.start, positive: true });
      else {
        this.samBox = {
          x: box.x,
          y: box.y,
          width: box.width,
          height: box.height,
        };
      }
      this.emitSam();
    }
    if (action?.kind === 'bbox') {
      const geometry = boxFromPoints(action.start, action.end);
      if (geometry.width >= 2 && geometry.height >= 2) this.shape(geometry);
    }
    if (action?.kind === 'stroke') {
      const source = c.document.masks.find(
        (m) => m.layerId === c.layerId,
      )?.pixels;
      if (source && action.changed)
        rasterChanges.set(action.mask.pixels, {
          source,
          bounds: action.changed,
        });
      this.commit({
        ...c.document,
        masks: [
          ...c.document.masks.filter((m) => m.layerId !== c.layerId),
          action.mask,
        ],
      });
    }
    if (action?.kind === 'transform') {
      const g = action.preview.geometry;
      if (g.type === 'polygon' && !validPolygonRings(g.points, g.holes)) {
        c.error('invalidPolygon');
        this.renderer.clear();
      } else if (g.type === 'bbox' && (g.width < 2 || g.height < 2)) {
        c.error('invalidBox');
      } else if (JSON.stringify(g) !== JSON.stringify(action.shape.geometry))
        this.commit({
          ...c.document,
          annotations: c.document.annotations.map((a) =>
            a.id === action.preview.id ? action.preview : a,
          ),
        });
    }
    this.invalidate();
  }
  finishPolygon() {
    if (!this.polygon.length) return;
    if (!validPolygon(this.polygon)) {
      this.config?.error('invalidPolygon');
      return;
    }
    const points = this.polygon;
    this.polygon = [];
    const c = this.config!;
    if (
      c.profile.layers.find((l) => l.id === c.layerId)?.layerType === 'semantic'
    )
      this.fillSemantic(points);
    else this.shape({ type: 'polygon', points, holes: [] });
    this.invalidate();
  }
  /** Paints the polygon into the semantic raster: one undoable command, later shapes win. */
  private fillSemantic(points: Point[]) {
    const c = this.config!;
    const value =
      c.profile.classes.find((k) => k.id === c.classId)?.exportId ?? 255;
    const old = c.document.masks.find((m) => m.layerId === c.layerId);
    const pixels = old
      ? old.pixels.slice()
      : new Uint8Array(c.width * c.height).fill(255);
    const bounds = fillPolygon(pixels, c.width, c.height, points, value);
    if (!bounds.width || !bounds.height) return;
    if (old) rasterChanges.set(pixels, { source: old.pixels, bounds });
    const mask = {
      layerId: c.layerId,
      width: c.width,
      height: c.height,
      pixels,
    };
    // Recolor only the polygon's bounding box in the cached overlay.
    this.renderer.dirty(mask, c.profile, bounds, old?.pixels);
    this.commit({
      ...c.document,
      masks: [...c.document.masks.filter((m) => m.layerId !== c.layerId), mask],
    });
  }
  backspace() {
    this.polygon.pop();
    this.invalidate();
  }
  deleteSelected(confirmed: boolean) {
    const c = this.config;
    if (!c || !confirmed || !this.selected || c.disabled) return;
    this.commit({
      ...c.document,
      annotations: c.document.annotations.filter((a) => a.id !== this.selected),
    });
    this.select(null);
  }
  assignClass(classId: string) {
    const c = this.config;
    if (!c || !this.selected || c.disabled) return;
    const target = c.profile.classes.find((k) => k.id === classId);
    const shape = c.document.annotations.find((a) => a.id === this.selected);
    const layer = c.profile.layers.find((l) => l.id === shape?.layerId);
    if (
      !shape ||
      !target ||
      layer?.namespace !== target.namespace ||
      target.isIgnore
    )
      return;
    this.commit({
      ...c.document,
      annotations: c.document.annotations.map((a) =>
        a.id === this.selected ? { ...a, classId } : a,
      ),
    });
  }
  draw(ctx: CanvasRenderingContext2D, zoom: number) {
    const c = this.config;
    if (!c) return;
    this.zoom = zoom;
    this.flushStroke();
    const a = this.action;
    this.renderer.draw(
      ctx,
      c.document,
      c.profile,
      zoom,
      c.visible,
      c.opacity,
      this.selected,
      a?.kind === 'bbox' ? boxFromPoints(a.start, a.end) : null,
      this.polygon,
      this.cursor,
      c.radius,
      c.tool === 'brush' || c.tool === 'eraser',
      a?.kind === 'stroke' ? a.mask : null,
      a?.kind === 'transform' ? a.preview : null,
    );
    this.drawSam(ctx, zoom, a?.kind === 'samDrag' ? a : null);
  }
  private drawSam(
    ctx: CanvasRenderingContext2D,
    zoom: number,
    drag: { start: Point; end: Point } | null,
  ) {
    if (this.samMask) {
      ctx.save();
      ctx.globalAlpha = 0.5;
      ctx.imageSmoothingEnabled = false;
      ctx.drawImage(this.samMask.canvas, 0, 0);
      ctx.restore();
    }
    const box = drag
      ? boxFromPoints(drag.start, drag.end)
      : this.samBox && { ...this.samBox };
    ctx.save();
    ctx.lineWidth = 2 / zoom;
    if (box && box.width * zoom >= 6) {
      ctx.strokeStyle = '#00d2ff';
      ctx.setLineDash([6 / zoom, 4 / zoom]);
      ctx.strokeRect(box.x, box.y, box.width, box.height);
      ctx.setLineDash([]);
    }
    for (const p of this.samPoints) {
      ctx.beginPath();
      ctx.arc(p.x, p.y, 6 / zoom, 0, Math.PI * 2);
      ctx.fillStyle = p.positive ? '#2fd36b' : '#ff4d6d';
      ctx.fill();
      ctx.strokeStyle = '#fff';
      ctx.stroke();
    }
    ctx.restore();
  }
}
