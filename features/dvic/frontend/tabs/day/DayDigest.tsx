import { useEffect, useState } from 'react';
import { ChevronRight } from 'lucide-react';
import type { DvicInspection, DvicDaySummary } from '../../../api/index.js';
import { useDayInspections } from '../../../api/client.js';
import {
  bandLabel,
  inspectionBand,
  inspectionClock,
  inspectionDuration as duration,
  inspectionInitials,
  inspectionShortfall,
} from '../../dvic.js';
import {
  Empty,
  ErrorBox,
  Loading,
  Pagination,
} from '../../../../../core/shell/frontend/ui/index.js';
import { DurationBar } from '../../InspectionDetail.js';

function VehicleGroup({
  vehicles,
  label,
  date,
  query,
  publication,
  onSelect,
}: {
  vehicles: 'dot' | 'non-dot';
  label: string;
  date: string;
  query: string;
  publication: string;
  onSelect: (row: DvicInspection) => void;
}) {
  const [page, setPage] = useState(0);
  const request = useDayInspections(date, query, vehicles, page, publication);
  const data = request.data;
  useEffect(() => {
    if (data && page * 100 >= data.total && page)
      setPage(Math.max(0, Math.ceil(data.total / 100) - 1));
  }, [data, page]);
  return (
    <section className="dvic-group" aria-label={label}>
      <header>
        <span className="dvic-fleet" data-vehicles={vehicles}>
          <i aria-hidden="true" />
          {label}
        </span>
        <span>{data ? `${data.total} short · minimum ${duration(data.minimum)}` : '—'}</span>
      </header>
      <div className="dvic-items">
        <ErrorBox message={request.error} />
        {request.error && <button onClick={request.refresh}>Retry inspections</button>}
        {!data && !request.error && <Loading />}
        {data?.inspections.map((row) => (
          <button
            className="dvic-item"
            data-band={inspectionBand(row)}
            key={row.id}
            onClick={() => onSelect(row)}
            aria-label={`View ${row.driverName || row.driverId}, ${inspectionClock(row.startTime)}, ${duration(row.durationSeconds)}`}
          >
            <span className="dvic-avatar" aria-hidden="true">
              {inspectionInitials(row)}
            </span>
            <span className="dvic-driver">
              <strong>{row.driverName || row.driverId}</strong>
              <small>
                {inspectionClock(row.startTime)} · {row.fleetType} · VIN …{row.vin.slice(-6)}
              </small>
            </span>
            <span className="dvic-took">
              <strong>{duration(row.durationSeconds)}</strong>
              <small>of {duration(row.minimumSeconds)}</small>
            </span>
            <span className="dvic-gap">
              <DurationBar row={row} />
              <small>
                {inspectionShortfall(row.shortBySeconds)} short · {bandLabel[inspectionBand(row)]}
              </small>
            </span>
            <ChevronRight size={16} aria-hidden="true" />
          </button>
        ))}
        {data && <Pagination page={page} pageSize={100} total={data.total} onChange={setPage} />}
      </div>
    </section>
  );
}

/** One day's short inspections: headline numbers, repeat drivers, then records by vehicle. */
export function DayDigest({
  day,
  summary,
  query,
  publication,
  filtered,
  collected,
  onSelect,
}: {
  day: string;
  summary?: DvicDaySummary;
  query: string;
  publication: string;
  filtered: boolean;
  /** Whether a checked report covers the day. */
  collected: boolean;
  onSelect: (row: DvicInspection) => void;
}) {
  const repeat = summary?.repeatDrivers ?? [];
  const count = summary?.count ?? 0;
  const groups = [
    { vehicles: 'non-dot', label: 'Non-DOT · Cargo Van & CDV', total: summary?.nonDot ?? 0 },
    { vehicles: 'dot', label: 'DOT · Step Van', total: summary?.dot ?? 0 },
  ] as const;
  return (
    <>
      <div className="dvic-kpi">
        <span>Short inspections</span>
        <strong>{count || collected ? count : '—'}</strong>
      </div>
      {repeat.length > 0 && (
        <p className="dvic-callout" role="note">
          <strong>{repeat.length}</strong> of this day's{' '}
          {repeat.length === 1 ? 'drivers has' : 'drivers have'} three or more short inspections
          this week: {repeat.join(', ')}.
        </p>
      )}
      {count ? (
        groups
          .filter((group) => group.total)
          .map((group) => (
            <VehicleGroup
              key={`${group.vehicles}:${day}:${query}`}
              {...group}
              date={day}
              query={query}
              publication={publication}
              onSelect={onSelect}
            />
          ))
      ) : !collected ? (
        <Empty title="Not collected yet">
          No DVIC report collected so far covers this day, so its short inspections are unknown.
        </Empty>
      ) : (
        <Empty title={filtered ? 'No matching inspections' : 'No short inspections on this day'}>
          {filtered
            ? 'Try a different driver or vehicle type.'
            : 'No short inspection records are stored for this date. This does not indicate whether every driver completed an inspection.'}
        </Empty>
      )}
    </>
  );
}
