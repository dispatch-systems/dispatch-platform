import { useCallback, useEffect, useRef, useState } from 'react';
import type { Group, GroupEntry, GroupMessages } from '../../../api/index.js';
import { getGroup } from '../../../api/client.js';
import { ApiError } from '../../../../../core/shell/frontend/runtime/api.js';
import { messageOf } from '../../../../../core/shell/frontend/lib/errors.js';
import type { Changed } from '../../inbox/useInbox.js';

/** Entries in the order written: by when, then by ID, as the server pages them. */
const written = (a: GroupEntry, b: GroupEntry) =>
  a.at !== b.at ? a.at - b.at : a.id < b.id ? -1 : a.id === b.id ? 0 : 1;
/** `kept` with `read`, each entry once, as it is now, in the order written. */
function joined(kept: readonly GroupEntry[], read: readonly GroupEntry[]) {
  const byId = new Map(kept.map((entry) => [entry.id, entry]));
  for (const entry of read) byId.set(entry.id, entry);
  return [...byId.values()].sort(written);
}

/** The group open, and how many times it's been read, which its people are read again by. */
type Opened = { id: string; group: Group; entries: GroupEntry[]; more: boolean; read: number };

/**
 * The group open, with the DSP's messages to it and their answers: the latest as it opens, again
 * each time an update changes it, and older ones as they're asked for, fifty at a time. `gone`
 * once it's no longer there.
 */
export function useGroup(id: string, changed: Changed) {
  const [opened, setOpened] = useState<Opened>();
  const [error, setError] = useState('');
  const [gone, setGone] = useState(false);
  const [older, setOlder] = useState(false);
  const kept = useRef<Opened | undefined>(undefined);
  const keep = useCallback((next: Opened) => {
    kept.current = next;
    setOpened(next);
  }, []);
  // The reads in turn: one answered after a later one is left. The pane is the group's alone, so
  // every read is of it.
  const reads = useRef({ asked: 0, shown: 0 });

  useEffect(() => {
    const ours = kept.current?.id === id;
    if (ours && changed.ids !== 'all' && !changed.ids.includes(id)) return;
    const turn = ++reads.current.asked;
    getGroup(id).then(
      (latest: GroupMessages) => {
        if (turn < reads.current.shown) return;
        reads.current.shown = turn;
        const before = kept.current?.id === id ? kept.current : undefined;
        keep({
          id,
          group: latest.group,
          entries: joined(before?.entries ?? [], latest.entries),
          more: before ? before.more : latest.more,
          read: (before?.read ?? 0) + 1,
        });
        setError('');
      },
      (cause) => {
        if (cause instanceof ApiError && cause.status === 404) setGone(true);
        else setError(messageOf(cause));
      },
    );
  }, [id, changed, keep]);

  /** Reads the fifty entries before the first shown. */
  const readOlder = useCallback(async () => {
    const shown = kept.current;
    if (!shown?.more || older || !shown.entries[0]) return;
    setOlder(true);
    try {
      const page = await getGroup(shown.id, shown.entries[0]);
      const now = kept.current;
      if (now?.id === shown.id)
        keep({ ...now, entries: joined(now.entries, page.entries), more: page.more });
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setOlder(false);
    }
  }, [keep, older]);
  /** Shows the group as one of the page's own writes left it, and a message it sent. */
  const show = useCallback(
    (group: Group, sent?: GroupEntry) => {
      const now = kept.current;
      if (now?.id !== group.id) return;
      keep({ ...now, group, entries: sent ? joined(now.entries, [sent]) : now.entries });
    },
    [keep],
  );
  return { opened: opened?.id === id ? opened : undefined, error, gone, readOlder, show };
}
