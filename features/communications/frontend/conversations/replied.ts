import { useEffect, useMemo, useRef } from 'react';
import type { Message } from '../../api/index.js';

// Before what's written goes, whether a teammate answered meanwhile: someone else at the DSP, or
// the phone itself, wrote in the conversation after the latest message the page showed as the
// member started writing, as it opened with what they'd written, or as they last answered that
// it had. Read from what the page holds: the conversation's messages as read, and its last
// message as each update brings it.

/** When the latest of `messages` was written; 0 for none. */
export const latestAt = (messages: readonly Message[]) =>
  messages.reduce((latest, message) => Math.max(latest, message.at), 0);

/** The latest message the DSP sent after `since` by anyone but the member `me`. */
export function repliedSince(messages: readonly Message[], since: number, me: string) {
  let latest: Message | undefined;
  for (const message of messages)
    if (
      message.direction === 'out' &&
      message.at > since &&
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
 * there's one; and `seen`, that the member saw what's there now, as they start writing, answer
 * that, or sent.
 */
export function useReplied(messages: readonly Message[] | undefined, me: string) {
  const since = useRef<number | undefined>(undefined);
  const held = useRef(messages);
  useEffect(() => {
    held.current = messages;
    if (messages && since.current === undefined) since.current = latestAt(messages);
  }, [messages]);
  return useMemo(
    () => ({
      check: () => {
        const replied =
          held.current &&
          since.current !== undefined &&
          repliedSince(held.current, since.current, me);
        return replied ? repliedText(replied) : undefined;
      },
      seen: () => {
        if (held.current) since.current = latestAt(held.current);
      },
    }),
    [me],
  );
}
