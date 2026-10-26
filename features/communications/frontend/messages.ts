import type { Message } from '../api/index.js';
import { dayOf } from './format.js';

// A conversation's messages as the page shows them: under the day each was written, in runs by
// whoever wrote them.

/** How soon after one message the same writer's next is shown with it, in a run. */
const TOGETHER_MS = 10 * 60_000;

/** Messages one after another from the same writer, within a few minutes, shown together. */
type Run = { key: string; writer: string; messages: Message[] };

/** Who wrote a message, to tell runs apart: whom it's with; or someone at the DSP, by their
 * account, or the phone a text was sent from on its own messaging app, and the group it went to. */
const writer = (message: Message) =>
  message.direction === 'in'
    ? 'in'
    : `out:${message.sender?.userId ?? ''}:${message.sender?.phoneId ?? ''}:${message.group?.id ?? ''}`;

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
