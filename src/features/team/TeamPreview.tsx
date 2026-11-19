import { useEffect, useState } from 'react';
import { command } from '../../lib/backend';
import type { TeamComment, TeamMediaRef } from '../../lib/contracts';
import { team as t } from '../../app/teamStrings';

const cache = new Map<string, string | null>();
function remember(sha: string, url: string | null) {
  cache.set(sha, url);
  if (cache.size > 24) cache.delete(cache.keys().next().value!);
}

/** Shared JPEG preview with the asker's vector labels and pin drawn in image coordinates. */
export function TeamPreview({
  media,
  comment,
  showLabels,
}: {
  media: TeamMediaRef;
  comment: TeamComment | undefined;
  showLabels: boolean;
}) {
  const sha = media.sha256.toLowerCase();
  const [loaded, setLoaded] = useState<{ sha: string; url: string | null }>(
    () => ({ sha, url: cache.get(sha) ?? null }),
  );
  useEffect(() => {
    if (cache.get(sha)) return;
    let cancelled = false;
    void command('team_preview', { sha256: sha })
      .then((url) => {
        if (url) remember(sha, url);
        if (!cancelled) setLoaded({ sha, url });
      })
      .catch(() => {
        if (!cancelled) setLoaded({ sha, url: null });
      });
    return () => {
      cancelled = true;
    };
  }, [sha]);
  const url = cache.get(sha) ?? (loaded.sha === sha ? loaded.url : null);
  const stroke = Math.max(2, Math.max(media.width, media.height) / 400);
  return (
    <div className="team-preview">
      {url ? (
        <svg
          viewBox={`0 0 ${media.width} ${media.height}`}
          preserveAspectRatio="xMidYMid meet"
          role="img"
          aria-label={media.name}
        >
          <image href={url} width={media.width} height={media.height} />
          {showLabels &&
            comment?.labels.map((l, i) => {
              const [a, b] = l.points;
              return l.kind === 'box' && a && b ? (
                <rect
                  key={i}
                  x={Math.min(a[0], b[0])}
                  y={Math.min(a[1], b[1])}
                  width={Math.abs(b[0] - a[0])}
                  height={Math.abs(b[1] - a[1])}
                  fill="none"
                  stroke={l.color}
                  strokeWidth={stroke}
                >
                  <title>{l.name}</title>
                </rect>
              ) : (
                <polygon
                  key={i}
                  points={l.points.map((p) => p.join(',')).join(' ')}
                  fill={l.color}
                  fillOpacity={0.18}
                  stroke={l.color}
                  strokeWidth={stroke}
                >
                  <title>{l.name}</title>
                </polygon>
              );
            })}
          {comment?.pin && (
            <g transform={`translate(${comment.pin.x} ${comment.pin.y})`}>
              <circle
                r={stroke * 7}
                fill="#ff8a3d"
                fillOpacity={0.35}
                stroke="#fff"
                strokeWidth={stroke}
              />
              <circle r={stroke * 2} fill="#fff" />
            </g>
          )}
        </svg>
      ) : (
        <p className="muted">{t.noPreview}</p>
      )}
    </div>
  );
}
