import { useNewDemoRequests } from '../../api/client.js';

/**
 * How often the count is asked for again: sooner on the page itself, where a change of status
 * reads it again at once anyway.
 */
const POLL_MS = 5 * 60_000;
const OPEN_POLL_MS = 60_000;

/** How many requests nobody has answered yet, beside the page's name in the sidebar. */
export function NewClientsCount({ active }: { active: boolean }) {
  const { data: waiting = 0 } = useNewDemoRequests(active ? OPEN_POLL_MS : POLL_MS);
  return waiting > 0 ? (
    <span className="client-count" aria-label={`${waiting} new`}>
      {waiting}
    </span>
  ) : null;
}
