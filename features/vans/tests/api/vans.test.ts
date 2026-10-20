import test from 'node:test';
import assert from 'node:assert/strict';
import { demo, fixture, until } from '../../../../core/shell/tests/support/support.js';
import { connectFleet } from '../support/fleet.js';

test('the fleet collects, for those who may collect it', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const { owner, dsp } = await connectFleet(f);
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
  assert.deepEqual([status.running, status.inactive], [5, 1]);
  // No default role holds vans.collect, so a member is refused until a role grants it.
  const member = await f.client(demo.member);
  await member.select(dsp.id);
  assert.equal((await member.get('/api/dsp/vans/status')).status, 403);
  assert.equal((await member.post('/api/dsp/vans/collect', { requestId: 'member' })).status, 403);
});
