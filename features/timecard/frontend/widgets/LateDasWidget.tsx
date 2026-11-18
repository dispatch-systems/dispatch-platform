import { AlarmClock } from 'lucide-react';
import type { WidgetProps } from '../../../../core/shell/frontend/runtime/slots.js';
import { useCollectionUpdates } from '../../../../core/collection/frontend/live-collection.js';
import { dspHash } from '../../../../core/shell/frontend/runtime/navigation.js';
import { localDate } from '../../../../core/shell/frontend/lib/day.js';
import {
  WidgetCount,
  WidgetFooter,
  WidgetLink,
  WidgetList,
  WidgetNote,
  WidgetPill,
  WidgetRow,
  WidgetValue,
  WidgetZero,
  initials,
  widgetFit,
} from '../../../../core/shell/frontend/ui/Widget.js';
import { useMealComparison, usePaycomSettings } from '../../api/client.js';
import { mealPageUrl } from '../../api/urls.js';
import type { MealEmployee } from '../../api/index.js';
import { clockLabel, displayMeal, parseClock, personName } from '../meal-breaks.js';

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
  const { narrow, tall, detail, wide } = widgetFit({ columns, rows });
  const lateRows = late?.rows ?? [];
  return (
    <WidgetLink
      href={dspHash(view.dsp.id, 'paycom', { late: today })}
      columns={columns}
      rows={rows}
      icon={AlarmClock}
      label="Late DAs"
      short="Late"
      day={today}
    >
      {failed ? (
        <WidgetNote>{failed}</WidgetNote>
      ) : !late || !settings.data || !rule ? null : !late.counts.all ? (
        <WidgetNote>No Paycom punches yet</WidgetNote>
      ) : !lateRows.length ? (
        <WidgetZero label="late">{!narrow && tall && `At or after ${clockLabel(rule)}`}</WidgetZero>
      ) : (
        <>
          {narrow && <WidgetCount count={lateRows.length} label="late" />}
          {(!narrow || tall) && (
            <WidgetList>
              {lateRows.map((row) => (
                <Late
                  key={row.id}
                  row={row}
                  name={personName(row.name, settings.data!.nameOrder)}
                  rule={rule}
                  brief={narrow}
                  detail={detail}
                  by={wide}
                />
              ))}
            </WidgetList>
          )}
          {!narrow && tall && (
            <WidgetFooter>
              {lateRows.length} late · at or after {clockLabel(rule)}
            </WidgetFooter>
          )}
        </>
      )}
    </WidgetLink>
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
  return (
    <WidgetRow
      title={name}
      detail={detail && row.paycom?.department}
      initials={detail ? initials(name) : undefined}
    >
      {!brief && inDay && (
        <>
          {by && from !== null && <WidgetPill>{lateBy(inDay.minute - from + 1)}</WidgetPill>}
          <WidgetValue>{inDay.label}</WidgetValue>
        </>
      )}
    </WidgetRow>
  );
}
