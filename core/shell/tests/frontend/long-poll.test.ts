import test from 'node:test';
import assert from 'node:assert/strict';
import { longPoll } from '../../frontend/runtime/long-poll.js';
import { ApiError } from '../../frontend/runtime/api.js';

function page(t: test.TestContext) {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  for (const [key, value] of Object.entries({
    document: { hidden: false },
    navigator: { onLine: true },
  })) {
    const before = Object.getOwnPropertyDescriptor(globalThis, key);
    Object.defineProperty(globalThis, key, { configurable: true, value });
    t.after(() => {
      if (before) Object.defineProperty(globalThis, key, before);
      else Reflect.deleteProperty(globalThis, key);
    });
  }
}

function deferred() {
  let resolve!: () => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<void>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

test('pausing a live read lets its replacement run and ignores the old failure', async (t) => {
  page(t);
  const requests: { signal: AbortSignal; done: ReturnType<typeof deferred> }[] = [];
  const poll = longPoll(
    (signal) => {
      const done = deferred();
      requests.push({ signal, done });
      return done.promise;
    },
    () => assert.fail('an aborted request reported a failure'),
  );
  t.after(() => poll.dispose());
  const first = poll.listen();
  await poll.listen();
  assert.equal(requests.length, 1);
  poll.pause();
  assert.equal(requests[0]!.signal.aborted, true);
  const second = poll.listen();
  requests[0]!.done.reject(new Error('late failure'));
  await first;
  // The old request finishing must neither clear the replacement nor schedule another read.
  t.mock.timers.tick(30_000);
  await poll.listen();
  assert.equal(requests.length, 2);
  requests[1]!.done.resolve();
  await second;
  poll.dispose();
  t.mock.timers.tick(30_000);
  await poll.listen();
  assert.equal(requests.length, 2);
});

test('transport failures retry, success resets the failure streak, and access refusals stop', async (t) => {
  page(t);
  const failures: [boolean, boolean][] = [];
  let response: unknown = new Error('offline');
  let reads = 0;
  const poll = longPoll(
    async () => {
      reads++;
      if (response) throw response;
    },
    (_cause, stopped, first) => failures.push([stopped, first]),
  );
  t.after(() => poll.dispose());
  await poll.listen();
  t.mock.timers.tick(1000);
  // Let the retry finish without starting a second read.
  await poll.listen();
  assert.equal(reads, 2);
  assert.deepEqual(failures, [
    [false, true],
    [false, false],
  ]);
  poll.pause();
  response = undefined;
  await poll.listen();
  poll.pause();
  response = new ApiError('forbidden', 'Forbidden', 403);
  await poll.listen();
  assert.deepEqual(failures.at(-1), [true, true]);
  const stoppedAt = reads;
  t.mock.timers.tick(30_000);
  assert.equal(reads, stoppedAt);
});
