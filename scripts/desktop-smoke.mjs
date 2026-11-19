import { expect } from '@playwright/test';
import { withDesktop } from './desktop-harness.mjs';
await withDesktop(['--home'], async (page) => {
  await expect(
    page.getByRole('heading', { name: 'A workspace for your vision.' }),
  ).toBeVisible();
  if (page.url().startsWith('http://127.0.0.1:1420'))
    throw Error('Expected packaged frontend, found Vite dev server.');
  const result = await page.evaluate(async () => {
    const bridge = window.__TAURI_INTERNALS__;
    const closed = await bridge.invoke('close_project');
    // Fast-labeling commands are allowlisted: status works without a project,
    // segmenting needs one.
    const sam = await bridge.invoke('sam_status');
    let samError = null;
    try {
      await bridge.invoke('sam_segment', {
        mediaId: 'x',
        points: [],
        region: null,
        encodeOnly: true,
      });
    } catch (error) {
      samError = error;
    }
    try {
      await bridge.invoke('restore_image');
      throw Error('Expected noProject');
    } catch (error) {
      return { closed, error, sam, samError };
    }
  });
  expect(result.closed).toBeNull();
  expect(result.error.code).toBe('noProject');
  expect(result.sam).toHaveProperty('model');
  expect(result.samError?.code).toBe('noProject');
  await page.screenshot({
    path: 'docs/milestones/screenshots/m1-desktop-home.png',
  });
  console.log(
    'PASS: packaged desktop startup, embedded assets, real allowlisted IPC, typed noProject error, screenshot.',
  );
  console.log('Native file picker interaction is a separate manual check.');
});
