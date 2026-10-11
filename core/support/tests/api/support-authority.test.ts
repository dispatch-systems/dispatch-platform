import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../shell/tests/support/support.js';

// Transport checks protect routes that backend storage tests cannot exercise: public
// writes, signed DSP views, cookie precedence, and dedicated machine authentication.
test('support browser reads as its reporter but cannot write or cross DSPs', async (t) => {
  const token = 'fixture-support-worker-token-of-at-least-32-characters';
  const f = await fixture({ env: { DISPATCH_DEV_SUPPORT_WORKER_TOKEN: token } });
  t.after(f.close);
  const member = await f.client('member@dispatch.test');
  const owner = await f.client();
  const dsp = member.session.dsps[0];
  const created = await member.post('/api/support/tickets', {
    requestId: 'support-authority-test-request',
    kind: 'bug',
    title: 'Selection issue',
    description: 'The selection disappears after refresh.',
    attachmentIds: [],
  });
  assert.equal(created.status, 201);
  assert.equal(created.value.ticket.dspId, dsp.id);
  assert.equal(
    created.value.ticket.reporterName,
    member.session.user.firstName + ' ' + member.session.user.lastName,
  );
  assert.equal((await owner.post('/api/v1/support/worker/claim')).status, 401);
  const machine = { authorization: `Bearer ${token}` };
  assert.equal(
    (await f.request('/api/v1/support/worker/claim', {}, { ...owner.headers, ...machine })).status,
    401,
  );
  const claimed = await f.request('/api/v1/support/worker/claim', {}, machine);
  assert.equal(claimed.status, 200);
  const job = claimed.value;
  const browser = await f.request(
    `/api/v1/support/worker/jobs/${job.id}/browser`,
    { lease: job.lease },
    machine,
  );
  assert.equal(browser.status, 200);
  const cookie = browser.value.cookie;
  const headers = { ...f.at('nll'), cookie: `${cookie.name}=${cookie.value}` };
  const session = await f.request('/api/session', undefined, headers);
  assert.equal(session.status, 200);
  assert.equal(session.value.user.id, member.session.user.id);
  const authorized = { ...headers, 'x-csrf-token': session.value.csrf };
  const view = await f.request('/api/session/dsp', { dspId: dsp.id }, authorized);
  assert.equal(view.status, 200);
  assert.equal(
    (await f.request(`/api/support/tickets/${created.value.ticket.id}`, undefined, headers)).status,
    200,
  );
  for (const path of [
    '/api/support/tickets',
    '/api/auth/logout',
    '/api/auth/login',
    '/api/auth/security/recovery-codes',
  ]) {
    const denied = await f.request(path, {}, authorized);
    assert.equal(denied.status, 403, path);
    assert.equal(denied.value.error, 'support_read_only', path);
  }
  // GET invitation acceptance paths are writes in the route table, too.
  assert.equal(
    (await f.request('/api/invitations/nonexistent', undefined, headers)).value.error,
    'support_read_only',
  );
  assert.equal(
    (await f.request('/api/session', undefined, { ...headers, ...f.at('sll') })).status,
    401,
  );
  const wait = await f.request(
    `/api/v1/support/worker/jobs/${job.id}/wait`,
    {
      lease: job.lease,
      reason: 'Awaiting additional evidence',
      checkpoint: {},
    },
    machine,
  );
  assert.equal(wait.status, 200);
  assert.equal((await f.request('/api/session', undefined, headers)).status, 401);
});
