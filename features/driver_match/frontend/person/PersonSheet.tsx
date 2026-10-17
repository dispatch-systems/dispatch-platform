import { GitMerge } from 'lucide-react';
import { useState } from 'react';
import type { Person, PersonDetails, PersonId } from '../../api/index.js';
import { splitOff, usePersonDetails } from '../../api/client.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { time } from '../../../../core/shell/frontend/lib/format.js';
import { ConfirmDialog, DataState, Modal } from '../../../../core/shell/frontend/ui/index.js';
import { dataAmount, dataLabel, eventText, goesBy, shortId, sourceLabel } from '../driver-match.js';
import { DataIcon } from '../parts/DataIcon.js';
import { Face } from '../parts/Face.js';
import { RoleTag } from '../parts/Tags.js';
import { DayStrip } from './DayStrip.js';
import { MergeDialog } from './MergeDialog.js';
import { SheetIds } from './SheetIds.js';

/** One person in full: their IDs, what they appear in, their last fourteen days and history. */
export function PersonSheet({
  code,
  people,
  timezone,
  today,
  onClose,
}: {
  code: string;
  people: Person[];
  timezone: string;
  today: string;
  onClose: () => void;
}) {
  const details = usePersonDetails(code);
  const [merging, setMerging] = useState(false);
  const [splitting, setSplitting] = useState<PersonId | null>(null);
  const split = useAction((id: PersonId) => splitOff(details.data!.person.code, id.source, id.id), {
    success: (id) => `${sourceLabel(id.source)} ID ${shortId(id)} is now its own person`,
  });
  const person = details.data?.person;
  return (
    <Modal
      variant="sheet"
      onClose={onClose}
      title={person ? <SheetTitle person={person} /> : 'Person'}
    >
      <DataState data={details.data} error={details.error} retry={details.refresh}>
        {(data) => (
          <>
            <SheetIds person={data.person} today={today} onSplit={setSplitting} />
            <Appears data={data} />
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
              <MergeDialog person={data.person} people={people} onClose={() => setMerging(false)} />
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
                {splitting.name && ` (${splitting.name})`}
              </ConfirmDialog>
            )}
          </>
        )}
      </DataState>
    </Modal>
  );
}

function SheetTitle({ person }: { person: Person }) {
  const alias = goesBy(person);
  return (
    <span className="driver-sheet-person">
      <Face person={person} size="large" />
      <span>
        <span className="driver-sheet-name">{person.name}</span>
        <span className="driver-sheet-meta">
          <span className="driver-code">{person.code}</span>
          <RoleTag role={person.role} />
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

function Appears({ data }: { data: PersonDetails }) {
  return (
    <section className="driver-sheet-section" aria-labelledby="driver-activity">
      <h3 id="driver-activity">Appears in</h3>
      {data.appears.length ? (
        <dl className="driver-activity">
          {data.appears.map(({ data: kind, count }) => (
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

function History({ data, timezone }: { data: PersonDetails; timezone: string }) {
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
