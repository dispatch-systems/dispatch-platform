import { useEffect, useState, useSyncExternalStore } from 'react';
import { hashQuery } from '../../../../core/shell/frontend/runtime/navigation.js';
import type { DocumentsItem, FolderStep, NewKind } from '../../api/client.js';

// What the browser shows and how: its filters, what New makes, the folder the address names, the
// changes it shows before Google confirms them, and how a listing is overlaid with them.

export type Filter = 'all' | 'folders' | 'docs' | 'sheets' | 'slides' | 'uploads';
export const FILTERS: [Filter, string][] = [
  ['all', 'All'],
  ['folders', 'Folders'],
  ['docs', 'Docs'],
  ['sheets', 'Sheets'],
  ['slides', 'Slides'],
  ['uploads', 'Uploads'],
];
export const shown = (filter: Filter) => (item: DocumentsItem) => {
  switch (filter) {
    case 'all':
      return true;
    case 'folders':
      return item.kind === 'folder';
    case 'docs':
      return item.kind === 'doc';
    case 'sheets':
      return item.kind === 'sheet';
    case 'slides':
      return item.kind === 'slides';
    case 'uploads':
      return ['pdf', 'image', 'file'].includes(item.kind);
  }
};

export const NEW: [NewKind, string, string][] = [
  ['folder', 'Folder', 'Untitled folder'],
  ['doc', 'Google Doc', 'Untitled document'],
  ['sheet', 'Google Sheet', 'Untitled spreadsheet'],
  ['slides', 'Google Slides', 'Untitled presentation'],
];

/** The folder the address names, followed as it changes. */
export function useFolder() {
  return useSyncExternalStore(
    (changed) => {
      window.addEventListener('hashchange', changed);
      return () => window.removeEventListener('hashchange', changed);
    },
    () => hashQuery().get('folder') ?? undefined,
  );
}
/** `value`, once it stops changing for a moment. */
export function useSettled(value: string, ms = 150) {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return settled;
}

export const LAYOUT = 'documents.layout';
export type Layout = 'list' | 'grid';

/** What the page is asked to do with an item. */
export type Asked =
  | { to: 'make'; kind: NewKind }
  | { to: 'rename'; item: DocumentsItem }
  | { to: 'move'; items: DocumentsItem[] }
  | { to: 'trash'; item: DocumentsItem };

/**
 * A change the page shows before the folder it's in is read again with it: made, uploaded,
 * renamed, moved or trashed in `folder`. Something being made has a stand-in's ID until Google
 * makes it; something moved goes `to` a folder, or the top. Once Google confirmed it, it shows
 * until a reading of the folder does, or a while, and a move counts in the cards of the folders
 * it left and went into until they're read again (`counted`).
 */
export type Pending = {
  key: string;
  folder?: string;
  change: 'made' | 'renamed' | 'moved' | 'trashed';
  item: DocumentsItem;
  to?: string;
  settled?: number;
  counted?: boolean;
};
export const SETTLING_MS = 30_000;
/** Something Google moved, and the folder it was in: `null` for the top. */
export type Moved = { item: DocumentsItem; from: string | null };
/** What the page's own drags carry, beside a link: the IDs of what's dragged. */
export const CARRIED = 'application/x-dispatch-documents';
/** Whether `item` stands in for something Google is still making. */
export const making = (item: DocumentsItem) => item.id.startsWith('pending:');
/** How a row or card shows: dimmed while Google makes it, or once its file is gone. */
export const look = (item: DocumentsItem) =>
  making(item) ? ' documents-making' : item.unavailable ? ' documents-unavailable' : '';

/** Folders first, by name; then files, the most recently changed first: as the server sorts. */
export function arranged(items: DocumentsItem[]) {
  const order = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
  return [...items].sort(
    (a, b) =>
      Number(b.kind === 'folder') - Number(a.kind === 'folder') ||
      (a.kind === 'folder'
        ? order(a.name.toLowerCase(), b.name.toLowerCase())
        : order(b.modifiedAt, a.modifiedAt)),
  );
}
/** What a folder or a search shows, with the changes it hasn't been read with yet. */
export function overlaid(
  items: DocumentsItem[],
  pending: Pending[],
  folder: string | undefined,
  searching: boolean,
) {
  const here = pending.filter((each) =>
    each.change === 'made' ? !searching && each.folder === folder : true,
  );
  if (!here.length) return items;
  const latest = (id: string) =>
    here.findLast((each) => each.change !== 'made' && each.item.id === id);
  // A folder's card counts what moved in or out of it until it's read again.
  const counted = (item: DocumentsItem) => {
    if (item.items === null) return item;
    const moved = here.filter((each) => each.change === 'moved' && !each.counted);
    const into = moved.filter((each) => each.to === item.id).length;
    const out = moved.filter((each) => each.folder === item.id).length;
    return into === out ? item : { ...item, items: Math.max(0, item.items + into - out) };
  };
  const shown = items.flatMap((item) => {
    const change = latest(item.id);
    if (!change) return [counted(item)];
    if (change.change === 'trashed') return [];
    // A search shows what moved where it went; a folder, only what's in it.
    if (change.change === 'moved' && !searching && change.to !== folder) return [];
    return [{ ...counted(item), name: change.item.name, location: change.item.location }];
  });
  if (searching) return shown;
  const arrived = here
    .filter(
      (each) =>
        (each.change === 'made' ||
          (each.change === 'moved' && each.to === folder && latest(each.item.id) === each)) &&
        !items.some((item) => item.id === each.item.id),
    )
    .map((each) => each.item);
  return arranged([...arrived, ...shown]);
}

/** The way to each folder the page has shown, so its path shows before the folder comes. */
export const trails = new Map<string, FolderStep[]>();
