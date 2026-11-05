import { Smartphone } from 'lucide-react';
import type { Pace, Today } from '../../api/index.js';
import { lastFour, paceText } from '../shared/format.js';
import './route-facts.css';

// Pieces of a driver's route today, as Today's Routes, Recents, a work conversation's head and the
// profile show them: the route's code, a work phone by its last digits, or in Recents where the
// conversation is, how many stops are done of how many, as a bar in the colour of its pace and in
// words, "98 of 168 stops", or in a list's row, short of room, "98/168", read aloud in full; and
// its pace, but on time.

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

/** How many of a route's stops are done, of how many, as a bar and in words, or `short`, as
 * "98/168", while it has any. */
export function Stops({
  today,
  short,
}: {
  today: Pick<Today, 'stops' | 'stopsDone' | 'pace'>;
  short?: boolean;
}) {
  if (today.stops === null) return null;
  const done = today.stopsDone ?? 0;
  const words = `${done} of ${today.stops} stops`;
  return (
    <>
      <span className={today.pace ? `stops-bar ${today.pace}` : 'stops-bar'} aria-hidden="true">
        <i style={{ width: `${Math.min(100, (100 * done) / today.stops)}%` }} />
      </span>
      {short ? (
        <span className="stops">
          <span aria-hidden="true">
            {done}/{today.stops}
          </span>
          <span className="sr-only">{words}</span>
        </span>
      ) : (
        <span className="stops">{words}</span>
      )}
    </>
  );
}

/** How a route keeps up, but on time. */
export function PaceMark({ pace }: { pace: Pace | null }) {
  return pace && <span className={`pace ${pace}`}>{paceText[pace]}</span>;
}

/** A driver's route today and the work phone they hold now, as their conversation's head says. */
export function RouteToday({ today }: { today: Today }) {
  return (
    <>
      {today.route && <RouteCode code={today.route} />}
      {today.van && <span>Van {today.van}</span>}
      {today.phone && <WorkPhone number={today.phone} />}
    </>
  );
}

/** A driver's route under their row in a list: its code, their work phone, or in Recents where the
 * conversation is (`place`), the stops done and its pace. */
export function RouteLine({ today, place }: { today: Today; place?: string }) {
  return (
    <>
      {today.route && <RouteCode code={today.route} />}
      {place ? (
        <span className="route-place">{place}</span>
      ) : (
        today.phone && <WorkPhone number={today.phone} />
      )}
      <Stops today={today} short />
      <PaceMark pace={today.pace} />
    </>
  );
}
