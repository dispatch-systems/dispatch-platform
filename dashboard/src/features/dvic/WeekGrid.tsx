import type { DvicInspection } from '../../../../shared/contracts/dvic.js';
import {
  bandLabel,
  bandRank,
  driversByCount,
  inspectionBand,
  inspectionDate,
  inspectionDuration as duration,
  shortestFirst,
  type Band,
} from '../../lib/dvic.js';
import { Empty } from '../../ui/index.js';

const weekday = (date: string) =>
  new Intl.DateTimeFormat('en-US', { timeZone: 'UTC', weekday: 'short' }).format(
    new Date(date + 'T12:00:00Z'),
  );

/** Drivers by day. Each cell is the driver's inspection time that day. */
export function WeekGrid({
  days,
  rows,
  today,
  filtered,
  onSelect,
}: {
  days: string[];
  rows: DvicInspection[];
  today: string;
  filtered: boolean;
  onSelect: (row: DvicInspection) => void;
}) {
  const drivers = driversByCount(rows);
  const totals = days.map((date) => rows.filter((row) => row.startDate === date).length);
  return (
    <>
      <div className="dvic-legend" aria-hidden="true">
        {(['high', 'mid', 'low'] as Band[]).map((band) => (
          <span key={band}>
            <i data-band={band} /> {bandLabel[band]}
          </span>
        ))}
      </div>
      {drivers.length ? (
        <div className="dvic-grid-wrap">
          <table className="dvic-grid">
            <thead>
              <tr>
                <th scope="col" className="dvic-grid-name">
                  Driver
                </th>
                {days.map((date, index) => (
                  <th scope="col" key={date} data-future={date > today}>
                    <span>{weekday(date)}</span>
                    <strong>{Number(date.slice(8))}</strong>
                    <small>{date > today ? '—' : totals[index]}</small>
                  </th>
                ))}
                <th scope="col" className="dvic-grid-total">
                  Week
                </th>
              </tr>
            </thead>
            <tbody>
              {drivers.map((driver) => (
                <tr key={driver.driverId}>
                  <th scope="row" className="dvic-grid-name">
                    <strong>{driver.driverName}</strong>
                    <small>
                      {driver.fleets.join(' · ')} · min{' '}
                      {[...new Set(driver.rows.map((row) => row.minimumSeconds))]
                        .sort((a, b) => a - b)
                        .map(duration)
                        .join(' / ')}
                    </small>
                  </th>
                  {days.map((date) => {
                    const hits = shortestFirst(driver.rows.filter((row) => row.startDate === date));
                    const shortest = hits[0];
                    if (!shortest)
                      return (
                        <td key={date}>
                          <span className="dvic-cell-blank" />
                        </td>
                      );
                    const worst = hits
                      .map(inspectionBand)
                      .sort((a, b) => bandRank[a] - bandRank[b])[0]!;
                    return (
                      <td key={date}>
                        <button
                          className="dvic-cell"
                          data-band={worst}
                          aria-label={`${driver.driverName}, ${inspectionDate(date, true)}: ${duration(shortest.durationSeconds)}`}
                          onClick={() => onSelect(shortest)}
                        >
                          <strong>{duration(shortest.durationSeconds)}</strong>
                        </button>
                      </td>
                    );
                  })}
                  <td className="dvic-grid-total">{driver.rows.length}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <Empty title={filtered ? 'No matching inspections' : 'No short inspections stored'}>
          {filtered
            ? 'Try a different driver or vehicle type.'
            : 'No short inspection records are stored for these dates. This does not indicate whether every driver completed an inspection.'}
        </Empty>
      )}
    </>
  );
}
