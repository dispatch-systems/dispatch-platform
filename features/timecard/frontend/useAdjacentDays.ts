import { useEffect } from 'react';
import { cancelPrefetches, prefetchData } from '../../app/prefetch.js';
import { addDays } from '../../lib/calendar.js';

/** Warm only the previous and next day, using the current filters and sort. */
export function useAdjacentDays(url: string, date: string, today: string, loaded?: object) {
  const ready = Boolean(loaded);
  // Live invalidation refreshes the selected day; neighbors revalidate on selection.
  useEffect(() => {
    if (!ready) return;
    const owner = `days:${url}`;
    prefetchData(
      [-1, 1]
        .map((offset) => addDays(date, offset))
        .filter((day) => day <= today)
        .map((day) => url.replace(`date=${date}`, `date=${day}`)),
      { owner },
    );
    return () => cancelPrefetches(owner);
  }, [url, date, today, ready]);
}
