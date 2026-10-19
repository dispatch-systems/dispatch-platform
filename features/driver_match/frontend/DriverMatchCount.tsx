import type { DriverMatch, MatchCounts } from '../api/index.js';
import { useCachedData } from '../../../core/shell/frontend/runtime/api.js';

/**
 * How often the count is asked for again while the page is closed. It changes only with a
 * decision, which reads it again at once, or a collection, which a page following collections
 * reads it again for; this catches up those finished while no such page was open.
 */
const POLL_MS = 5 * 60_000;

/** How many questions wait, beside the page's name in the sidebar. */
export function DriverMatchCount({ active }: { active: boolean }) {
  // The open page owns the full roster's polling; its count shares that read.
  const { data } = useCachedData<DriverMatch | MatchCounts>(
    active ? '/api/dsp/driver-match' : '/api/dsp/driver-match/counts',
    active ? -1 : POLL_MS,
  );
  const waiting = data ? ('people' in data ? data.questions.length : data.questions) : 0;
  return waiting > 0 ? (
    <span className="driver-match-count" aria-label={`${waiting} to check`}>
      {waiting}
    </span>
  ) : null;
}
