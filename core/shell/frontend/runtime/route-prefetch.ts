import type { DspView, SessionView } from '../../../accounts/api/index.js';
import { api, view as admittedToken } from './api.js';
import { dataCache } from './data-cache.js';
import { canPrefetch, prefetchData, type PrefetchRead } from './prefetch.js';
import { routeOf } from './slots.js';

/** Reads for `owner`: at once and at high priority, or as queued warmups. */
export function warm(reads: PrefetchRead[], owner: string, immediate: boolean) {
  if (immediate) {
    if (document.hidden || !navigator.onLine) return;
    for (const request of reads) {
      const url = typeof request === 'string' ? request : request.url;
      const read = (signal: AbortSignal) =>
        typeof request === 'string'
          ? api(url, undefined, signal, 'high')
          : request.read(signal, 'high');
      void dataCache.read(url, read).catch(() => {});
    }
  } else if (canPrefetch()) prefetchData(reads, { owner, priority: true });
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
  const warmPage = (reads: PrefetchRead[]) => warm(reads, `route:${page}`, immediate);
  if (!view) {
    if (!admittedToken)
      routeOf('platform', page)?.prefetch?.({ session, immediate, warm: warmPage });
    return;
  }
  if (admitted(view))
    routeOf('dsp', page)?.prefetch?.({ view, session, immediate, warm: warmPage });
}
