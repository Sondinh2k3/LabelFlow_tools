import { useModalFocus } from '../../components/useModalFocus';
import { useEffect, useRef, useState } from 'react';
import { command, normalizeError } from '../../lib/backend';
import type {
  ExtractionOptions,
  FrameInfo,
  VideoInfo,
  Progress,
} from '../../lib/contracts';
import { m2 } from '../../app/m2Strings';
import { m1 } from '../../app/m1Strings';

export function VideoWorkspace({
  busy,
  progress,
  operationError,
  onTask,
  onClose,
}: {
  busy: boolean;
  progress: Progress | null;
  operationError: string | null;
  onTask: (action: () => Promise<unknown>) => Promise<void>;
  onClose: () => void;
}) {
  const modal = useModalFocus();
  const [videos, setVideos] = useState<VideoInfo[]>([]);
  const [selected, setSelected] = useState('');
  const [localError, setLocalError] = useState('');
  const [seekTime, setSeekTime] = useState(0);
  const [playing, setPlaying] = useState(false);
  const exact = useRef(false);
  const [tools, setTools] = useState<{
    available: boolean;
    version: string | null;
    error: string | null;
  } | null>(null);
  const [index, setIndex] = useState(0),
    [frame, setFrame] = useState<FrameInfo | null>(null);
  const [preview, setPreview] = useState(''),
    [stream, setStream] = useState('');
  const [group, setGroup] = useState(''),
    [playbackError, setPlaybackError] = useState(false);
  const [options, setOptions] = useState<ExtractionOptions>({
    videoId: '',
    indices: [],
    startMs: 0,
    endMs: 0,
    intervalMs: 1000,
    format: 'png',
    quality: 95,
  });
  const request = useRef(0),
    player = useRef<HTMLVideoElement>(null);
  const video = videos.find((v) => v.id === selected);
  function chooseVideo(id: string, list = videos) {
    request.current++;
    void command('cancel_job', undefined).catch((cause) =>
      setLocalError(normalizeError(cause).message),
    );
    exact.current = false;
    player.current?.pause();
    setPlaying(false);
    setSelected(id);
    setIndex(0);
    setFrame(null);
    setPreview('');
    setStream('');
    setLocalError('');
    setPlaybackError(false);
    const next = list.find((v) => v.id === id);
    setGroup(next?.splitGroup ?? '');
    if (next)
      setOptions((o) => ({
        ...o,
        videoId: id,
        startMs: 0,
        endMs: Math.max(0, next.durationMs - 1),
      }));
  }
  useEffect(() => {
    void (async () => {
      setTools(await command('video_tools', undefined));
      const list = await command('list_videos', undefined);
      setVideos(list);
      chooseVideo(list[0]?.id ?? '', list);
    })().catch((cause) => setLocalError(normalizeError(cause).message));
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (!video) return;
    let cancelled = false;
    void (async () => {
      const source = await command('video_stream', { id: video.id });
      const page = await command('video_frames', {
        id: video.id,
        offset: 0,
        limit: 1,
      });
      if (!cancelled) {
        setGroup(video.splitGroup);
        setIndex(0);
        setFrame(null);
        setPreview('');
        setStream('');
        setPlaybackError(false);
        setOptions((o) => ({
          ...o,
          videoId: video.id,
          startMs: 0,
          endMs: Math.max(0, video.durationMs - 1),
        }));

        setStream(source);
        setFrame(page.frames[0] ?? null);
      }
    })().catch((cause) => {
      if (!cancelled) setLocalError(normalizeError(cause).message);
    });
    return () => {
      cancelled = true;
    };
  }, [selected]); // eslint-disable-line react-hooks/exhaustive-deps
  async function show(at: number) {
    if (!video || busy) return;
    exact.current = true;
    const target = Math.max(0, Math.min(video.frameCount - 1, at));
    const serial = ++request.current;
    player.current?.pause();
    await onTask(async () => {
      const result = await command('preview_frame', {
        id: video.id,
        index: target,
      });
      if (serial === request.current) {
        setIndex(target);
        setFrame(result.frame);
        setPreview(result.dataUrl);
        if (player.current)
          player.current.currentTime = result.frame.timeMs / 1000;
      }
    });
    // Warm adjacent exact frames without changing the visible frame or active media.
    for (const neighbor of [target + 1, target - 1]) {
      if (
        serial !== request.current ||
        neighbor < 0 ||
        neighbor >= video.frameCount
      )
        break;
      try {
        await command('preview_frame', { id: video.id, index: neighbor });
      } catch {
        break;
      }
    }
  }
  useEffect(() => {
    const generation = request;
    return () => {
      generation.current++;
    };
  }, []);
  function playbackPosition() {
    if (
      !video ||
      !player.current?.paused ||
      exact.current ||
      player.current.dataset.videoId !== video.id
    )
      return;
    const timeMs = Math.max(0, player.current.currentTime * 1000),
      serial = request.current;
    void command('seek_video', { id: video.id, timeMs })
      .then((f) => {
        if (
          serial === request.current &&
          !exact.current &&
          player.current?.dataset.videoId === video.id
        ) {
          setFrame(f);
          setIndex(f.index);
        }
      })
      .catch((cause) => setLocalError(normalizeError(cause).message));
  }
  function chooseIndex(value: number) {
    request.current++;
    exact.current = true;
    player.current?.pause();
    setIndex(value);
    setFrame(null);
    setPreview('');
  }
  return (
    <div className="modal-backdrop">
      <section
        ref={modal}
        className="dialog video-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={m2.video}
      >
        <h2>{m2.video}</h2>
        {(localError || operationError) && (
          <p role="alert">{localError || operationError}</p>
        )}
        <p>{m2.timing}</p>
        {!tools?.available && (
          <p role="status">
            {m2.toolsMissing}
            <br />
            {tools?.error}
          </p>
        )}
        <div className="field-row">
          <button
            disabled={busy}
            onClick={() =>
              void onTask(async () => {
                await command('configure_video_tools', undefined);
                setTools(await command('video_tools', undefined));
              })
            }
          >
            {m2.configure}
          </button>
          <button
            disabled={busy || !tools?.available}
            onClick={() =>
              void onTask(async () => {
                const added = await command('open_video', undefined);
                if (added) {
                  const list = await command('list_videos', undefined);
                  setVideos(list);
                  chooseVideo(added.id, list);
                }
              })
            }
          >
            {m2.importVideo}
          </button>
        </div>
        {tools?.version && <small>{tools.version.split('\n')[0]}</small>}
        <select
          aria-label={m2.video}
          value={selected}
          disabled={busy}
          onChange={(e) => chooseVideo(e.target.value)}
        >
          <option value="">{m2.noVideos}</option>
          {videos.map((v) => (
            <option key={v.id} value={v.id}>
              {v.name}
            </option>
          ))}
        </select>
        {video && (
          <>
            <button
              disabled={busy}
              title={m2.repairHelp}
              onClick={() =>
                void onTask(async () => {
                  if (await command('repair_video', { id: video.id })) {
                    setStream(await command('video_stream', { id: video.id }));
                    setLocalError('');
                    setVideos(await command('list_videos', undefined));
                  }
                })
              }
            >
              {m2.repairVideo}
            </button>
            <label>
              {m2.seekTime}
              <input
                type="number"
                min={0}
                max={video.durationMs}
                value={seekTime}
                disabled={busy}
                onChange={(e) => setSeekTime(Number(e.target.value))}
              />
            </label>
            <button
              disabled={busy}
              onClick={() => {
                void command('seek_video', { id: video.id, timeMs: seekTime })
                  .then((f) => show(f.index))
                  .catch((cause) =>
                    setLocalError(normalizeError(cause).message),
                  );
              }}
            >
              {m2.seek}
            </button>
            <div className="video-preview">
              {stream && (
                <video
                  ref={player}
                  data-video-id={video.id}
                  src={stream}
                  controls
                  preload="metadata"
                  onPlay={() => {
                    exact.current = false;
                    setPreview('');
                    setPlaying(true);
                  }}
                  onPause={() => {
                    setPlaying(false);
                    playbackPosition();
                  }}
                  onError={() => setPlaybackError(true)}
                  onSeeked={playbackPosition}
                />
              )}
              {preview && <img src={preview} alt={`${m2.frame} ${index}`} />}
            </div>
            {playbackError && <p>{m2.playbackError}</p>}
            <div className="project-meta">
              {video.codec} · {video.width} × {video.height} ·{' '}
              {video.frameCount} frames · {video.durationMs.toFixed(3)} ms ·
              time base {video.timeBaseNum}/{video.timeBaseDen}
            </div>
            <label>
              {m2.frame}
              <input
                aria-label={m2.frame}
                type="number"
                min={0}
                max={video.frameCount - 1}
                value={index}
                disabled={busy}
                onChange={(e) =>
                  chooseIndex(
                    Math.max(
                      0,
                      Math.min(
                        video.frameCount - 1,
                        Math.trunc(Number(e.target.value) || 0),
                      ),
                    ),
                  )
                }
              />
            </label>
            <input
              onPointerUp={() => void show(index)}
              onKeyUp={(e) => {
                if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key))
                  void show(index);
              }}
              aria-label="Frame timeline"
              type="range"
              min={0}
              max={video.frameCount - 1}
              step={1}
              value={index}
              disabled={busy}
              onChange={(e) => chooseIndex(Number(e.target.value))}
            />
            <div className="field-row">
              <button disabled={busy} onClick={() => void show(index - 1)}>
                {m2.previous}
              </button>
              <button disabled={busy} onClick={() => void show(index)}>
                {m2.exact}
              </button>
              <button disabled={busy} onClick={() => void show(index + 1)}>
                {m2.next}
              </button>
            </div>
            <output>
              PTS {frame?.pts ?? '—'} · {frame?.timeMs.toFixed(3) ?? '—'} ms
            </output>
            <label>
              {m2.group}
              <input
                value={group}
                maxLength={512}
                disabled={busy}
                onChange={(e) => setGroup(e.target.value)}
              />
            </label>
            <button
              disabled={busy || !group.trim()}
              onClick={() =>
                void onTask(async () => {
                  await command('set_video_group', { id: video.id, group });
                  setVideos(await command('list_videos', undefined));
                })
              }
            >
              {m2.saveGroup}
            </button>
            <div className="field-row">
              {(['startMs', 'endMs', 'intervalMs'] as const).map((field, i) => (
                <label key={field}>
                  {[m2.start, m2.end, m2.every][i]}
                  <input
                    type="number"
                    min={field === 'intervalMs' ? 1 : 0}
                    value={options[field] ?? 1000}
                    disabled={busy}
                    onChange={(e) =>
                      setOptions({
                        ...options,
                        [field]: Number(e.target.value),
                      })
                    }
                  />
                </label>
              ))}
            </div>
            <div className="field-row">
              <label>
                {m2.format}
                <select
                  value={options.format}
                  disabled={busy}
                  onChange={(e) =>
                    setOptions({
                      ...options,
                      format: e.target.value as 'png' | 'jpeg',
                    })
                  }
                >
                  <option value="png">PNG (lossless)</option>
                  <option value="jpeg">JPEG</option>
                </select>
              </label>
              <label>
                {m2.quality}
                <input
                  type="number"
                  min={1}
                  max={100}
                  value={options.quality}
                  disabled={busy || options.format === 'png'}
                  onChange={(e) =>
                    setOptions({ ...options, quality: Number(e.target.value) })
                  }
                />
              </label>
            </div>
            <div className="field-row">
              <button
                disabled={busy || playing || !tools?.available}
                onClick={() =>
                  void onTask(() =>
                    command('extract_frames', {
                      options: {
                        ...options,
                        videoId: video.id,
                        indices: [index],
                        intervalMs: null,
                      },
                    }),
                  )
                }
              >
                {m2.extract}
              </button>
              <button
                disabled={busy || !tools?.available}
                onClick={() =>
                  void onTask(() =>
                    command('extract_frames', {
                      options: { ...options, videoId: video.id, indices: [] },
                    }),
                  )
                }
              >
                {m2.interval}
              </button>
            </div>
          </>
        )}
        {busy && (
          <p role="status">
            {progress
              ? `${progress.completed} / ${progress.total} · ${progress.message}`
              : m2.working}
          </p>
        )}
        <div className="dialog-actions">
          {busy && (
            <button onClick={() => void command('cancel_job', undefined)}>
              {m1.cancel}
            </button>
          )}
          <button disabled={busy} onClick={onClose}>
            {m1.close}
          </button>
        </div>
      </section>
    </div>
  );
}
