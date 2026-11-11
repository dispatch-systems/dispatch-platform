import { useCallback, useEffect, useId, useState } from 'react';
import { ChevronRight, Phone, Search, Users } from 'lucide-react';
import type { CommunicationsWorkspace } from '../../api/index.js';
import { getContacts } from '../../api/client.js';
import { Modal } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { phoneText } from '../../../../core/shell/frontend/lib/format.js';
import type { Target } from '../conversations/target.js';
import { Face } from '../shared/Face.js';
import { memberTarget } from '../slack/members.js';
import { ListEmpty } from '../shared/List.js';
import { roleText } from '../shared/format.js';
import { Name } from '../shared/Name.js';
import { useFound } from '../shared/useFound.js';
import { textNumber } from '../texting/numbers.js';
import './new-message.css';

// New message: a number to text without saving a contact, or a member found by name, however
// it's spelt. With nothing typed, Message a group comes first while Slack is connected. The
// arrow keys move through the choices and Enter picks one.

export function NewMessage({
  workspaces,
  hasPhone,
  reach,
  group,
  close,
}: {
  workspaces: readonly CommunicationsWorkspace[];
  /** Whether the DSP has a phone to text through. */
  hasPhone: boolean;
  /** Opens the conversation with whoever was picked, or starts it. */
  reach: (target: Target) => Promise<void>;
  /** Starts a message to a group. */
  group: () => void;
  close: () => void;
}) {
  const [typed, setTyped] = useState('');
  const [active, setActive] = useState(0);
  const list = useId();
  const name = typed.trim();
  const number = hasPhone ? textNumber(name) : undefined;
  const slack = workspaces.length > 0;
  const searching = slack && !number;
  const read = useCallback(() => getContacts(name).then((answer) => answer.contacts), [name]);
  // Those found by a name are another list from everyone.
  const { found, failed } = useFound(
    searching ? read : undefined,
    name ? 'found' : 'all',
    0,
    typed ? 150 : 0,
  );
  const contacts = searching ? found : [];
  // Each answer is gone through from its first.
  useEffect(() => setActive(0), [found, number, slack]);
  // The one the arrow keys reach stays in view.
  useEffect(() => {
    document.getElementById(`${list}-${active}`)?.scrollIntoView({ block: 'nearest' });
  }, [list, active]);
  const chosen = useAction(reach);
  // With several workspaces, each says which it's in.
  const named = new Map(workspaces.map((workspace) => [workspace.teamId, workspace.name]));
  const several = named.size > 1;
  const option = (at: number) => `${list}-${at}`;
  // A number or, with nothing typed, a group comes before the people found.
  const grouping = slack && !name;
  const first = Boolean(number || grouping);
  const options = (contacts?.length ?? 0) + (first ? 1 : 0);
  const choose = (at: number) => {
    if (chosen.busy) return;
    if (number && at === 0) return void chosen.run({ kind: 'number', number });
    if (grouping && at === 0) return group();
    const contact = contacts?.[first ? at - 1 : at];
    const target = contact && memberTarget(contact);
    if (target) void chosen.run(target);
  };
  const label = slack ? (hasPhone ? 'Name or phone number' : 'Name') : 'Phone number';
  return (
    <Modal title="New message" onClose={close} initialFocus="input">
      <div className="new-message">
        <label className="search new-search">
          <Search size={16} aria-hidden="true" />
          <input
            role="combobox"
            aria-label={label}
            aria-expanded={options > 0}
            aria-controls={list}
            aria-activedescendant={options ? option(active) : undefined}
            aria-autocomplete="list"
            placeholder={label}
            inputMode={!slack ? 'tel' : undefined}
            value={typed}
            onChange={(event) => {
              setTyped(event.target.value);
              setActive(0);
            }}
            onKeyDown={(event) => {
              if (event.key === 'ArrowDown' && options) setActive((active + 1) % options);
              else if (event.key === 'ArrowUp' && options)
                setActive((active - 1 + options) % options);
              else if (event.key === 'Enter' && options) choose(active);
              else return;
              event.preventDefault();
            }}
          />
        </label>
        {searching && failed && <ListEmpty>{failed}</ListEmpty>}
        {contacts && !contacts.length && name && !number && <ListEmpty>No matches</ListEmpty>}
        <ul id={list} role="listbox" aria-label="Recipients" className="results">
          {first && (
            <li
              id={option(0)}
              role="option"
              aria-selected={active === 0}
              className={`result${grouping ? ' group' : ''}${active === 0 ? ' active' : ''}`}
              onMouseEnter={() => setActive(0)}
              onClick={() => choose(0)}
            >
              <span className="lead">
                {number ? (
                  <Phone size={19} aria-hidden="true" />
                ) : (
                  <Users size={19} aria-hidden="true" />
                )}
              </span>
              <span className="result-body">
                <strong>{number ? `Text ${phoneText(number)}` : 'Message a group'}</strong>
              </span>
              <ChevronRight size={16} aria-hidden="true" />
            </li>
          )}
          {contacts?.map((contact, index) => {
            const at = first ? index + 1 : index;
            return (
              <li
                key={`${contact.teamId}:${contact.person.userId}`}
                id={option(at)}
                role="option"
                aria-selected={at === active}
                className={at === active ? 'result active' : 'result'}
                onMouseEnter={() => setActive(at)}
                onClick={() => choose(at)}
              >
                <Face {...contact.person} size="list" />
                <span className="result-body">
                  <strong>
                    <Name name={contact.person.name} matched={contact.matched} />
                  </strong>
                  <small>
                    {[roleText(contact.person), several && named.get(contact.teamId)]
                      .filter(Boolean)
                      .join(' · ')}
                  </small>
                </span>
                <ChevronRight size={16} aria-hidden="true" />
              </li>
            );
          })}
        </ul>
      </div>
    </Modal>
  );
}
