import assert from 'node:assert/strict';
import test, { type TestContext } from 'node:test';
import { alertsInFront, followPush } from '../../frontend/alerts/push.js';

// What a browser's subscription comes to as the member chose at a DSP: made under the server's key
// once its worker is active, kept, made again under a new key, or let go of, the server told of
// each; and kept for the other DSPs opened at the same address while any of them wants it. And
// which pushes a page in front says it alerts of itself.

/** The server's key, and another it had before. */
const KEY = 'BAAAAA';
const OLD = 'BAAAAQ';

/** `text`'s bytes, as base64url. */
const bytes = (text: string) =>
  Uint8Array.from(atob(text.replace(/-/g, '+').replace(/_/g, '/')), (char) => char.charCodeAt(0))
    .buffer;

/** The DSPs opened here. */
const NORTH = 'dsp_north';
const SUMMIT = 'dsp_summit';

/**
 * A browser that can be pushed to, subscribed under `subscribed` if anything, for the DSPs
 * `keeping` it, at a server whose key is `key`; and what it and the server were asked, in order.
 * Its worker, newly registered, is active only once it's ready.
 */
function browser(t: TestContext, key: string | null, subscribed?: string, keeping = [NORTH]) {
  const asked: string[] = [];
  const storage = new Map<string, string>();
  if (subscribed)
    storage.set(
      'dispatch-communications-push',
      JSON.stringify({ endpoint: 'https://push.example/old', dsps: keeping }),
    );
  const subscription = (endpoint: string, under: string) => ({
    endpoint,
    options: { applicationServerKey: bytes(under) },
    toJSON: () => ({ endpoint, keys: { p256dh: 'p256dh', auth: 'auth' } }),
    unsubscribe: async () => {
      asked.push(`unsubscribe ${endpoint}`);
      return true;
    },
  });
  let current = subscribed ? subscription('https://push.example/old', subscribed) : null;
  let active = Boolean(subscribed);
  const pushManager = {
    getSubscription: async () => current,
    subscribe: async ({ applicationServerKey }: { applicationServerKey: Uint8Array }) => {
      // As browsers refuse while the worker isn't active yet.
      if (!active) throw new DOMException('No active worker', 'InvalidStateError');
      const under = btoa(String.fromCharCode(...applicationServerKey))
        .replace(/\+/g, '-')
        .replace(/\//g, '_')
        .replace(/=+$/, '');
      asked.push(`subscribe ${under}`);
      current = subscription('https://push.example/new', under);
      return current;
    },
  };
  let registered = Boolean(subscribed);
  const serviceWorker = {
    register: async (url: string) => {
      asked.push(`register ${url}`);
      registered = true;
      return { pushManager };
    },
    get ready() {
      active = registered;
      return Promise.resolve({ pushManager });
    },
    getRegistration: async () => (registered ? { pushManager } : undefined),
  };
  t.mock.method(globalThis, 'fetch', async (url: string, init: RequestInit) => {
    if (init.method === 'POST') asked.push(`POST ${url} ${String(init.body)}`);
    return Response.json(init.method === 'POST' ? { ok: true } : { key });
  });
  Object.defineProperty(navigator, 'serviceWorker', { value: serviceWorker, configurable: true });
  Object.defineProperty(globalThis, 'localStorage', {
    value: {
      getItem: (at: string) => storage.get(at) ?? null,
      setItem: (at: string, value: string) => storage.set(at, value),
    },
    configurable: true,
  });
  Object.assign(globalThis, {
    window: { PushManager: class {} },
    Notification: class {},
  });
  t.after(() => {
    delete (navigator as { serviceWorker?: unknown }).serviceWorker;
    delete (globalThis as { localStorage?: unknown }).localStorage;
    delete (globalThis as { window?: unknown }).window;
    delete (globalThis as { Notification?: unknown }).Notification;
  });
  return asked;
}

const kept = (endpoint: string) =>
  `POST /api/dsp/communications/push {"endpoint":"${endpoint}","keys":{"p256dh":"p256dh","auth":"auth"}}`;
const removed = (endpoint: string) =>
  `POST /api/dsp/communications/push/remove {"endpoint":"${endpoint}"}`;

test("a browser is subscribed under the server's key once its worker is active, and kept as the member's", async (t) => {
  const asked = browser(t, KEY);
  await followPush(true, NORTH);
  assert.deepEqual(asked, [
    'register /service-worker.js',
    `subscribe ${KEY}`,
    kept('https://push.example/new'),
  ]);
});

test('a browser subscribed already is kept again, as the server may have let go of it', async (t) => {
  const asked = browser(t, KEY, KEY);
  await followPush(true, NORTH);
  assert.deepEqual(asked, ['register /service-worker.js', kept('https://push.example/old')]);
});

test("a browser subscribed under the server's old key is subscribed again under its new one", async (t) => {
  const asked = browser(t, KEY, OLD);
  await followPush(true, NORTH);
  assert.deepEqual(asked, [
    'register /service-worker.js',
    removed('https://push.example/old'),
    'unsubscribe https://push.example/old',
    `subscribe ${KEY}`,
    kept('https://push.example/new'),
  ]);
});

test('a browser is let go of once the member no longer wants it, or the server pushes nothing', async (t) => {
  for (const [wanted, key] of [
    [false, KEY],
    [true, null],
  ] as const) {
    await t.test(`wanted ${wanted}, key ${key}`, async (t) => {
      const asked = browser(t, key, KEY);
      await followPush(wanted, NORTH);
      assert.deepEqual(asked, [
        removed('https://push.example/old'),
        'unsubscribe https://push.example/old',
      ]);
    });
  }
  // Never subscribed, there's nothing to let go of.
  const asked = browser(t, KEY);
  await followPush(false, NORTH);
  assert.deepEqual(asked, []);
});

test('a browser another DSP opened here still wants is let go of by this one alone', async (t) => {
  const asked = browser(t, KEY, KEY, [NORTH, SUMMIT]);
  await followPush(false, NORTH);
  assert.deepEqual(asked, [removed('https://push.example/old')]);
  // Then let go of by the last DSP that wanted it.
  await followPush(false, SUMMIT);
  assert.deepEqual(asked, [
    removed('https://push.example/old'),
    removed('https://push.example/old'),
    'unsubscribe https://push.example/old',
  ]);
});

test('a page in front alerts of a push of its own DSP it shows open or notifies of itself', () => {
  const push = { dsp: NORTH, conversation: 'cv_1', group: 'gr_1' };
  for (const [pushed, inFront, notifies, alerts] of [
    [push, { conversation: 'cv_1' }, false, true],
    [push, { group: 'gr_1' }, false, true],
    [push, {}, true, true],
    // As on a phone, whose page notifies of nothing, out of the conversation.
    [push, { conversation: 'cv_2' }, false, false],
    // Another DSP's, opened at the same address.
    [{ ...push, dsp: SUMMIT }, { conversation: 'cv_1' }, true, false],
    [{ dsp: NORTH, conversation: 'cv_1', group: null }, { group: undefined }, false, false],
  ] as const)
    assert.equal(
      alertsInFront(pushed, NORTH, inFront, notifies),
      alerts,
      JSON.stringify([pushed, inFront, notifies]),
    );
});
