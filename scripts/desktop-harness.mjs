import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createServer } from 'node:net';
import { toolEnvironment } from './environment.mjs';
const profile = resolve(`.cache/desktop-smoke-profile-${process.pid}`);

export async function withDesktop(args, check, options = {}) {
  if (process.platform !== 'win32')
    throw Error('Desktop smoke requires Windows WebView2.');
  // WebView2 descendants may release the CDP socket shortly after the app exits.
  for (let attempt = 0; attempt < 80; attempt++) {
    const probe = createServer();
    try {
      await new Promise((done, reject) => {
        probe.once('error', reject);
        probe.listen(9223, '127.0.0.1', done);
      });
      await new Promise((done) => probe.close(done));
      break;
    } catch (error) {
      if (error.code !== 'EADDRINUSE' || attempt === 79) throw error;
      await new Promise((done) => setTimeout(done, 125));
    }
  }
  await mkdir(profile, { recursive: true });
  await mkdir('docs/milestones/screenshots', { recursive: true });
  const child = spawn(
    resolve(
      options.executable ??
        process.env.LABELFLOW_DESKTOP_EXE ??
        'src-tauri/target/debug/labelflow.exe',
    ),
    args,
    {
      windowsHide: true,
      stdio: ['ignore', 'inherit', 'inherit'],
      env: {
        ...toolEnvironment(process.env),
        WEBVIEW2_USER_DATA_FOLDER: profile,
        LABELFLOW_CONFIG_DIR: resolve('.cache/desktop-smoke-settings'),
      },
    },
  );
  const exited = new Promise((done) => child.once('exit', done));
  let browser;
  try {
    let endpoint;
    for (let attempt = 0; attempt < 120; attempt++) {
      if (child.exitCode !== null)
        throw Error(`Desktop exited: ${child.exitCode}`);
      try {
        endpoint = (
          await (
            await fetch('http://127.0.0.1:9223/json/version', {
              signal: AbortSignal.timeout(1000),
            })
          ).json()
        ).webSocketDebuggerUrl;
      } catch {
        /* Wait for WebView2 initialization. */
      }
      if (endpoint) break;
      await new Promise((done) => setTimeout(done, 250));
    }
    if (!endpoint)
      throw Error('Smoke-build WebView2 CDP endpoint did not start');
    browser = await chromium.connectOverCDP(endpoint);
    const context = browser.contexts()[0];
    const page = context.pages()[0] ?? (await context.waitForEvent('page'));
    const errors = [];
    page.on('pageerror', (error) => errors.push(error.message));
    page.setDefaultTimeout(20000);
    page.setDefaultNavigationTimeout(20000);
    if (page.url().startsWith('http://127.0.0.1:1420'))
      throw Error('Expected packaged frontend');
    let result;
    try {
      result = await check(page, { pid: child.pid });
    } catch (error) {
      console.error((await page.locator('body').innerText()).slice(0, 3500));
      await page.screenshot({ path: '.cache/desktop-smoke-failed.png' });
      throw error;
    }
    if (errors.length) throw Error(`Frontend errors: ${errors.join('; ')}`);
    return result;
  } finally {
    if (browser) await browser.close();
    if (child.exitCode === null) child.kill();
    await exited;
  }
}
