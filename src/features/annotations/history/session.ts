import type {
  AnnotationDocument,
  AppError,
  RasterMask,
} from '../../../lib/contracts';
import { decodeRuns, encodeRuns } from '../geometry/raster';
import { normalizeError } from '../../../lib/backend';
import type { OperationJournal } from './journal';
import {
  historyDelta,
  applyHistory,
  historyBytes,
  type HistoryDelta,
} from './delta';
export interface Pixels extends Omit<RasterMask, 'runs'> {
  pixels: Uint8Array;
}
export interface EditDocument extends Omit<AnnotationDocument, 'masks'> {
  masks: Pixels[];
}
export const decodeDocument = (doc: AnnotationDocument): EditDocument => ({
  ...doc,
  masks: doc.masks.map((m) => ({
    layerId: m.layerId,
    width: m.width,
    height: m.height,
    pixels: decodeRuns(m),
  })),
});
const encodedPixels = new WeakMap<Uint8Array, number[]>();
export const encodeDocument = (
  doc: EditDocument,
  revision: number,
): AnnotationDocument => ({
  ...doc,
  revision,
  masks: doc.masks.map(({ pixels, ...m }) => {
    let runs = encodedPixels.get(pixels);
    if (!runs) {
      runs = encodeRuns(pixels);
      encodedPixels.set(pixels, runs);
    }
    return { ...m, runs };
  }),
});
export type SaveStatus = 'saved' | 'saving' | 'failed';
export interface SessionSnapshot {
  document: EditDocument;
  status: SaveStatus;
  error: AppError | null;
  canUndo: boolean;
  canRedo: boolean;
  pending: number;
  historyBytes: number;
  undoCount: number;
}
type Save = (
  document: AnnotationDocument,
  operationId: string,
) => Promise<number>;
export class EditSession {
  private listeners = new Set<() => void>();
  private undoStack: HistoryDelta[] = [];
  private redoStack: HistoryDelta[] = [];
  private pending: {
    doc: EditDocument;
    id: string;
    wire: AnnotationDocument;
    journal: Promise<void>;
  }[] = [];
  private draining: Promise<void> | null = null;
  private revision: number;
  private snapshot: SessionSnapshot;
  constructor(
    document: AnnotationDocument,
    private save: Save,
    private journal: OperationJournal,
    private delay = (ms: number) =>
      new Promise<void>((resolve) => setTimeout(resolve, ms)),
  ) {
    this.revision = document.revision;
    this.snapshot = {
      document: decodeDocument(document),
      status: 'saved',
      error: null,
      canUndo: false,
      canRedo: false,
      pending: 0,
      historyBytes: 0,
      undoCount: 0,
    };
  }
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  getSnapshot = () => this.snapshot;
  private publish(
    document = this.snapshot.document,
    status: SaveStatus = this.pending.length ? 'saving' : 'saved',
    error: AppError | null = null,
  ) {
    this.snapshot = {
      document,
      status,
      error,
      canUndo: !!this.undoStack.length,
      canRedo: !!this.redoStack.length,
      pending: this.pending.length,
      historyBytes: historyBytes([...this.undoStack, ...this.redoStack]),
      undoCount: this.undoStack.length,
    };
    for (const listener of this.listeners) listener();
  }
  private bound() {
    while (
      this.undoStack.length &&
      (this.undoStack.length > 100 ||
        historyBytes([...this.undoStack, ...this.redoStack]) > 64 * 1024 * 1024)
    )
      this.undoStack.shift();
  }
  commit(next: EditDocument) {
    if (this.snapshot.status === 'failed')
      throw Error('Retry persistence before continuing edits');
    this.undoStack.push(historyDelta(this.snapshot.document, next));
    this.redoStack = [];
    this.bound();
    this.enqueue(next);
  }
  private enqueue(next: EditDocument) {
    const id = crypto.randomUUID(),
      wire = encodeDocument(next, this.revision + this.pending.length);
    const journal = this.journal.stage({ id, document: wire });
    void journal.catch(() => {});
    this.pending.push({ doc: next, id, wire, journal });
    this.publish(next, 'saving');
    void this.drain();
  }
  undo() {
    if (this.snapshot.status === 'failed') return;
    const previous = this.undoStack.pop();
    if (!previous) return;
    this.redoStack.push(previous);
    this.enqueue(applyHistory(this.snapshot.document, previous, 'before'));
  }
  redo() {
    if (this.snapshot.status === 'failed') return;
    const next = this.redoStack.pop();
    if (!next) return;
    this.undoStack.push(next);
    this.enqueue(applyHistory(this.snapshot.document, next, 'after'));
  }
  adopt(document: AnnotationDocument) {
    if (this.pending.length)
      throw Error('Cannot replace a document while persistence is pending');
    const decoded = decodeDocument(document);
    this.undoStack.push(historyDelta(this.snapshot.document, decoded));
    this.redoStack = [];
    this.bound();
    this.revision = document.revision;
    this.publish(decoded, 'saved');
  }
  retry() {
    if (this.snapshot.status === 'failed') {
      for (const task of this.pending) {
        task.journal = this.journal.stage({ id: task.id, document: task.wire });
        void task.journal.catch(() => {});
      }
      this.publish(this.snapshot.document, 'saving');
      void this.drain();
    }
  }
  private drain(): Promise<void> {
    if (this.draining) return this.draining;
    this.draining = this.run().finally(() => {
      this.draining = null;
      if (this.pending.length && this.snapshot.status !== 'failed')
        void this.drain();
    });
    return this.draining;
  }
  private async run() {
    while (this.pending.length) {
      const task = this.pending[0]!;
      let error: AppError | null = null;
      for (let attempt = 0; attempt < 3; attempt++) {
        try {
          await task.journal;
          const revision = await this.save(task.wire, task.id);
          await this.journal.remove(task.id);
          this.revision = revision;
          error = null;
          break;
        } catch (cause) {
          error = normalizeError(cause);
          if (
            error.code === 'conflict' ||
            error.code === 'invalidInput' ||
            attempt === 2
          )
            break;
          await this.delay(250 * 2 ** attempt);
        }
      }
      if (error) {
        this.publish(this.snapshot.document, 'failed', error);
        return;
      }
      this.pending.shift();
      this.publish(
        this.pending.at(-1)?.doc ?? { ...task.doc, revision: this.revision },
      );
    }
  }
  async flush() {
    if (this.snapshot.status === 'failed')
      throw this.snapshot.error ?? Error('Retry persistence before navigating');
    await this.drain();
    if (this.pending.length || this.getSnapshot().status === 'failed')
      throw this.snapshot.error ?? Error('Save has not completed');
  }
}
