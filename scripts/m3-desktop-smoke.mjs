import { expect } from '@playwright/test';
import { readFile, writeFile, mkdir, rm } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { withDesktop } from './desktop-harness.mjs';

const latest = JSON.parse(
  await readFile('.cache/m3-acceptance/latest.json', 'utf8'),
);
const settings = resolve('.cache/desktop-smoke-settings/ai.json');
let previous;
try {
  previous = await readFile(settings);
} catch (error) {
  if (error.code !== 'ENOENT') throw error;
}
const python = resolve('.cache/ai-venv/Scripts/python.exe');
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
const hash = async (path) =>
  createHash('sha256')
    .update(await readFile(path))
    .digest('hex');
const unpack = (mask) => {
  const pixels = new Uint8Array(mask.width * mask.height);
  let at = 0;
  for (let i = 0; i < mask.runs.length; i += 2) {
    pixels.fill(mask.runs[i], at, at + mask.runs[i + 1]);
    at += mask.runs[i + 1];
  }
  expect(at).toBe(pixels.length);
  return pixels;
};
async function configure(name, executable = python) {
  await writeFile(
    settings,
    JSON.stringify({
      python: executable,
      manifest: resolve(`tests/fixtures/ai/${name}.json`),
    }),
  );
}

try {
  await mkdir(resolve('.cache/desktop-smoke-settings'), { recursive: true });
  await configure('semantic', resolve('.cache/missing-python.exe'));
  const baseline = await withDesktop(
    ['--project', latest.project],
    async (page) => {
      await expect(
        page.getByRole('heading', { name: 'M3 AI acceptance', exact: true }),
      ).toBeVisible();
      const records = await invoke(page, 'list_media', {
        offset: 0,
        limit: 40,
        query: '',
        status: '',
      });
      const record = records.items.find((r) => r.id === latest.mediaId);
      const before = await invoke(page, 'load_document', {
        mediaId: latest.mediaId,
      });
      await page
        .getByRole('button', { name: record.name, exact: true })
        .first()
        .click();
      await page
        .getByRole('button', { name: 'AI assistance', exact: true })
        .click();
      const dialog = page.getByRole('dialog', {
        name: 'AI assistance',
        exact: true,
      });
      await expect(dialog.getByText(/AI is off/)).toBeVisible();
      await expect(
        dialog.getByRole('button', {
          name: 'Generate suggestion',
          exact: true,
        }),
      ).toBeDisabled();
      await expect(invoke(page, 'ai_start')).rejects.toThrow();
      expect(
        await invoke(page, 'load_document', { mediaId: latest.mediaId }),
      ).toEqual(before);
      await configure('semantic');
      await dialog
        .getByRole('button', { name: 'Start / restart worker', exact: true })
        .click();
      await expect(
        dialog.getByRole('button', {
          name: 'Generate suggestion',
          exact: true,
        }),
      ).toBeEnabled();
      await dialog
        .getByRole('button', { name: 'Generate suggestion', exact: true })
        .click();
      await expect(
        dialog.getByRole('button', { name: 'Accept suggestion', exact: true }),
      ).toBeEnabled();
      const pending = await invoke(page, 'ai_suggestions', {
        mediaId: latest.mediaId,
      });
      expect(pending).toHaveLength(1);
      const raw = await invoke(page, 'ai_preview', { id: pending[0].id });
      expect(raw.prediction.processing.provider).toBe('CPUExecutionProvider');
      expect([raw.prediction.width, raw.prediction.height]).toEqual([
        1920, 1080,
      ]);
      expect(
        await invoke(page, 'load_document', { mediaId: latest.mediaId }),
      ).toEqual(before);
      await page.screenshot({
        path: 'docs/milestones/screenshots/m3-suggestion-desktop.png',
      });
      await dialog
        .getByRole('button', { name: 'Accept suggestion', exact: true })
        .click();
      await expect(dialog.getByText('accepted', { exact: true })).toBeVisible();
      const accepted = await invoke(page, 'load_document', {
        mediaId: latest.mediaId,
      });
      expect(accepted.annotations).toEqual(before.annotations);
      const pixels = unpack(accepted.masks[0]);
      const proposed = unpack(raw.masks[0]);
      proposed[0] = 13;
      expect(Buffer.from(pixels).equals(Buffer.from(proposed))).toBe(true);
      expect(accepted.reviewStatus).toBe('needs_review');
      await invoke(page, 'ai_stop');
      await configure('boxes');
      await dialog
        .getByRole('button', { name: 'Start / restart worker', exact: true })
        .click();
      await dialog
        .getByRole('button', { name: 'Generate suggestion', exact: true })
        .click();
      await dialog
        .getByRole('button', { name: 'Reject suggestion', exact: true })
        .click();
      await expect(dialog.getByText('rejected', { exact: true })).toBeVisible();
      expect(
        await invoke(page, 'load_document', { mediaId: latest.mediaId }),
      ).toEqual(accepted);
      // A document edit after prediction must make approval fail without changing that edit.
      const stale = await invoke(page, 'ai_predict', {
        mediaId: latest.mediaId,
        threshold: 0.5,
      });
      const changed = structuredClone(accepted);
      changed.notes += ' · edited after inference';
      changed.revision = await invoke(page, 'save_document', {
        document: changed,
        operationId: crypto.randomUUID(),
      });
      await expect(
        invoke(page, 'ai_approve', {
          id: stale,
          expectedRevision: changed.revision,
        }),
      ).rejects.toThrow(/changed since/);
      expect(
        await invoke(page, 'load_document', { mediaId: latest.mediaId }),
      ).toEqual(changed);
      await invoke(page, 'ai_reject', { id: stale });
      // Native manual-save remains available after a worker load failure.
      await configure('bad-pidnet');
      await expect(invoke(page, 'ai_start')).rejects.toThrow(/Logit shape/);
      expect((await invoke(page, 'ai_status')).runtime.running).toBe(false);
      expect(
        await invoke(page, 'load_document', { mediaId: latest.mediaId }),
      ).toEqual(changed);
      await dialog.getByRole('button', { name: 'Close', exact: true }).click();
      await page.screenshot({
        path: 'docs/milestones/screenshots/m3-reviewed-desktop.png',
      });
      expect(await hash(record.path)).toBe(latest.sha256);
      return {
        document: changed,
        suggestions: await invoke(page, 'ai_suggestions', {
          mediaId: latest.mediaId,
        }),
        sourcePath: record.path,
      };
    },
  );
  await withDesktop(['--project', latest.project], async (page) => {
    await expect(
      page.getByRole('heading', { name: 'M3 AI acceptance', exact: true }),
    ).toBeVisible();
    expect((await invoke(page, 'ai_status')).runtime.running).toBe(false);
    expect(
      await invoke(page, 'load_document', { mediaId: latest.mediaId }),
    ).toEqual(baseline.document);
    expect(
      await invoke(page, 'ai_suggestions', { mediaId: latest.mediaId }),
    ).toEqual(baseline.suggestions);
    expect(await hash(baseline.sourcePath)).toBe(latest.sha256);
  });
  await writeFile(
    '.cache/m3-acceptance/desktop.json',
    JSON.stringify(
      {
        passed: true,
        sourceUnchanged: true,
        canonicalPixels: 1920 * 1080,
        pendingIsolation: true,
        manualPixelsPreserved: true,
        restartExact: true,
        suggestions: baseline.suggestions.length,
      },
      null,
      2,
    ) + '\n',
  );
  console.log(
    'M3 native desktop: optional runtime, real CPU inference, overlay, accept/reject, stale protection, original SHA, exact restart passed.',
  );
} finally {
  if (previous) await writeFile(settings, previous);
  else await rm(settings, { force: true });
}
