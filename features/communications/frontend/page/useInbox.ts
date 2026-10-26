import { useCallback, useEffect, useRef, useState } from 'react';
import type { Communications, Conversation, Group } from '../../api/index.js';
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
import {
  lastAnswered,
  listingOf,
  moreIn,
  shownWorkspace,
  updated,
  withConversation,
  withGroup,
  withOlder,
  type Section,
} from '../inbox.js';

/** What changed in the latest update: the conversations and groups, or every one after reading
 * afresh. */
export type Changed = { at: number; ids: readonly string[] | 'all' };

/**
 * The DSP's conversations and groups as they are now: every one waiting, and those answered of
 * every workspace or the one `chosen` (`team`, while it's one of two or more Communications works
 * in); read, then kept up to date by waiting on each change (`updates`), and read afresh when the
 * server says to, the page comes back into view, the browser comes back online or another
 * workspace is chosen. `changed` says which conversations and groups the latest update changed,
 * for the one open to read again.
 */
export function useInbox(chosen?: string) {
  const [inbox, setInbox] = useState<Communications>();
  const team = shownWorkspace(chosen, inbox?.workspaces);
  const [changed, setChanged] = useState<Changed>({ at: 0, ids: 'all' });
  const [error, setError] = useState('');
  const [generation, setGeneration] = useState(0);
  // The list the page's own writes and each update change in turn, and the workspace whose
  // answered conversations it holds, which reading older ones goes on in.
  const current = useRef<Communications | undefined>(undefined);
  const scope = useRef<string | undefined>(undefined);
  const change = useCallback((next: Communications) => {
    // The sidebar's count reads again as the page first reads how many wait, and once that
    // changes.
    if (current.current?.waiting !== next.waiting)
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
          const read = await getCommunications(request.signal, team);
          if (disposed || request.signal.aborted) return;
          fresh = false;
          scope.current = team;
          change(read);
          // Read afresh with nothing changed since, as when another workspace is chosen, the
          // conversation open needs no reading again.
          if (read.revision !== listed?.revision)
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
            const ids = [...update.conversations, ...update.groups].map((each) => each.id);
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
  }, [change, generation, team]);

  /** Shows a conversation as one of the page's own writes left it. */
  const show = useCallback(
    (conversation: Conversation) => {
      if (current.current) change(withConversation(current.current, conversation));
    },
    [change],
  );
  /** Shows a group as one of the page's own writes left it. */
  const showGroup = useCallback(
    (group: Group) => {
      if (current.current) change(withGroup(current.current, group));
    },
    [change],
  );
  const [reading, setReading] = useState(false);
  /** Reads the answered conversations and groups of `section` older than the last listed. */
  const readOlder = useCallback(
    async (section: Section) => {
      const listed = current.current;
      const last = listed && lastAnswered(listed, section);
      if (!listed || !moreIn(listed, section) || !last || reading) return;
      setReading(true);
      try {
        const asked = scope.current;
        const older = await olderConversations(last, listingOf(section, asked));
        // Read while another workspace's list came, it isn't of that list.
        if (current.current && scope.current === asked)
          change(withOlder(current.current, older, section));
      } catch (cause) {
        setError(messageOf(cause));
      } finally {
        setReading(false);
      }
    },
    [change, reading],
  );
  return { inbox, team, changed, error, refresh, show, showGroup, readOlder };
}
