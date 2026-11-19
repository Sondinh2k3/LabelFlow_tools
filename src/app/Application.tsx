import { useEffect, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { text } from './strings';
import { m1 } from './m1Strings';
import { projects } from '../features/projects/service';
import { command, normalizeError } from '../lib/backend';
import type { AppError, Project } from '../lib/contracts';
import { Workspace } from './Workspace';
import { classStrings } from './teamStrings';
const SET_KEY = 'labelflow-semantic-set',
  LIST_KEY = 'labelflow-semantic-list';
function stored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
function remember(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* convenience only */
  }
}
export function App() {
  const [project, setProject] = useState<Project | null>(null),
    [name, setName] = useState<string>(text.defaultProjectName),
    [profile, setProfile] = useState('road-encroachment-v1'),
    [classSet, setClassSet] = useState<'cityscapes' | 'custom'>(() =>
      stored(SET_KEY) === 'custom' ? 'custom' : 'cityscapes',
    ),
    [classList, setClassList] = useState(() => stored(LIST_KEY) ?? ''),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<AppError | null>(null);
  useEffect(() => {
    if (!isTauri()) return;
    void command('current_project', undefined)
      .then((selected) => {
        if (selected) setProject(selected);
      })
      .catch((cause) => setError(normalizeError(cause)));
  }, []);
  async function select(action: 'create' | 'open' | 'restore') {
    setBusy(true);
    setError(null);
    try {
      const custom =
        classSet === 'custom'
          ? classList
              .split('\n')
              .map((l) => l.trim())
              .filter(Boolean)
          : undefined;
      if (action === 'create') {
        remember(SET_KEY, classSet);
        if (custom) remember(LIST_KEY, classList);
      }
      const selected = await (action === 'create'
        ? projects.create(name, profile, custom)
        : action === 'open'
          ? projects.open()
          : command('restore_backup', undefined));
      if (selected) setProject(selected);
    } catch (cause) {
      setError(normalizeError(cause));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="app-shell">
      <header className="app-header">
        <div className="brand">
          <span className="brand-icon">
            L<span>F</span>
          </span>
          <strong>{text.app}</strong>
        </div>
        <span className="offline">
          <i />
          {text.offline}
        </span>
      </header>
      {error && (
        <div className="error-banner" role="alert">
          <span>
            {error.code}: {error.message}
          </span>
          <button aria-label={text.closeError} onClick={() => setError(null)}>
            ×
          </button>
        </div>
      )}
      {project ? (
        <Workspace
          key={project.id}
          project={project}
          onClose={() => setProject(null)}
        />
      ) : (
        <main className="home">
          <h1>{text.welcome}</h1>
          <p className="intro">{text.intro}</p>
          <section className="project-card">
            <label htmlFor="project-name">{text.projectName}</label>
            <input
              id="project-name"
              value={name}
              maxLength={120}
              disabled={busy}
              onChange={(e) => setName(e.target.value)}
            />
            <label htmlFor="profile">{m1.profile}</label>
            <select
              id="profile"
              value={profile}
              disabled={busy}
              onChange={(e) => setProfile(e.target.value)}
            >
              <option value="road-encroachment-v1">{m1.road}</option>
              <option value="generic-v1">{m1.generic}</option>
            </select>
            <label htmlFor="semantic-set">{classStrings.semanticSet}</label>
            <select
              id="semantic-set"
              value={classSet}
              disabled={busy}
              onChange={(e) =>
                setClassSet(
                  e.target.value === 'custom' ? 'custom' : 'cityscapes',
                )
              }
            >
              <option value="cityscapes">{classStrings.cityscapes}</option>
              <option value="custom">{classStrings.custom}</option>
            </select>
            {classSet === 'custom' && (
              <>
                <textarea
                  aria-label={classStrings.custom}
                  className="class-list-input"
                  rows={8}
                  value={classList}
                  disabled={busy}
                  placeholder={'Road\nCurb\nSidewalk\nVehicle\nPerson'}
                  onChange={(e) => setClassList(e.target.value)}
                />
                <p className="review-help">{classStrings.customHelp}</p>
              </>
            )}
            <div className="project-actions">
              <button
                className="primary"
                aria-label={text.newProject}
                disabled={
                  busy ||
                  !name.trim() ||
                  (classSet === 'custom' && !classList.trim())
                }
                onClick={() => void select('create')}
              >
                {text.newProject}
                <span>＋</span>
              </button>
              <button
                aria-label={text.openProject}
                disabled={busy}
                onClick={() => void select('open')}
              >
                {text.openProject}
                <span>↗</span>
              </button>
            </div>
            <button
              className="restore-button"
              disabled={busy}
              onClick={() => void select('restore')}
            >
              {m1.restore}
            </button>
            <p>{text.browse}</p>
          </section>
          <p className="privacy">◇ {text.localNote}</p>
        </main>
      )}
      {!project && (
        <footer className="statusbar">
          <span>
            <i />
            {busy ? text.working : text.ready}
          </span>
          <span>{text.localNote}</span>
        </footer>
      )}
    </div>
  );
}
