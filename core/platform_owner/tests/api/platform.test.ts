import test from 'node:test';
import assert from 'node:assert/strict';
import { demo, fixture } from '../../../shell/tests/support/support.js';

// Core's own routes: what only the platform owner reads. Switching a DSP's features is tested
// with the features it switches, in app/tests/api/feature-switches.test.ts.
test('only the platform owner reads the platform: its DSPs, health and diagnostics', async (t) => {
  const f = await fixture();
  t.after(f.close);
  assert.equal((await f.request('/api/platform/health')).status, 401);
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  assert.equal((await member.get('/api/platform/dsps')).status, 403);
  const diagnostics = await owner.get('/api/platform/diagnostics');
  assert(diagnostics.value.runtime.coreMemoryBytes > 0);
  assert.equal(diagnostics.value.runtime.workerMemoryBytes, 0);
});

test('a disabled DSP signs its people out and lets none in, until enabling it turns it back on as it was', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client(demo.member);
  const north = member.session.dsps[0];
  await member.select(north.id);
  const summary = async () =>
    (await owner.read('/api/platform/dsps')).find((dsp: { id: string }) => dsp.id === north.id);
  const before = await summary();
  assert.notDeepEqual(before.connections, {});
  assert.equal((await owner.post(`/api/platform/dsps/${north.id}/disable`)).status, 200);
  // The platform owner still has the admin's pages; its people have no session, nor a new one.
  assert.equal((await summary()).status, 'suspended');
  assert.equal((await member.get('/api/session')).status, 401);
  const login = { email: demo.member, password: demo.password };
  const refused = await f.request('/api/auth/login', login, f.at(demo.memberDsp));
  assert.deepEqual([refused.status, refused.value.error], [403, 'dsp_disabled']);
  assert.equal((await owner.post(`/api/platform/dsps/${north.id}/enable`)).status, 200);
  // Enabled, its features and connections are as they were; the sessions it ended stay ended.
  const after = await summary();
  assert.deepEqual([after.features, after.connections], [before.features, before.connections]);
  assert.equal((await member.get('/api/session')).status, 401);
  const back = await f.client(demo.member);
  await back.select(north.id);
});
