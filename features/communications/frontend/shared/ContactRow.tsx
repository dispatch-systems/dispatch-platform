import { Fragment, type ReactNode } from 'react';
import { letter, listTime } from './format.js';
import { Name } from './Name.js';
import { Wait } from './Wait.js';

// Someone in a list of contacts, Slack's or Messaging's: their face, their name with the letters
// that matched what was searched marked, a line about them, and how long they've waited or when
// they last wrote; it opens their conversation. And the contacts under each first letter of their
// names, A to Z, in the order the server lists them.

export function ContactRow({
  face,
  name,
  matched,
  detail,
  waitingSince,
  lastAt,
  selected,
  timeZone,
  now,
  open,
}: {
  face: ReactNode;
  name: string;
  matched: readonly (readonly [number, number])[];
  /** What the line under their name says of them. */
  detail: ReactNode;
  waitingSince: number | null;
  lastAt: number | null;
  /** Whether their conversation is the one open. */
  selected: boolean;
  timeZone: string;
  now: number;
  open: () => void;
}) {
  return (
    <li className={selected ? 'convo contact selected' : 'convo contact'}>
      <button className="convo-open" aria-current={selected || undefined} onClick={open}>
        {face}
        <span className="convo-body">
          <strong>
            <Name name={name} matched={matched} />
          </strong>
          <small>{detail}</small>
        </span>
        {waitingSince !== null ? (
          <Wait since={waitingSince} now={now} />
        ) : (
          lastAt !== null && <time>{listTime(lastAt, timeZone, now)}</time>
        )}
      </button>
    </li>
  );
}

/** `contacts` under each first letter of their names (`nameOf`), each drawn by `row`. */
export function ByInitial<T>({
  contacts,
  nameOf,
  row,
}: {
  contacts: readonly T[];
  nameOf: (contact: T) => string;
  row: (contact: T) => ReactNode;
}) {
  const letters: { letter: string; contacts: T[] }[] = [];
  for (const contact of contacts) {
    const first = letter(nameOf(contact));
    if (letters.at(-1)?.letter !== first) letters.push({ letter: first, contacts: [] });
    letters.at(-1)!.contacts.push(contact);
  }
  return (
    <>
      {letters.map(({ letter: first, contacts: under }, at) => (
        <Fragment key={`${at}:${first}`}>
          <h2 className="list-head">{first}</h2>
          <ul className="convos">{under.map(row)}</ul>
        </Fragment>
      ))}
    </>
  );
}
