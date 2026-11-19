import { useEffect, useState } from 'react';
import { useModalFocus } from '../../components/useModalFocus';
import { command, normalizeError } from '../../lib/backend';
import type { Profile } from '../../lib/contracts';
import { classStrings as t } from '../../app/teamStrings';

const palette: readonly string[] = [
  '#ff6b6b',
  '#ffa94d',
  '#ffd43b',
  '#69db7c',
  '#38d9a9',
  '#4dabf7',
  '#748ffc',
  '#da77f2',
  '#f783ac',
  '#a9e34b',
];

/** Add, rename, recolor and (when unused) delete object or semantic classes of the open project. */
export function ClassManager({
  profile,
  namespace,
  onProfile,
  onClose,
}: {
  profile: Profile;
  namespace: string;
  onProfile: (profile: Profile) => void;
  onClose: () => void;
}) {
  const modal = useModalFocus();
  const semantic =
    profile.layers.find((l) => l.namespace === namespace)?.layerType ===
    'semantic';
  const classes = profile.classes.filter(
    (c) => c.namespace === namespace && !c.isIgnore,
  );
  const [drafts, setDrafts] = useState<
      Record<string, { name: string; color: string }>
    >({}),
    [name, setName] = useState(''),
    [color, setColor] = useState(
      palette[classes.length % palette.length] ?? '#4dabf7',
    ),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<string | null>(null);
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
  async function act(action: () => Promise<Profile>) {
    setBusy(true);
    setError(null);
    try {
      onProfile(await action());
      return true;
    } catch (cause) {
      setError(normalizeError(cause).message);
      return false;
    } finally {
      setBusy(false);
    }
  }
  const add = async () => {
    if (
      await act(() =>
        command('save_class', { id: null, name, color, namespace }),
      )
    ) {
      setName('');
      setColor(palette[(classes.length + 1) % palette.length] ?? '#4dabf7');
    }
  };
  return (
    <div className="modal-backdrop">
      <section
        ref={modal}
        role="dialog"
        aria-modal="true"
        aria-label={semantic ? t.semanticTitle : t.title}
        className="dialog class-dialog"
      >
        <h2>{semantic ? t.semanticTitle : t.title}</h2>
        <p className="review-help">{semantic ? t.semanticHelp : t.help}</p>
        {error && (
          <p role="alert" className="team-error">
            {error}
          </p>
        )}
        <div className="class-editor">
          {classes.map((c) => {
            const draft = drafts[c.id] ?? {
              name: c.displayName,
              color: c.color,
            };
            const changed =
              draft.name !== c.displayName || draft.color !== c.color;
            return (
              <div className="class-row" key={c.id}>
                <span className="class-id">{c.exportId}</span>
                <input
                  type="color"
                  aria-label={`${t.color} ${c.displayName}`}
                  value={draft.color}
                  disabled={busy}
                  onChange={(e) =>
                    setDrafts({
                      ...drafts,
                      [c.id]: { ...draft, color: e.target.value },
                    })
                  }
                />
                <input
                  aria-label={`${t.name} ${c.displayName}`}
                  value={draft.name}
                  maxLength={64}
                  disabled={busy}
                  onChange={(e) =>
                    setDrafts({
                      ...drafts,
                      [c.id]: { ...draft, name: e.target.value },
                    })
                  }
                />
                <button
                  disabled={busy || !changed || !draft.name.trim()}
                  onClick={() =>
                    void act(() =>
                      command('save_class', {
                        id: c.id,
                        name: draft.name,
                        color: draft.color,
                      }),
                    ).then((ok) => {
                      if (ok)
                        setDrafts((old) => {
                          const next = { ...old };
                          delete next[c.id];
                          return next;
                        });
                    })
                  }
                >
                  {t.save}
                </button>
                <button
                  disabled={busy || classes.length <= 1}
                  onClick={() => {
                    if (window.confirm(t.confirmDelete(c.displayName)))
                      void act(() => command('delete_class', { id: c.id }));
                  }}
                >
                  {t.delete}
                </button>
              </div>
            );
          })}
          <form
            className="class-row"
            onSubmit={(e) => {
              e.preventDefault();
              if (name.trim()) void add();
            }}
          >
            <span className="class-id">＋</span>
            <input
              type="color"
              aria-label={t.color}
              value={color}
              disabled={busy}
              onChange={(e) => setColor(e.target.value)}
            />
            <input
              aria-label={t.newName}
              placeholder={t.newName}
              value={name}
              maxLength={64}
              disabled={busy}
              onChange={(e) => setName(e.target.value)}
            />
            <button
              type="submit"
              className="primary"
              disabled={busy || !name.trim()}
            >
              {t.add}
            </button>
          </form>
        </div>
        <div className="team-row">
          <span className="tool-spacer" />
          <button onClick={onClose}>{t.close}</button>
        </div>
      </section>
    </div>
  );
}
