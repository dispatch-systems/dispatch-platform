import type { DriverDay } from '../../../../shared/contracts/index.js';
import { dateFormatter } from '../../lib/date-format.js';

const weekday = (date: string) =>
  dateFormatter('en-US', { weekday: 'narrow', timeZone: 'UTC' }).format(
    new Date(`${date}T00:00:00Z`),
  );
const state = (value: boolean | null) =>
  `driver-day${value ? ' on' : value === null ? ' uncollected' : ''}`;
const said = (value: boolean | null) => (value === null ? 'not collected' : value ? 'yes' : 'no');

/** The last fourteen days side by side: clocked in on Paycom above drove for Amazon. */
export function DayStrip({ days }: { days: DriverDay[] }) {
  const rows: [string, (day: DriverDay) => boolean | null][] = [
    ['Clocked in', (day) => day.clockedIn],
    ['Drove', (day) => day.drove],
  ];
  return (
    <div className="driver-days">
      <div className="driver-days-row head" aria-hidden="true">
        <span />
        {days.map((day) => (
          <span key={day.date}>
            <b>{weekday(day.date)}</b>
            {Number(day.date.slice(8))}
          </span>
        ))}
      </div>
      {rows.map(([label, value]) => (
        <div className="driver-days-row" key={label}>
          <span>{label}</span>
          {days.map((day) => (
            <span
              key={day.date}
              className={state(value(day))}
              title={`${day.date}, ${label.toLowerCase()}: ${said(value(day))}`}
            />
          ))}
        </div>
      ))}
      <div className="driver-days-key">
        <span>
          <span className="driver-day on" /> Yes
        </span>
        <span>
          <span className="driver-day" /> No
        </span>
        <span>
          <span className="driver-day uncollected" /> Not collected
        </span>
      </div>
    </div>
  );
}
