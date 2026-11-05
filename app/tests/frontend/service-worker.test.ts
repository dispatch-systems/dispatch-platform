import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

// What the service worker makes of a push, as browsers ask: every push shows a notification, and
// one arriving while a window of Dispatch is in front, which alerts itself, is shown silently and
// closed at once, or left to the page's own of it.

type Shown = { title: string; tag: string; silent: boolean; closed: boolean };

/**
 * The worker, run in a stand-in for its global scope with the windows `windows` open and the
 * notifications `shown` showing; what each push does to them.
 */
function worker(windows: { focused: boolean; visibilityState: string }[], shown: Shown[] = []) {
  const listeners = new Map<string, (event: unknown) => void>();
  const self = {
    addEventListener: (type: string, listener: (event: unknown) => void) =>
      listeners.set(type, listener),
    clients: { matchAll: async () => windows, claim: async () => {} },
    registration: {
      showNotification: async (title: string, options: { tag: string; silent: boolean }) => {
        // One with the tag of a notification shown takes its place.
        shown.splice(0, shown.length, ...shown.filter((each) => each.tag !== options.tag));
        shown.push({ title, tag: options.tag, silent: options.silent, closed: false });
      },
      getNotifications: async ({ tag }: { tag: string }) =>
        shown
          .filter((each) => each.tag === tag && !each.closed)
          .map((each) => ({ close: () => (each.closed = true) })),
    },
  };
  vm.runInNewContext(fs.readFileSync('app/frontend/public/service-worker.js', 'utf8'), { self });
  /** Delivers a push of a message tagged `tag`, until the worker is done with it. */
  const push = async (tag: string) => {
    let done: Promise<unknown> = Promise.resolve();
    listeners.get('push')!({
      data: { json: () => ({ title: 'Casey Rivera', body: 'Where do I park?', url: '/', tag }) },
      waitUntil: (work: Promise<unknown>) => (done = work),
    });
    await done;
  };
  return { shown, push };
}

test('a push shows its notification while no window of Dispatch is in front', async () => {
  const { shown, push } = worker([{ focused: false, visibilityState: 'hidden' }]);
  await push('message-1');
  assert.deepEqual(shown, [
    { title: 'Casey Rivera', tag: 'message-1', silent: false, closed: false },
  ]);
});

test('a push while Dispatch is in front shows silently and closes at once', async () => {
  const { shown, push } = worker([{ focused: true, visibilityState: 'visible' }]);
  await push('message-1');
  assert.deepEqual(shown, [
    { title: 'Casey Rivera', tag: 'message-1', silent: true, closed: true },
  ]);
});

test("a push while Dispatch is in front leaves the page's own notification of it showing", async () => {
  const page = { title: 'Casey Rivera', tag: 'message-1', silent: false, closed: false };
  const { shown, push } = worker([{ focused: true, visibilityState: 'visible' }], [{ ...page }]);
  await push('message-1');
  assert.deepEqual(shown, [page]);
});
