import { useEffect } from 'react';
import type { DvicStatus } from '../api/index.js';
import { dataCache } from '../../../core/shell/frontend/runtime/data-cache.js';
import { publication } from './dvic.js';

/**
 * What DVIC has published, as `publication` reads it. Whichever screen sees it change first, DVIC's
 * page or Home's Latest DVIC, drops the inspections read before it, those still being read too.
 */
export function usePublished(status: DvicStatus | undefined) {
  const published = publication(status);
  useEffect(() => {
    if (status)
      dataCache.observeVersion('dvic', published.publication, (key) =>
        ['summary?', 'day?', 'inspection?'].some((part) => key.startsWith('/api/dsp/dvic/' + part)),
      );
  }, [published.publication, status]);
  return published;
}
