import type { DriverCounts, DriverMatch } from '../api/index.js';
import { useCachedData } from '../../../core/shell/frontend/runtime/api.js';

/** The tab's name, with how many questions wait: pairs and unconfirmed name variants. */
export function DriverMatchTabLabel({ active }: { active: boolean }) {
  // The open panel owns the full roster's polling; its badge shares that read.
  const { data } = useCachedData<DriverMatch | DriverCounts>(
    active ? '/api/dsp/driver-match' : '/api/dsp/driver-match/counts',
    active ? -1 : 0,
  );
  const counts = data && ('counts' in data ? data.counts : data);
  const waiting = counts ? counts.review + counts.unconfirmed : 0;
  return (
    <>
      Driver Match
      {waiting > 0 && (
        <span className="driver-tab-flag" aria-label={`${waiting} to check`}>
          {waiting}
        </span>
      )}
    </>
  );
}
