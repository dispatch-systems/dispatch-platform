import { useCallback, useEffect, useState } from 'react';
import type { UniformAdjustment, UniformInventory } from '../api/index.js';
import { dataCache } from '../../../core/shell/frontend/runtime/data-cache.js';
import { longPoll } from '../../../core/shell/frontend/runtime/long-poll.js';
import { getUniformUpdates } from '../api/client.js';
import { messageOf } from '../../../core/shell/frontend/lib/errors.js';
import { applyUniformAdjustments, applyUniformSnapshot, applyUniformUpdates } from './uniforms.js';

export type InventoryStatus = 'connecting' | 'live' | 'reconnecting' | 'paused' | 'unavailable';

export function useUniformInventory(token: string) {
  const cacheKey = `/api/dsp/uniforms/snapshot?view=${encodeURIComponent(token)}`;
  const [data, setData] = useState<UniformInventory | undefined>(
    () => dataCache.peek(cacheKey).data as UniformInventory | undefined,
  );
  useEffect(() => {
    if (data) dataCache.put(cacheKey, data);
  }, [cacheKey, data]);
  const [status, setStatus] = useState<InventoryStatus>('connecting');
  const [error, setError] = useState('');
  const [generation, setGeneration] = useState(0);
  const refresh = useCallback(() => setGeneration((n) => n + 1), []);
  const accept = useCallback(
    (next: UniformInventory) => setData((current) => applyUniformSnapshot(current, next)),
    [],
  );
  const acknowledge = useCallback((change: UniformAdjustment) => {
    // Acknowledgements must not advance the stream cursor: other users' changes may precede them.
    setData((current) => current && applyUniformAdjustments(current, [change]));
  }, []);

  useEffect(() => {
    let after: number | undefined;
    const poll = longPoll(
      async (signal) => {
        const update = await getUniformUpdates(
          after,
          AbortSignal.any([signal, AbortSignal.timeout(25_000)]),
        );
        if (signal.aborted) return;
        setData((current) => applyUniformUpdates(current, update));
        after = update.revision;
        setError('');
        setStatus('live');
      },
      (cause, stopped) => {
        setError(messageOf(cause));
        setStatus(stopped ? 'unavailable' : 'reconnecting');
        after = undefined;
      },
    );
    const resume = () => {
      poll.pause();
      after = undefined;
      setStatus(document.hidden ? 'paused' : navigator.onLine ? 'connecting' : 'reconnecting');
      if (!document.hidden && navigator.onLine) void poll.listen();
    };
    document.addEventListener('visibilitychange', resume);
    window.addEventListener('online', resume);
    window.addEventListener('offline', resume);
    resume();
    return () => {
      poll.dispose();
      document.removeEventListener('visibilitychange', resume);
      window.removeEventListener('online', resume);
      window.removeEventListener('offline', resume);
    };
  }, [token, generation]);
  return { data, status, error, refresh, accept, acknowledge };
}
