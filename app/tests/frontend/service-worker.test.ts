import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

// What the service worker makes of a push, as browsers ask: every push shows a notification, and
// one a window of Dispatch in front says it alerts of itself is shown silently and closed at once,
// or left to the page's own notification of it. A window that says otherwise, or nothing in time,
// leaves it shown.

type Shown = { title: string; tag: string; silent: boolean; closed: boolean };
/** A window open: in front or not, and what it answers when asked about a push, if anything. */
type Window = { focused: boolean; answers?: boolean };

/**
 * The worker, run in a stand-in for its global scope with `windows` open and the notifications
 * `shown` showing; what each push does to them. A window that answers nothing is waited for only
 * until the worker gives up on it, at once here.
 */
function worker(windows: Window[], shown: Shown[] = []) {
  const listeners = new Map<string, (event: unknown) => void>();
  const asked: unknown[] = [];
  const clients = windows.map(({ focused, answers }) => ({
    focused,
    visibilityState: focused ? 'visible' : 'hidden',
    postMessage: (message: unknown, [port]: MessagePort[]) => {
      // As the page reads it, outside the worker's realm.
      asked.push(JSON.parse(JSON.stringify(message)));
      if (answers !== undefined) port!.postMessage({ alerts: answers });
      port!.close();
    },
  }));
  const silent = windows.some((window) => window.focused && window.answers === undefined);
  const self = {
    addEventListener: (type: string, listener: (event: unknown) => void) =>
      listeners.set(type, listener),
    clients: { matchAll: async () => clients, claim: async () => {} },
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
  // Time passes for a window that answers nothing; one that answers does before it would.
  const setTimeout = (then: () => void) => {
    if (silent) then();
  };
  vm.runInNewContext(fs.readFileSync('app/frontend/public/service-worker.js', 'utf8'), {
    self,
    MessageChannel,
    setTimeout,
  });
  const push = { title: 'Casey Rivera', body: 'Where do I park?', url: '/', tag: 'message-1' };
  /** Delivers the push of a message tagged `message-1`, until the worker is done with it. */
  const deliver = async () => {
    let done: Promise<unknown> = Promise.resolve();
    listeners.get('push')!({
      data: { json: () => push },
      waitUntil: (work: Promise<unknown>) => (done = work),
    });
    await done;
  };
  return { shown, asked, deliver, push };
}

const shown = (silent: boolean, closed: boolean) => ({
  title: 'Casey Rivera',
  tag: 'message-1',
  silent,
  closed,
});

test('a push shows its notification while no window of Dispatch is in front, asking none', async () => {
  const { shown: showing, asked, deliver } = worker([{ focused: false, answers: true }]);
  await deliver();
  assert.deepEqual(showing, [shown(false, false)]);
  assert.deepEqual(asked, []);
});

test('a push a window in front alerts of itself shows silently and closes at once', async () => {
  const {
    shown: showing,
    asked,
    deliver,
    push,
  } = worker([{ focused: false }, { focused: true, answers: true }]);
  await deliver();
  assert.deepEqual(showing, [shown(true, true)]);
  assert.deepEqual(asked, [{ push }]);
});

test('a push stays shown while the window in front alerts of it not, or says nothing', async (t) => {
  for (const answers of [false, undefined]) {
    await t.test(`answering ${answers}`, async () => {
      const { shown: showing, deliver } = worker([{ focused: true, answers }]);
      await deliver();
      assert.deepEqual(showing, [shown(false, false)]);
    });
  }
});

test("a push a window in front alerts of itself leaves the page's own notification showing", async () => {
  const page = shown(false, false);
  const { shown: showing, deliver } = worker([{ focused: true, answers: true }], [{ ...page }]);
  await deliver();
  assert.deepEqual(showing, [page]);
});
