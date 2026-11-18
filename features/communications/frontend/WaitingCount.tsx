import { DISPATCH_PHONE, type TextingLine } from './line.js';

/**
 * How many conversations of a line wait on an answer, beside its page's name in the sidebar: as
 * the line's banner of phones down, on every page, reads them every 30 seconds, and the page, open,
 * as the number changes (`PhonesDown`).
 */
export function WaitingCount({ line = DISPATCH_PHONE }: { active: boolean; line?: TextingLine }) {
  const { data } = line.texting.useWaiting(-1);
  const count = data?.count ?? 0;
  return count > 0 ? (
    <span className="communications-count" aria-label={`${count} waiting`}>
      {count}
    </span>
  ) : null;
}
