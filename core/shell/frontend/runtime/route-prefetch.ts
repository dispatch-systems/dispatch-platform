import type { DspView, SessionView } from '../../../../shared/contracts/accounts.js';
import { api, view as admittedToken } from './api.js';
import { dataCache } from './data-cache.js';
import { canPrefetch, prefetchData } from './prefetch.js';
import { routeOf } from './slots.js';

/** Reads `urls` for `owner`: at once and at high priority, or as queued warmups. */
export function warm(urls: string[], owner: string, immediate: boolean) {
  if (immediate) {
    if (document.hidden || !navigator.onLine) return;
    for (const url of urls)
      void dataCache.read(url, (signal) => api(url, undefined, signal, 'high')).catch(() => {});
  } else if (canPrefetch()) prefetchData(urls, { owner, priority: true });
}
/** Whether reads made now belong to this view's admission. */
export const admitted = (view?: DspView): view is DspView =>
  Boolean(view && admittedToken === view.token);

/**
 * Warms a page's primary data through its owner's manifest: a DSP page's only for the admitted
 * view, and a platform page's only while no DSP view is admitted.
 */
export function prefetchRouteData(
  page: string,
  view?: DspView,
  session?: SessionView,
  immediate = false,
) {
  const warmPage = (urls: string[]) => warm(urls, `route:${page}`, immediate);
  if (!view) {
    if (!admittedToken)
      routeOf('platform', page)?.prefetch?.({ session, immediate, warm: warmPage });
    return;
  }
  if (admitted(view))
    routeOf('dsp', page)?.prefetch?.({ view, session, immediate, warm: warmPage });
}
