import type { AnnotationDocument } from '../../../lib/contracts';
import { command } from '../../../lib/backend';
export interface JournalEntry {
  id: string;
  scope: string;
  document: AnnotationDocument;
}
export interface OperationJournal {
  stage(entry: Omit<JournalEntry, 'scope'>): Promise<void>;
  remove(id: string): Promise<void>;
}
let database: Promise<IDBDatabase> | undefined;
function open() {
  return (database ??= new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open('labelflow-command-journal', 1);
    request.onupgradeneeded = () => {
      const store = request.result.createObjectStore('operations', {
        keyPath: 'id',
      });
      store.createIndex('scope', 'scope');
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => {
      database = undefined;
      reject(request.error);
    };
  }));
}
export class DurableJournal implements OperationJournal {
  constructor(private scope: string) {}
  private async write(action: (store: IDBObjectStore) => void) {
    const db = await open();
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction('operations', 'readwrite', {
        durability: 'strict',
      });
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error ?? Error('Journal write failed'));
      tx.onabort = () => reject(tx.error ?? Error('Journal aborted'));
      action(tx.objectStore('operations'));
    });
  }
  stage(entry: Omit<JournalEntry, 'scope'>) {
    return this.write((store) => {
      store.put({ ...entry, scope: this.scope });
    });
  }
  remove(id: string) {
    return this.write((store) => {
      store.delete(id);
    });
  }
  async recover(): Promise<number> {
    const db = await open();
    const entries = await new Promise<JournalEntry[]>((resolve, reject) => {
      const tx = db.transaction('operations', 'readonly');
      const request = tx
        .objectStore('operations')
        .index('scope')
        .getAll(this.scope);
      request.onsuccess = () => resolve(request.result as JournalEntry[]);
      request.onerror = () => reject(request.error);
    });
    entries.sort(
      (a, b) =>
        a.document.mediaId.localeCompare(b.document.mediaId) ||
        a.document.revision - b.document.revision,
    );
    for (const entry of entries) {
      await command('save_document', {
        document: entry.document,
        operationId: entry.id,
      });
      await this.remove(entry.id);
    }
    return entries.length;
  }
}
