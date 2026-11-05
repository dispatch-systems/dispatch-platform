import assert from 'node:assert/strict';
import test, { type TestContext } from 'node:test';
import { followPush } from '../../frontend/alerts/push.js';

// What a browser's subscription comes to as the member chose: made under the server's key, kept,
// made again under a new key, or let go of, the server told of each.

/** The server's key, and another it had before. */
const KEY = 'BAAAAA';
const OLD = 'BAAAAQ';

/** `text`'s bytes, as base64url. */
const bytes = (text: string) =>
  Uint8Array.from(atob(text.replace(/-/g, '+').replace(/_/g, '/')), (char) => char.charCodeAt(0))
    .buffer;

/**
 * A browser that can be pushed to, subscribed under `subscribed` if anything, at a server whose key
 * is `key`; and what it and the server were asked, in order.
 */
function browser(t: TestContext, key: string | null, subscribed?: string) {
  const asked: string[] = [];
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
  const pushManager = {
    getSubscription: async () => current,
    subscribe: async ({ applicationServerKey }: { applicationServerKey: Uint8Array }) => {
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
    getRegistration: async () => (registered ? { pushManager } : undefined),
  };
  t.mock.method(globalThis, 'fetch', async (url: string, init: RequestInit) => {
    if (init.method === 'POST') asked.push(`POST ${url} ${String(init.body)}`);
    return Response.json(init.method === 'POST' ? { ok: true } : { key });
  });
  Object.defineProperty(navigator, 'serviceWorker', { value: serviceWorker, configurable: true });
  Object.assign(globalThis, {
    window: { PushManager: class {} },
    Notification: class {},
  });
  t.after(() => {
    delete (navigator as { serviceWorker?: unknown }).serviceWorker;
    delete (globalThis as { window?: unknown }).window;
    delete (globalThis as { Notification?: unknown }).Notification;
  });
  return asked;
}

const kept = (endpoint: string) =>
  `POST /api/dsp/communications/push {"endpoint":"${endpoint}","keys":{"p256dh":"p256dh","auth":"auth"}}`;
const removed = (endpoint: string) =>
  `POST /api/dsp/communications/push/remove {"endpoint":"${endpoint}"}`;

test("a browser is subscribed under the server's key and kept as the member's while they want it", async (t) => {
  const asked = browser(t, KEY);
  await followPush(true);
  assert.deepEqual(asked, [
    'register /service-worker.js',
    `subscribe ${KEY}`,
    kept('https://push.example/new'),
  ]);
});

test('a browser subscribed already is kept again, as the server may have let go of it', async (t) => {
  const asked = browser(t, KEY, KEY);
  await followPush(true);
  assert.deepEqual(asked, ['register /service-worker.js', kept('https://push.example/old')]);
});

test("a browser subscribed under the server's old key is subscribed again under its new one", async (t) => {
  const asked = browser(t, KEY, OLD);
  await followPush(true);
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
      await followPush(wanted);
      assert.deepEqual(asked, [
        removed('https://push.example/old'),
        'unsubscribe https://push.example/old',
      ]);
    });
  }
  // Never subscribed, there's nothing to let go of.
  const asked = browser(t, KEY);
  await followPush(false);
  assert.deepEqual(asked, []);
});
