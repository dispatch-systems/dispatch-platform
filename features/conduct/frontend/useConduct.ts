import { useCallback, useState } from 'react';
import type { ConductOverview } from '../api/index.js';
import { useConductOverview } from '../api/client.js';

/**
 * The page's data: as read, or as the last write answered, until it's read again. Each write
 * answers with the data after it, so `take` shows its change at once, with no read between.
 */
export function useConduct() {
  const read = useConductOverview();
  const [written, setWritten] = useState<{ after?: ConductOverview; value: ConductOverview }>();
  const overview = written && written.after === read.data ? written.value : read.data;
  const take = useCallback(
    (value: ConductOverview) => setWritten({ after: read.data, value }),
    [read.data],
  );
  return { overview, error: read.error, refresh: read.refresh, take };
}
