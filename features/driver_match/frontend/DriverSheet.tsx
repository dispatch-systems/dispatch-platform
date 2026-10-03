import { GitMerge, Split } from 'lucide-react';
import { useState } from 'react';
import type { Driver, DriverId } from '../api/index.js';
import { splitDriver, useDriverDetails } from '../api/client.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import {
  dataAmount,
  dataLabels,
  dataOrder,
  eventText,
  goesBy,
  seenRange,
  shortId,
} from './driver-match.js';
import { time } from '../../../core/shell/frontend/lib/format.js';
import { ConfirmDialog, DataState, Modal } from '../../../core/shell/frontend/ui/index.js';
import { dataIcons } from './dataIcons.js';
import { DayStrip } from './DayStrip.js';
import { DriverAvatar } from './DriverAvatar.js';
import { DriverTag } from './DriverTag.js';
import { MergeDialog } from './MergeDialog.js';

const linkLabels: Record<DriverId['linkedBy'], string> = {
  new: 'First ID',
  name: 'Same name',
  variant: 'Name variant',
  saved: 'Linked on Meal Breaks',
  person: 'Confirmed',
};
const sourceLabels = { paycom: 'Paycom', amazon: 'Amazon' };

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
      success: (id) => `${sourceLabels[id.source]} ID ${shortId(id)} is now its own person`,
    },
  );
  const driver = details.data?.driver;
  const alias = driver && goesBy(driver);
  return (
    <Modal
      variant="sheet"
      onClose={onClose}
      title={
        driver ? (
          <span className="driver-sheet-person">
            <DriverAvatar name={driver.name} code={driver.code} large />
            <span>
              <span className="driver-sheet-name">{driver.name}</span>
              <span className="driver-sheet-meta">
                <span className="driver-code">{driver.code}</span>
                <DriverTag status={driver.status} />
                {alias && <span>Goes by {alias} on Amazon</span>}
              </span>
            </span>
          </span>
        ) : (
          'Driver'
        )
      }
    >
      <DataState data={details.data} error={details.error} retry={details.refresh}>
        {(data) => (
          <>
            <section className="driver-sheet-section" aria-labelledby="driver-ids">
              <h3 id="driver-ids">IDs</h3>
              {data.driver.ids.map((id) => (
                <div className="driver-id" key={`${id.source}:${id.id}`}>
                  <span className="driver-id-mark">{id.source === 'paycom' ? 'PAY' : 'AMZ'}</span>
                  <div>
                    <strong>
                      {sourceLabels[id.source]} ·{' '}
                      <span className="driver-source-id" title={id.id}>
                        {shortId(id)}
                      </span>
                    </strong>
                    <small>{[id.name, id.department].filter(Boolean).join(' · ')}</small>
                    <small>
                      {seenRange(id.firstSeen, id.lastSeen, today)} · {linkLabels[id.linkedBy]}
                    </small>
                  </div>
                  {data.driver.ids.length > 1 && (
                    <button onClick={() => setSplitting(id)}>
                      <Split size={14} aria-hidden="true" />
                      Split off
                    </button>
                  )}
                </div>
              ))}
            </section>
            <section className="driver-sheet-section" aria-labelledby="driver-activity">
              <h3 id="driver-activity">Appears in</h3>
              {data.activity.length ? (
                <dl className="driver-activity">
                  {dataOrder
                    .filter((kind) => data.activity.some((a) => a.data === kind))
                    .map((kind) => {
                      const Icon = dataIcons[kind];
                      const count = data.activity.find((a) => a.data === kind)!.count;
                      return (
                        <div key={kind}>
                          <dt>
                            <Icon size={14} aria-hidden="true" />
                            {dataLabels[kind]}
                          </dt>
                          <dd>{dataAmount(kind, count)}</dd>
                        </div>
                      );
                    })}
                </dl>
              ) : (
                <p className="driver-match-copy">No collected data yet.</p>
              )}
            </section>
            <section className="driver-sheet-section" aria-labelledby="driver-days">
              <h3 id="driver-days">Last 14 days</h3>
              <DayStrip days={data.days} />
            </section>
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
                {sourceLabels[splitting.source]} ID {splitting.id}
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
