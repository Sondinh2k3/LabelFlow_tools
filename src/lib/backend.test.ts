import { beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { command, normalizeError } from './backend';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => vi.clearAllMocks());
it('passes exact command arguments and preserves native dialog cancellation', async () => {
  vi.mocked(invoke).mockResolvedValue(null);
  expect(await command('create_project', { name: 'Đường phố' })).toBeNull();
  expect(invoke).toHaveBeenCalledWith('create_project', { name: 'Đường phố' });
});
it('preserves structured backend errors', async () => {
  vi.mocked(invoke).mockRejectedValue({
    code: 'projectLocked',
    message: 'Already open',
  });
  await expect(command('open_project', undefined)).rejects.toEqual({
    code: 'projectLocked',
    message: 'Already open',
  });
});
it('normalizes unexpected failures without unsafe type assertions in UI', () => {
  expect(normalizeError(new Error('Unavailable'))).toEqual({
    code: 'internal',
    message: 'Unavailable',
  });
  expect(normalizeError({ code: 'unknown', message: 'x' }).code).toBe(
    'internal',
  );
});
