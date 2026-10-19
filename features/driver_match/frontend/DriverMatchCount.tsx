import type { DriverMatch, MatchCounts } from '../api/index.js';
import { useCachedData } from '../../../core/shell/frontend/runtime/api.js';

/** How many questions wait, beside the page's name in the sidebar. */
export function DriverMatchCount({ active }: { active: boolean }) {
  // The open page owns the full roster's polling; its count shares that read.
  const { data } = useCachedData<DriverMatch | MatchCounts>(
    active ? '/api/dsp/driver-match' : '/api/dsp/driver-match/counts',
    active ? -1 : 0,
  );
  const waiting = data ? ('people' in data ? data.questions.length : data.questions) : 0;
  return waiting > 0 ? (
    <span className="driver-match-count" aria-label={`${waiting} to check`}>
      {waiting}
    </span>
  ) : null;
}
