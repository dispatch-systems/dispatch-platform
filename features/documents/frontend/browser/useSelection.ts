import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from 'react';
import type { DocumentsItem } from '../../api/client.js';
import { making } from './browsing.js';

/**
 * What's selected among `items`: a click with Shift, Ctrl or ⌘ selects instead of opening, Ctrl
 * or ⌘ adding or taking away one and Shift selecting those from the last one picked; Escape and
 * a change of folder, search or filter let go.
 */
export function useSelection(items: DocumentsItem[], resets: readonly unknown[]) {
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const anchor = useRef<string>(undefined);
  // eslint-disable-next-line react-hooks/exhaustive-deps -- `resets` names what a change of lets go
  useEffect(() => setSelected(new Set()), resets);
  useEffect(() => {
    if (!selected.size) return;
    const clear = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setSelected(new Set());
    };
    document.addEventListener('keydown', clear);
    return () => document.removeEventListener('keydown', clear);
  }, [selected]);
  const pick = (item: DocumentsItem, event: ReactMouseEvent) => {
    if (!(event.shiftKey || event.metaKey || event.ctrlKey) || making(item)) return;
    event.preventDefault();
    const ids = items.filter((each) => !making(each)).map((each) => each.id);
    const start = anchor.current ? ids.indexOf(anchor.current) : -1;
    if (event.shiftKey && start >= 0) {
      const end = ids.indexOf(item.id);
      setSelected(new Set(ids.slice(Math.min(start, end), Math.max(start, end) + 1)));
      return;
    }
    anchor.current = item.id;
    setSelected((all) => {
      const next = new Set(all);
      if (!next.delete(item.id)) next.add(item.id);
      return next;
    });
  };
  /** The selection, when `item` is in it, or else `item`. */
  const chosen = (item: DocumentsItem) =>
    selected.has(item.id) ? items.filter((each) => selected.has(each.id)) : [item];
  const clear = () => setSelected(new Set());
  return { selected, pick, chosen, clear };
}
