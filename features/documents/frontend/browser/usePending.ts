import { useEffect, useRef, useState } from 'react';
import { moment, type DocumentsItem } from '../../api/client.js';
import { reconciled, type Pending, type Reading } from './browsing.js';

/**
 * The changes the page shows before the folder they're in is read again with them: each shown
 * as it starts, settled once Google confirms it, and dropped if Google refuses it, until a
 * reading after it (`reconciled`). Something made in the folder open is revealed.
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
        each.key === key
          ? { ...each, item: item ?? each.item, settled: Date.now(), confirmed: moment() }
          : each,
      ),
    );
  const drop = (key: string) => setPending((all) => all.filter((each) => each.key !== key));
  /** The ID something being made has until Google makes it. */
  const standIn = () => `pending:${changes.current}`;
  useEffect(() => {
    if (read) setPending((all) => reconciled(all, read, folder, searching));
  }, [read, folder, searching]);
  return { pending, show, settle, drop, standIn, reveal, revealed: () => setReveal(undefined) };
}
