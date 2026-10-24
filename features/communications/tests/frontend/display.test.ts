import assert from 'node:assert/strict';
import test from 'node:test';
import type { MessageDisplay } from '../../api/index.js';
import { defaults } from '../../frontend/display/display.js';
import { autosave, type Perform } from '../../frontend/display/saving.js';

/** The clash a save gets when another session saved first. */
class Clash extends Error {}

/**
 * Autosave as the Settings section has it, with what the test holds: each save, answered by the
 * test; the moment after a change, which the test passes; what's shown; and the error a save
 * leaves, as `useAction` keeps it, cleared as each save starts.
 */
function saving({ revision = 0, view = () => 'north' as unknown } = {}) {
  const calls: { values: MessageDisplay; revision: number }[] = [];
  const answers: { resolve: (revision: number) => void; reject: (error: unknown) => void }[] = [];
  const state = { shown: undefined as MessageDisplay | undefined, error: '', rereads: 0, most: 0 };
  let saving = 0;
  const perform: Perform = async (work) => {
    state.most = Math.max(state.most, ++saving);
    state.error = '';
    try {
      await work();
    } catch (error) {
      state.error = (error as Error).message;
    } finally {
      saving--;
    }
  };
  let pending: (() => void) | undefined;
  const saver = autosave({
    save: (values, revision) => {
      calls.push({ values, revision });
      return new Promise<number>((resolve, reject) => answers.push({ resolve, reject }));
    },
    view,
    revision,
    perform,
    later: (then) => {
      pending = then;
      return () => (pending = undefined);
    },
    shown: (draft) => (state.shown = draft),
    clash: (error) => error instanceof Clash,
    reread: () => state.rereads++,
  });
  /** The moment after the last change passes. */
  const wait = () => pending?.();
  return { saver, calls, answers, state, wait };
}
/** Until what's queued has had its turn. */
const turns = () => new Promise((resolve) => setImmediate(resolve));

test('choices save a moment after the last, one at a time, each the newest, on the revision before it', async () => {
  const { saver, calls, answers, state, wait } = saving({ revision: 5 });
  saver.change({ ...defaults, text_size: 15 });
  wait();
  await turns();
  // While the first saves, two more changes, then the moment after them: only the newer goes.
  saver.change({ ...defaults, text_size: 16 });
  saver.change({ ...defaults, text_size: 17 });
  wait();
  answers[0]!.resolve(6);
  await turns();
  // Saved, the first isn't the newest: what's shown stays the newest made.
  assert.equal(state.shown?.text_size, 17);
  answers[1]!.resolve(7);
  await turns();
  assert.deepEqual(
    calls.map((call) => [call.values.text_size, call.revision]),
    [
      [15, 5],
      [17, 6],
    ],
  );
  assert.equal(state.most, 1);
  // The newest saved, Settings holds what's shown.
  assert.equal(state.shown, undefined);
});

test('choices made in one view are never saved once another is open', async () => {
  let view = 'north';
  const { saver, calls, wait } = saving({ view: () => view });
  saver.change({ ...defaults, team_color: 'green' });
  view = 'summit';
  wait();
  await turns();
  assert.equal(calls.length, 0);
});

test('a save queued behind one that fails waits for it, and leaves no error once it succeeds', async () => {
  const { saver, calls, answers, state, wait } = saving();
  saver.change({ ...defaults, spacing: 'compact' });
  wait();
  await turns();
  saver.change({ ...defaults, spacing: 'spacious' });
  wait();
  await turns();
  assert.equal(calls.length, 1);
  answers[0]!.reject(new Error('offline'));
  await turns();
  assert.equal(calls.length, 2);
  answers[1]!.resolve(1);
  await turns();
  assert.equal(state.error, '');
  assert.equal(state.most, 1);
});

test('a clash drops every change not yet saved, reads Settings again, and takes none until it has', async () => {
  const { saver, calls, answers, state, wait } = saving({ revision: 3 });
  saver.change({ ...defaults, phone_text_size: 15 });
  wait();
  await turns();
  // Another change queued behind the save that clashes, and one more still waiting.
  saver.change({ ...defaults, phone_text_size: 16 });
  wait();
  saver.change({ ...defaults, phone_text_size: 17 });
  answers[0]!.reject(new Clash('changed in another session'));
  await turns();
  assert.equal(state.error, 'changed in another session');
  assert.equal(state.rereads, 1);
  assert.equal(state.shown, undefined);
  wait();
  await turns();
  assert.equal(calls.length, 1);
  // Until Settings is read again, a change isn't taken; then it saves on what it holds.
  assert.equal(saver.change({ ...defaults, phone_text_size: 18 }), false);
  saver.reread(4);
  assert.equal(saver.change({ ...defaults, phone_text_size: 18 }), true);
  wait();
  await turns();
  assert.deepEqual(calls.at(-1), { values: { ...defaults, phone_text_size: 18 }, revision: 4 });
  // The clash's error stays until the next save succeeds.
  assert.equal(state.error, '');
});

test('leaving before the moment passes saves at once, and says so if it fails', async () => {
  const { saver, calls, answers, state } = saving();
  saver.change({ ...defaults, enter_sends: false });
  void saver.flush();
  await turns();
  assert.equal(calls.length, 1);
  answers[0]!.reject(new Error('offline'));
  await turns();
  assert.equal(state.error, 'offline');
});
