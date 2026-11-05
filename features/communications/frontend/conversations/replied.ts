import { useEffect, useMemo, useRef } from 'react';
import type { Message } from '../../api/index.js';

// Before what's written goes, whether a teammate answered meanwhile: someone else at the DSP, or
// the phone itself, wrote in the conversation a message the page didn't hold as the member
// started writing, as it opened with what they'd written, or as they last answered that it had.
// Read from what the page holds: the conversation's messages as read, and its last message as
// each update brings it. Told by which messages were held, not by when each was written, as a
// phone's clock may run minutes ahead of the server's; and one written before the earliest held
// is one read from further back, not news.

/** The messages the page held as the member last saw it, by ID, and when the earliest of them
 * was written. */
export type Seen = { ids: ReadonlySet<string>; from: number };

/** `messages` as seen now. */
export const seenOf = (messages: readonly Message[]): Seen => ({
  ids: new Set(messages.map((message) => message.id)),
  from: messages.length
    ? messages.reduce((earliest, message) => Math.min(earliest, message.at), Infinity)
    : -Infinity,
});

/** The latest message the DSP sent that wasn't `seen`, by anyone but the member `me`. */
export function repliedSince(messages: readonly Message[], seen: Seen, me: string) {
  let latest: Message | undefined;
  for (const message of messages)
    if (
      message.direction === 'out' &&
      !seen.ids.has(message.id) &&
      message.at > seen.from &&
      message.sender &&
      message.sender.userId !== me &&
      (!latest || message.at > latest.at)
    )
      latest = message;
  return latest;
}

/** "Keisha just replied": a teammate by their first name, the phone by its own; "Someone" for
 * one without a name. */
export function repliedText(message: Message) {
  const sender = message.sender!;
  const name = sender.userId ? sender.name.split(' ')[0] : sender.name;
  return `${name || 'Someone'} just replied`;
}

/**
 * What the composer asks before sending in a conversation whose `messages` the page holds, none
 * until it's read: `check`, what a teammate's reply since the member started writing says, if
 * there's one; and `seen`, that the member saw what's there now, as they start writing or answer
 * that.
 */
export function useReplied(messages: readonly Message[] | undefined, me: string) {
  const since = useRef<Seen | undefined>(undefined);
  const held = useRef(messages);
  useEffect(() => {
    held.current = messages;
    if (messages && since.current === undefined) since.current = seenOf(messages);
  }, [messages]);
  return useMemo(
    () => ({
      check: () => {
        const replied =
          held.current && since.current && repliedSince(held.current, since.current, me);
        return replied ? repliedText(replied) : undefined;
      },
      seen: () => {
        if (held.current) since.current = seenOf(held.current);
      },
    }),
    [me],
  );
}
