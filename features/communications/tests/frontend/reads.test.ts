import assert from 'node:assert/strict';
import test from 'node:test';
import type { Conversation, ConversationMessages } from '../../api/index.js';
import { ConversationReads } from '../../frontend/conversations/reads.js';

/** A conversation by text, `id`, as its read answers it. */
const read = (id: string): ConversationMessages => ({
  conversation: {
    id,
    kind: 'number',
    teamId: null,
    number: '+14255550199',
    driverId: null,
    contact: false,
    blocked: false,
    with: { userId: null, name: id, code: null, role: null, title: null, photo: null },
    state: 'responded',
    waitingSince: null,
    lastAt: null,
    last: null,
    matched: [],
    groupId: null,
  } satisfies Conversation,
  messages: [],
  more: false,
  today: null,
  elsewhere: [],
  workdays: [],
  limits: { text: 1600, photos: 10, largest: 1, kinds: [] },
});

/** The conversations the page asks the server for, in turn, each answered once `answer` is
 * called. */
function server() {
  const asked: string[] = [];
  const waiting: (() => void)[] = [];
  globalThis.fetch = ((url: string) => {
    const id = url.split('/').at(-1)!;
    asked.push(id);
    return new Promise<Response>((answered) =>
      waiting.push(() => answered(new Response(JSON.stringify(read(id))))),
    );
  }) as typeof fetch;
  /** Answers every read asked so far, and lets what follows from each run. */
  const answer = async () => {
    for (const each of waiting.splice(0)) each();
    for (let turn = 0; turn < 20; turn++) await Promise.resolve();
    await new Promise((settled) => setImmediate(settled));
  };
  return { asked, answer };
}

test('a conversation read opens again as read until an update changes it; the one open is read again at once as one does, and once more if one came while it was read', async () => {
  const { asked, answer } = server();
  const reads = new ConversationReads();
  reads.change({ at: 1, ids: 'all' });
  reads.open('tc_a');
  reads.open('tc_b');
  await answer();
  assert.deepEqual(asked, ['tc_a', 'tc_b']);
  assert.equal(reads.held('tc_a').opened?.id, 'tc_a');

  // Opened again, nothing changed: as held, unread.
  reads.open('tc_a');
  // An update naming another, or one not held, changes nothing held.
  reads.change({ at: 2, ids: ['tc_b', 'tc_z'] });
  reads.open('tc_a');
  assert.deepEqual(asked, ['tc_a', 'tc_b']);
  // Changed while closed: read as it opens, shown as held until then.
  reads.open('tc_b');
  assert.deepEqual(asked, ['tc_a', 'tc_b', 'tc_b']);
  assert.equal(reads.held('tc_b').opened?.id, 'tc_b');
  await answer();

  // Open, it's read again as an update names it, and once more for one that came meanwhile.
  const stop = reads.watch('tc_a', () => undefined);
  reads.change({ at: 3, ids: ['tc_a'] });
  reads.change({ at: 4, ids: ['tc_a'] });
  assert.deepEqual(asked.slice(3), ['tc_a']);
  await answer();
  assert.deepEqual(asked.slice(3), ['tc_a', 'tc_a']);
  await answer();
  // Reading afresh changes every one, but only the one open is read.
  reads.change({ at: 5, ids: 'all' });
  assert.deepEqual(asked.slice(3), ['tc_a', 'tc_a', 'tc_a']);
  await answer();
  stop();
});
