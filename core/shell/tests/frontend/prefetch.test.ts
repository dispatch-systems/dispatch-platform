import test from 'node:test';
import assert from 'node:assert/strict';
import { warm } from '../../frontend/runtime/route-prefetch.js';
import { cancelPrefetches } from '../../frontend/runtime/prefetch.js';
import { cacheLimits, dataCache } from '../../frontend/runtime/data-cache.js';

test('route warmups use custom readers in the existing immediate and queued paths', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const previous = new Map<string, PropertyDescriptor | undefined>(
    ['document', 'navigator', 'location'].map((key) => [
      key,
      Object.getOwnPropertyDescriptor(globalThis, key),
    ]),
  );
  Object.defineProperties(globalThis, {
    document: { configurable: true, value: { hidden: false } },
    navigator: { configurable: true, value: { onLine: true } },
    location: { configurable: true, value: { hash: '#example' } },
  });
  t.after(() => {
    cancelPrefetches();
    dataCache.clear();
    for (const [key, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else Reflect.deleteProperty(globalThis, key);
    }
  });
  t.mock.method(globalThis, 'fetch', async () =>
    assert.fail('custom warmup used the plain URL reader'),
  );
  for (const immediate of [true, false]) {
    dataCache.clear();
    const key = '/api/dsp/example';
    const calls: string[] = [];
    warm(
      [
        {
          url: key,
          read: async (signal, priority) => {
            assert.equal(signal.aborted, false);
            calls.push(priority);
            return { value: 42 };
          },
        },
      ],
      'route:example',
      immediate,
    );
    if (!immediate) t.mock.timers.tick(cacheLimits.preloadDelayMs);
    // Selection coalesces with the warmup under the logical key in both paths.
    const value = await dataCache.read(key, async () => assert.fail('warmup did not run'));
    assert.deepEqual(value, { value: 42 });
    assert.deepEqual(calls, [immediate ? 'high' : 'low']);
    assert.equal(dataCache.peek(key).data, value);
  }
});
