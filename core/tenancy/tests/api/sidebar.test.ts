import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../shell/tests/support/support.js';

test("how a member arranged a DSP's sidebar comes with their view of it, theirs alone and that DSP's alone", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const named = (name: string) =>
    owner.session.dsps.find((dsp: { name: string }) => dsp.name === name);
  const [north, summit] = [named('Northline Logistics'), named('Summit Delivery')];
  assert.equal((await owner.select(north.id)).sidebar, undefined);
  const arrangement = { order: ['team', 'timecard'], hidden: ['documents'] };
  const saved = await owner.post('/api/dsp/sidebar', arrangement);
  assert.deepEqual([saved.status, saved.value], [200, arrangement]);
  assert.deepEqual((await owner.select(north.id)).sidebar, arrangement);
  assert.equal((await owner.select(summit.id)).sidebar, undefined);
  const member = await f.client('member@dispatch.test');
  assert.equal((await member.select(north.id)).sidebar, undefined);
  // Only lists of page ids are kept.
  for (const refused of [{ order: 'team' }, { order: [''], hidden: [] }, { pinned: ['team'] }])
    assert.equal((await member.post('/api/dsp/sidebar', refused)).status, 400);
  assert.equal((await member.select(north.id)).sidebar, undefined);
});
