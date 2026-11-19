import { useEffect, useState } from 'react';
import { useModalFocus } from '../../components/useModalFocus';
import { command, normalizeError } from '../../lib/backend';
import type { TeamThread } from '../../lib/contracts';
import { team as t } from '../../app/teamStrings';
import type { TeamApi } from './useTeamFeed';
import { TeamPreview } from './TeamPreview';
import { when } from './format';

type Filter = 'open' | 'resolved' | 'all' | 'mine';

export function TeamPanel({
  api,
  currentSha,
  initialThread,
  onOpenMedia,
  onClose,
}: {
  api: TeamApi;
  currentSha: string | null;
  initialThread: string | null;
  onOpenMedia: (id: string) => void;
  onClose: () => void;
}) {
  const modal = useModalFocus();
  const { status, feed } = api;
  const [filter, setFilter] = useState<Filter>('open'),
    [onlyThis, setOnlyThis] = useState(false),
    [selectedId, setSelectedId] = useState<string | null>(initialThread),
    [reply, setReply] = useState(''),
    [name, setName] = useState(status?.userName ?? ''),
    [settings, setSettings] = useState(!status?.folder),
    [showLabels, setShowLabels] = useState(true),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<string | null>(null);
  const me = status?.userId;
  const threads = feed.threads.filter(
    (th) =>
      (!onlyThis || th.media.sha256.toLowerCase() === currentSha) &&
      (filter === 'all' ||
        (filter === 'mine'
          ? th.comments.some((c) => c.author.id === me)
          : th.status === filter)),
  );
  const selected =
    feed.threads.find((th) => th.id === selectedId) ??
    (selectedId ? undefined : threads[0]);
  const { markSeen } = api;
  useEffect(() => {
    if (selected) markSeen(selected);
  }, [selected, markSeen]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (
        e.key === 'Escape' &&
        [...document.querySelectorAll('[aria-modal="true"]')].at(-1) ===
          modal.current
      )
        onClose();
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, [modal, onClose]);

  async function act(action: () => Promise<unknown>) {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (cause) {
      setError(normalizeError(cause).message);
    } finally {
      setBusy(false);
    }
  }
  const configure = (folder: 'keep' | 'pick' | 'clear') =>
    act(async () => {
      const next = await command('team_configure', {
        userName: name,
        folder,
      });
      if (next) {
        api.setStatus(next);
        if (next.folder) setSettings(false);
      }
    });
  const send = (thread: TeamThread) =>
    act(async () => {
      await command('team_post', {
        input: {
          threadId: thread.id,
          mediaId: null,
          body: reply,
          pin: null,
          includeLabels: false,
        },
      });
      setReply('');
      const next = await api.refresh();
      const updated = next.threads.find((x) => x.id === thread.id);
      if (updated) markSeen(updated);
    });
  const toggle = (thread: TeamThread) =>
    act(async () => {
      await command('team_set_status', {
        threadId: thread.id,
        resolved: thread.status === 'open',
      });
      const next = await api.refresh();
      const updated = next.threads.find((x) => x.id === thread.id);
      if (updated) markSeen(updated);
    });

  return (
    <div className="modal-backdrop">
      <section
        ref={modal}
        role="dialog"
        aria-modal="true"
        aria-label={t.title}
        className="dialog team-dialog"
      >
        <header className="team-header">
          <h2>{t.title}</h2>
          <span className="tool-spacer" />
          {status?.folder && (
            <button disabled={busy} onClick={() => setSettings((v) => !v)}>
              {t.setupTitle}
            </button>
          )}
          <button disabled={busy} onClick={() => void api.refresh()}>
            {t.refresh}
          </button>
          <button onClick={onClose}>{t.close}</button>
        </header>
        {error && (
          <p role="alert" className="team-error">
            {error}
          </p>
        )}
        {api.error && status?.folder && (
          <p role="alert" className="team-error">
            {api.error}
          </p>
        )}
        {settings && (
          <fieldset className="team-setup">
            <legend>{t.setupTitle}</legend>
            <p className="review-help">{t.setupHelp}</p>
            <label>
              {t.yourName}
              <input
                value={name}
                maxLength={80}
                disabled={busy}
                onChange={(e) => setName(e.target.value)}
              />
            </label>
            <p className="source-path">{status?.folder ?? t.notConnected}</p>
            <div className="team-row">
              <button
                className="primary"
                disabled={busy || !name.trim()}
                onClick={() => void configure('pick')}
              >
                {status?.folder ? t.changeFolder : t.chooseFolder}
              </button>
              {status?.folder && (
                <>
                  <button
                    disabled={busy || !name.trim()}
                    onClick={() => void configure('keep')}
                  >
                    {t.saveName}
                  </button>
                  <button
                    disabled={busy}
                    onClick={() => void configure('clear')}
                  >
                    {t.disconnect}
                  </button>
                </>
              )}
            </div>
            <p className="review-help">{t.syncNote}</p>
          </fieldset>
        )}
        {status?.folder && (
          <div className="team-body">
            <div className="team-list">
              <div className="team-filters" role="tablist">
                {(['open', 'resolved', 'mine', 'all'] as const).map((f) => (
                  <button
                    key={f}
                    role="tab"
                    aria-selected={filter === f}
                    className={filter === f ? 'active' : ''}
                    onClick={() => setFilter(f)}
                  >
                    {t[f]}
                  </button>
                ))}
              </div>
              {currentSha && (
                <label className="checkbox">
                  <input
                    type="checkbox"
                    checked={onlyThis}
                    onChange={(e) => setOnlyThis(e.target.checked)}
                  />
                  {t.thisImage}
                </label>
              )}
              {!threads.length && <p className="muted">{t.noThreads}</p>}
              {threads.map((th) => (
                <button
                  key={th.id}
                  className={`team-thread ${th.id === selected?.id ? 'active' : ''}`}
                  onClick={() => setSelectedId(th.id)}
                >
                  <span className="team-thread-title">
                    {api.isUnread(th) && <i className="team-dot" />}
                    {th.comments[0]?.body.split('\n')[0] ?? th.media.name}
                  </span>
                  <small>
                    {th.comments[0]?.author.name} · {th.media.name} ·{' '}
                    {t.replies(th.comments.length)} · {when(th.updatedAtMs)}
                    {th.status === 'resolved' && ` · ${t.resolved}`}
                  </small>
                </button>
              ))}
              {feed.warnings.map((w) => (
                <p key={w} className="review-help">
                  {w}
                </p>
              ))}
            </div>
            <div className="team-detail">
              {selected ? (
                <>
                  <TeamPreview
                    media={selected.media}
                    comment={selected.comments[0]}
                    showLabels={showLabels}
                  />
                  <div className="team-row">
                    <label className="checkbox">
                      <input
                        type="checkbox"
                        checked={showLabels}
                        onChange={(e) => setShowLabels(e.target.checked)}
                      />
                      {t.showLabels}
                    </label>
                    <span className="tool-spacer" />
                    {selected.localMediaId ? (
                      <button
                        className="primary"
                        disabled={busy}
                        onClick={() => onOpenMedia(selected.localMediaId!)}
                      >
                        {t.openImage}
                      </button>
                    ) : (
                      <small className="muted">{t.notInProject}</small>
                    )}
                    <button
                      disabled={busy}
                      onClick={() => void toggle(selected)}
                    >
                      {selected.status === 'open' ? t.resolve : t.reopen}
                    </button>
                  </div>
                  {selected.status === 'resolved' && selected.statusBy && (
                    <p className="review-help">
                      {t.resolvedBy(selected.statusBy.name)}
                    </p>
                  )}
                  <ol className="team-comments">
                    {selected.comments.map((c) => (
                      <li
                        key={c.id}
                        className={c.author.id === me ? 'mine' : ''}
                      >
                        <div>
                          <strong>{c.author.name}</strong>
                          <small>
                            {when(c.createdAtMs)}
                            {c.project && ` · ${c.project}`}
                          </small>
                        </div>
                        <p>{c.body}</p>
                      </li>
                    ))}
                  </ol>
                  <textarea
                    aria-label={t.reply}
                    placeholder={t.reply}
                    value={reply}
                    maxLength={4000}
                    disabled={busy}
                    onChange={(e) => setReply(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' && e.ctrlKey && reply.trim())
                        void send(selected);
                    }}
                  />
                  <div className="team-row">
                    <span className="tool-spacer" />
                    <button
                      className="primary"
                      disabled={busy || !reply.trim()}
                      onClick={() => void send(selected)}
                    >
                      {t.send}
                    </button>
                  </div>
                </>
              ) : (
                <p className="muted">{t.noThreads}</p>
              )}
            </div>
          </div>
        )}
      </section>
    </div>
  );
}
