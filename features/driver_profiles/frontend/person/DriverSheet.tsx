import { GitMerge } from 'lucide-react';
import { useState } from 'react';
import type { Driver, DriverDetails, DriverId } from '../../api/index.js';
import { splitDriver, useDriverDetails } from '../../api/client.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { time } from '../../../../core/shell/frontend/lib/format.js';
import { ConfirmDialog, DataState, Modal } from '../../../../core/shell/frontend/ui/index.js';
import {
  dataAmount,
  dataKinds,
  dataLabel,
  eventText,
  goesBy,
  shortId,
  sourceLabel,
} from '../driver-match.js';
import { DataIcon } from '../parts/DataIcon.js';
import { DriverAvatar } from '../parts/DriverAvatar.js';
import { RoleTag } from '../parts/Tags.js';
import { DayStrip } from './DayStrip.js';
import { MergeDialog } from './MergeDialog.js';
import { SheetIds } from './SheetIds.js';

/** One person in full: their IDs, what they appear in, their last fourteen days and history. */
export function DriverSheet({
  code,
  drivers,
  timezone,
  today,
  onClose,
}: {
  code: string;
  drivers: Driver[];
  timezone: string;
  today: string;
  onClose: () => void;
}) {
  const details = useDriverDetails(code);
  const [merging, setMerging] = useState(false);
  const [splitting, setSplitting] = useState<DriverId | null>(null);
  const split = useAction(
    (id: DriverId) => splitDriver(details.data!.driver.code, id.source, id.id),
    {
      success: (id) => `${sourceLabel(id.source)} ID ${shortId(id)} is now its own person`,
    },
  );
  const driver = details.data?.driver;
  return (
    <Modal
      variant="sheet"
      onClose={onClose}
      title={driver ? <SheetTitle driver={driver} /> : 'Driver'}
    >
      <DataState data={details.data} error={details.error} retry={details.refresh}>
        {(data) => (
          <>
            <SheetIds driver={data.driver} today={today} onSplit={setSplitting} />
            <Activity data={data} />
            {data.days.length > 0 && (
              <section className="driver-sheet-section" aria-labelledby="driver-days">
                <h3 id="driver-days">Last 14 days</h3>
                <DayStrip dates={data.dates} days={data.days} />
              </section>
            )}
            <History data={data} timezone={timezone} />
            <div className="driver-sheet-actions">
              <button onClick={() => setMerging(true)}>
                <GitMerge size={15} aria-hidden="true" />
                Merge with…
              </button>
            </div>
            {merging && (
              <MergeDialog
                driver={data.driver}
                drivers={drivers}
                onClose={() => setMerging(false)}
              />
            )}
            {splitting && (
              <ConfirmDialog
                title="Split this ID off?"
                confirm="Split off"
                busy={split.busy}
                onCancel={() => setSplitting(null)}
                onConfirm={async () => {
                  if (await split.run(splitting)) setSplitting(null);
                }}
              >
                {sourceLabel(splitting.source)} ID {splitting.id}
                {splitting.name && ` (${splitting.name})`} moves to a new person with a new code.
                The two are kept apart from then on.
              </ConfirmDialog>
            )}
          </>
        )}
      </DataState>
    </Modal>
  );
}

function SheetTitle({ driver }: { driver: Driver }) {
  const alias = goesBy(driver);
  return (
    <span className="driver-sheet-person">
      <DriverAvatar driver={driver} large />
      <span>
        <span className="driver-sheet-name">{driver.name}</span>
        <span className="driver-sheet-meta">
          <span className="driver-code">{driver.code}</span>
          <RoleTag role={driver.role} />
          {alias && (
            <span>
              Goes by {alias.name} on {alias.source}
            </span>
          )}
        </span>
      </span>
    </span>
  );
}

function Activity({ data }: { data: DriverDetails }) {
  return (
    <section className="driver-sheet-section" aria-labelledby="driver-activity">
      <h3 id="driver-activity">Appears in</h3>
      {data.activity.length ? (
        <dl className="driver-activity">
          {dataKinds
            .map((kind) => data.activity.find((a) => a.data === kind))
            .filter((a) => a !== undefined)
            .map(({ data: kind, count }) => (
              <div key={kind}>
                <dt>
                  <DataIcon kind={kind} size={14} />
                  {dataLabel(kind)}
                </dt>
                <dd>{dataAmount(kind, count)}</dd>
              </div>
            ))}
        </dl>
      ) : (
        <p className="driver-match-copy">No collected data yet.</p>
      )}
    </section>
  );
}

function History({ data, timezone }: { data: DriverDetails; timezone: string }) {
  return (
    <section className="driver-sheet-section" aria-labelledby="driver-history">
      <h3 id="driver-history">History</h3>
      <ol className="driver-history">
        {data.history.map((event, index) => (
          <li
            key={`${event.at}:${index}`}
            className={
              event.kind !== 'added' && (event.kind !== 'linked' || event.link === 'person')
                ? 'decided'
                : undefined
            }
          >
            <i aria-hidden="true" />
            <div>
              {eventText(event)}
              <small>
                {time(event.at, timezone)}
                {event.actor && ` · ${event.actor}`}
              </small>
            </div>
          </li>
        ))}
      </ol>
    </section>
  );
}
