import { expect } from '@playwright/test';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { withDesktop } from './desktop-harness.mjs';
const latest = JSON.parse(
  await readFile('.cache/m2-acceptance/latest.json', 'utf8'),
);
process.env.LABELFLOW_FFMPEG ??= resolve('.tools/ffmpeg/ffmpeg.exe');
process.env.LABELFLOW_FFPROBE ??= resolve('.tools/ffmpeg/ffprobe.exe');
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
const baseline = await withDesktop(
  ['--project', latest.project],
  async (page) => {
    await expect(
      page.getByRole('heading', { name: 'M2 video acceptance', exact: true }),
    ).toBeVisible();
    const records = await invoke(page, 'list_media', {
      offset: 0,
      limit: 200,
      query: '',
      status: '',
    });
    expect(records.total).toBeGreaterThan(10);
    const documents = [];
    for (const record of records.items)
      documents.push(
        await invoke(page, 'load_document', { mediaId: record.id }),
      );
    expect(
      documents.some((d) =>
        d.annotations.some((a) => a.geometry.type === 'mask'),
      ),
    ).toBe(true);
    await page
      .getByRole('button', { name: records.items[0].name, exact: true })
      .first()
      .click();
    await expect(
      page.getByRole('button', { name: 'Zoom in', exact: true }),
    ).toBeEnabled();
    const preview = await invoke(page, 'select_media', {
      id: records.items[0].id,
    });
    expect(preview.dataUrl).toMatch(/^http:\/\/lfmedia.localhost\//);
    expect(
      await page.evaluate(
        (url) =>
          new Promise((done, reject) => {
            const img = new Image();
            img.onload = () => done([img.naturalWidth, img.naturalHeight]);
            img.onerror = reject;
            img.src = url;
          }),
        preview.dataUrl,
      ),
    ).toEqual([records.items[0].width, records.items[0].height]);
    await page.screenshot({
      path: 'docs/milestones/screenshots/m2-instance-desktop.png',
    });
    await page
      .getByRole('button', { name: 'Video workspace', exact: true })
      .click();
    const dialog = page.getByRole('dialog', {
      name: 'Video workspace',
      exact: true,
    });
    const video = latest.videos.find((v) => v.video.name === 'vfr.mp4').video;
    await dialog
      .getByRole('combobox', { name: 'Video workspace', exact: true })
      .selectOption(video.id);
    await expect(dialog.locator('video')).toHaveAttribute(
      'src',
      /^http:\/\/lfmedia.localhost\//,
    );
    await expect
      .poll(() => dialog.locator('video').evaluate((v) => v.readyState))
      .toBeGreaterThanOrEqual(1);
    await dialog.locator('video').evaluate(async (v) => {
      await v.play();
    });
    await expect
      .poll(() => dialog.locator('video').evaluate((v) => v.currentTime))
      .toBeGreaterThan(0);
    await dialog.locator('video').evaluate((v) => v.pause());
    await dialog
      .getByRole('spinbutton', { name: 'Frame index', exact: true })
      .fill('1');
    await dialog
      .getByRole('button', { name: 'Show exact frame', exact: true })
      .click();
    await expect(
      dialog.getByAltText('Frame index 1', { exact: true }),
    ).toBeVisible();
    const expected = latest.videos.find((v) => v.video.id === video.id)
      .frames[1];
    await expect(
      dialog.getByText(new RegExp(`PTS ${expected.pts} `)),
    ).toBeVisible();
    await page.screenshot({
      path: 'docs/milestones/screenshots/m2-video-desktop.png',
    });
    // Re-extraction must retain the existing media identity and every annotation.
    await dialog
      .getByRole('button', { name: 'Extract selected frame', exact: true })
      .click();
    const report = page.getByRole('dialog', {
      name: 'Operation report',
      exact: true,
    });
    await expect(report).toBeVisible();
    await report.getByRole('button', { name: 'Close', exact: true }).click();
    await dialog.getByRole('button', { name: 'Close', exact: true }).click();
    await page
      .getByRole('button', { name: 'Export dataset', exact: true })
      .click();
    const wizard = page.getByRole('dialog', {
      name: 'Export dataset',
      exact: true,
    });
    await wizard.getByRole('combobox').selectOption('coco');
    await wizard
      .getByRole('button', {
        name: 'Validate source pixels and annotations',
        exact: true,
      })
      .click();
    await expect(wizard.getByText(/Images: [0-9]+/)).toBeVisible();
    await expect(
      wizard.getByRole('button', {
        name: 'Choose destination and export',
        exact: true,
      }),
    ).toBeEnabled();
    await page.screenshot({
      path: 'docs/milestones/screenshots/m2-export-desktop.png',
    });
    await wizard.getByRole('button', { name: 'Close', exact: true }).click();
    const after = [];
    for (const record of records.items)
      after.push(await invoke(page, 'load_document', { mediaId: record.id }));
    expect(after).toEqual(documents);
    expect(await page.getByRole('alert').count()).toBe(0);
    return documents;
  },
);
await withDesktop(['--project', latest.project], async (page) => {
  await expect(
    page.getByRole('heading', { name: 'M2 video acceptance', exact: true }),
  ).toBeVisible();
  const restored = [];
  for (const before of baseline)
    restored.push(
      await invoke(page, 'load_document', { mediaId: before.mediaId }),
    );
  expect(restored).toEqual(baseline);
});
await writeFile(
  '.cache/m2-acceptance/desktop.json',
  JSON.stringify(
    {
      packagedFrontend: true,
      nativeIpc: true,
      binaryPreviewProtocol: true,
      h264Playback: true,
      vfrExactFrame: true,
      duplicateExtractionPreserved: true,
      qualityWizard: true,
      reopenedDocuments: baseline.length,
      allDocumentsExact: true,
    },
    null,
    2,
  ),
);
console.log(
  'PASS: packaged M2 desktop, playback, exact VFR preview, extraction, quality wizard and exact process-restart recovery.',
);
