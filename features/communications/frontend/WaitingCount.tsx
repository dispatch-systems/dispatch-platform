import { useCommunicationsWaiting } from '../api/client.js';

/**
 * How many conversations wait on an answer, beside the page's name in the sidebar: as the banner
 * of phones down, on every page, reads them every 30 seconds, and the page, open, as the number
 * changes (`PhonesDown`).
 */
export function WaitingCount(_: { active: boolean }) {
  const { data } = useCommunicationsWaiting(-1);
  const count = data?.count ?? 0;
  return count > 0 ? (
    <span className="communications-count" aria-label={`${count} waiting`}>
      {count}
    </span>
  ) : null;
}
