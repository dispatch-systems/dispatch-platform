import assert from 'node:assert/strict';
import test from 'node:test';
import type { WaitingMessage } from '../../api/index.js';
import { arrived, SEEN_MS, type InFront, type Seen } from '../../frontend/alerts/arrivals.js';

/** The message `id` waiting in the conversation `conversation`, listed under `group`, if any. */
const waits = (conversation: string, id: string, group: string | null = null): WaitingMessage => ({
  conversationId: conversation,
  groupId: group,
  from: 'Jordan Ellis',
  message: {
    id,
    ts: '1761400000.000100',
    direction: 'in',
    text: 'Running late',
    at: 0,
    editedAt: null,
    inThread: false,
    sender: null,
    files: [],
    group: null,
  },
});

/**
 * The member's tabs at a DSP, sharing what they saw as the browser keeps it: `looks` is the tab
 * alerting looking at what waits, as its own copy has it, at `now`, answering what alerts.
 */
function browser() {
  let seen: Seen | undefined;
  return (latest: WaitingMessage[], now: number, inFront?: InFront) => {
    const looked = arrived(latest, seen, now, inFront);
    seen = looked.seen;
    return looked.fresh.map((each) => each.message.id);
  };
}

test('a new message from an employee alerts once, whichever tab looks, and none from before', () => {
  const looks = browser();
  // What waits as Dispatch opens alerts no one.
  assert.deepEqual(looks([waits('jordan', 'm1')], 0), []);
  // A conversation that starts waiting, then another message in one waiting, each alert once.
  assert.deepEqual(looks([waits('jordan', 'm1'), waits('casey', 'm2')], 5_000), ['m2']);
  assert.deepEqual(looks([waits('jordan', 'm1'), waits('casey', 'm2')], 10_000), []);
  assert.deepEqual(looks([waits('jordan', 'm3'), waits('casey', 'm2')], 15_000), ['m3']);
  // Another tab, alerting now, reads an older copy of what waits, then a newer: neither alerts.
  assert.deepEqual(looks([waits('jordan', 'm1'), waits('casey', 'm2')], 20_000), []);
  assert.deepEqual(looks([waits('jordan', 'm3'), waits('casey', 'm2')], 25_000), []);
  // Answered, Casey's goes; written to again, the new message alerts.
  assert.deepEqual(looks([waits('jordan', 'm3')], 30_000), []);
  assert.deepEqual(looks([waits('jordan', 'm3'), waits('casey', 'm4')], 35_000), ['m4']);
  // With every tab closed a while, what came meanwhile is from before Dispatch opened again.
  const reopened = 35_000 + SEEN_MS + 1;
  assert.deepEqual(looks([waits('jordan', 'm3'), waits('casey', 'm5')], reopened), []);
});

test('a message in front of the member alerts no one, then or later', () => {
  const looks = browser();
  looks([], 0);
  // The conversation open on the page, or the group it's listed under.
  assert.deepEqual(looks([waits('jordan', 'm1')], 5_000, { conversation: 'jordan' }), []);
  const grouped = [waits('jordan', 'm1'), waits('casey', 'm2', 'drivers')];
  assert.deepEqual(looks(grouped, 10_000, { group: 'drivers' }), []);
  assert.deepEqual(looks(grouped, 15_000), []);
  // Another conversation's still does.
  assert.deepEqual(looks([...grouped, waits('sam', 'm3')], 20_000, { group: 'drivers' }), ['m3']);
});
