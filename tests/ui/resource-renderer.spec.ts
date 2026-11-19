import { test, expect } from '@playwright/test';
test('brush coalesces uploads, reuses unchanged RGBA and undo/render remain byte exact', async ({
  page,
}) => {
  await page.goto('/');
  const result = await page.evaluate(async () => {
    const { SceneRenderer } = await import(
      '/src/features/annotations/canvas/renderer.ts' as string
    );
    const { EditorController } = await import(
      '/src/features/annotations/tools/controller.ts' as string
    );
    const { historyDelta, applyHistory } = await import(
      '/src/features/annotations/history/delta.ts' as string
    );
    const profile = {
      id: 'test',
      layers: [
        {
          id: 'semantic',
          layerType: 'semantic',
          namespace: 'classes',
          color: '#123456',
        },
      ],
      classes: [
        {
          id: 'road',
          exportId: 0,
          color: '#804080',
          namespace: 'classes',
          isIgnore: false,
        },
        {
          id: 'other',
          exportId: 1,
          color: '#336699',
          namespace: 'classes',
          isIgnore: false,
        },
      ],
    };
    const width = 137,
      height = 129,
      pixels = new Uint8Array(width * height);
    const before = {
      mediaId: 'test',
      revision: 0,
      reviewStatus: 'in_progress',
      eventLabel: null,
      notes: '',
      splitGroup: '',
      annotations: [],
      masks: [{ layerId: 'semantic', width, height, pixels }],
    };
    let after = before;
    const editor = new EditorController();
    editor.configure({
      document: before,
      profile,
      width,
      height,
      tool: 'brush',
      layerId: 'semantic',
      classId: 'other',
      radius: 4,
      visible: new Set(['semantic']),
      opacity: 1,
      disabled: false,
      commit: (doc: typeof before) => {
        after = doc;
      },
      selection: () => {},
      error: (message: string) => {
        throw Error(message);
      },
    });
    const canvas = document.createElement('canvas');
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext('2d')!;
    editor.draw(context, 1);
    let uploads = 0,
      uploadedPixels = 0;
    const real = CanvasRenderingContext2D.prototype.putImageData;
    CanvasRenderingContext2D.prototype.putImageData = function (
      ...args: [ImageData, number, number, number?, number?, number?, number?]
    ) {
      uploads++;
      uploadedPixels += (args[5] ?? width) * (args[6] ?? height);
      return Reflect.apply(real, this, args);
    };
    try {
      editor.pointerDown({ x: 80, y: 80 });
      for (let i = 0; i < 20; i++)
        editor.pointerMove({ x: 80 + i / 2, y: 80 + i / 2 });
      const beforeDraw = uploads;
      editor.draw(context, 1);
      const afterDraw = uploads;
      editor.pointerUp({ x: 90, y: 90 });
      const retained = editor.renderer.cache.get('semantic').canvas;
      editor.cancelPointer(); // The browser's normal lostpointercapture after pointerUp.
      const keptAfterPointerRelease =
        editor.renderer.cache.get('semantic')?.canvas === retained;
      const strokeUploadedPixels = uploadedPixels;
      const delta = historyDelta(before, after),
        undo = applyHistory(after, delta, 'before'),
        redo = applyHistory(undo, delta, 'after');
      const renderer = new SceneRenderer();
      renderer.dirty(redo.masks[0], profile);
      const expected = renderer.cache.get('semantic').data.data;
      const actual = editor.renderer.cache.get('semantic').data.data;
      const exact = expected.every((v: number, i: number) => v === actual[i]);
      renderer.clear();
      editor.dispose();
      return {
        beforeDraw,
        afterDraw,
        uploadedPixels: strokeUploadedPixels,
        fullPixels: width * height,
        exact,
        undoExact: undo.masks[0].pixels.every((v: number) => v === 0),
        sourceUnchanged: pixels.every((v) => v === 0),
        keptAfterPointerRelease,
        releasedBacking: retained.width === 0 && retained.height === 0,
      };
    } finally {
      CanvasRenderingContext2D.prototype.putImageData = real;
      editor.dispose();
    }
  });
  expect(result.beforeDraw).toBe(0);
  expect(result.afterDraw).toBe(1);
  expect(result.uploadedPixels).toBeLessThan(result.fullPixels);
  expect(result.exact && result.undoExact && result.sourceUnchanged).toBe(true);
  expect(result.keptAfterPointerRelease).toBe(true);
  expect(result.releasedBacking).toBe(true);
});
