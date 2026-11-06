import { useEffect, useState, useCallback, useRef, useSyncExternalStore } from 'react';
import { backoff } from '../lib/backoff.js';
import { performancePolicy } from '../lib/performance-policy.js';
import { useReadAvailability } from '../ui/useReadAvailability.js';
import { api, ApiError } from './api.js';
import { dataCache } from './data-cache.js';

/** Opt in to bounded session memory and background revalidation. */
export function useCachedData<T>(
  url: string,
  poll = 0,
  refreshKey?: string | null,
  load?: (signal: AbortSignal) => Promise<T>,
) {
  return useData<T>(url, poll, refreshKey, url, true, load);
}

export function useData<T>(
  url: string,
  poll = 0,
  refreshKey?: string | null,
  dataScope?: string,
  cache = false,
  load?: (signal: AbortSignal) => Promise<T>,
) {
  const availability = useReadAvailability();
  const cached = useSyncExternalStore(
    useCallback(
      (listener) => (cache ? dataCache.subscribe(url, listener) : () => {}),
      [cache, url],
    ),
    useCallback(() => (cache ? dataCache.peek(url) : undefined), [cache, url]),
  );
  const generation = cached?.generation ?? 0;
  const session = dataCache.session;
  const scope = cache ? url : (dataScope ?? url);
  const reader = useRef<(force?: boolean) => Promise<void>>(undefined);
  const previous = useRef<{ url: string; refreshKey?: string | null; revision: number }>(undefined);
  const [result, setResult] = useState<{
      data: T;
      scope: string | undefined;
      session: number;
      refreshKey?: string | null;
    }>(),
    [failure, setFailure] = useState<{
      message: string;
      code: string;
      scope: string;
      session: number;
    }>(),
    [revision, setRevision] = useState(0);
  const refresh = useCallback(() => setRevision((v) => v + 1), []);
  useEffect(() => {
    const changed = previous.current;
    const force =
      changed?.url === url && (changed.refreshKey !== refreshKey || changed.revision !== revision);
    previous.current = { url, refreshKey, revision };
    const controller = new AbortController();
    let active = true;
    setFailure(undefined);
    let reading = false;
    let failures = 0;
    let retry: ReturnType<typeof setTimeout> | undefined;
    const read = async (force = false) => {
      if (!url || reading || document.hidden || !navigator.onLine) return;
      reading = true;
      try {
        const request = load ?? ((signal: AbortSignal) => api<T>(url, undefined, signal));
        const value = cache
          ? await dataCache.read(url, request, force)
          : await request(controller.signal);
        if (
          active &&
          (!cache ||
            (generation === dataCache.peek(url).generation && session === dataCache.session))
        ) {
          setResult((previous) =>
            previous?.data === value &&
            previous.scope === scope &&
            previous.session === session &&
            previous.refreshKey === refreshKey
              ? previous
              : { data: value, scope, session, refreshKey },
          );
          setFailure(undefined);
          failures = 0;
          if (retry) clearTimeout(retry);
        }
      } catch (error) {
        if (
          active &&
          (!cache ||
            (generation === dataCache.peek(url).generation && session === dataCache.session)) &&
          error instanceof Error &&
          error.name !== 'AbortError'
        ) {
          setFailure({
            message: error.message,
            code: error instanceof ApiError ? error.code : '',
            scope,
            session,
          });
          // A missed table response must recover even when no further driver arrives.
          if (!(error instanceof ApiError) || error.status >= 500 || error.status === 429) {
            if (retry) clearTimeout(retry);
            retry = setTimeout(() => void read(true), backoff(failures++));
          }
        }
      } finally {
        reading = false;
      }
    };
    reader.current = read;
    const visible = () => {
      if (!document.hidden) void read(true);
    };
    document.addEventListener('visibilitychange', visible);
    window.addEventListener('online', visible);
    void read(force);
    return () => {
      active = false;
      controller.abort();
      document.removeEventListener('visibilitychange', visible);
      window.removeEventListener('online', visible);
      if (reader.current === read) reader.current = undefined;
      if (retry) clearTimeout(retry);
    };
  }, [url, revision, refreshKey, scope, cache, generation, session, load]);
  // Changing the active/idle cadence must never abort an in-progress read.
  useEffect(() => {
    const interval = poll < 0 ? 0 : poll || (cache ? performancePolicy.recoveryPollMs : 0);
    if (!interval) return;
    const timer = setInterval(() => void reader.current?.(true), interval);
    return () => clearInterval(timer);
  }, [poll, cache]);
  // A date-scoped view can keep its controls mounted without showing the previous day's rows.
  const shown = result?.session === session ? result : undefined;
  const data =
    (cached?.data as T | undefined) ?? (shown?.scope === scope ? shown?.data : undefined);
  const currentFailure =
    failure?.scope === scope && failure.session === session ? failure : undefined;
  const error = currentFailure?.message ?? '';
  const errorCode = currentFailure?.code ?? '';
  // The previous scope's value lets a view hold its layout, marked busy, until the new one lands.
  const stale = data || error ? undefined : shown?.data;
  const paused = Boolean(url) && availability !== 'ready';
  return {
    data,
    stale,
    error,
    errorCode,
    refresh,
    paused,
    loading: Boolean(url) && data === undefined && !error && !paused,
    validatedKey: shown?.refreshKey,
  };
}
