import { Fragment, useCallback } from 'react';
import type { Conversation, TextContact } from '../../api/index.js';
import { blockNumber, getTextContacts } from '../../api/client.js';
import { Loading } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { phoneText } from '../../../../core/shell/frontend/lib/format.js';
import { letter, listTime, numberBeside } from '../format.js';
import { Face, PhoneFace } from './Face.js';
import { Name, Wait } from './parts.js';
import { useFound } from './useFound.js';

// Messaging's Contacts: whom the DSP can text, everyone Driver Match has a number for and every
// number it saved a name for, A to Z under each name's first letter, or those found by a name or
// number typed, the closest first. Each shows their number and how long they've waited, or when
// they last wrote, and opens their conversation, or starts one. Then the numbers blocked, each
// with Unblock.

export function TextContactList({
  typed,
  revision,
  current,
  timeZone,
  now,
  pick,
  show,
}: {
  /** What's typed in Search, trimmed. */
  typed: string;
  /** The conversations' latest change, which reads them again. */
  revision: number;
  /** The conversation open. */
  current?: Conversation;
  timeZone: string;
  now: number;
  /** Opens the conversation with whoever was picked. */
  pick: (contact: TextContact) => Promise<void>;
  /** Shows a conversation as the page's own writes left it. */
  show: (conversation: Conversation) => void;
}) {
  const read = useCallback(() => getTextContacts(typed), [typed]);
  const { found, failed } = useFound(read, typed ? 'found' : 'all', revision, typed ? 200 : 0);
  const chosen = useAction(pick);
  const unblock = useAction(async (contact: TextContact) =>
    show(await blockNumber(contact.conversationId!, false)),
  );
  if (failed || (typed && found?.contacts.length === 0))
    return <p className="list-empty">{failed || 'No matches'}</p>;
  if (!found) return <Loading />;
  const row = (contact: TextContact) => {
    const { person } = contact;
    const selected = Boolean(current && current.id === contact.conversationId);
    return (
      <li key={contact.number} className={selected ? 'convo contact selected' : 'convo contact'}>
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
            <small>{phoneText(contact.number)}</small>
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
  const blocked = !typed && found.blocked.length > 0 && (
    <>
      <h2 className="list-head">Blocked</h2>
      <ul className="convos">
        {found.blocked.map((contact) => {
          const number = numberBeside(contact.person.name, contact.number);
          return (
            <li key={contact.number} className="convo contact blocked">
              <span className="convo-open">
                <PhoneFace size="contact" mark="blocked" />
                <span className="convo-body">
                  <strong>{contact.person.name}</strong>
                  {number && <small>{number}</small>}
                </span>
                <button
                  className="unblock"
                  disabled={unblock.busy || !contact.conversationId}
                  onClick={() => void unblock.run(contact)}
                >
                  Unblock
                </button>
              </span>
            </li>
          );
        })}
      </ul>
    </>
  );
  if (typed) return <ul className="convos">{found.contacts.map(row)}</ul>;
  // A to Z, under each first letter.
  const letters: { letter: string; contacts: TextContact[] }[] = [];
  for (const contact of found.contacts) {
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
      {blocked}
    </>
  );
}
