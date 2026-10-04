import test from 'node:test';
import assert from 'node:assert/strict';
import { demo, fixture } from '../../../../core/shell/tests/support/support.js';
const { password } = demo;

test('Rust API enforces login, origin, host, CSRF, tenant views and membership permissions', async (t) => {
  const f = await fixture();
  t.after(f.close);
  assert.equal((await f.request('/api/session')).status, 401);
  assert.equal(
    (
      await f.request(
        '/api/auth/login',
        { email: 'owner@dispatch.test', password },
        { origin: 'https://evil.test' },
      )
    ).status,
    403,
  );
  assert.equal((await f.request('/api/health', undefined, { host: 'evil.test' })).status, 400);
  assert.equal(
    (
      await f.request(
        '/api/auth/login',
        { email: 'owner@dispatch.test', password, extra: true },
        { host: new URL(f.env.DISPATCH_ORIGIN).host },
      )
    ).status,
    400,
  );
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  assert.equal(member.session.dsps.length, 1);
  assert.equal((await member.get('/api/platform/dsps')).status, 403);
  assert.equal((await member.get('/api/dsp/employees')).status, 403);
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await member.select(north.id);
  assert.equal((await member.get('/api/dsp/employees')).value.total, 12);
  assert.equal((await member.get('/api/dsp/connections')).status, 403);
  assert.equal((await member.post('/api/dsp/jobs', { requestId: 'forbidden' })).status, 403);
  assert.equal(
    (await f.request('/api/session/dsp', { dspId: north.id }, { cookie: owner.headers.cookie! }))
      .status,
    403,
  );
  const dev = owner.session.dsps.find((d: { permanent: boolean }) => d.permanent);
  assert.equal((await member.post('/api/session/dsp', { dspId: dev.id })).status, 403);
  await owner.select(north.id);
  owner.headers['x-dispatch-view'] += 'tampered';
  assert.equal((await owner.get('/api/dsp/employees')).status, 409);
  const diagnostics = await owner.get('/api/platform/diagnostics');
  assert(diagnostics.value.runtime.coreMemoryBytes > 0);
  assert.equal(diagnostics.value.runtime.workerMemoryBytes, 0);
});

test('independent platforms reject each other’s sessions and signed views', async (t) => {
  const a = await fixture(false);
  t.after(a.close);
  const b = await fixture(false);
  t.after(b.close);
  const one = await a.client(),
    two = await b.client();
  await one.select(one.session.dsps[0].id);
  await two.select(two.session.dsps[0].id);
  assert.equal((await b.request('/api/session', undefined, one.headers)).status, 401);
  two.headers['x-dispatch-view'] = one.headers['x-dispatch-view']!;
  assert.equal((await two.get('/api/dsp/employees')).status, 404);
});
