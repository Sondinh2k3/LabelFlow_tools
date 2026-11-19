import { spawnSync } from 'node:child_process';
import { toolEnvironment } from './environment.mjs';
const result = spawnSync(
  process.execPath,
  ['scripts/ui-tests.mjs', 'm5-performance.spec.ts'],
  {
    env: { ...toolEnvironment(process.env), LABELFLOW_M5_BENCHMARK: '1' },
    stdio: 'inherit',
    windowsHide: true,
  },
);
if (result.error) console.error(result.error.message);
process.exit(result.status ?? 1);
