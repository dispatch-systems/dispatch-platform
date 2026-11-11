import { ApiError } from './api.js';
import { backoff } from '../lib/backoff.js';

/** One request at a time, retried while the page is visible and online. The caller keeps its
 * cursor and decides what a response, a failure and returning to the page mean. */
export function longPoll(
  read: (signal: AbortSignal) => Promise<void>,
  failed: (cause: unknown, stopped: boolean, first: boolean) => void,
) {
  let disposed = false;
  let controller: AbortController | undefined;
  let retry: ReturnType<typeof setTimeout> | undefined;
  let failures = 0;
  const listen = async () => {
    if (disposed || document.hidden || !navigator.onLine || controller) return;
    const request = new AbortController();
    controller = request;
    let delay = 0;
    let stopped = false;
    try {
      await read(request.signal);
      if (!request.signal.aborted) failures = 0;
    } catch (cause) {
      if (disposed || request.signal.aborted) return;
      stopped = cause instanceof ApiError && [401, 403].includes(cause.status);
      failed(cause, stopped, failures === 0);
      delay = backoff(failures++);
    } finally {
      if (controller === request) controller = undefined;
      if (!disposed && !request.signal.aborted && !stopped && !document.hidden)
        retry = setTimeout(() => void listen(), delay);
    }
  };
  const clearRetry = () => {
    if (retry) clearTimeout(retry);
    retry = undefined;
  };
  const pause = () => {
    clearRetry();
    controller?.abort();
    controller = undefined;
  };
  return {
    listen,
    clearRetry,
    pause,
    dispose: () => {
      disposed = true;
      pause();
    },
  };
}
