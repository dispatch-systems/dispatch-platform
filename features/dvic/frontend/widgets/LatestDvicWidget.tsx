import { ClipboardCheck } from 'lucide-react';
import type { CSSProperties } from 'react';
import type { DvicCoverage } from '../../api/index.js';
import type { WidgetProps } from '../../../../core/shell/frontend/runtime/slots.js';
import { useCollectionUpdates } from '../../../../core/collection/frontend/live-collection.js';
import { dspHash } from '../../../../core/shell/frontend/runtime/navigation.js';
import { localDate, shiftDate } from '../../../../core/shell/frontend/lib/day.js';
import { performancePolicy } from '../../../../core/shell/frontend/lib/performance-policy.js';
import {
  WidgetBar,
  WidgetCount,
  WidgetFooter,
  WidgetLink,
  WidgetList,
  WidgetNote,
  WidgetRow,
  WidgetValue,
  WidgetZero,
  widgetFit,
  type WidgetTone,
} from '../../../../core/shell/frontend/ui/Widget.js';
import { summaryUrl, useDvicStatus, useSummary, useWholeDay } from '../../api/client.js';
import { usePublished } from '../published.js';
import {
  fleetLabel,
  inspectionBand,
  inspectionClock,
  inspectionDuration as duration,
  inspectionInitials,
  inspectionShare,
  inspectionWeekday,
  isCollected,
  latestCollected,
  shortDrivers,
  type Band,
  type ShortDriver,
} from '../dvic.js';
import './latest-dvic.css';

const tones: Record<Band, WidgetTone> = {
  high: 'good',
  mid: 'warn',
  low: 'bad',
};

/**
 * Who was short on the latest collected day, shortest first, at any size from one cell to four by
 * four: the count in one column, with the drivers' names under it from two rows; the drivers as
 * rows fit from two columns, with each one's time and van from three by two, and a bar for their
 * share of the minimum from four by three. A day nobody was short shows its 0 beside the days
 * before it from two rows. It opens DVIC on that day, whether or not anyone was short.
 */
export function LatestDvicWidget({ columns, rows, view }: WidgetProps) {
  // A run that ends, wherever it started, shows here at once, as it does on DVIC's page.
  useCollectionUpdates();
  const status = useDvicStatus(performancePolicy.recoveryPollMs);
  const date = latestCollected(status.data, localDate(view.dsp.timezone));
  const { publication } = usePublished(status.data);
  const day = useWholeDay(date ?? '', publication);
  const drivers = day.data ? shortDrivers(day.data.inspections) : undefined;
  const failed = status.error || day.error;
  const { narrow, tall, detail, wide } = widgetFit({ columns, rows });
  return (
    <WidgetLink
      className="dvic-latest"
      href={dspHash(view.dsp.id, 'dvic', date ? { day: date } : undefined)}
      columns={columns}
      rows={rows}
      icon={ClipboardCheck}
      label="Latest DVIC"
      short="DVIC"
      day={date}
    >
      {failed ? (
        <WidgetNote>{failed}</WidgetNote>
      ) : status.data && !date ? (
        <WidgetNote>Not collected yet</WidgetNote>
      ) : !drivers ? null : !drivers.length ? (
        <Quiet
          date={date!}
          covered={status.data!.covered}
          columns={columns}
          rows={rows}
          publication={publication}
        />
      ) : narrow ? (
        <>
          <WidgetCount count={drivers.length} label="short" />
          {tall && (
            <WidgetList>
              {drivers.map((driver) => (
                <Driver key={driver.shortest.driverId} driver={driver} brief />
              ))}
            </WidgetList>
          )}
        </>
      ) : (
        <>
          <WidgetList>
            {drivers.map((driver) => (
              <Driver
                key={driver.shortest.driverId}
                driver={driver}
                detail={detail}
                bar={wide && rows >= 3}
              />
            ))}
          </WidgetList>
          {tall && <WidgetFooter>{drivers.length} short</WidgetFooter>}
        </>
      )}
    </WidgetLink>
  );
}

/**
 * A day nobody was short: its 0, and from two rows a bar of short inspections for each day before
 * it, rising, five at two columns and seven from three, or across, in one column, four at two rows
 * and seven from three; a day no report covers has none, its count unknown.
 */
function Quiet({
  date,
  covered,
  columns,
  rows,
  publication,
}: {
  date: string;
  covered: DvicCoverage[];
  columns: number;
  rows: number;
  publication: string;
}) {
  const across = columns === 1;
  const shown = rows === 1 ? 0 : across ? (rows >= 3 ? 7 : 4) : columns >= 3 ? 7 : 5;
  const from = shiftDate(date, 1 - shown);
  const summary = useSummary(shown ? summaryUrl(from, date, '', '') : '', publication).data;
  const counts = new Map(summary?.days.map((day) => [day.date, day.count]));
  const days = Array.from({ length: shown }, (_, i) => shiftDate(from, i));
  const most = Math.max(1, ...counts.values());
  return (
    <div className="dvic-latest-quiet" data-days={shown || undefined}>
      <WidgetZero label="short" />
      {summary && (
        <ol className="dvic-latest-days" data-across={across || undefined}>
          {/* Across, the latest first: the stack fills from it up, and the oldest go first. */}
          {(across ? [...days].reverse() : days).map((day) => {
            const count = isCollected(covered, day) ? (counts.get(day) ?? 0) : undefined;
            const height = { '--height': `${((count ?? 0) / most) * 100}%` } as CSSProperties;
            return (
              <li key={day} data-latest={day === date || undefined}>
                <span className="dvic-latest-track" style={height}>
                  {count !== undefined && (
                    <>
                      <b>{count}</b>
                      <i data-zero={!count || undefined} />
                    </>
                  )}
                </span>
                <small>{inspectionWeekday(day)}</small>
              </li>
            );
          })}
        </ol>
      )}
    </div>
  );
}

/** A driver's row: their name alone, `brief`, in one column; else their shortest's time too. */
function Driver({
  driver: { shortest, count },
  detail = false,
  bar = false,
  brief = false,
}: {
  driver: ShortDriver;
  detail?: boolean;
  bar?: boolean;
  brief?: boolean;
}) {
  return (
    <WidgetRow
      title={shortest.driverName || shortest.driverId}
      badge={count > 1 && `×${count}`}
      detail={
        detail && `${inspectionClock(shortest.startTime)} · ${fleetLabel(shortest.fleetType)}`
      }
      initials={detail ? inspectionInitials(shortest) : undefined}
      tone={tones[inspectionBand(shortest)]}
    >
      {bar && <WidgetBar share={inspectionShare(shortest)} />}
      {!brief && (
        <WidgetValue of={`of ${duration(shortest.minimumSeconds)}`}>
          {duration(shortest.durationSeconds)}
        </WidgetValue>
      )}
    </WidgetRow>
  );
}
