import { useCallback, useEffect } from 'react';
import type { DvicInspections } from '../../../../shared/contracts/dvic.js';
import { api, useCachedData } from '../../app/api.js';
import { dataCache } from '../../app/data-cache.js';
import { readInspectionWeek } from '../../lib/dvic.js';
import { performancePolicy } from '../../lib/performance-policy.js';

export function useInspections(from: string, to: string, publication: string, enabled: boolean) {
  const query = new URLSearchParams({ from, to, limit: '500' }).toString();
  const url = enabled ? '/api/dsp/dvic/inspections?' + query : '';
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
