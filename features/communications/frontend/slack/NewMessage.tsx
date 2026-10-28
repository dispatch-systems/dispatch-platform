import { useCallback, useEffect, useId, useState } from 'react';
import { ChevronRight, Search, Users } from 'lucide-react';
import type { CommunicationsWorkspace } from '../../api/index.js';
import { getContacts } from '../../api/client.js';
import { Modal } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import type { Target } from '../conversations/target.js';
import { Face } from '../shared/Face.js';
import { memberTarget } from './members.js';
import { ListEmpty } from '../shared/List.js';
import { roleText } from '../shared/format.js';
import { Name } from '../shared/Name.js';
import { useFound } from '../shared/useFound.js';
import './new-message.css';

// New message: whom to write to, found by their name however it's spelt, the closest first,
// with the letters that matched marked; with nothing typed, Message a group first. The arrow keys
// move through them and Enter picks one.

export function NewMessage({
  workspaces,
  reach,
  group,
  close,
}: {
  workspaces: readonly CommunicationsWorkspace[];
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
  const read = useCallback(() => getContacts(name).then((answer) => answer.contacts), [name]);
  // Those found by a name are another list from everyone.
  const { found, failed } = useFound(read, name ? 'found' : 'all', 0, typed ? 150 : 0);
  // Each answer is gone through from its first.
  useEffect(() => setActive(0), [found]);
  // The one the arrow keys reach stays in view.
  useEffect(() => {
    document.getElementById(`${list}-${active}`)?.scrollIntoView({ block: 'nearest' });
  }, [list, active]);
  const chosen = useAction(reach);
  // With several workspaces, each says which it's in.
  const named = new Map(workspaces.map((workspace) => [workspace.teamId, workspace.name]));
  const several = named.size > 1;
  const option = (at: number) => `${list}-${at}`;
  // With nothing typed, a group comes first, then everyone.
  const grouping = !name;
  const options = (found?.length ?? 0) + (grouping ? 1 : 0);
  const choose = (at: number) => {
    if (grouping && at === 0) return group();
    const contact = found?.[grouping ? at - 1 : at];
    const target = contact && memberTarget(contact);
    if (target && !chosen.busy) void chosen.run(target);
  };
  return (
    <Modal title="New message" onClose={close} initialFocus="input">
      <div className="new-message">
        <label className="search new-search">
          <Search size={16} aria-hidden="true" />
          <input
            role="combobox"
            aria-label="Name"
            aria-expanded={options > 0}
            aria-controls={list}
            aria-activedescendant={options ? option(active) : undefined}
            aria-autocomplete="list"
            placeholder="Name"
            value={typed}
            onChange={(event) => setTyped(event.target.value)}
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
        {failed && <ListEmpty>{failed}</ListEmpty>}
        {found && !found.length && name && <ListEmpty>No matches</ListEmpty>}
        <ul id={list} role="listbox" aria-label="People" className="results">
          {grouping && (
            <li
              id={option(0)}
              role="option"
              aria-selected={active === 0}
              className={active === 0 ? 'result group active' : 'result group'}
              onMouseEnter={() => setActive(0)}
              onClick={() => choose(0)}
            >
              <span className="lead">
                <Users size={19} aria-hidden="true" />
              </span>
              <span className="result-body">
                <strong>Message a group</strong>
              </span>
              <ChevronRight size={16} aria-hidden="true" />
            </li>
          )}
          {found?.map((contact, index) => {
            const at = grouping ? index + 1 : index;
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
