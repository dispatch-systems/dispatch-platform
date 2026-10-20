import { useCallback, useEffect, useRef, useState } from 'react';
import type { Communications, Conversation } from '../../api/index.js';
import {
  getCommunications,
  getCommunicationsUpdates,
  olderConversations,
  waitingUrl,
} from '../../api/client.js';
import { ApiError } from '../../../../core/shell/frontend/runtime/api.js';
import { dataCache } from '../../../../core/shell/frontend/runtime/data-cache.js';
import { backoff } from '../../../../core/shell/frontend/lib/backoff.js';
import { messageOf } from '../../../../core/shell/frontend/lib/errors.js';
import { lastAnswered, updated, withConversation, withOlder } from '../inbox.js';

/** What changed in the latest update: the conversations, or every one after reading afresh. */
export type Changed = { at: number; ids: readonly string[] | 'all' };

/**
 * The DSP's conversations as they are now: read, then kept up to date by waiting on each change
 * (`updates`), and read afresh when the server says to, the page comes back into view or the
 * browser comes back online. `changed` says which conversations the latest update changed, for
 * the one open to read again.
 */
export function useInbox() {
  const [inbox, setInbox] = useState<Communications>();
  const [changed, setChanged] = useState<Changed>({ at: 0, ids: 'all' });
  const [error, setError] = useState('');
  const [generation, setGeneration] = useState(0);
  // The list the page's own writes and each update change in turn.
  const current = useRef<Communications | undefined>(undefined);
  const change = useCallback((next: Communications) => {
    // The sidebar's count reads again once the number waiting changes.
    if (current.current && current.current.waiting !== next.waiting)
      dataCache.invalidate((key) => key === waitingUrl);
    current.current = next;
    setInbox(next);
  }, []);
  /** Reads the list afresh, as when Slack was connected or updated. */
  const refresh = useCallback(() => setGeneration((n) => n + 1), []);

  useEffect(() => {
    let disposed = false;
    let controller: AbortController | undefined;
    let retry: ReturnType<typeof setTimeout> | undefined;
    let failures = 0;
    let fresh = true;
    const listen = async () => {
      if (disposed || document.hidden || controller || !navigator.onLine) return;
      const request = new AbortController();
      controller = request;
      let delay = 0;
      let stopped = false;
      try {
        const listed = current.current;
        if (fresh || !listed) {
          const read = await getCommunications(request.signal);
          if (disposed || request.signal.aborted) return;
          fresh = false;
          change(read);
          setChanged((last) => ({ at: last.at + 1, ids: 'all' }));
        } else {
          const update = await getCommunicationsUpdates(
            listed.revision,
            listed.typing.version,
            AbortSignal.any([request.signal, AbortSignal.timeout(30_000)]),
          );
          if (disposed || request.signal.aborted) return;
          if (update.reset) fresh = true;
          else {
            change(updated(current.current ?? listed, update));
            const ids = update.conversations.map((conversation) => conversation.id);
            if (ids.length) setChanged((last) => ({ at: last.at + 1, ids }));
          }
        }
        failures = 0;
        setError('');
      } catch (cause) {
        if (disposed || request.signal.aborted) return;
        stopped = cause instanceof ApiError && [401, 403].includes(cause.status);
        setError(messageOf(cause));
        fresh = true;
        delay = backoff(failures++);
      } finally {
        if (controller === request) controller = undefined;
        if (!disposed && !request.signal.aborted && !stopped && !document.hidden)
          retry = setTimeout(() => void listen(), delay);
      }
    };
    const resume = () => {
      if (retry) clearTimeout(retry);
      controller?.abort();
      controller = undefined;
      fresh = true;
      if (!document.hidden && navigator.onLine) void listen();
    };
    document.addEventListener('visibilitychange', resume);
    window.addEventListener('online', resume);
    resume();
    return () => {
      disposed = true;
      controller?.abort();
      if (retry) clearTimeout(retry);
      document.removeEventListener('visibilitychange', resume);
      window.removeEventListener('online', resume);
    };
  }, [change, generation]);

  /** Shows a conversation as one of the page's own writes left it. */
  const show = useCallback(
    (conversation: Conversation) => {
      if (current.current) change(withConversation(current.current, conversation));
    },
    [change],
  );
  const [reading, setReading] = useState(false);
  /** Reads the answered conversations older than the last listed. */
  const readOlder = useCallback(async () => {
    const listed = current.current;
    const last = listed && lastAnswered(listed);
    if (!listed?.more || !last || reading) return;
    setReading(true);
    try {
      const older = await olderConversations(last);
      if (current.current) change(withOlder(current.current, older));
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setReading(false);
    }
  }, [change, reading]);
  return { inbox, changed, error, refresh, show, readOlder };
}
