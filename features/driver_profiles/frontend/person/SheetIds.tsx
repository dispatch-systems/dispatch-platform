import { Split } from 'lucide-react';
import type { Driver, DriverId } from '../../api/index.js';
import { confirmDriver } from '../../api/client.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { ErrorBox } from '../../../../core/shell/frontend/ui/index.js';
import { confirmable, seenRange, shortId, sourceLabel } from '../driver-match.js';
import { LinkTag, SourceMark } from '../parts/Tags.js';

/**
 * A person's IDs, each with what its source says of them and how it came to them: a link a
 * match made can be confirmed, and any ID split off while they hold more than one.
 */
export function SheetIds({
  driver,
  today,
  onSplit,
}: {
  driver: Driver;
  today: string;
  onSplit: (id: DriverId) => void;
}) {
  const confirm = useAction((id: DriverId) => confirmDriver(driver.code, id.source, id.id), {
    inline: true,
    success: (id) => `${sourceLabel(id.source)} ID ${shortId(id)} is confirmed`,
  });
  return (
    <section className="driver-sheet-section" aria-labelledby="driver-ids">
      <h3 id="driver-ids">IDs</h3>
      <ErrorBox message={confirm.error} />
      {driver.ids.map((id) => (
        <div className="driver-id" key={`${id.source}:${id.id}`}>
          <SourceMark source={id.source} />
          <div>
            <strong className="driver-source-id" title={id.id}>
              {shortId(id)}
            </strong>
            <small>{[id.name, id.department].filter(Boolean).join(' · ')}</small>
            {id.emails.length + id.phones.length > 0 && (
              <small>{[...id.emails, ...id.phones].join(' · ')}</small>
            )}
            <small>{seenRange(id.firstSeen, id.lastSeen, today)}</small>
          </div>
          <div className="driver-id-actions">
            <LinkTag link={id.linkedBy} />
            {confirmable(id.linkedBy) && (
              <button disabled={confirm.busy} onClick={() => void confirm.run(id)}>
                Confirm
              </button>
            )}
            {driver.ids.length > 1 && (
              <button onClick={() => onSplit(id)}>
                <Split size={14} aria-hidden="true" />
                Split off
              </button>
            )}
          </div>
        </div>
      ))}
    </section>
  );
}
