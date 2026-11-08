import type { BackupDay } from '../../../backups/api/index.js';
import { WEEKDAYS } from '../../../shell/frontend/lib/calendar.js';
import { until, when } from './backupRules.js';

// A DSP's backups as they'd be taken: the coming five weeks, from today, a dot a day in each
// rule's colour that runs on it; and the next backups, as the server works them out where the
// DSP is.

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

export function BackupCalendar({
  days,
  upcoming,
  timezone,
  off,
}: {
  /** From today, as the server answered for the rules as they stand. */
  days: BackupDay[] | undefined;
  upcoming: string[] | undefined;
  timezone: string;
  /** Whether the schedule is off, so none of them runs. */
  off: boolean;
}) {
  // A calendar's rows begin on Monday: today's place among them leaves room before it.
  const first = days?.[0];
  const lead = first ? (new Date(`${first.date}T12:00:00Z`).getUTCDay() + 6) % 7 : 0;
  return (
    <div className={`backups-preview${off ? ' is-off' : ''}`}>
      <div>
        <h3>Coming weeks</h3>
        <div className="backups-calendar" role="grid" aria-label="Coming weeks">
          {WEEKDAYS.map((day) => (
            <b key={day} role="columnheader">
              {day}
            </b>
          ))}
          {Array.from({ length: lead }, (_, at) => (
            <span key={`lead-${at}`} aria-hidden="true" />
          ))}
          {(days ?? []).map((day, index) => {
            const [, month, date] = day.date.split('-').map(Number) as [number, number, number];
            return (
              <span
                key={day.date}
                role="gridcell"
                className={`backups-day${index === 0 ? ' is-today' : ''}`}
              >
                <small>{date === 1 || index === 0 ? `${MONTHS[month - 1]} ${date}` : date}</small>
                <span className="backups-dots">
                  {day.rules.map((rule) => (
                    <i key={rule} data-rule={rule % 5} />
                  ))}
                </span>
              </span>
            );
          })}
        </div>
      </div>
      <div>
        <h3>Next backups</h3>
        <ul className="backups-runs">
          {(upcoming ?? []).map((run) => (
            <li key={run}>
              <span>{when(run, timezone)}</span>
              <span>{until(run, timezone)}</span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
