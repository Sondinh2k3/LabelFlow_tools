import { useEffect, useRef, useState } from 'react';
import { command } from '../../lib/backend';
import type {
  LibraryFilter,
  MediaPage,
  MediaRecord,
} from '../../lib/contracts';
import { reviewStatuses } from '../../lib/contracts';
import { m2 } from '../../app/m2Strings';
import { m1 } from '../../app/m1Strings';

let thumbnailQueue: Promise<void> = Promise.resolve();
function Thumbnail({ record }: { record: MediaRecord }) {
  const [source, setSource] = useState<string | null>(null);
  const frame = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let cancelled = false;
    const element = frame.current;
    if (!element) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        observer.disconnect();
        const request = async () => {
          if (cancelled) return;
          try {
            const data = await command('media_thumbnail', { id: record.id });
            if (!cancelled) setSource(data);
          } catch {
            if (!cancelled) setSource(null);
          }
        };
        thumbnailQueue = thumbnailQueue.then(request, request);
      },
      { root: element.closest('.media-grid'), rootMargin: '80px' },
    );
    observer.observe(element);
    return () => {
      cancelled = true;
      observer.disconnect();
    };
  }, [record.id]);
  return (
    <div ref={frame} className="thumbnail-frame">
      {source ? (
        <img src={source} alt="" loading="lazy" />
      ) : (
        <span className="thumbnail-placeholder">◇</span>
      )}
    </div>
  );
}
export function MediaBrowser({
  offset,
  onOffset,
  selected,
  refresh,
  disabled,
  onSelect,
  onPage,
  onError,
  discussed,
}: {
  offset: number;
  onOffset: (offset: number) => void;
  selected: string | undefined;
  refresh: number;
  disabled: boolean;
  onSelect: (id: string) => void;
  onPage: (
    page: MediaPage & {
      offset: number;
      query: string;
      status: string;
      filter?: LibraryFilter;
    },
  ) => void;
  onError: (error: unknown) => void;
  /** Content SHA-256 → state of team discussions about that image. */
  discussed?: Map<string, string>;
}) {
  const [query, setQuery] = useState(''),
    [status, setStatus] = useState(''),
    [filter, setFilter] = useState<LibraryFilter>({
      event: '',
      group: '',
      annotated: null,
    }),
    [page, setPage] = useState<MediaPage>({ items: [], total: 0 });
  useEffect(() => {
    let cancelled = false;
    const timer = setTimeout(() => {
      void command('list_media', { offset, limit: 40, query, status, filter })
        .then((value) => {
          if (!cancelled) {
            setPage(value);
            onPage({ ...value, offset, query, status, filter });
          }
        })
        .catch(onError);
    }, 120);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [offset, query, status, filter, refresh, onPage, onError]);
  return (
    <>
      <input
        aria-label={m1.search}
        placeholder={m1.search}
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          onOffset(0);
        }}
      />
      <select
        aria-label={m1.filter}
        value={status}
        onChange={(e) => {
          setStatus(e.target.value);
          onOffset(0);
        }}
      >
        <option value="">{m1.all}</option>
        {reviewStatuses.map((s) => (
          <option key={s} value={s}>
            {s.replaceAll('_', ' ')}
          </option>
        ))}
      </select>
      <select
        aria-label={m2.eventFilter}
        value={filter.event}
        onChange={(e) => {
          setFilter({ ...filter, event: e.target.value });
          onOffset(0);
        }}
      >
        <option value="">
          {m2.eventFilter}: {m2.any}
        </option>
        {['positive', 'negative', 'uncertain', 'none'].map((v) => (
          <option key={v} value={v}>
            {v}
          </option>
        ))}
      </select>
      <select
        aria-label={m2.annotationFilter}
        value={filter.annotated === null ? '' : String(filter.annotated)}
        onChange={(e) => {
          setFilter({
            ...filter,
            annotated: e.target.value === '' ? null : e.target.value === 'true',
          });
          onOffset(0);
        }}
      >
        <option value="">
          {m2.annotationFilter}: {m2.any}
        </option>
        <option value="true">{m2.annotated}</option>
        <option value="false">{m2.empty}</option>
      </select>
      <input
        aria-label={m2.groupFilter}
        placeholder={m2.groupFilter}
        value={filter.group}
        onChange={(e) => {
          setFilter({ ...filter, group: e.target.value });
          onOffset(0);
        }}
      />
      <div className="library-count">
        {page.total} {m1.images.toLowerCase()}
      </div>
      <div className="media-grid">
        {page.items.map((record) => (
          <button
            className={`media-card ${record.id === selected ? 'active' : ''}`}
            key={record.id}
            disabled={disabled}
            onClick={() => onSelect(record.id)}
            title={record.name}
            aria-label={record.name}
          >
            <Thumbnail record={record} />
            {discussed?.has(record.sha256.toLowerCase()) && (
              <i
                className={`card-discussion ${discussed.get(record.sha256.toLowerCase())}`}
                aria-label="Team discussion"
              />
            )}
            <span>{record.name}</span>
            <small>
              {record.width} × {record.height} ·{' '}
              {record.reviewStatus.replaceAll('_', ' ')}
            </small>
          </button>
        ))}
      </div>
      {!page.items.length && <p className="muted">{m1.noMedia}</p>}
      <div className="pagination">
        <button
          aria-label={m1.previousPage}
          disabled={disabled || offset === 0}
          onClick={() => onOffset(Math.max(0, offset - 40))}
        >
          ←
        </button>
        <span>
          {page.total ? offset + 1 : 0}–{Math.min(offset + 40, page.total)} /{' '}
          {page.total}
        </span>
        <button
          aria-label={m1.nextPage}
          disabled={disabled || offset + 40 >= page.total}
          onClick={() => onOffset(offset + 40)}
        >
          →
        </button>
      </div>
    </>
  );
}
