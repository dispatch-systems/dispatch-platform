import assert from 'node:assert/strict';
import test from 'node:test';
import type { DocumentsItem } from '../../api/client.js';
import { overlaid, type Pending } from '../../frontend/browser/browsing.js';

const item = (id: string, items: number | null): DocumentsItem => ({
  id,
  name: id,
  kind: items === null ? 'pdf' : 'folder',
  size: null,
  modifiedAt: '2026-11-04T00:00:00Z',
  modifiedBy: null,
  addedBy: null,
  items,
  url: `https://drive.google.com/drive/folders/${id}`,
  location: null,
  thumbnail: null,
  extension: null,
  shortcut: false,
  unavailable: false,
});

test("a folder's card counts a move once, whether or not the reading already holds it", () => {
  // November is moving from the top into Sheets, and Google hasn't answered yet.
  const november = item('november', null);
  const moving: Pending = {
    key: 'moved:1',
    folder: undefined,
    change: 'moved',
    item: november,
    to: 'sheets',
  };
  const count = (items: DocumentsItem[]) =>
    overlaid(items, [moving], undefined, false).find((each) => each.id === 'sheets')?.items;
  // Read before the move: Sheets holds one, and the card counts November going in.
  assert.equal(count([item('sheets', 1), november]), 2);
  // Read after Google made it: Sheets already counts November, which left the top.
  assert.equal(count([item('sheets', 2)]), 2);
});
