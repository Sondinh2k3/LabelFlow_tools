import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import type {
  ImagePreview,
  Profile,
  MediaOrigin,
  Point,
} from '../../lib/contracts';
import { command, normalizeError } from '../../lib/backend';
import type { TeamApi } from '../team/useTeamFeed';
import { when } from '../team/format';
import { ClassManager } from './ClassManager';
import {
  team as tt,
  classStrings,
  samStrings,
  workflow,
} from '../../app/teamStrings';
import { useSam } from './useSam';
import { reviewStatuses } from '../../lib/contracts';
import { EditSession, type SaveStatus } from './history/session';
import { EditorController, type Tool } from './tools/controller';
import { ImageCanvas } from './canvas/ImageCanvas';
import { librarySummary } from './history/summary';
import { isTextEditing, toolKeys } from '../../app/commands';
import { m2 } from '../../app/m2Strings';
import { m4 } from '../../app/m4Strings';
import { m1 } from '../../app/m1Strings';
import { text } from '../../app/strings';
import type { LayerDefinition } from '../../lib/contracts';
/** The semantic layer keeps its historic ID, but its classes are project-defined. */
const layerName = (l: LayerDefinition) =>
  l.layerType === 'semantic' ? classStrings.semanticLayer : l.displayName;

export function EditorWorkspace({
  image,
  origin,
  profile,
  session,
  disabled,
  registerGuard,
  onStatus,
  onNavigate,
  onExport,
  onError,
  onProfile,
  team,
  onOpenTeam,
}: {
  image: ImagePreview;
  origin?: MediaOrigin | null;
  profile: Profile;
  session: EditSession;
  disabled: boolean;
  registerGuard: (guard: () => Promise<void>) => void;
  onStatus: (status: SaveStatus, summary: string) => void;
  onNavigate: (direction: number) => void;
  onExport: () => void;
  onError: (error: unknown) => void;
  onProfile: (profile: Profile) => void;
  team: TeamApi;
  onOpenTeam: (threadId: string | null) => void;
}) {
  const snapshot = useSyncExternalStore(session.subscribe, session.getSnapshot);
  const doc = snapshot.document;
  const summary = librarySummary(doc);
  const [controller] = useState(() => new EditorController());
  const samReady = useSyncExternalStore(
    controller.subscribe,
    () => controller.samReady,
  );
  const [tool, setTool] = useState<Tool>('select'),
    [layerId, setLayerId] = useState(() => {
      const saved = localStorage.getItem(`labelflow-layer-${profile.id}`);
      return (
        profile.layers.find((l) => l.id === saved)?.id ?? 'candidate_objects'
      );
    }),
    [classId, setClassId] = useState<string | null>(() => {
      const namespace = profile.layers.find((l) => l.id === layerId)?.namespace;
      const saved = localStorage.getItem(
        `labelflow-class-${profile.id}-${namespace}`,
      );
      return (
        profile.classes.find((c) => c.namespace === namespace && c.id === saved)
          ?.id ??
        profile.classes.find((c) => c.namespace === namespace && !c.isIgnore)
          ?.id ??
        null
      );
    }),
    [radius, setRadius] = useState(14),
    [selected, setSelected] = useState<string | null>(null),
    [opacity, setOpacity] = useState(0.45),
    [visible, setVisible] = useState(
      () => new Set(profile.layers.map((l) => l.id)),
    ),
    [overlays, setOverlays] = useState(true),
    [query, setQuery] = useState(''),
    [panels, setPanels] = useState(
      () => localStorage.getItem('labelflow-panels') !== 'hidden',
    );
  const [classesOpen, setClassesOpen] = useState(false),
    [help, setHelp] = useState(false),
    [ask, setAsk] = useState<{
      body: string;
      labels: boolean;
      pin: Point | null;
      picking: boolean;
      busy: boolean;
    } | null>(null);
  const threads = useMemo(
    () => team.bySha.get(image.info.sha256.toLowerCase()) ?? [],
    [team.bySha, image.info.sha256],
  );
  const openThreads = threads.filter((th) => th.status === 'open').length;
  const pins = useMemo(() => {
    const list: { at: Point; open: boolean }[] = [];
    for (const th of threads) {
      const pin = th.comments[0]?.pin;
      if (pin) list.push({ at: pin, open: th.status === 'open' });
    }
    if (ask?.pin) list.push({ at: ask.pin, open: true });
    return list;
  }, [threads, ask?.pin]);
  const drawPins = useCallback(
    (ctx: CanvasRenderingContext2D, zoom: number) => {
      for (const { at, open } of pins) {
        ctx.beginPath();
        ctx.arc(at.x, at.y, 11 / zoom, 0, Math.PI * 2);
        ctx.fillStyle = open
          ? 'rgba(255,138,61,0.45)'
          : 'rgba(160,160,160,0.4)';
        ctx.fill();
        ctx.lineWidth = 2 / zoom;
        ctx.strokeStyle = '#fff';
        ctx.stroke();
        ctx.beginPath();
        ctx.arc(at.x, at.y, 3 / zoom, 0, Math.PI * 2);
        ctx.fillStyle = '#fff';
        ctx.fill();
      }
    },
    [pins],
  );
  async function postQuestion() {
    if (!ask?.body.trim()) return;
    setAsk({ ...ask, busy: true, picking: false });
    try {
      await session.flush();
      await command('team_post', {
        input: {
          threadId: null,
          mediaId: image.info.id,
          body: ask.body,
          pin: ask.pin,
          includeLabels: ask.labels,
        },
      });
      setAsk(null);
      const next = await team.refresh();
      // Your own new question is not "new" to you.
      for (const th of next.threads)
        if (
          th.activity === 1 &&
          th.comments[0]?.author.id === team.status?.userId &&
          th.media.sha256.toLowerCase() === image.info.sha256.toLowerCase()
        )
          team.markSeen(th);
    } catch (cause) {
      setAsk((old) => (old ? { ...old, busy: false } : old));
      onError(normalizeError(cause));
    }
  }
  const search = useRef<HTMLInputElement>(null);
  const layer = profile.layers.find((l) => l.id === layerId)!;
  const classes = profile.classes.filter(
    (c) => c.namespace === layer.namespace,
  );
  // A class deleted in the class manager falls back to the first remaining class.
  const activeClassId =
    !classId || classes.some((c) => c.id === classId)
      ? classId
      : (classes.find((c) => !c.isIgnore)?.id ?? null);
  const locked =
    disabled || snapshot.status === 'failed' || snapshot.pending >= 8;
  const sam = useSam(controller, image.info.id, tool === 'sam', onError);
  useEffect(() => {
    localStorage.setItem(`labelflow-layer-${profile.id}`, layerId);
    if (classId)
      localStorage.setItem(
        `labelflow-class-${profile.id}-${layer.namespace}`,
        classId,
      );
  }, [profile.id, layerId, classId, layer.namespace]);
  useLayoutEffect(() => {
    controller.configure({
      document: doc,
      profile,
      width: image.info.width,
      height: image.info.height,
      tool,
      layerId,
      classId: activeClassId,
      radius,
      visible: overlays ? visible : new Set(),
      opacity,
      disabled: locked,
      commit: (d) => session.commit(d),
      selection: setSelected,
      error: (key) =>
        onError(Error(key in m1 ? String(m1[key as keyof typeof m1]) : key)),
      sam: sam.request,
    });
  }, [
    controller,
    doc,
    profile,
    image,
    tool,
    layerId,
    activeClassId,
    radius,
    visible,
    overlays,
    opacity,
    locked,
    session,
    onError,
    sam.request,
  ]);
  useEffect(() => () => controller.dispose(), [controller]);
  useEffect(() => {
    onStatus(snapshot.status, summary);
  }, [snapshot.status, summary, onStatus]);
  useEffect(() => {
    registerGuard(async () => {
      if (controller.hasDraft) throw Error(m1.finishDraft);
      await session.flush();
    });
  }, [registerGuard, session, controller]);
  useEffect(() => {
    localStorage.setItem('labelflow-panels', panels ? 'visible' : 'hidden');
    document.documentElement.classList.toggle('panels-hidden', !panels);
    return () => document.documentElement.classList.remove('panels-hidden');
  }, [panels]);
  function chooseLayer(id: string) {
    setLayerId(id);
    const layer = profile.layers.find((l) => l.id === id)!;
    setClassId(
      profile.classes.find(
        (c) =>
          c.namespace === layer.namespace &&
          c.id ===
            localStorage.getItem(
              `labelflow-class-${profile.id}-${layer.namespace}`,
            ),
      )?.id ??
        profile.classes.find(
          (c) => c.namespace === layer.namespace && !c.isIgnore,
        )?.id ??
        null,
    );
    // Polygons on the semantic layer fill pixels with the chosen class.
    if (layer.layerType === 'semantic' && tool === 'bbox') setTool('brush');
    if (layer.layerType === 'region' && tool === 'bbox') setTool('polygon');
  }
  function chooseTool(next: Tool) {
    if (next === 'bbox' && layer.layerType !== 'instance')
      chooseLayer('candidate_objects');
    if (
      (next === 'brush' || next === 'eraser') &&
      layer.layerType === 'instance'
    )
      chooseLayer('cityscapes_semantic');
    setTool(next);
  }
  function chooseClass(id: string) {
    setClassId(id);
    controller.assignClass(id);
  }
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (
        e.defaultPrevented ||
        isTextEditing(e.target) ||
        document.querySelector('[aria-modal="true"]')
      )
        return;
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z') {
        e.preventDefault();
        if (!locked) {
          controller.cancel();
          if (e.shiftKey) session.redo();
          else session.undo();
        }
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
        e.preventDefault();
        if (!locked && !controller.hasDraft) finish(e.shiftKey);
        return;
      }
      if (e.key === '?') {
        e.preventDefault();
        setHelp((v) => !v);
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'e') {
        e.preventDefault();
        onExport();
        return;
      }
      // Alt+1..9 select classes 10..18 (the plain digits only reach the first nine).
      if (e.altKey && !e.ctrlKey && !e.metaKey && /^Digit[1-9]$/.test(e.code)) {
        const c = classes.filter((k) => !k.isIgnore)[
          8 + Number(e.code.slice(5))
        ];
        if (c) {
          e.preventDefault();
          chooseClass(c.id);
        }
        return;
      }
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const name = e.key.toLowerCase();
      if (name in toolKeys) {
        e.preventDefault();
        chooseTool(toolKeys[name as keyof typeof toolKeys]);
      }
      if (name === 'escape') {
        controller.cancel();
        if (ask?.picking) setAsk({ ...ask, picking: false });
      }
      if (name === 'enter' && controller.hasDraft) {
        e.preventDefault();
        controller.finishPolygon();
      } else if (name === 'enter' && controller.hasSam) {
        e.preventDefault();
        controller.applySam(e.shiftKey);
      }
      if (name === 'backspace' && controller.hasDraft) {
        e.preventDefault();
        controller.backspace();
      }
      if (name === 'delete' && selected) {
        e.preventDefault();
        controller.deleteSelected(window.confirm(m1.confirmDelete));
      }
      if (name === 'arrowleft' || name === 'arrowright') {
        e.preventDefault();
        onNavigate(name === 'arrowleft' ? -1 : 1);
      }
      if (/^[1-9]$/.test(name)) {
        const c = classes[Number(name) - 1];
        if (c) chooseClass(c.id);
      }
      if (name === '[') setRadius((r) => Math.max(1, r - 2));
      if (name === ']') setRadius((r) => Math.min(256, r + 2));
      if (name === 'l') setOverlays((v) => !v);
      if (name === 'tab') {
        e.preventDefault();
        setPanels((v) => !v);
      }
      if (name === 'c') {
        e.preventDefault();
        search.current?.focus();
      }
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  });
  const update = (patch: Partial<typeof doc>) => {
    if (!locked) session.commit({ ...doc, ...patch });
  };
  function finish(approve: boolean) {
    const next = approve ? 'approved' : 'needs_review';
    if (doc.reviewStatus !== next) update({ reviewStatus: next });
    onNavigate(1);
  }
  return (
    <>
      <section className="editor-center">
        <div className="tool-strip">
          {(Object.keys(m1.tools) as Tool[]).map((t) => (
            <button
              key={t}
              disabled={locked}
              aria-label={m1.tools[t]}
              aria-pressed={tool === t}
              className={tool === t ? 'active' : ''}
              onClick={() => chooseTool(t)}
            >
              {m1.tools[t]}
            </button>
          ))}
          <span className="tool-spacer" />
          <button
            disabled={locked || !snapshot.canUndo}
            onClick={() => {
              controller.cancel();
              session.undo();
            }}
          >
            {m1.undo}
          </button>
          <button
            disabled={locked || !snapshot.canRedo}
            onClick={() => {
              controller.cancel();
              session.redo();
            }}
          >
            {m1.redo}
          </button>
        </div>
        <ImageCanvas
          image={image}
          editor={controller}
          overlay={pins.length ? drawPins : undefined}
          pick={
            ask?.picking
              ? (p) =>
                  setAsk((old) =>
                    old
                      ? {
                          ...old,
                          picking: false,
                          pin: {
                            x: Math.max(0, Math.min(image.info.width, p.x)),
                            y: Math.max(0, Math.min(image.info.height, p.y)),
                          },
                        }
                      : old,
                  )
              : null
          }
        />
        <div className="editor-status">
          <span className={`save-${snapshot.status}`}>
            {m1[snapshot.status]}
          </span>
          <span>
            {m1.revision} {doc.revision} · {doc.annotations.length}{' '}
            {text.annotations.toLowerCase()}
          </span>
          {snapshot.error && (
            <span role="alert">
              {snapshot.error.code}: {snapshot.error.message}
            </span>
          )}
          {snapshot.status === 'failed' && (
            <button onClick={() => session.retry()}>{m1.retry}</button>
          )}
          <span className="tool-spacer" />
          <button className="link-button" onClick={() => setHelp(true)}>
            {workflow.shortcutHint}
          </button>
        </div>
      </section>
      <aside className="inspector annotation-inspector">
        <div className="panel-label">
          {tt.section}
          {openThreads > 0 && <span className="team-badge">{openThreads}</span>}
        </div>
        {!team.status?.folder ? (
          <button className="team-cta" onClick={() => onOpenTeam(null)}>
            {tt.setUp}
          </button>
        ) : (
          <div className="team-inline">
            {threads.map((th) => (
              <button
                key={th.id}
                className={`team-thread ${th.status}`}
                onClick={() => onOpenTeam(th.id)}
              >
                <span className="team-thread-title">
                  {team.isUnread(th) && <i className="team-dot" />}
                  {th.comments[0]?.body.split('\n')[0]}
                </span>
                <small>
                  {th.comments.at(-1)?.author.name} ·{' '}
                  {tt.replies(th.comments.length)} · {when(th.updatedAtMs)}
                  {th.status === 'resolved' && ` · ${tt.resolved}`}
                </small>
              </button>
            ))}
            {ask ? (
              <div className="team-ask">
                <textarea
                  aria-label={tt.ask}
                  placeholder={tt.question}
                  value={ask.body}
                  maxLength={4000}
                  disabled={ask.busy}
                  autoFocus
                  onChange={(e) => setAsk({ ...ask, body: e.target.value })}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && e.ctrlKey) {
                      e.preventDefault();
                      void postQuestion();
                    }
                  }}
                />
                <label className="checkbox">
                  <input
                    type="checkbox"
                    checked={ask.labels}
                    disabled={ask.busy}
                    onChange={(e) =>
                      setAsk({ ...ask, labels: e.target.checked })
                    }
                  />
                  {tt.shareLabels}
                </label>
                <div className="team-row">
                  <button
                    aria-pressed={ask.picking}
                    className={ask.picking ? 'active' : ''}
                    disabled={ask.busy}
                    onClick={() => setAsk({ ...ask, picking: !ask.picking })}
                  >
                    {ask.picking ? tt.pinActive : ask.pin ? tt.pinSet : tt.pin}
                  </button>
                  {ask.pin && (
                    <button
                      disabled={ask.busy}
                      onClick={() => setAsk({ ...ask, pin: null })}
                    >
                      {tt.clearPin}
                    </button>
                  )}
                </div>
                <div className="team-row">
                  <button disabled={ask.busy} onClick={() => setAsk(null)}>
                    {tt.cancel}
                  </button>
                  <span className="tool-spacer" />
                  <button
                    className="primary"
                    disabled={ask.busy || !ask.body.trim()}
                    onClick={() => void postQuestion()}
                  >
                    {tt.post}
                  </button>
                </div>
              </div>
            ) : (
              <button
                className="team-cta"
                disabled={disabled}
                onClick={() =>
                  setAsk({
                    body: '',
                    labels: true,
                    pin: null,
                    picking: false,
                    busy: false,
                  })
                }
              >
                {tt.ask}
              </button>
            )}
          </div>
        )}
        {profile.id === 'road-encroachment-v1' && (
          <details className="road-protocol">
            <summary>Road labeling protocol</summary>
            <p className="review-help">{m4.workflow}</p>
            <p className="review-help">{m4.roadHelp}</p>
            <div className="road-quick-layers">
              {profile.layers
                .filter((l) => l.layerType !== 'instance')
                .map((l) => (
                  <button
                    key={l.id}
                    disabled={locked}
                    onClick={() => {
                      chooseLayer(l.id);
                      setTool('brush');
                    }}
                  >
                    Edit {layerName(l)}
                  </button>
                ))}
            </div>
            <p className="review-help">
              {doc.eventLabel
                ? `Human event: ${doc.eventLabel}`
                : 'Event is unassessed. Use Road / dataset review to assess the event first.'}
            </p>
          </details>
        )}
        <div className="panel-label">{m1.layers}</div>
        <select
          aria-label={m1.activeLayer}
          value={layerId}
          onChange={(e) => chooseLayer(e.target.value)}
          disabled={disabled}
        >
          {profile.layers.map((l) => (
            <option value={l.id} key={l.id}>
              {layerName(l)}
            </option>
          ))}
        </select>
        <div className="layer-list">
          {profile.layers.map((l) => (
            <label className="checkbox" key={l.id}>
              <input
                type="checkbox"
                checked={visible.has(l.id)}
                onChange={(e) =>
                  setVisible((old) => {
                    const next = new Set(old);
                    if (e.target.checked) next.add(l.id);
                    else next.delete(l.id);
                    return next;
                  })
                }
              />
              <span style={{ color: l.color }}>{layerName(l)}</span>
            </label>
          ))}
        </div>
        <label className="range-field">
          {m1.opacity}
          <input
            type="range"
            min={0}
            max={1}
            step={0.05}
            value={opacity}
            onChange={(e) => setOpacity(Number(e.target.value))}
          />
        </label>
        <div className="panel-label">{m1.classes}</div>
        {layer.namespace ? (
          <>
            <input
              ref={search}
              aria-label={m1.searchClasses}
              placeholder={m1.searchClasses}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <div className="class-list">
              {classes
                .filter((c) =>
                  c.displayName.toLowerCase().includes(query.toLowerCase()),
                )
                .map((c) => (
                  <button
                    className={activeClassId === c.id ? 'active' : ''}
                    aria-pressed={activeClassId === c.id}
                    key={c.id}
                    disabled={locked}
                    onClick={() => chooseClass(c.id)}
                  >
                    <i style={{ background: c.color }} />
                    <span>
                      {c.exportId} · {c.displayName}
                    </span>
                  </button>
                ))}
            </div>
          </>
        ) : (
          <p className="muted">{m1.noClass}</p>
        )}
        {layer.namespace && (
          <button disabled={locked} onClick={() => setClassesOpen(true)}>
            {classStrings.edit}
          </button>
        )}
        <div className="panel-label">{samStrings.section}</div>
        <div className="sam-panel">
          <span className={`sam-state sam-${sam.state}`}>
            {samStrings.states[sam.state]}
            {sam.model && ` · ${sam.model.name}`}
            {sam.state === 'ready' &&
              sam.lastMs !== null &&
              ` · ${Math.round(sam.lastMs)} ms`}
          </span>
          <div className="sam-actions">
            {sam.state === 'unconfigured' && (
              <button onClick={() => void sam.configure()}>
                {samStrings.choose}
              </button>
            )}
            {sam.state === 'stopped' && (
              <>
                <button className="primary" onClick={() => void sam.start()}>
                  {samStrings.start}
                </button>
                <button onClick={() => void sam.configure()}>
                  {samStrings.change}
                </button>
              </>
            )}
            {sam.model && tool !== 'sam' && (
              <button onClick={() => chooseTool('sam')}>
                {m1.tools.sam} (S)
              </button>
            )}
            {sam.model && (
              <button onClick={() => void sam.stop()}>{samStrings.stop}</button>
            )}
          </div>
          {tool === 'sam' && (
            <>
              <div className="sam-actions">
                <button
                  className="primary"
                  disabled={locked || !samReady}
                  onClick={() => controller.applySam(false)}
                >
                  {samStrings.apply}
                </button>
                {layer.layerType === 'semantic' && (
                  <button
                    disabled={locked || !samReady}
                    onClick={() => controller.applySam(true)}
                  >
                    {samStrings.applyUnlabeled}
                  </button>
                )}
                <button onClick={() => controller.cancel()}>
                  {samStrings.clear}
                </button>
              </div>
              <p className="review-help">{samStrings.help}</p>
            </>
          )}
        </div>
        <label>
          {m1.radius}
          <input
            type="number"
            min={1}
            max={256}
            value={radius}
            onChange={(e) =>
              setRadius(Math.max(1, Math.min(256, Number(e.target.value) || 1)))
            }
          />
        </label>
        <div className="panel-label">{m1.annotations}</div>
        <div className="annotation-list">
          {doc.annotations.map((a, i) => (
            <button
              key={a.id}
              className={a.id === selected ? 'active' : ''}
              onClick={() => {
                chooseLayer(a.layerId);
                setTool('select');
                controller.selectId(a.id);
              }}
            >
              {i + 1} · {a.geometry.type} ·{' '}
              {profile.classes.find((c) => c.id === a.classId)?.displayName ??
                profile.layers.find((l) => l.id === a.layerId)?.displayName}
            </button>
          ))}
        </div>
        {doc.annotations.find((a) => a.id === selected)?.geometry.type ===
          'mask' && <p className="muted">{m2.instanceMask}</p>}
        {!doc.annotations.length && <p className="muted">{m1.noAnnotations}</p>}
        <button
          disabled={locked || !selected}
          onClick={() =>
            controller.deleteSelected(window.confirm(m1.confirmDelete))
          }
        >
          {m1.delete}
        </button>
        <label>
          {m1.review}
          <select
            aria-label={m1.review}
            value={doc.reviewStatus}
            disabled={locked}
            onChange={(e) =>
              update({
                reviewStatus: e.target.value as typeof doc.reviewStatus,
              })
            }
          >
            {reviewStatuses.map((s) => (
              <option value={s} key={s}>
                {s.replaceAll('_', ' ')}
              </option>
            ))}
          </select>
        </label>
        <div className="team-row">
          <button
            disabled={locked}
            title={workflow.doneHelp}
            onClick={() => finish(false)}
          >
            {workflow.done}
          </button>
          <button
            className="primary"
            disabled={locked}
            title={workflow.approveHelp}
            onClick={() => finish(true)}
          >
            {workflow.approve}
          </button>
        </div>
        <label>
          {m1.event}
          <select
            aria-label={m1.event}
            value={doc.eventLabel ?? ''}
            disabled={locked}
            onChange={(e) =>
              update({
                eventLabel: (e.target.value || null) as typeof doc.eventLabel,
              })
            }
          >
            {(['none', 'positive', 'negative', 'uncertain'] as const).map(
              (v, i) => (
                <option key={v} value={i ? v : ''}>
                  {m1[v]}
                </option>
              ),
            )}
          </select>
        </label>
        <label>
          {m1.notes}
          <textarea
            aria-label={m1.notes}
            value={doc.notes}
            maxLength={8000}
            disabled={locked}
            onChange={(e) => update({ notes: e.target.value })}
          />
        </label>
        <label>
          {m1.splitGroup}
          <input
            aria-label={m1.splitGroup}
            value={doc.splitGroup}
            maxLength={512}
            disabled={
              locked || !!origin || /^trip:[\da-f-]{36}$/i.test(doc.splitGroup)
            }
            onChange={(e) => update({ splitGroup: e.target.value })}
          />
        </label>
        {/^trip:[\da-f-]{36}$/i.test(doc.splitGroup) && (
          <p className="review-help">
            Split group is managed by the assigned trip in Road / dataset
            review.
          </p>
        )}
        {origin && (
          <p className="source-path">
            Video {origin.videoId}
            <br />
            Frame {origin.frameIndex} · PTS {origin.pts} ·{' '}
            {origin.timeMs.toFixed(3)} ms
          </p>
        )}
        <p className="source-path">
          {image.info.name}
          <br />
          {image.info.width} × {image.info.height}
          <br />
          {image.info.path}
        </p>
      </aside>
      {classesOpen && layer.namespace && (
        <ClassManager
          profile={profile}
          namespace={layer.namespace}
          onProfile={onProfile}
          onClose={() => setClassesOpen(false)}
        />
      )}
      {help && (
        <div className="modal-backdrop" onClick={() => setHelp(false)}>
          <section
            role="dialog"
            aria-modal="true"
            aria-label={workflow.shortcuts}
            className="dialog shortcut-dialog"
            onClick={(e) => e.stopPropagation()}
          >
            <h2>{workflow.shortcuts}</h2>
            <dl>
              {workflow.list.map(([keys, action]) => (
                <div key={keys}>
                  <dt>
                    <kbd>{keys}</kbd>
                  </dt>
                  <dd>{action}</dd>
                </div>
              ))}
            </dl>
            <div className="team-row">
              <span className="tool-spacer" />
              <button autoFocus onClick={() => setHelp(false)}>
                {tt.close}
              </button>
            </div>
          </section>
        </div>
      )}
    </>
  );
}
