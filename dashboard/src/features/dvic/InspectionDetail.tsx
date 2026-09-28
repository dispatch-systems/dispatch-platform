import type { DvicInspection } from '../../../../shared/contracts/dvic.js';
import {
  bandLabel,
  fleetLabel,
  inspectionBand,
  inspectionClock,
  inspectionDate,
  inspectionDuration as duration,
  inspectionShare,
  inspectionShortfall,
} from '../../lib/dvic.js';
import { Modal } from '../../ui/index.js';

export function DurationBar({ row }: { row: DvicInspection }) {
  return (
    <span className="dvic-duration-bar" data-band={inspectionBand(row)} aria-hidden="true">
      <span style={{ width: `${inspectionShare(row) * 100}%` }} />
    </span>
  );
}

export function InspectionDetail({
  row,
  related,
  onClose,
  onSelect,
}: {
  row: DvicInspection;
  related: DvicInspection[];
  onClose: () => void;
  onSelect: (row: DvicInspection) => void;
}) {
  return (
    <Modal title="Inspection details" variant="sheet" onClose={onClose}>
      <div className="dvic-detail">
        <div>
          <h3>{row.driverName || row.driverId}</h3>
          <p className="muted">
            {row.driverId} · {inspectionDate(row.startDate)}, {row.startDate.slice(0, 4)}
          </p>
        </div>
        <div className="dvic-duration-panel" data-band={inspectionBand(row)}>
          <span>Inspection duration</span>
          <strong>
            {duration(row.durationSeconds)} <small>/ {duration(row.minimumSeconds)} minimum</small>
          </strong>
          <DurationBar row={row} />
          <p>
            {inspectionShortfall(row.shortBySeconds)} below the minimum ·{' '}
            {bandLabel[inspectionBand(row)]}
          </p>
        </div>
        <dl className="dvic-facts">
          <div>
            <dt>Vehicle type</dt>
            <dd>{fleetLabel(row.fleetType)}</dd>
          </div>
          <div>
            <dt>VIN</dt>
            <dd>{row.vin}</dd>
          </div>
          <div>
            <dt>Inspection</dt>
            <dd>{row.inspectionType.replaceAll('_', ' ')}</dd>
          </div>
          <div>
            <dt>Checklist result</dt>
            <dd>{row.status}</dd>
          </div>
          <div>
            <dt>Started</dt>
            <dd>
              {inspectionDate(row.startDate)} · {inspectionClock(row.startTime)}
            </dd>
          </div>
          <div>
            <dt>Finished</dt>
            <dd>
              {inspectionDate(row.endTime.slice(0, 10))} · {inspectionClock(row.endTime)}
            </dd>
          </div>
          <div>
            <dt>Report published</dt>
            <dd>
              {inspectionDate(row.sourceReportDate)}, {row.sourceReportDate.slice(0, 4)}
            </dd>
          </div>
        </dl>
        <p className="muted">
          Times are shown as reported by Amazon, without timezone conversion. This record is flagged
          for its duration, independently of the checklist result.
        </p>
        <h4>Other short inspections this week</h4>
        {related.length > 50 && (
          <p className="muted">Showing the 50 most recent of {related.length} other records.</p>
        )}
        {related.length ? (
          related.slice(0, 50).map((item) => (
            <button className="dvic-related" key={item.id} onClick={() => onSelect(item)}>
              <span>
                {inspectionDate(item.startDate)} · {item.fleetType}
              </span>
              <strong>{duration(item.durationSeconds)}</strong>
            </button>
          ))
        ) : (
          <p className="muted">
            No other short inspections stored for this driver in the selected week.
          </p>
        )}
      </div>
    </Modal>
  );
}
