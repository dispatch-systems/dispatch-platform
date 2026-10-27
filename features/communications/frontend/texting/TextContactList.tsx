import { useCallback } from 'react';
import type { Conversation, TextContact } from '../../api/index.js';
import { blockNumber, getTextContacts } from '../../api/client.js';
import { Loading } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { phoneText } from '../../../../core/shell/frontend/lib/format.js';
import { existingOr, type Target } from '../conversations/target.js';
import { ByInitial, ContactRow } from '../shared/ContactRow.js';
import { Face, PhoneFace } from '../shared/Face.js';
import { ListEmpty, ListHead, Rows } from '../shared/List.js';
import { numberBeside } from '../shared/format.js';
import { useFound } from '../shared/useFound.js';
import './text-contacts.css';

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
  reach,
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
  /** Opens the conversation with whoever was picked, or starts it with their number. */
  reach: (target: Target) => Promise<void>;
  /** Shows a conversation as the page's own writes left it. */
  show: (conversation: Conversation) => void;
}) {
  const read = useCallback(() => getTextContacts(typed), [typed]);
  const { found, failed } = useFound(read, typed ? 'found' : 'all', revision, typed ? 200 : 0);
  const chosen = useAction(reach);
  const unblock = useAction(async (contact: TextContact) =>
    show(await blockNumber(contact.conversationId!, false)),
  );
  if (failed || (typed && found?.contacts.length === 0))
    return <ListEmpty>{failed || 'No matches'}</ListEmpty>;
  if (!found) return <Loading />;
  const row = (contact: TextContact) => (
    <ContactRow
      key={contact.number}
      face={<Face {...contact.person} size="contact" />}
      name={contact.person.name}
      matched={contact.matched}
      detail={phoneText(contact.number)}
      waitingSince={contact.waitingSince}
      lastAt={contact.lastAt}
      selected={Boolean(current && current.id === contact.conversationId)}
      timeZone={timeZone}
      now={now}
      open={() => {
        if (!chosen.busy)
          void chosen.run(
            existingOr(contact.conversationId, { kind: 'number', number: contact.number }),
          );
      }}
    />
  );
  const blocked = !typed && found.blocked.length > 0 && (
    <>
      <ListHead>Blocked</ListHead>
      <Rows>
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
      </Rows>
    </>
  );
  if (typed) return <Rows>{found.contacts.map(row)}</Rows>;
  return (
    <>
      <ByInitial contacts={found.contacts} nameOf={(contact) => contact.person.name} row={row} />
      {blocked}
    </>
  );
}
