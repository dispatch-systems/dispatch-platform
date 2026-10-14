import type { DvicSummary } from '../../../api/index.js';
import { useEffect } from 'react';
import {
  bandLabel,
  inspectionBand,
  inspectionDate,
  inspectionDuration as duration,
  inspectionWeekday,
  type Band,
} from '../../dvic.js';
import { Empty, Pagination } from '../../../../../core/shell/frontend/ui/index.js';

const pageSize = 25;

/** Drivers by day. Each cell is the driver's inspection time that day. */
export function WeekGrid({
  days,
  summary,
  today,
  collected,
  filtered,
  page,
  onPageChange,
  onSelect,
}: {
  days: string[];
  summary: DvicSummary;
  today: string;
  /** Whether a checked report covers the day. */
  collected: (date: string) => boolean;
  filtered: boolean;
  page: number;
  onPageChange: (page: number) => void;
  onSelect: (id: string) => void;
}) {
  const drivers = summary.drivers;
  const totals = new Map(summary.days.map((day) => [day.date, day.count]));
  const currentPage = Math.min(page, Math.max(0, Math.ceil(summary.totalDrivers / pageSize) - 1));
  useEffect(() => {
    if (currentPage !== page) onPageChange(currentPage);
  }, [currentPage, page, onPageChange]);
  return (
    <>
      <div className="dvic-legend" aria-hidden="true">
        {(['high', 'mid', 'low'] as Band[]).map((band) => (
          <span key={band}>
            <i data-band={band} /> {bandLabel[band]}
          </span>
        ))}
      </div>
      {summary.totalDrivers ? (
        <>
          <div className="dvic-grid-wrap">
            <table className="dvic-grid">
              <thead>
                <tr>
                  <th scope="col" className="dvic-grid-name">
                    Driver
                  </th>
                  {days.map((date) => (
                    <th scope="col" key={date} data-future={date > today}>
                      <span>{inspectionWeekday(date)}</span>
                      <strong>{Number(date.slice(8))}</strong>
                      <small>
                        {date > today || !(totals.get(date) || collected(date))
                          ? '—'
                          : (totals.get(date) ?? 0)}
                      </small>
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
                        {driver.minimums.map(duration).join(' / ')}
                      </small>
                    </th>
                    {days.map((date) => {
                      const shortest = driver.cells.find((cell) => cell.startDate === date);
                      if (!shortest)
                        return (
                          <td key={date}>
                            <span className="dvic-cell-blank" />
                          </td>
                        );
                      return (
                        <td key={date}>
                          <button
                            className="dvic-cell"
                            data-band={inspectionBand(shortest)}
                            aria-label={`${driver.driverName}, ${inspectionDate(date, true)}: ${duration(shortest.durationSeconds)}`}
                            onClick={() => onSelect(shortest.id)}
                          >
                            <strong>{duration(shortest.durationSeconds)}</strong>
                          </button>
                        </td>
                      );
                    })}
                    <td className="dvic-grid-total">{driver.count}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <nav className="dvic-grid-pagination" aria-label="Inspection drivers">
            <Pagination
              page={currentPage}
              pageSize={pageSize}
              total={summary.totalDrivers}
              onChange={onPageChange}
            />
          </nav>
        </>
      ) : days.every((date) => date > today || !collected(date)) ? (
        <Empty title="Not collected yet">
          No DVIC report collected so far covers these dates, so their short inspections are
          unknown.
        </Empty>
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
