import type { SourceDays } from '../../api/index.js';
import { dateFormatter } from '../../../../core/shell/frontend/lib/date-format.js';
import { sourceLabel } from '../driver-match.js';

const weekday = (date: string) =>
  dateFormatter('en-US', { weekday: 'narrow', timeZone: 'UTC' }).format(
    new Date(`${date}T00:00:00Z`),
  );
const state = (value: boolean | null) =>
  `driver-day${value ? ' on' : value === null ? ' uncollected' : ''}`;
const said = (value: boolean | null) => (value === null ? 'not collected' : value ? 'yes' : 'no');

/** The last fourteen days side by side, a row for each source that saw them work. */
export function DayStrip({ dates, days }: { dates: string[]; days: SourceDays[] }) {
  return (
    <div className="driver-days">
      <div className="driver-days-row head" aria-hidden="true">
        <span />
        {dates.map((date) => (
          <span key={date}>
            <b>{weekday(date)}</b>
            {Number(date.slice(8))}
          </span>
        ))}
      </div>
      {days.map(({ source, worked }) => (
        <div className="driver-days-row" key={source}>
          <span>{sourceLabel(source)}</span>
          {worked.map((value, index) => (
            <span
              key={dates[index]}
              className={state(value)}
              title={`${dates[index]}, ${sourceLabel(source)}: ${said(value)}`}
            />
          ))}
        </div>
      ))}
      <div className="driver-days-key">
        <span>
          <span className="driver-day on" /> Worked
        </span>
        <span>
          <span className="driver-day" /> Didn't
        </span>
        <span>
          <span className="driver-day uncollected" /> Not collected
        </span>
      </div>
    </div>
  );
}
