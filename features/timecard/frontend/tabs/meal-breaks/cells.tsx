import { AlertTriangle, ArrowUpRight } from 'lucide-react';
import type { ClockTime, DeliveryGap } from '../../meal-breaks.js';

/** The provider page a time was read from: a Paycom timecard, or a Cortex route or stop. */
export interface SourceLink {
  href: string;
  site: 'Paycom' | 'Cortex';
  /** The Cortex route opens at the stop of this time's delivery. */
  stop?: boolean;
}
const opens = ({ site, stop }: SourceLink) =>
  site === 'Paycom' ? 'timecard in Paycom' : `${stop ? 'stop' : 'route'} in Cortex`;
export function Source({ name }: { name: 'Paycom' | 'Flex' }) {
  return (
    <span className={`meal-source ${name.toLowerCase()}`}>
      <i aria-hidden="true" />
      {name}
    </span>
  );
}
export function Clock({
  value,
  difference,
  link,
}: {
  value?: ClockTime | null;
  difference?: number | null;
  /** Makes the time, even a missing one, open the page it came from in a new tab. */
  link?: SourceLink | null;
}) {
  const clock = (
    <span className={`meal-clock${difference ? ' different' : ''}`}>
      {value ? (
        <>
          {value.label}
          {value.day !== 0 && (
            <small>
              {' '}
              ({value.day > 0 ? '+' : ''}
              {value.day}d)
            </small>
          )}
        </>
      ) : (
        <span className="meal-missing" aria-label="Not available">
          —
        </span>
      )}
      {difference !== undefined && difference !== null && difference !== 0 && (
        <small className="meal-delta">
          {difference > 0 ? '+' : '−'}
          {Math.abs(difference)}m
        </small>
      )}
    </span>
  );
  // The time keeps its look; only the pointer and an arrow on hover mark the link.
  return link ? (
    <a className="meal-clock-link" href={link.href} target="_blank" rel="noreferrer">
      {clock}
      <span className="sr-only">(open {opens(link)})</span>
      <ArrowUpRight className="meal-link-arrow" size={13} aria-hidden="true" />
    </a>
  ) : (
    clock
  );
}
export function LunchCell({
  paycom,
  cortex,
  difference,
  paycomLink,
  cortexLink,
}: {
  paycom?: ClockTime | null;
  cortex?: ClockTime | null;
  difference?: number | null;
  paycomLink?: SourceLink | null;
  cortexLink?: SourceLink | null;
}) {
  return (
    <>
      <div>
        <Source name="Paycom" />
        <Clock value={paycom} link={paycomLink} />
      </div>
      <div>
        <Source name="Flex" />
        <Clock value={cortex} difference={difference} link={cortexLink} />
      </div>
    </>
  );
}
export function GapBadge({
  gap,
  side,
  link,
}: {
  gap: DeliveryGap | null;
  side: 'before' | 'after';
  /** Makes a gap over the limit open the delivery it was measured from, like its time. */
  link?: SourceLink | null;
}) {
  const endpoints =
    side === 'before' ? 'Last delivery → Flex OUT LUNCH' : 'Flex IN LUNCH → first delivery';
  const detail = `${endpoints}: ${gap ? `${gap.label}${gap.overLimit ? ' · over 5 minutes' : ''}` : 'gap unavailable'}`;
  const className = `meal-gap${gap?.overLimit ? ' over-limit' : ''}`;
  const badge = (
    <>
      {gap?.overLimit && <AlertTriangle size={14} aria-hidden="true" />}
      <span>
        {gap ? (
          <>
            <span className="meal-gap-duration">{gap.label}</span> {side} lunch
          </>
        ) : (
          'Gap unavailable'
        )}
      </span>
    </>
  );
  return gap?.overLimit && link ? (
    <a
      className={`${className} meal-gap-link`}
      href={link.href}
      target="_blank"
      rel="noreferrer"
      aria-label={`${detail} (open ${opens(link)})`}
    >
      {badge}
      <ArrowUpRight className="meal-link-arrow" size={13} aria-hidden="true" />
    </a>
  ) : (
    <span className={className} aria-label={detail}>
      {badge}
    </span>
  );
}
