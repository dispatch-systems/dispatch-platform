import { Fragment, useCallback } from 'react';
import type { CommunicationsWorkspace, Contact, Conversation } from '../../api/index.js';
import { getContacts } from '../../api/client.js';
import { Loading } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { letter, listTime } from '../shared/format.js';
import { Face } from '../shared/Face.js';
import { roleText } from '../shared/format.js';
import { Name } from '../shared/Name.js';
import { Wait } from '../shared/Wait.js';
import { useFound } from '../shared/useFound.js';

// Contacts: everyone the DSP can message, of every workspace or the one chosen, A to Z under
// each name's first letter; or those found by a name typed, misspelt or not, the closest first.
// Each shows how long they've waited, or when they last wrote, and opens their conversation, or
// starts one.

export function ContactList({
  team,
  typed,
  revision,
  workspaces,
  current,
  timeZone,
  now,
  pick,
}: {
  /** The workspace chosen, if one is. */
  team?: string;
  /** What's typed in Search, trimmed. */
  typed: string;
  /** The conversations' latest change, which reads them again. */
  revision: number;
  /** The workspaces Communications works in, which each says it's in while there are several. */
  workspaces: readonly CommunicationsWorkspace[];
  /** The conversation open. */
  current?: Conversation;
  timeZone: string;
  now: number;
  /** Opens the conversation with whoever was picked. */
  pick: (contact: Contact) => Promise<void>;
}) {
  const read = useCallback(
    () => getContacts(typed, team).then((found) => found.contacts),
    [typed, team],
  );
  // Those found by a name are another list from all of them, as is each workspace's.
  const list = `${team ?? ''} ${typed ? 'found' : 'all'}`;
  const { found, failed } = useFound(read, list, revision, typed ? 200 : 0);
  const chosen = useAction(pick);
  if (failed || (typed && found?.length === 0))
    return <p className="list-empty">{failed || 'No matches'}</p>;
  if (!found) return <Loading />;
  // With every workspace shown, each says which it's in.
  const named = new Map(workspaces.map((workspace) => [workspace.teamId, workspace.name]));
  const where = !team && named.size > 1 ? named : undefined;
  const row = (contact: Contact) => {
    const { person } = contact;
    const selected = current?.teamId === contact.teamId && current.with.userId === person.userId;
    return (
      <li
        key={`${contact.teamId}:${person.userId}`}
        className={selected ? 'convo contact selected' : 'convo contact'}
      >
        <button
          className="convo-open"
          aria-current={selected || undefined}
          onClick={() => {
            if (!chosen.busy) void chosen.run(contact);
          }}
        >
          <Face {...person} size="contact" />
          <span className="convo-body">
            <strong>
              <Name name={person.name} matched={contact.matched} />
            </strong>
            <small>
              {[roleText(person), where?.get(contact.teamId)].filter(Boolean).join(' · ')}
            </small>
          </span>
          {contact.waitingSince !== null ? (
            <Wait since={contact.waitingSince} now={now} />
          ) : (
            contact.lastAt !== null && <time>{listTime(contact.lastAt, timeZone, now)}</time>
          )}
        </button>
      </li>
    );
  };
  if (typed) return <ul className="convos">{found.map(row)}</ul>;
  // A to Z, under each first letter.
  const letters: { letter: string; contacts: Contact[] }[] = [];
  for (const contact of found) {
    const first = letter(contact.person.name);
    if (letters.at(-1)?.letter !== first) letters.push({ letter: first, contacts: [] });
    letters.at(-1)!.contacts.push(contact);
  }
  return (
    <>
      {letters.map(({ letter: first, contacts }, at) => (
        <Fragment key={`${at}:${first}`}>
          <h2 className="list-head">{first}</h2>
          <ul className="convos">{contacts.map(row)}</ul>
        </Fragment>
      ))}
    </>
  );
}
