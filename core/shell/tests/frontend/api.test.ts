import test from 'node:test';
import assert from 'node:assert/strict';
import { ApiError, wordedApi } from '../../frontend/runtime/api.js';

// The server answers every call with the error code given, as it answers a refused write.
const refusing = (t: test.TestContext, code: string) =>
  t.mock.method(
    globalThis,
    'fetch',
    async () =>
      new Response(JSON.stringify({ error: code, message: code.replaceAll('_', ' ') }), {
        status: 409,
        headers: { 'x-request-id': 'request-1' },
      }),
  );

test("an owner's own error codes read as it words them, and every other as before", async (t) => {
  const api = wordedApi({ example_taken: 'Another example already uses this name.' });
  refusing(t, 'example_taken');
  await assert.rejects(api('/api/dsp/example', { name: 'One' }), {
    name: 'Error',
    code: 'example_taken',
    message: 'Another example already uses this name.',
    status: 409,
    requestId: 'request-1',
  });
  t.mock.restoreAll();
  // Core's own wording, for a code the owner doesn't word.
  refusing(t, 'rate_limited');
  const refused = await api('/api/dsp/example', {}).catch((error: unknown) => error);
  assert.ok(refused instanceof ApiError);
  assert.equal(refused.message, 'Too many attempts. Wait a few minutes and try again.');
});
