import { invoke } from '@tauri-apps/api/core';
import type { AppError, CommandMap, ErrorCode } from './contracts';

const codes: readonly ErrorCode[] = [
  'io',
  'database',
  'invalidProject',
  'projectLocked',
  'unsupportedVersion',
  'invalidInput',
  'invalidImage',
  'noProject',
  'internal',
  'conflict',
  'cancelled',
];
export function normalizeError(error: unknown): AppError {
  if (
    typeof error === 'object' &&
    error !== null &&
    'code' in error &&
    'message' in error &&
    codes.includes(error.code as ErrorCode) &&
    typeof error.message === 'string'
  ) {
    return { code: error.code as ErrorCode, message: error.message };
  }
  return {
    code: 'internal',
    message: error instanceof Error ? error.message : String(error),
  };
}
export async function command<K extends keyof CommandMap>(
  name: K,
  args: CommandMap[K]['args'],
): Promise<CommandMap[K]['result']> {
  try {
    return await invoke<CommandMap[K]['result']>(name, args);
  } catch (error) {
    throw normalizeError(error);
  }
}
