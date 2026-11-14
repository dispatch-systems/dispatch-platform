import { CircleCheck, ClipboardCheck } from 'lucide-react';
import type { CSSProperties } from 'react';
import type { DvicCoverage } from '../../api/index.js';
import type { WidgetProps } from '../../../../core/shell/frontend/runtime/slots.js';
import { useCollectionUpdates } from '../../../../core/collection/frontend/live-collection.js';
import { dspHash } from '../../../../core/shell/frontend/runtime/navigation.js';
import { localDate, shiftDate } from '../../../../core/shell/frontend/lib/day.js';
import { performancePolicy } from '../../../../core/shell/frontend/lib/performance-policy.js';
import { summaryUrl, useDvicStatus, useSummary, useWholeDay } from '../../api/client.js';
import { usePublished } from '../published.js';
import {
  fleetLabel,
  inspectionBand,
  inspectionClock,
  inspectionDate,
  inspectionDuration as duration,
  inspectionInitials,
  inspectionShare,
  inspectionWeekday,
  isCollected,
  latestCollected,
  shortDrivers,
  type ShortDriver,
} from '../dvic.js';
import './latest-dvic.css';

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
  const count = drivers?.length;
  const failed = status.error || day.error;
  const detail = columns >= 3 && rows >= 2;
  return (
    <a
      className="dvic-latest"
      href={dspHash(view.dsp.id, 'dvic', date ? { day: date } : undefined)}
      data-columns={columns}
      data-rows={rows}
    >
      <header>
        <ClipboardCheck aria-hidden="true" />
        {/* One column has room for its date beside DVIC alone. */}
        <strong>{columns === 1 ? 'DVIC' : 'Latest DVIC'}</strong>
        {date && <span>{inspectionDate(date, columns >= 3)}</span>}
      </header>
      {failed ? (
        <p className="dvic-latest-note">{failed}</p>
      ) : status.data && !date ? (
        <p className="dvic-latest-note">Not collected yet</p>
      ) : !drivers ? null : !drivers.length ? (
        <Quiet
          date={date!}
          covered={status.data!.covered}
          columns={columns}
          rows={rows}
          publication={publication}
        />
      ) : columns === 1 ? (
        <>
          <div className="dvic-latest-count">
            <strong>{count}</strong>
            <span>short</span>
          </div>
          {rows > 1 && (
            <ol className="dvic-latest-list">
              {drivers.map((driver) => (
                <Driver key={driver.shortest.driverId} driver={driver} brief />
              ))}
            </ol>
          )}
        </>
      ) : (
        <>
          <ol className="dvic-latest-list">
            {drivers.map((driver) => (
              <Driver
                key={driver.shortest.driverId}
                driver={driver}
                detail={detail}
                bar={columns >= 4 && rows >= 3}
              />
            ))}
          </ol>
          {rows > 1 && <footer>{count} short</footer>}
        </>
      )}
    </a>
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
      <div className="dvic-latest-zero">
        <CircleCheck aria-hidden="true" />
        <span>
          <strong>0</strong> short
        </span>
      </div>
      {summary && (
        <ol className="dvic-latest-days" data-across={across || undefined}>
          {days.map((day) => {
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
    <li data-band={inspectionBand(shortest)}>
      {detail && (
        <span className="dvic-latest-initials" aria-hidden="true">
          {inspectionInitials(shortest)}
        </span>
      )}
      <span className="dvic-latest-driver">
        <strong>
          {shortest.driverName || shortest.driverId}
          {count > 1 && <span className="dvic-latest-times">×{count}</span>}
        </strong>
        {detail && (
          <small>
            {inspectionClock(shortest.startTime)} · {fleetLabel(shortest.fleetType)}
          </small>
        )}
      </span>
      {bar && (
        <span className="dvic-latest-bar" aria-hidden="true">
          <span style={{ width: `${inspectionShare(shortest) * 100}%` }} />
        </span>
      )}
      {!brief && (
        <span className="dvic-latest-took">
          {duration(shortest.durationSeconds)} <small>of {duration(shortest.minimumSeconds)}</small>
        </span>
      )}
    </li>
  );
}
