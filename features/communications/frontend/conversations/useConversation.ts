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
  return reads.reads;
}

/**
 * The open conversation and its messages, as the page's `reads` hold it: at once as last read, if
 * it was, and read when it opens unless no update changed it since, and again each time an update
 * changes it; older ones as they're asked for, fifty at a time. `gone` once it's no longer there.
 */
export function useConversation(reads: ConversationReads, id: string) {
  const held = useSyncExternalStore(
    useCallback((watch: () => void) => reads.watch(id, watch), [reads, id]),
    () => reads.held(id),
  );
  useEffect(() => reads.open(id), [reads, id]);
  const readOlder = useCallback(() => void reads.readOlder(id), [reads, id]);
  const show = useCallback(
    (conversation: Conversation, message?: Message) => reads.show(conversation, message),
    [reads],
  );
  return { ...held, readOlder, show };
}
