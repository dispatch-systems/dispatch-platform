import { GitMerge } from 'lucide-react';
import { useRef, useState } from 'react';
import type { GivenRole, Person, PersonDetails, PersonId } from '../../api/index.js';
import { giveRole, removePhone, splitOff, usePersonDetails } from '../../api/client.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { phoneText, time } from '../../../../core/shell/frontend/lib/format.js';
import {
  ConfirmDialog,
  DataState,
  ErrorBox,
  Modal,
} from '../../../../core/shell/frontend/ui/index.js';
import {
  dataAmount,
  dataLabel,
  eventText,
  givenLabel,
  givenRoles,
  goesBy,
  shortId,
  sourceLabel,
} from '../driver-match.js';
import { DataIcon } from '../parts/DataIcon.js';
import { Face } from '../parts/Face.js';
import { RoleTag } from '../parts/Tags.js';
import { DayStrip } from './DayStrip.js';
import { MergeDialog } from './MergeDialog.js';
import { SheetIds } from './SheetIds.js';

/**
 * One person in full: their role, their IDs, the phone numbers someone added to theirs, what they
 * appear in, their last fourteen days and history.
 */
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
            {/* The former keep the role someone gave them, unseen, for when they come back. */}
            {data.person.role !== 'former' && (
              <RolePicker key={data.person.given ?? ''} person={data.person} />
            )}
            <SheetIds person={data.person} today={today} onSplit={setSplitting} />
            {data.addedPhones.length > 0 && (
              <AddedPhones code={data.person.code} phones={data.addedPhones} />
            )}
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

/**
 * The phone numbers someone added to theirs, beside their sources', each taken off by Remove: gone
 * at once, and back if that fails.
 */
function AddedPhones({ code, phones }: { code: string; phones: string[] }) {
  const remove = useAction((phone: string) => removePhone(code, phone), {
    inline: true,
    success: (phone) => `${phoneText(phone)} is removed`,
  });
  // Numbers removed here, gone until the person is read again without them.
  const [removed, setRemoved] = useState<string[]>([]);
  const take = (phone: string) => {
    setRemoved((before) => [...before, phone]);
    void remove.run(phone).then((done) => {
      if (!done) setRemoved((before) => before.filter((each) => each !== phone));
    });
  };
  const shown = phones.filter((phone) => !removed.includes(phone));
  if (!shown.length && !remove.error) return null;
  return (
    <section className="driver-sheet-section" aria-labelledby="driver-phones">
      <h3 id="driver-phones">Added numbers</h3>
      <ErrorBox message={remove.error} />
      {shown.map((phone) => (
        <div className="driver-id" key={phone}>
          <strong>{phoneText(phone)}</strong>
          <div className="driver-id-actions">
            <button onClick={() => take(phone)}>Remove</button>
          </div>
        </div>
      ))}
    </section>
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
          <RoleTag person={person} />
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

/**
 * The role someone gave them, else Automatic, from their sources: saved as it's chosen, one
 * choice after another in the order made; one that fails goes back to what it was.
 */
function RolePicker({ person }: { person: Person }) {
  const [chosen, setChosen] = useState(person.given ?? '');
  const save = useAction((role: GivenRole | null) => giveRole(person.code, role), {
    inline: true,
    success: (role) =>
      role ? `${person.name} is now ${givenLabel(role)}` : `${person.name}'s role is automatic`,
  });
  const queue = useRef(Promise.resolve());
  return (
    <section className="driver-sheet-section" aria-labelledby="driver-role">
      <h3 id="driver-role">Role</h3>
      <ErrorBox message={save.error} />
      <select
        aria-labelledby="driver-role"
        value={chosen}
        onChange={(event) => {
          const role = event.target.value as GivenRole | '';
          setChosen(role);
          queue.current = queue.current.then(async () => {
            if (!(await save.run(role || null))) setChosen(person.given ?? '');
          });
        }}
      >
        <option value="">Automatic</option>
        {givenRoles.map((role) => (
          <option key={role.id} value={role.id}>
            {role.label}
          </option>
        ))}
      </select>
    </section>
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
