import { useCallback, useEffect, useRef, useState } from 'react';
import type { Communications, TodaysRoutes } from '../../api/index.js';
import { getTodaysRoutes, todaysRoutesUrl } from '../../api/client.js';
import { useData } from '../../../../core/shell/frontend/runtime/api.js';
import type { Changed } from '../inbox/useInbox.js';
import type { TodaysRead } from './todays-routes.js';

// Today's Routes as live route data holds it, read while it shows: at once, again every
// `ROUTES_READ_MS` while the page is in view, as its live reads come every few minutes and say
// nothing as they do, and again once an update changes a work conversation, or the list is read
// afresh; and as the page comes back into view or the browser back online, a failed read tried
// again, as the dashboard's reads are.

/** How often Today's Routes is read again while it shows, for its drivers' progress. */
export const ROUTES_READ_MS = 60_000;

/** Today's Routes while `shown`, once read, and whether that read answers the latest change; why
 * the latest read failed, if it did; and reading it again. */
export function useTodaysRoutes(shown: boolean, changed: Changed, inbox?: Communications) {
  // An update changing a work conversation, or a fresh read of the list, reads it again.
  const [generation, setGeneration] = useState(0);
  const seen = useRef(changed.at);
  useEffect(() => {
    if (changed.at === seen.current) return;
    seen.current = changed.at;
    const { ids } = changed;
    const work =
      ids === 'all' ||
      ids.some((id) => inbox?.conversations.some((each) => each.id === id && each.kind === 'work'));
    if (work) setGeneration((n) => n + 1);
  }, [changed, inbox]);
  const load = useCallback((signal: AbortSignal) => getTodaysRoutes(signal), []);
  const key = String(generation);
  const { data, error, refresh, validatedKey } = useData<TodaysRoutes>(
    shown ? todaysRoutesUrl : '',
    ROUTES_READ_MS,
    key,
    undefined,
    false,
    load,
  );
  const read: TodaysRead | undefined =
    shown && data ? { routes: data, current: validatedKey === key } : undefined;
  return { read, error, refresh };
}
