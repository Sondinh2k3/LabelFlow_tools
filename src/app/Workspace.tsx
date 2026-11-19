import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { command, normalizeError } from '../lib/backend';
import type {
  AppError,
  ExportOptions,
  ImagePreview,
  MediaPage,
  Profile,
  Progress,
  Project,
  YoloInspection,
  MaskInspection,
  MaskFolderInspection,
  LibraryFilter,
  MediaOrigin,
} from '../lib/contracts';
import { VideoWorkspace } from '../features/video/VideoWorkspace';
import { AiWorkspace } from '../features/ai/AiWorkspace';
import { m3 } from './m3Strings';
import { m4 } from './m4Strings';
import { m5 } from './m5Strings';
import { DatasetTools } from '../features/quality/DatasetTools';
import { ReviewWorkspace } from '../features/review/ReviewWorkspace';
import { m2 } from './m2Strings';
import { m1 } from './m1Strings';
import { text } from './strings';
import { MediaBrowser } from '../features/media/MediaBrowser';
import { EditorWorkspace } from '../features/annotations/EditorWorkspace';
import {
  EditSession,
  type SaveStatus,
} from '../features/annotations/history/session';
import { DurableJournal } from '../features/annotations/history/journal';
import { ImageCanvas } from '../features/annotations/canvas/ImageCanvas';
import { decodedImages } from '../features/annotations/canvas/imagePool';
import { ExportDialog } from '../features/export/ExportDialog';
import { MappingDialog } from '../features/media/MappingDialog';
import { OperationReport } from '../components/OperationReport';
import { PanelLayout } from '../components/PanelLayout';
import { useTeamFeed } from '../features/team/useTeamFeed';
import { TeamPanel } from '../features/team/TeamPanel';
import { team as teamText, prelabel } from './teamStrings';

