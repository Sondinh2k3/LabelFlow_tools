import type { ImagePreview } from '../../../lib/contracts';

type Loader = (url: string) => Promise<HTMLImageElement>;
interface Entry {
  ready: Promise<HTMLImageElement>;
  bytes: number;
  users: number;
}
const load: Loader = (url) =>
  new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => {
      image.onload = null;
      image.onerror = null;
      resolve(image);
    };
    image.onerror = () => {
      image.onload = null;
      image.onerror = null;
      reject(Error('Cannot decode image'));
    };
    image.src = url;
  });

/** Shared by main/review canvases. Active images remain usable even above budget;
 * speculative images never push the estimated decoded RGBA budget above 32 MiB. */
export class DecodedImagePool {
  private entries = new Map<string, Entry>();
  private prefetching = false;
  constructor(
    private loader: Loader = load,
    private budget = 32 * 1024 * 1024,
    private limit = 2,
  ) {}
  private key(image: ImagePreview) {
    return `${image.info.id}:${image.info.sha256}:${image.dataUrl}`;
  }
  private trim() {
    for (const [key, entry] of this.entries) {
      if (this.bytes <= this.budget && this.entries.size <= this.limit) break;
      if (!entry.users) this.entries.delete(key);
    }
  }
  get bytes() {
    return [...this.entries.values()].reduce((sum, e) => sum + e.bytes, 0);
  }
  get size() {
    return this.entries.size;
  }
  acquire(image: ImagePreview) {
    const key = this.key(image);
    let entry = this.entries.get(key);
    if (entry) {
      this.entries.delete(key);
      this.entries.set(key, entry);
    } else {
      entry = {
        ready: this.loader(image.dataUrl),
        bytes: image.info.width * image.info.height * 4,
        users: 0,
      };
      this.entries.set(key, entry);
      const created = entry;
      void entry.ready.catch(() => {
        if (this.entries.get(key) === created) this.entries.delete(key);
      });
    }
    entry.users++;
    this.trim();
    let released = false;
    return {
      ready: entry.ready,
      release: () => {
        if (!released) {
          released = true;
          entry.users--;
          this.trim();
        }
      },
    };
  }
  async prefetch(image: ImagePreview) {
    if (this.prefetching) return;
    if (this.entries.has(this.key(image))) return;
    const cost = image.info.width * image.info.height * 4;
    const active = [...this.entries.values()].filter((e) => e.users);
    if (
      active.reduce((sum, e) => sum + e.bytes, 0) + cost > this.budget ||
      active.length >= this.limit
    )
      return;
    // Evict obsolete speculative decodes before starting another one.
    for (const [key, entry] of this.entries)
      if (!entry.users) this.entries.delete(key);
    const lease = this.acquire(image);
    this.prefetching = true;
    try {
      await lease.ready;
    } finally {
      lease.release();
      this.prefetching = false;
    }
  }
  clearIdle() {
    for (const [key, entry] of this.entries)
      if (!entry.users) this.entries.delete(key);
  }
  clear() {
    this.entries.clear();
  }
}
export const decodedImages = new DecodedImagePool();
