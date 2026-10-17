import type { DriverMatch, MatchCounts } from '../api/index.js';
import { useCachedData } from '../../../core/shell/frontend/runtime/api.js';

/** The tab's name, with how many questions wait. */
export function DriverMatchTabLabel({ active }: { active: boolean }) {
  // The open panel owns the full roster's polling; its badge shares that read.
  const { data } = useCachedData<DriverMatch | MatchCounts>(
    active ? '/api/dsp/driver-match' : '/api/dsp/driver-match/counts',
    active ? -1 : 0,
  );
  const waiting = data ? ('people' in data ? data.questions.length : data.questions) : 0;
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
