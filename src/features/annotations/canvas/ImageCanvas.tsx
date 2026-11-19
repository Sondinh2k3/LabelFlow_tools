import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import type { ImagePreview } from '../../../lib/contracts';
import { text } from '../../../app/strings';
import { isTextEditing, shortcuts } from '../../../app/commands';
import {
  backingSize,
  fitImage,
  pointerToScreen,
  screenToImage,
  zoomAt,
  type Point,
  type Size,
  type Viewport,
} from '../geometry/viewport';
import type { EditorController } from '../tools/controller';
import { decodedImages } from './imagePool';

export function ImageCanvas({
  image,
  editor,
  overlay,
  pick,
}: {
  image: ImagePreview | null;
  editor?: EditorController;
  overlay?: (context: CanvasRenderingContext2D, zoom: number) => void;
  /** When set, a left click places a point (e.g. a discussion pin) instead of editing. */
  pick?: ((point: Point) => void) | null;
}) {
  const host = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const drag = useRef<{ start: Point; pan: Point; pointerId: number } | null>(
    null,
  );
  const space = useRef(false);
  useEffect(() => {
    const element = canvas.current;
    return () => {
      if (element) {
        element.width = 0;
        element.height = 0;
      }
    };
  }, []);
  const [size, setSize] = useState<Size>({ width: 1, height: 1 });
  const [decoded, setDecoded] = useState<{
    url: string;
    bitmap: HTMLImageElement;
  } | null>(null);
  const [failedUrl, setFailedUrl] = useState<string | null>(null);
  const bitmap =
    decoded?.url === image?.dataUrl ? (decoded?.bitmap ?? null) : null;
  const error = !!image && failedUrl === image.dataUrl;
  const [view, setView] = useState<Viewport>({ zoom: 1, pan: { x: 0, y: 0 } });
  const viewRef = useRef(view);
  const sizeRef = useRef(size);
  useLayoutEffect(() => {
    viewRef.current = view;
  }, [view]);

  const fit = useCallback(() => {
    if (image) setView(fitImage(image.info, sizeRef.current));
  }, [image]);
  const actual = useCallback(
    () =>
      setView((v) =>
        zoomAt(
          v,
          { x: sizeRef.current.width / 2, y: sizeRef.current.height / 2 },
          1 / v.zoom,
        ),
      ),
    [],
  );
  const scale = (factor: number) =>
    setView((v) =>
      zoomAt(v, { x: size.width / 2, y: size.height / 2 }, factor),
    );

  useEffect(() => {
    const element = host.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      const next = {
        width: entry.contentRect.width,
        height: entry.contentRect.height,
      };
      const old = sizeRef.current;
      setView((v) => ({
        ...v,
        pan: {
          x: v.pan.x + (next.width - old.width) / 2,
          y: v.pan.y + (next.height - old.height) / 2,
        },
      }));
      sizeRef.current = next;
      setSize(next);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    let cancelled = false;
    if (!image) return;
    const lease = decodedImages.acquire(image);
    void lease.ready
      .then((decoded) => {
        if (!cancelled) {
          // Cached images can finish before ResizeObserver's first delivery.
          const measured = host.current
            ? {
                width: host.current.clientWidth,
                height: host.current.clientHeight,
              }
            : sizeRef.current;
          sizeRef.current = measured;
          setSize(measured);
          setDecoded({ url: image.dataUrl, bitmap: decoded });
          setFailedUrl(null);
          setView(fitImage(image.info, measured));
        }
      })
      .catch(() => {
        if (!cancelled) setFailedUrl(image.dataUrl);
      });
    return () => {
      cancelled = true;
      lease.release();
    };
  }, [image]);

  useEffect(() => {
    const element = host.current;
    if (!element) return;
    const wheel = (event: WheelEvent) => {
      if (!image) return;
      event.preventDefault();
      const rect = element.getBoundingClientRect();
      const pointer = pointerToScreen(
        { x: event.clientX, y: event.clientY },
        rect,
        sizeRef.current,
      );
      const delta =
        event.deltaY *
        (event.deltaMode === 1
          ? 16
          : event.deltaMode === 2
            ? sizeRef.current.height
            : 1);
      setView((v) =>
        zoomAt(
          v,
          pointer,
          Math.exp(-Math.max(-500, Math.min(500, delta)) * 0.002),
        ),
      );
    };
    element.addEventListener('wheel', wheel, { passive: false });
    return () => element.removeEventListener('wheel', wheel);
  }, [image]);

  useEffect(() => {
    const down = (event: KeyboardEvent) => {
      const modal = [...document.querySelectorAll('[aria-modal="true"]')].at(
        -1,
      );
      if (
        event.defaultPrevented ||
        isTextEditing(event.target) ||
        (modal && !modal.contains(host.current)) ||
        event.ctrlKey ||
        event.metaKey ||
        event.altKey
      )
        return;
      if (event.key === shortcuts.pan) {
        if (
          event.target instanceof HTMLElement &&
          event.target.closest('button,[role="button"]')
        )
          return;
        space.current = true;
        event.preventDefault();
      }
      if (event.key === shortcuts.fit) {
        fit();
        event.preventDefault();
      }
      if (event.key === shortcuts.reset) {
        actual();
        event.preventDefault();
      }
    };
    const clear = () => {
      space.current = false;
      drag.current = null;
      editor?.cancelPointer();
    };
    const up = (event: KeyboardEvent) => {
      if (event.key === shortcuts.pan) space.current = false;
    };
    window.addEventListener('keydown', down);
    window.addEventListener('keyup', up);
    window.addEventListener('blur', clear);
    return () => {
      window.removeEventListener('keydown', down);
      window.removeEventListener('keyup', up);
      window.removeEventListener('blur', clear);
    };
  }, [fit, actual, editor]);

  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    let frame = 0;
    const draw = () => {
      frame = 0;
      const dpr = window.devicePixelRatio || 1;
      const backing = backingSize(size, dpr);
      // Assigning even the same size clears and reallocates the backing store.
      if (element.width !== Math.max(1, backing.width))
        element.width = Math.max(1, backing.width);
      if (element.height !== Math.max(1, backing.height))
        element.height = Math.max(1, backing.height);
      const ctx = element.getContext('2d');
      if (!ctx) return;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, size.width, size.height);
      if (bitmap) {
        ctx.translate(view.pan.x, view.pan.y);
        ctx.scale(view.zoom, view.zoom);
        ctx.imageSmoothingEnabled = view.zoom < 2;
        ctx.drawImage(bitmap, 0, 0);
        editor?.draw(ctx, view.zoom);
        overlay?.(ctx, view.zoom);
      }
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(draw);
    };
    schedule();
    const unsubscribe = editor?.subscribe(schedule);
    // Monitor scale changes when the window moves between monitors.
    let resolution = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
    const changed = () => {
      schedule();
      resolution.removeEventListener('change', changed);
      resolution = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
      resolution.addEventListener('change', changed);
    };
    resolution.addEventListener('change', changed);
    return () => {
      cancelAnimationFrame(frame);
      unsubscribe?.();
      resolution.removeEventListener('change', changed);
    };
  }, [bitmap, view, size, editor, overlay]);

  return (
    <section className="canvas-area" aria-label={text.workspace}>
      <div className="canvas-toolbar">
        <span className="filename">{image?.info.name ?? text.noImage}</span>
        <div className="zoom-controls">
          <button
            disabled={!bitmap}
            onClick={() => scale(1 / 1.2)}
            aria-label={text.zoomOut}
          >
            −
          </button>
          <output aria-label={text.zoom}>
            {Math.round(view.zoom * 1000) / 10}%
          </output>
          <button
            disabled={!bitmap}
            onClick={() => scale(1.2)}
            aria-label={text.zoomIn}
          >
            +
          </button>
          <button disabled={!bitmap} onClick={fit}>
            {text.fit}
          </button>
          <button disabled={!bitmap} onClick={actual}>
            {text.actual}
          </button>
        </div>
      </div>
      <div
        ref={host}
        className={`canvas-host${pick ? ' picking' : ''}`}
        data-testid="canvas-host"
        tabIndex={0}
        aria-label={text.workspace}
        onContextMenu={(event) => {
          if (editor) event.preventDefault();
        }}
        onPointerDown={(event) => {
          // Right click or Alt+click: a negative point for click-to-mask.
          if (
            bitmap &&
            editor &&
            !space.current &&
            (event.button === 2 || (event.button === 0 && event.altKey))
          ) {
            const screen = pointerToScreen(
              { x: event.clientX, y: event.clientY },
              event.currentTarget.getBoundingClientRect(),
              sizeRef.current,
            );
            if (editor.secondaryDown(screenToImage(screen, viewRef.current))) {
              event.preventDefault();
              return;
            }
          }
          if (
            !bitmap ||
            (event.button !== 0 && event.button !== 1 && !space.current)
          )
            return;
          event.preventDefault();
          event.currentTarget.focus();
          event.currentTarget.setPointerCapture(event.pointerId);
          const screen = pointerToScreen(
            { x: event.clientX, y: event.clientY },
            event.currentTarget.getBoundingClientRect(),
            sizeRef.current,
          );
          if (event.button === 0 && !space.current && pick) {
            pick(screenToImage(screen, viewRef.current));
            return;
          }
          if (
            event.button === 0 &&
            !space.current &&
            editor?.pointerDown(screenToImage(screen, viewRef.current))
          )
            return;
          drag.current = {
            start: pointerToScreen(
              { x: event.clientX, y: event.clientY },
              event.currentTarget.getBoundingClientRect(),
              sizeRef.current,
            ),
            pan: viewRef.current.pan,
            pointerId: event.pointerId,
          };
        }}
        onPointerMove={(event) => {
          const active = drag.current;
          const pointer = pointerToScreen(
            { x: event.clientX, y: event.clientY },
            event.currentTarget.getBoundingClientRect(),
            sizeRef.current,
          );
          if (!active) {
            editor?.pointerMove(screenToImage(pointer, viewRef.current));
            return;
          }
          if (active.pointerId !== event.pointerId) return;
          setView((v) => ({
            ...v,
            pan: {
              x: active.pan.x + pointer.x - active.start.x,
              y: active.pan.y + pointer.y - active.start.y,
            },
          }));
        }}
        onPointerUp={(event) => {
          if (!drag.current) {
            const p = pointerToScreen(
              { x: event.clientX, y: event.clientY },
              event.currentTarget.getBoundingClientRect(),
              sizeRef.current,
            );
            editor?.pointerUp(screenToImage(p, viewRef.current));
          }
          drag.current = null;
        }}
        onPointerCancel={() => {
          drag.current = null;
          editor?.cancel();
        }}
        onLostPointerCapture={() => {
          drag.current = null;
          editor?.cancelPointer();
        }}
        onDoubleClick={() => {
          if (!editor) fit();
        }}
      >
        <canvas
          ref={canvas}
          aria-label={image?.info.name ?? text.empty}
          style={{ width: size.width, height: size.height }}
        />
        {!bitmap && (
          <div className="canvas-empty">
            <span className="empty-symbol">＋</span>
            <h2>{error ? text.previewError : text.empty}</h2>
            <p>{text.emptyHelp}</p>
          </div>
        )}
      </div>
      <div className="canvas-caption">
        <span>{text.panHint}</span>
        <span>{bitmap ? `${bitmap.width} × ${bitmap.height}` : '—'}</span>
      </div>
    </section>
  );
}
