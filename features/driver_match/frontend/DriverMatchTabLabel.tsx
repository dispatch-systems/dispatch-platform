import type { DriverCounts, DriverMatch } from '../../../shared/contracts/driver-match.js';
import { useCachedData } from '../../../core/shell/frontend/runtime/api.js';

/** The tab's name, with how many pairs wait for a decision. */
export function DriverMatchTabLabel({ active }: { active: boolean }) {
  // The open panel owns the full roster's polling; its badge shares that read.
  const { data } = useCachedData<DriverMatch | DriverCounts>(
    active ? '/api/dsp/driver-match' : '/api/dsp/driver-match/counts',
    active ? -1 : 0,
  );
  const review = data ? ('counts' in data ? data.counts.review : data.review) : 0;
  return (
    <>
      Driver Match
      {review > 0 && (
        <span className="driver-tab-flag" aria-label={`${review} to review`}>
          {review}
        </span>
      )}
    </>
  );
}
