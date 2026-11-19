import { useModalFocus } from '../../components/useModalFocus';
import { useState } from 'react';
import { m1 } from '../../app/m1Strings';
import { command, normalizeError } from '../../lib/backend';
import { m2 } from '../../app/m2Strings';
import type { QualityReport, ExportOptions } from '../../lib/contracts';
export function ExportDialog({
  onClose,
  onTask,
  onExport,
}: {
  onClose: () => void;
  onTask: (action: () => Promise<void>) => Promise<void>;
  onExport: (options: ExportOptions) => void;
}) {
  const modal = useModalFocus();
  const [options, setOptions] = useState<ExportOptions>({
    format: 'all',
    approvedOnly: false,
    seed: 'labelflow-v1',
    trainPercent: 80,
    valPercent: 10,
  });
  const [quality, setQuality] = useState<QualityReport | null>(null);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState('');
  async function validate(deep: boolean) {
    setBusy(true);
    setError('');
    try {
      await onTask(async () => {
        try {
          setQuality(await command('dataset_quality', { options, deep }));
        } catch (cause) {
          setError(normalizeError(cause).message);
        }
      });
    } catch (cause) {
      setError(normalizeError(cause).message);
    } finally {
      setBusy(false);
    }
  }
  const changeOptions = (next: ExportOptions) => {
    setOptions(next);
    setQuality(null);
  };
  return (
    <div className="modal-backdrop">
      <section
        ref={modal}
        role="dialog"
        aria-modal="true"
        aria-label={m1.export}
        className="dialog"
      >
        <h2>{m1.export}</h2>
        <p>{m1.exportHelp}</p>
        <label>
          {m1.exportFormat}
          <select
            disabled={busy}
            value={options.format}
            onChange={(e) =>
              changeOptions({
                ...options,
                format: e.target.value as ExportOptions['format'],
              })
            }
          >
            <option value="yolo">{m1.yolo}</option>
            <option value="semantic">{m1.semantic}</option>
            <option value="all">{m1.both}</option>
            <option value="yoloSeg">{m2.yoloSeg}</option>
            <option value="coco">{m2.coco}</option>
          </select>
        </label>
        <label className="checkbox">
          <input
            type="checkbox"
            disabled={busy}
            checked={options.approvedOnly}
            onChange={(e) =>
              changeOptions({ ...options, approvedOnly: e.target.checked })
            }
          />
          {m1.approved}
        </label>
        <label>
          {m1.seed}
          <input
            disabled={busy}
            value={options.seed}
            maxLength={200}
            onChange={(e) =>
              changeOptions({ ...options, seed: e.target.value })
            }
          />
        </label>
        <div className="field-row">
          <label>
            {m1.train}
            <input
              disabled={busy}
              type="number"
              min={0}
              max={100}
              value={options.trainPercent}
              onChange={(e) =>
                changeOptions({
                  ...options,
                  trainPercent: Number(e.target.value),
                })
              }
            />
          </label>
          <label>
            {m1.val}
            <input
              disabled={busy}
              type="number"
              min={0}
              max={100}
              value={options.valPercent}
              onChange={(e) =>
                changeOptions({
                  ...options,
                  valPercent: Number(e.target.value),
                })
              }
            />
          </label>
        </div>
        <p>{m2.groupHelp}</p>
        {options.format === 'yoloSeg' && (
          <label className="checkbox">
            <input
              type="checkbox"
              checked={options.skipUnsupported ?? false}
              onChange={(e) =>
                changeOptions({ ...options, skipUnsupported: e.target.checked })
              }
            />
            {m2.skipUnsupported}
          </label>
        )}
        <div className="field-row">
          <button disabled={busy} onClick={() => void validate(false)}>
            {m2.preview}
          </button>
          <button disabled={busy} onClick={() => void validate(true)}>
            {m2.validate}
          </button>
        </div>
        {busy && (
          <button onClick={() => void command('cancel_job', undefined)}>
            {m1.cancel}
          </button>
        )}
        {error && <p role="alert">{error}</p>}
        {quality && (
          <div className="quality-report">
            <p>
              {m2.images}: {quality.images} · {m2.groups}:{' '}
              {quality.groups.length}
            </p>
            <details>
              <summary>{m2.reviewProgress}</summary>
              {Object.entries(quality.reviewCounts).map(([state, count]) => (
                <p key={state}>
                  {state.replaceAll('_', ' ')}: {count}
                </p>
              ))}
            </details>
            <table>
              <thead>
                <tr>
                  <th>Split</th>
                  <th>{m2.images}</th>
                </tr>
              </thead>
              <tbody>
                {['train', 'val', 'test'].map((split) => (
                  <tr key={split}>
                    <td>{split}</td>
                    <td>{quality.splitCounts[split] ?? 0}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <details>
              <summary>{m2.groups}</summary>
              {quality.groups.map((g) => (
                <p key={g.group}>
                  {g.group}: {g.images} → {g.split}
                </p>
              ))}
            </details>
            <details>
              <summary>{m2.mapping}</summary>
              {quality.classMapping
                .filter((c) => c.namespace === 'candidate')
                .map((c) => (
                  <p key={c.id}>
                    {c.exportId} · {c.displayName} · {c.stableKey}
                  </p>
                ))}
            </details>
            {[...quality.errors, ...quality.warnings].map((message, i) => (
              <p key={i}>{message}</p>
            ))}
          </div>
        )}
        <div className="dialog-actions">
          <button disabled={busy} onClick={onClose}>
            {m1.close}
          </button>
          <button
            className="primary"
            disabled={
              busy ||
              (quality !== null && quality.errors.length > 0) ||
              options.trainPercent < 0 ||
              options.valPercent < 0 ||
              options.trainPercent + options.valPercent > 100 ||
              !Number.isInteger(options.trainPercent) ||
              !Number.isInteger(options.valPercent)
            }
            onClick={() => onExport(options)}
          >
            {m1.exportNow}
          </button>
        </div>
      </section>
    </div>
  );
}
