import { useCallback, useEffect } from 'react';
import { api } from '../../../core/shell/frontend/runtime/api.js';
import { dataCache } from '../../../core/shell/frontend/runtime/data-cache.js';
import type { ConductOverview } from '../api/index.js';
import { conductUrl, useConductOverview } from '../api/client.js';

/** Compact shared data, refreshed while visible and at the DSP's date boundary. */
export function useConduct(timezone?: string) {
  const read = useConductOverview();
  const { refresh } = read;
  const reload = useCallback(
    () =>
      dataCache.read(
        conductUrl,
        (signal) => api<ConductOverview>(conductUrl, undefined, signal),
        true,
      ),
    [],
  );
  useEffect(() => {
    if (!timezone) return;
    const format = new Intl.DateTimeFormat('en-CA', {
      timeZone: timezone,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
    });
    const now = Date.now();
    const today = format.format(now);
    let low = now,
      high = now + 26 * 60 * 60 * 1000;
    while (high - low > 1000) {
      const middle = Math.floor((low + high) / 2);
      if (format.format(middle) === today) low = middle;
      else high = middle;
    }
    const timer = setTimeout(refresh, high - now);
    return () => clearTimeout(timer);
  }, [timezone, read.data?.today, refresh]);
  const take = useCallback(
    (value?: ConductOverview) => {
      if (value) dataCache.put(conductUrl, value);
      else refresh();
    },
    [refresh],
  );
  return { overview: read.data, error: read.error, refresh: read.refresh, reload, take };
}
