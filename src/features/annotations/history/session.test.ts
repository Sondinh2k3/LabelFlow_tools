import { describe, it, expect, vi } from 'vitest';
import type { AnnotationDocument } from '../../../lib/contracts';
import { EditSession } from './session';
import type { OperationJournal, JournalEntry } from './journal';
const initial: AnnotationDocument = {
  mediaId: 'image',
  revision: 0,
  reviewStatus: 'unlabeled',
  eventLabel: null,
  notes: '',
  splitGroup: 'trip',
  annotations: [],
  masks: [],
};
class MemoryJournal implements OperationJournal {
  entries = new Map<string, Omit<JournalEntry, 'scope'>>();
  stage = vi.fn(async (entry: Omit<JournalEntry, 'scope'>) => {
    this.entries.set(entry.id, entry);
  });
  remove = vi.fn(async (id: string) => {
    this.entries.delete(id);
  });
}
describe('ordered durable command queue', () => {
  it('retains 100 sparse 1080p strokes within a small raster budget and undoes exact pixels', async () => {
    let revision = 0;
    const store = new EditSession(
      {
        ...initial,
        masks: [
          {
            layerId: 'semantic',
            width: 1920,
            height: 1080,
            runs: [0, 1920 * 1080],
          },
        ],
      },
      async () => ++revision,
      new MemoryJournal(),
    );
    for (let i = 0; i < 110; i++) {
      const doc = store.getSnapshot().document,
        mask = doc.masks[0]!,
        pixels = mask.pixels.slice();
      pixels[i] = 13;
      store.commit({ ...doc, masks: [{ ...mask, pixels }] });
      await store.flush();
    }
    expect(store.getSnapshot().undoCount).toBe(100);
    expect(store.getSnapshot().historyBytes).toBeLessThanOrEqual(100 * 4096);
    for (let i = 0; i < 100; i++) {
      store.undo();
      await store.flush();
    }
    const pixels = store.getSnapshot().document.masks[0]!.pixels;
    expect([...pixels.slice(0, 12)]).toEqual([
      ...Array<number>(10).fill(13),
      0,
      0,
    ]);
    expect(pixels.subarray(10).every((v) => v === 0)).toBe(true);
    expect(store.getSnapshot().canUndo).toBe(false);
    expect(store.getSnapshot().canRedo).toBe(true);
    expect(revision).toBe(210);
  });
  it('journals each edit before IPC and removes it only after acknowledgment', async () => {
    const journal = new MemoryJournal();
    const seen: number[] = [];
    let head = 0;
    const store = new EditSession(
      initial,
      async (doc, id) => {
        expect(journal.entries.has(id)).toBe(true);
        expect(doc.revision).toBe(head);
        seen.push(doc.revision);
        return ++head;
      },
      journal,
    );
    for (let i = 0; i < 12; i++)
      store.commit({ ...store.getSnapshot().document, notes: `edit ${i}` });
    await store.flush();
    expect(seen).toEqual(Array.from({ length: 12 }, (_, i) => i));
    expect(journal.entries.size).toBe(0);
    expect(store.getSnapshot().document.notes).toBe('edit 11');
    expect(store.getSnapshot().status).toBe('saved');
  });
  it('retries the same operation ID and base revision after transient failure', async () => {
    const journal = new MemoryJournal();
    const ids: string[] = [],
      revisions: number[] = [];
    let calls = 0;
    const store = new EditSession(
      initial,
      async (doc, id) => {
        ids.push(id);
        revisions.push(doc.revision);
        if (++calls < 3) throw { code: 'io', message: 'temporary' };
        return 1;
      },
      journal,
      async () => {},
    );
    store.commit({ ...store.getSnapshot().document, notes: 'durable' });
    await store.flush();
    expect(new Set(ids).size).toBe(1);
    expect(revisions).toEqual([0, 0, 0]);
    expect(journal.entries.size).toBe(0);
  });
  it('retains drafts in the journal and prevents navigation after failed save', async () => {
    const journal = new MemoryJournal();
    let fail = true;
    const store = new EditSession(
      initial,
      async () => {
        if (fail) throw { code: 'io', message: 'disk full' };
        return 1;
      },
      journal,
      async () => {},
    );
    store.commit({ ...store.getSnapshot().document, notes: 'retain me' });
    await expect(store.flush()).rejects.toMatchObject({ code: 'io' });
    expect(store.getSnapshot().status).toBe('failed');
    expect(journal.entries.size).toBe(1);
    fail = false;
    await expect(store.flush()).rejects.toMatchObject({ code: 'io' });
    expect(store.getSnapshot().document.notes).toBe('retain me');
    store.retry();
    await store.flush();
    expect(store.getSnapshot().status).toBe('saved');
    expect(store.getSnapshot().document.notes).toBe('retain me');
  });
  it('surfaces stale revisions without blind retries or overwrites', async () => {
    const save = vi.fn(async () => {
      throw { code: 'conflict', message: 'revision changed' };
    });
    const store = new EditSession(
      initial,
      save,
      new MemoryJournal(),
      async () => {},
    );
    store.commit({ ...store.getSnapshot().document, notes: 'a' });
    await expect(store.flush()).rejects.toMatchObject({ code: 'conflict' });
    expect(save).toHaveBeenCalledTimes(1);
  });
  it('undo/redo are new ordered revisions and preserve raster bytes', async () => {
    let head = 0;
    const store = new EditSession(
      initial,
      async () => ++head,
      new MemoryJournal(),
    );
    store.commit({
      ...store.getSnapshot().document,
      masks: [
        {
          layerId: 'cityscapes_semantic',
          width: 2,
          height: 2,
          pixels: Uint8Array.from([0, 13, 255, 18]),
        },
      ],
    });
    await store.flush();
    store.undo();
    await store.flush();
    expect(store.getSnapshot().document.masks).toEqual([]);
    store.redo();
    await store.flush();
    expect([...store.getSnapshot().document.masks[0]!.pixels]).toEqual([
      0, 13, 255, 18,
    ]);
    expect(head).toBe(3);
  });
  it('counts shared immutable rasters once so vector history retains its command budget', async () => {
    let revision = 0;
    const store = new EditSession(
      {
        ...initial,
        masks: [
          {
            layerId: 'semantic',
            width: 1024,
            height: 1024,
            runs: [255, 1024 * 1024],
          },
        ],
      },
      async () => ++revision,
      new MemoryJournal(),
    );
    for (let i = 0; i < 80; i++)
      store.commit({ ...store.getSnapshot().document, notes: `edit ${i}` });
    await store.flush();
    for (let i = 0; i < 80; i++) {
      expect(store.getSnapshot().canUndo).toBe(true);
      store.undo();
    }
    await store.flush();
    expect(store.getSnapshot().document.notes).toBe('');
    expect(store.getSnapshot().document.masks[0]!.pixels[0]).toBe(255);
    expect(revision).toBe(160);
  });
});
