import type { DvicInspection } from '../../../../shared/contracts/dvic.js';
import { useEffect, useMemo } from 'react';
import {
  bandLabel,
  driversByCount,
  inspectionBand,
  inspectionDate,
  inspectionDuration as duration,
  inspectionWeekday,
  shortestByDay,
  type Band,
} from '../../lib/dvic.js';
import { Empty, Pagination } from '../../ui/index.js';

const pageSize = 25;

/** Drivers by day. Each cell is the driver's inspection time that day. */
export function WeekGrid({
  days,
  rows,
  today,
  filtered,
  page,
  onPageChange,
  onSelect,
}: {
  days: string[];
  rows: DvicInspection[];
  today: string;
  filtered: boolean;
  page: number;
  onPageChange: (page: number) => void;
  onSelect: (row: DvicInspection) => void;
}) {
  const drivers = useMemo(
    () =>
      driversByCount(rows).map((driver) => ({
        ...driver,
        cells: shortestByDay(driver.rows),
        minima: [...new Set(driver.rows.map((row) => row.minimumSeconds))]
          .sort((a, b) => a - b)
          .map(duration)
          .join(' / '),
      })),
    [rows],
  );
  const totals = useMemo(() => {
    const totals = new Map<string, number>();
    for (const row of rows) totals.set(row.startDate, (totals.get(row.startDate) ?? 0) + 1);
    return totals;
  }, [rows]);
  const currentPage = Math.min(page, Math.max(0, Math.ceil(drivers.length / pageSize) - 1));
  useEffect(() => {
    if (currentPage !== page) onPageChange(currentPage);
  }, [currentPage, page, onPageChange]);
  const visible = drivers.slice(currentPage * pageSize, (currentPage + 1) * pageSize);
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
                      <small>{date > today ? '—' : (totals.get(date) ?? 0)}</small>
                    </th>
                  ))}
                  <th scope="col" className="dvic-grid-total">
                    Week
                  </th>
                </tr>
              </thead>
              <tbody>
                {visible.map((driver) => (
                  <tr key={driver.driverId}>
                    <th scope="row" className="dvic-grid-name">
                      <strong>{driver.driverName}</strong>
                      <small>
                        {driver.fleets.join(' · ')} · min {driver.minima}
                      </small>
                    </th>
                    {days.map((date) => {
                      const shortest = driver.cells.get(date);
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
          <nav className="dvic-grid-pagination" aria-label="Inspection drivers">
            <Pagination
              page={currentPage}
              pageSize={pageSize}
              total={drivers.length}
              onChange={onPageChange}
            />
          </nav>
        </>
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
