import { useCallback, useEffect, useRef, useState } from 'react';
import type { Conversation, ConversationMessages, Message } from '../../api/index.js';
import { getConversation } from '../../api/client.js';
import { ApiError } from '../../../../core/shell/frontend/runtime/api.js';
import { messageOf } from '../../../../core/shell/frontend/lib/errors.js';
import type { Changed } from './useInbox.js';

/** Slack's IDs of messages, `<seconds>.<microseconds>`, in the order they were written. */
const byTs = (a: Message, b: Message) =>
  a.ts.padStart(24, '0') < b.ts.padStart(24, '0') ? -1 : a.ts === b.ts ? 0 : 1;
/** `kept` with `read`, each message once, as it is now, in the order written. */
function joined(kept: readonly Message[], read: readonly Message[]) {
  const byId = new Map(kept.map((message) => [message.id, message]));
  for (const message of read) byId.set(message.id, message);
  return [...byId.values()].sort(byTs);
}

type Opened = { id: string; conversation: Conversation; messages: Message[]; more: boolean };

/**
 * The open conversation and its messages: the latest when it opens, again each time an update
 * changes it, and older ones as they're asked for, fifty at a time. `gone` once it's no longer
 * there.
 */
export function useConversation(id: string | undefined, changed: Changed) {
  const [opened, setOpened] = useState<Opened>();
  const [error, setError] = useState('');
  const [gone, setGone] = useState(false);
  const [older, setOlder] = useState(false);
  const kept = useRef<Opened | undefined>(undefined);
  const keep = useCallback((next: Opened) => {
    kept.current = next;
    setOpened(next);
  }, []);
  // The conversation open, and the reads of it in turn: a read answered after a later one is
  // left, as is one of a conversation no longer open.
  const wanted = useRef<string | undefined>(undefined);
  const reads = useRef({ asked: 0, shown: 0 });

  useEffect(() => {
    wanted.current = id;
    if (!id) return;
    const ours = kept.current?.id === id;
    if (ours && changed.ids !== 'all' && !changed.ids.includes(id)) return;
    if (!ours) {
      kept.current = undefined;
      setOpened(undefined);
      setGone(false);
    }
    const turn = ++reads.current.asked;
    getConversation(id).then(
      (latest: ConversationMessages) => {
        if (wanted.current !== id || turn < reads.current.shown) return;
        reads.current.shown = turn;
        const before = kept.current?.id === id ? kept.current : undefined;
        keep({
          id,
          conversation: latest.conversation,
          messages: joined(before?.messages ?? [], latest.messages),
          more: before ? before.more : latest.more,
        });
        setError('');
      },
      (cause) => {
        if (wanted.current !== id) return;
        if (cause instanceof ApiError && cause.status === 404) setGone(true);
        else setError(messageOf(cause));
      },
    );
  }, [id, changed, keep]);

  /** Reads the fifty messages before the first shown. */
  const readOlder = useCallback(async () => {
    const shown = kept.current;
    if (!shown?.more || older || !shown.messages[0]) return;
    setOlder(true);
    try {
      const page = await getConversation(shown.id, shown.messages[0]);
      const now = kept.current;
      if (now?.id === shown.id)
        keep({ ...now, messages: joined(now.messages, page.messages), more: page.more });
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setOlder(false);
    }
  }, [keep, older]);
  /** Shows the conversation as one of the page's own writes left it, and a message it sent. */
  const show = useCallback(
    (conversation: Conversation, sent?: Message) => {
      const now = kept.current;
      if (now?.id !== conversation.id) return;
      keep({ ...now, conversation, messages: sent ? joined(now.messages, [sent]) : now.messages });
    },
    [keep],
  );
  const shown = opened?.id === id ? opened : undefined;
  return { opened: shown, error, gone, readingOlder: older, readOlder, show };
}
