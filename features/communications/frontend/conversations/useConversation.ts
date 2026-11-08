import { useCallback, useEffect, useLayoutEffect, useState, useSyncExternalStore } from 'react';
import type { Conversation, Message } from '../../api/index.js';
import type { Changed } from '../inbox/useInbox.js';
import { ConversationReads } from './reads.js';

/**
 * The conversations the page reads, at the DSP `dsp`, told of each update, `changed`: before the
 * conversation open's own effects, so one opened as the page reads afresh is read once.
 */
export function useConversationReads(dsp: string, changed: Changed) {
  const [reads, setReads] = useState(() => ({ dsp, reads: new ConversationReads() }));
  if (reads.dsp !== dsp) setReads({ dsp, reads: new ConversationReads() });
  useLayoutEffect(() => reads.reads.change(changed), [reads, changed]);
  useEffect(() => () => reads.reads.dispose(), [reads]);
  return reads.reads;
}

/**
 * The open conversation and its messages, as the page's `reads` hold it: at once as last read, if
 * it was, and read when it opens unless no update changed it since, and again each time an update
 * changes it, or as asked again once a read failed (`retry`); older ones as they're asked for,
 * fifty at a time. `gone` once it's no longer there.
 */
export function useConversation(reads: ConversationReads, id: string) {
  const held = useSyncExternalStore(
    useCallback((watch: () => void) => reads.watch(id, watch), [reads, id]),
    () => reads.held(id),
  );
  useEffect(() => reads.open(id), [reads, id]);
  // Not while a read of older ones is under way, and made again as it ends, so what reads them as
  // it's in view looks again.
  const { older } = held;
  const readOlder = useCallback(() => {
    if (!older) void reads.readOlder(id);
  }, [reads, id, older]);
  const retry = useCallback(() => reads.open(id), [reads, id]);
  const show = useCallback(
    (conversation: Conversation, message?: Message) => reads.show(conversation, message),
    [reads],
  );
  return { ...held, readOlder, retry, show };
}
