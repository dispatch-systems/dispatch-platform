import { useEffect, useMemo } from 'react';
import type { Message } from '../../api/index.js';

// Before what's written goes, whether a teammate answered meanwhile: someone else at the DSP, or
// the phone itself, wrote in the conversation a message the page didn't hold as the member
// started writing, as it opened with what they'd written, or as they last answered that it had.
// Each reply is kept once it's been held, whatever the page holds after, until the member saw it.
// Told by which messages were held, never by when each was written, as a phone's clock may run
// minutes ahead of the server's: what the page holds is its latest read and the last message
// each update brings, never older pages read from further back, which are no news.

/** Whether `message` is a teammate's: the DSP's, by anyone but the member `me`. */
const teammates = (message: Message, me: string) =>
  message.direction === 'out' && message.sender !== null && message.sender.userId !== me;

/** "Keisha just replied": a teammate by their first name, the phone by its own; "Someone" for
 * one without a name. */
export function repliedText(message: Message) {
  const sender = message.sender!;
  const name = sender.userId ? sender.name.split(' ')[0] : sender.name;
  return `${name || 'Someone'} just replied`;
}

/**
 * A conversation's teammates' replies since the member last saw it, for the member `me`: told of
 * what the page holds as it changes (`hold`), the first time as it's read; asked what the latest
 * of them says (`check`); and told the member saw what's held now (`seen`), as they start writing
 * or answer that, which counts once it's first read.
 */
export class Replies {
  /** Each message held as the member last saw it, and each reply since, by ID. */
  private known: Set<string> | undefined;
  private readonly news = new Map<string, Message>();
  private held: readonly Message[] = [];
  constructor(private readonly me: string) {}

  hold = (messages: readonly Message[]) => {
    this.held = messages;
    if (!this.known) {
      this.known = new Set(messages.map((message) => message.id));
      return;
    }
    for (const message of messages)
      if (!this.known.has(message.id) && teammates(message, this.me))
        this.news.set(message.id, message);
  };
  check = () => {
    let latest: Message | undefined;
    for (const reply of this.news.values()) if (!latest || reply.at > latest.at) latest = reply;
    return latest && repliedText(latest);
  };
  seen = () => {
    if (!this.known) return;
    this.known = new Set([
      ...this.known,
      ...this.news.keys(),
      ...this.held.map((message) => message.id),
    ]);
    this.news.clear();
  };
}

/** The `Replies` of a conversation whose `messages` the page holds, none until it's read. */
export function useReplied(messages: readonly Message[] | undefined, me: string) {
  const replies = useMemo(() => new Replies(me), [me]);
  useEffect(() => {
    if (messages) replies.hold(messages);
  }, [replies, messages]);
  return replies;
}
