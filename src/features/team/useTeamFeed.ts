import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { command, normalizeError } from '../../lib/backend';
import type { TeamFeed, TeamStatus, TeamThread } from '../../lib/contracts';

const POLL_MS = 20_000;
const empty: TeamFeed = { threads: [], warnings: [] };

function readSeen(key: string): Record<string, number> {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(key) ?? '{}');
    return value && typeof value === 'object'
      ? (value as Record<string, number>)
      : {};
  } catch {
    return {};
  }
}

export interface TeamApi {
  status: TeamStatus | null;
  feed: TeamFeed;
  error: string | null;
  refresh: () => Promise<TeamFeed>;
  setStatus: (status: TeamStatus) => void;
  isUnread: (thread: TeamThread) => boolean;
  markSeen: (thread: TeamThread) => void;
  unread: number;
  /** content SHA-256 → open/resolved, for thumbnails and the canvas. */
  bySha: Map<string, TeamThread[]>;
}

/** Polls the shared folder while a project is open. Failures never block labeling. */
export function useTeamFeed(projectId: string): TeamApi {
  const seenKey = `labelflow-team-seen:${projectId}`;
  const [status, setStatus] = useState<TeamStatus | null>(null),
    [feed, setFeed] = useState<TeamFeed>(empty),
    [error, setError] = useState<string | null>(null),
    [seen, setSeen] = useState<Record<string, number>>(() => readSeen(seenKey));
  const statusRef = useRef(status);
  useLayoutEffect(() => {
    statusRef.current = status;
  }, [status]);
  const refresh = useCallback(async () => {
    if (!statusRef.current?.folder) {
      setFeed(empty);
      return empty;
    }
    try {
      const next = await command('team_threads', undefined);
      setFeed(next);
      setError(null);
      return next;
    } catch (cause) {
      setError(normalizeError(cause).message);
      return empty;
    }
  }, []);
  useEffect(() => {
    let cancelled = false;
    void command('team_status', undefined)
      .then((value) => {
        if (!cancelled) setStatus(value);
      })
      .catch(() => {
        /* Team discussion is optional. */
      });
    return () => {
      cancelled = true;
    };
  }, [projectId]);
  useEffect(() => {
    if (!status?.folder) return;
    const first = window.setTimeout(() => void refresh(), 0);
    const timer = window.setInterval(() => {
      if (document.visibilityState === 'visible') void refresh();
    }, POLL_MS);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(timer);
    };
  }, [status?.folder, refresh]);
  const markSeen = useCallback(
    (thread: TeamThread) =>
      setSeen((old) => {
        if (old[thread.id] === thread.activity) return old;
        const next = { ...old, [thread.id]: thread.activity };
        try {
          localStorage.setItem(seenKey, JSON.stringify(next));
        } catch {
          /* Unread markers are a convenience only. */
        }
        return next;
      }),
    [seenKey],
  );
  const isUnread = useCallback(
    (thread: TeamThread) => (seen[thread.id] ?? 0) < thread.activity,
    [seen],
  );
  const connected = status?.folder ? feed : empty;
  const bySha = useMemo(() => {
    const map = new Map<string, TeamThread[]>();
    for (const t of connected.threads) {
      const key = t.media.sha256.toLowerCase();
      map.set(key, [...(map.get(key) ?? []), t]);
    }
    return map;
  }, [connected]);
  const unread = connected.threads.filter(isUnread).length;
  return {
    status,
    feed: connected,
    error,
    refresh,
    setStatus,
    isUnread,
    markSeen,
    unread,
    bySha,
  };
}
