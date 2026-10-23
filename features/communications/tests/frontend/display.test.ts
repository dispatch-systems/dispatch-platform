import assert from 'node:assert/strict';
import test from 'node:test';
import type { MessageDisplay } from '../../api/index.js';
import { defaults } from '../../frontend/display/display.js';
import { choicesSaver } from '../../frontend/display/saving.js';

/** Saves the test answers itself: each call, and what settles it. */
function saves() {
  const calls: { values: MessageDisplay; revision: number }[] = [];
  const answers: { resolve: (revision: number) => void; reject: (error: unknown) => void }[] = [];
  const save = (values: MessageDisplay, revision: number) => {
    calls.push({ values, revision });
    return new Promise<number>((resolve, reject) => answers.push({ resolve, reject }));
  };
  return { calls, answers, save };
}
/** Until what's queued has had its turn. */
const turns = () => new Promise((resolve) => setImmediate(resolve));

test('choices save one at a time, each the newest waiting, on the revision the save before answered', async () => {
  const { calls, answers, save } = saves();
  const saver = choicesSaver({ save, view: () => 'north', revision: 5 });
  saver.change({ ...defaults, text_size: 15 });
  const first = saver.flush();
  await turns();
  // While the first is saving, two more changes: only the newer goes.
  saver.change({ ...defaults, text_size: 16 });
  saver.change({ ...defaults, text_size: 17 });
  const second = saver.flush();
  await turns();
  assert.equal(calls.length, 1);
  answers[0]!.resolve(6);
  assert.deepEqual(await first, { version: 1 });
  await turns();
  assert.deepEqual(
    calls.map((call) => [call.values.text_size, call.revision]),
    [
      [15, 5],
      [17, 6],
    ],
  );
  answers[1]!.resolve(7);
  assert.deepEqual(await second, { version: 3 });
  assert.equal(saver.made, 3);
});

test('choices made in one view are never saved once another is open', async () => {
  const { calls, save } = saves();
  let view = 'north';
  const saver = choicesSaver({ save, view: () => view, revision: 0 });
  saver.change({ ...defaults, team_color: 'green' });
  view = 'summit';
  assert.deepEqual(await saver.flush(), { version: 1, dropped: true });
  assert.equal(calls.length, 0);
});

test('a failed save fails its own turn only: the next goes on from the last revision answered', async () => {
  const { calls, answers, save } = saves();
  const saver = choicesSaver({ save, view: () => 'north', revision: 2 });
  saver.change({ ...defaults, spacing: 'compact' });
  const failed = saver.flush();
  await turns();
  const offline = new Error('offline');
  answers[0]!.reject(offline);
  assert.deepEqual(await failed, { version: 1, error: offline });
  saver.change({ ...defaults, spacing: 'spacious' });
  const next = saver.flush();
  await turns();
  answers[1]!.resolve(3);
  assert.deepEqual(await next, { version: 2 });
  assert.deepEqual(
    calls.map((call) => call.revision),
    [2, 2],
  );
});
