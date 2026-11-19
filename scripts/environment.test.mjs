import { test } from 'node:test';
import assert from 'node:assert/strict';
import { dirname } from 'node:path';
import { spawnSync } from 'node:child_process';
import { toolEnvironment } from './environment.mjs';

for (const key of ['Path', 'PATH', 'path']) {
  test(`Windows preserves inherited ${key} and emits one PATH key`, () => {
    const source = { [key]: 'inherited-tools;system-tools', OTHER: 'keep' };
    const env = toolEnvironment(source, {
      platform: 'win32',
      cargoBin: 'local-cargo',
    });
    assert.equal(
      env.PATH,
      `local-cargo;${dirname(process.execPath)};inherited-tools;system-tools`,
    );
    assert.deepEqual(
      Object.keys(env).filter((name) => name.toUpperCase() === 'PATH'),
      ['PATH'],
    );
    assert.equal(env.OTHER, 'keep');
    assert.deepEqual(source, {
      [key]: 'inherited-tools;system-tools',
      OTHER: 'keep',
    });
  });
}

test('Windows local toolchain overrides remove case variants of Rust variables', () => {
  const env = toolEnvironment(
    { Path: 'keep', Cargo_Home: 'old', Rustup_Home: 'old' },
    {
      platform: 'win32',
      cargoHome: 'local-cargo-home',
      rustupHome: 'local-rustup-home',
    },
  );
  assert.equal(env.CARGO_HOME, 'local-cargo-home');
  assert.equal(env.RUSTUP_HOME, 'local-rustup-home');
  assert.equal(env.Cargo_Home, undefined);
  assert.equal(env.Rustup_Home, undefined);
});

test('Linux preserves case-sensitive variables and the system toolchain', () => {
  const env = toolEnvironment(
    { PATH: '/usr/bin:/bin', Path: 'unrelated', CARGO_HOME: '/custom/cargo' },
    { platform: 'linux' },
  );
  assert.equal(env.PATH, `${dirname(process.execPath)}:/usr/bin:/bin`);
  assert.equal(env.Path, 'unrelated');
  assert.equal(env.CARGO_HOME, '/custom/cargo');
});

test(
  'Windows child shell can find npm with Path casing and a local Rust toolchain',
  { skip: process.platform !== 'win32' },
  () => {
    const source = { ...process.env };
    for (const key of Object.keys(source)) {
      if (key.toUpperCase() === 'PATH') delete source[key];
    }
    source.Path = 'C:\\LabelFlow-unavailable-tool-directory';
    const env = toolEnvironment(source, {
      cargoBin: 'C:\\LabelFlow-local-cargo',
    });
    const result = spawnSync(
      process.env.ComSpec ?? 'C:\\Windows\\System32\\cmd.exe',
      ['/d', '/s', '/c', 'npm --version'],
      { env, encoding: 'utf8', windowsHide: true },
    );
    assert.equal(result.status, 0, result.stderr || result.error?.message);
    assert.match(result.stdout.trim(), /^\d+\.\d+\.\d+$/);
  },
);
