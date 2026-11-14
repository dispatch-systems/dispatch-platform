import { AlarmClock, CircleCheck } from 'lucide-react';
import type { WidgetProps } from '../../../../core/shell/frontend/runtime/slots.js';
import { useCollectionUpdates } from '../../../../core/collection/frontend/live-collection.js';
import { dspHash } from '../../../../core/shell/frontend/runtime/navigation.js';
import { localDate } from '../../../../core/shell/frontend/lib/day.js';
import { dateFormatter } from '../../../../core/shell/frontend/lib/date-format.js';
import { useMealComparison, usePaycomSettings } from '../../api/client.js';
import { mealPageUrl } from '../../api/urls.js';
import type { MealEmployee } from '../../api/index.js';
import { clockLabel, displayMeal, parseClock, personName } from '../meal-breaks.js';
import './late-das.css';

const day = (date: string, weekday: boolean) =>
  dateFormatter('en-US', {
    timeZone: 'UTC',
    month: 'short',
    day: 'numeric',
    ...(weekday ? { weekday: 'short' } : {}),
  }).format(new Date(date + 'T12:00:00Z'));

/** How long after the last minute on time an IN DAY punch came: +14m, +1h 5m, +2h. */
function lateBy(minutes: number) {
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return `+${hours ? `${hours}h` : ''}${hours && rest ? ' ' : ''}${rest || !hours ? `${rest}m` : ''}`;
}

/**
 * Today's late DAs, as Meal Breaks' Late DAs lists them by the DSP's own rule, latest in first, at
 * any size from one cell to four by four: the count in one column, with their names under it from
 * two rows; each one's IN DAY time from two columns, their department from three by two, and how
 * late from four. It opens Meal Breaks on today with Late DAs.
 */
export function LateDasWidget({ columns, rows, view }: WidgetProps) {
  // A run that ends, wherever it started, shows here at once, as it does on Timecard.
  useCollectionUpdates();
  const today = localDate(view.dsp.timezone);
  const settings = usePaycomSettings();
  const read = useMealComparison(mealPageUrl(today, '', 'late', 'inDay', true, 0, 'all'));
  const late = read.data?.date === today ? read.data : undefined;
  const failed = settings.error || read.error;
  const rule = settings.data?.late.time;
  const detail = columns >= 3 && rows >= 2;
  const lateRows = late?.rows ?? [];
  return (
    <a
      className="late-das"
      href={dspHash(view.dsp.id, 'paycom', { late: today })}
      data-columns={columns}
      data-rows={rows}
    >
      <header>
        <AlarmClock aria-hidden="true" />
        {/* One column has room for its date beside Late alone. */}
        <strong>{columns === 1 ? 'Late' : 'Late DAs'}</strong>
        <span>{day(today, columns >= 3)}</span>
      </header>
      {failed ? (
        <p className="late-das-note">{failed}</p>
      ) : !late || !settings.data || !rule ? null : !late.counts.all ? (
        <p className="late-das-note">No Paycom punches yet</p>
      ) : !lateRows.length ? (
        <div className="late-das-none">
          <CircleCheck aria-hidden="true" />
          <span>
            <strong>0</strong> late
          </span>
          {columns > 1 && rows > 1 && <small>At or after {clockLabel(rule)}</small>}
        </div>
      ) : (
        <>
          {columns === 1 && (
            <div className="late-das-count">
              <strong>{lateRows.length}</strong>
              <span>late</span>
            </div>
          )}
          {(columns > 1 || rows > 1) && (
            <ol className="late-das-list">
              {lateRows.map((row) => (
                <Late
                  key={row.id}
                  row={row}
                  name={personName(row.name, settings.data!.nameOrder)}
                  rule={rule}
                  brief={columns === 1}
                  detail={detail}
                  by={columns >= 4}
                />
              ))}
            </ol>
          )}
          {columns > 1 && rows > 1 && (
            <footer>
              {lateRows.length} late · at or after {clockLabel(rule)}
            </footer>
          )}
        </>
      )}
    </a>
  );
}

/** A late DA's row: their name alone, `brief`, in one column; else their IN DAY time too. */
function Late({
  row,
  name,
  rule,
  brief,
  detail,
  by,
}: {
  row: MealEmployee;
  name: string;
  rule: string;
  brief: boolean;
  detail: boolean;
  by: boolean;
}) {
  const inDay = displayMeal(row).paycom.inDay;
  const from = parseClock(rule);
  const department = row.paycom?.department;
  return (
    <li>
      {detail && (
        <span className="late-das-initials" aria-hidden="true">
          {name
            .split(' ')
            .slice(0, 2)
            .map((word) => word[0])
            .join('')}
        </span>
      )}
      <span className="late-das-who">
        <strong>{name}</strong>
        {detail && department && <small>{department}</small>}
      </span>
      {!brief && inDay && (
        <>
          {by && from !== null && (
            <span className="late-das-by">{lateBy(inDay.minute - from + 1)}</span>
          )}
          <span className="late-das-time">{inDay.label}</span>
        </>
      )}
    </li>
  );
}
