import { expect } from '@playwright/test';
import { readFile, writeFile } from 'node:fs/promises';
import { createHash, randomUUID } from 'node:crypto';
import { withDesktop } from './desktop-harness.mjs';
const latest = JSON.parse(
  await readFile('.cache/m5-acceptance/latest.json', 'utf8'),
);
const invoke = (page, name, args = {}) =>
  page.evaluate(
    async ({ name, args }) => {
      try {
        return await window.__TAURI_INTERNALS__.invoke(name, args);
      } catch (e) {
        throw Error(`${name}: ${JSON.stringify(e)}`, { cause: e });
      }
    },
    { name, args },
  );
const sha = async (path) =>
  createHash('sha256')
    .update(await readFile(path))
    .digest('hex');
const pixels = (m) => {
  const p = new Uint8Array(m.width * m.height);
  let at = 0;
  for (let i = 0; i < m.runs.length; i += 2) {
    p.fill(m.runs[i], at, at + m.runs[i + 1]);
    at += m.runs[i + 1];
  }
  return p;
};
const baseline = await withDesktop(
  ['--project', latest.project],
  async (page) => {
    await expect(
      page.getByRole('heading', { name: 'M5 acceptance', exact: true }),
    ).toBeVisible();
    await page
      .getByRole('button', { name: 'anchor.png', exact: true })
      .first()
      .click();
    const anchor = await invoke(page, 'load_document', {
      mediaId: latest.anchorId,
    });
    const target = await invoke(page, 'load_document', {
      mediaId: latest.targetId,
    });
    target.eventLabel = 'negative';
    target.notes = 'Human event must survive propagation';
    target.masks = [
      {
        layerId: 'cityscapes_semantic',
        width: 128,
        height: 96,
        runs: [7, 1, 255, 128 * 96 - 1],
      },
    ];
    await invoke(page, 'save_document', {
      document: target,
      operationId: randomUUID(),
    });
    const before = await invoke(page, 'load_document', {
      mediaId: latest.targetId,
    });
    const hashes = {};
    for (const name of ['anchor.png', 'target.png', 'next.png', 'flat.png']) {
      const path = latest.project.replace(/project$/, '') + name;
      hashes[path] = await sha(path);
    }
    await page
      .getByRole('button', { name: 'Dataset tools', exact: true })
      .click();
    let tools = page.getByRole('dialog', {
      name: 'Dataset tools',
      exact: true,
    });
    await expect(
      tools.getByText('Pending propagations', { exact: true }),
    ).toBeVisible();
    await page.screenshot({
      path: 'docs/milestones/screenshots/m5-dashboard-desktop.png',
    });
    await tools
      .getByRole('button', { name: 'Similar images', exact: true })
      .click();
    await tools
      .getByRole('button', { name: 'Index image fingerprints', exact: true })
      .click();
    await expect
      .poll(async () => (await invoke(page, 'quality_dashboard')).indexed)
      .toBe(4);
    await tools.getByLabel('Hamming distance', { exact: true }).fill('64');
    await tools
      .getByRole('button', { name: 'Rank candidates', exact: true })
      .click();
    await expect(tools.locator('tbody tr')).toHaveCount(3);
    await tools
      .getByRole('button', { name: 'target.png', exact: true })
      .click();
    await expect(
      tools.getByRole('img', { name: 'target.png', exact: true }),
    ).toBeVisible();
    await tools
      .getByRole('img', { name: 'target.png', exact: true })
      .scrollIntoViewIfNeeded();
    await page.screenshot({
      path: 'docs/milestones/screenshots/m5-similarity-desktop.png',
    });
    await tools
      .getByRole('button', { name: 'Video propagation', exact: true })
      .click();
    await tools.getByRole('checkbox', { name: /target.png/ }).check();
    await tools
      .getByRole('button', {
        name: 'Generate translation suggestions',
        exact: true,
      })
      .click();
    await tools
      .getByRole('button', {
        name: 'Open target for visual review',
        exact: true,
      })
      .click();
    const review = page.getByRole('dialog', {
      name: 'Road / dataset review',
      exact: true,
    });
    const accept = review.getByRole('button', {
      name: 'Accept pending prediction',
      exact: true,
    });
    await expect(accept).toBeDisabled();
    const rows = await invoke(page, 'review_predictions', {
      mediaId: latest.targetId,
    });
    const key = rows[0].key;
    const info = await invoke(page, 'propagation_info', { key });
    expect([info.dx, info.dy]).toEqual([3, 2]);
    expect(info.verifiedAt).toBeNull();
    expect(
      await invoke(page, 'load_document', { mediaId: latest.targetId }),
    ).toEqual(before);
    await expect(
      invoke(page, 'approve_review_prediction', {
        key,
        expectedRevision: before.revision,
      }),
    ).rejects.toThrow(/verify/i);
    await review
      .getByLabel('Overlay view', { exact: true })
      .selectOption('prediction');
    await review
      .getByLabel('Review layer', { exact: true })
      .selectOption('cityscapes_semantic');
    await review
      .getByRole('checkbox', {
        name: 'I inspected the target overlay, boundaries and classes',
        exact: true,
      })
      .check();
    await review
      .getByRole('button', { name: 'Mark visually verified', exact: true })
      .click();
    await expect(
      review.getByText('Verified for the current target revision', {
        exact: true,
      }),
    ).toBeVisible();
    await expect(accept).toBeEnabled();
    await page.screenshot({
      path: 'docs/milestones/screenshots/m5-propagation-desktop.png',
    });
    await accept.click();
    await expect
      .poll(
        async () =>
          (await invoke(page, 'load_document', { mediaId: latest.targetId }))
            .revision,
      )
      .toBe(before.revision + 1);
    const accepted = await invoke(page, 'load_document', {
      mediaId: latest.targetId,
    });
    expect(accepted.eventLabel).toBe('negative');
    expect(accepted.reviewStatus).toBe('needs_review');
    const mask = pixels(
      accepted.masks.find((m) => m.layerId === 'cityscapes_semantic'),
    );
    expect(mask[0]).toBe(7);
    expect(mask[22 * 128 + 23]).toBe(0);
    expect(mask[1]).toBe(255);
    expect(
      await invoke(page, 'load_document', { mediaId: latest.anchorId }),
    ).toEqual(anchor);
    await review.getByRole('button', { name: 'Close', exact: true }).click();
    await page
      .getByRole('button', { name: 'Dataset tools', exact: true })
      .click();
    tools = page.getByRole('dialog', { name: 'Dataset tools', exact: true });
    await tools
      .getByRole('button', { name: 'Dataset versions', exact: true })
      .click();
    await tools
      .getByLabel('Version name', { exact: true })
      .fill('Native M5 frozen dataset');
    await tools
      .getByRole('checkbox', { name: 'Approved images only', exact: true })
      .uncheck();
    await tools
      .getByRole('button', { name: 'Create immutable version', exact: true })
      .click();
    await expect(tools.locator('tbody tr')).toHaveCount(1);
    const first = (await invoke(page, 'list_snapshots'))[0];
    await tools
      .getByRole('button', { name: 'Create immutable version', exact: true })
      .click();
    await expect(tools.locator('tbody tr')).toHaveCount(1);
    await tools
      .getByRole('button', { name: 'Verify labels and sources', exact: true })
      .click();
    await expect(tools.getByLabel('Operation result')).toContainText(
      'sourceChecked',
    );
    const changed = {
      ...accepted,
      notes: 'Live labels edited after immutable snapshot',
    };
    await invoke(page, 'save_document', {
      document: changed,
      operationId: randomUUID(),
    });
    await tools
      .getByRole('button', { name: 'Create immutable version', exact: true })
      .click();
    await expect(tools.locator('tbody tr')).toHaveCount(2);
    const versions = await invoke(page, 'list_snapshots');
    expect(versions.every((v) => v.images === 4)).toBe(true);
    expect(new Set(versions.map((v) => v.id)).size).toBe(2);
    const check = await invoke(page, 'verify_snapshot', {
      id: first.id,
      checkSources: true,
    });
    expect(Object.keys(check.splitCounts)).toHaveLength(1);
    await expect(
      tools.getByRole('button', { name: 'Close', exact: true }),
    ).toBeEnabled();
    await page.screenshot({
      path: 'docs/milestones/screenshots/m5-snapshots-desktop.png',
    });
    return {
      key,
      info,
      anchor,
      target: await invoke(page, 'load_document', { mediaId: latest.targetId }),
      versions,
      hashes,
    };
  },
);
await withDesktop(['--project', latest.project], async (page) => {
  await expect(
    page.getByRole('heading', { name: 'M5 acceptance', exact: true }),
  ).toBeVisible();
  expect(
    await invoke(page, 'load_document', { mediaId: latest.anchorId }),
  ).toEqual(baseline.anchor);
  expect(
    await invoke(page, 'load_document', { mediaId: latest.targetId }),
  ).toEqual(baseline.target);
  expect(await invoke(page, 'list_snapshots')).toEqual(baseline.versions);
  expect(
    (await invoke(page, 'propagation_info', { key: baseline.key })).verifiedAt,
  ).toBeTruthy();
  for (const version of baseline.versions)
    expect(
      (
        await invoke(page, 'verify_snapshot', {
          id: version.id,
          checkSources: true,
        })
      ).images,
    ).toBe(4);
});
for (const [path, hash] of Object.entries(baseline.hashes))
  expect(await sha(path)).toBe(hash);
await writeFile(
  '.cache/m5-acceptance/desktop.json',
  JSON.stringify(
    {
      passed: true,
      restartExact: true,
      sourcesUnchanged: true,
      translation: [3, 2],
      verificationRequired: true,
      manualPixelPreserved: true,
      independentEventPreserved: true,
      snapshots: baseline.versions.map((v) => v.id),
    },
    null,
    2,
  ) + '\n',
);
console.log(
  'M5 native desktop: fingerprints, propagation verification/acceptance, independent GT, immutable versions and process restart passed.',
);
