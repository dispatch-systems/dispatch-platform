import test from 'node:test';
import assert from 'node:assert/strict';
import { demo, fixture, until } from '../../../../core/shell/tests/support/support.js';

test('request IDs correlate sanitized failure logs without logging secrets or invite tokens', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const token = 'private-invitation-token';
  const result = await f.request(`/api/invitations/${token}?secret=private-query`, undefined, {
    'x-request-id': 'untrusted-request-id',
  });
  assert.equal(result.status, 404);
  const id = result.headers.get('x-request-id');
  assert.match(id!, /^req_[a-f0-9]{32}$/);
  // Another event of the request, such as a slow database, can be logged before its own.
  const logged = () =>
    f
      .logs()
      .split('\n')
      .filter((line) => line.startsWith('{'))
      .map((line) => JSON.parse(line))
      .find((e) => e.event === 'http.request' && e.fields?.requestId === id);
  await until(async () => logged() !== undefined);
  const event = logged();
  assert.equal(event.fields.error, 'invitation_expired');
  assert.equal(event.fields.route, '/api/invitations/{token}');
  for (const secret of [token, 'private-query', 'untrusted-request-id', demo.password])
    assert(!f.logs().includes(secret));
});
