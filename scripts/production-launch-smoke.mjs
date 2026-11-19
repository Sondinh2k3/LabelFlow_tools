import { spawn, execFileSync } from 'node:child_process';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { createConnection } from 'node:net';
import { resolve } from 'node:path';
import { setTimeout as pause } from 'node:timers/promises';

if (process.platform !== 'win32')
  throw Error('Production launch smoke requires Windows');
const executable = resolve(
  process.argv[2] ?? 'releases/LabelFlow/LabelFlow.exe',
);
const fixture = JSON.parse(
  await readFile('.cache/resource-benchmark/optimized-fixture.json', 'utf8'),
);
const portOpen = () =>
  new Promise((resolve) => {
    const socket = createConnection({ port: 9223, host: '127.0.0.1' });
    const done = (value) => {
      socket.destroy();
      resolve(value);
    };
    socket.once('connect', () => done(true));
    socket.once('error', () => done(false));
    socket.setTimeout(500, () => done(false));
  });
if (await portOpen())
  throw Error('CDP port already in use before production launch');
const profile = resolve(`.cache/production-launch-profile-${process.pid}`);
await mkdir(profile, { recursive: true });
const child = spawn(executable, ['--project', fixture.project], {
  windowsHide: true,
  stdio: 'ignore',
  env: {
    ...process.env,
    WEBVIEW2_USER_DATA_FOLDER: profile,
    LABELFLOW_CONFIG_DIR: resolve('.cache/production-launch-settings'),
  },
});
const exited = new Promise((done, reject) => {
  child.once('exit', done);
  child.once('error', reject);
});
try {
  await pause(3000);
  if (child.exitCode !== null || !Number.isInteger(child.pid))
    throw Error('Production application did not stay running');
  if (await portOpen())
    throw Error('Production unexpectedly exposes the smoke CDP port');
  const window = JSON.parse(
    execFileSync(
      'powershell.exe',
      [
        '-NoProfile',
        '-NonInteractive',
        '-Command',
        `$p=Get-Process -Id ${child.pid} -ErrorAction Stop; [pscustomobject]@{title=$p.MainWindowTitle;handle=$p.MainWindowHandle.ToInt64();closeRequested=$p.CloseMainWindow()} | ConvertTo-Json -Compress`,
      ],
      { encoding: 'utf8', windowsHide: true },
    ),
  );
  if (!window.handle || window.title !== 'LabelFlow' || !window.closeRequested)
    throw Error(
      'Production native window was not available for graceful close',
    );
  for (let attempt = 0; attempt < 80 && child.exitCode === null; attempt++)
    await pause(100);
  if (child.exitCode !== 0)
    throw Error(`Production did not exit cleanly: ${child.exitCode}`);
  const report = {
    passed: true,
    nativeWindow: true,
    gracefulExit: true,
    cdpExposed: false,
    executable,
    scope:
      'Production portable launch on the existing isolated resource fixture; functional window/exit/port check, not a full UI acceptance or startup timing benchmark.',
  };
  await writeFile(
    '.cache/resource-benchmark/production-launch.json',
    JSON.stringify(report, null, 2) + '\n',
  );
  console.log(JSON.stringify(report));
} finally {
  if (child.exitCode === null) child.kill();
  await exited;
}
