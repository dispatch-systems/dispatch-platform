import type { Message } from '../../api/index.js';
import { dayOf } from '../shared/format.js';

// A conversation's messages as the page holds and shows them: kept in the order written as they're
// read, each read of the latest as it is now; and under the day each was written, in runs by
// whoever wrote them.

/** How soon after one message the same writer's next is shown with it, in a run. */
const TOGETHER_MS = 10 * 60_000;

/** Messages one after another from the same writer, within a few minutes, shown together. */
type Run = { key: string; writer: string; messages: Message[] };

/** Messages by their place, `ts`, in the order written: Slack's IDs, `<seconds>.<microseconds>`,
 * or a text's time, thirteen digits, and ID. */
const byTs = (a: Message, b: Message) =>
  a.ts.padStart(24, '0') < b.ts.padStart(24, '0') ? -1 : a.ts === b.ts ? 0 : 1;
/** `kept` with `read`, each message once, as it is now, in the order written. */
export function joined(kept: readonly Message[], read: readonly Message[]) {
  const byId = new Map(kept.map((message) => [message.id, message]));
  for (const message of read) byId.set(message.id, message);
  return [...byId.values()].sort(byTs);
}

/** Messages held, and whether older ones are left to read. */
type Held = { messages: readonly Message[]; more: boolean };
/**
 * `kept` once the latest messages are read again (`latest`). What that read covers, from its first
 * message on, or every message once none is left before it, is as it read: a message no longer
 * there is gone, as a text filed in another conversation is. Those before it stay, as read
 * before; and so does a message the page sent that the read may not hold yet (`sending`).
 */
export function fresh(
  kept: Held | undefined,
  latest: Held,
  sending: (message: Message) => boolean,
) {
  const first = latest.more ? latest.messages[0] : undefined;
  const before = first ? (kept?.messages ?? []).filter((message) => byTs(message, first) < 0) : [];
  const unread = (kept?.messages ?? []).filter(sending);
  return {
    messages: joined([...before, ...unread], latest.messages),
    more: kept && before.length > 0 ? kept.more : latest.more,
  };
}

/** Who wrote a message, to tell runs apart: whom it's with; or someone at the DSP, by their
 * account, or the phone a text was sent from on its own messaging app, and the group it went to;
 * and for a text, the other party's number, as a driver's work phone changes. */
const writer = (message: Message) =>
  (message.direction === 'in'
    ? 'in'
    : `out:${message.sender?.userId ?? ''}:${message.sender?.phoneId ?? ''}:${message.group?.id ?? ''}`) +
  `:${message.number ?? ''}`;

/** The messages under each day they were written, in runs by writer. */
export function runs(messages: readonly Message[], timeZone: string) {
  const days: { day: string; at: number; runs: Run[] }[] = [];
  for (const message of messages) {
    const day = dayOf(message.at, timeZone);
    if (days.at(-1)?.day !== day) days.push({ day, at: message.at, runs: [] });
    const current = days.at(-1)!;
    const by = writer(message);
    const run = current.runs.at(-1);
    if (run?.writer === by && message.at - run.messages.at(-1)!.at < TOGETHER_MS)
      run.messages.push(message);
    else current.runs.push({ key: message.id, writer: by, messages: [message] });
  }
  return days;
}
