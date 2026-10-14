import type { DvicInspection } from '../api/index.js';
import { useInspection } from '../api/client.js';
import {
  bandLabel,
  fleetLabel,
  inspectionBand,
  inspectionClock,
  inspectionDate,
  inspectionDuration as duration,
  inspectionShare,
  inspectionShortfall,
} from './dvic.js';
import { ErrorBox, Loading, Modal } from '../../../core/shell/frontend/ui/index.js';

export function DurationBar({ row }: { row: DvicInspection }) {
  return (
    <span className="dvic-duration-bar" data-band={inspectionBand(row)} aria-hidden="true">
      <span style={{ width: `${inspectionShare(row) * 100}%` }} />
    </span>
  );
}

export function InspectionDetail({
  id,
  from,
  to,
  publication,
  onClose,
  onSelect,
}: {
  id: string;
  from: string;
  to: string;
  publication: string;
  onClose: () => void;
  onSelect: (id: string) => void;
}) {
  const request = useInspection(id, from, to, publication);
  return (
    <Modal title="Inspection details" variant="sheet" onClose={onClose}>
      <ErrorBox message={request.error} />
      {request.error && <button onClick={request.refresh}>Retry inspections</button>}
      {request.data ? (
        <InspectionBody
          row={request.data.inspection}
          related={request.data.related}
          relatedTotal={request.data.relatedTotal}
          onSelect={onSelect}
        />
      ) : (
        !request.error && <Loading />
      )}
    </Modal>
  );
}

function InspectionBody({
  row,
  related,
  relatedTotal,
  onSelect,
}: {
  row: DvicInspection;
  related: DvicInspection[];
  relatedTotal: number;
  onSelect: (id: string) => void;
}) {
  return (
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
      {relatedTotal > 50 && (
        <p className="muted">Showing the 50 most recent of {relatedTotal} other records.</p>
      )}
      {related.length ? (
        related.map((item) => (
          <button className="dvic-related" key={item.id} onClick={() => onSelect(item.id)}>
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
  );
}
