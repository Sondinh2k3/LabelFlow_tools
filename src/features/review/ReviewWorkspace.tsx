import { useEffect, useState } from 'react';
import { useModalFocus } from '../../components/useModalFocus';
import { m4, roadTags } from '../../app/m4Strings';
import { m5 } from '../../app/m5Strings';
import { command, normalizeError } from '../../lib/backend';
import type {
  AnnotationDocument,
  ImagePreview,
  Profile,
  Progress,
  RoadMetadata,
  Trip,
  ReviewFilter,
  ReviewPage,
  ReviewSnapshot,
  ReviewPredictionSummary,
  PropagationInfo,
} from '../../lib/contracts';
import { reviewStatuses } from '../../lib/contracts';
import { ImageCanvas } from '../annotations/canvas/ImageCanvas';
import { useReviewOverlay, type ReviewView } from './useReviewOverlay';

const emptyTrip: Trip = {
  id: '',
  name: '',
  cameraId: '',
  route: '',
  distanceKm: null,
  notes: '',
  revision: 0,
};
export function ReviewWorkspace({
  image,
  profile,
  videoFrame,
  busy,
  progress,
  onTask,
  onChanged,
  onSelect,
  onClose,
  registerExitGuard,
}: {
  image: ImagePreview;
  profile: Profile;
  videoFrame: boolean;
  busy: boolean;
  progress: Progress | null;
  onTask: (action: () => Promise<void>) => Promise<void>;
  onChanged: (doc: AnnotationDocument) => void;
  onSelect: (id: string) => void;
  onClose: () => void;
  registerExitGuard: (guard: () => Promise<void>) => void;
}) {
  const modal = useModalFocus();
  const [tab, setTab] = useState('compare'),
    [error, setError] = useState(''),
    [refresh, setRefresh] = useState(0);
  const [doc, setDoc] = useState<AnnotationDocument | null>(null),
    [metadata, setMetadata] = useState<RoadMetadata | null>(null),
    [savedMetadata, setSavedMetadata] = useState('');
  const [trips, setTrips] = useState<Trip[]>([]),
    [trip, setTrip] = useState<Trip>(emptyTrip),
    [savedTrip, setSavedTrip] = useState(JSON.stringify(emptyTrip));
  const [predictions, setPredictions] = useState<ReviewPredictionSummary[]>([]),
    [key, setKey] = useState(''),
    [snapshot, setSnapshot] = useState<ReviewSnapshot | null>(null);
  const [view, setView] = useState<ReviewView>('groundTruth'),
    [layer, setLayer] = useState(''),
    [opacity, setOpacity] = useState(0.5);
  const [filter, setFilter] = useState<ReviewFilter>({
      status: '',
      event: '',
      tag: '',
      tripId: '',
      disagreementsOnly: false,
    }),
    [offset, setOffset] = useState(0),
    [page, setPage] = useState<ReviewPage>({ total: 0, items: [] });
  const [exportPath, setExportPath] = useState('');
  const [propagation, setPropagation] = useState<PropagationInfo | null>(null),
    [inspected, setInspected] = useState(false);
  const dirty =
    (metadata !== null && JSON.stringify(metadata) !== savedMetadata) ||
    JSON.stringify(trip) !== savedTrip;
  useEffect(() => {
    registerExitGuard(async () => {
      if (dirty && !window.confirm(m4.discard)) throw Error(m4.discard);
    });
    const before = (event: BeforeUnloadEvent) => {
      if (dirty) {
        event.preventDefault();
        event.returnValue = '';
      }
    };
    window.addEventListener('beforeunload', before);
    return () => {
      registerExitGuard(async () => {});
      window.removeEventListener('beforeunload', before);
    };
  }, [dirty, registerExitGuard]);
  function allowLeave() {
    return !dirty || window.confirm(m4.discard);
  }
  useEffect(() => {
    let cancelled = false;
    void Promise.all([
      command('load_document', { mediaId: image.info.id }),
      command('review_metadata', { mediaId: image.info.id }),
      command('list_trips', undefined),
      command('review_predictions', { mediaId: image.info.id }),
    ])
      .then(([d, m, t, p]) => {
        if (cancelled) return;
        setDoc(d);
        setMetadata(m);
        setSavedMetadata(JSON.stringify(m));
        setTrips(t);
        setPredictions(p);
        setKey(p[0]?.key ?? '');
        setSnapshot(null);
        setView('groundTruth');
        setError('');
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
    if (key)
      void command('review_snapshot', { key })
        .then(async (value) => {
          const info = value.prediction.propagation
            ? await command('propagation_info', { key })
            : null;
          if (!cancelled) {
            setSnapshot(value);
            setPropagation(info);
            setInspected(false);
          }
        })
        .catch((cause) => {
          if (!cancelled) setError(normalizeError(cause).message);
        });
    return () => {
      cancelled = true;
    };
  }, [key, refresh]);
  useEffect(() => {
    if (tab !== 'queue') return;
    let cancelled = false;
    void command('review_page', { filter, offset, limit: 40 })
      .then((value) => {
        if (!cancelled) setPage(value);
      })
      .catch((cause) => {
        if (!cancelled) setError(normalizeError(cause).message);
      });
    return () => {
      cancelled = true;
    };
  }, [filter, offset, tab, refresh]);
  const overlay = useReviewOverlay(
    snapshot,
    profile,
    view,
    layer,
    opacity,
    doc,
  );
  function task(action: () => Promise<void>) {
    void onTask(async () => {
      setError('');
      try {
        await action();
        setRefresh((n) => n + 1);
      } catch (cause) {
        setError(normalizeError(cause).message);
        throw cause;
      }
    });
  }
  const propagationVerified = Boolean(
    propagation?.verifiedAt &&
    propagation.verifiedRevision === snapshot?.groundTruth.revision,
  );
  async function reloadPredictions() {
    const p = await command('review_predictions', { mediaId: image.info.id });
    setPredictions(p);
    return p;
  }
  async function saveLabel(change: Partial<AnnotationDocument>) {
    if (!doc) return;
    const next = { ...doc, ...change };
    next.revision = await command('save_document', {
      document: next,
      operationId: crypto.randomUUID(),
    });
    setDoc(next);
    onChanged(next);
  }
  function selectImage(id: string) {
    if (!allowLeave()) return;
    onSelect(id);
    setTab('compare');
  }
  const numeric = (value: string) => (value === '' ? null : Number(value));
  return (
    <div className="modal-backdrop">
      <section
        ref={modal}
        role="dialog"
        aria-modal="true"
        aria-label={m4.title}
        className="dialog review-dialog"
      >
        <div className="ai-heading">
          <div>
            <h2>{m4.title}</h2>
            <p className="review-help">{m4.workflow}</p>
          </div>
          <button
            disabled={busy}
            onClick={() => {
              if (allowLeave()) onClose();
            }}
          >
            {m4.close}
          </button>
        </div>
        <nav className="review-tabs" aria-label="Review sections">
          {(['compare', 'metadata', 'trips', 'queue'] as const).map((value) => (
            <button
              key={value}
              aria-pressed={tab === value}
              disabled={busy}
              onClick={() => setTab(value)}
            >
              {m4[value]}
            </button>
          ))}
        </nav>
        {error && (
          <p role="alert" className="error-banner">
            {error}
          </p>
        )}
        {busy && (
          <div className="job-bar">
            <span>
              {progress
                ? `${progress.completed}/${progress.total} · ${progress.message}`
                : 'Working…'}
            </span>
            <button
              onClick={() =>
                void command('cancel_job', undefined).catch((cause) =>
                  setError(normalizeError(cause).message),
                )
              }
            >
              {m4.cancel}
            </button>
          </div>
        )}
        {tab === 'compare' && (
          <div className="review-layout">
            <div className="review-controls">
              <p className="review-help">{m4.roadHelp}</p>
              <label>
                {m4.event}
                <select
                  aria-label={m4.event}
                  disabled={busy || !doc}
                  value={doc?.eventLabel ?? ''}
                  onChange={(e) => {
                    const eventLabel = (e.target.value ||
                      null) as AnnotationDocument['eventLabel'];
                    task(() =>
                      saveLabel({
                        eventLabel,
                        ...(doc?.reviewStatus === 'approved'
                          ? { reviewStatus: 'needs_review' as const }
                          : {}),
                      }),
                    );
                  }}
                >
                  <option value="">Unassessed</option>
                  {['positive', 'negative', 'uncertain'].map((v) => (
                    <option key={v}>{v}</option>
                  ))}
                </select>
              </label>
              <label>
                {m4.review}
                <select
                  aria-label={m4.review}
                  disabled={busy || !doc}
                  value={doc?.reviewStatus ?? 'unlabeled'}
                  onChange={(e) => {
                    const reviewStatus = e.target
                      .value as AnnotationDocument['reviewStatus'];
                    task(() => saveLabel({ reviewStatus }));
                  }}
                >
                  {reviewStatuses.map((s) => (
                    <option key={s}>{s}</option>
                  ))}
                </select>
              </label>
              <button
                disabled={busy}
                onClick={() =>
                  task(async () => {
                    const imported = await command('import_review_prediction', {
                      mediaId: image.info.id,
                    });
                    if (imported) {
                      await reloadPredictions();
                      setKey(imported);
                      setSnapshot(null);
                      setView('prediction');
                    }
                  })
                }
              >
                {m4.import}
              </button>
              <label>
                Stored prediction
                <select
                  aria-label="Stored prediction"
                  value={key}
                  disabled={busy || !predictions.length}
                  onChange={(e) => {
                    setSnapshot(null);
                    setKey(e.target.value);
                  }}
                >
                  <option value="">Select prediction</option>
                  {predictions.map((p) => (
                    <option key={p.key} value={p.key}>
                      {p.modelName} / {p.modelVersion} · {p.status}
                    </option>
                  ))}
                </select>
              </label>
              {!predictions.length && (
                <p className="review-help">{m4.noPrediction}</p>
              )}
              <label>
                Overlay view
                <select
                  aria-label="Overlay view"
                  value={view}
                  onChange={(e) => setView(e.target.value as ReviewView)}
                >
                  {(
                    ['groundTruth', 'prediction', 'both', 'difference'] as const
                  ).map((v) => (
                    <option key={v} value={v}>
                      {m4[v]}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Review layer
                <select
                  aria-label="Review layer"
                  value={layer}
                  onChange={(e) => setLayer(e.target.value)}
                >
                  <option value="">All independent layers</option>
                  {profile.layers.map((l) => (
                    <option key={l.id} value={l.id}>
                      {l.displayName}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Overlay opacity
                <input
                  type="range"
                  min="0"
                  max="1"
                  step=".05"
                  value={opacity}
                  onChange={(e) => setOpacity(Number(e.target.value))}
                />
              </label>
              {snapshot && (
                <>
                  <p className="review-help">
                    {snapshot.prediction.model.name} ·{' '}
                    {snapshot.prediction.model.version}
                    <br />
                    GT event: {snapshot.comparison.gtEvent ?? 'unassessed'} ·
                    predicted event:{' '}
                    {snapshot.comparison.predictedEvent ?? 'not supplied'}
                    <br />
                    GT revision {snapshot.comparison.gtRevision} ·{' '}
                    {snapshot.comparison.gtApproved
                      ? 'approved'
                      : 'draft / excluded from aggregate metrics'}
                  </p>
                  <table className="review-metrics">
                    <thead>
                      <tr>
                        <th>Layer</th>
                        <th>IoU</th>
                        <th>Different</th>
                      </tr>
                    </thead>
                    <tbody>
                      {snapshot.comparison.layers.map((l) => (
                        <tr key={l.layerId}>
                          <td>{l.layerId}</td>
                          <td>
                            {l.meanIou === null
                              ? 'N/A'
                              : `${(l.meanIou * 100).toFixed(1)}%`}
                          </td>
                          <td>{l.disagreementPixels}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                  {snapshot.comparison.missingGtLayers.length > 0 && (
                    <p className="review-help">
                      Missing GT (unscored):{' '}
                      {snapshot.comparison.missingGtLayers.join(', ')}
                    </p>
                  )}
                  {snapshot.comparison.differenceOmitted && (
                    <p className="review-help">
                      Difference overlay exceeds display budget; exact counts
                      remain available.
                    </p>
                  )}
                  <p className="review-help">
                    Human / predicted objects:{' '}
                    {
                      snapshot.groundTruth.annotations.filter(
                        (a) => a.classId !== null,
                      ).length
                    }{' '}
                    /{' '}
                    {
                      snapshot.prediction.annotations.filter(
                        (a) => a.classId !== null,
                      ).length
                    }
                  </p>
                </>
              )}
              <p className="review-help">{m4.protect}</p>
              {snapshot?.prediction.propagation && (
                <fieldset
                  disabled={
                    busy ||
                    snapshot.summary.status !== 'pending' ||
                    propagationVerified
                  }
                  className="propagation-check"
                >
                  <legend>{m5.propagation}</legend>
                  <p className="review-help">{m5.verifyHelp}</p>
                  <p>
                    {m5.error}:{' '}
                    {snapshot.prediction.propagation.meanError.toFixed(2)} · Δ{' '}
                    {snapshot.prediction.propagation.dx},{' '}
                    {snapshot.prediction.propagation.dy} px · {m5.skippedShapes}
                    : {snapshot.prediction.propagation.skippedShapes}
                  </p>
                  <label className="checkbox">
                    <input
                      type="checkbox"
                      checked={propagationVerified || inspected}
                      onChange={(e) => setInspected(e.target.checked)}
                    />
                    {m5.inspect}
                  </label>
                  <button
                    disabled={!inspected}
                    onClick={() =>
                      task(async () => {
                        setPropagation(
                          await command('verify_propagation', {
                            key,
                            expectedRevision: snapshot.groundTruth.revision,
                            confirmed: inspected,
                          }),
                        );
                      })
                    }
                  >
                    {m5.verify}
                  </button>
                  {propagation?.verifiedRevision ===
                    snapshot.groundTruth.revision &&
                    propagation.verifiedAt && (
                      <p role="status">{m5.verified}</p>
                    )}
                </fieldset>
              )}
              <div className="ai-buttons">
                <button
                  disabled={
                    busy ||
                    snapshot?.summary.status !== 'pending' ||
                    (!!snapshot?.prediction.propagation &&
                      (!propagation?.verifiedAt ||
                        propagation.verifiedRevision !==
                          snapshot.groundTruth.revision))
                  }
                  onClick={() =>
                    task(async () => {
                      if (!snapshot) return;
                      const updated = await command(
                        'approve_review_prediction',
                        {
                          key,
                          expectedRevision: snapshot.groundTruth.revision,
                        },
                      );
                      setDoc(updated);
                      onChanged(updated);
                      await reloadPredictions();
                    })
                  }
                >
                  {m4.accept}
                </button>
                <button
                  disabled={busy || snapshot?.summary.status !== 'pending'}
                  onClick={() =>
                    task(async () => {
                      await command('reject_review_prediction', { key });
                      await reloadPredictions();
                    })
                  }
                >
                  {m4.reject}
                </button>
              </div>
              <p className="review-help">{m4.metricsHelp}</p>
            </div>
            <div className="review-preview">
              <ImageCanvas image={image} overlay={overlay} />
              <p className="review-help">{m4.noLegal}</p>
            </div>
          </div>
        )}
        {tab === 'metadata' && metadata && (
          <fieldset disabled={busy} className="review-form">
            <legend>Capture metadata · {image.info.name}</legend>
            <p className="review-help">
              {m4.eventHelp}
              {videoFrame &&
                ' Trip assignment applies to the whole source video.'}
            </p>
            <label>
              Trip
              <select
                aria-label="Capture trip"
                value={metadata.tripId ?? ''}
                onChange={(e) =>
                  setMetadata({ ...metadata, tripId: e.target.value || null })
                }
              >
                <option value="">Unassigned</option>
                {trips.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Camera ID
              <input
                value={metadata.cameraId}
                onChange={(e) =>
                  setMetadata({ ...metadata, cameraId: e.target.value })
                }
              />
            </label>
            <label>
              Route
              <input
                value={metadata.route}
                onChange={(e) =>
                  setMetadata({ ...metadata, route: e.target.value })
                }
              />
            </label>
            <label>
              Captured at (ISO 8601, optional)
              <input
                value={metadata.capturedAt}
                onChange={(e) =>
                  setMetadata({ ...metadata, capturedAt: e.target.value })
                }
              />
            </label>
            <label>
              Latitude
              <input
                type="number"
                min="-90"
                max="90"
                step="any"
                value={metadata.latitude ?? ''}
                onChange={(e) =>
                  setMetadata({
                    ...metadata,
                    latitude: numeric(e.target.value),
                  })
                }
              />
            </label>
            <label>
              Longitude
              <input
                type="number"
                min="-180"
                max="180"
                step="any"
                value={metadata.longitude ?? ''}
                onChange={(e) =>
                  setMetadata({
                    ...metadata,
                    longitude: numeric(e.target.value),
                  })
                }
              />
            </label>
            <div className="review-tags">
              {roadTags.map((tag) => (
                <label key={tag} className="checkbox">
                  <input
                    type="checkbox"
                    checked={metadata.tags.includes(tag)}
                    onChange={(e) =>
                      setMetadata({
                        ...metadata,
                        tags: e.target.checked
                          ? [...metadata.tags, tag]
                          : metadata.tags.filter((t) => t !== tag),
                      })
                    }
                  />
                  {tag.replaceAll('_', ' ')}
                </label>
              ))}
            </div>
            <button
              className="primary"
              onClick={() =>
                task(async () => {
                  if (
                    videoFrame &&
                    metadata.tripId !== JSON.parse(savedMetadata).tripId &&
                    !window.confirm(m4.videoTrip)
                  )
                    return;
                  const saved = await command('save_review_metadata', {
                    metadata,
                  });
                  setMetadata(saved);
                  setSavedMetadata(JSON.stringify(saved));
                  const next = await command('load_document', {
                    mediaId: image.info.id,
                  });
                  setDoc(next);
                  onChanged(next);
                })
              }
            >
              {m4.saveMetadata}
            </button>
          </fieldset>
        )}
        {tab === 'trips' && (
          <fieldset disabled={busy} className="review-form">
            <legend>Trip / session split groups</legend>
            <label>
              Saved trip
              <select
                aria-label="Saved trip"
                value={trip.id}
                onChange={(e) => {
                  if (
                    JSON.stringify(trip) !== savedTrip &&
                    !window.confirm(m4.discard)
                  )
                    return;
                  const next =
                    trips.find((t) => t.id === e.target.value) ?? emptyTrip;
                  setTrip(next);
                  setSavedTrip(JSON.stringify(next));
                }}
              >
                <option value="">{m4.newTrip}</option>
                {trips.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Trip name
              <input
                value={trip.name}
                onChange={(e) => setTrip({ ...trip, name: e.target.value })}
              />
            </label>
            <label>
              Trip route
              <input
                value={trip.route}
                onChange={(e) => setTrip({ ...trip, route: e.target.value })}
              />
            </label>
            <label>
              Trip camera
              <input
                value={trip.cameraId}
                onChange={(e) => setTrip({ ...trip, cameraId: e.target.value })}
              />
            </label>
            <label>
              Measured trip distance (km, optional)
              <input
                type="number"
                min="0.000001"
                step="any"
                value={trip.distanceKm ?? ''}
                onChange={(e) =>
                  setTrip({ ...trip, distanceKm: numeric(e.target.value) })
                }
              />
            </label>
            <label>
              Trip notes
              <textarea
                value={trip.notes}
                onChange={(e) => setTrip({ ...trip, notes: e.target.value })}
              />
            </label>
            <p className="review-help">
              Distance metadata does not establish continuous evaluation
              coverage. Metrics report reviewed false-positive frames/km
              separately from event-trigger episodes.
            </p>
            <button
              className="primary"
              onClick={() =>
                task(async () => {
                  const saved = await command('save_trip', { trip });
                  setTrip(saved);
                  setSavedTrip(JSON.stringify(saved));
                  setTrips(await command('list_trips', undefined));
                })
              }
            >
              {m4.saveTrip}
            </button>
          </fieldset>
        )}
        {tab === 'queue' && (
          <div className="review-queue">
            <fieldset disabled={busy} className="review-filters">
              <legend>Review filters</legend>
              <label>
                Status
                <select
                  value={filter.status}
                  onChange={(e) => {
                    setOffset(0);
                    setFilter({ ...filter, status: e.target.value });
                  }}
                >
                  <option value="">All review states</option>
                  {reviewStatuses.map((s) => (
                    <option key={s}>{s}</option>
                  ))}
                </select>
              </label>
              <label>
                Event
                <select
                  value={filter.event}
                  onChange={(e) => {
                    setOffset(0);
                    setFilter({ ...filter, event: e.target.value });
                  }}
                >
                  <option value="">All events</option>
                  {['positive', 'negative', 'uncertain', 'none'].map((s) => (
                    <option key={s}>{s}</option>
                  ))}
                </select>
              </label>
              <label>
                Tag
                <select
                  value={filter.tag}
                  onChange={(e) => {
                    setOffset(0);
                    setFilter({ ...filter, tag: e.target.value });
                  }}
                >
                  <option value="">All tags</option>
                  {roadTags.map((t) => (
                    <option key={t}>{t}</option>
                  ))}
                </select>
              </label>
              <label>
                Trip
                <select
                  value={filter.tripId}
                  onChange={(e) => {
                    setOffset(0);
                    setFilter({ ...filter, tripId: e.target.value });
                  }}
                >
                  <option value="">All trips</option>
                  {trips.map((t) => (
                    <option key={t.id} value={t.id}>
                      {t.name}
                    </option>
                  ))}
                </select>
              </label>
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={filter.disagreementsOnly}
                  onChange={(e) => {
                    setOffset(0);
                    setFilter({
                      ...filter,
                      disagreementsOnly: e.target.checked,
                    });
                  }}
                />
                Disagreements only
              </label>
            </fieldset>
            <div className="ai-buttons">
              <button
                disabled={busy}
                onClick={() =>
                  task(async () => {
                    await command('analyze_review', undefined);
                  })
                }
              >
                {m4.analyze}
              </button>
              <button
                disabled={busy}
                onClick={() =>
                  task(async () => {
                    const path = await command(
                      'export_review_metrics',
                      undefined,
                    );
                    if (path) setExportPath(path);
                  })
                }
              >
                {m4.export}
              </button>
            </div>
            <p className="review-help">
              Latest stored prediction per image. Filters use comparisons for
              the current GT revision; rerun analysis after edits. {page.total}{' '}
              images match.
            </p>
            <table className="review-metrics">
              <thead>
                <tr>
                  <th>Image</th>
                  <th>Human event / review</th>
                  <th>Trip</th>
                  <th>Latest model</th>
                  <th>Comparison</th>
                </tr>
              </thead>
              <tbody>
                {page.items.map((r) => (
                  <tr key={r.mediaId}>
                    <td>
                      <button
                        disabled={busy}
                        onClick={() => selectImage(r.mediaId)}
                      >
                        {r.name}
                      </button>
                    </td>
                    <td>
                      {r.eventLabel ?? 'unassessed'} · {r.reviewStatus}
                    </td>
                    <td>{r.tripName ?? 'unassigned'}</td>
                    <td>{r.modelName ?? 'none'}</td>
                    <td>
                      {r.analyzed
                        ? r.disagreement === null
                          ? m4.unscored
                          : r.disagreement
                            ? 'Disagreement'
                            : 'Matches'
                        : m4.unscored}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div className="ai-buttons">
              <button
                disabled={busy || offset === 0}
                onClick={() => setOffset((n) => Math.max(0, n - 40))}
              >
                Previous page
              </button>
              <span>
                {offset + 1}–{Math.min(offset + 40, page.total)} / {page.total}
              </span>
              <button
                disabled={busy || offset + 40 >= page.total}
                onClick={() => setOffset((n) => n + 40)}
              >
                Next page
              </button>
            </div>
            {exportPath && (
              <p className="review-path" role="status">
                Export created: {exportPath}
              </p>
            )}
          </div>
        )}
      </section>
    </div>
  );
}
