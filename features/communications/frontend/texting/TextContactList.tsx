import { useCallback, useRef, useState } from 'react';
import type { Conversation, TextContact } from '../../api/index.js';
import { blockNumber, getTextContacts } from '../../api/client.js';
import { Loading } from '../../../../core/shell/frontend/ui/index.js';
import { useFeedback } from '../../../../core/shell/frontend/runtime/feedback.js';
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
// with Unblock, which takes it off the list at once, putting it back if that fails.

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
  // Numbers unblocked here, gone from Blocked until the contacts are read again once the last
  // unblocking is done.
  const { perform } = useFeedback();
  const [unblocked, setUnblocked] = useState<{ numbers: string[]; read?: unknown }>({
    numbers: [],
  });
  const latest = useRef<unknown>(found);
  latest.current = found;
  const unblocking = useRef(0);
  const gone = (number: string) =>
    unblocked.numbers.includes(number) && (!unblocked.read || unblocked.read === found);
  const unblock = (contact: TextContact) => {
    unblocking.current += 1;
    setUnblocked((before) => ({
      numbers: [
        ...(before.read && before.read !== latest.current ? [] : before.numbers),
        contact.number,
      ],
    }));
    void perform(async () => show(await blockNumber(contact.conversationId!, false))).then(
      (done) => {
        unblocking.current -= 1;
        const last = !unblocking.current;
        setUnblocked((before) => ({
          numbers: done ? before.numbers : before.numbers.filter((n) => n !== contact.number),
          read: last ? latest.current : undefined,
        }));
      },
    );
  };
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
  const blockedNow = found.blocked.filter((contact) => !gone(contact.number));
  const blocked = !typed && blockedNow.length > 0 && (
    <>
      <ListHead>Blocked</ListHead>
      <Rows>
        {blockedNow.map((contact) => {
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
                  disabled={!contact.conversationId}
                  onClick={() => unblock(contact)}
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
