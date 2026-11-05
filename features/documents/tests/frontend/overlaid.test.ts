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
/** What the top's card for `id` counts, read as `items` with `pending` shown on it. */
const card = (items: DocumentsItem[], pending: Pending[], id: string) =>
  overlaid(items, pending, undefined, false).find((each) => each.id === id)?.items;

test("a folder's card counts a move once, whether or not the reading already holds it", () => {
  // November is moving from the top into Sheets, and Google hasn't answered yet.
  const november = item('november', null);
  const moving: Pending = { key: 'moved:1', change: 'moved', item: november, to: 'sheets' };
  // Read before the move: Sheets holds one, and the card counts November going in.
  assert.equal(card([item('sheets', 1), november], [moving], 'sheets'), 2);
  // Read after Google made it: Sheets already counts November, which left the top.
  assert.equal(card([item('sheets', 2)], [moving], 'sheets'), 2);
});

test('a move the reading cannot place counts until a reading after it', () => {
  // October went into Sheets, and back out with Undo, before the top was read again.
  const october = item('october', null);
  const into: Pending = {
    key: 'moved:1',
    change: 'moved',
    item: october,
    to: 'sheets',
    settled: 1,
  };
  const back: Pending = { key: 'moved:2', folder: 'sheets', change: 'moved', item: october };
  assert.equal(card([item('sheets', 0), october], [into, back], 'sheets'), 0);
  // A result of a search of the top, from Source, is moving into Target, and the search is
  // cleared before Google answers: the top's reading never listed it.
  const report = item('report', null);
  const moving: Pending = {
    key: 'moved:3',
    change: 'moved',
    item: report,
    to: 'target',
    searched: true,
  };
  assert.equal(card([item('source', 1), item('target', 0)], [moving], 'target'), 1);
});
