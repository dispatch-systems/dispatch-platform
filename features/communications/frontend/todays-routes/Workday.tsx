import { ArrowLeftRight } from 'lucide-react';
import type { Workday } from '../../api/index.js';
import { lastFour } from '../shared/format.js';
import { RouteCode, WorkPhone } from './RouteFacts.js';
import './workday.css';

// What marks a driver's work conversation's days: the line that begins each workday, and where
// they texted from another work phone that day.

/** The line across a driver's work conversation that begins a workday: the day, the route and van
 * they drove, and the work phone its first text went through; today's marked. */
export function WorkdayLine({
  label,
  workday,
  phone,
  today,
}: {
  label: string;
  workday?: Workday;
  phone: string | null;
  today: boolean;
}) {
  return (
    <div className={today ? 'workday today' : 'workday'}>
      <span className="workday-tag">
        <b>{label}</b>
        {workday?.route && <RouteCode code={workday.route} />}
        {workday?.van && <span>Van {workday.van}</span>}
        {phone && <WorkPhone number={phone} />}
      </span>
    </div>
  );
}

/** Where a driver went on texting from another work phone, `number`, that day. */
export function Switched({ number }: { number: string }) {
  return (
    <div className="switched">
      <ArrowLeftRight aria-hidden="true" />
      Switched to {lastFour(number)}
    </div>
  );
}
