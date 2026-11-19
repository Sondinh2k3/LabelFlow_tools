import { expect } from '@playwright/test';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { withDesktop } from './desktop-harness.mjs';

const root =
  process.env.LABELFLOW_SMOKE_PROJECT ??
  JSON.parse(await readFile('.cache/m1-acceptance/latest.json', 'utf8'))
    .restored;
const invoke = (page, name, args = {}) =>
  page.evaluate(
    async ({ name, args }) => {
      try {
        return await window.__TAURI_INTERNALS__.invoke(name, args);
      } catch (error) {
        throw Error(`${name}: ${JSON.stringify(error)}`, { cause: error });
      }
    },
    { name, args },
  );
async function loadAll(page) {
  const records = await invoke(page, 'list_media', {
    offset: 0,
    limit: 200,
    query: '',
    status: '',
  });
  expect(records.total).toBe(100);
  const documents = [];
  for (const record of records.items)
    documents.push(await invoke(page, 'load_document', { mediaId: record.id }));
  console.log(`Loaded ${documents.length} real documents through native IPC.`);
  return documents;
}
const baseline = await withDesktop(['--project', root], async (page) => {
  await expect(page.getByRole('button', { name: 'Zoom in' })).toBeEnabled({
    timeout: 30000,
  });
  const documents = await loadAll(page);
  const current = await invoke(page, 'restore_image');
  const before = documents.find((doc) => doc.mediaId === current.info.id);
  const saveMs = [];
  await page.getByRole('button', { name: 'Bounding box', exact: true }).click();
  await page.getByRole('button', { name: 'Fit', exact: true }).click();
  const area = await page.getByTestId('canvas-host').boundingBox();
  for (let i = 0; i < 20; i++) {
    const x = area.x + area.width * (0.25 + (i % 5) * 0.07),
      y = area.y + area.height * (0.35 + Math.floor(i / 5) * 0.07);
    const started = performance.now();
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x + area.width * 0.055, y + area.height * 0.04, {
      steps: 3,
    });
    await page.mouse.up();
    await expect
      .poll(
        async () =>
          (await invoke(page, 'load_document', { mediaId: current.info.id }))
            .revision,
        { timeout: 30000 },
      )
      .toBe(before.revision + i + 1);
    saveMs.push(performance.now() - started);
  }
  const beforeStroke = await invoke(page, 'load_document', {
    mediaId: current.info.id,
  });
  expect(beforeStroke.annotations.length).toBe(before.annotations.length + 20);
  await page
    .getByLabel('Active layer', { exact: true })
    .selectOption('cityscapes_semantic');
  await page.getByRole('button', { name: 'Brush', exact: true }).click();
  await page.getByRole('button', { name: 'Fit', exact: true }).click();
  const rect = await page.getByTestId('canvas-host').boundingBox();
  const start = performance.now();
  await page.mouse.move(rect.x + rect.width * 0.4, rect.y + rect.height * 0.5);
  await page.mouse.down();
  await page.mouse.move(
    rect.x + rect.width * 0.65,
    rect.y + rect.height * 0.55,
    { steps: 30 },
  );
  await page.mouse.up();
  await expect(
    page.getByRole('button', { name: 'Undo', exact: true }),
  ).toBeEnabled();
  await expect
    .poll(
      async () =>
        (await invoke(page, 'load_document', { mediaId: current.info.id }))
          .revision,
      { timeout: 30000 },
    )
    .toBe(beforeStroke.revision + 1);
  const strokeSaveMs = performance.now() - start;
  expect(strokeSaveMs).toBeLessThan(15000);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect
    .poll(
      async () =>
        (await invoke(page, 'load_document', { mediaId: current.info.id }))
          .revision,
      { timeout: 30000 },
    )
    .toBe(beforeStroke.revision + 2);
  const afterStroke = await invoke(page, 'load_document', {
    mediaId: current.info.id,
  });
  expect({ ...afterStroke, revision: beforeStroke.revision }).toEqual(
    beforeStroke,
  );
  const errors = await invoke(page, 'project_health');
  expect(errors).toEqual([]);
  await page.screenshot({ path: 'docs/milestones/screenshots/m1-desktop.png' });
  for (let i = 0; i < 20; i++) {
    await page.getByRole('button', { name: 'Undo', exact: true }).click();
    await expect
      .poll(
        async () =>
          (await invoke(page, 'load_document', { mediaId: current.info.id }))
            .revision,
        { timeout: 30000 },
      )
      .toBe(beforeStroke.revision + 3 + i);
  }
  const after = await invoke(page, 'load_document', {
    mediaId: current.info.id,
  });
  expect({ ...after, revision: before.revision }).toEqual(before);
  console.log(
    `PASS: real Tauri IPC, 100 documents + masks, brush/undo saved in ${strokeSaveMs.toFixed(1)} ms`,
  );
  const expected = documents.map((doc) =>
    doc.mediaId === after.mediaId ? after : doc,
  );
  await page
    .getByRole('button', { name: 'Close project', exact: true })
    .click();
  await expect(
    page.getByRole('heading', { name: 'A workspace for your vision.' }),
  ).toBeVisible();
  saveMs.sort((a, b) => a - b);
  return {
    expected,
    strokeSaveMs,
    boxSaveP50Ms: saveMs[9],
    boxSaveP95Ms: saveMs[18],
    activeId: current.info.id,
  };
});
await withDesktop(['--project', root], async (page) => {
  await expect(page.getByRole('button', { name: 'Zoom in' })).toBeEnabled({
    timeout: 30000,
  });
  expect(await loadAll(page)).toEqual(baseline.expected);
  expect((await invoke(page, 'restore_image')).info.id).toBe(baseline.activeId);
  await mkdir('.cache/m1-acceptance', { recursive: true });
  await writeFile(
    '.cache/m1-acceptance/desktop.json',
    JSON.stringify(
      {
        project: root,
        images: 100,
        exactReopen: true,
        strokeSaveMs: baseline.strokeSaveMs,
        boxSaveP50Ms: baseline.boxSaveP50Ms,
        boxSaveP95Ms: baseline.boxSaveP95Ms,
        shapesDuringBrush:
          baseline.expected.find((doc) => doc.mediaId === baseline.activeId)
            .annotations.length + 20,
        runtime: 'packaged debug Tauri/WebView2; no Vite',
        date: new Date().toISOString(),
      },
      null,
      2,
    ),
  );
  console.log(
    'PASS: actual app process restart restored all 100 documents and masks exactly.',
  );
});
