import { ClipboardCheck } from 'lucide-react';
import type { WidgetProps } from '../../../../core/shell/frontend/runtime/slots.js';
import { useCollectionUpdates } from '../../../../core/collection/frontend/live-collection.js';
import { dspHash } from '../../../../core/shell/frontend/runtime/navigation.js';
import { localDate } from '../../../../core/shell/frontend/lib/day.js';
import { performancePolicy } from '../../../../core/shell/frontend/lib/performance-policy.js';
import { useDayInspections, useDvicStatus } from '../../api/client.js';
import {
  fleetLabel,
  inspectionBand,
  inspectionClock,
  inspectionDate,
  inspectionDuration as duration,
  inspectionInitials,
  inspectionShare,
  latestCollected,
  publication as publishedAt,
  shortDrivers,
  type ShortDriver,
} from '../dvic.js';
import './latest-dvic.css';

/**
 * Who was short on the latest collected day, shortest first: the count alone in one column, the
 * drivers as rows fit from two, with each one's time and van from three, and a bar for their share
 * of the minimum from four by three. It opens DVIC on that day, whether or not anyone was short.
 */
export function LatestDvicWidget({ columns, rows, view }: WidgetProps) {
  // A run that ends, wherever it started, shows here at once, as it does on DVIC's page.
  useCollectionUpdates();
  const status = useDvicStatus(performancePolicy.recoveryPollMs);
  const date = latestCollected(status.data, localDate(view.dsp.timezone));
  const day = useDayInspections(date ?? '', '', '', 0, publishedAt(status.data).publication);
  const drivers = day.data ? shortDrivers(day.data.inspections) : undefined;
  // A day of more than a page of short inspections may have more drivers than the page names.
  const more = day.data ? day.data.total > day.data.inspections.length : false;
  const count = drivers && `${drivers.length}${more ? '+' : ''}`;
  const failed = status.error || day.error;
  const detail = columns >= 3;
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
        {date && <span>{inspectionDate(date, detail)}</span>}
      </header>
      {failed ? (
        <p className="dvic-latest-note">{failed}</p>
      ) : status.data && !date ? (
        <p className="dvic-latest-note">Not collected yet</p>
      ) : !drivers || !count ? null : columns === 1 || !drivers.length ? (
        <div className="dvic-latest-count">
          <strong>{count}</strong>
          <span>short</span>
        </div>
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

function Driver({
  driver: { shortest, count },
  detail,
  bar,
}: {
  driver: ShortDriver;
  detail: boolean;
  bar: boolean;
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
      <span className="dvic-latest-took">
        {duration(shortest.durationSeconds)} <small>of {duration(shortest.minimumSeconds)}</small>
      </span>
    </li>
  );
}
