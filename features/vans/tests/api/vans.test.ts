import test from 'node:test';
import assert from 'node:assert/strict';
import { demo, fixture, until } from '../../../../core/shell/tests/support/support.js';

test('the fleet collects, for those who may collect it', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const north = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(north.id);
  const profile = { name: north.name, abbreviation: 'NLL', stationCode: 'TST1' };
  assert.equal(
    (await owner.post('/api/dsp/profile', { ...profile, timezone: north.timezone })).status,
    200,
  );
  await owner.select(north.id);
  const connected = await owner.post('/api/dsp/connections/cortex', {
    username: 'fixture@example.test',
    password: 'fixture-password',
  });
  assert.equal(connected.status, 200, connected.body);
  const job = await owner.post('/api/dsp/vans/collect', { requestId: 'fleet' });
  assert.equal(job.status, 202, job.body);
  await until(async () => {
    const status = await owner.read('/api/dsp/vans/status');
    const current = status.jobs.find((item: { id: string }) => item.id === job.value.id);
    assert.notEqual(current?.status, 'failed', JSON.stringify(current));
    return current?.status === 'succeeded';
  });
  const status = await owner.read('/api/dsp/vans/status');
  assert.ok(status.collectedAt);
  assert.equal(status.listed, 6);
  // No default role holds vans.collect, so a member is refused until a role grants it.
  const member = await f.client(demo.member);
  await member.select(north.id);
  assert.equal((await member.get('/api/dsp/vans/status')).status, 403);
  assert.equal((await member.post('/api/dsp/vans/collect', { requestId: 'member' })).status, 403);
});
