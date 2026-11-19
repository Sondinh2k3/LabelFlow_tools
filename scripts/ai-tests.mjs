import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { toolEnvironment } from './environment.mjs';
const local = resolve(
  '.cache/ai-venv',
  process.platform === 'win32' ? 'Scripts/python.exe' : 'bin/python',
);
const candidate =
  process.env.LABELFLOW_TEST_PYTHON ?? (existsSync(local) ? local : 'python');
const discovery = spawnSync(
  candidate,
  ['-I', '-c', 'import sys; print(sys.executable)'],
  { encoding: 'utf8', windowsHide: true },
);
if (discovery.status !== 0) {
  console.error(
    'Choose an installed Python environment with LABELFLOW_TEST_PYTHON and install worker/requirements.txt.',
  );
  process.exit(1);
}
const python = discovery.stdout.trim();
const env = {
  ...toolEnvironment(process.env),
  LABELFLOW_TEST_PYTHON: python,
  CARGO_INCREMENTAL: '0',
};
function run(executable, args) {
  const result = spawnSync(executable, args, {
    env,
    stdio: 'inherit',
    windowsHide: true,
  });
  if (result.error) console.error(result.error.message);
  if (result.status !== 0) process.exit(result.status ?? 1);
}
run(python, [
  '-I',
  '-c',
  'import numpy, PIL, onnxruntime; print("ONNX Runtime", onnxruntime.__version__)',
]);
run(python, ['-I', '-m', 'unittest', 'discover', '-s', 'tests/worker', '-v']);
run(process.execPath, [
  'scripts/rust.mjs',
  'cargo',
  'test',
  '--locked',
  '--manifest-path',
  'src-tauri/Cargo.toml',
  '--test',
  'ai_assistance',
  'real_worker_cpu_inference_cancel_crash_limits_and_restart',
  '--',
  '--ignored',
  '--nocapture',
]);
run(process.execPath, [
  'scripts/rust.mjs',
  'cargo',
  'test',
  '--locked',
  '--manifest-path',
  'src-tauri/Cargo.toml',
  '--test',
  'fast_labeling',
  'real_sam_worker_masks_clicks_at_canonical_resolution',
  '--',
  '--ignored',
  '--nocapture',
]);
