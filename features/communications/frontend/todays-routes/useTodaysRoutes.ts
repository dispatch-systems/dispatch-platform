import { useCallback, useEffect, useRef, useState } from 'react';
import type { Communications, TodaysRoutes } from '../../api/index.js';
import { getTodaysRoutes } from '../../api/client.js';
import { messageOf } from '../../../../core/shell/frontend/lib/errors.js';
import type { Changed } from '../inbox/useInbox.js';

// Today's Routes as live route data holds it, read while it shows: at once, again every
// `ROUTES_READ_MS` while the page is in view, as its live reads come every few minutes and say
// nothing as they do, and again once an update changes a work conversation, or the list is read
// afresh.

/** How often Today's Routes is read again while it shows, for its drivers' progress. */
export const ROUTES_READ_MS = 60_000;

/** Today's Routes while `shown`, read again as it changes; `refresh` reads it at once. */
export function useTodaysRoutes(shown: boolean, changed: Changed, inbox?: Communications) {
  const [routes, setRoutes] = useState<TodaysRoutes>();
  const [error, setError] = useState('');
  const [generation, setGeneration] = useState(0);
  const refresh = useCallback(() => setGeneration((n) => n + 1), []);
  // An update changing a work conversation, or a fresh read of the list, reads it again.
  const seen = useRef(changed.at);
  useEffect(() => {
    if (changed.at === seen.current) return;
    seen.current = changed.at;
    const { ids } = changed;
    const work =
      ids === 'all' ||
      ids.some((id) => inbox?.conversations.some((each) => each.id === id && each.kind === 'work'));
    if (work) refresh();
  }, [changed, inbox, refresh]);

  useEffect(() => {
    if (!shown) return;
    let disposed = false;
    let controller: AbortController | undefined;
    const read = () => {
      if (document.hidden) return;
      controller?.abort();
      const request = new AbortController();
      controller = request;
      getTodaysRoutes(request.signal).then(
        (read) => {
          if (disposed || request.signal.aborted) return;
          setRoutes(read);
          setError('');
        },
        (cause) => {
          if (!disposed && !request.signal.aborted) setError(messageOf(cause));
        },
      );
    };
    read();
    const timer = setInterval(read, ROUTES_READ_MS);
    document.addEventListener('visibilitychange', read);
    return () => {
      disposed = true;
      controller?.abort();
      clearInterval(timer);
      document.removeEventListener('visibilitychange', read);
    };
  }, [shown, generation]);
  return { routes, error, refresh };
}
