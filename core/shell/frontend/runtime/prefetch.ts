import { api } from './api.js';
import { cacheLimits, dataCache } from './data-cache.js';

// Selected records load immediately; this small queue only warms likely next selections.
type Warmup = { generation: number; owners: Set<string>; priority: boolean };
const queue = new Map<string, Warmup>();
let running = 0;
let timer: ReturnType<typeof setTimeout> | undefined;

function drain() {
  timer = undefined;
  if (document.hidden || !navigator.onLine) return;
  while (running < cacheLimits.preloadConcurrent && queue.size) {
    const [url, warmup] =
      [...queue].find(([, item]) => item.priority) ?? queue.entries().next().value!;
    queue.delete(url);
    if (warmup.generation !== dataCache.generation) continue;
    running++;
    void dataCache
      .read(url, (signal) => api(url, undefined, signal, 'low'))
      .catch(() => {}) // An optional preload must not interrupt the page; selection retries it.
      .finally(() => {
        running--;
        schedule();
      });
  }
}

function schedule() {
  if (!timer && queue.size && !document.hidden && navigator.onLine)
    timer = setTimeout(drain, cacheLimits.preloadDelayMs);
}

export function canPrefetch() {
  const connection = (
    navigator as Navigator & {
      connection?: { saveData?: boolean; effectiveType?: string };
    }
  ).connection;
  return (
    navigator.onLine &&
    !connection?.saveData &&
    !['slow-2g', '2g'].includes(connection?.effectiveType ?? '')
  );
}

export function prefetchData(urls: string[], options: { owner?: string; priority?: boolean } = {}) {
  if (!canPrefetch()) return;
  const owner = options.owner ?? location.hash.split('?')[0]!;
  for (const url of urls) {
    if (!url) continue;
    const current = queue.get(url);
    if (current?.generation === dataCache.generation) {
      current.owners.add(owner);
      current.priority ||= Boolean(options.priority);
    } else {
      if (queue.size >= cacheLimits.preloadQueued) {
        if (!options.priority) break;
        const discarded = [...queue].find(([, item]) => !item.priority)?.[0];
        if (!discarded) break;
        queue.delete(discarded);
      }
      queue.set(url, {
        generation: dataCache.generation,
        owners: new Set([owner]),
        priority: Boolean(options.priority),
      });
    }
  }
  schedule();
}

/** Queued work belongs to its page; active reads remain shared with selected records. */
export function cancelPrefetches(owner?: string) {
  for (const [url, item] of queue) {
    if (owner === undefined) queue.delete(url);
    else {
      item.owners.delete(owner);
      if (!item.owners.size) queue.delete(url);
    }
  }
  if (!queue.size && timer) {
    clearTimeout(timer);
    timer = undefined;
  }
}

// Outside a browser, where the route table's tests load the manifests, nothing is ever queued.
if (typeof document !== 'undefined') {
  document.addEventListener('visibilitychange', schedule);
  window.addEventListener('online', schedule);
}
