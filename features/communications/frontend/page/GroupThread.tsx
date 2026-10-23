import { Fragment, useContext, useLayoutEffect, useRef } from 'react';
import { Reply } from 'lucide-react';
import type { GroupEntry, GroupProgress } from '../../api/index.js';
import { clock, dayLabel, dayOf } from '../format.js';
import { Shown } from '../display/display.js';
import { Face } from './Face.js';
import { More } from './More.js';
import { Written } from './Thread.js';

// A group's messages, in the order written, under the day each was written: the DSP's on the
// right under whoever sent each, with how far it has gone; each answer on the left under the name
// of whoever wrote it, which, with their photo, opens their own conversation, as Reply does,
// shown as an answer is pointed at or reached by the keyboard (Tab reaches the name, then Reply).

/** Answers one after another from the same person, within a few minutes, shown together. */
type Run = { key: string; entries: GroupEntry[] };
const TOGETHER_MS = 10 * 60_000;

/** The entries under each day they were written, each of the DSP's on its own and the answers in
 * runs by who wrote them. */
function runs(entries: readonly GroupEntry[], timeZone: string) {
  const days: { day: string; at: number; runs: Run[] }[] = [];
  for (const entry of entries) {
    const day = dayOf(entry.at, timeZone);
    if (days.at(-1)?.day !== day) days.push({ day, at: entry.at, runs: [] });
    const current = days.at(-1)!;
    const run = current.runs.at(-1);
    const last = run?.entries.at(-1);
    const together =
      entry.conversationId !== null &&
      last?.conversationId === entry.conversationId &&
      entry.at - last.at < TOGETHER_MS;
    if (together) run!.entries.push(entry);
    else current.runs.push({ key: entry.id, entries: [entry] });
  }
  return days;
}

/** How far a message to the group has gone: going, or gone to all, or to some of them. */
export function progressText({ people, sent, failed }: GroupProgress) {
  if (sent + failed < people) return { text: `Sending ${sent} of ${people}`, sending: true };
  return { text: failed ? `Sent to ${sent} of ${people}` : `Sent to ${people}`, sending: false };
}

export function GroupThread({
  entries,
  more,
  readOlder,
  timeZone,
  now,
  reply,
}: {
  entries: readonly GroupEntry[];
  more: boolean;
  readOlder: () => void;
  timeZone: string;
  now: number;
  /** Opens the conversation of whoever wrote an answer. */
  reply: (entry: GroupEntry) => void;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  // Where it was before the latest change, as a conversation keeps it.
  const before = useRef({ first: '', height: 0, top: 0, latest: true });
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const first = entries[0]?.id ?? '';
    const was = before.current;
    if (was.first && first !== was.first && !was.latest)
      element.scrollTop = was.top + element.scrollHeight - was.height;
    else if (was.latest) element.scrollTop = element.scrollHeight;
    before.current = { ...was, first, height: element.scrollHeight, top: element.scrollTop };
  }, [entries]);
  const scrolled = () => {
    const element = scroller.current;
    if (!element) return;
    before.current = {
      ...before.current,
      height: element.scrollHeight,
      top: element.scrollTop,
      latest: element.scrollHeight - element.scrollTop - element.clientHeight < 80,
    };
  };
  const { style, times } = useContext(Shown);
  const plain = style === 'plain';
  return (
    <div className="thread" ref={scroller} onScroll={scrolled}>
      {more && <More read={readOlder} />}
      <div className="thread-messages">
        {runs(entries, timeZone).map(({ day, at, runs }) => (
          <Fragment key={day}>
            <div className="day">{dayLabel(at, timeZone, now)}</div>
            {runs.map(({ key, entries: written }) => {
              const first = written[0]!;
              const last = written.at(-1)!;
              if (first.sender) {
                const gone = first.progress && progressText(first.progress);
                return (
                  <div key={key} className="msg-group out">
                    <Face
                      name={first.sender.name}
                      code={first.sender.code}
                      photo={first.sender.photo}
                      size="message"
                    />
                    <div className="stack">
                      <div className="sender">
                        <span className="sender-name">{first.sender.name}</span>
                        {plain && <span className="sent-at">{clock(first.at, timeZone)}</span>}
                      </div>
                      <Written message={first} />
                      {(!plain || gone) && (
                        <div className="meta">
                          {!plain && clock(first.at, timeZone)}
                          {!plain && gone && ' · '}
                          {gone && (
                            <span className={gone.sending ? 'sending' : undefined}>
                              {gone.text}
                            </span>
                          )}
                        </div>
                      )}
                    </div>
                  </div>
                );
              }
              const from = first.from;
              const name = from?.name ?? '';
              return (
                <div key={key} className="msg-group in answer">
                  {/* Left out of Tab's order: their name, above the answer, opens it from the keyboard. */}
                  <button
                    className="face-button"
                    tabIndex={-1}
                    aria-label={name}
                    onClick={() => reply(first)}
                  >
                    {from && <Face {...from} size="message" />}
                  </button>
                  <div className="stack">
                    <button className="sender" onClick={() => reply(first)}>
                      {name}
                      {plain && <span className="sent-at">{clock(first.at, timeZone)}</span>}
                    </button>
                    {written.map((entry) => (
                      <Fragment key={entry.id}>
                        <Written message={entry} />
                        {!plain && times === 'each_message' && (
                          <div className="meta">{clock(entry.at, timeZone)}</div>
                        )}
                      </Fragment>
                    ))}
                    {!plain && times !== 'each_message' && (
                      <div className="meta">{clock(last.at, timeZone)}</div>
                    )}
                  </div>
                  <button
                    className="reply-to"
                    aria-label={`Reply to ${name}`}
                    onClick={() => reply(first)}
                  >
                    <Reply size={13} aria-hidden="true" />
                    Reply
                  </button>
                </div>
              );
            })}
          </Fragment>
        ))}
      </div>
    </div>
  );
}
