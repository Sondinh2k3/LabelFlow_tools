import { useEffect, useState } from 'react';
import { resources as text } from '../../app/resourceStrings';
import { command, normalizeError } from '../../lib/backend';
import type { StorageUsage } from '../../lib/contracts';
import { decodedImages } from '../annotations/canvas/imagePool';

const formatBytes = (bytes: number) =>
  bytes < 1024
    ? `${bytes} B`
    : bytes < 1024 * 1024
      ? `${(bytes / 1024).toFixed(1)} KiB`
      : `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
export function StoragePanel({
  busy,
  onTask,
}: {
  busy: boolean;
  onTask: (action: () => Promise<void>) => Promise<void>;
}) {
  const [usage, setUsage] = useState<StorageUsage | null>(null),
    [confirmed, setConfirmed] = useState(false),
    [error, setError] = useState('');
  useEffect(() => {
    let cancelled = false;
    void command('storage_usage', undefined)
      .then((value) => {
        if (!cancelled) setUsage(value);
      })
      .catch((cause) => {
        if (!cancelled) setError(normalizeError(cause).message);
      });
    return () => {
      cancelled = true;
    };
  }, []);
  const task = (clear: boolean) =>
    void onTask(async () => {
      setError('');
      try {
        const result = clear
          ? await command('clear_derived_cache', { confirmed })
          : await command('storage_usage', undefined);
        setUsage(result);
        if (clear) {
          decodedImages.clearIdle();
          setConfirmed(false);
        }
      } catch (cause) {
        setError(normalizeError(cause).message);
        throw cause;
      }
    });
  return (
    <section aria-label={text.title} className="dataset-section">
      <p className="muted">{text.help}</p>
      <p className="muted">{text.scope}</p>
      {error && <p role="alert">{error}</p>}
      {!usage ? (
        <p>{text.loading}</p>
      ) : (
        <>
          <table className="review-table">
            <thead>
              <tr>
                <th>{text.category}</th>
                <th>{text.size}</th>
                <th>{text.budget}</th>
              </tr>
            </thead>
            <tbody>
              {usage.categories.map((item) => (
                <tr key={item.name}>
                  <td>{text[item.name]}</td>
                  <td>{formatBytes(item.bytes)}</td>
                  <td>
                    {item.budget === null ? '—' : formatBytes(item.budget)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {usage.removedFiles > 0 && (
            <p role="status">
              {text.removed}: {formatBytes(usage.removedBytes)} (
              {usage.removedFiles})
            </p>
          )}
        </>
      )}
      <p>{text.preserved}</p>
      <label className="checkbox">
        <input
          type="checkbox"
          checked={confirmed}
          disabled={busy}
          onChange={(event) => setConfirmed(event.target.checked)}
        />
        {text.confirm}
      </label>
      <div className="dialog-actions">
        <button disabled={busy} onClick={() => task(false)}>
          {text.refresh}
        </button>
        <button
          disabled={busy || !confirmed || !usage?.removableBytes}
          onClick={() => task(true)}
        >
          {text.clear}
        </button>
      </div>
    </section>
  );
}
