import assert from 'node:assert/strict';
import test from 'node:test';
import { dataCache } from '../../../../core/shell/frontend/runtime/data-cache.js';
import { credentials } from '../../../../core/shell/frontend/runtime/api.js';
import { feature } from '../../frontend/feature.js';
import type { DspView } from '../../../../core/accounts/api/index.js';
import type { DspRoute, RoutePrefetch } from '../../../../core/shell/frontend/runtime/slots.js';
import type { PrefetchRead } from '../../../../core/shell/frontend/runtime/prefetch.js';
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

const top: DocumentsFolder = {
  grantGeneration: 'b'.repeat(64),
  id: null,
  name: 'Documents',
  url: 'https://drive.google.com/drive/folders/top',
  path: [],
  items: [item('sheets', 2)],
};
const revision = 'a'.repeat(64);
const reply = (folder: DocumentsFolder | null, rev = revision) =>
  new Response(JSON.stringify({ revision: rev, folder }));
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

test("a folder's card waits for a newly asked unchanged reading after confirmation, including clock rollback", async (t) => {
  t.mock.timers.enable({ apis: ['Date'], now: 7_200_000 });
  dataCache.clear();
  t.after(() => dataCache.clear());
  const held = deferred<Response>();
  const entered = deferred<void>();
  let calls = 0;
  t.mock.method(globalThis, 'fetch', async (input: string) => {
    const query = new URL(input, 'http://dispatch.test').searchParams;
    if (++calls === 1) {
      assert.equal(query.has('since'), false);
      return reply(top);
    }
    assert.equal(query.get('since'), revision);
    if (calls === 2) {
      entered.resolve();
      return held.promise;
    }
    return reply(null);
  });
  const url = documentsFolderUrl();
  const read = () => dataCache.read(url, readFolder(url), true);
  const moving: Pending = {
    key: 'moved:0',
    change: 'moved',
    item: item('november', null),
    to: 'sheets',
  };
  const before = await read();
  const waiting = read();
  await entered.promise;
  let pending = reconciled([moving], before, undefined, false);
  pending = pending.map((each) => ({ ...each, settled: Date.now(), confirmed: moment() }));
  held.resolve(reply(null));
  const old = await waiting;
  assert.notEqual(old, before);
  assert.equal(reconciled(pending, old, undefined, false), pending);
  assert.equal(overlaid(old.items, pending, undefined, false)[0]?.items, 3);
  t.mock.timers.setTime(3_600_000);
  const after = await read();
  assert.notEqual(after, old);
  assert.ok(after.asked! > pending[0]!.confirmed!);
  const settled = reconciled(pending, after, undefined, false);
  assert.equal(overlaid(after.items, settled, undefined, false)[0]?.items, 2);
});

test('unchanged uses the captured complete baseline and rejects unsolicited or mismatched reuse', async (t) => {
  dataCache.clear();
  t.after(() => dataCache.clear());
  const url = documentsFolderUrl('safety', ' plan ');
  dataCache.put(url, { ...top, revision, asked: moment() });
  const held = deferred<Response>();
  const entered = deferred<void>();
  t.mock.method(globalThis, 'fetch', async (input: string) => {
    const parsed = new URL(input, 'http://dispatch.test');
    assert.equal(parsed.pathname, '/api/dsp/documents/folder/read');
    assert.equal(parsed.searchParams.get('id'), 'safety');
    assert.equal(parsed.searchParams.get('q'), 'plan');
    assert.equal(parsed.searchParams.get('since'), revision);
    entered.resolve();
    return held.promise;
  });
  const reading = readFolder(url)(new AbortController().signal);
  await entered.promise;
  // A later cache value must never be substituted for the revision the request supplied.
  dataCache.put(url, { ...top, items: [], revision: 'b'.repeat(64) });
  held.resolve(reply(null));
  const answer = await reading;
  assert.equal(answer.items, top.items);
  assert.equal(answer.revision, revision);

  t.mock.restoreAll();
  t.mock.method(globalThis, 'fetch', async () => reply(null, 'c'.repeat(64)));
  await assert.rejects(readFolder(url)(new AbortController().signal), {
    code: 'invalid_api_response',
  });
  dataCache.clear();
  await assert.rejects(readFolder(url)(new AbortController().signal), {
    code: 'invalid_api_response',
  });
});

test('full and unchanged answers cannot cross a session clear or scoped invalidation', async (t) => {
  const url = documentsFolderUrl();
  for (const full of [true, false]) {
    for (const clear of [true, false]) {
      dataCache.clear();
      dataCache.put(url, { ...top, revision, asked: moment() });
      const held = deferred<Response>();
      const entered = deferred<void>();
      t.mock.method(globalThis, 'fetch', async () => {
        entered.resolve();
        return held.promise;
      });
      // A direct caller has no ResponseCache-owned abort signal: the reader must also fence it.
      const reading = readFolder(url)(new AbortController().signal);
      await entered.promise;
      if (clear) dataCache.clear();
      else dataCache.invalidate((key) => key === url);
      const refused = assert.rejects(reading, { name: 'AbortError' });
      held.resolve(reply(full ? top : null));
      await refused;
      t.mock.restoreAll();
    }
  }
  dataCache.clear();
});

test('the reader closure keeps no revision across credentials changes; legacy cache starts full', async (t) => {
  dataCache.clear();
  t.after(() => {
    credentials('');
    dataCache.clear();
  });
  const url = documentsFolderUrl();
  const reader = readFolder(url);
  dataCache.put(url, top);
  const sent: (string | null)[] = [];
  t.mock.method(globalThis, 'fetch', async (input: string) => {
    sent.push(new URL(input, 'http://dispatch.test').searchParams.get('since'));
    return reply(top);
  });
  const first = await dataCache.read(url, reader, true);
  credentials('fixture-csrf', 'fixture-view');
  const next = await dataCache.read(url, reader, true);
  assert.deepEqual(sent, [null, null]);
  assert.ok(next.asked! > first.asked!);
  assert.notEqual(next, first);
});

test('Documents route warming supplies the same custom reader and logical cache key', async (t) => {
  dataCache.clear();
  t.after(() => dataCache.clear());
  let reads: PrefetchRead[] = [];
  // A permitted view for the manifest hook, as the route host supplies.
  const view = {
    features: ['documents'],
    role: { owner: false },
    permissions: ['documents.use'],
  } as DspView;
  const route = feature.routes![0]! as DspRoute;
  route.prefetch!({
    view,
    immediate: true,
    warm: (next: Parameters<RoutePrefetch['warm']>[0]) => {
      reads = next;
    },
  });
  const folder = reads.find(
    (read) => typeof read !== 'string' && read.url === documentsFolderUrl(),
  );
  assert.ok(folder && typeof folder !== 'string');
  t.mock.method(globalThis, 'fetch', async (input: string, options: RequestInit) => {
    assert.equal(input, '/api/dsp/documents/folder/read');
    assert.equal((options as RequestInit & { priority?: string }).priority, 'high');
    return reply(top);
  });
  const reading = (await dataCache.read(folder.url, (signal) =>
    folder.read(signal, 'high'),
  )) as FolderReading;
  assert.ok(reading.asked);
  assert.equal(reading.revision, revision);
  assert.equal(dataCache.peek(documentsFolderUrl()).data, reading);
});
