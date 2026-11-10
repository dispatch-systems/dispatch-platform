import assert from 'node:assert/strict';
import test from 'node:test';
import type {
  Conversation,
  ConversationMessages,
  ConversationWindow,
  Message,
} from '../../api/index.js';
import {
  ConversationReads,
  type ConversationViewport,
  type Held,
} from '../../frontend/conversations/reads.js';

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
  working: false,
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
            hidden: false,
          },
        ]
      : [],
  workdays: [],
  limits: { text: 1600, photos: 10, largest: 1, kinds: [] },
});

/** Existing fixtures exercise the legacy capability path. Preserve their transport gates and
 * assertions while answering the new opt-in capability read with the same actual snapshot. */
function legacyFetch(fetcher: typeof fetch) {
  globalThis.fetch = (async (input: string, options?: RequestInit) => {
    const url = new URL(input, 'https://dispatch.test');
    const window = url.pathname.endsWith('/window');
    const address = window ? url.pathname.slice(0, -'/window'.length) : input;
    const response = await fetcher(address, options);
    if (!window || !response.ok) return response;
    const value = (await response.json()) as ConversationMessages;
    return new Response(
      JSON.stringify({
        ...value,
        latest: value.messages,
        range: { hasBefore: value.more },
        origins: { ready: false },
      }),
    );
  }) as typeof fetch;
}

/** The conversations the page asks the server for, in turn, each answered once `answer` is
 * called. */
