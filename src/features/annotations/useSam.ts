import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import { command, normalizeError } from '../../lib/backend';
import type { SamModel } from '../../lib/contracts';
import { decodeRuns } from './geometry/raster';
import type { EditorController, SamPrompt } from './tools/controller';

export type SamState =
  'unconfigured' | 'stopped' | 'starting' | 'ready' | 'encoding' | 'thinking';

interface Job {
  seq: number;
  mediaId: string;
  prompt: SamPrompt | null; // null = encode only (prefetch)
}

/**
 * Drives the optional click-to-mask worker. Requests are serialized (the worker is single
 * threaded) and only the newest prompt is sent: clicks made while SAM is busy collapse
 * into one request, so the editor never waits on stale masks.
 */
export function useSam(
  controller: EditorController,
  mediaId: string,
  active: boolean,
  onError: (error: unknown) => void,
) {
  const [state, setState] = useState<SamState>('stopped'),
    [model, setModel] = useState<SamModel | null>(null),
    [lastMs, setLastMs] = useState<number | null>(null);
  const seq = useRef(0),
    busy = useRef(false),
    pending = useRef<Job | null>(null),
    media = useRef(mediaId),
    ready = useRef(false),
    encoded = useRef(new Set<string>()),
    report = useRef(onError),
    pumpRef = useRef<() => void>(() => {});
  useLayoutEffect(() => {
    media.current = mediaId;
    ready.current = !!model;
    report.current = onError;
  });

  useEffect(() => {
    let alive = true;
    void command('sam_status', undefined)
      .then((s) => {
        if (!alive) return;
        setModel(s.model);
        setState(s.model ? 'ready' : s.config ? 'stopped' : 'unconfigured');
      })
      .catch(() => alive && setState('unconfigured'));
    return () => {
      alive = false;
    };
  }, []);

  const pump = useCallback(() => {
    if (busy.current || !pending.current) return;
    const job = pending.current;
    pending.current = null;
    busy.current = true;
    const encodeOnly = !job.prompt;
    setState(encoded.current.has(job.mediaId) ? 'thinking' : 'encoding');
    void command('sam_segment', {
      mediaId: job.mediaId,
      points: job.prompt?.points ?? [],
      region: job.prompt?.box ?? null,
      encodeOnly,
    })
      .then((result) => {
        encoded.current.add(job.mediaId);
        if (encoded.current.size > 8)
          encoded.current.delete(encoded.current.values().next().value!);
        if (
          encodeOnly ||
          job.seq !== seq.current ||
          job.mediaId !== media.current ||
          !result.runs
        )
          return;
        setLastMs(result.milliseconds ?? null);
        controller.setSamMask(
          decodeRuns({
            layerId: 'sam',
            width: result.width!,
            height: result.height!,
            runs: result.runs,
          }),
        );
      })
      .catch((cause) => {
        const error = normalizeError(cause);
        // A failed request drops the worker; reflect that so the user can restart it.
        void command('sam_status', undefined).then((s) => {
          setModel(s.model);
          if (!s.model) setState(s.config ? 'stopped' : 'unconfigured');
        });
        if (job.seq === seq.current) report.current(error);
      })
      .finally(() => {
        busy.current = false;
        setState((s) =>
          s === 'encoding' || s === 'thinking'
            ? ready.current
              ? 'ready'
              : 'stopped'
            : s,
        );
        pumpRef.current();
      });
  }, [controller]);
  useLayoutEffect(() => {
    pumpRef.current = pump;
  }, [pump]);

  const request = useCallback(
    (prompt: SamPrompt) => {
      seq.current++;
      if (!prompt.points.length && !prompt.box) {
        controller.setSamMask(null);
        if (pending.current?.prompt) pending.current = null;
        return;
      }
      if (!ready.current) {
        onError(Error('Start SAM first (right panel → Click-to-mask).'));
        return;
      }
      pending.current = { seq: seq.current, mediaId: media.current, prompt };
      pump();
    },
    [controller, onError, pump],
  );

  // Encode the image as soon as SAM is the active tool, so the first click is fast.
  useEffect(() => {
    if (!active || !model || encoded.current.has(mediaId)) return;
    if (!pending.current)
      pending.current = { seq: seq.current, mediaId, prompt: null };
    pump();
  }, [active, model, mediaId, pump]);

  const run = useCallback(
    async (action: () => Promise<void>) => {
      try {
        await action();
      } catch (cause) {
        onError(normalizeError(cause));
      }
    },
    [onError],
  );
  const start = () =>
    run(async () => {
      setState('starting');
      try {
        const loaded = await command('sam_start', undefined);
        encoded.current.clear();
        setModel(loaded);
        setState('ready');
      } catch (cause) {
        setModel(null);
        setState('stopped');
        throw cause;
      }
    });
  const configure = () =>
    run(async () => {
      const config = await command('sam_configure', undefined);
      if (!config) return;
      setModel(null);
      setState('stopped');
      await start();
    });
  const stop = () =>
    run(async () => {
      await command('sam_stop', undefined);
      encoded.current.clear();
      setModel(null);
      setState('stopped');
    });
  return { state, model, lastMs, request, start, stop, configure };
}
