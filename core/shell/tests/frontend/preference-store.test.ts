import test from 'node:test';
import assert from 'node:assert/strict';
import type { DspView, PreferenceSaved } from '../../../accounts/api/index.js';
import { PreferenceStore, type Post } from '../../frontend/runtime/preference-store.js';

type Arrangement = { order: string[]; hidden: string[] };
type Body = { revision: number; value: unknown };

/** A server keeping one preference as core does: a save made from another revision is told what's kept. */
function server(kept: { revision: number; value?: unknown }) {
  const calls: { body: Body; answer: (saved?: PreferenceSaved) => void; refuse: () => void }[] = [];
  let waiting: (() => void) | undefined;
  const post: Post = (_token, _id, body) =>
    new Promise((resolve, reject) => {
      const keep = () => {
        if (body.revision !== kept.revision) return { outcome: 'stale', current: kept } as const;
        kept =
          body.value === null
            ? { revision: 0 }
            : { revision: body.revision + 1, value: body.value };
        return { outcome: 'saved', revision: kept.revision } as const;
      };
      calls.push({
        body,
        answer: (saved) => resolve(saved ?? keep()),
        refuse: () => reject(new Error('offline')),
      });
      waiting?.();
    });
  return {
    post,
    calls,
    get kept() {
      return kept;
    },
    /** Another screen of theirs saves first. */
    elsewhere(value: unknown) {
      kept = { revision: kept.revision + 1, value };
    },
    /** Waits until `count` saves have been sent. */
    sent: (count: number) =>
      new Promise<void>((resolve) => {
        const check = () => {
          if (calls.length < count) return;
          waiting = undefined;
          resolve();
        };
        waiting = check;
        check();
      }),
  };
}
const viewOf = (preferences: DspView['preferences']) =>
  ({ dsp: { id: 'dsp_1' }, token: 'view.', preferences }) as unknown as DspView;
const hide =
  (id: string) =>
  (now: unknown): Arrangement => {
    const { order, hidden } = (now as Arrangement | undefined) ?? { order: [], hidden: [] };
    return { order, hidden: [...hidden, id] };
  };

test("a change made from an older revision is made again on what another screen kept, so neither replaces the other's", async () => {
  const first = { order: ['team'], hidden: [] };
  const remote = server({ revision: 1, value: first });
  const store = new PreferenceStore(remote.post);
  const kept = store.of(
    viewOf({ 'shell.sidebar': { revision: 1, value: first } }),
    'shell.sidebar',
  );
  remote.elsewhere({ order: ['vans', 'team'], hidden: [] });
  store.change(kept, hide('dvic'));
  assert.deepEqual(kept.shown, { order: ['team'], hidden: ['dvic'] });
  await remote.sent(1);
  remote.calls[0]!.answer();
  await remote.sent(2);
  assert.deepEqual(remote.calls[1]!.body, {
    revision: 2,
    value: { order: ['vans', 'team'], hidden: ['dvic'] },
  });
  const shown = new Promise((resolve) => store.subscribe(() => resolve(undefined)));
  remote.calls[1]!.answer();
  await shown;
  assert.deepEqual(remote.kept, {
    revision: 3,
    value: { order: ['vans', 'team'], hidden: ['dvic'] },
  });
  assert.deepEqual(kept.shown, remote.kept.value);
});

test('changes go one save at a time, the latest with those before, and one that fails stays shown and goes with the next', async () => {
  const remote = server({ revision: 0 });
  const store = new PreferenceStore(remote.post);
  let failed: (error: unknown) => void = () => {};
  const failure = new Promise((resolve) => (failed = resolve));
  const kept = store.of(viewOf({}), 'shell.sidebar');
  kept.failed = failed;
  store.change(kept, hide('a'));
  await remote.sent(1);
  store.change(kept, hide('b'));
  store.change(kept, hide('c'));
  assert.equal(remote.calls.length, 1);
  assert.deepEqual(kept.shown, { order: [], hidden: ['a', 'b', 'c'] });
  remote.calls[0]!.answer();
  await remote.sent(2);
  assert.deepEqual(remote.calls[1]!.body, {
    revision: 1,
    value: { order: [], hidden: ['a', 'b', 'c'] },
  });
  remote.calls[1]!.refuse();
  assert.equal(((await failure) as Error).message, 'offline');
  // A view opened again meanwhile never replaces what is still to be kept.
  assert.equal(store.of(viewOf({}), 'shell.sidebar'), kept);
  assert.deepEqual(kept.shown, { order: [], hidden: ['a', 'b', 'c'] });
  store.change(kept, () => undefined);
  await remote.sent(3);
  assert.deepEqual(remote.calls[2]!.body, { revision: 1, value: null });
});
