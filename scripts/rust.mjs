import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawn } from 'node:child_process';
import { toolEnvironment } from './environment.mjs';

// Prefer a workspace-local toolchain when available; never change the user's PATH.
const local = resolve('.tools/cargo/bin');
const hasLocalToolchain = existsSync(local);
const env = toolEnvironment(
  process.env,
  hasLocalToolchain
    ? {
        cargoBin: local,
        cargoHome: resolve('.tools/cargo'),
        rustupHome: resolve('.tools/rustup'),
      }
    : undefined,
);
const [tool, ...args] = process.argv.slice(2);
let executable;
if (tool === 'tauri') {
  executable = process.execPath;
  args.unshift(resolve('node_modules/@tauri-apps/cli/tauri.js'));
} else if (tool === 'cargo') {
  executable = hasLocalToolchain
    ? resolve(local, process.platform === 'win32' ? 'cargo.exe' : 'cargo')
    : 'cargo';
} else {
  throw new Error('Expected cargo or tauri');
}
const child = spawn(executable, args, { env, stdio: 'inherit', shell: false });
child.on('error', (error) => {
  console.error(error.message);
  process.exitCode = 1;
});
child.on('exit', (code) => {
  process.exitCode = code ?? 1;
});
