import { Fragment, useContext } from 'react';
import { Reply } from 'lucide-react';
import type { GroupEntry, GroupProgress } from '../../../api/index.js';
import { slackAttachments } from '../../../api/client.js';
import { clock, dayLabel, dayOf } from '../../shared/format.js';
import { Shown } from '../../display/display.js';
import { Face } from '../../shared/Face.js';
import { FaceButton, MessageMeta, MessageRun, SenderLine } from '../../shared/MessageRun.js';
import { DayLabel, ThreadFrame } from '../../shared/ThreadFrame.js';
import { MessageBody } from '../../shared/MessageBody.js';
import './group-thread.css';

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
  const { style, times } = useContext(Shown);
  const plain = style === 'plain';
  return (
    <ThreadFrame items={entries} more={more} readOlder={readOlder}>
      {runs(entries, timeZone).map(({ day, at, runs }) => (
        <Fragment key={day}>
          <DayLabel>{dayLabel(at, timeZone, now)}</DayLabel>
          {runs.map(({ key, entries: written }) => {
            const first = written[0]!;
            const last = written.at(-1)!;
            if (first.sender) {
              const gone = first.progress && progressText(first.progress);
              return (
                <MessageRun
                  key={key}
                  direction="out"
                  face={
                    <Face
                      name={first.sender.name}
                      code={first.sender.code}
                      photo={first.sender.photo}
                      size="message"
                    />
                  }
                  sender={
                    <SenderLine name={first.sender.name} at={plain && clock(first.at, timeZone)} />
                  }
                >
                  <MessageBody message={first} address={slackAttachments.address} />
                  <MessageMeta time={!plain && clock(first.at, timeZone)}>
                    {gone && (
                      <span className={gone.sending ? 'sending' : undefined}>{gone.text}</span>
                    )}
                  </MessageMeta>
                </MessageRun>
              );
            }
            const from = first.from;
            const name = from?.name ?? '';
            return (
              <MessageRun
                key={key}
                direction="in"
                className="answer"
                // Their name, above the answer, opens it from the keyboard.
                face={
                  <FaceButton label={name} onClick={() => reply(first)}>
                    {from && <Face {...from} size="message" />}
                  </FaceButton>
                }
                sender={
                  <SenderLine
                    name={name}
                    at={plain && clock(first.at, timeZone)}
                    onClick={() => reply(first)}
                  />
                }
                after={
                  <button
                    className="reply-to"
                    aria-label={`Reply to ${name}`}
                    onClick={() => reply(first)}
                  >
                    <Reply size={13} aria-hidden="true" />
                    Reply
                  </button>
                }
              >
                {written.map((entry) => (
                  <Fragment key={entry.id}>
                    <MessageBody message={entry} address={slackAttachments.address} />
                    <MessageMeta
                      time={!plain && times === 'each_message' && clock(entry.at, timeZone)}
                    />
                  </Fragment>
                ))}
                <MessageMeta
                  time={!plain && times !== 'each_message' && clock(last.at, timeZone)}
                />
              </MessageRun>
            );
          })}
        </Fragment>
      ))}
    </ThreadFrame>
  );
}
