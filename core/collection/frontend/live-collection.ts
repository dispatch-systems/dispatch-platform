import { useEffect } from 'react';
import { view } from '../../shell/frontend/runtime/api.js';
import { longPoll } from '../../shell/frontend/runtime/long-poll.js';
import { dataCache } from '../../shell/frontend/runtime/data-cache.js';
import {
  collectionAffects,
  collectionData,
  type CollectionChange,
} from '../../shell/frontend/runtime/data-policy.js';
import { getCollectionUpdates } from '../api/client.js';

/**
 * One sleeping request for a page that follows its collections live, shared across its days and
 * tabs. A finished collection refreshes the reads its feature's cache rules say it reaches.
 */
export function useCollectionUpdates() {
  const token = view;
  useEffect(() => {
    let disposed = false;
    let pending: ReturnType<typeof setTimeout> | undefined;
    let after = '';
    let pendingVersion: string | undefined;
    let pendingChanges: CollectionChange[] = [];
    const refresh = (version: string, changes: CollectionChange[]) => {
      pendingVersion = version;
      pendingChanges.push(...changes);
      if (pending) return;
      // Coalesce closely spaced driver results from the collection lanes.
      pending = setTimeout(() => {
        pending = undefined;
        if (!disposed) {
          const changes = pendingChanges;
          pendingChanges = [];
          dataCache.observeVersion('collections', pendingVersion!, (url) =>
            collectionAffects(url, changes),
          );
        }
      }, 400);
    };
    const poll = longPoll(
      async (signal) => {
        const result = await getCollectionUpdates(
          after,
          AbortSignal.any([signal, AbortSignal.timeout(30000)]),
        );
        if (!signal.aborted && !disposed) {
          // Record the baseline before a fast first update can coalesce with it.
          if (after === '')
            dataCache.observeVersion('collections', result.revision, collectionData);
          else if (result.revision !== after) refresh(result.revision, result.changes);
          after = result.revision;
        }
      },
      (_cause, stopped, first) => {
        // A transient transport failure still gets a conventional data refresh.
        if (!stopped && first) dataCache.invalidate(collectionData);
      },
    );
    const visibility = () => {
      poll.clearRetry();
      if (document.hidden || !navigator.onLine) {
        poll.pause();
        if (pending) clearTimeout(pending);
        pending = undefined;
        pendingChanges = [];
      } else {
        after = '';
        void poll.listen();
      }
    };
    document.addEventListener('visibilitychange', visibility);
    window.addEventListener('online', visibility);
    window.addEventListener('offline', visibility);
    void poll.listen();
    return () => {
      disposed = true;
      poll.dispose();
      if (pending) clearTimeout(pending);
      document.removeEventListener('visibilitychange', visibility);
      window.removeEventListener('online', visibility);
      window.removeEventListener('offline', visibility);
    };
  }, [token]);
}
