import { useEffect, useState } from 'react';
import { performancePolicy } from '../../../core/shell/frontend/lib/performance-policy.js';
import { randomId } from '../../../core/shell/frontend/lib/random-id.js';
import { dataCache } from '../../../core/shell/frontend/runtime/data-cache.js';
import { collectionData } from '../../../core/shell/frontend/runtime/data-policy.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { syncMealBreaks, useDailySyncStatus, usePaycomStatus } from '../api/client.js';

export function useDailySync(date: string, daily: boolean) {
  const [syncRevision, setSyncRevision] = useState(0);
  const [collecting, setCollecting] = useState(false);
  const overview = usePaycomStatus();
  const syncState = useDailySyncStatus(
    date,
    collecting ? performancePolicy.activeCollectionPollMs : performancePolicy.recoveryPollMs,
    String(syncRevision),
    daily,
  );
  // The last known state stays up while another date loads so the page does not shift.
  const sourceState = syncState.data ?? syncState.stale;
  const sourceCurrent = syncState.data?.date === date;
  const { refresh } = overview;
  const activeSync = daily && (sourceState?.paycom.active || sourceState?.flex.active);
  useEffect(() => setCollecting(Boolean(activeSync)), [activeSync]);
  const collectedAt = overview.data?.workforce.collectedAt;
  useEffect(() => {
    if (collectedAt) dataCache.observeVersion('paycom', collectedAt, collectionData);
  }, [collectedAt]);
  const refreshKey = String(syncRevision);
  useEffect(() => {
    if (sourceState?.flex.collectedAt)
      dataCache.observeVersion('flex', sourceState.flex.collectedAt, (url) =>
        url.startsWith('/api/dsp/paycom/meal-breaks'),
      );
  }, [sourceState?.flex.collectedAt]);
  const syncUnavailable = daily
    ? !sourceState
      ? 'Checking connections…'
      : !sourceState.paycom.enabled
        ? 'Connect Paycom in Settings → Connections to sync.'
        : !sourceState.flex.enabled
          ? 'Connect Cortex in Settings → Connections to sync Flex.'
          : !sourceState.scopeAvailable
            ? 'Complete your DSP profile with a station code to sync Flex.'
            : ''
    : '';
  const onQueued = () => {
    refresh();
    // Keep every Sync Now disabled until status read after this request arrives.
    setSyncRevision((value) => value + 1);
  };
  const sync = useAction(
    async () => {
      try {
        if (daily) await syncMealBreaks(date, randomId());
      } finally {
        onQueued();
      }
    },
    { success: 'Flex and Paycom collections queued' },
  );
  const disabled =
    !!syncUnavailable ||
    !syncState.data ||
    syncState.validatedKey !== String(syncRevision) ||
    !!syncState.error ||
    sync.busy ||
    !!activeSync ||
    !sourceCurrent;
  return {
    overview: { data: overview.data, error: overview.error },
    sourceState,
    activeSync,
    syncUnavailable,
    refreshKey,
    sync: { run: sync.run, disabled, error: syncState.error },
    onQueued,
  };
}
