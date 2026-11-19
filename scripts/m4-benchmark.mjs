import { spawnSync } from 'node:child_process';
import { toolEnvironment } from './environment.mjs';
const env = {
  ...toolEnvironment(process.env),
  CARGO_INCREMENTAL: '0',
  LABELFLOW_BENCHMARK: '1',
};
for (const args of [
  [
    'scripts/rust.mjs',
    'cargo',
    'test',
    '--release',
    '--locked',
    '--manifest-path',
    'src-tauri/Cargo.toml',
    '--test',
    'road_review',
    'benchmark_m4_suggestion_listing',
    '--',
    '--ignored',
    '--nocapture',
  ],
  ['scripts/ui-tests.mjs', 'performance.spec.ts'],
]) {
  const result = spawnSync(process.execPath, args, {
    env,
    stdio: 'inherit',
    windowsHide: true,
  });
  if (result.error) console.error(result.error.message);
  if (result.status !== 0) process.exit(result.status ?? 1);
}
