import assert from 'node:assert/strict';
import test from 'node:test';
import { ResponseCache } from '../../../../core/shell/frontend/lib/response-cache.js';
import {
  documentsFolderUrl,
  moment,
  readFolder,
  type DocumentsFolder,
  type DocumentsItem,
  type FolderReading,
} from '../../api/client.js';
import { overlaid, reconciled, type Pending } from '../../frontend/browser/browsing.js';

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

test("a folder's card stops counting a move at a reading asked for after Google answered it, though Google reads the same", async (t) => {
  t.mock.timers.enable({ apis: ['Date'], now: 7_200_000 });
  // Google took November's move from the top into Sheets before answering it: every reading of
  // the top lists Sheets holding 2, and November no more.
  const top: DocumentsFolder = {
    id: null,
    name: 'Documents',
    url: 'https://drive.google.com/drive/folders/top',
    path: [],
    items: [item('sheets', 2)],
  };
  t.mock.method(globalThis, 'fetch', async () => new Response(JSON.stringify(top)));
  const cache = new ResponseCache({ entries: 8, bytes: 1_000_000, freshMs: 60_000 });
  const url = documentsFolderUrl();
  const read = () => cache.read(url, readFolder(url), true);
  const card = (pending: Pending[], reading: FolderReading) =>
    overlaid(reading.items, pending, undefined, false).find((each) => each.id === 'sheets')?.items;
  // As the page does, the pending changes see a reading when it is a new one.
  const seen = (pending: Pending[], reading: FolderReading, last: FolderReading) =>
    reading === last ? pending : reconciled(pending, reading, undefined, false);

  const moving: Pending = {
    key: 'moved:0',
    change: 'moved',
    item: item('november', null),
    to: 'sheets',
  };
  const before = await read();
  let pending = reconciled([moving], before, undefined, false);
  // Google answers within the same tick of the clock as the reading was asked for.
  pending = pending.map((each) => ({ ...each, settled: Date.now(), confirmed: moment() }));
  // A reading asked for before Google answered says nothing of the move, though it holds it.
  assert.equal(reconciled(pending, before, undefined, false), pending);
  // The clock is set back an hour before the page reads the top again.
  t.mock.timers.setTime(3_600_000);
  const after = await read();
  assert.equal(card(seen(pending, after, before), after), 2);
});
