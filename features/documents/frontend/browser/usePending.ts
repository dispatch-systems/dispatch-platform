import { useEffect, useRef, useState } from 'react';
import type { DocumentsItem } from '../../api/client.js';
import { SETTLING_MS, type Pending } from './browsing.js';

/** What a reading of a folder or a search answers with, as far as the pending changes read it. */
type Reading = { items: DocumentsItem[] };

/**
 * The changes the page shows before the folder they're in is read again with them: each shown
 * as it starts, settled once Google confirms it, and dropped if Google refuses it. A change
 * Google confirmed goes once the folder is read with it, and a move stops counting in its
 * folders' cards once a reading after it does. Something made in the folder open is revealed.
 */
export function usePending(
  folder: string | undefined,
  read: Reading | undefined,
  searching: boolean,
) {
  const [pending, setPending] = useState<Pending[]>([]);
  const [reveal, setReveal] = useState<string>();
  const changes = useRef(0);
  const show = (change: Omit<Pending, 'key'>) => {
    if (change.change === 'made' && change.folder === folder) setReveal(change.item.id);
    const key = `${change.change}:${changes.current++}`;
    setPending((all) => [...all, { ...change, key }]);
    return key;
  };
  const settle = (key: string, item?: DocumentsItem) =>
    setPending((all) =>
      all.map((each) =>
        each.key === key ? { ...each, item: item ?? each.item, settled: Date.now() } : each,
      ),
    );
  const drop = (key: string) => setPending((all) => all.filter((each) => each.key !== key));
  /** The ID something being made has until Google makes it. */
  const standIn = () => `pending:${changes.current}`;
  useEffect(() => {
    if (!read) return;
    setPending((all) => {
      let changed = false;
      const left = all.flatMap((each) => {
        if (!each.settled) return [each];
        const listed = read.items.find((item) => item.id === each.item.id);
        const inside = !searching && each.folder === folder;
        const shown =
          Date.now() - each.settled > SETTLING_MS ||
          (each.change === 'renamed'
            ? listed?.name === each.item.name
            : each.change === 'made'
              ? inside && Boolean(listed)
              : each.change === 'moved'
                ? !searching && (each.to === folder ? Boolean(listed) : inside && !listed)
                : inside && !listed);
        changed ||= shown || (each.change === 'moved' && !each.counted);
        if (shown) return [];
        return [each.change === 'moved' ? { ...each, counted: true } : each];
      });
      return changed ? left : all;
    });
  }, [read, folder, searching]);
  return { pending, show, settle, drop, standIn, reveal, revealed: () => setReveal(undefined) };
}
