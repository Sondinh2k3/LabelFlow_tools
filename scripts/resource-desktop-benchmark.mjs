import { expect } from '@playwright/test';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { performance } from 'node:perf_hooks';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { withDesktop } from './desktop-harness.mjs';
const label = process.argv[2] ?? 'optimized';
if (!['baseline', 'optimized'].includes(label))
  throw Error('Expected baseline or optimized');
const executable =
  label === 'baseline'
    ? '.cache/resource-baseline/labelflow-smoke.exe'
    : 'src-tauri/target/release/labelflow.exe';
const fixture = JSON.parse(
  await readFile('.cache/resource-benchmark/latest.json', 'utf8'),
);
const sourceHashes = async () =>
  Promise.all(
    Array.from({ length: 4 }, async (_, i) =>
      createHash('sha256')
        .update(await readFile(join(fixture.project, `frame-${i}.png`)))
        .digest('hex'),
    ),
  );
const originalHashes = await sourceHashes();
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
const stats = (values) => {
  values.sort((a, b) => a - b);
  return {
    p50Ms: values[Math.floor(values.length * 0.5)],
    p95Ms: values[Math.floor(values.length * 0.95)],
    samples: values.length,
  };
};
function processMemory(pid) {
  if (!Number.isInteger(pid) || pid <= 0) throw Error('Invalid benchmark PID');
  const script = `$all = Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId; $ids = [System.Collections.Generic.HashSet[int]]::new(); [void]$ids.Add(${pid}); do { $added = 0; foreach ($item in $all) { if ($ids.Contains([int]$item.ParentProcessId) -and $ids.Add([int]$item.ProcessId)) { $added++ } } } while ($added -gt 0); $rows = foreach ($n in $ids) { $p = Get-Process -Id $n -ErrorAction SilentlyContinue; if ($p) { $detail = (Get-CimInstance Win32_Process -Filter ("ProcessId = " + $n)).CommandLine; $kind = if ($detail -match "--type=([a-z-]+)") { $Matches[1] } else { "main" }; [pscustomobject]@{type=$kind;name=$p.ProcessName;pid=$p.Id;workingSet=$p.WorkingSet64;privateBytes=$p.PrivateMemorySize64;cpuSeconds=$p.TotalProcessorTime.TotalSeconds} } }; ConvertTo-Json -InputObject @($rows) -Compress`;
  return JSON.parse(
    execFileSync(
      'powershell.exe',
      ['-NoProfile', '-NonInteractive', '-Command', script],
      { encoding: 'utf8', windowsHide: true },
    ),
  );
}
const start = performance.now();
const result = await withDesktop(
  ['--project', fixture.project],
  async (page, { pid }) => {
    await expect(
      page.getByRole('heading', { name: 'Resource benchmark', exact: true }),
    ).toBeVisible();
    const shellVisibleMs = performance.now() - start;
    await page
      .getByRole('button', { name: 'frame-0.png', exact: true })
      .first()
      .click();
    await expect(page.locator('.editor-status .save-saved')).toBeVisible();
    await expect(
      page.getByRole('button', { name: 'Zoom in', exact: true }),
    ).toBeEnabled();
    const firstImageReadyMs = performance.now() - start;
    const doc = await invoke(page, 'load_document', {
      mediaId: fixture.ids[0],
    });
    expect(doc.annotations).toHaveLength(20);
    expect(doc.masks).toHaveLength(2);
    const cdp = await page.context().newCDPSession(page);
    await page.waitForTimeout(1500);
    await cdp.send('HeapProfiler.collectGarbage');
    const idleHeap = await cdp.send('Runtime.getHeapUsage');
    const idleMemory = processMemory(pid);
    const idleAt = performance.now();
    await page.waitForTimeout(2000); // Deliberate quiet interval to sample idle process CPU.
    const idleEnd = processMemory(pid);
    const idleSeconds = (performance.now() - idleAt) / 1000;
    const idleCpuOneCorePercent =
      (100 *
        (idleEnd.reduce((sum, r) => sum + r.cpuSeconds, 0) -
          idleMemory.reduce((sum, r) => sum + r.cpuSeconds, 0))) /
      idleSeconds;
    const navigation = [];
    for (let i = 0; i < 14; i++) {
      const id = fixture.ids[i % 4];
      await invoke(page, 'preload_media', { id });
      const at = performance.now();
      await invoke(page, 'select_media', { id });
      await invoke(page, 'load_document', { mediaId: id });
      if (i >= 4) navigation.push(performance.now() - at);
    }
    // IPC probes above do not install a new editor session; UI remains on frame 0.
    const selection = [];
    const selectImage = async (index) => {
      const name = `frame-${index}.png`;
      await page.getByRole('button', { name, exact: true }).first().click();
      await expect(page.locator('.canvas-host canvas')).toHaveAttribute(
        'aria-label',
        name,
      );
      await expect(
        page.getByRole('button', { name: 'Zoom in', exact: true }),
      ).toBeEnabled();
      await expect(page.locator('.editor-status .save-saved')).toBeVisible();
      await page.evaluate(
        () =>
          new Promise((resolve) =>
            requestAnimationFrame(() => requestAnimationFrame(resolve)),
          ),
      );
    };
    for (let i = 0; i < 12; i++) {
      const at = performance.now();
      await selectImage(i % 4);
      if (i >= 2) selection.push(performance.now() - at);
    }
    await selectImage(0);
    await page
      .getByLabel('Active layer', { exact: true })
      .selectOption('cityscapes_semantic');
    await page.getByRole('button', { name: 'Brush', exact: true }).click();
    const box = await page.getByTestId('canvas-host').boundingBox();
    if (!box) throw Error('Canvas missing');
    const zoom =
      Math.min(box.width / fixture.width, box.height / fixture.height) * 0.9;
    const point = (x, y) => ({
      x: box.x + (box.width - fixture.width * zoom) / 2 + x * zoom,
      y: box.y + (box.height - fixture.height * zoom) / 2 + y * zoom,
    });
    const strokes = [];
    let revision = doc.revision;
    for (let i = 0; i < 60; i++) {
      const a = point(500 + (i % 10) * 60, 380 + Math.floor(i / 10) * 45),
        b = point(530 + (i % 10) * 60, 390 + Math.floor(i / 10) * 45);
      await page.mouse.move(a.x, a.y);
      const at = performance.now();
      await page.mouse.down();
      await page.mouse.move(b.x, b.y, { steps: 2 });
      await page.mouse.up();
      revision++;
      await expect(page.locator('.editor-status')).toContainText(
        `Revision ${revision}`,
      );
      await expect(page.locator('.editor-status .save-saved')).toBeVisible();
      if (i >= 10) strokes.push(performance.now() - at);
    }
    await cdp.send('HeapProfiler.collectGarbage');
    await page.waitForTimeout(2000);
    await cdp.send('HeapProfiler.collectGarbage');
    const afterHeap = await cdp.send('Runtime.getHeapUsage');
    const afterMemory = processMemory(pid);
    const restored = await invoke(page, 'load_document', {
      mediaId: fixture.ids[0],
    });
    expect(restored.revision).toBe(revision);
    expect(restored.annotations).toEqual(doc.annotations);
    expect(restored.eventLabel).toBe(doc.eventLabel);
    let correctness = { sourceUnchanged: true };
    if (label === 'optimized') {
      await page.keyboard.press('Control+z');
      await expect(page.locator('.editor-status')).toContainText(
        `Revision ${revision + 1}`,
      );
      await expect(page.locator('.editor-status .save-saved')).toBeVisible();
      await page.keyboard.press('Control+Shift+z');
      await expect(page.locator('.editor-status')).toContainText(
        `Revision ${revision + 2}`,
      );
      await expect(page.locator('.editor-status .save-saved')).toBeVisible();
      const redone = await invoke(page, 'load_document', {
        mediaId: fixture.ids[0],
      });
      expect(redone.masks).toEqual(restored.masks);
      expect(redone.annotations).toEqual(restored.annotations);
      await page
        .getByRole('button', { name: 'Dataset tools', exact: true })
        .click();
      await page
        .getByRole('button', { name: 'Storage & performance', exact: true })
        .click();
      const panel = page.getByRole('region', { name: 'Storage & performance' });
      await expect(
        panel.getByText('Library thumbnails', { exact: true }),
      ).toBeVisible();
      await expect(
        panel.getByRole('button', { name: 'Clear derived cache', exact: true }),
      ).toBeDisabled();
      await panel
        .getByLabel('Rebuild thumbnails and previews when needed', {
          exact: true,
        })
        .check();
      await panel
        .getByRole('button', { name: 'Clear derived cache', exact: true })
        .click();
      await expect(panel.getByRole('status')).toContainText('Cache removed');
      const usage = await invoke(page, 'storage_usage');
      expect(usage.removableBytes).toBe(0);
      expect(
        await invoke(page, 'load_document', { mediaId: fixture.ids[0] }),
      ).toEqual(redone);
      await page.screenshot({
        path: 'docs/milestones/screenshots/resource-storage.png',
      });
      correctness = {
        ...correctness,
        nativeUndoRedoExact: true,
        cacheCleanupPreservesDocument: true,
      };
    }
    const summarize = (rows) => ({
      processes: rows.length,
      workingSetBytes: rows.reduce((a, r) => a + r.workingSet, 0),
      privateBytes: rows.reduce((a, r) => a + r.privateBytes, 0),
      rows,
    });
    return {
      shellVisibleMs,
      firstImageReadyMs,
      idleCpuOneCorePercent,
      idleSeconds,
      navigation: stats(navigation),
      selectionToCanvas: stats(selection),
      strokeToSaved: stats(strokes),
      idleHeap,
      afterHeap,
      idleMemory: summarize(idleMemory),
      afterMemory: summarize(afterMemory),
      measuredRevision: revision,
      correctness,
    };
  },
  { executable },
);
expect(await sourceHashes()).toEqual(originalHashes);
result.sourceUnchanged = true;
await mkdir('.cache/resource-benchmark', { recursive: true });
result.label = label;
result.executableBytes = (await readFile(executable)).length;
result.executableSha256 = createHash('sha256')
  .update(await readFile(executable))
  .digest('hex');
result.scope =
  'Native release Tauri + 1080p, 20 shapes, two masks. Gesture includes Playwright delivery and saved-DOM observation. UI selection includes two animation frames after bitmap readiness; navigation is separate preloaded IPC selection/document retrieval. Memory after explicit GC; summed process working sets double-count shared pages. Startup and idle CPU are single functional samples, not cold-start p95 or sustained power measurements.';
await writeFile(
  `.cache/resource-benchmark/${label}.json`,
  JSON.stringify(result, null, 2) + '\n',
);
console.log(
  JSON.stringify({
    label,
    navigation: result.navigation,
    strokeToSaved: result.strokeToSaved,
    privateBytes: result.afterMemory.privateBytes,
    heap: result.afterHeap,
  }),
);
