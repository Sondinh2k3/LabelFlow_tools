import { useEffect, useMemo, useState } from 'react';
import { useModalFocus } from '../../components/useModalFocus';
import { m3 } from '../../app/m3Strings';
import { command, normalizeError } from '../../lib/backend';
import type {
  AiPreview,
  AiStatus,
  AiSuggestion,
  AnnotationDocument,
  ImagePreview,
  Profile,
} from '../../lib/contracts';
import type { EditSession } from '../annotations/history/session';
import { decodeDocument } from '../annotations/history/session';
import { SceneRenderer } from '../annotations/canvas/renderer';
import { ImageCanvas } from '../annotations/canvas/ImageCanvas';

export function AiWorkspace({
  image,
  profile,
  session,
  busy,
  onTask,
  onAccepted,
  onClose,
}: {
  image: ImagePreview;
  profile: Profile;
  session: EditSession;
  busy: boolean;
  onTask: (action: () => Promise<void>) => Promise<void>;
  onAccepted: (document: AnnotationDocument) => void;
  onClose: () => void;
}) {
  const modal = useModalFocus();
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [suggestions, setSuggestions] = useState<AiSuggestion[]>([]);
  const [selected, setSelected] = useState('');
  const [preview, setPreview] = useState<AiPreview | null>(null);
  const [show, setShow] = useState(true);
  const [threshold, setThreshold] = useState(0.5);
  const [error, setError] = useState('');
  const current = suggestions.find((s) => s.id === selected);
  useEffect(() => {
    let cancelled = false;
    void Promise.all([
      command('ai_status', undefined),
      command('ai_suggestions', { mediaId: image.info.id }),
    ])
      .then(([next, rows]) => {
        if (!cancelled) {
          setStatus(next);
          setSuggestions(rows);
          setSelected(
            rows.find((s) => s.status === 'pending')?.id ?? rows[0]?.id ?? '',
          );
        }
      })
      .catch((cause) => {
        if (!cancelled) setError(normalizeError(cause).message);
      });
    return () => {
      cancelled = true;
    };
  }, [image.info.id]);
  useEffect(() => {
    let cancelled = false;
    if (selected)
      void command('ai_preview', { id: selected })
        .then((value) => {
          if (!cancelled) setPreview(value);
        })
        .catch((cause) => {
          if (!cancelled) setError(normalizeError(cause).message);
        });
    return () => {
      cancelled = true;
    };
  }, [selected]);
  const overlay = useMemo(() => {
    if (!preview || !show) return undefined;
    const renderer = new SceneRenderer();
    const document = decodeDocument({
      mediaId: image.info.id,
      revision: 0,
      reviewStatus: 'unlabeled',
      eventLabel: null,
      notes: '',
      splitGroup: '',
      annotations: preview.annotations,
      masks: preview.masks,
    });
    const visible = new Set(profile.layers.map((l) => l.id));
    return (ctx: CanvasRenderingContext2D, zoom: number) =>
      renderer.draw(
        ctx,
        document,
        profile,
        zoom,
        visible,
        0.48,
        null,
        null,
        [],
        null,
        0,
        false,
        null,
        null,
      );
  }, [preview, show, profile, image.info.id]);
  function task(action: () => Promise<void>) {
    void onTask(async () => {
      setError('');
      try {
        await action();
      } catch (cause) {
        setError(normalizeError(cause).message);
        throw cause;
      } finally {
        setStatus(await command('ai_status', undefined));
      }
    });
  }
  async function refresh() {
    setSuggestions(await command('ai_suggestions', { mediaId: image.info.id }));
  }
  return (
    <div className="modal-backdrop">
      <section
        ref={modal}
        className="dialog ai-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={m3.title}
      >
        <div className="ai-heading">
          <div>
            <h2>{m3.title}</h2>
            <p>{m3.optional}</p>
          </div>
          <button disabled={busy} onClick={onClose}>
            Close
          </button>
        </div>
        {error && (
          <p role="alert" className="error-banner">
            {error}
          </p>
        )}
        <div className="ai-layout">
          <div className="ai-controls">
            <p>
              {status?.runtime.running
                ? 'Worker active · CPUExecutionProvider'
                : m3.unavailable}
            </p>
            <button
              disabled={busy}
              onClick={() =>
                task(async () => {
                  await command('ai_configure', undefined);
                })
              }
            >
              {m3.configure}
            </button>
            {status?.config && (
              <details>
                <summary>Local configuration</summary>
                <p className="ai-path">{status.config.python}</p>
                <p className="ai-path">{status.config.manifest}</p>
              </details>
            )}
            <div className="ai-buttons">
              <button
                disabled={busy || !status?.config}
                onClick={() =>
                  task(async () => {
                    await command('ai_start', undefined);
                  })
                }
              >
                {m3.start}
              </button>
              <button
                disabled={busy || !status?.runtime.running}
                onClick={() =>
                  task(async () => {
                    await command('ai_stop', undefined);
                  })
                }
              >
                {m3.stop}
              </button>
            </div>
            {status?.runtime.model && (
              <p>
                {status.runtime.model.name} · {status.runtime.model.version}
                <br />
                {status.runtime.model.task} · {status.runtime.model.layerId}
              </p>
            )}
            <label>
              Box score threshold
              <input
                type="number"
                min="0"
                max="1"
                step="0.05"
                value={threshold}
                disabled={busy || status?.runtime.model?.task !== 'boxes'}
                onChange={(e) => setThreshold(Number(e.target.value))}
              />
            </label>
            <button
              className="primary"
              disabled={busy || !status?.runtime.model}
              onClick={() =>
                task(async () => {
                  const id = await command('ai_predict', {
                    mediaId: image.info.id,
                    threshold,
                  });
                  await refresh();
                  setPreview(null);
                  setSelected(id);
                })
              }
            >
              {m3.predict}
            </button>
            {busy && (
              <button
                onClick={() =>
                  void command('ai_cancel', undefined).catch((cause) =>
                    setError(normalizeError(cause).message),
                  )
                }
              >
                {m3.cancel}
              </button>
            )}
            <p className="ai-help">{m3.prompt}</p>
            <label>
              Suggestions
              <select
                aria-label="Suggestions"
                value={selected}
                disabled={busy || !suggestions.length}
                onChange={(e) => {
                  setPreview(null);
                  setSelected(e.target.value);
                }}
              >
                {!suggestions.length && <option value="">{m3.empty}</option>}
                {suggestions.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.status} · {s.modelName} · r{s.baseRevision} ·{' '}
                    {s.createdAt}
                  </option>
                ))}
              </select>
            </label>
            <label className="checkbox">
              <input
                type="checkbox"
                checked={show}
                onChange={(e) => setShow(e.target.checked)}
              />
              {m3.overlay}
            </label>
            {current && (
              <p>
                Status: <strong>{current.status}</strong> · base revision{' '}
                {current.baseRevision}
                <br />
                Confidence:{' '}
                {current.confidence === null
                  ? 'not provided'
                  : `${(current.confidence * 100).toFixed(1)}%`}
              </p>
            )}
            {preview && (
              <details>
                <summary>Model provenance</summary>
                <p>
                  {preview.prediction.model.name} ·{' '}
                  {preview.prediction.model.version} ·{' '}
                  {preview.prediction.model.adapter}
                </p>
                <p className="ai-path">
                  SHA-256: {preview.prediction.model.sha256}
                </p>
                <p>License: {preview.prediction.model.license}</p>
                <p>
                  {preview.prediction.processing.milliseconds.toFixed(1)} ms ·{' '}
                  {preview.prediction.processing.provider}
                </p>
                <p>Class map: {preview.prediction.model.classMap.join(', ')}</p>
              </details>
            )}
            <p className="ai-help">{m3.protection}</p>
            <div className="ai-buttons">
              <button
                disabled={busy || current?.status !== 'pending' || !preview}
                onClick={() =>
                  task(async () => {
                    const doc = await command('load_document', {
                      mediaId: image.info.id,
                    });
                    if (doc.revision !== current?.baseRevision)
                      throw Error(m3.stale);
                    const accepted = await command('ai_approve', {
                      id: selected,
                      expectedRevision: doc.revision,
                    });
                    onAccepted(accepted);
                    await refresh();
                  })
                }
              >
                {m3.approve}
              </button>
              <button
                disabled={busy || current?.status !== 'pending'}
                onClick={() =>
                  task(async () => {
                    await command('ai_reject', { id: selected });
                    await refresh();
                  })
                }
              >
                {m3.reject}
              </button>
            </div>
            <span className="ai-help">
              Current review: {session.getSnapshot().document.reviewStatus}
            </span>
          </div>
          <div className="ai-preview">
            <ImageCanvas image={image} overlay={overlay} />
            <p className="ai-help">
              Suggestion overlay · {image.info.width} × {image.info.height}{' '}
              canonical pixels. Source image is unchanged.
            </p>
          </div>
        </div>
      </section>
    </div>
  );
}
