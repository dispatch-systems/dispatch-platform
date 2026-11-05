import { useState } from 'react';
import { Check, UserPlus } from 'lucide-react';
import type { Conversation, Correspondent, RouteDriver } from '../../api/index.js';
import { addToPerson, saveContact, useTextPeople } from '../../api/client.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { Loading, Modal, Popover, SearchInput } from '../../../../core/shell/frontend/ui/index.js';
import { Face } from '../shared/Face.js';
import { ListEmpty } from '../shared/List.js';
import { Segments } from '../shared/Segments.js';
import './save-contact.css';

/** The most a contact's name may be, as the server keeps it. */
const NAME_MOST = 80;

/** What Save contact does with the number: adds it to someone Driver Match knows, or saves a name
 * for it. */
type Saving = 'driver' | 'contact';

/**
 * Save contact, or Edit contact once a name is saved: the number added to someone Driver Match
 * knows, today's drivers first, or a name saved for it. It closes on Cancel, Escape (focus going
 * back to the button), a press anywhere else or the button again. A driver picked or a name
 * changed and not saved, in either tab, is never just closed: it asks "Save changes?" with only
 * Save and Discard first, as the role sheet does, saving the open tab's, else the other's.
 */
export function SaveContact({
  conversation,
  drivers,
  show,
}: {
  conversation: Conversation;
  /** Today's drivers, listed first. */
  drivers: readonly RouteDriver[];
  show: (conversation: Conversation) => void;
}) {
  const saved = conversation.contact ? conversation.with.name : '';
  const [open, setOpen] = useState(false);
  const [saving, setSaving] = useState<Saving>('driver');
  const [picked, setPicked] = useState<Correspondent>();
  const [name, setName] = useState(saved);
  const [confirming, setConfirming] = useState(false);
  // Each tab's draft, kept while the other is open, so neither is lost unasked.
  const dirty: Record<Saving, boolean> = {
    driver: picked !== undefined,
    contact: name.trim() !== saved,
  };
  const changed = dirty.driver || dirty.contact;
  // What Save changes? saves: the open tab's draft, else the other's.
  const unsaved: Saving = dirty[saving] ? saving : saving === 'driver' ? 'contact' : 'driver';
  const closed = () => {
    setOpen(false);
    setPicked(undefined);
    setConfirming(false);
  };
  const leave = () => (changed ? setConfirming(true) : closed());
  const save = useAction(async (which: Saving) => {
    show(
      which === 'driver'
        ? await addToPerson(conversation.id, picked!.code!)
        : await saveContact(conversation.id, name.trim()),
    );
    closed();
  });
  // Its words, which on a phone only its mark shows.
  const label = conversation.contact ? 'Edit contact' : 'Save contact';
  const saveable = (which: Saving) =>
    (which === 'driver' ? Boolean(picked) : Boolean(name.trim())) && !save.busy;
  return (
    <>
      <Popover
        className="save-contact"
        panelClassName="contact-panel"
        label={label}
        trigger={
          <>
            <UserPlus size={15} />
            <span>{label}</span>
          </>
        }
        open={open}
        onRequestOpen={() => {
          setSaving(conversation.contact ? 'contact' : 'driver');
          setName(saved);
          setOpen(true);
        }}
        onRequestClose={leave}
      >
        {open && (
          <form
            className="contact-form"
            onSubmit={(event) => {
              event.preventDefault();
              if (saveable(saving)) void save.run(saving);
            }}
          >
            <Segments
              label={label}
              value={saving}
              onChange={(value) => setSaving(value as Saving)}
              items={[
                ['driver', 'Add to driver'],
                ['contact', conversation.contact ? 'Contact' : 'New contact'],
              ]}
            />
            {saving === 'driver' ? (
              <People drivers={drivers} picked={picked} pick={setPicked} />
            ) : (
              <label className="contact-name">
                <span>Name</span>
                <input
                  autoFocus
                  value={name}
                  maxLength={NAME_MOST}
                  onChange={(event) => setName(event.target.value)}
                />
              </label>
            )}
            <div className="contact-form-buttons">
              <button type="button" onClick={leave}>
                Cancel
              </button>
              <button type="submit" className="primary" disabled={!saveable(saving)}>
                {saving === 'driver' ? 'Add' : 'Save'}
              </button>
            </div>
          </form>
        )}
      </Popover>
      {confirming && (
        <Modal title="Save changes?" dismissible={false} onClose={() => {}}>
          <div className="form-actions">
            <button type="button" onClick={closed}>
              Discard changes
            </button>
            <button
              type="button"
              className="primary"
              disabled={!saveable(unsaved)}
              onClick={async () => {
                if (!(await save.run(unsaved))) setConfirming(false);
              }}
            >
              Save changes
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}

/** Everyone the number can be added to, found by name: today's drivers, then everyone else. */
function People({
  drivers,
  picked,
  pick,
}: {
  drivers: readonly RouteDriver[];
  picked?: Correspondent;
  pick: (person: Correspondent) => void;
}) {
  const people = useTextPeople();
  const [typed, setTyped] = useState('');
  const wanted = typed.trim().toLocaleLowerCase();
  const found = (people.data?.people ?? []).filter((person) =>
    person.name.toLocaleLowerCase().includes(wanted),
  );
  const today = new Set(drivers.map((driver) => driver.person.code));
  const todays = found.filter((person) => today.has(person.code));
  const others = found.filter((person) => !today.has(person.code));
  const row = (person: Correspondent) => (
    <li key={person.code}>
      <button
        type="button"
        aria-pressed={picked?.code === person.code}
        onClick={() => pick(person)}
      >
        <Face {...person} size="contact" />
        <strong>{person.name}</strong>
        {picked?.code === person.code && <Check size={16} aria-hidden="true" />}
      </button>
    </li>
  );
  return (
    <>
      <SearchInput label="Search people" placeholder="Search" value={typed} onChange={setTyped} />
      {people.error ? (
        <ListEmpty>{people.error}</ListEmpty>
      ) : !people.data ? (
        <Loading />
      ) : found.length === 0 ? (
        <ListEmpty>No matches</ListEmpty>
      ) : (
        <div className="contact-people">
          {todays.length > 0 && (
            <>
              <h3>Today's Routes</h3>
              <ul>{todays.map(row)}</ul>
              {others.length > 0 && <h3>Everyone</h3>}
            </>
          )}
          {others.length > 0 && <ul>{others.map(row)}</ul>}
        </div>
      )}
    </>
  );
}
