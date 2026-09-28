import { useState } from 'react';
import { ChevronRight } from 'lucide-react';
import type { DvicInspection } from '../../../../shared/contracts/dvic.js';
import {
  bandLabel,
  groupByVehicleClass,
  inspectionBand,
  inspectionClock,
  inspectionDuration as duration,
  inspectionInitials,
  inspectionShortfall,
  repeatDrivers,
} from '../../lib/dvic.js';
import { Empty } from '../../ui/index.js';
import { DurationBar } from './InspectionDetail.js';

function VehicleGroup({
  vehicles,
  label,
  minimum,
  rows,
  onSelect,
}: {
  vehicles: 'dot' | 'non-dot';
  label: string;
  minimum: number;
  rows: DvicInspection[];
  onSelect: (row: DvicInspection) => void;
}) {
  const [limit, setLimit] = useState(100);
  return (
    <section className="dvic-group" aria-label={label}>
      <header>
        <span className="dvic-fleet" data-vehicles={vehicles}>
          <i aria-hidden="true" />
          {label}
        </span>
        <span>
          {rows.length} short · minimum {duration(minimum)}
        </span>
      </header>
      <div className="dvic-items">
        {rows.slice(0, limit).map((row) => (
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
        {rows.length > limit && (
          <button className="dvic-more" onClick={() => setLimit((n) => n + 100)}>
            Show more inspections ({rows.length - limit} remaining)
          </button>
        )}
      </div>
    </section>
  );
}

/** One day's short inspections: headline numbers, repeat drivers, then records by vehicle. */
export function DayDigest({
  rows,
  week,
  filtered,
  onSelect,
}: {
  rows: DvicInspection[];
  week: DvicInspection[];
  filtered: boolean;
  onSelect: (row: DvicInspection) => void;
}) {
  const repeat = repeatDrivers(week, rows);
  return (
    <>
      <div className="dvic-kpi">
        <span>Short inspections</span>
        <strong>{rows.length}</strong>
      </div>
      {repeat.length > 0 && (
        <p className="dvic-callout" role="note">
          <strong>{repeat.length}</strong> of this day's{' '}
          {repeat.length === 1 ? 'drivers has' : 'drivers have'} three or more short inspections
          this week: {repeat.join(', ')}.
        </p>
      )}
      {rows.length ? (
        groupByVehicleClass(rows).map((group) => (
          <VehicleGroup key={group.vehicles} {...group} onSelect={onSelect} />
        ))
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
