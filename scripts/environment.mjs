import { dirname } from 'node:path';

export function toolEnvironment(
  source,
  {
    platform = process.platform,
    nodeExecutable = process.execPath,
    cargoBin,
    cargoHome,
    rustupHome,
  } = {},
) {
  const env = { ...source };
  const windows = platform === 'win32';
  const pathKey = windows
    ? Object.keys(source).find((key) => key.toUpperCase() === 'PATH')
    : 'PATH';
  const inheritedPath = source[pathKey] ?? '';

  function set(key, value) {
    if (windows) {
      // Node passes just one case variant to Windows child processes. Keep
      // exactly one key so Path/PATH cannot hide the inherited search path.
      for (const existing of Object.keys(env)) {
        if (existing.toUpperCase() === key.toUpperCase()) delete env[existing];
      }
    }
    env[key] = value;
  }

  const separator = windows ? ';' : ':';
  set(
    'PATH',
    [cargoBin, dirname(nodeExecutable), inheritedPath]
      .filter(Boolean)
      .join(separator),
  );
  if (cargoHome) set('CARGO_HOME', cargoHome);
  if (rustupHome) set('RUSTUP_HOME', rustupHome);
  return env;
}
