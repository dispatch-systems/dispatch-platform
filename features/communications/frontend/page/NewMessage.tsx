import { useEffect, useId, useState } from 'react';
import { ChevronRight, Search } from 'lucide-react';
import type { CommunicationsWorkspace, Contact } from '../../api/index.js';
import { getContacts } from '../../api/client.js';
import { Modal } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { messageOf } from '../../../../core/shell/frontend/lib/errors.js';
import { Face } from './Face.js';
import { Name, roles } from './parts.js';

// New message: whom to write to, found by their name however it's spelt, the closest first,
// with the letters that matched marked. The arrow keys move through them and Enter picks one.

export function NewMessage({
  workspaces,
  pick,
  close,
}: {
  workspaces: readonly CommunicationsWorkspace[];
  /** Opens the conversation with whoever was picked. */
  pick: (contact: Contact) => Promise<void>;
  close: () => void;
}) {
  const [typed, setTyped] = useState('');
  const [found, setFound] = useState<Contact[]>();
  const [failed, setFailed] = useState('');
  const [active, setActive] = useState(0);
  const list = useId();
  useEffect(() => {
    let live = true;
    const timer = setTimeout(
      () => {
        getContacts(typed.trim()).then(
          (read) => {
            if (!live) return;
            setFound(read.contacts);
            setFailed('');
            setActive(0);
          },
          (cause) => live && setFailed(messageOf(cause)),
        );
      },
      typed ? 150 : 0,
    );
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [typed]);
  // The one the arrow keys reach stays in view.
  useEffect(() => {
    document.getElementById(`${list}-${active}`)?.scrollIntoView({ block: 'nearest' });
  }, [list, active]);
  const chosen = useAction(pick);
  // With several workspaces, each says which it's in.
  const named = new Map(workspaces.map((workspace) => [workspace.teamId, workspace.name]));
  const several = named.size > 1;
  const option = (at: number) => `${list}-${at}`;
  const choose = (contact: Contact | undefined) => {
    if (contact && !chosen.busy) void chosen.run(contact);
  };
  return (
    <Modal title="New message" onClose={close} initialFocus="input">
      <div className="new-message">
        <label className="search new-search">
          <Search size={16} aria-hidden="true" />
          <input
            role="combobox"
            aria-label="Name"
            aria-expanded={Boolean(found?.length)}
            aria-controls={list}
            aria-activedescendant={found?.length ? option(active) : undefined}
            aria-autocomplete="list"
            placeholder="Name"
            value={typed}
            onChange={(event) => setTyped(event.target.value)}
            onKeyDown={(event) => {
              const count = found?.length ?? 0;
              if (event.key === 'ArrowDown' && count) setActive((active + 1) % count);
              else if (event.key === 'ArrowUp' && count) setActive((active - 1 + count) % count);
              else if (event.key === 'Enter') choose(found?.[active]);
              else return;
              event.preventDefault();
            }}
          />
        </label>
        {failed && <p className="list-empty">{failed}</p>}
        {found && !found.length && typed.trim() && <p className="list-empty">No matches</p>}
        <ul id={list} role="listbox" aria-label="People" className="results">
          {found?.map((contact, at) => (
            <li
              key={`${contact.teamId}:${contact.person.userId}`}
              id={option(at)}
              role="option"
              aria-selected={at === active}
              className={at === active ? 'result active' : 'result'}
              onMouseEnter={() => setActive(at)}
              onClick={() => choose(contact)}
            >
              <Face {...contact.person} size="list" />
              <span className="result-body">
                <strong>
                  <Name name={contact.person.name} matched={contact.matched} />
                </strong>
                <small>
                  {[
                    contact.person.role && roles[contact.person.role],
                    several && named.get(contact.teamId),
                  ]
                    .filter(Boolean)
                    .join(' · ')}
                </small>
              </span>
              <ChevronRight size={16} aria-hidden="true" />
            </li>
          ))}
        </ul>
      </div>
    </Modal>
  );
}
