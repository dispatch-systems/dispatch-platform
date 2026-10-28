import { useCallback, useEffect, useRef, useState } from 'react';
import type { Conversation, ConversationMessages, Message, Workday } from '../../api/index.js';
import { getConversation } from '../../api/client.js';
import { ApiError } from '../../../../core/shell/frontend/runtime/api.js';
import { messageOf } from '../../../../core/shell/frontend/lib/errors.js';
import { fresh, joined } from './messages.js';
import type { Changed } from '../inbox/useInbox.js';

/** The conversation open, and when its latest read answered, by the page's own clock. */
type Opened = Omit<ConversationMessages, 'messages'> & {
  id: string;
  messages: Message[];
  readAt: number;
};

/** `kept` workdays with those `read`, each day once, as last read. */
const withDays = (kept: readonly Workday[], read: readonly Workday[]) => [
  ...kept.filter((day) => !read.some((each) => each.day === day.day)),
  ...read,
];

/**
 * The open conversation and its messages: the latest when it opens, again each time an update
 * changes it, each read as it is now, and older ones as they're asked for, fifty at a time. `gone`
 * once it's no longer there.
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
  // Each message the page sent, by the read asked last as it showed: one asked before then may
  // not hold it yet.
  const sent = useRef(new Map<string, number>());

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
        const unread = (message: Message) => (sent.current.get(message.id) ?? 0) >= turn;
        keep({
          ...latest,
          id,
          readAt: performance.now(),
          workdays: withDays(before?.workdays ?? [], latest.workdays),
          ...fresh(before, latest, unread),
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
    // A read of the latest shown before the page comes may have filed some of its messages in
    // another conversation: the page is then from before, and is read again while it's in view.
    const since = reads.current.shown;
    try {
      const page = await getConversation(shown.id, shown.messages[0]);
      const now = kept.current;
      if (now?.id === shown.id && reads.current.shown === since)
        keep({
          ...now,
          messages: joined(now.messages, page.messages),
          more: page.more,
          workdays: withDays(now.workdays, page.workdays),
        });
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setOlder(false);
    }
  }, [keep, older]);
  /** Shows the conversation as one of the page's own writes left it, and a message it sent. */
  const show = useCallback(
    (conversation: Conversation, message?: Message) => {
      const now = kept.current;
      if (now?.id !== conversation.id) return;
      if (message) sent.current.set(message.id, reads.current.asked);
      keep({
        ...now,
        conversation,
        messages: message ? joined(now.messages, [message]) : now.messages,
      });
    },
    [keep],
  );
  const shown = opened?.id === id ? opened : undefined;
  return { opened: shown, error, gone, readingOlder: older, readOlder, show };
}