function server() {
  const asked: string[] = [];
  const waiting: (() => void)[] = [];
  legacyFetch(((url: string) => {
    const id = url.split('/').at(-1)!;
    asked.push(id);
    return new Promise<Response>((answered) =>
      waiting.push(() => answered(new Response(JSON.stringify(read(id))))),
    );
  }) as typeof fetch);
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
  const reads = new ConversationReads('America/Los_Angeles');
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
  legacyFetch(((url: string) => {
    asked.push(url);
    return Promise.resolve(
      failing
        ? new Response(JSON.stringify({ error: 'unavailable' }), { status: 503 })
        : new Response(JSON.stringify(url.includes('?') ? { ...latest, more: false } : latest)),
    );
  }) as typeof fetch);
  const reads = new ConversationReads('America/Los_Angeles');
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

test('older messages read in are held, but not as the latest read brought them', async () => {
  const at = Date.UTC(2026, 9, 26, 16);
  const message = (id: string, minutes: number) => ({
    id,
    ts: `${at + minutes * 60_000}:${id}`,
    direction: 'out' as const,
    text: id,
    at: at + minutes * 60_000,
    editedAt: null,
    inThread: false,
    sender: { userId: 'user_keisha', phoneId: null, name: 'Keisha Brown', code: null, photo: null },
    files: [],
    group: null,
    status: null,
    error: null,
    number: '+14255550199',
  });
  const latest = { ...read('tc_a'), more: true, messages: [message('m2', 2)] };
  const older = { ...read('tc_a'), more: false, messages: [message('m1', 1)] };
  legacyFetch(((url: string) =>
    Promise.resolve(
      new Response(JSON.stringify(url.includes('?') ? older : latest)),
    )) as typeof fetch);
  const reads = new ConversationReads('America/Los_Angeles');
  reads.open('tc_a');
  await new Promise((settled) => setImmediate(settled));
  await reads.readOlder('tc_a');
  const opened = reads.held('tc_a').opened!;
  assert.deepEqual(
    opened.messages.map((each) => each.id),
    ['m1', 'm2'],
  );
  assert.deepEqual(
    opened.lastRead.map((each) => each.id),
    ['m2'],
  );
});

test('closing deep history bounds the cache and opening it can read every older message again', async () => {
  const at = Date.UTC(2025, 0, 1, 12);
  const messages: Message[] = Array.from({ length: 400 }, (_, index) => ({
    id: `m${index}`,
    ts: `${at + index * 86_400_000}:m${index}`,
    direction: 'in',
    text: `message ${index}`,
    at: at + index * 86_400_000,
    editedAt: null,
    inThread: false,
    sender: null,
    files: [],
    gif: null,
    group: null,
    status: null,
    error: null,
    number: '+14255550199',
    moved: false,
  }));
  const page = (before: number): ConversationMessages => ({
    ...read('tc_a'),
    messages: messages.slice(Math.max(0, before - 100), before),
    more: before > 100,
    workdays: messages.slice(Math.max(0, before - 100), before).map((message) => ({
      day: new Date(message.at).toISOString().slice(0, 10),
      route: null,
      van: null,
    })),
  });
  legacyFetch(((address: string) => {
    const url = new URL(address, 'https://dispatch.test');
    const cursor = url.searchParams.get('before');
    const before = cursor
      ? messages.findIndex((message) => message.ts === cursor)
      : messages.length;
    assert.ok(before >= 0);
    return Promise.resolve(new Response(JSON.stringify(page(before))));
  }) as typeof fetch);
  const reads = new ConversationReads('America/Los_Angeles');
  const close = reads.watch('tc_a', () => undefined);
  reads.open('tc_a');
  await new Promise((settled) => setImmediate(settled));
  for (let page = 0; page < 3; page++) await reads.readOlder('tc_a');
  assert.equal(reads.held('tc_a').opened?.messages.length, 400);
  assert.equal(reads.held('tc_a').opened?.more, false);
  const retried = (index: number): Message => ({
    ...messages[index]!,
    direction: 'out',
    sender: { userId: 'user_dispatch', phoneId: null, name: 'Dispatch', code: null, photo: null },
    status: 'waiting',
  });
  messages[10] = retried(10);
  reads.show(read('tc_a').conversation, messages[10]);
  close();
  // A retry begun before closure can also answer after the cached window was trimmed.
  messages[20] = retried(20);
  reads.show(read('tc_a').conversation, messages[20]);
  const closed = reads.held('tc_a').opened!;
  assert.equal(closed.messages.length, 150);
  assert.equal(closed.messages[0]?.id, 'm250');
  assert.equal(closed.more, true);
  assert.equal(closed.workdays.length, 151);
  const closeAgain = reads.watch('tc_a', () => undefined);
  reads.open('tc_a');
  for (let page = 0; page < 3; page++) await reads.readOlder('tc_a');
  assert.deepEqual(
    reads.held('tc_a').opened?.messages.map((message) => message.id),
    messages.map((message) => message.id),
  );
  assert.equal(reads.held('tc_a').opened?.more, false);
  for (const index of [10, 20])
    assert.equal(
      reads.held('tc_a').opened?.messages.find((message) => message.id === `m${index}`)?.status,
      'waiting',
    );
  closeAgain();
  reads.dispose();
});

test('leaving the page aborts its reads and ignores a transport that still answers afterward', async () => {
  const waiting: { signal: AbortSignal; answer: () => void }[] = [];
  legacyFetch(
    ((_address: string, options?: RequestInit) =>
      new Promise<Response>((answer) => {
        waiting.push({
          signal: options!.signal!,
          answer: () => answer(new Response(JSON.stringify(read('tc_a')))),
        });
      })) as typeof fetch,
  );
  const reads = new ConversationReads('America/Los_Angeles');
  reads.open('tc_a');
  assert.equal(waiting.length, 1);
  reads.dispose();
  assert.equal(waiting[0]!.signal.aborted, true);
  waiting[0]!.answer();
  await new Promise((settled) => setImmediate(settled));
  assert.equal(reads.held('tc_a').opened, undefined);
});

test('server statuses win over delayed send snapshots on latest and older reads, while absent sends stay protected', async () => {
  const at = Date.UTC(2026, 9, 26, 16);
  const waiting: Message = {
    id: 'sent',
    ts: `${at}:sent`,
    direction: 'out',
    text: 'On my way',
    at,
    editedAt: null,
    inThread: false,
    sender: { userId: 'dispatcher', phoneId: null, name: 'Dispatch', code: null, photo: null },
    files: [],
    gif: null,
    group: null,
    status: 'waiting',
    error: null,
    number: '+14255550199',
    moved: false,
  };
  const head: Message = { ...waiting, id: 'later', ts: `${at + 60_000}:later`, at: at + 60_000 };
  for (const older of [false, true]) {
    for (const status of ['failed', 'delivered', null] as const) {
      const server = status
        ? { ...waiting, status, error: status === 'failed' ? 'No service' : null }
        : undefined;
      let answer: ((response: Response) => void) | undefined;
      let first = true;
      legacyFetch((() => {
        if (first) {
          first = false;
          return Promise.resolve(
            new Response(
              JSON.stringify({
                ...read('tc_a'),
                messages: older ? [head] : [waiting],
                more: older,
              }),
            ),
          );
        }
        return new Promise<Response>((resolve) => {
          answer = resolve;
        });
      }) as typeof fetch);
      const reads = new ConversationReads('America/Los_Angeles');
      const close = reads.watch('tc_a', () => undefined);
      reads.open('tc_a');
      await new Promise((settled) => setImmediate(settled));
      let reading: Promise<void> | undefined;
      if (older) reading = reads.readOlder('tc_a');
      else reads.change({ at: 1, ids: ['tc_a'] });
      assert.ok(answer, 'the read began before the delayed write response arrived');
      reads.show(read('tc_a').conversation, waiting);
      answer(
        new Response(
          JSON.stringify({
            ...read('tc_a'),
            messages: server ? [server] : [],
            more: false,
          }),
        ),
      );
      if (reading) await reading;
      else await new Promise((settled) => setImmediate(settled));
      const shown = reads.held('tc_a').opened?.messages.find((message) => message.id === 'sent');
      assert.equal(shown?.status, status ?? 'waiting');
      assert.equal(shown?.error, status === 'failed' ? 'No service' : null);
      close();
      reads.dispose();
    }
  }
});

/** Real API selection semantics: replacement rows with a separate latest observation. */
function boundedWindow(messages: Message[], start: number, all: Message[]): ConversationWindow {
  const neighbor = (row?: Message) =>
    row
      ? {
          id: row.id,
          ts: row.ts,
          at: row.at,
          direction: row.direction,
          sender: row.sender,
          groupId: row.group?.id ?? null,
          number: row.number,
        }
      : null;
  const context = {
    previous: neighbor(all[start - 1]),
    next: neighbor(all[start + messages.length]),
    sentBefore: null,
  };
  const selection = (rows: Message[]) =>
    rows[0]
      ? {
          origin: {
            id: rows[0].id,
            ts: rows[0].ts,
            at: rows[0].at,
            direction: rows[0].direction,
            sender: rows[0].sender,
            group: rows[0].group,
            number: rows[0].number,
          },
          beforeOrigin: neighbor(all[all.findIndex((row) => row.id === rows[0]!.id) - 1]),
          sentBeforeOrigin: null,
          firstDay: { id: rows[0].id, ts: rows[0].ts, at: rows[0].at, number: rows[0].number },
        }
      : null;
  const latest = all.slice(-50);
  return {
    ...read('tc_a'),
    messages,
    latest,
    context,
    latestContext: {
      previous: neighbor(all[all.length - latest.length - 1]),
      next: null,
      sentBefore: null,
    },
    latestWorkdays: [],
    origins: {
      ready: true,
      version: 1,
      algorithm: 'conversation-origins-1:2026a',
      timeZone: 'America/Los_Angeles',
      selected: selection(messages),
      latest: selection(latest),
    },
    range: {
      revision: '9007199254740993',
      first: messages[0]?.ts ?? null,
      last: messages.at(-1)?.ts ?? null,
      hasBefore: start > 0,
      hasAfter: start + messages.length < all.length,
      before: 'opaque-before',
      after: 'opaque-after',
      refresh: 'opaque-refresh',
      rebased: false,
    },
  };
}

const history = (): Message[] =>
  Array.from({ length: 400 }, (_, index) => ({
    id: `m${index}`,
    ts: `${1700000000000 + index * 86400000}:m${index}`,
    at: 1700000000000 + index * 86400000,
    direction: 'in',
    text: `message ${index}`,
    editedAt: null,
    inThread: false,
    sender: null,
    files: [],
    gif: null,
    group: null,
    status: null,
    error: null,
    number: '+14255550199',
    moved: false,
  }));

test('ready active windows replace in both directions, retain independent latest evidence, and old retries never become cursors', async () => {
  const all = history();
  const asked: URL[] = [];
  let failing = false;
  globalThis.fetch = ((address: string) => {
    const url = new URL(address, 'https://dispatch.test');
    asked.push(url);
    if (failing)
      return Promise.resolve(
        new Response(JSON.stringify({ error: 'unavailable' }), { status: 503 }),
      );
    assert.ok(url.pathname.endsWith('/window'));
    assert.equal(url.searchParams.get('origins'), '1');
    const anchor = url.searchParams.get('anchor');
    const index = anchor ? all.findIndex((row) => row.ts === anchor) : all.length;
    assert.ok(index >= 0);
    const start = Math.max(
      0,
      Math.min(
        all.length - 150,
        anchor ? index - Number(url.searchParams.get('leading')) : all.length - 150,
      ),
    );
    return Promise.resolve(
      new Response(JSON.stringify(boundedWindow(all.slice(start, start + 150), start, all))),
    );
  }) as typeof fetch;
  const reads = new ConversationReads('America/Los_Angeles');
  const close = reads.watch('tc_a', () => undefined);
  reads.open('tc_a');
  await new Promise((settled) => setImmediate(settled));
  let viewport: ConversationViewport = {
    anchor: 'm250',
    visible: ['m250'],
    latest: false,
    blocked: false,
  };
  reads.viewport('tc_a', () => viewport);
  const visited = new Set<string>();
  const observe = () => {
    const opened = reads.held('tc_a').opened!;
    assert.equal(opened.window!.messages.length, 150);
    assert.ok(opened.messages.length <= 150);
    assert.equal(opened.lastRead.length, 50);
    assert.equal(opened.lastRead[0]?.id, 'm350');
    assert.deepEqual(opened.workdays, []);
    for (const row of opened.window!.messages) visited.add(row.id);
    return opened;
  };
  let opened = observe();
  for (let step = 0; step < 5; step++) {
    viewport = {
      ...viewport,
      anchor: opened.window!.messages[0]!.id,
      visible: [opened.window!.messages[0]!.id],
    };
    await reads.readOlder('tc_a');
    opened = observe();
  }
  assert.equal(opened.more, false);
  assert.equal(visited.size, 400);
  // Retrying a sparse old row while reading a later window owns a receipt, never the range.
  const old = { ...all[10]!, direction: 'out' as const, status: 'waiting' as const };
  await reads.readLatest('tc_a');
  viewport = { ...viewport, anchor: 'm250', visible: ['m250'] };
  const observed = reads.version('tc_a');
  viewport = { ...viewport, blocked: true };
  reads.show(read('tc_a').conversation, old, observed);
  assert.equal(
    reads.held('tc_a').opened!.messages.some((row) => row.id === old.id),
    false,
  );
  viewport = { ...viewport, blocked: false };
  await reads.readOlder('tc_a');
  assert.equal(asked.at(-1)!.searchParams.get('anchor'), all[250]!.ts);
  opened = observe();
  // A forward failure retains the selection and requires the existing explicit retry action.
  const retained = opened.messages.map((row) => row.id);
  const edge = opened.window!.messages.at(-1)!;
  viewport = { ...viewport, anchor: edge.id, visible: [edge.id] };
  failing = true;
  await reads.readNewer('tc_a');
  assert.equal(reads.held('tc_a').olderFailed, true);
  assert.deepEqual(
    reads.held('tc_a').opened!.messages.map((row) => row.id),
    retained,
  );
  const failedAt = asked.length;
  reads.resume('tc_a');
  assert.equal(asked.length, failedAt);
  failing = false;
  await reads.readOlder('tc_a'); // Try again retries the failed forward intent.
  assert.equal(reads.held('tc_a').olderFailed, false);
  opened = observe();
  // Every newer shift also replaces, including a latest observation which is disconnected.
  for (let step = 0; step < 4 && opened.window!.range.hasAfter; step++) {
    const row = opened.window!.messages.at(-1)!;
    viewport = { ...viewport, anchor: row.id, visible: [row.id] };
    await reads.readNewer('tc_a');
    opened = observe();
  }
  assert.equal(opened.window!.range.hasAfter, false);
  // Closing history seeds only latest50, explicitly requiring a latest150 bootstrap on reopen.
  viewport = {
    ...viewport,
    anchor: opened.window!.messages[0]!.id,
    visible: [opened.window!.messages[0]!.id],
  };
  await reads.readOlder('tc_a');
  close();
  assert.deepEqual(
    reads.held('tc_a').opened!.messages.map((row) => row.id),
    all.slice(-50).map((row) => row.id),
  );
  assert.equal(reads.held('tc_a').opened!.provisional, true);
  const again = reads.watch('tc_a', () => undefined);
  reads.open('tc_a');
  await new Promise((settled) => setImmediate(settled));
  assert.equal(reads.held('tc_a').opened!.provisional, undefined);
  assert.equal(reads.held('tc_a').opened!.window!.messages[0]!.id, 'm250');
  again();
  reads.dispose();
});

test('a ready replacement is fenced by current interactions and navigation, and delayed receipts cannot regress authoritative status or moved', async () => {
  const row: Message = {
    id: 'm',
    ts: '1700000000000:m',
    at: 1700000000000,
    direction: 'out',
    text: 'Hi',
    editedAt: null,
    inThread: false,
    sender: null,
    files: [],
    gif: null,
    group: null,
    status: 'waiting',
    error: null,
    number: '+14255550199',
    moved: false,
  };
  let answer: ((value: Response) => void) | undefined;
  let first = true;
  let signal: AbortSignal | undefined;
  globalThis.fetch = ((_url: string, options?: RequestInit) => {
    if (first) {
      first = false;
      return Promise.resolve(new Response(JSON.stringify(boundedWindow([row], 0, [row]))));
    }
    signal = options!.signal!;
    return new Promise<Response>((resolve) => {
      answer = resolve;
    });
  }) as typeof fetch;
  const reads = new ConversationReads('America/Los_Angeles');
  const stop = reads.watch('tc_a', () => undefined);
  reads.open('tc_a');
  await new Promise((settled) => setImmediate(settled));
  let blocked = false;
  reads.viewport('tc_a', () => ({ anchor: 'm', visible: ['m'], latest: true, blocked }));
  const receipt = reads.version('tc_a');
  reads.change({ at: 1, ids: ['tc_a'] });
  blocked = true;
  answer!(new Response(JSON.stringify(boundedWindow([{ ...row, text: 'discarded' }], 0, [row]))));
  await new Promise((settled) => setImmediate(settled));
  assert.equal(reads.held('tc_a').opened!.messages[0]!.text, 'Hi');
  blocked = false;
  reads.resume('tc_a');
  const current = { ...row, status: 'failed' as const, error: 'No service', moved: true };
  answer!(new Response(JSON.stringify(boundedWindow([current], 0, [current]))));
  await new Promise((settled) => setImmediate(settled));
  reads.show(read('tc_a').conversation, row, receipt);
  assert.equal(reads.held('tc_a').opened!.messages[0]!.status, 'failed');
  assert.equal(reads.held('tc_a').opened!.messages[0]!.moved, true);
  assert.equal(reads.held('tc_a').opened!.messages[0]!.error, 'No service');
  assert.ok(answer);
  stop();
  assert.equal(signal!.aborted, true);
  answer!(new Response(JSON.stringify(boundedWindow([{ ...row, text: 'late' }], 0, [row]))));
  await new Promise((settled) => setImmediate(settled));
  assert.equal(reads.held('tc_a').opened!.messages[0]!.text, 'Hi');
  reads.dispose();
});

test('origin capability transitions preserve the current legacy viewport and re-probe on owned updates', async () => {
  const all = history();
  let ready = false;
  const asked: URL[] = [];
  globalThis.fetch = ((address: string) => {
    const url = new URL(address, 'https://dispatch.test');
    asked.push(url);
    if (!url.pathname.endsWith('/window')) {
      const before = url.searchParams.get('before');
      const end = before ? all.findIndex((row) => row.ts === before) : all.length;
      return Promise.resolve(
        new Response(
          JSON.stringify({
            ...read('tc_a'),
            messages: all.slice(Math.max(0, end - 50), end),
            more: end > 50,
          }),
        ),
      );
    }
    const anchor = url.searchParams.get('anchor');
    const index = anchor ? all.findIndex((row) => row.ts === anchor) : all.length;
    const start = Math.max(
      0,
      Math.min(250, anchor ? index - Number(url.searchParams.get('leading')) : 250),
    );
    const value = boundedWindow(all.slice(start, start + 150), start, all);
    if (!ready) value.origins = { ...value.origins!, ready: false, selected: null, latest: null };
    return Promise.resolve(new Response(JSON.stringify(value)));
  }) as typeof fetch;
  const reads = new ConversationReads('America/Los_Angeles');
  const stop = reads.watch('tc_a', () => undefined);
  reads.open('tc_a');
  await new Promise((settled) => setImmediate(settled));
  assert.equal(reads.held('tc_a').opened!.window, undefined);
  for (let step = 0; step < 4; step++) await reads.readOlder('tc_a');
  reads.viewport('tc_a', () => ({
    anchor: 'm120',
    visible: ['m120'],
    latest: false,
    blocked: false,
  }));
  ready = true;
  reads.change({ at: 1, ids: ['tc_a'] });
  await new Promise((settled) => setImmediate(settled));
  const opened = reads.held('tc_a').opened!;
  assert.equal(asked.at(-1)!.searchParams.get('anchor'), all[120]!.ts);
  assert.equal(opened.window!.range.hasAfter, true);
  assert.ok(opened.messages.some((row) => row.id === 'm120'));
  assert.equal(opened.messages.length, 150);
  assert.equal(opened.lastRead.at(-1)!.id, 'm399');
  ready = false;
  reads.change({ at: 2, ids: ['tc_a'] });
  await new Promise((settled) => setImmediate(settled));
  assert.equal(reads.held('tc_a').opened!.window, undefined);
  assert.ok(reads.held('tc_a').opened!.messages.some((row) => row.id === 'm120'));
  stop();
  reads.dispose();
});

/** Requests are deliberately allowed to answer after abort, to exercise publication ownership. */
function gatedReads() {
  type Request = {
    url: URL;
    signal: AbortSignal;
    answer: (value: ConversationMessages | ConversationWindow) => void;
  };
  const queued: Request[] = [];
  const readers: ((request: Request) => void)[] = [];
  globalThis.fetch = ((address: string, options?: RequestInit) =>
    new Promise<Response>((resolve) => {
      const request: Request = {
        url: new URL(address, 'https://dispatch.test'),
        signal: options!.signal!,
        answer: (value) => resolve(new Response(JSON.stringify(value))),
      };
      const reader = readers.shift();
      if (reader) reader(request);
      else queued.push(request);
    })) as typeof fetch;
  return () => {
    const request = queued.shift();
    return request
      ? Promise.resolve(request)
      : new Promise<Request>((resolve) => readers.push(resolve));
  };
}

function published(reads: ConversationReads, predicate: (held: Held) => boolean) {
  return new Promise<Held>((resolve) => {
    const stop = reads.watch('tc_a', () => {
      const held = reads.held('tc_a');
      if (predicate(held)) {
        stop();
        resolve(held);
      }
    });
  });
}
const legacyWindow = (all: Message[], start = 250) => {
  const window = boundedWindow(all.slice(start, start + 150), start, all);
  return { ...window, origins: { ...window.origins!, ready: false, selected: null, latest: null } };
};

test('superseding legacy older ownership releases busy before and after readiness, and its late finalizer cannot release the next owner', async () => {
  for (const ready of [false, true]) {
    const all = history();
    const next = gatedReads();
    const reads = new ConversationReads('America/Los_Angeles');
    const close = reads.watch('tc_a', () => undefined);
    const opened = published(reads, (held) => Boolean(held.opened));
    reads.open('tc_a');
    (await next()).answer(legacyWindow(all));
    await opened;
    const oldRead = reads.readOlder('tc_a');
    const obsolete = await next();
    assert.equal(reads.held('tc_a').older, true);
    const previous = reads.held('tc_a').opened;
    const refreshed = published(
      reads,
      (held) => held.opened !== undefined && held.opened !== previous && !held.older,
    );
    reads.change({ at: 1, ids: ['tc_a'] });
    const refresh = await next();
    assert.equal(obsolete.signal.aborted, true);
    assert.equal(
      reads.held('tc_a').older,
      false,
      'the superseding refresh relinquishes the old busy state immediately',
    );
    refresh.answer(ready ? boundedWindow(all.slice(250), 250, all) : legacyWindow(all));
    await refreshed;
    const newRead = reads.readOlder('tc_a');
    const owner = await next();
    assert.equal(reads.held('tc_a').older, true);
    obsolete.answer({ ...read('tc_a'), messages: all.slice(200, 250), more: true });
    await oldRead;
    assert.equal(
      reads.held('tc_a').older,
      true,
      'an obsolete finalizer cannot release the new owner',
    );
    assert.equal(reads.held('tc_a').opened!.messages[0]!.id, 'm250');
    owner.answer(
      ready
        ? boundedWindow(all.slice(200, 350), 200, all)
        : { ...read('tc_a'), messages: all.slice(200, 250), more: true },
    );
    await newRead;
    assert.equal(reads.held('tc_a').older, false);
    assert.equal(reads.held('tc_a').opened!.messages[0]!.id, 'm200');
    // Another page remains traversable after both superseded transports have settled.
    const another = reads.readOlder('tc_a');
    (await next()).answer(
      ready
        ? boundedWindow(all.slice(150, 300), 150, all)
        : { ...read('tc_a'), messages: all.slice(150, 200), more: true },
    );
    await another;
    assert.equal(reads.held('tc_a').opened!.messages[0]!.id, 'm150');
    close();
    reads.dispose();
  }
});

test('a legacy page completed after refresh dispatch survives fallback publication together with its current viewport and days', async () => {
  const all = history();
  const next = gatedReads();
  const reads = new ConversationReads('America/Los_Angeles');
  const close = reads.watch('tc_a', () => undefined);
  const opened = published(reads, (held) => Boolean(held.opened));
  reads.open('tc_a');
  (await next()).answer(legacyWindow(all));
  await opened;
  let view: ConversationViewport = {
    anchor: 'm300',
    visible: ['m300', 'm301'],
    latest: false,
    blocked: false,
    serial: 1,
  };
  reads.viewport('tc_a', () => view);
  reads.change({ at: 1, ids: ['tc_a'] });
  (await next()).answer(legacyWindow(all));
  const fallback = await next();
  assert.equal(fallback.url.pathname.endsWith('/window'), false);
  const older = reads.readOlder('tc_a');
  (await next()).answer({
    ...read('tc_a'),
    messages: all.slice(200, 250),
    more: true,
    workdays: [{ day: '2024-06-01', route: null, van: null }],
  });
  await older;
  view = { ...view, anchor: 'm200', visible: ['m200', 'm201'], serial: 2 };
  const shown = published(reads, (held) => held.opened?.lastRead[0]?.id === 'm350' && !held.older);
  fallback.answer({
    ...read('tc_a'),
    messages: all.slice(350).filter((row) => row.id !== 'm380'),
    more: true,
  });
  await shown;
  const current = reads.held('tc_a').opened!;
  assert.equal(current.messages[0]!.id, 'm200');
  assert.ok(current.messages.some((row) => row.id === view.anchor));
  assert.ok(current.messages.some((row) => row.id === 'm201'));
  assert.equal(
    current.messages.some((row) => row.id === 'm380'),
    false,
    'the refreshed covered range still removes authoritative omissions',
  );
  assert.ok(current.workdays.some((day) => day.day === '2024-06-01'));
  assert.equal(current.lastRead.length, 49);
  close();
  reads.dispose();
});

test('acknowledged retries replace cached failures while interaction defers reads, and only returned later rows outrank a delayed receipt', async () => {
  for (const status of ['waiting', 'delivered'] as const) {
    const all = history();
    const failed: Message = {
      ...all[399]!,
      direction: 'out',
      status: 'failed',
      error: 'No service',
    };
    all[399] = failed;
    const next = gatedReads();
    const reads = new ConversationReads('America/Los_Angeles');
    const close = reads.watch('tc_a', () => undefined);
    const opened = published(reads, (held) => Boolean(held.opened));
    reads.open('tc_a');
    (await next()).answer(boundedWindow(all.slice(250), 250, all));
    await opened;
    let blocked = true; // The Retry control still owns focus at acknowledgement.
    reads.viewport('tc_a', () => ({
      anchor: failed.id,
      visible: [failed.id],
      latest: true,
      blocked,
    }));
    const observed = reads.version('tc_a');
    reads.show(read('tc_a').conversation, { ...failed, status, error: null }, observed);
    const receipt = reads.held('tc_a').opened!.messages.at(-1)!;
    assert.equal(receipt.status, status);
    assert.equal(receipt.error, null);
    assert.equal(
      reads.held('tc_a').opened!.window!.messages.at(-1)!.status,
      'failed',
      'the receipt must outrank this pre-write cache',
    );
    blocked = false;
    reads.resume('tc_a');
    const authoritative = {
      ...failed,
      status: 'failed' as const,
      error: 'Authoritative later failure',
      moved: true,
    };
    all[399] = authoritative;
    const shown = published(
      reads,
      (held) => held.opened?.messages.at(-1)?.error === authoritative.error,
    );
    (await next()).answer(boundedWindow(all.slice(250), 250, all));
    await shown;
    assert.equal(reads.held('tc_a').opened!.messages.at(-1)!.moved, true);
    reads.show(read('tc_a').conversation, { ...failed, status, error: null }, observed);
    assert.equal(reads.held('tc_a').opened!.messages.at(-1)!.error, authoritative.error);
    close();
    reads.dispose();
  }
  // An unrelated legacy refresh retains an old failed row; retaining it isn't a new observation.
  const all = history();
  all[250] = { ...all[250]!, direction: 'out', status: 'failed', error: 'Cached failure' };
  const next = gatedReads();
  const reads = new ConversationReads('America/Los_Angeles');
  const close = reads.watch('tc_a', () => undefined);
  const opened = published(reads, (held) => Boolean(held.opened));
  reads.open('tc_a');
  (await next()).answer(legacyWindow(all));
  await opened;
  const observed = reads.version('tc_a');
  reads.change({ at: 1, ids: ['tc_a'] });
  const refreshed = published(reads, (held) => held.opened?.lastRead[0]?.id === 'm350');
  const latest = legacyWindow(all, 350);
  (await next()).answer(latest);
  await refreshed;
  reads.show(read('tc_a').conversation, { ...all[250]!, status: 'waiting', error: null }, observed);
  assert.equal(
    reads.held('tc_a').opened!.messages.find((row) => row.id === 'm250')!.status,
    'waiting',
  );
  close();
  reads.dispose();
});

test('legacy400 conversion retains first-visible250 and its suffix, and fallback follows a viewport moved during its extra awaits', async () => {
  const all = history();
  const next = gatedReads();
  const reads = new ConversationReads('America/Los_Angeles');
  const close = reads.watch('tc_a', () => undefined);
  const opened = published(reads, (held) => Boolean(held.opened));
  reads.open('tc_a');
  (await next()).answer(legacyWindow(all));
  await opened;
  for (let start = 200; start >= 0; start -= 50) {
    const older = reads.readOlder('tc_a');
    (await next()).answer({
      ...read('tc_a'),
      messages: all.slice(start, start + 50),
      more: start > 0,
    });
    await older;
  }
  assert.equal(reads.held('tc_a').opened!.messages.length, 400);
  let view: ConversationViewport = {
    anchor: 'm250',
    visible: all.slice(250, 271).map((row) => row.id),
    latest: false,
    blocked: false,
    serial: 1,
  };
  reads.viewport('tc_a', () => view);
  reads.change({ at: 1, ids: ['tc_a'] });
  const conversion = await next();
  assert.equal(conversion.url.searchParams.get('anchor'), all[250]!.ts);
  const leading = Number(conversion.url.searchParams.get('leading'));
  assert.ok(leading <= 129, 'the 21 visible rows need room after the anchor');
  const converted = published(reads, (held) => Boolean(held.opened?.window));
  const start = Math.max(0, Math.min(250, 250 - leading));
  conversion.answer(boundedWindow(all.slice(start, start + 150), start, all));
  await converted;
  assert.ok(
    view.visible.every((id) => reads.held('tc_a').opened!.messages.some((row) => row.id === id)),
  );
  assert.equal(reads.held('tc_a').opened!.messages.length, 150);
  assert.equal(reads.held('tc_a').opened!.lastRead.at(-1)!.id, 'm399');
  const latestRead = reads.readLatest('tc_a');
  (await next()).answer(boundedWindow(all.slice(250), 250, all));
  await latestRead;
  view = { ...view, anchor: 'm300', visible: ['m300', 'm301'], serial: 2 };
  reads.change({ at: 2, ids: ['tc_a'] });
  (await next()).answer(legacyWindow(all));
  const latest = await next();
  view = { ...view, anchor: 'm260', visible: ['m260', 'm261'], serial: 3 };
  latest.answer({ ...read('tc_a'), messages: all.slice(350), more: true });
  const page = await next();
  view = { ...view, anchor: 'm250', visible: ['m250', 'm251'], serial: 4 };
  page.answer({ ...read('tc_a'), messages: all.slice(300, 350), more: true });
  const restored = published(reads, (held) => Boolean(held.opened && !held.opened.window));
  const progression = await Promise.race([
    next().then((request) => ({ request })),
    restored.then(() => ({ request: undefined })),
  ]);
  assert.ok(progression.request, 'fallback must read through the current anchor before publishing');
  progression.request.answer({ ...read('tc_a'), messages: all.slice(250, 300), more: true });
  await restored;
  assert.equal(reads.held('tc_a').opened!.messages[0]!.id, 'm250');
  assert.ok(
    view.visible.every((id) => reads.held('tc_a').opened!.messages.some((row) => row.id === id)),
  );
  assert.equal(reads.held('tc_a').opened!.lastRead[0]!.id, 'm350');
  close();
  reads.dispose();
});

test('a snapshot issued before Retry began cannot replay its failed row over the acknowledgement', async () => {
  const all = history();
  const failed: Message = {
    ...all[399]!,
    direction: 'out',
    status: 'failed',
    error: 'Cached failure',
  };
  all[399] = failed;
  const next = gatedReads();
  const reads = new ConversationReads('America/Los_Angeles');
  const close = reads.watch('tc_a', () => undefined);
  const opened = published(reads, (held) => Boolean(held.opened));
  reads.open('tc_a');
  (await next()).answer(boundedWindow(all.slice(250), 250, all));
  await opened;
  reads.change({ at: 1, ids: ['tc_a'] });
  const preWrite = await next();
  const observed = reads.version('tc_a');
  reads.show(read('tc_a').conversation, { ...failed, status: 'waiting', error: null }, observed);
  const previous = reads.held('tc_a').opened!.window;
  const accepted = published(reads, (held) => held.opened?.window !== previous);
  preWrite.answer(boundedWindow(all.slice(250), 250, all));
  await accepted;
  assert.equal(reads.held('tc_a').opened!.messages.at(-1)!.status, 'waiting');
  assert.equal(reads.held('tc_a').opened!.messages.at(-1)!.error, null);
  const authoritative = await next(); // Coalesced read after acknowledgement, not a second invalidation.
  all[399] = { ...failed, status: 'delivered', error: null };
  const delivered = published(
    reads,
    (held) => held.opened?.messages.at(-1)?.status === 'delivered',
  );
  authoritative.answer(boundedWindow(all.slice(250), 250, all));
  await delivered;
  close();
  reads.dispose();
});
