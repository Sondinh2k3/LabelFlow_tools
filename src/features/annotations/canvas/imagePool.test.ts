import { expect, it, vi } from 'vitest';
import { DecodedImagePool } from './imagePool';
import type { ImagePreview } from '../../../lib/contracts';
const preview = (id: string, pixels = 100): ImagePreview => ({
  dataUrl: id,
  info: {
    id,
    name: id,
    path: id,
    width: pixels,
    height: 1,
    orientation: 1,
    sha256: id,
  },
});
const loader = () => vi.fn(async () => ({}) as HTMLImageElement);
it('shares an in-flight decode between main and review and reuses a prefetched neighbor', async () => {
  const load = loader(),
    pool = new DecodedImagePool(load, 1000, 2);
  const main = pool.acquire(preview('a')),
    review = pool.acquire(preview('a'));
  expect(main.ready).toBe(review.ready);
  await main.ready;
  await pool.prefetch(preview('b'));
  const next = pool.acquire(preview('b'));
  await next.ready;
  expect(load).toHaveBeenCalledTimes(2);
  main.release();
  review.release();
  next.release();
  expect(pool.bytes).toBe(800);
});
it('evicts idle images, honors the speculative byte budget and never drops active leases', async () => {
  const load = loader(),
    pool = new DecodedImagePool(load, 1000, 2);
  const main = pool.acquire(preview('a', 200));
  await main.ready;
  await pool.prefetch(preview('large', 100));
  expect(load).toHaveBeenCalledTimes(1);
  await pool.prefetch(preview('small', 40));
  expect(pool.bytes).toBe(960);
  const next = pool.acquire(preview('b', 200));
  await next.ready;
  expect(pool.size).toBe(2); // Both necessary images stay usable temporarily above budget.
  main.release();
  main.release();
  expect(pool.bytes).toBe(800);
  next.release();
  pool.clear();
  expect(pool.size).toBe(0);
});
it('failed decodes are retryable and changed URLs never inherit stale decoded content', async () => {
  const load = loader();
  load.mockRejectedValueOnce(Error('decode failed'));
  const pool = new DecodedImagePool(load),
    failed = pool.acquire(preview('a'));
  await expect(failed.ready).rejects.toThrow('decode failed');
  failed.release();
  const retry = pool.acquire(preview('a'));
  await retry.ready;
  const changed = pool.acquire({ ...preview('a'), dataUrl: 'new-token' });
  await changed.ready;
  expect(load).toHaveBeenCalledTimes(3);
  retry.release();
  changed.release();
});
