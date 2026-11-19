import { spawn } from 'node:child_process';
import { toolEnvironment } from './environment.mjs';

// Own Vite directly on Windows: avoid an intermediate cmd.exe tree during teardown.
let server;
const available = async () => {
  try {
    return (
      await fetch('http://127.0.0.1:1420', {
        signal: AbortSignal.timeout(1000),
      })
    ).ok;
  } catch {
    return false;
  }
};
try {
  if (!(await available())) {
    server = spawn(
      process.execPath,
      [
        'node_modules/vite/bin/vite.js',
        '--host',
        '127.0.0.1',
        '--port',
        '1420',
        '--strictPort',
      ],
      {
        env: toolEnvironment(process.env),
        windowsHide: true,
        stdio: 'inherit',
      },
    );
    let ready = false;
    for (let attempt = 0; attempt < 120; attempt++) {
      if (server.exitCode !== null)
        throw Error(`Vite exited: ${server.exitCode}`);
      if (await available()) {
        ready = true;
        break;
      }
      await new Promise((done) => setTimeout(done, 250));
    }
    if (!ready) throw Error('Vite did not become ready');
  }
  const test = spawn(
    process.execPath,
    ['node_modules/playwright/cli.js', 'test', ...process.argv.slice(2)],
    {
      env: {
        ...toolEnvironment(process.env),
        LABELFLOW_UI_SERVER: 'external',
      },
      windowsHide: true,
      stdio: 'inherit',
    },
  );
  process.exitCode = await new Promise((done, reject) => {
    test.once('error', reject);
    test.once('exit', (code) => done(code ?? 1));
  });
} finally {
  if (server && server.exitCode === null) {
    const exited = new Promise((done) => server.once('exit', done));
    server.kill();
    await exited;
  }
}
