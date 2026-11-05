import type { WaitingMessage } from '../../api/index.js';
import type { Down } from '../texting/phones-down.js';

// Which messages alert the member: a message from an employee that wasn't waiting when the tab
// alerting them last looked; and which of the DSP's phones, each once as it goes down. What it saw is kept in the browser, shared by the member's tabs at
// the DSP, so a message alerts once however many are open, whichever alerts. What was waiting
// before Dispatch was open alerts no one.

/** What the member's tabs at the DSP saw waiting last, each message by its ID, and when. */
export type Seen = { at: number; ids: readonly string[] };
/** What's in front of the member: the conversation or group open on the page, in view. */
export type InFront = { conversation?: string; group?: string };

/**
 * How long what was seen holds: longer than the alerting tab takes to look again, even in a
 * browser slowing a tab out of view, so it's older only while no tab has looked, as with
 * Dispatch closed.
 */
export const SEEN_MS = 3 * 60_000;
/** How many messages no longer waiting are remembered, the latest; those waiting all are. */
const MOST = 500;

/**
 * What the alerting tab makes of `latest`, the message each conversation waits on, at `now`:
 * the new ones to alert of, but one in front of the member; and what's seen after it. Nothing is
 * new to a tab that's the first to look in a while.
 */
export function arrived(
  latest: readonly WaitingMessage[],
  seen: Seen | undefined,
  now: number,
  inFront: InFront = {},
) {
  const ids = latest.map((each) => each.message.id);
  if (!seen || now - seen.at > SEEN_MS) return { fresh: [], seen: { at: now, ids } };
  const known = new Set(seen.ids);
  const waiting = new Set(ids);
  const fresh = latest.filter(
    (each) =>
      !known.has(each.message.id) &&
      each.conversationId !== inFront.conversation &&
      !(each.groupId && each.groupId === inFront.group),
  );
  // Those no longer waiting are remembered too, the latest of them, so a tab reading an older
  // copy than the last to look forgets none that alerted already.
  const gone = seen.ids.filter((id) => !waiting.has(id)).slice(-MOST);
  return { fresh, seen: { at: now, ids: [...gone, ...ids] } };
}

/**
 * What the alerting tab makes of the phones `downs` at `now`: those that went down since it last
 * looked, to alert of, and what's seen after it, each by its key. None to a tab that's the first to
 * look in a while, as with messages.
 */
export function wentDown(downs: readonly Down[], seen: Seen | undefined, now: number) {
  const ids = downs.map((each) => each.key);
  if (!seen || now - seen.at > SEEN_MS) return { fresh: [], seen: { at: now, ids } };
  const known = new Set(seen.ids);
  return { fresh: downs.filter((each) => !known.has(each.key)), seen: { at: now, ids } };
}

/** What the tabs keep what they saw of: the messages waiting, or the phones down. */
type Seeing = 'messages' | 'phones';
/** Where the member's tabs at the DSP keep what they saw. */
const key = (dsp: string, user: string, of: Seeing) =>
  `dispatch-communications-${of === 'messages' ? 'seen' : 'phones-down'}:${dsp}:${user}`;
/** What the member's tabs at the DSP saw last, if any. */
export function readSeen(dsp: string, user: string, of: Seeing = 'messages'): Seen | undefined {
  try {
    return (
      (JSON.parse(localStorage.getItem(key(dsp, user, of)) ?? 'null') as Seen | null) ?? undefined
    );
  } catch {
    return undefined;
  }
}
export function keepSeen(dsp: string, user: string, seen: Seen, of: Seeing = 'messages') {
  try {
    localStorage.setItem(key(dsp, user, of), JSON.stringify(seen));
  } catch {
    // Not kept, what waits next is seen afresh, alerting of nothing.
  }
}
