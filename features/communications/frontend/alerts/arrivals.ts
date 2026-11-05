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
 * The phones down the member's tabs at the DSP saw, each by its key with when a tab last saw it
 * down, and when they last looked.
 */
export type Downs = { at: number; down: Readonly<Record<string, number>> };

/**
 * What the alerting tab makes of the phones `downs` at `now`: those that went down since the tabs
 * last saw them, to alert of, and what's seen after it. None to a tab that's the first to look in
 * a while, as with messages. One no longer down is remembered for as long as what was seen holds,
 * as a tab taking its turn may read the phones as they were before another tab saw it go down: it
 * alerts again only once it went down again after that.
 */
export function wentDown(downs: readonly Down[], seen: Downs | undefined, now: number) {
  const holds = seen !== undefined && now - seen.at <= SEEN_MS;
  const down: Record<string, number> = {};
  if (holds)
    for (const [key, at] of Object.entries(seen.down)) if (now - at <= SEEN_MS) down[key] = at;
  const fresh = holds ? downs.filter((each) => !(each.key in down)) : [];
  for (const each of downs) down[each.key] = now;
  return { fresh, seen: { at: now, down } };
}

/** Where the member's tabs at the DSP keep what they saw, of messages or of phones. */
const key = (dsp: string, user: string, of: 'seen' | 'phones-down') =>
  `dispatch-communications-${of}:${dsp}:${user}`;
function read<T>(at: string): T | undefined {
  try {
    return (JSON.parse(localStorage.getItem(at) ?? 'null') as T | null) ?? undefined;
  } catch {
    return undefined;
  }
}
function keep(at: string, value: unknown) {
  try {
    localStorage.setItem(at, JSON.stringify(value));
  } catch {
    // Not kept, what's seen next is seen afresh, alerting of nothing.
  }
}
/** What the member's tabs at the DSP saw waiting last, if any. */
export const readSeen = (dsp: string, user: string) => read<Seen>(key(dsp, user, 'seen'));
export const keepSeen = (dsp: string, user: string, seen: Seen) =>
  keep(key(dsp, user, 'seen'), seen);
/** The phones down the member's tabs at the DSP saw last, if any. */
export const readDowns = (dsp: string, user: string) => read<Downs>(key(dsp, user, 'phones-down'));
export const keepDowns = (dsp: string, user: string, downs: Downs) =>
  keep(key(dsp, user, 'phones-down'), downs);
