import { useCommunicationsWaiting } from '../api/client.js';

/**
 * How often the count is read again while the page is closed. The page, open, reads it again
 * as soon as the number waiting changes.
 */
const POLL_MS = 30_000;

/** How many conversations wait on an answer, beside the page's name in the sidebar. */
export function WaitingCount({ active }: { active: boolean }) {
  const { data } = useCommunicationsWaiting(active ? -1 : POLL_MS);
  const count = data?.count ?? 0;
  return count > 0 ? (
    <span className="communications-count" aria-label={`${count} waiting`}>
      {count}
    </span>
  ) : null;
}
