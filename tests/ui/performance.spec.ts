import { test, expect } from '@playwright/test';
import { mkdir, writeFile } from 'node:fs/promises';

test('M4 raster benchmark: 1080p replacement masks, exact RGBA and Canvas2D upload', async ({
  page,
}) => {
  test.skip(
    process.env.LABELFLOW_BENCHMARK !== '1',
    'Run npm run benchmark:m4 for measured performance',
  );
  await page.goto('/');
  const result = await page.evaluate(async () => {
    const { SceneRenderer } = await import(
      '/src/features/annotations/canvas/renderer.ts' as string
    );
    const width = 1920,
      height = 1080;
    const masks = [
      new Uint8Array(width * height),
      new Uint8Array(width * height),
    ];
    for (let m = 0; m < 2; m++)
      for (let i = 0; i < masks[m]!.length; i++)
        masks[m]![i] =
          (Math.floor(i / 97) + m) % 20 === 19
            ? 255
            : (Math.floor(i / 97) + m) % 19;
    const classes = Array.from({ length: 19 }, (_, id) => ({
      id: `class-${id}`,
      namespace: 'test',
      exportId: id,
      color: `#${(((id + 1) * 678901) % 16777216).toString(16).padStart(6, '0')}`,
      isIgnore: false,
    }));
    const profile = {
      id: 'benchmark',
      classes,
      layers: [
        {
          id: 'semantic',
          layerType: 'semantic',
          namespace: 'test',
          color: '#123456',
        },
      ],
    };
    const oldTimes: number[] = [],
      newTimes: number[] = [];
    const renderer = new SceneRenderer();
    let baselineData: ImageData | undefined;
    const baseline = (pixels: Uint8Array) => {
      // M3 source: a replacement pixel buffer creates canvas, ImageData and palette again.
      const canvas = document.createElement('canvas');
      canvas.width = width;
      canvas.height = height;
      const context = canvas.getContext('2d')!;
      const data = context.createImageData(width, height);
      const table = new Uint8ClampedArray(1024);
      for (const c of classes) {
        const at = c.exportId * 4;
        table[at] = parseInt(c.color.slice(1, 3), 16);
        table[at + 1] = parseInt(c.color.slice(3, 5), 16);
        table[at + 2] = parseInt(c.color.slice(5, 7), 16);
        table[at + 3] = 255;
      }
      for (let y = 0; y < height; y++)
        for (let x = 0; x < width; x++) {
          const p = y * width + x,
            at = p * 4,
            c = pixels[p]! * 4;
          data.data[at] = table[c]!;
          data.data[at + 1] = table[c + 1]!;
          data.data[at + 2] = table[c + 2]!;
          data.data[at + 3] = table[c + 3]!;
        }
      context.putImageData(data, 0, 0, 0, 0, width, height);
      baselineData = data;
    };
    for (let i = 0; i < 35; i++) {
      const pixels = masks[i % 2]!;
      let at = performance.now();
      baseline(pixels);
      if (i >= 5) oldTimes.push(performance.now() - at);
      at = performance.now();
      renderer.dirty({ layerId: 'semantic', width, height, pixels }, profile);
      if (i >= 5) newTimes.push(performance.now() - at);
    }
    // Inspect the rendered canvas cache only in this test to prove buffer reuse and byte identity.
    const item = (
      renderer as unknown as {
        cache: Map<string, { data: ImageData; canvas: HTMLCanvasElement }>;
      }
    ).cache.get('semantic')!;
    let equal = true;
    for (let i = 0; i < item.data.data.length; i++)
      if (item.data.data[i] !== baselineData!.data[i]) {
        equal = false;
        break;
      }
    const previous = item.data;
    renderer.dirty(
      { layerId: 'semantic', width, height, pixels: masks[1]! },
      profile,
    );
    const reused =
      (
        renderer as unknown as { cache: Map<string, { data: ImageData }> }
      ).cache.get('semantic')!.data === previous;
    const stats = (values: number[]) => {
      values.sort((a, b) => a - b);
      return {
        p50Ms: values[Math.floor(values.length * 0.5)],
        p95Ms: values[Math.floor(values.length * 0.95)],
        samples: values.length,
      };
    };
    return {
      width,
      height,
      baseline: stats(oldTimes),
      optimized: stats(newTimes),
      byteExact: equal,
      imageDataReused: reused,
      userAgent: navigator.userAgent,
      scope:
        'Canvas2D full-raster replacement colorization + upload; not whole-app FPS',
    };
  });
  expect(result.byteExact).toBe(true);
  expect(result.imageDataReused).toBe(true);
  await mkdir('.cache/m4-benchmark', { recursive: true });
  await writeFile(
    '.cache/m4-benchmark/renderer.json',
    JSON.stringify(result, null, 2) + '\n',
  );
});
