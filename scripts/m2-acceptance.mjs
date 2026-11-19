import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
const env = { ...process.env, CARGO_INCREMENTAL: '0' };
for (const [name, file] of [
  ['LABELFLOW_FFMPEG', 'ffmpeg'],
  ['LABELFLOW_FFPROBE', 'ffprobe'],
]) {
  const local = resolve(
    `.tools/ffmpeg/${file}${process.platform === 'win32' ? '.exe' : ''}`,
  );
  if (!env[name] && existsSync(local)) env[name] = local;
}
for (const [program, args] of [
  ['python', ['scripts/generate-video-fixtures.py']],
  [
    process.execPath,
    [
      'scripts/rust.mjs',
      'cargo',
      'test',
      '--locked',
      '--manifest-path',
      'src-tauri/Cargo.toml',
      '--test',
      'video_interoperability',
      'video_acceptance',
      '--',
      '--ignored',
      '--exact',
      '--nocapture',
    ],
  ],
  ['python', ['scripts/validate-m2-exports.py']],
]) {
  const result = spawnSync(program, args, {
    stdio: 'inherit',
    env,
    shell: false,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
