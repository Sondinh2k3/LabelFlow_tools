import { test, expect } from '@playwright/test';
import { mkdir, writeFile } from 'node:fs/promises';
import type { Profile } from '../../src/lib/contracts';
import type { EditDocument } from '../../src/features/annotations/history/session';

test('M5 brush p50/p95 versus M1 and renderer cache retention', async ({
  page,
}) => {
  test.skip(
    process.env.LABELFLOW_M5_BENCHMARK !== '1',
    'Run npm run benchmark:m5',
  );
  await page.goto('/');
  const result = await page.evaluate(async () => {
    const { paintSegment } = await import(
      '/src/features/annotations/geometry/raster.ts' as string
    );
    const { m1PaintSegment } = await import(
      '/tests/fixtures/m1Brush.ts' as string
    );
    const { SceneRenderer } = await import(
      '/src/features/annotations/canvas/renderer.ts' as string
    );
    const width = 1920,
      height = 1080,
      old = new Uint8Array(width * height),
      next = new Uint8Array(width * height);
    const stats = (values: number[]) => {
      values.sort((a, b) => a - b);
      return {
        p50Ms: values[Math.floor(values.length * 0.5)],
        p95Ms: values[Math.floor(values.length * 0.95)],
        samples: values.length,
      };
    };
    const cases = [
      {
        name: '32 short brush moves, radius 16',
        radius: 16,
        value: 3,
        points: Array.from({ length: 33 }, (_, i) => ({
          x: 300 + i * 12,
          y: 400 + Math.sin(i / 3) * 40,
        })),
      },
      {
        name: '4 long diagonal moves, radius 24',
        radius: 24,
        value: 0,
        points: [
          { x: 20, y: 20 },
          { x: 1800, y: 1000 },
          { x: 50, y: 950 },
          { x: 1700, y: 50 },
          { x: 20, y: 20 },
        ],
      },
      {
        name: '32 clipped eraser moves, radius 32',
        radius: 32,
        value: 255,
        points: Array.from({ length: 33 }, (_, i) => ({
          x: i * 15 - 10,
          y: Math.sin(i / 5) * 20,
        })),
      },
    ];
    const brush = [];
    for (const c of cases) {
      const baseline: number[] = [],
        optimized: number[] = [];
      for (let sample = 0; sample < 55; sample++) {
        old.fill(c.value === 255 ? 3 : 255);
        next.fill(c.value === 255 ? 3 : 255);
        let at = performance.now();
        for (let i = 1; i < c.points.length; i++)
          m1PaintSegment(
            old,
            width,
            height,
            c.points[i - 1]!,
            c.points[i]!,
            c.radius,
            c.value,
          );
        if (sample >= 5) baseline.push(performance.now() - at);
        at = performance.now();
        for (let i = 1; i < c.points.length; i++)
          paintSegment(
            next,
            width,
            height,
            c.points[i - 1]!,
            c.points[i]!,
            c.radius,
            c.value,
          );
        if (sample >= 5) optimized.push(performance.now() - at);
      }
      let exact = true;
      for (let p = 0; p < old.length; p++)
        if (old[p] !== next[p]) {
          exact = false;
          break;
        }
      brush.push({
        workload: c.name,
        baseline: stats(baseline),
        optimized: stats(optimized),
        exact,
      });
    }
    const renderer = new SceneRenderer();
    const canvas = document.createElement('canvas');
    canvas.width = 128;
    canvas.height = 96;
    const ctx = canvas.getContext('2d')!;
    const profile = {
      id: 'benchmark',
      classes: [
        {
          id: 'object',
          namespace: 'candidate',
          stableKey: 'object',
          exportId: 0,
          displayName: 'Object',
          isIgnore: false,
          color: '#ffb86b',
        },
      ],
      layers: [
        {
          id: 'objects',
          stableKey: 'objects',
          displayName: 'Objects',
          namespace: 'candidate',
          layerType: 'instance',
          color: '#ffb86b',
        },
        {
          id: 'semantic',
          stableKey: 'semantic',
          displayName: 'Semantic',
          color: '#804080',
          layerType: 'semantic',
          namespace: 'test',
        },
      ],
    } as Profile;
    const oldCache = new Map<object, HTMLCanvasElement>();
    const editDocument: EditDocument = {
      mediaId: 'benchmark',
      revision: 1,
      reviewStatus: 'in_progress',
      eventLabel: null,
      notes: '',
      splitGroup: 'group',
      annotations: [],
      masks: [],
    };
    for (let generation = 0; generation < 50; generation++) {
      editDocument.annotations = Array.from({ length: 10 }, (_, i) => ({
        id: String(i),
        layerId: 'objects',
        classId: 'object',
        source: 'manual' as const,
        geometry: {
          type: 'mask' as const,
          width: 128,
          height: 96,
          runs: [0, generation + 1, 1, 128 * 96 - generation - 1],
          iscrowd: false,
        },
      }));
      for (const shape of editDocument.annotations) {
        const backing = document.createElement('canvas');
        backing.width = 128;
        backing.height = 96;
        const context = backing.getContext('2d')!;
        context.fillStyle = '#ffb86b';
        context.fillRect(0, 0, 128, 96);
        context.clearRect(0, 0, generation + 1, 1);
        oldCache.set(shape.geometry, backing); // M4 geometry-key retention, below its 64 MiB cap.
      }
      renderer.draw(
        ctx,
        editDocument,
        profile,
        1,
        new Set(['objects']),
        0.5,
        null,
        null,
        [],
        null,
        0,
        false,
        null,
        null,
      );
    }
    const retained = (
      renderer as unknown as {
        instances: Map<object, { canvas: HTMLCanvasElement }>;
      }
    ).instances;
    const instanceBytes = [...retained.values()].reduce(
      (n, c) => n + c.canvas.width * c.canvas.height * 4,
      0,
    );
    const instanceEntries = retained.size;
    editDocument.masks = [
      {
        layerId: 'semantic',
        width: 128,
        height: 96,
        pixels: new Uint8Array(128 * 96).fill(255),
      },
    ];
    renderer.draw(
      ctx,
      editDocument,
      profile,
      1,
      new Set(['semantic']),
      0.5,
      null,
      null,
      [],
      null,
      0,
      false,
      null,
      null,
    );
    renderer.draw(
      ctx,
      editDocument,
      profile,
      1,
      new Set(),
      0.5,
      null,
      null,
      [],
      null,
      0,
      false,
      null,
      null,
    );
    const hiddenRasterEntries = (
      renderer as unknown as { cache: Map<string, object> }
    ).cache.size;
    return {
      width,
      height,
      brush,
      cache: {
        baselineM4Entries: oldCache.size,
        baselineM4RgbaBytes: [...oldCache.values()].reduce(
          (a, b) => a + b.width * b.height * 4,
          0,
        ),
        m5Entries: instanceEntries,
        m5RgbaBytes: instanceBytes,
        hiddenRasterEntries,
        scope:
          'Referenced Canvas2D backing-store RGBA bytes; not OS working set',
      },
      userAgent: navigator.userAgent,
      scope:
        'M1 capsule-distance brush versus M5 scanline kernel; input arrays reset outside timed samples; excludes persistence and display scheduling',
    };
  });
  expect(result.brush.every((c) => c.exact)).toBe(true);
  expect(result.cache.m5RgbaBytes).toBe(10 * 128 * 96 * 4);
  expect(result.cache.hiddenRasterEntries).toBe(0);
  await mkdir('.cache/m5-benchmark', { recursive: true });
  await writeFile(
    '.cache/m5-benchmark/renderer.json',
    JSON.stringify(result, null, 2) + '\n',
  );
});
