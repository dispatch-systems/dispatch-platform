import test from 'node:test';
import assert from 'node:assert/strict';
import { demo, fixture } from '../../../shell/tests/support/support.js';
import { asked, fromSite } from '../support/demo-requests.js';

test('a request the site signed is kept once for the platform owners, who are emailed it, say where it stands and delete it', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const requests = () => owner.read('/api/platform/demo-requests');
  const now = Math.floor(Date.now() / 1000);

  // Only what the site signed, lately, at the admin's address, is taken.
  const refused = async (sent: unknown, how: Parameters<typeof fromSite>[2]) => {
    const answer = await fromSite(f, sent, how);
    return [answer.status, answer.value.error];
  };
  assert.deepEqual(await refused(asked('a'), { secret: 'another-secret-of-the-same-length-000' }), [
    401,
    'demo_request_unsigned',
  ]);
  assert.deepEqual(await refused(asked('a'), { at: now - 10 * 60 }), [
    401,
    'demo_request_unsigned',
  ]);
  assert.deepEqual(await refused(asked('a'), { headers: f.at(demo.memberDsp) }), [
    404,
    'not_found',
  ]);
  // Signed, a request still holds only what the form allows.
  assert.deepEqual(await refused(asked('a', { email: 'not an address' }), {}), [
    400,
    'invalid_input',
  ]);
  assert.deepEqual(await refused(asked('a', { name: 'Maria\nAlvarez' }), {}), [
    400,
    'invalid_input',
  ]);
  assert.deepEqual(await refused({ ...asked('a'), admin: true }, {}), [400, 'invalid_input']);
  assert.deepEqual(await requests(), []);

  // One the site sends again, as after an answer it never got, is kept once.
  assert.equal((await fromSite(f, asked('a'))).status, 200);
  assert.equal((await fromSite(f, asked('a'))).status, 200);
  assert.equal((await fromSite(f, asked('b', { name: 'Luis Carrasco', message: '' }))).status, 200);
  const [luis, maria] = await requests();
  assert.deepEqual(
    [maria.name, maria.email, maria.dsp, maria.message, maria.status],
    ['Maria Alvarez', 'maria@northline.example', 'Northline Logistics', asked('a').message, 'new'],
  );
  assert.deepEqual([luis.name, luis.message, (await requests()).length], ['Luis Carrasco', '', 2]);
  assert.equal(await owner.read('/api/platform/demo-requests/new'), 2);
  // Each owner is emailed a request once.
  const notices = (await owner.read('/api/platform/mail')).filter(
    (mail: { kind: string }) => mail.kind === 'demo_request',
  );
  assert.deepEqual(
    notices.map((mail: { recipient: string }) => mail.recipient),
    [demo.email, demo.email],
  );

  // Nobody else reads them.
  assert.equal((await f.request('/api/platform/demo-requests')).status, 401);
  const member = await f.client(demo.member);
  assert.equal((await member.get('/api/platform/demo-requests')).status, 403);

  // The owner says where one stands, among the statuses there are.
  const status = (id: string, to: string) =>
    owner.post(`/api/platform/demo-requests/${id}/status`, { status: to });
  assert.equal((await status(maria.id, 'contacted')).status, 200);
  assert.equal((await status(maria.id, 'answered')).status, 400);
  const missing = await status('dmr_none', 'contacted');
  assert.deepEqual([missing.status, missing.value.error], [404, 'demo_request_not_found']);
  assert.deepEqual(
    (await requests()).map((request: { status: string }) => request.status),
    ['new', 'contacted'],
  );
  assert.equal(await owner.read('/api/platform/demo-requests/new'), 1);

  // The owner deletes one: the answer is those left, and it is gone for good.
  const remove = (id: string) => owner.post(`/api/platform/demo-requests/${id}/delete`);
  const left = await remove(luis.id);
  assert.deepEqual(
    [left.status, left.value.map((request: { name: string }) => request.name)],
    [200, ['Maria Alvarez']],
  );
  assert.equal((await remove(luis.id)).status, 404);
  assert.equal(await owner.read('/api/platform/demo-requests/new'), 0);
  // The site sending a deleted one again doesn't bring it back, or email anyone again.
  assert.equal((await fromSite(f, asked('b', { name: 'Luis Carrasco', message: '' }))).status, 200);
  assert.deepEqual(
    (await requests()).map((request: { name: string }) => request.name),
    ['Maria Alvarez'],
  );
  assert.equal(
    (await owner.read('/api/platform/mail')).filter(
      (mail: { kind: string }) => mail.kind === 'demo_request',
    ).length,
    notices.length,
  );
});
