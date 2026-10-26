import { Smartphone } from 'lucide-react';
import type { Pace, Today } from '../../api/index.js';
import { lastFour, paceText } from '../format.js';

// Pieces of a driver's route today, as Today's Routes, a work conversation's head and the profile
// show them: the route's code, a work phone by its last digits, how many stops are done of how
// many, as a bar in the colour of its pace, and its pace, but on time.

/** A route's code, as CX14. */
export function RouteCode({ code }: { code: string }) {
  return <span className="route-code">{code}</span>;
}

/** A work phone, by its last four digits. */
export function WorkPhone({ number }: { number: string }) {
  return (
    <span className="work-phone">
      <Smartphone aria-hidden="true" />
      {lastFour(number)}
    </span>
  );
}

/** How many of a route's stops are done, of how many, while it has any. */
export function Stops({ today }: { today: Pick<Today, 'stops' | 'stopsDone' | 'pace'> }) {
  if (today.stops === null) return null;
  const done = today.stopsDone ?? 0;
  return (
    <>
      <span
        className={today.pace ? `stops-bar ${today.pace}` : 'stops-bar'}
        role="meter"
        aria-label="Stops"
        aria-valuemin={0}
        aria-valuemax={today.stops}
        aria-valuenow={done}
      >
        <i style={{ width: `${Math.min(100, (100 * done) / today.stops)}%` }} />
      </span>
      <span className="stops">
        {done}/{today.stops}
      </span>
    </>
  );
}

/** How a route keeps up, but on time. */
export function PaceMark({ pace }: { pace: Pace | null }) {
  return pace && <span className={`pace ${pace}`}>{paceText[pace]}</span>;
}
