import assert from 'node:assert/strict';
import test from 'node:test';
import { Drafts } from '../../frontend/conversations/drafts.js';

/** The browser's tab's storage, as a reload finds it. */
const tab = new Map<string, string>();
Object.assign(globalThis, {
  sessionStorage: {
    getItem: (key: string) => tab.get(key) ?? null,
    setItem: (key: string, value: string) => void tab.set(key, value),
    removeItem: (key: string) => void tab.delete(key),
  },
});

test('what’s written stays each conversation’s, for each member at each DSP, through a reload, and a send takes out only what went', () => {
  const drafts = new Drafts('dsp_a', 'user_a');
  drafts.write('c1', 'On my way');
  drafts.write('c2', 'Call me');
  // Another conversation opened and this one again, or the page reloaded: as written.
  const reloaded = new Drafts('dsp_a', 'user_a');
  assert.equal(reloaded.text('c1'), 'On my way');
  assert.equal(reloaded.text('c2'), 'Call me');
  assert.equal(new Drafts('dsp_a', 'user_b').text('c1'), '');
  assert.equal(new Drafts('dsp_b', 'user_a').text('c1'), '');

  // Written on while it went: that stays.
  let changes = 0;
  const stop = reloaded.watch('c1', () => changes++);
  reloaded.write('c1', 'On my way\nAt the station in 10');
  reloaded.sent('c1', 'On my way');
  assert.equal(reloaded.text('c1'), 'At the station in 10');
  assert.equal(changes, 2);
  stop();
  // Changed while it went: all of it stays. Sent as it was: gone, from the tab too.
  reloaded.write('c2', 'Call me later');
  reloaded.sent('c2', 'Call me now');
  assert.equal(reloaded.text('c2'), 'Call me later');
  reloaded.sent('c2', 'Call me later');
  assert.equal(new Drafts('dsp_a', 'user_a').text('c2'), '');
});
