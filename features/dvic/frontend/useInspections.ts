import { useCallback, useEffect } from 'react';
import type { DvicInspections } from '../api/index.js';
import { api, useCachedData } from '../../../core/shell/frontend/runtime/api.js';
import { dataCache } from '../../../core/shell/frontend/runtime/data-cache.js';
import { readInspectionWeek } from './dvic.js';
import { performancePolicy } from '../../../core/shell/frontend/lib/performance-policy.js';

function inspectionWeekUrl(from: string, to: string) {
  return '/api/dsp/dvic/inspections?' + new URLSearchParams({ from, to, limit: '500' });
}

export const inspectionWeekCacheKey = (from: string, to: string) =>
  inspectionWeekUrl(from, to) + '#complete-week';

export function useInspections(from: string, to: string, publication: string, enabled: boolean) {
  const url = enabled ? inspectionWeekUrl(from, to) : '';
  // An assembled array must never share an entry with a single API cursor-page response.
  const cacheKey = url ? url + '#complete-week' : '';
  // All assembled weeks follow the status publication, including weeks retained after navigation.
  useEffect(() => {
    if (enabled)
      dataCache.observeVersion('dvic', publication, (key) =>
        key.startsWith('/api/dsp/dvic/inspections?'),
      );
  }, [enabled, publication]);
  const load = useCallback(
    (signal: AbortSignal) =>
      readInspectionWeek(
        (after) =>
          api<DvicInspections>(
            url + (after ? '&after=' + encodeURIComponent(after) : ''),
            undefined,
            signal,
          ),
        signal,
      ),
    [url],
  );
  const result = useCachedData(cacheKey, performancePolicy.recoveryPollMs, publication, load);
  return { ...result, rows: result.data };
}
