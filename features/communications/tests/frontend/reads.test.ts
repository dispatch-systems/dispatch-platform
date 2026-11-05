import assert from 'node:assert/strict';
import test from 'node:test';
import type { Conversation, ConversationMessages } from '../../api/index.js';
import { ConversationReads } from '../../frontend/conversations/reads.js';

/** A conversation by text, `id`, as its read answers it: `tc_c`'s with whom it's with's work
 * conversation, `tc_w`, beside it. */
const read = (id: string): ConversationMessages => ({
  conversation: {
    id,
    kind: 'number',
    teamId: null,
    number: '+14255550199',
    driverId: null,
    contact: false,
    added: false,
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
  elsewhere:
    id === 'tc_c'
      ? [
          {
            kind: 'work',
            conversationId: 'tc_w',
            waiting: false,
            number: '+14255550131',
            driverId: 'D1',
            teamId: null,
            userId: null,
          },
        ]
      : [],
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

test('a conversation read opens again as read until an update changes it, or one of whom it’s with’s others it shows; the one open is read again at once as one does, and once more if one came while it was read', async () => {
  const { asked, answer } = server();
  const reads = new ConversationReads();
  reads.change({ at: 1, ids: 'all' });
  reads.open('tc_a');
  reads.open('tc_b');
  reads.open('tc_c');
  await answer();
  assert.deepEqual(asked.splice(2), ['tc_c']);
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
  // Their work conversation changed, as its phone did: what it shows of it is read again.
  reads.change({ at: 3, ids: ['tc_w'] });
  reads.open('tc_c');
  assert.deepEqual(asked.splice(3), ['tc_c']);
  await answer();

  // Open, it's read again as an update names it, and once more for one that came meanwhile.
  const stop = reads.watch('tc_a', () => undefined);
  reads.change({ at: 4, ids: ['tc_a'] });
  reads.change({ at: 5, ids: ['tc_a'] });
  assert.deepEqual(asked.slice(3), ['tc_a']);
  await answer();
  assert.deepEqual(asked.slice(3), ['tc_a', 'tc_a']);
  await answer();
  // Reading afresh changes every one, but only the one open is read.
  reads.change({ at: 6, ids: 'all' });
  assert.deepEqual(asked.slice(3), ['tc_a', 'tc_a', 'tc_a']);
  await answer();
  stop();
});

test('a read of older ones that failed is held as failed until it’s asked again, which reads them', async () => {
  const at = Date.UTC(2026, 9, 26, 16);
  const latest = {
    ...read('tc_a'),
    more: true,
    messages: [
      {
        id: 'm1',
        ts: `${at}:m1`,
        direction: 'in' as const,
        text: 'Running late',
        at,
        editedAt: null,
        inThread: false,
        sender: null,
        files: [],
        group: null,
        status: null,
        number: '+14255550199',
      },
    ],
  };
  const asked: string[] = [];
  let failing = false;
  globalThis.fetch = ((url: string) => {
    asked.push(url);
    return Promise.resolve(
      failing
        ? new Response(JSON.stringify({ error: 'unavailable' }), { status: 503 })
        : new Response(JSON.stringify(url.includes('?') ? { ...latest, more: false } : latest)),
    );
  }) as typeof fetch;
  const reads = new ConversationReads();
  reads.open('tc_a');
  await new Promise((settled) => setImmediate(settled));
  failing = true;
  await reads.readOlder('tc_a');
  assert.equal(reads.held('tc_a').olderFailed, true);
  assert.equal(reads.held('tc_a').older, false);
  assert.equal(asked.length, 2);
  failing = false;
  await reads.readOlder('tc_a');
  assert.equal(asked.length, 3);
  assert.equal(reads.held('tc_a').olderFailed, false);
  assert.equal(reads.held('tc_a').opened?.more, false);
});
