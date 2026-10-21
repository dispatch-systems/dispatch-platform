import type { ScheduleRuleInput } from '../../api/index.js';
import { dateFormatter } from '../../../shell/frontend/lib/date-format.js';
import { DAYS, week } from './rules.js';

// A collection's schedule as it would run: this week a strip a day, a tick a run, each rule in
// its own colour; and the next runs, as the server works them out where the DSP is.

const DAY = 24 * 60;
const percent = (minutes: number) => `${((minutes / DAY) * 100).toFixed(3)}%`;
const AXIS = ['12 AM', '6 AM', '12 PM', '6 PM', '12 AM'];

/** The day of the week, Monday 0, and the minute of it, now where the DSP is. */
function nowIn(timeZone: string) {
  const parts = dateFormatter('en-US', {
    timeZone,
    weekday: 'short',
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
  }).formatToParts(new Date());
  const part = (type: string) => parts.find((each) => each.type === type)?.value ?? '';
  return {
    day: DAYS.indexOf(part('weekday')),
    minutes: Number(part('hour')) * 60 + Number(part('minute')),
  };
}
const dayOf = (at: Date, timeZone: string) =>
  dateFormatter('en-CA', { timeZone, year: 'numeric', month: '2-digit', day: '2-digit' }).format(
    at,
  );

/**
 * When a run is or was, where the DSP is: "Today, 7:45 AM", "Tomorrow, 6:00 AM", "Yesterday,
 * 9:00 PM", "Wed, 9:00 AM" within a week, else "Oct 28, 9:00 AM".
 */
export function runDay(value: string, timeZone: string, now = new Date()) {
  const at = new Date(value);
  const clock = dateFormatter('en-US', { timeZone, hour: 'numeric', minute: '2-digit' }).format(at);
  const days = Math.round(
    (Date.parse(dayOf(at, timeZone)) - Date.parse(dayOf(now, timeZone))) / 86_400_000,
  );
  const named: Record<number, string> = { [-1]: 'Yesterday', 0: 'Today', 1: 'Tomorrow' };
  const day =
    named[days] ??
    dateFormatter('en-US', {
      timeZone,
      ...(Math.abs(days) < 7 ? { weekday: 'short' } : { month: 'short', day: 'numeric' }),
    }).format(at);
  return `${day}, ${clock}`;
}
/** When a run is, in a list's few words: "7:45 AM" today, "Wed 6:00 AM" within a week, else
 * "Oct 28". */
export function runShort(value: string, timeZone: string, now = new Date()) {
  const at = new Date(value);
  const clock = dateFormatter('en-US', { timeZone, hour: 'numeric', minute: '2-digit' }).format(at);
  const days = Math.round(
    (Date.parse(dayOf(at, timeZone)) - Date.parse(dayOf(now, timeZone))) / 86_400_000,
  );
  if (days === 0) return clock;
  if (Math.abs(days) < 7)
    return `${dateFormatter('en-US', { timeZone, weekday: 'short' }).format(at)} ${clock}`;
  return dateFormatter('en-US', { timeZone, month: 'short', day: 'numeric' }).format(at);
}
/** How long until a run: "in 4 min", "in 1 hr 4 min", "in 6 hr", "in 3 days". */
export function until(value: string, now = Date.now()) {
  const minutes = Math.max(0, Math.round((Date.parse(value) - now) / 60_000));
  if (minutes < 60) return `in ${minutes} min`;
  if (minutes < 120) return `in 1 hr${minutes > 60 ? ` ${minutes - 60} min` : ''}`;
  if (minutes < DAY) return `in ${Math.round(minutes / 60)} hr`;
  const days = Math.round(minutes / DAY);
  return `in ${days} day${days === 1 ? '' : 's'}`;
}
export function WeekPreview({
  rules,
  timezone,
  runs,
  off,
}: {
  /** The rules as they'd be saved, each drawn in its place's colour. */
  rules: (ScheduleRuleInput | undefined)[];
  timezone: string;
  /** The next runs, once the server has worked them out. */
  runs: string[] | undefined;
  /** Whether the collection is off, so none of them runs. */
  off: boolean;
}) {
  const now = nowIn(timezone);
  const strips = rules.map((rule) => (rule ? week([rule]) : []));
  return (
    <div className={`collectors-preview${off ? ' is-off' : ''}`}>
      <div>
        <h3>This week</h3>
        <div className="collectors-strips">
          {DAYS.map((day, index) => {
            const today = index === now.day;
            return (
              <div className={`collectors-strip${today ? ' is-today' : ''}`} key={day}>
                <b>{day}</b>
                <div className="collectors-track">
                  {strips.flatMap((days, rule) =>
                    (days[index] ?? []).map((minutes) => (
                      <i
                        key={`${rule}-${minutes}`}
                        className={today && minutes < now.minutes ? 'past' : undefined}
                        data-rule={rule % 5}
                        style={{ left: `calc(${percent(minutes)} - 0.75px)` }}
                      />
                    )),
                  )}
                  {today && (
                    <span className="collectors-now" style={{ left: percent(now.minutes) }} />
                  )}
                </div>
              </div>
            );
          })}
        </div>
        <div className="collectors-axis" aria-hidden="true">
          {AXIS.map((label, index) => (
            <span key={index} style={{ left: `${index * 25}%` }}>
              {label}
            </span>
          ))}
        </div>
      </div>
      <div>
        <h3>Next runs</h3>
        <ul className="collectors-runs">
          {(runs ?? []).map((run) => (
            <li key={run}>
              <i className="dot" />
              <span>{runDay(run, timezone)}</span>
              <span>{until(run)}</span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