export function Workspace({
  project,
  onClose,
}: {
  project: Project;
  onClose: () => void;
}) {
  const [profile, setProfile] = useState<Profile | null>(null),
    [ready, setReady] = useState(false),
    [active, setActive] = useState<{
      image: ImagePreview;
      session: EditSession;
      origin: MediaOrigin | null;
    } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<AppError | null>(null),
    [refresh, setRefresh] = useState(0),
    [offset, setOffset] = useState(0),
    [page, setPage] = useState<
      MediaPage & {
        offset: number;
        query: string;
        status: string;
        filter?: LibraryFilter;
      }
    >({ items: [], total: 0, offset: 0, query: '', status: '' }),
    [copied, setCopied] = useState(false),
    [progress, setProgress] = useState<Progress | null>(null),
    [status, setStatus] = useState<SaveStatus>('saved'),
    [exportOpen, setExportOpen] = useState(false),
    [videoOpen, setVideoOpen] = useState(false),
    [aiOpen, setAiOpen] = useState(false),
    [reviewOpen, setReviewOpen] = useState(false),
    [toolsOpen, setToolsOpen] = useState(false),
    [repairTarget, setRepairTarget] = useState(''),
    [inspection, setInspection] = useState<{
      value: YoloInspection | MaskInspection | MaskFolderInspection;
      layerId: string;
      instances?: boolean;
      folder?: boolean;
    } | null>(null),
    [report, setReport] = useState<unknown>(null),
    [teamOpen, setTeamOpen] = useState<{ thread: string | null } | null>(null);
  const team = useTeamFeed(project.id);
  const discussed = new Map(
    [...team.bySha].map(([sha, list]) => [
      sha,
      list.some((t) => t.status === 'open') ? 'open' : 'resolved',
    ]),
  );
  const [journal] = useState(
    () => new DurableJournal(`${project.id}:${project.root}`),
  );
  const guard = useRef<() => Promise<void>>(async () => {});
  const reviewGuard = useRef<() => Promise<void>>(async () => {});
  const registerReviewGuard = useCallback((value: () => Promise<void>) => {
    reviewGuard.current = value;
  }, []);
  const registerGuard = useCallback((value: () => Promise<void>) => {
    guard.current = value;
  }, []);
  const nativeActions = useRef<{
    drop: (paths: string[]) => void;
    busy: boolean;
  }>({ drop: () => {}, busy: false });
  const handleError = useCallback(
    (cause: unknown) => setError(normalizeError(cause)),
    [],
  );
  const savedSummary = useRef('');
  const updateStatus = useCallback((next: SaveStatus, summary: string) => {
    setStatus(next);
    if (next === 'saved' && summary !== savedSummary.current) {
      if (savedSummary.current) setRefresh((n) => n + 1);
      savedSummary.current = summary;
    }
  }, []);
  const makeSession = useCallback(
    (doc: Awaited<ReturnType<typeof command<'load_document'>>>) =>
      new EditSession(
        doc,
        (document, operationId) =>
          command('save_document', { document, operationId }),
        journal,
      ),
    [journal],
  );
  const install = useCallback(
    async (image: ImagePreview) => {
      const doc = await command('load_document', { mediaId: image.info.id });
      const origin = await command('media_origin', { id: image.info.id });
      setActive({ image, session: makeSession(doc), origin });
      setRepairTarget(image.info.id);
    },
    [makeSession],
  );
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const config = await command('project_profile', undefined);
        if (cancelled) return;
        setProfile(config);
        await journal.recover();
        const warnings = await command('project_health', undefined);
        if (!cancelled && warnings.length) setReport({ warnings });
        const image = await command('restore_image', undefined);
        if (!cancelled && image) await install(image);
      } catch (cause) {
        if (!cancelled) handleError(cause);
      } finally {
        if (!cancelled) setReady(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [handleError, install, journal]);
  async function run(action: () => Promise<void>, flush = true) {
    if (busy) {
      handleError(Error(m1.busy));
      return;
    }
    setBusy(true);
    setError(null);
    try {
      if (flush) await guard.current();
      await action();
    } catch (cause) {
      handleError(cause);
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }
  const select = (id: string) => {
    setRepairTarget(id);
    void run(async () => {
      const image = await command('select_media', { id });
      await install(image);
    });
  };
  const importMedia = (folder: boolean, paths?: string[]) =>
    void run(async () => {
      const result = await command('import_media', {
        folder,
        copied,
        ...(paths ? { paths } : {}),
      });
      if (result) {
        setReport(result);
        setRefresh((n) => n + 1);
      }
    });
  useLayoutEffect(() => {
    nativeActions.current = {
      drop: (paths) => importMedia(false, paths),
      busy,
    };
  });
  const exportData = (options: ExportOptions) => {
    setExportOpen(false);
    void run(async () =>
      setReport(await command('export_dataset', { options })),
    );
  };
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const stops: (() => void)[] = [];
    const add = (promise: Promise<() => void>) =>
      void promise
        .then((stop) => {
          if (disposed) stop();
          else stops.push(stop);
        })
        .catch(handleError);
    add(
      listen<Progress>('job-progress', (event) => setProgress(event.payload)),
    );
    add(
      listen<string[]>('media-drop', (event) =>
        nativeActions.current.drop(event.payload),
      ),
    );
    add(
      getCurrentWindow().onCloseRequested(async (event) => {
        event.preventDefault();
        if (nativeActions.current.busy) {
          handleError(Error(m1.busy));
          return;
        }
        setBusy(true);
        try {
          await guard.current();
          await reviewGuard.current();
          await getCurrentWindow().destroy();
        } catch (cause) {
          handleError(cause);
          setBusy(false);
        }
      }),
    );
    return () => {
      disposed = true;
      for (const stop of stops) stop();
    };
  }, [handleError]);
  useEffect(() => {
    const warn = (event: BeforeUnloadEvent) => {
      if (active?.session.getSnapshot().status !== 'saved') {
        event.preventDefault();
        event.returnValue = '';
      }
    };
    window.addEventListener('beforeunload', warn);
    return () => window.removeEventListener('beforeunload', warn);
  }, [active]);
  const at = page.items.findIndex((r) => r.id === active?.image.info.id);
  const neighbor =
    at < 0 ? undefined : (page.items[at + 1] ?? page.items[at - 1]);
  const neighborId = neighbor?.id;
  const preloadFits =
    !!active &&
    !!neighbor &&
    (active.image.info.width * active.image.info.height +
      neighbor.width * neighbor.height) *
      4 <=
      32 * 1024 * 1024;
  useEffect(() => {
    if (!neighborId || !preloadFits) return;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          const preview = await command('preload_media', { id: neighborId });
          if (!cancelled) await decodedImages.prefetch(preview);
        } catch {
          /* Selection reports the recoverable source error. */
        }
      })();
    }, 150);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [neighborId, preloadFits]);
  useEffect(() => () => decodedImages.clear(), []);
  function navigate(direction: number) {
    const index = page.items.findIndex((r) => r.id === active?.image.info.id),
      next = page.items[index + direction];
    if (next) select(next.id);
    else {
      const targetOffset = page.offset + direction * 40;
      if (targetOffset < 0 || targetOffset >= page.total) return;
      void run(async () => {
        const result = await command('list_media', {
          offset: targetOffset,
          limit: 40,
          query: page.query,
          status: page.status,
          filter: page.filter,
        });
        const target = direction > 0 ? result.items[0] : result.items.at(-1);
        if (target) {
          await install(await command('select_media', { id: target.id }));
          setOffset(targetOffset);
        }
      });
    }
  }
  const reloadActive = async () => {
    if (active) {
      const doc = await command('load_document', {
        mediaId: active.image.info.id,
      });
      active.session.adopt(doc);
    }
    setRefresh((n) => n + 1);
  };
  const applyMapping = (
    mapping: { source: number; target: string }[],
    mode: 'fillUnlabeled' | 'replace' = 'fillUnlabeled',
  ) => {
    const pending = inspection;
    if (!pending) return;
    setInspection(null);
    void run(async () => {
      if (pending.folder) {
        setReport(
          await command('apply_mask_folder', {
            token: pending.value.token,
            mapping: mapping.map((m) => ({
              source: m.source,
              target: Number(m.target),
            })),
            mode,
          }),
        );
        await reloadActive();
      } else if ('values' in pending.value) {
        const doc = await command('apply_mask', {
          token: pending.value.token,
          mapping: mapping.map((m) => ({
            source: m.source,
            target: Number(m.target),
          })),
        });
        active?.session.adopt(doc);
        setRefresh((n) => n + 1);
      } else {
        const result = await command(
          pending.instances ? 'apply_instances' : 'import_yolo',
          {
            token: pending.value.token,
            mapping,
          },
        );
        setReport(result);
        if (active) {
          const doc = await command('load_document', {
            mediaId: active.image.info.id,
          });
          active.session.adopt(doc);
        }
        setRefresh((n) => n + 1);
      }
    });
  };
  return (
    <div className="workspace-shell">
      <div className="workspace-actions">
        <button
          disabled={busy || !profile}
          className="primary"
          onClick={() => importMedia(false)}
        >
          {m1.importImages}
        </button>
        <button disabled={busy || !profile} onClick={() => importMedia(true)}>
          {m1.importFolder}
        </button>
        <button
          disabled={busy || !profile}
          onClick={() =>
            void run(async () => {
              const image = await command('open_image', undefined);
              if (image) {
                await install(image);
                setRefresh((n) => n + 1);
              }
            })
          }
        >
          {text.openImage}
        </button>
        <button
          disabled={busy || !profile}
          onClick={() => void run(async () => setVideoOpen(true))}
        >
          {m2.video}
        </button>
        <label className="checkbox">
          <input
            type="checkbox"
            checked={copied}
            disabled={busy}
            onChange={(e) => setCopied(e.target.checked)}
          />
          {m1.managed}
        </label>
        <button
          disabled={busy || !active || !profile}
          onClick={() => void run(async () => setAiOpen(true))}
        >
          {m3.title}
        </button>
        <button
          disabled={busy || !active || !profile}
          onClick={() => void run(async () => setReviewOpen(true))}
        >
          {m4.title}
        </button>
        <button
          disabled={busy || !profile}
          onClick={() => void run(async () => setToolsOpen(true))}
        >
          {m5.title}
        </button>
        <button
          disabled={busy || !profile}
          onClick={() => setTeamOpen({ thread: null })}
        >
          {teamText.title}
          {team.unread > 0 && (
            <span className="team-badge">{teamText.newCount(team.unread)}</span>
          )}
        </button>
        <span className="tool-spacer" />
        <button
          disabled={busy || !profile}
          onClick={() => void run(async () => setExportOpen(true))}
        >
          {m1.export}
        </button>
        <button
          disabled={busy}
          onClick={() =>
            void run(async () => {
              const value = await command('backup_project', undefined);
              if (value) setReport({ [m1.backupCreated]: value });
            })
          }
        >
          {m1.backup}
        </button>
      </div>
      {error && (
        <div role="alert" className="error-banner">
          <span>
            {error.code}: {error.message}
          </span>
          <button aria-label={text.closeError} onClick={() => setError(null)}>
            ×
          </button>
        </div>
      )}
      {busy && (
        <div className="job-bar">
          <span>
            {progress
              ? `${progress.completed} / ${progress.total} · ${progress.message}`
              : text.working}
          </span>
          <progress value={progress?.completed} max={progress?.total || 1} />
          <button
            onClick={() =>
              void command('cancel_job', undefined).catch(handleError)
            }
          >
            {m1.cancel}
          </button>
        </div>
      )}
      <PanelLayout>
        <aside className="media-panel">
          <div className="panel-label">{text.project}</div>
          <h2 title={project.root}>{project.name}</h2>
          <div className="project-meta">
            {project.profileId} · {text.schema} {project.schemaVersion}
          </div>
          {ready && (
            <MediaBrowser
              offset={offset}
              onOffset={setOffset}
              selected={active?.image.info.id}
              refresh={refresh}
              disabled={busy}
              onSelect={select}
              onPage={setPage}
              onError={handleError}
              discussed={discussed}
            />
          )}
          <div className="media-navigation">
            <button
              disabled={
                busy ||
                (page.offset === 0 &&
                  page.items.findIndex((r) => r.id === active?.image.info.id) <=
                    0)
              }
              aria-label={m1.previous}
              onClick={() => navigate(-1)}
            >
              ←
            </button>
            <button
              disabled={
                busy ||
                !active ||
                (page.offset + page.items.length >= page.total &&
                  page.items.findIndex((r) => r.id === active.image.info.id) >=
                    page.items.length - 1)
              }
              aria-label={m1.next}
              onClick={() => navigate(1)}
            >
              →
            </button>
          </div>
          <div className="sidebar-bottom">
            <button
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  await command('close_project', undefined);
                  onClose();
                })
              }
            >
              {text.closeProject}
            </button>
          </div>
        </aside>
        {active && profile ? (
          <EditorWorkspace
            key={active.image.info.id}
            image={active.image}
            origin={active.origin}
            profile={profile}
            session={active.session}
            disabled={
              busy ||
              exportOpen ||
              videoOpen ||
              aiOpen ||
              reviewOpen ||
              toolsOpen ||
              !!teamOpen ||
              !!inspection ||
              report !== null
            }
            registerGuard={registerGuard}
            onStatus={updateStatus}
            onNavigate={navigate}
            onExport={() => void run(async () => setExportOpen(true))}
            onError={handleError}
            onProfile={setProfile}
            team={team}
            onOpenTeam={(thread) => setTeamOpen({ thread })}
          />
        ) : (
          <>
            <ImageCanvas image={null} />
            <aside className="inspector">
              <div className="panel-label">{text.inspector}</div>
              <p className="muted">{m1.noMedia}</p>
            </aside>
          </>
        )}
      </PanelLayout>
      <div className="interchange-bar">
        {(['coco', 'yoloSeg'] as const).map((format) => (
          <button
            key={format}
            disabled={busy || !profile}
            onClick={() =>
              void run(async () => {
                const value = await command('inspect_instances', { format });
                if (value)
                  setInspection({
                    value,
                    layerId: 'candidate_objects',
                    instances: true,
                  });
              })
            }
          >
            {format === 'coco' ? m2.importCoco : m2.importSeg}
          </button>
        ))}
        <button
          disabled={busy || !repairTarget}
          title={m2.repairHelp}
          onClick={() =>
            void run(async () => {
              if (await command('repair_media', { id: repairTarget })) {
                await install(
                  await command('select_media', { id: repairTarget }),
                );
                setRefresh((n) => n + 1);
              }
            })
          }
        >
          {m2.repair}
        </button>
        <button
          disabled={busy || !profile}
          onClick={() =>
            void run(async () => {
              const value = await command('inspect_yolo', undefined);
              if (value) setInspection({ value, layerId: 'candidate_objects' });
            })
          }
        >
          {m1.importLabels}
        </button>
        <select aria-label={m1.importMask} id="mask-layer">
          {profile?.layers
            .filter((l) => l.layerType !== 'instance')
            .map((l) => (
              <option key={l.id} value={l.id}>
                {l.displayName}
              </option>
            ))}
        </select>
        <button
          disabled={busy || !active}
          onClick={() =>
            void run(async () => {
              const layerId = (
                document.getElementById('mask-layer') as HTMLSelectElement
              ).value;
              const value = await command('inspect_mask', {
                mediaId: active!.image.info.id,
                layerId,
              });
              if (value) setInspection({ value, layerId });
            })
          }
        >
          {m1.importMask}
        </button>
        <button
          disabled={busy}
          title={prelabel.folderHelp}
          onClick={() =>
            void run(async () => {
              const layerId = (
                document.getElementById('mask-layer') as HTMLSelectElement
              ).value;
              const value = await command('inspect_mask_folder', { layerId });
              if (value) setInspection({ value, layerId, folder: true });
            })
          }
        >
          {prelabel.folder}
        </button>
        <button
          disabled={busy}
          title={prelabel.templateHelp}
          onClick={() =>
            void run(async () => {
              const value = await command('inspect_ignore_template', {
                tripId: null,
              });
              if (!value || !window.confirm(prelabel.confirmTemplate(value)))
                return;
              setReport(
                await command('apply_ignore_template', { token: value.token }),
              );
              await reloadActive();
            })
          }
        >
          {prelabel.template}
        </button>
        <span className="tool-spacer" />
        <span>{m1.classHotkeys}</span>
      </div>
      <footer className="statusbar">
        <span>
          <i />
          {busy ? text.working : m1[status]}
        </span>
        <span>{project.root}</span>
        <span>
          {team.status?.folder
            ? `${teamText.title}: ${team.status.userName}`
            : teamText.notConnected}
        </span>
      </footer>
      {teamOpen && (
        <TeamPanel
          api={team}
          currentSha={active?.image.info.sha256.toLowerCase() ?? null}
          initialThread={teamOpen.thread}
          onClose={() => setTeamOpen(null)}
          onOpenMedia={(id) => {
            setTeamOpen(null);
            select(id);
          }}
        />
      )}
      {aiOpen && active && profile && (
        <AiWorkspace
          image={active.image}
          profile={profile}
          session={active.session}
          busy={busy}
          onTask={(action) => run(action)}
          onAccepted={(doc) => {
            active.session.adopt(doc);
            setRefresh((n) => n + 1);
          }}
          onClose={() => setAiOpen(false)}
        />
      )}
      {reviewOpen && active && profile && (
        <ReviewWorkspace
          key={active.image.info.id}
          image={active.image}
          profile={profile}
          videoFrame={!!active.origin}
          busy={busy}
          progress={progress}
          registerExitGuard={registerReviewGuard}
          onTask={(action) => run(action)}
          onSelect={select}
          onChanged={(doc) => {
            active.session.adopt(doc);
            setRefresh((n) => n + 1);
          }}
          onClose={() => setReviewOpen(false)}
        />
      )}
      {toolsOpen && (
        <DatasetTools
          image={active?.image ?? null}
          videoFrame={!!active?.origin}
          busy={busy}
          progress={progress}
          onTask={(action) => run(action)}
          onClose={() => setToolsOpen(false)}
          onSelect={(id) => {
            setToolsOpen(false);
            select(id);
          }}
          onReview={(id) => {
            void run(async () => {
              const image = await command('select_media', { id });
              await install(image);
              setToolsOpen(false);
              setReviewOpen(true);
            });
          }}
        />
      )}
      {videoOpen && (
        <VideoWorkspace
          busy={busy}
          progress={progress}
          operationError={error?.message ?? null}
          onClose={() => setVideoOpen(false)}
          onTask={async (action) => {
            await run(async () => {
              const result = await action();
              if (
                result &&
                typeof result === 'object' &&
                'imported' in result
              ) {
                setReport(result);
                setRefresh((n) => n + 1);
              }
            });
          }}
        />
      )}
      {exportOpen && (
        <ExportDialog
          onTask={(action) => run(action, false)}
          onClose={() => setExportOpen(false)}
          onExport={exportData}
        />
      )}
      {inspection && profile && (
        <MappingDialog
          key={inspection.value.token}
          inspection={inspection.value}
          profile={profile}
          layerId={inspection.layerId}
          onClose={() => setInspection(null)}
          onApply={applyMapping}
          folder={
            inspection.folder && 'matched' in inspection.value
              ? inspection.value
              : undefined
          }
        />
      )}
      {report !== null && (
        <OperationReport value={report} onClose={() => setReport(null)} />
      )}
    </div>
  );
}
