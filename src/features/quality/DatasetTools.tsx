import { useEffect, useState } from 'react';
import { m5 } from '../../app/m5Strings';
import { resources } from '../../app/resourceStrings';
import { StoragePanel } from './StoragePanel';
import { useModalFocus } from '../../components/useModalFocus';
import { command, normalizeError } from '../../lib/backend';
import type {
  DatasetDashboard,
  DatasetSnapshot,
  DuplicateCandidate,
  FrameCandidate,
  ImagePreview,
  Progress,
  PropagationReport,
  SnapshotOptions,
} from '../../lib/contracts';

export function DatasetTools({
  image,
  videoFrame,
  busy,
  progress,
  onTask,
  onSelect,
  onReview,
  onClose,
}: {
  image: ImagePreview | null;
  videoFrame: boolean;
  busy: boolean;
  progress: Progress | null;
  onTask: (action: () => Promise<void>) => Promise<void>;
  onSelect: (id: string) => void;
  onReview: (id: string) => void;
  onClose: () => void;
}) {
  const modal = useModalFocus();
  const [tab, setTab] = useState('dashboard'),
    [error, setError] = useState(''),
    [summary, setSummary] = useState<DatasetDashboard | null>(null),
    [versions, setVersions] = useState<DatasetSnapshot[]>([]);
  const [report, setReport] = useState<unknown>(null),
    [force, setForce] = useState(false),
    [scope, setScope] = useState<'project' | 'group'>('group'),
    [distance, setDistance] = useState(8),
    [candidates, setCandidates] = useState<DuplicateCandidate[]>([]),
    [preview, setPreview] = useState(''),
    [previewId, setPreviewId] = useState('');
  const [targets, setTargets] = useState<FrameCandidate[]>([]),
    [selected, setSelected] = useState<string[]>([]),
    [propagated, setPropagated] = useState<PropagationReport | null>(null);
  const [options, setOptions] = useState<SnapshotOptions>({
    name: 'Dataset version',
    seed: 'labelflow-v1',
    trainPercent: 80,
    valPercent: 10,
    approvedOnly: true,
  });
  async function reload() {
    const [d, v] = await Promise.all([
      command('quality_dashboard', undefined),
      command('list_snapshots', undefined),
    ]);
    setSummary(d);
    setVersions(v);
  }
  useEffect(() => {
    let cancelled = false;
    void Promise.all([
      command('quality_dashboard', undefined),
      command('list_snapshots', undefined),
    ])
      .then(([d, v]) => {
        if (!cancelled) {
          setSummary(d);
          setVersions(v);
        }
      })
      .catch((e) => {
        if (!cancelled) setError(normalizeError(e).message);
      });
    return () => {
      cancelled = true;
    };
  }, []);
  useEffect(() => {
    let cancelled = false;
    if (videoFrame && image)
      void command('propagation_targets', { anchorId: image.info.id })
        .then((v) => {
          if (!cancelled) setTargets(v);
        })
        .catch((e) => {
          if (!cancelled) setError(normalizeError(e).message);
        });
    return () => {
      cancelled = true;
    };
  }, [image, videoFrame]);
  useEffect(() => {
    let cancelled = false;
    if (previewId)
      void command('media_thumbnail', { id: previewId })
        .then((v) => {
          if (!cancelled) setPreview(v);
        })
        .catch((e) => {
          if (!cancelled) setError(normalizeError(e).message);
        });
    return () => {
      cancelled = true;
    };
  }, [previewId]);
  function task(action: () => Promise<void>) {
    void onTask(async () => {
      setError('');
      try {
        await action();
      } catch (e) {
        setError(normalizeError(e).message);
        throw e;
      }
    });
  }
  const entries = (values: Record<string, number>) => (
    <dl className="quality-counts">
      {Object.entries(values).map(([label, count]) => (
        <div key={label}>
          <dt>{label}</dt>
          <dd>{count}</dd>
        </div>
      ))}
    </dl>
  );
  return (
    <div className="modal-backdrop">
      <section
        ref={modal}
        role="dialog"
        aria-modal="true"
        aria-label={m5.title}
        className="dialog dataset-tools"
      >
        <header className="section-heading">
          <div>
            <h2>{m5.title}</h2>
            <p className="review-help">{m5.milestone}</p>
          </div>
          <button disabled={busy} onClick={onClose}>
            {m5.close}
          </button>
        </header>
        <nav className="review-tabs" aria-label={m5.title}>
          {(
            [
              'dashboard',
              'similar',
              'propagation',
              'snapshots',
              'storage',
            ] as const
          ).map((t) => (
            <button
              key={t}
              aria-pressed={tab === t}
              disabled={busy}
              onClick={() => {
                setTab(t);
                setReport(null);
              }}
            >
              {t === 'storage' ? resources.title : m5[t]}
            </button>
          ))}
        </nav>
        {tab === 'storage' && <StoragePanel busy={busy} onTask={onTask} />}
        {error && (
          <p role="alert" className="error-banner">
            {error}
          </p>
        )}
        {busy && (
          <div className="job-bar">
            <span>
              {progress
                ? `${progress.completed} / ${progress.total} · ${progress.message}`
                : m5.working}
            </span>
            <progress max={progress?.total || 1} value={progress?.completed} />
            <button
              onClick={() =>
                void command('cancel_job', undefined).catch((e) =>
                  setError(normalizeError(e).message),
                )
              }
            >
              {m5.cancel}
            </button>
          </div>
        )}
        {tab === 'dashboard' && (
          <section>
            <button disabled={busy} onClick={() => task(reload)}>
              {m5.refresh}
            </button>
            {summary && (
              <>
                <dl className="quality-counts">
                  {(
                    [
                      'images',
                      'groupCount',
                      'tripCount',
                      'indexed',
                      'unindexed',
                      'indexErrors',
                      'lowDetailCandidates',
                      'pendingPredictions',
                      'pendingPropagations',
                      'snapshots',
                    ] as const
                  ).map((k) => (
                    <div key={k}>
                      <dt>{m5.stats[k]}</dt>
                      <dd>{summary[k]}</dd>
                    </div>
                  ))}
                </dl>
                <h3>{m5.reviewStates}</h3>
                {entries(summary.reviewCounts)}
                <h3>{m5.events}</h3>
                {entries(summary.eventCounts)}
                <h3>{m5.tags}</h3>
                {entries(summary.tagCounts)}
                <p className="review-help">{m5.detailHelp}</p>
              </>
            )}
          </section>
        )}
        {tab === 'similar' && (
          <section>
            <p>{m5.similarityHelp}</p>
            <fieldset disabled={busy} className="review-form">
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={force}
                  onChange={(e) => setForce(e.target.checked)}
                />
                {m5.rebuild}
              </label>
              <button
                onClick={() =>
                  task(async () => {
                    setReport(await command('index_fingerprints', { force }));
                    await reload();
                  })
                }
              >
                {m5.index}
              </button>
              <label>
                {m5.scope}
                <select
                  value={scope}
                  onChange={(e) => setScope(e.target.value as typeof scope)}
                >
                  <option value="group">{m5.sameGroup}</option>
                  <option value="project">{m5.wholeProject}</option>
                </select>
              </label>
              <label>
                {m5.distance}
                <input
                  type="number"
                  min={0}
                  max={64}
                  value={distance}
                  onChange={(e) => setDistance(Number(e.target.value))}
                />
              </label>
              <button
                disabled={!image}
                onClick={() =>
                  task(async () => {
                    if (!image) return;
                    setCandidates(
                      await command('rank_similar_media', {
                        mediaId: image.info.id,
                        scope,
                        maxDistance: distance,
                      }),
                    );
                    setPreviewId('');
                  })
                }
              >
                {m5.rank}
              </button>
            </fieldset>
            {!image && <p>{m5.noImage}</p>}
            <table className="review-table">
              <thead>
                <tr>
                  <th>{m5.image}</th>
                  <th>{m5.distance}</th>
                  <th>{m5.identity}</th>
                  <th>{m5.detail}</th>
                  <th>{m5.actions}</th>
                </tr>
              </thead>
              <tbody>
                {candidates.map((c) => (
                  <tr key={c.mediaId}>
                    <td>
                      <button
                        disabled={busy}
                        onClick={() => {
                          setPreview('');
                          setPreviewId(c.mediaId);
                        }}
                      >
                        {c.name}
                      </button>
                    </td>
                    <td>{c.distance} / 64</td>
                    <td>
                      {c.sameSourceHash ? m5.exactHash : m5.perceptual}
                      {c.sameGroup ? ` · ${m5.sameGroup}` : ''}
                    </td>
                    <td>
                      {c.sharpness.toFixed(1)} / {c.brightness.toFixed(1)}
                    </td>
                    <td>
                      <button
                        disabled={busy}
                        onClick={() => onSelect(c.mediaId)}
                      >
                        {m5.openImage}
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {preview && image && (
              <div className="similar-preview">
                <figure>
                  <img src={image.dataUrl} alt={image.info.name} />
                  <figcaption>{image.info.name}</figcaption>
                </figure>
                <figure>
                  <img
                    src={preview}
                    alt={
                      candidates.find((c) => c.mediaId === previewId)?.name ??
                      m5.image
                    }
                  />
                  <figcaption>
                    {candidates.find((c) => c.mediaId === previewId)?.name}
                  </figcaption>
                </figure>
              </div>
            )}
          </section>
        )}
        {tab === 'propagation' && (
          <section>
            <p>{m5.propagationHelp}</p>
            {(!image || !videoFrame || !targets.length) && <p>{m5.noFrames}</p>}
            {!!targets.length && (
              <>
                <fieldset disabled={busy} className="propagation-targets">
                  <legend>{m5.targetFrames}</legend>
                  {targets.map((t) => (
                    <label key={t.mediaId} className="checkbox">
                      <input
                        type="checkbox"
                        disabled={
                          t.reviewStatus === 'approved' ||
                          (!selected.includes(t.mediaId) &&
                            selected.length >= 10)
                        }
                        checked={selected.includes(t.mediaId)}
                        onChange={(e) =>
                          setSelected((v) =>
                            e.target.checked
                              ? [...v, t.mediaId]
                              : v.filter((id) => id !== t.mediaId),
                          )
                        }
                      />
                      {t.name} · {t.deltaMs.toFixed(1)} ms · {t.reviewStatus}
                    </label>
                  ))}
                </fieldset>
                <button
                  disabled={busy || !selected.length}
                  onClick={() =>
                    task(async () => {
                      if (!image) return;
                      const result = await command('propagate_frames', {
                        anchorId: image.info.id,
                        targetIds: selected,
                      });
                      setPropagated(result);
                      setReport(result);
                      await reload();
                    })
                  }
                >
                  {m5.generate}
                </button>
              </>
            )}
            {propagated?.created.map((p) => (
              <p key={p.key}>
                {p.dx}, {p.dy} px · {m5.error}: {p.meanError.toFixed(2)} ·{' '}
                {m5.skippedShapes}: {p.skippedShapes}{' '}
                <button disabled={busy} onClick={() => onReview(p.targetId)}>
                  {m5.review}
                </button>
              </p>
            ))}
          </section>
        )}
        {tab === 'snapshots' && (
          <section>
            <p>{m5.snapshotHelp}</p>
            <fieldset disabled={busy} className="review-form">
              <label>
                {m5.name}
                <input
                  maxLength={160}
                  value={options.name}
                  onChange={(e) =>
                    setOptions({ ...options, name: e.target.value })
                  }
                />
              </label>
              <label>
                {m5.seed}
                <input
                  maxLength={128}
                  value={options.seed}
                  onChange={(e) =>
                    setOptions({ ...options, seed: e.target.value })
                  }
                />
              </label>
              <label>
                {m5.train}
                <input
                  type="number"
                  min={0}
                  max={100}
                  value={options.trainPercent}
                  onChange={(e) =>
                    setOptions({
                      ...options,
                      trainPercent: Number(e.target.value),
                    })
                  }
                />
              </label>
              <label>
                {m5.val}
                <input
                  type="number"
                  min={0}
                  max={100}
                  value={options.valPercent}
                  onChange={(e) =>
                    setOptions({
                      ...options,
                      valPercent: Number(e.target.value),
                    })
                  }
                />
              </label>
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={options.approvedOnly}
                  onChange={(e) =>
                    setOptions({ ...options, approvedOnly: e.target.checked })
                  }
                />
                {m5.approvedOnly}
              </label>
              <button
                onClick={() =>
                  task(async () => {
                    const result = await command('create_snapshot', {
                      options,
                    });
                    setReport(result);
                    await reload();
                  })
                }
              >
                {m5.create}
              </button>
            </fieldset>
            <p className="review-help">{m5.snapshotFormat}</p>
            <table className="review-table">
              <thead>
                <tr>
                  <th>{m5.name}</th>
                  <th>{m5.contentId}</th>
                  <th>{m5.imageCount}</th>
                  <th>{m5.actions}</th>
                </tr>
              </thead>
              <tbody>
                {versions.map((v) => (
                  <tr key={v.id}>
                    <td>
                      {v.name}
                      <small>{v.createdAt}</small>
                    </td>
                    <td>
                      <code title={v.id}>{v.id.slice(0, 16)}…</code>
                    </td>
                    <td>{v.images}</td>
                    <td>
                      <button
                        disabled={busy}
                        onClick={() =>
                          task(async () =>
                            setReport(
                              await command('verify_snapshot', {
                                id: v.id,
                                checkSources: true,
                              }),
                            ),
                          )
                        }
                      >
                        {m5.check}
                      </button>
                      <button
                        disabled={busy}
                        onClick={() =>
                          task(async () => {
                            const path = await command('export_snapshot', {
                              id: v.id,
                            });
                            if (path) setReport({ path });
                          })
                        }
                      >
                        {m5.export}
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        )}
        {report !== null && (
          <pre className="dataset-report" aria-label={m5.result}>
            {JSON.stringify(report, null, 2)}
          </pre>
        )}
      </section>
    </div>
  );
}
