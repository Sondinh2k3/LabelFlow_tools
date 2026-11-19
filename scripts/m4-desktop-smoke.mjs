import { expect } from '@playwright/test';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { withDesktop } from './desktop-harness.mjs';
const latest = JSON.parse(
  await readFile('.cache/m4-acceptance/latest.json', 'utf8'),
);
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
const sourceHash = async () =>
  createHash('sha256')
    .update(await readFile(latest.source))
    .digest('hex');
const baseline = await withDesktop(
  ['--project', latest.project],
  async (page) => {
    await expect(
      page.getByRole('heading', { name: 'M4 road acceptance', exact: true }),
    ).toBeVisible();
    const before = await invoke(page, 'load_document', {
      mediaId: latest.mediaId,
    });
    await page
      .getByRole('button', { name: 'calibration-1920x1080.png', exact: true })
      .first()
      .click();
    await page
      .getByRole('button', { name: 'Road / dataset review', exact: true })
      .click();
    const dialog = page.getByRole('dialog', {
      name: 'Road / dataset review',
      exact: true,
    });
    await expect(
      dialog.getByLabel('Stored prediction', { exact: true }),
    ).toHaveValue(latest.predictionKey);
    const snapshot = await invoke(page, 'review_snapshot', {
      key: latest.predictionKey,
    });
    expect(snapshot.comparison.gtApproved).toBe(true);
    expect(snapshot.comparison.eventMismatch).toBe(true);
    const road = snapshot.comparison.layers.find(
      (l) => l.layerId === 'road_extent',
    );
    expect(road.disagreementPixels).toBe(60_000);
    expect(
      snapshot.comparison.layers.find(
        (l) => l.layerId === 'cityscapes_semantic',
      ).ignoredPixels,
    ).toBe(1_293_600);
    await dialog
      .getByLabel('Review layer', { exact: true })
      .selectOption('road_extent');
    await dialog
      .getByLabel('Overlay view', { exact: true })
      .selectOption('difference');
    await expect(
      dialog.getByRole('button', { name: 'Zoom in', exact: true }),
    ).toBeEnabled();
    await page.screenshot({
      path: 'docs/milestones/screenshots/m4-disagreement-desktop.png',
    });
    await dialog
      .getByRole('button', { name: 'Accept pending prediction', exact: true })
      .click();
    await expect(dialog.getByRole('alert')).toContainText('protected');
    expect(
      await invoke(page, 'load_document', { mediaId: latest.mediaId }),
    ).toEqual(before);
    await dialog
      .getByRole('button', { name: 'Reject pending prediction', exact: true })
      .click();
    await dialog.getByRole('button', { name: 'Trips', exact: true }).click();
    await dialog.getByLabel('Trip name', { exact: true }).fill('M4 test route');
    await dialog
      .getByLabel('Trip route', { exact: true })
      .fill('Synthetic road fixture');
    await dialog
      .getByLabel('Measured trip distance (km, optional)', { exact: true })
      .fill('1.5');
    await dialog
      .getByRole('button', { name: 'Save trip', exact: true })
      .click();
    const [trip] = await invoke(page, 'list_trips');
    await dialog
      .getByRole('button', { name: 'Tags & capture', exact: true })
      .click();
    await dialog
      .getByLabel('Capture trip', { exact: true })
      .selectOption(trip.id);
    await dialog
      .getByLabel('Camera ID', { exact: true })
      .fill('camera-native-test');
    await dialog.getByLabel('hard negative:median', { exact: true }).check();
    await dialog.getByLabel('light:day', { exact: true }).check();
    await dialog.getByLabel('Latitude', { exact: true }).fill('10.8');
    await dialog.getByLabel('Longitude', { exact: true }).fill('106.6');
    await dialog
      .getByRole('button', {
        name: 'Save capture metadata & tags',
        exact: true,
      })
      .click();
    const metadata = await invoke(page, 'review_metadata', {
      mediaId: latest.mediaId,
    });
    expect(metadata.tags).toEqual(['hard_negative:median', 'light:day']);
    expect(metadata.tripId).toBe(trip.id);
    await expect(
      dialog.getByRole('button', { name: 'Cancel operation', exact: true }),
    ).toHaveCount(0);
    await dialog.evaluate((element) => {
      element.scrollTop = 0;
    });
    await page.screenshot({
      path: 'docs/milestones/screenshots/m4-metadata-desktop.png',
    });
    await dialog
      .getByRole('button', { name: 'Review queue', exact: true })
      .click();
    await dialog
      .getByRole('button', { name: 'Analyze comparisons', exact: true })
      .click();
    await dialog.getByLabel('Disagreements only', { exact: true }).check();
    await expect(
      dialog.getByRole('cell', { name: 'Disagreement', exact: true }),
    ).toBeVisible();
    await page.screenshot({
      path: 'docs/milestones/screenshots/m4-queue-desktop.png',
    });
    await dialog.getByRole('button', { name: 'Close', exact: true }).click();
    const after = await invoke(page, 'load_document', {
      mediaId: latest.mediaId,
    });
    expect(after.annotations).toEqual(before.annotations);
    expect(after.masks).toEqual(before.masks);
    expect(after.eventLabel).toBe('negative');
    expect(after.reviewStatus).toBe('approved');
    expect(after.splitGroup).toBe(`trip:${trip.id}`);
    expect(await sourceHash()).toBe(latest.sha256);
    return {
      document: after,
      metadata,
      trip,
      predictions: await invoke(page, 'review_predictions', {
        mediaId: latest.mediaId,
      }),
    };
  },
);
await withDesktop(['--project', latest.project], async (page) => {
  await expect(
    page.getByRole('heading', { name: 'M4 road acceptance', exact: true }),
  ).toBeVisible();
  expect(
    await invoke(page, 'load_document', { mediaId: latest.mediaId }),
  ).toEqual(baseline.document);
  expect(
    await invoke(page, 'review_metadata', { mediaId: latest.mediaId }),
  ).toEqual(baseline.metadata);
  expect(await invoke(page, 'list_trips')).toEqual([baseline.trip]);
  expect(
    await invoke(page, 'review_predictions', { mediaId: latest.mediaId }),
  ).toEqual(baseline.predictions);
  expect(await sourceHash()).toBe(latest.sha256);
});
await writeFile(
  '.cache/m4-acceptance/desktop.json',
  JSON.stringify(
    {
      passed: true,
      groundTruthPreserved: true,
      sourceUnchanged: true,
      restartExact: true,
      roadDisagreementPixels: 60000,
      semanticIgnoredPixels: 1293600,
      tripAndTags: true,
    },
    null,
    2,
  ) + '\n',
);
console.log(
  'M4 native desktop: independent overlays, exact disagreement, approved protection, trips/tags/GPS, current-revision queue and restart passed.',
);
